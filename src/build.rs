use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::llms::{self, PageEntry};
use crate::manifest::{self, PageManifestEntry};
use crate::minify;
use crate::render;
use crate::types::{
    Align, CalloutVariant, Component, Page, SemColor, Shell, SiteConfig, Stat, TableColumn,
};

#[derive(serde::Serialize)]
#[serde(tag = "event")]
#[allow(dead_code)]
enum BuildEvent {
    #[serde(rename = "build_start")]
    BuildStart {
        dir: String,
        out: String,
        release: bool,
        timestamp: String,
    },
    #[serde(rename = "page_built")]
    PageBuilt {
        path: String,
        title: String,
        timestamp: String,
    },
    #[serde(rename = "asset_copied")]
    AssetCopied { path: String, timestamp: String },
    #[serde(rename = "warning")]
    Warning {
        message: String,
        file: Option<String>,
        timestamp: String,
    },
    #[serde(rename = "page_error")]
    PageError {
        path: String,
        message: String,
        timestamp: String,
    },
    #[serde(rename = "stale_page")]
    StalePage {
        path: String,
        title: String,
        days_overdue: Option<i64>,
        days_until_due: Option<i64>,
        owner: Option<String>,
        timestamp: String,
    },
    #[serde(rename = "build_complete")]
    BuildComplete {
        pages: usize,
        assets: usize,
        duration_ms: u64,
        stale_pages: usize,
        timestamp: String,
    },
}

fn now_iso() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

fn emit_json(event: &BuildEvent) {
    if let Ok(line) = serde_json::to_string(event) {
        println!("{}", line);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    dir: &Path,
    out: &Path,
    release: bool,
    allow_orphans: bool,
    json: bool,
    no_manifest: bool,
    no_search: bool,
    no_health: bool,
) -> Result<()> {
    let start = std::time::Instant::now();
    let config = load_config(dir)?;
    fs::create_dir_all(out)?;

    if json {
        emit_json(&BuildEvent::BuildStart {
            dir: dir.display().to_string(),
            out: out.display().to_string(),
            release,
            timestamp: now_iso(),
        });
    }

    // Canonicalize the output dir so we can reliably skip walking into it
    // when it lives inside the source dir (e.g. docs/_site under docs/).
    let out_canonical = out.canonicalize().unwrap_or_else(|_| out.to_path_buf());

    let mut pages = 0;
    let mut assets = 0;
    let mut entries: Vec<PageEntry> = Vec::new();
    let mut manifest_entries: Vec<PageManifestEntry> = Vec::new();
    let mut search_entries: Vec<crate::search::SearchEntry> = Vec::new();
    // Collect stale-review pages so we can print a summary at the end
    // of the build. Staleness is evaluated against `KAZAM_TODAY` or the
    // system clock (see `freshness::today_iso`).
    let today = crate::freshness::today_iso();
    let mut stale_pages: Vec<StaleEntry> = Vec::new();
    // Per-page href inventory so we can run the link-graph analysis after
    // the walk. Populated alongside each page render.
    let mut page_links: Vec<crate::links::PageLinks> = Vec::new();

    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            // Skip the configured output directory (e.g. _site/ nested in
            // source dir), AND any `_site` folder anywhere in the tree —
            // otherwise running kazam from a parent directory that contains
            // previously-built sub-sites would recursively ingest all those
            // `_site/` outputs as if they were source.
            if e.path()
                .canonicalize()
                .map(|p| p.starts_with(&out_canonical))
                .unwrap_or(false)
            {
                return false;
            }
            if e.depth() > 0 && e.file_type().is_dir() {
                let name = e.file_name();
                if name == "_site" || name == "prompts" {
                    return false;
                }
            }
            // Skip hidden entries (.git, .DS_Store, .vscode, etc.) at any depth
            // except the source dir itself, which is often passed as "." and
            // would be filtered by a naive starts-with check.
            if e.depth() > 0 {
                if let Some(name) = e.file_name().to_str() {
                    if name.starts_with('.') {
                        return false;
                    }
                }
            }
            true
        })
    {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }

        let fname = path.file_name().unwrap_or_default();
        if fname == "kazam.yaml" || fname == "404.yaml" {
            continue;
        }

        let rel = path.strip_prefix(dir)?;
        let is_yaml = path.extension().map(|e| e == "yaml").unwrap_or(false);

        if is_yaml {
            let content =
                fs::read_to_string(path).with_context(|| format!("reading {:?}", path))?;
            let page: Page =
                serde_yaml::from_str(&content).with_context(|| format!("parsing {:?}", path))?;

            // Semantic validation — catches structural/value errors serde can't.
            let file_str = rel.to_string_lossy().to_string();
            let val_errors = crate::validate::validate_page(&file_str, &page);
            if !val_errors.is_empty() {
                for e in &val_errors {
                    let loc = if e.path.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", e.path)
                    };
                    eprintln!("  validation error in {}{}: {}", file_str, loc, e.message);
                }
                anyhow::bail!(
                    "{} validation error(s) in {}. Run `kazam validate` for details.",
                    val_errors.len(),
                    file_str
                );
            }

            let base = base_path_for(rel);
            let source_filename = rel
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            let source_stem = rel
                .file_stem()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();

            // The source pill (bottom-right dropdown) is on by default.
            // Opt out with `view_source: false` in kazam.yaml.
            // source_view_href is the primary action link; empty = pill hidden.
            let source_view_href = if config.view_source == Some(false) {
                String::new()
            } else if !release {
                format!("{}.source.html", source_stem)
            } else if let Some(ref edit_url) = config.edit_url {
                let base = edit_url.trim_end_matches('/');
                let yaml_path = rel.to_string_lossy();
                format!("{}/{}", base, yaml_path)
            } else {
                format!("{}.source.html", source_stem)
            };

            // URL-shaped relative path for canonical / og:url meta. Always
            // forward-slash separated, `.html` extension.
            let html_rel = rel
                .with_extension("html")
                .to_string_lossy()
                .replace('\\', "/");
            let source_rel = format!("{}.source.html", source_stem);

            let yaml_rel = rel.to_string_lossy();
            let mut html = render::render_page(
                &page,
                &config,
                &base,
                &source_view_href,
                &html_rel,
                release,
                &yaml_rel,
                config.edit_url.as_deref(),
            );
            if release {
                html = minify::minify_html(&html);
            }

            let out_path = out.join(rel).with_extension("html");
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&out_path, html)?;

            if config.view_source != Some(false) {
                let mut source_view = render::render_source_view(
                    &page,
                    &config,
                    &content,
                    &base,
                    &source_filename,
                    &source_rel,
                    release,
                    &rel.to_string_lossy(),
                );
                if release {
                    source_view = minify::minify_html(&source_view);
                }
                let source_view_path =
                    out_path.with_file_name(format!("{}.source.html", source_stem));
                fs::write(&source_view_path, source_view)?;
            }

            // Always copy the raw YAML — llms.txt points at it and it's
            // useful for `curl` / programmatic access even without view_source.
            let yaml_out = out.join(rel);
            fs::copy(path, &yaml_out)?;

            // Collect metadata for llms.txt (unless marked unlisted or archived)
            let html_path_str = rel.with_extension("html").to_string_lossy().to_string();
            let yaml_path_str = rel.to_string_lossy().to_string();
            let archived = page.is_archived(&today);
            let excluded = page.unlisted || archived || page.draft;
            if !excluded {
                entries.push(PageEntry {
                    title: page.title.clone(),
                    subtitle: page.subtitle.clone(),
                    html_path: html_path_str.clone(),
                    yaml_path: yaml_path_str.clone(),
                });
            }

            // Collect metadata for site.json manifest (all pages, including unlisted)
            {
                let page_components = {
                    let top_level = page.components.as_deref().unwrap_or(&[]);
                    let mut all = manifest::collect_component_types(top_level);
                    // For deck pages, also walk slide components.
                    if let Some(slides) = &page.slides {
                        for slide in slides {
                            for name in manifest::collect_component_types(&slide.components) {
                                if !all.contains(&name) {
                                    all.push(name);
                                }
                            }
                        }
                    }
                    all
                };
                let freshness_manifest = page
                    .freshness
                    .as_ref()
                    .and_then(|fv| fv.as_full())
                    .map(|f| manifest::freshness_manifest(f, &today));
                manifest_entries.push(PageManifestEntry {
                    path: html_path_str,
                    source: yaml_path_str,
                    title: page.title.clone(),
                    subtitle: page.subtitle.clone(),
                    shell: manifest::shell_name(page.shell).to_string(),
                    components: page_components,
                    freshness: freshness_manifest,
                    unlisted: page.unlisted,
                    archived,
                    draft: page.draft,
                    personas: page.personas.clone(),
                });
            }

            // Collect internal hrefs for the link-graph pass.
            let html_rel_for_links = rel
                .with_extension("html")
                .to_string_lossy()
                .replace('\\', "/");
            page_links.push(crate::links::collect_page_links(&html_rel_for_links, &page));

            // Compute freshness status once — used by both search index and
            // the stale-page summary so they never disagree.
            let freshness_full = page.freshness.as_ref().and_then(|fv| fv.as_full());
            let freshness_status =
                crate::freshness::info_for(freshness_full, &today).map(|info| info.status());

            if !no_search && !excluded {
                let status_str = freshness_status.as_ref().map(|s| {
                    use crate::freshness::FreshnessStatus;
                    match s {
                        FreshnessStatus::Fresh => "fresh",
                        FreshnessStatus::DueSoon { .. } => "due_soon",
                        FreshnessStatus::Overdue { .. } => "overdue",
                        FreshnessStatus::Expired { .. } => "expired",
                    }
                });
                search_entries.push(crate::search::entry_for(
                    &html_rel_for_links,
                    &page,
                    status_str,
                ));
            }

            if let Some(status) = freshness_status {
                if !matches!(status, crate::freshness::FreshnessStatus::Fresh) {
                    stale_pages.push(StaleEntry {
                        html_path: rel.with_extension("html").to_string_lossy().to_string(),
                        title: page.title.clone(),
                        owner: freshness_full
                            .and_then(|f| f.owner.clone())
                            .or_else(|| page.owner.clone()),
                        status,
                        cadence: freshness_full
                            .and_then(|f| f.review_every.clone())
                            .unwrap_or_default(),
                    });
                }
            }

            if json {
                emit_json(&BuildEvent::PageBuilt {
                    path: out_path.display().to_string(),
                    title: page.title.clone(),
                    timestamp: now_iso(),
                });
            } else {
                println!("  {}", out_path.display());
            }
            pages += 1;
        } else {
            // Static asset — copy verbatim
            let out_path = out.join(rel);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, &out_path)?;
            if json {
                emit_json(&BuildEvent::AssetCopied {
                    path: out_path.display().to_string(),
                    timestamp: now_iso(),
                });
            }
            assets += 1;
        }
    }

    // Emit llms.txt
    if !entries.is_empty() {
        llms::write(out, &config, &entries)?;
    }

    // Emit site.json manifest (skippable via --no-manifest)
    if !no_manifest && !manifest_entries.is_empty() {
        manifest::write(out, &config, &manifest_entries)?;
    }

    // Emit search.json index (skippable via --no-search)
    if !no_search && !search_entries.is_empty() {
        crate::search::write(out, &search_entries)?;
    }

    // Emit sitemap.xml + robots.txt when a canonical URL is configured.
    // Without a URL they'd emit bogus/relative paths, so skip silently.
    if let Some(site_url) = config.url.as_deref() {
        write_sitemap(out, site_url, &entries)?;
        write_robots(out, site_url)?;
    }

    if !json {
        if assets > 0 {
            println!(
                "\n✓ {} page(s), {} asset(s) → {}{}",
                pages,
                assets,
                out.display(),
                if release { " (minified)" } else { "" }
            );
        } else {
            println!(
                "\n✓ {} page(s) → {}{}",
                pages,
                out.display(),
                if release { " (minified)" } else { "" }
            );
        }
    }

    // Generate 404.html. If the source dir contains 404.yaml, render that
    // as the 404 page; otherwise use the built-in "Page not found" page.
    // The 404 page uses a special base so all internal links are absolute —
    // hosting platforms serve 404.html at whatever URL the browser tried.
    let custom_404 = if dir.join("404.yaml").exists() {
        let content =
            fs::read_to_string(dir.join("404.yaml")).with_context(|| "reading 404.yaml")?;
        Some(serde_yaml::from_str::<Page>(&content).with_context(|| "parsing 404.yaml")?)
    } else {
        None
    };
    let mut html_404 = render::render_404_page(custom_404, &config, release);
    if release {
        html_404 = minify::minify_html(&html_404);
    }
    fs::write(out.join("404.html"), html_404)?;

    if json {
        emit_stale_json(&stale_pages);
    } else {
        print_freshness_report(&stale_pages);
    }
    write_freshness_report_md(out, &stale_pages, &today)?;

    // Link-graph analysis runs after every build. Orphans can be silenced
    // for draft workflows (dev mode, `--allow-orphans`) but broken links
    // always surface — there's no legitimate reason to tolerate those.
    let mut report = crate::links::analyze(&page_links, config.nav.as_deref());
    if allow_orphans {
        report.orphans.clear();
    }
    if !json {
        crate::links::print_report(&report);
    }
    crate::links::write_report_md(out, &report)?;

    if !no_health {
        generate_health_page(
            out,
            &config,
            &stale_pages,
            &manifest_entries,
            &today,
            release,
        )?;
    }

    if json {
        let duration_ms = start.elapsed().as_millis() as u64;
        emit_json(&BuildEvent::BuildComplete {
            pages,
            assets,
            duration_ms,
            stale_pages: stale_pages.len(),
            timestamp: now_iso(),
        });
    }

    // Re-scan anatomy if the workspace exists. Preserves enriched descriptions.
    if dir.join(".kazam").is_dir() {
        if let Ok(store) = crate::ctx::scan::scan(dir) {
            let flat = dir.join(".kazam/ctx/anatomy.flat.yaml");
            let _ = crate::workspace::write_yaml(&flat, &store);
            let _ = crate::ctx::scan::write_layered(dir, &store);
        }
    }

    Ok(())
}

/// One stale-review page collected during the build walk. Separate from
/// `PageEntry` because it needs cadence/owner info and we want to sort
/// it by overdue-ness, not by llms.txt order.
struct StaleEntry {
    html_path: String,
    title: String,
    owner: Option<String>,
    status: crate::freshness::FreshnessStatus,
    cadence: String,
}

/// Emit one `stale_page` JSON event per stale entry when running in JSON mode.
fn emit_stale_json(stale: &[StaleEntry]) {
    use crate::freshness::FreshnessStatus;
    for e in stale {
        let (days_overdue, days_until_due) = match e.status {
            FreshnessStatus::Expired { days_past_expiry } => (Some(days_past_expiry), None),
            FreshnessStatus::Overdue { days_overdue } => (Some(days_overdue), None),
            FreshnessStatus::DueSoon { days_until_due } => (None, Some(days_until_due)),
            FreshnessStatus::Fresh => (None, None),
        };
        emit_json(&BuildEvent::StalePage {
            path: e.html_path.clone(),
            title: e.title.clone(),
            days_overdue,
            days_until_due,
            owner: e.owner.clone(),
            timestamp: now_iso(),
        });
    }
}

/// Write the stale-page report to `<out>/stale.md` whenever any page is
/// not Fresh. Markdown so agents can read it straight, humans can too.
/// Silent (no file written) when nothing is stale — matches the console
/// behavior, keeps the output dir clean on healthy builds.
fn write_freshness_report_md(out: &Path, stale: &[StaleEntry], today: &str) -> std::io::Result<()> {
    use crate::freshness::FreshnessStatus;

    if stale.is_empty() {
        // Don't leave a stale file behind from a previous (dirtier) build.
        let p = out.join("stale.md");
        if p.exists() {
            fs::remove_file(p)?;
        }
        return Ok(());
    }

    let mut overdue: Vec<&StaleEntry> = stale
        .iter()
        .filter(|e| {
            matches!(
                e.status,
                FreshnessStatus::Overdue { .. } | FreshnessStatus::Expired { .. }
            )
        })
        .collect();
    let mut due_soon: Vec<&StaleEntry> = stale
        .iter()
        .filter(|e| matches!(e.status, FreshnessStatus::DueSoon { .. }))
        .collect();
    overdue.sort_by_key(|e| match e.status {
        FreshnessStatus::Expired { days_past_expiry } => -days_past_expiry - 10000,
        FreshnessStatus::Overdue { days_overdue } => -days_overdue,
        _ => 0,
    });
    due_soon.sort_by_key(|e| match e.status {
        FreshnessStatus::DueSoon { days_until_due } => days_until_due,
        _ => 0,
    });

    let mut md = String::new();
    md.push_str(&format!(
        "# Stale page report\n\n_Generated {} by `kazam build`. Point an agent at this file and ask it to refresh the listed pages — they're in the source tree as `.yaml`, each with its own `freshness.sources_of_truth`._\n\n",
        today
    ));

    if !overdue.is_empty() {
        md.push_str(&format!("## Overdue ({})\n\n", overdue.len()));
        for e in &overdue {
            let days = match e.status {
                FreshnessStatus::Expired { days_past_expiry } => days_past_expiry,
                FreshnessStatus::Overdue { days_overdue } => days_overdue,
                _ => 0,
            };
            let owner = e
                .owner
                .as_deref()
                .map(|o| format!(" — owner: {}", o))
                .unwrap_or_default();
            md.push_str(&format!(
                "- **`{}`** — {} day(s) overdue (cadence: every {}){}\n",
                e.html_path, days, e.cadence, owner
            ));
        }
        md.push('\n');
    }

    if !due_soon.is_empty() {
        md.push_str(&format!("## Due soon ({})\n\n", due_soon.len()));
        for e in &due_soon {
            let days = match e.status {
                FreshnessStatus::DueSoon { days_until_due } => days_until_due,
                _ => 0,
            };
            let owner = e
                .owner
                .as_deref()
                .map(|o| format!(" — owner: {}", o))
                .unwrap_or_default();
            md.push_str(&format!(
                "- **`{}`** — due in {} day(s) (cadence: every {}){}\n",
                e.html_path, days, e.cadence, owner
            ));
        }
        md.push('\n');
    }

    fs::write(out.join("stale.md"), md)
}

/// Print a grouped summary of pages past (or nearly past) their review
/// window. Always runs after a build; silent when nothing is stale.
/// Overdue items sort first, most overdue at the top.
fn print_freshness_report(stale: &[StaleEntry]) {
    use crate::freshness::FreshnessStatus;

    let mut overdue: Vec<&StaleEntry> = stale
        .iter()
        .filter(|e| {
            matches!(
                e.status,
                FreshnessStatus::Overdue { .. } | FreshnessStatus::Expired { .. }
            )
        })
        .collect();
    let mut due_soon: Vec<&StaleEntry> = stale
        .iter()
        .filter(|e| matches!(e.status, FreshnessStatus::DueSoon { .. }))
        .collect();

    if overdue.is_empty() && due_soon.is_empty() {
        return;
    }

    // Most urgent first.
    overdue.sort_by_key(|e| match e.status {
        FreshnessStatus::Expired { days_past_expiry } => -days_past_expiry - 10000,
        FreshnessStatus::Overdue { days_overdue } => -days_overdue,
        _ => 0,
    });
    due_soon.sort_by_key(|e| match e.status {
        FreshnessStatus::DueSoon { days_until_due } => days_until_due,
        _ => 0,
    });

    println!();
    if !overdue.is_empty() {
        println!("⚠ {} overdue page(s):", overdue.len());
        for e in overdue {
            let days = match e.status {
                FreshnessStatus::Expired { days_past_expiry } => days_past_expiry,
                FreshnessStatus::Overdue { days_overdue } => days_overdue,
                _ => 0,
            };
            let owner = e
                .owner
                .as_deref()
                .map(|o| format!(" — owner {}", o))
                .unwrap_or_default();
            println!(
                "    {:<40}  {} day(s) overdue (cadence: every {}){}",
                e.html_path, days, e.cadence, owner
            );
        }
    }
    if !due_soon.is_empty() {
        if !stale.is_empty()
            && stale.iter().any(|e| {
                matches!(
                    e.status,
                    FreshnessStatus::Overdue { .. } | FreshnessStatus::Expired { .. }
                )
            })
        {
            println!();
        }
        println!("⏳ {} page(s) due for review soon:", due_soon.len());
        for e in due_soon {
            let days = match e.status {
                FreshnessStatus::DueSoon { days_until_due } => days_until_due,
                _ => 0,
            };
            let owner = e
                .owner
                .as_deref()
                .map(|o| format!(" — owner {}", o))
                .unwrap_or_default();
            println!(
                "    {:<40}  due in {} day(s) (cadence: every {}){}",
                e.html_path, days, e.cadence, owner
            );
        }
    }
}

fn generate_health_page(
    out: &Path,
    config: &SiteConfig,
    stale_pages: &[StaleEntry],
    manifest_entries: &[PageManifestEntry],
    today: &str,
    release: bool,
) -> Result<()> {
    use crate::freshness::FreshnessStatus;

    let total = manifest_entries.len();
    let pages_with_freshness = manifest_entries
        .iter()
        .filter(|e| e.freshness.is_some())
        .count();
    let freshness_pct = if total > 0 {
        ((pages_with_freshness as f64 / total as f64) * 100.0).round() as u8
    } else {
        0
    };

    let overdue_entries: Vec<&StaleEntry> = stale_pages
        .iter()
        .filter(|e| {
            matches!(
                e.status,
                FreshnessStatus::Overdue { .. } | FreshnessStatus::Expired { .. }
            )
        })
        .collect();
    let due_soon_entries: Vec<&StaleEntry> = stale_pages
        .iter()
        .filter(|e| matches!(e.status, FreshnessStatus::DueSoon { .. }))
        .collect();

    let overdue_count = overdue_entries.len();
    let due_soon_count = due_soon_entries.len();
    let fresh_count = total.saturating_sub(overdue_count + due_soon_count);

    let mut components: Vec<Component> = Vec::new();

    // 1. StatGrid: Total / Fresh / Due soon / Overdue
    components.push(Component::StatGrid {
        stats: vec![
            Stat {
                label: "Total pages".into(),
                value: total.to_string(),
                detail: None,
                color: SemColor::Default,
            },
            Stat {
                label: "Fresh".into(),
                value: fresh_count.to_string(),
                detail: None,
                color: SemColor::Green,
            },
            Stat {
                label: "Due soon".into(),
                value: due_soon_count.to_string(),
                detail: None,
                color: if due_soon_count > 0 {
                    SemColor::Yellow
                } else {
                    SemColor::Default
                },
            },
            Stat {
                label: "Overdue".into(),
                value: overdue_count.to_string(),
                detail: None,
                color: if overdue_count > 0 {
                    SemColor::Red
                } else {
                    SemColor::Default
                },
            },
        ],
        columns: 4,
    });

    // 2. ProgressBar: Freshness coverage
    components.push(Component::ProgressBar {
        value: freshness_pct,
        label: Some("Freshness coverage".into()),
        color: if freshness_pct >= 80 {
            SemColor::Green
        } else if freshness_pct >= 50 {
            SemColor::Yellow
        } else {
            SemColor::Red
        },
        detail: Some(format!(
            "{} of {} pages have freshness metadata",
            pages_with_freshness, total
        )),
    });

    // 3. Overdue table (if any)
    if !overdue_entries.is_empty() {
        let mut sorted_overdue = overdue_entries;
        sorted_overdue.sort_by_key(|e| match e.status {
            FreshnessStatus::Expired { days_past_expiry } => -(days_past_expiry + 10000),
            FreshnessStatus::Overdue { days_overdue } => -days_overdue,
            _ => 0,
        });
        let overdue_rows: Vec<HashMap<String, serde_yaml::Value>> = sorted_overdue
            .iter()
            .map(|e| {
                let days = match e.status {
                    FreshnessStatus::Expired { days_past_expiry } => days_past_expiry,
                    FreshnessStatus::Overdue { days_overdue } => days_overdue,
                    _ => 0,
                };
                let mut row = HashMap::new();
                row.insert("page".into(), serde_yaml::Value::String(e.title.clone()));
                row.insert(
                    "path".into(),
                    serde_yaml::Value::String(e.html_path.clone()),
                );
                row.insert(
                    "days_overdue".into(),
                    serde_yaml::Value::Number(days.into()),
                );
                row.insert(
                    "cadence".into(),
                    serde_yaml::Value::String(e.cadence.clone()),
                );
                row.insert(
                    "owner".into(),
                    serde_yaml::Value::String(e.owner.clone().unwrap_or_else(|| "—".into())),
                );
                row
            })
            .collect();
        components.push(Component::Table {
            columns: vec![
                TableColumn {
                    key: "page".into(),
                    label: "Page".into(),
                    sortable: true,
                    align: Align::Left,
                },
                TableColumn {
                    key: "days_overdue".into(),
                    label: "Days overdue".into(),
                    sortable: true,
                    align: Align::Left,
                },
                TableColumn {
                    key: "cadence".into(),
                    label: "Cadence".into(),
                    sortable: false,
                    align: Align::Left,
                },
                TableColumn {
                    key: "owner".into(),
                    label: "Owner".into(),
                    sortable: true,
                    align: Align::Left,
                },
            ],
            rows: overdue_rows,
            filterable: true,
        });
    }

    // 4. Due soon table (if any)
    if !due_soon_entries.is_empty() {
        let mut sorted_due_soon = due_soon_entries;
        sorted_due_soon.sort_by_key(|e| match e.status {
            FreshnessStatus::DueSoon { days_until_due } => days_until_due,
            _ => 0,
        });
        let due_soon_rows: Vec<HashMap<String, serde_yaml::Value>> = sorted_due_soon
            .iter()
            .map(|e| {
                let days = match e.status {
                    FreshnessStatus::DueSoon { days_until_due } => days_until_due,
                    _ => 0,
                };
                let mut row = HashMap::new();
                row.insert("page".into(), serde_yaml::Value::String(e.title.clone()));
                row.insert(
                    "path".into(),
                    serde_yaml::Value::String(e.html_path.clone()),
                );
                row.insert("due_in".into(), serde_yaml::Value::Number(days.into()));
                row.insert(
                    "cadence".into(),
                    serde_yaml::Value::String(e.cadence.clone()),
                );
                row.insert(
                    "owner".into(),
                    serde_yaml::Value::String(e.owner.clone().unwrap_or_else(|| "—".into())),
                );
                row
            })
            .collect();
        components.push(Component::Table {
            columns: vec![
                TableColumn {
                    key: "page".into(),
                    label: "Page".into(),
                    sortable: true,
                    align: Align::Left,
                },
                TableColumn {
                    key: "due_in".into(),
                    label: "Due in".into(),
                    sortable: true,
                    align: Align::Left,
                },
                TableColumn {
                    key: "cadence".into(),
                    label: "Cadence".into(),
                    sortable: false,
                    align: Align::Left,
                },
                TableColumn {
                    key: "owner".into(),
                    label: "Owner".into(),
                    sortable: true,
                    align: Align::Left,
                },
            ],
            rows: due_soon_rows,
            filterable: true,
        });
    }

    // 5. Ownership summary
    {
        // Aggregate per-owner counts from stale_pages and manifest_entries.
        // Use a BTreeMap so output is deterministic.
        let mut owner_map: std::collections::BTreeMap<String, (usize, usize, usize, usize)> =
            std::collections::BTreeMap::new();

        // Count totals from manifest_entries
        for entry in manifest_entries {
            let owner_key = entry
                .freshness
                .as_ref()
                .and_then(|f| f.owner.clone())
                .unwrap_or_else(|| "—".into());
            let e = owner_map.entry(owner_key).or_insert((0, 0, 0, 0));
            e.0 += 1; // total
        }

        // Now overlay fresh / due_soon / overdue from stale_pages
        for stale in stale_pages {
            let owner_key = stale.owner.clone().unwrap_or_else(|| "—".into());
            match stale.status {
                FreshnessStatus::DueSoon { .. } => {
                    owner_map.entry(owner_key.clone()).or_insert((0, 0, 0, 0)).2 += 1;
                }
                FreshnessStatus::Overdue { .. } | FreshnessStatus::Expired { .. } => {
                    owner_map.entry(owner_key.clone()).or_insert((0, 0, 0, 0)).3 += 1;
                }
                FreshnessStatus::Fresh => {}
            }
        }

        // Compute fresh = total - due_soon - overdue for each owner
        let ownership_rows: Vec<HashMap<String, serde_yaml::Value>> = owner_map
            .iter()
            .map(|(owner, &(total_o, _, due_soon_o, overdue_o))| {
                let fresh_o = total_o.saturating_sub(due_soon_o + overdue_o);
                let mut row = HashMap::new();
                row.insert("owner".into(), serde_yaml::Value::String(owner.clone()));
                row.insert("total".into(), serde_yaml::Value::Number(total_o.into()));
                row.insert("fresh".into(), serde_yaml::Value::Number(fresh_o.into()));
                row.insert(
                    "due_soon".into(),
                    serde_yaml::Value::Number(due_soon_o.into()),
                );
                row.insert(
                    "overdue".into(),
                    serde_yaml::Value::Number(overdue_o.into()),
                );
                row
            })
            .collect();

        if !ownership_rows.is_empty() {
            components.push(Component::Table {
                columns: vec![
                    TableColumn {
                        key: "owner".into(),
                        label: "Owner".into(),
                        sortable: true,
                        align: Align::Left,
                    },
                    TableColumn {
                        key: "total".into(),
                        label: "Total".into(),
                        sortable: true,
                        align: Align::Left,
                    },
                    TableColumn {
                        key: "fresh".into(),
                        label: "Fresh".into(),
                        sortable: true,
                        align: Align::Left,
                    },
                    TableColumn {
                        key: "due_soon".into(),
                        label: "Due soon".into(),
                        sortable: true,
                        align: Align::Left,
                    },
                    TableColumn {
                        key: "overdue".into(),
                        label: "Overdue".into(),
                        sortable: true,
                        align: Align::Left,
                    },
                ],
                rows: ownership_rows,
                filterable: false,
            });
        }
    }

    // 6. Callout if pages lack freshness metadata
    let no_freshness_count = total.saturating_sub(pages_with_freshness);
    if no_freshness_count > 0 {
        components.push(Component::Callout {
            variant: CalloutVariant::Info,
            title: None,
            body: format!(
                "{} page{} no freshness metadata. Add `freshness:` to your page YAML to track their health.",
                no_freshness_count,
                if no_freshness_count == 1 {
                    " has".to_string()
                } else {
                    "s have".to_string()
                }
            ),
            links: None,
        });
    }

    let page = Page {
        title: "Site Health".to_string(),
        shell: Shell::Standard,
        eyebrow: Some(format!("Snapshot — {}", today)),
        subtitle: None,
        components: Some(components),
        slides: None,
        unlisted: true,
        texture: None,
        glow: None,
        print_flow: None,
        freshness: None,
        search_terms: Vec::new(),
        owner: None,
        references: Vec::new(),
        personas: Vec::new(),
        archived: false,
        draft: false,
    };

    let mut html = render::render_page(&page, config, "", "", "", false, "", None);
    if release {
        html = minify::minify_html(&html);
    }
    fs::write(out.join("_health.html"), html)?;
    Ok(())
}

fn write_sitemap(out: &Path, site_url: &str, entries: &[PageEntry]) -> Result<()> {
    let base = site_url.trim_end_matches('/');
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for e in entries {
        // html_path is forward-slash separated, `.html` extension — ready to
        // concatenate with the site base.
        xml.push_str(&format!(
            "  <url><loc>{}/{}</loc></url>\n",
            base,
            xml_escape(&e.html_path)
        ));
    }
    xml.push_str("</urlset>\n");
    fs::write(out.join("sitemap.xml"), xml)?;
    Ok(())
}

fn write_robots(out: &Path, site_url: &str) -> Result<()> {
    let base = site_url.trim_end_matches('/');
    let body = format!("User-agent: *\nAllow: /\nSitemap: {}/sitemap.xml\n", base);
    fs::write(out.join("robots.txt"), body)?;
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn base_path_for(rel: &Path) -> String {
    let depth = rel.parent().map(|p| p.components().count()).unwrap_or(0);
    "../".repeat(depth)
}

pub fn load_config(dir: &Path) -> Result<SiteConfig> {
    let config_path = dir.join("kazam.yaml");
    if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        let mut cfg: SiteConfig = serde_yaml::from_str(&content).context("parsing kazam.yaml")?;
        if let Some(ref mut nav) = cfg.nav {
            for link in nav.iter_mut() {
                link.normalize_hrefs();
            }
        }
        Ok(cfg)
    } else {
        Ok(SiteConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_path_at_root_is_empty() {
        assert_eq!(base_path_for(Path::new("index.yaml")), "");
    }

    #[test]
    fn base_path_one_level_deep() {
        assert_eq!(base_path_for(Path::new("customers/acme.yaml")), "../");
    }

    #[test]
    fn base_path_two_levels_deep() {
        assert_eq!(base_path_for(Path::new("a/b/c.yaml")), "../../");
    }
}

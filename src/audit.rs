//! `kazam audit` — merged freshness + structural quality report.
//!
//! Walks the site directory (same filters as `freshness show`), evaluates
//! every page against freshness cadence AND structural quality rules, and
//! emits a single JSON (or pretty-printed) health report.

use std::path::Path;

use crate::freshness::{info_for, json_escape, today_iso, FreshnessStatus};
use crate::types::SourceOfTruth;

/// Severity order for sorting issues (lower = more severe).
fn issue_severity(issue: &str) -> u8 {
    match issue {
        "expired" => 0,
        "overdue" => 1,
        "due_soon" => 2,
        "missing_freshness" => 3,
        "missing_owner" => 4,
        "empty_content" => 5,
        "no_sources_of_truth" => 6,
        _ => 7,
    }
}

fn sources_to_json(sources: &[SourceOfTruth]) -> String {
    if sources.is_empty() {
        return "[]".to_string();
    }
    let entries: Vec<String> = sources
        .iter()
        .map(|s| {
            format!(
                "{{\"label\":\"{}\",\"href\":\"{}\"}}",
                json_escape(s.label()),
                json_escape(s.href()),
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

struct IssueEntry {
    path: String,
    title: String,
    owner: Option<String>,
    issue: &'static str,
    detail: String,
    sources_of_truth: Vec<SourceOfTruth>,
}

/// Run `kazam audit`: walk `dir`, emit a JSON or human-readable health report.
pub fn run(dir: &Path, pretty: bool) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::fs;
    use walkdir::WalkDir;

    let today = today_iso();

    let mut issues: Vec<IssueEntry> = Vec::new();

    // Summary counters
    let mut total = 0usize;
    let mut fresh_count = 0usize;
    let mut due_soon_count = 0usize;
    let mut overdue_count = 0usize;
    let mut expired_count = 0usize;
    let mut missing_freshness_count = 0usize;
    let mut missing_owner_count = 0usize;
    let mut empty_content_count = 0usize;
    let mut no_sources_count = 0usize;

    // Pages with zero issues
    let mut clean_pages = 0usize;

    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() > 0 && e.file_name() == "_site" && e.file_type().is_dir() {
                return false;
            }
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
        if fname == "kazam.yaml" || fname == "404.yaml" || fname == "index.yaml" {
            continue;
        }

        let is_yaml = path.extension().map(|e| e == "yaml").unwrap_or(false);
        if !is_yaml {
            continue;
        }

        let rel = path.strip_prefix(dir)?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        let content = fs::read_to_string(path).with_context(|| format!("reading {:?}", path))?;
        let page: crate::types::Page =
            serde_yaml::from_str(&content).with_context(|| format!("parsing {:?}", path))?;

        total += 1;

        let is_never = page
            .freshness
            .as_ref()
            .map(|fv| fv.is_never())
            .unwrap_or(false);

        // Never-exempt pages: count as fresh, skip all issue checks.
        if is_never {
            fresh_count += 1;
            clean_pages += 1;
            continue;
        }

        let mut page_issues: Vec<IssueEntry> = Vec::new();

        // Extract sources_of_truth for this page (shared across all issues)
        let page_sources: Vec<SourceOfTruth> = page
            .freshness
            .as_ref()
            .and_then(|fv| fv.as_full())
            .and_then(|f| f.sources_of_truth.clone())
            .unwrap_or_default();

        // ── Freshness issues ──────────────────────────────────────────────
        match page.freshness.as_ref() {
            None => {
                // No freshness block at all
                missing_freshness_count += 1;
                page_issues.push(IssueEntry {
                    path: rel_str.clone(),
                    title: page.title.clone(),
                    owner: page.owner.clone(),
                    issue: "missing_freshness",
                    detail: "Page has no freshness metadata".to_string(),
                    sources_of_truth: page_sources.clone(),
                });
            }
            Some(fv) => {
                let f = fv.as_full().unwrap();
                let owner = f.owner.clone();
                let effective_owner = owner.as_deref().or(page.owner.as_deref());

                // Check freshness cadence status
                let status = match info_for(Some(f), &today) {
                    Some(info) => info.status(),
                    None => FreshnessStatus::Fresh,
                };

                match status {
                    FreshnessStatus::Fresh => {
                        fresh_count += 1;
                    }
                    FreshnessStatus::DueSoon { days_until_due } => {
                        due_soon_count += 1;
                        let review_every = f.review_every.as_deref().unwrap_or("unknown");
                        let updated = f.updated.as_deref().unwrap_or("unknown");
                        page_issues.push(IssueEntry {
                            path: rel_str.clone(),
                            title: page.title.clone(),
                            owner: effective_owner.map(str::to_string),
                            issue: "due_soon",
                            detail: format!(
                                "Review due in {} days (review_every: {}, updated: {})",
                                days_until_due, review_every, updated
                            ),
                            sources_of_truth: page_sources.clone(),
                        });
                    }
                    FreshnessStatus::Overdue { days_overdue } => {
                        overdue_count += 1;
                        let review_every = f.review_every.as_deref().unwrap_or("unknown");
                        let updated = f.updated.as_deref().unwrap_or("unknown");
                        page_issues.push(IssueEntry {
                            path: rel_str.clone(),
                            title: page.title.clone(),
                            owner: effective_owner.map(str::to_string),
                            issue: "overdue",
                            detail: format!(
                                "{} days overdue (review_every: {}, updated: {})",
                                days_overdue, review_every, updated
                            ),
                            sources_of_truth: page_sources.clone(),
                        });
                    }
                    FreshnessStatus::Expired { days_past_expiry } => {
                        expired_count += 1;
                        let expires = f.expires.as_deref().unwrap_or("unknown");
                        page_issues.push(IssueEntry {
                            path: rel_str.clone(),
                            title: page.title.clone(),
                            owner: effective_owner.map(str::to_string),
                            issue: "expired",
                            detail: format!(
                                "{} days past expiry (expires: {})",
                                days_past_expiry, expires
                            ),
                            sources_of_truth: page_sources.clone(),
                        });
                    }
                }

                // ── Structural issues on pages with a freshness block ─────

                // missing_owner: no owner or placeholder
                let owner_missing = f
                    .owner
                    .as_deref()
                    .map(|o| o.is_empty() || o == "changeme@company.com")
                    .unwrap_or(true);
                if owner_missing {
                    missing_owner_count += 1;
                    page_issues.push(IssueEntry {
                        path: rel_str.clone(),
                        title: page.title.clone(),
                        owner: None,
                        issue: "missing_owner",
                        detail: "Page has freshness metadata but no owner assigned".to_string(),
                        sources_of_truth: page_sources.clone(),
                    });
                }

                // no_sources_of_truth: freshness block with no sources_of_truth
                let no_sources = f
                    .sources_of_truth
                    .as_ref()
                    .map(|s| s.is_empty())
                    .unwrap_or(true);
                if no_sources {
                    no_sources_count += 1;
                    page_issues.push(IssueEntry {
                        path: rel_str.clone(),
                        title: page.title.clone(),
                        owner: effective_owner.map(str::to_string),
                        issue: "no_sources_of_truth",
                        detail:
                            "No sources_of_truth entries — drift detection won't cover this page"
                                .to_string(),
                        sources_of_truth: vec![],
                    });
                }
            }
        }

        // ── Structural issues on all pages ────────────────────────────────

        // empty_content: no components or empty components
        let empty = page
            .components
            .as_ref()
            .map(|c| c.is_empty())
            .unwrap_or(true);
        if empty {
            // Only flag if not already caught by missing_freshness (avoid double counting)
            empty_content_count += 1;
            page_issues.push(IssueEntry {
                path: rel_str.clone(),
                title: page.title.clone(),
                owner: page
                    .freshness
                    .as_ref()
                    .and_then(|fv| fv.as_full())
                    .and_then(|f| f.owner.clone())
                    .or_else(|| page.owner.clone()),
                issue: "empty_content",
                detail: "Page has no components — content is empty".to_string(),
                sources_of_truth: page_sources.clone(),
            });
        }

        if page_issues.is_empty() {
            clean_pages += 1;
        } else {
            issues.extend(page_issues);
        }
    }

    // Sort issues by severity
    issues.sort_by_key(|a| issue_severity(a.issue));

    let health_score = (clean_pages * 100).checked_div(total).unwrap_or(100) as u64;

    if pretty {
        println!("Site audit — {}", today);
        println!(
            "  Health score: {}% ({}/{} pages clean)",
            health_score, clean_pages, total
        );
        println!();
        println!(
            "  fresh: {}  due_soon: {}  overdue: {}  expired: {}",
            fresh_count, due_soon_count, overdue_count, expired_count
        );
        println!(
            "  missing_freshness: {}  missing_owner: {}  empty: {}  no_sources: {}",
            missing_freshness_count, missing_owner_count, empty_content_count, no_sources_count
        );

        if issues.is_empty() {
            println!("\n  All pages are clean.");
        } else {
            println!("\n  Issues ({}):", issues.len());
            for issue in &issues {
                let badge = match issue.issue {
                    "expired" => "[EXPIRED]           ",
                    "overdue" => "[OVERDUE]           ",
                    "due_soon" => "[DUE_SOON]          ",
                    "missing_freshness" => "[MISSING_FRESHNESS] ",
                    "missing_owner" => "[MISSING_OWNER]     ",
                    "empty_content" => "[EMPTY_CONTENT]     ",
                    "no_sources_of_truth" => "[NO_SOURCES]        ",
                    _ => "[UNKNOWN]           ",
                };
                let owner_display = issue
                    .owner
                    .as_deref()
                    .map(|o| format!("owner: {:<16}", o))
                    .unwrap_or_else(|| "owner: —                ".to_string());
                // Truncate path for display
                let path_display = if issue.path.len() > 34 {
                    format!("...{}", &issue.path[issue.path.len() - 31..])
                } else {
                    format!("{:<34}", issue.path)
                };
                // Short detail (first sentence or up to 40 chars)
                let short_detail = issue.detail.split(" (").next().unwrap_or(&issue.detail);
                println!(
                    "  {}  {}  {}  {}",
                    badge, path_display, owner_display, short_detail
                );
            }
        }
    } else {
        // JSON output — hand-rolled to avoid a serde dep on the audit module
        let issues_json: Vec<String> = issues
            .iter()
            .map(|e| {
                let owner_json = e
                    .owner
                    .as_deref()
                    .map(|o| format!("\"{}\"", json_escape(o)))
                    .unwrap_or_else(|| "null".to_string());
                format!(
                    "    {{\"path\":\"{}\",\"title\":\"{}\",\"owner\":{},\"issue\":\"{}\",\"detail\":\"{}\",\"sources_of_truth\":{}}}",
                    json_escape(&e.path),
                    json_escape(&e.title),
                    owner_json,
                    e.issue,
                    json_escape(&e.detail),
                    sources_to_json(&e.sources_of_truth),
                )
            })
            .collect();

        println!(
            "{{\n  \"date\":\"{}\",\n  \"health_score\":{},\n  \"summary\":{{\"total\":{},\"fresh\":{},\"due_soon\":{},\"overdue\":{},\"expired\":{},\"missing_freshness\":{},\"missing_owner\":{},\"empty_content\":{},\"no_sources_of_truth\":{}}},\n  \"issues\":[\n{}\n  ]\n}}",
            today,
            health_score,
            total,
            fresh_count,
            due_soon_count,
            overdue_count,
            expired_count,
            missing_freshness_count,
            missing_owner_count,
            empty_content_count,
            no_sources_count,
            issues_json.join(",\n"),
        );
    }

    Ok(())
}

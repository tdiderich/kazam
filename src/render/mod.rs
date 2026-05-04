mod charts;
mod components;
mod scripts;
mod shells;
mod slug;

use crate::types::{Component, Page, Shell, SiteConfig};

pub fn render_source_view(
    original: &Page,
    config: &SiteConfig,
    yaml_content: &str,
    base: &str,
    source_filename: &str,
    rel_path: &str,
    release: bool,
    yaml_rel_path: &str,
) -> String {
    slug::reset();

    let html_href = source_filename
        .strip_suffix(".yaml")
        .map(|s| format!("{}.html", s))
        .unwrap_or_else(|| source_filename.to_string());

    let mut rendered = Rendered::default();
    rendered.extend(components::render(
        &Component::Markdown {
            body: format!("[← Back to rendered page]({})", html_href),
        },
        base,
    ));
    rendered.extend(components::render(
        &Component::Code {
            language: Some("yaml".to_string()),
            code: yaml_content.to_string(),
        },
        base,
    ));

    if !release {
        rendered.html.push_str(&format!(
            r#"<div id="kazam-source-edit" data-path="{}" hidden></div>"#,
            esc(yaml_rel_path)
        ));
        rendered.scripts.push("source_edit");
    }

    let synthetic = Page {
        title: format!("{} — Source", original.title),
        shell: Shell::Standard,
        eyebrow: original.eyebrow.clone(),
        subtitle: Some(source_filename.to_string()),
        components: None,
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

    shells::standard::wrap(
        &synthetic,
        config,
        rendered,
        base,
        "",
        rel_path,
        release,
        "",
        &synthetic.title,
        None,
    )
}

pub fn render_page(
    page: &Page,
    config: &SiteConfig,
    base: &str,
    source_href: &str,
    rel_path: &str,
    release: bool,
    yaml_path: &str,
    edit_url: Option<&str>,
) -> String {
    // Clear the per-page anchor-id dedup map so slug collisions don't leak
    // between pages in a single build.
    slug::reset();

    let mut rendered = Rendered::default();

    // Inject the stale-review banner at the top of the page body when the
    // freshness metadata is expired. Zero runtime JS — this is evaluated
    // at build time against `KAZAM_TODAY` or the system clock.
    if let Some(banner) = freshness_banner(page, base) {
        rendered.html.push_str(&banner);
    }

    if page.draft {
        rendered.html.push_str(
            r#"<div class="c-callout c-callout-info c-freshness-banner"><div class="c-callout-title">Draft</div><div class="c-callout-body">This page is a draft and is not yet published. It is excluded from search and navigation.</div></div>"#,
        );
    } else if page.archived && freshness_banner(page, base).is_none() {
        rendered.html.push_str(
            r#"<div class="c-callout c-callout-warn c-freshness-banner"><div class="c-callout-title">Archived</div><div class="c-callout-body">This page has been archived and is no longer maintained. It is excluded from search and navigation.</div></div>"#,
        );
    }

    match page.shell {
        Shell::Deck => {
            if let Some(slides) = &page.slides {
                shells::deck::render(page, config, slides, base, &mut rendered);
            }
        }
        _ => {
            if let Some(comps) = &page.components {
                for c in comps {
                    rendered.extend(components::render(c, base));
                }
            }
        }
    }

    if !page.references.is_empty() {
        rendered
            .html
            .push_str(&references_section(&page.references));
    }

    match page.shell {
        Shell::Standard => shells::standard::wrap(
            page,
            config,
            rendered,
            base,
            source_href,
            rel_path,
            release,
            yaml_path,
            &page.title,
            edit_url,
        ),
        Shell::Document => shells::document::wrap(
            page,
            config,
            rendered,
            base,
            source_href,
            rel_path,
            release,
            yaml_path,
            &page.title,
            edit_url,
        ),
        Shell::Deck => shells::deck::wrap(
            page,
            config,
            rendered,
            base,
            source_href,
            rel_path,
            release,
            yaml_path,
            &page.title,
            edit_url,
        ),
    }
}

fn references_section(refs: &[crate::types::Reference]) -> String {
    let mut html = String::from(r#"<section class="c-references"><h3>References</h3><ul>"#);
    for r in refs {
        html.push_str("<li><a href=\"");
        html.push_str(&r.url);
        html.push_str("\" target=\"_blank\" rel=\"noopener\">");
        html.push_str(r.note.as_deref().unwrap_or(&r.url));
        html.push_str("</a></li>");
    }
    html.push_str("</ul></section>");
    html
}

/// Build the freshness banner HTML for a page, or return `None` when the
/// page is fresh (or has no freshness metadata). The banner reuses the
/// existing callout variants so color treatment stays consistent with the
/// rest of the theme: yellow (`c-callout-warn`) for "due soon" — within
/// 7 days of the review deadline — and red (`c-callout-danger`) for
/// overdue pages. A `c-freshness-banner` class is added for future
/// per-element styling.
fn freshness_banner(page: &Page, base: &str) -> Option<String> {
    use crate::freshness::FreshnessStatus;

    // "never" and absent freshness both skip the banner
    let freshness = crate::freshness::freshness_struct(page.freshness.as_ref())?;
    let today = crate::freshness::today_iso();
    let info = crate::freshness::info_for(Some(freshness), &today)?;

    let (variant_class, title, headline) = match info.status() {
        FreshnessStatus::Fresh => return None,
        FreshnessStatus::DueSoon { days_until_due } => (
            "c-callout-warn",
            "Review due soon",
            if days_until_due == 0 {
                "Review is due today.".to_string()
            } else {
                format!(
                    "Review is due in <strong>{} {}</strong>.",
                    days_until_due,
                    if days_until_due == 1 { "day" } else { "days" }
                )
            },
        ),
        FreshnessStatus::Overdue { days_overdue } => (
            "c-callout-danger",
            "Review overdue",
            format!(
                "Review is <strong>{} {} overdue</strong>.",
                days_overdue,
                if days_overdue == 1 { "day" } else { "days" }
            ),
        ),
        FreshnessStatus::Expired { days_past_expiry } => (
            "c-callout-danger c-freshness-banner--overdue",
            "Page expired",
            format!(
                "This page expired <strong>{} {}</strong> ago and may no longer be relevant.",
                days_past_expiry,
                if days_past_expiry == 1 { "day" } else { "days" }
            ),
        ),
    };

    let updated_iso = freshness.updated.as_deref().unwrap_or("");
    let elapsed = info.days_since_update().unwrap_or(0);
    let cadence = freshness
        .review_every
        .as_deref()
        .unwrap_or("(no cadence set)");

    let mut body = format!(
        r#"{headline} Last updated <strong>{updated}</strong> ({elapsed} {day_word} ago). Review cadence: <strong>every {cadence}</strong>. Site last built: {today}."#,
        headline = headline,
        updated = esc(&human_date(updated_iso)),
        elapsed = elapsed,
        day_word = if elapsed == 1 { "day" } else { "days" },
        cadence = esc(cadence),
        today = esc(&human_date(&today)),
    );
    if let Some(owner) = freshness.owner.as_deref() {
        body.push_str(&format!(r#" Owner: <strong>{}</strong>."#, esc(owner)));
    }

    let mut h = format!(
        r#"<div class="c-callout {variant_class} c-freshness-banner"><div class="c-callout-title">{title}</div>"#,
        variant_class = variant_class,
        title = esc(title),
    );
    h.push_str(&format!(r#"<div class="c-callout-body">{body}</div>"#));

    if let Some(sources) = freshness.sources_of_truth.as_ref() {
        if !sources.is_empty() {
            h.push_str(
                r#"<div class="c-freshness-sources"><span class="c-freshness-sources-label">Sources of truth:</span><ul>"#,
            );
            for src in sources {
                let href = resolve_href(src.href(), base);
                h.push_str(&format!(
                    r#"<li><a href="{}">{}</a></li>"#,
                    esc(&href),
                    esc(src.label()),
                ));
            }
            h.push_str("</ul></div>");
        }
    }
    h.push_str("</div>");
    Some(h)
}

/// Format an ISO `YYYY-MM-DD` date into a short human-readable form like
/// `Jan 15, 2026`. Falls back to the raw input when parsing fails.
fn human_date(iso: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = iso.split('-');
    let y = parts.next().and_then(|p| p.parse::<i32>().ok());
    let m = parts.next().and_then(|p| p.parse::<u32>().ok());
    let d = parts.next().and_then(|p| p.parse::<u32>().ok());
    match (y, m, d) {
        (Some(y), Some(m), Some(d)) if (1..=12).contains(&m) && (1..=31).contains(&d) => {
            format!("{} {}, {}", MONTHS[(m - 1) as usize], d, y)
        }
        _ => iso.to_string(),
    }
}

/// Render the site-wide 404 page. Uses a special base so all internal links
/// are absolute (the 404 page may be served at any URL by the hosting platform).
/// If `custom_page` is provided (from `404.yaml`), that page is rendered instead
/// of the default "Page not found" empty state.
pub fn render_404_page(custom_page: Option<Page>, config: &SiteConfig, release: bool) -> String {
    let base = config
        .url
        .as_deref()
        .map(|u| format!("{}/", u.trim_end_matches('/')))
        .unwrap_or_else(|| "/".to_string());

    let page = custom_page.unwrap_or_else(default_404_page);

    render_page(&page, config, &base, "", "404.html", release, "", None)
}

fn default_404_page() -> Page {
    use crate::types::{EmptyStateAction, Shell};
    Page {
        title: "Page not found".to_string(),
        shell: Shell::Standard,
        eyebrow: None,
        subtitle: None,
        components: Some(vec![Component::EmptyState {
            title: "Page not found".to_string(),
            body: Some("This page hasn't been created yet, or the link may be broken.".to_string()),
            action: Some(EmptyStateAction {
                label: "Go home".to_string(),
                href: "/index.html".to_string(),
            }),
            icon: Some("file".to_string()),
        }]),
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
    }
}

/// Rewrite an `href:` value for emission inside a page at `base` depth.
///
/// - Bare names (`content.html`, `assets/og.svg`) are **page-relative** —
///   left untouched so the browser resolves them against the current page,
///   matching standard HTML / Markdown semantics.
/// - Leading-`/` paths (`/index.html`, `/assets/og.svg`) are **site-root** —
///   the depth-base prefix (`../`) is prepended so they keep working under
///   subpath deployments (e.g. GitHub Pages `/kazam/`).
/// - `../`, `./`, `http(s)://`, `#`, `mailto:`, `tel:` pass through verbatim.
pub(super) fn resolve_href(href: &str, base: &str) -> String {
    if href.is_empty()
        || href.starts_with("http://")
        || href.starts_with("https://")
        || href.starts_with('#')
        || href.starts_with("mailto:")
        || href.starts_with("tel:")
        || href.starts_with("../")
        || href.starts_with("./")
    {
        return href.to_string();
    }
    if let Some(rest) = href.strip_prefix('/') {
        return format!("{}{}", base, rest);
    }
    href.to_string()
}

// ── Rendered: HTML + required JS fragment names ──

#[derive(Default)]
pub(super) struct Rendered {
    pub html: String,
    pub scripts: Vec<&'static str>,
}

impl Rendered {
    pub fn new(html: String) -> Self {
        Self {
            html,
            scripts: Vec::new(),
        }
    }
    pub fn with_script(mut self, name: &'static str) -> Self {
        self.scripts.push(name);
        self
    }
    pub fn extend(&mut self, other: Rendered) {
        self.html.push_str(&other.html);
        self.scripts.extend(other.scripts);
    }
}

pub(super) fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(super) fn collect_scripts(names: &[&'static str]) -> String {
    let mut seen = std::collections::HashSet::new();
    let mut out = String::new();
    for name in names {
        if seen.insert(*name) {
            if let Some(src) = scripts::get(name) {
                out.push_str("<script>");
                out.push_str(src);
                out.push_str("</script>\n");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esc_escapes_html_special_chars() {
        assert_eq!(esc("hello"), "hello");
        assert_eq!(esc("<script>"), "&lt;script&gt;");
        assert_eq!(esc("\"q\""), "&quot;q&quot;");
        assert_eq!(esc("a & b"), "a &amp; b");
    }

    #[test]
    fn resolve_href_passes_through_absolute_urls() {
        assert_eq!(
            resolve_href("https://example.com", "../"),
            "https://example.com"
        );
        assert_eq!(
            resolve_href("http://example.com", "../"),
            "http://example.com"
        );
    }

    #[test]
    fn resolve_href_passes_through_anchor_and_protocol_hrefs() {
        assert_eq!(resolve_href("#anchor", "../"), "#anchor");
        assert_eq!(
            resolve_href("mailto:hi@example.com", "../"),
            "mailto:hi@example.com"
        );
        assert_eq!(resolve_href("tel:+15551234", "../"), "tel:+15551234");
    }

    #[test]
    fn resolve_href_passes_through_dot_relative_hrefs() {
        assert_eq!(
            resolve_href("../customers/demo.html", "../"),
            "../customers/demo.html"
        );
        assert_eq!(resolve_href("./sibling.html", "../"), "./sibling.html");
    }

    #[test]
    fn resolve_href_passes_through_bare_names_as_page_relative() {
        // Bare names are page-relative — the browser resolves them against
        // the current page, so the depth base must NOT be prepended.
        assert_eq!(resolve_href("index.html", ""), "index.html");
        assert_eq!(resolve_href("index.html", "../"), "index.html");
        assert_eq!(resolve_href("sub/page.html", "../../"), "sub/page.html");
    }

    #[test]
    fn resolve_href_applies_depth_base_to_site_root_paths() {
        // Leading `/` = site-root; the depth base is prepended so the link
        // keeps working under subpath deployments.
        assert_eq!(resolve_href("/index.html", ""), "index.html");
        assert_eq!(resolve_href("/index.html", "../"), "../index.html");
        assert_eq!(
            resolve_href("/components/index.html", "../"),
            "../components/index.html"
        );
        assert_eq!(
            resolve_href("/assets/og.svg", "../../"),
            "../../assets/og.svg"
        );
    }
}

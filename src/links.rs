//! Build-time link graph analysis.
//!
//! Two checks run in one pass:
//!
//! 1. **Orphan detection** - source pages that aren't reachable from
//!    `index.html` via the `nav:` in `kazam.yaml` or any in-page `href:`.
//!    Marking a page `unlisted: true` excludes it from the orphan check
//!    (same mechanism `llms.txt` uses to skip drafts).
//! 2. **Broken-link detection** - internal `.html` hrefs that don't match
//!    any built page. Conservative: only `.html` targets are flagged. Asset
//!    hrefs (`.svg`, `.png`, `.pdf`, …), externals (`http://…`), anchors
//!    (`#foo`), `mailto:` and `tel:` are skipped.
//!
//! Both reports are silent on a clean build. When anything surfaces, the
//! build prints a grouped summary and writes a markdown companion at
//! `_site/links.md` so an agent can consume the list directly.

use crate::types::{Component, NavLink, Page};
use std::collections::{BTreeSet, HashMap, VecDeque};

/// A page's emitted HTML path and the internal `.html` hrefs it points at.
/// `href_target` paths are already resolved against the page's directory and
/// normalized (no `./`, no `..`). The `raw_href` is preserved verbatim so
/// broken-link output can echo what the author actually wrote.
pub struct PageLinks {
    pub html_path: String,
    pub unlisted: bool,
    pub hrefs: Vec<HrefRef>,
}

pub struct HrefRef {
    pub raw: String,
    pub resolved: String,
}

pub struct LinkReport {
    /// Built HTML pages that aren't reachable from `index.html` or the nav,
    /// and aren't explicitly `unlisted`.
    pub orphans: Vec<String>,
    /// `(page, raw_href)` pairs where the resolved `.html` target isn't a
    /// built page.
    pub broken: Vec<BrokenLink>,
}

pub struct BrokenLink {
    pub from: String,
    pub href: String,
}

pub fn analyze(pages: &[PageLinks], nav: Option<&[NavLink]>) -> LinkReport {
    let built: BTreeSet<String> = pages.iter().map(|p| p.html_path.clone()).collect();
    let unlisted: BTreeSet<String> = pages
        .iter()
        .filter(|p| p.unlisted)
        .map(|p| p.html_path.clone())
        .collect();

    // Build adjacency. A page's out-edges are only those resolved hrefs
    // that land on a built page; anything else is a broken link.
    let mut graph: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut broken: Vec<BrokenLink> = Vec::new();
    for p in pages {
        let mut out = BTreeSet::new();
        for h in &p.hrefs {
            if built.contains(&h.resolved) {
                out.insert(h.resolved.clone());
            } else {
                broken.push(BrokenLink {
                    from: p.html_path.clone(),
                    href: h.raw.clone(),
                });
            }
        }
        graph.insert(p.html_path.clone(), out);
    }

    // Roots: index.html and every nav target that actually maps to a built page.
    // A nav entry pointing at a missing page is a broken link; don't seed it
    // as a root or orphans become meaningless.
    let mut roots: BTreeSet<String> = BTreeSet::new();
    if built.contains("index.html") {
        roots.insert("index.html".to_string());
    }
    if let Some(nav) = nav {
        let mut nav_targets = BTreeSet::new();
        collect_nav_targets(nav, &mut nav_targets);
        for t in &nav_targets {
            if built.contains(t) {
                roots.insert(t.clone());
            }
        }
    }

    // BFS reachability.
    let mut reachable: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = roots.iter().cloned().collect();
    while let Some(p) = queue.pop_front() {
        if !reachable.insert(p.clone()) {
            continue;
        }
        if let Some(outs) = graph.get(&p) {
            for o in outs {
                if !reachable.contains(o) {
                    queue.push_back(o.clone());
                }
            }
        }
    }

    let mut orphans: Vec<String> = built
        .iter()
        .filter(|p| !reachable.contains(*p) && !unlisted.contains(*p))
        .cloned()
        .collect();
    orphans.sort();

    LinkReport { orphans, broken }
}

// ── Href collection ──────────────────────────────────────────────────

pub fn collect_page_links(html_path: &str, page: &Page) -> PageLinks {
    let page_dir = parent_dir(html_path);
    let mut raw: Vec<String> = Vec::new();
    if let Some(components) = &page.components {
        for c in components {
            collect_component_hrefs(c, &mut raw);
        }
    }
    if let Some(slides) = &page.slides {
        for s in slides {
            for c in &s.components {
                collect_component_hrefs(c, &mut raw);
            }
        }
    }

    let hrefs: Vec<HrefRef> = raw
        .into_iter()
        .filter(|h| !is_external_or_anchor(h))
        .filter_map(|h| {
            let resolved = resolve_href(&page_dir, &h);
            if resolved.ends_with(".html") {
                Some(HrefRef { raw: h, resolved })
            } else {
                None
            }
        })
        .collect();

    PageLinks {
        html_path: html_path.to_string(),
        unlisted: page.unlisted,
        hrefs,
    }
}

fn collect_component_hrefs(c: &Component, out: &mut Vec<String>) {
    use Component::*;
    match c {
        CardGrid { cards, .. } => {
            for card in cards {
                if let Some(h) = &card.href {
                    out.push(h.clone());
                }
                if let Some(links) = &card.links {
                    for l in links {
                        out.push(l.href.clone());
                    }
                }
            }
        }
        Callout {
            links: Some(links), ..
        } => {
            for b in links {
                out.push(b.href.clone());
            }
        }
        ButtonGroup { buttons } => {
            for b in buttons {
                out.push(b.href.clone());
            }
        }
        Breadcrumb { items } => {
            for item in items {
                if let Some(h) = &item.href {
                    out.push(h.clone());
                }
            }
        }
        EmptyState {
            action: Some(a), ..
        } => {
            out.push(a.href.clone());
        }
        Tabs { tabs } => {
            for t in tabs {
                for c in &t.components {
                    collect_component_hrefs(c, out);
                }
            }
        }
        Section { components, .. } => {
            for c in components {
                collect_component_hrefs(c, out);
            }
        }
        Columns { columns, .. } => {
            for col in columns {
                for c in col {
                    collect_component_hrefs(c, out);
                }
            }
        }
        Accordion { items } => {
            for item in items {
                for c in &item.components {
                    collect_component_hrefs(c, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_nav_targets(nav: &[NavLink], out: &mut BTreeSet<String>) {
    for n in nav {
        if let Some(href) = &n.href {
            if !is_external_or_anchor(href) {
                let clean = strip_fragment_and_query(href);
                if clean.ends_with(".html") {
                    out.insert(normalize_path(clean.trim_start_matches('/')));
                }
            }
        }
        if let Some(children) = &n.children {
            collect_nav_targets(children, out);
        }
    }
}

// ── Href resolution ──────────────────────────────────────────────────

fn is_external_or_anchor(href: &str) -> bool {
    href.is_empty()
        || href.starts_with("http://")
        || href.starts_with("https://")
        || href.starts_with("mailto:")
        || href.starts_with("tel:")
        || href.starts_with('#')
        || href.starts_with("//")
}

fn strip_fragment_and_query(href: &str) -> String {
    let no_frag = href.split('#').next().unwrap_or("");
    no_frag.split('?').next().unwrap_or("").to_string()
}

fn parent_dir(html_path: &str) -> String {
    match html_path.rsplit_once('/') {
        Some((parent, _)) => parent.to_string(),
        None => String::new(),
    }
}

fn resolve_href(page_dir: &str, href: &str) -> String {
    let clean = strip_fragment_and_query(href);
    let joined = if clean.starts_with('/') {
        clean.trim_start_matches('/').to_string()
    } else if page_dir.is_empty() {
        clean
    } else {
        format!("{}/{}", page_dir, clean)
    };
    normalize_path(&joined)
}

fn normalize_path(p: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

// ── Reporting ────────────────────────────────────────────────────────

pub fn print_report(report: &LinkReport) {
    if report.orphans.is_empty() && report.broken.is_empty() {
        return;
    }

    println!();
    if !report.orphans.is_empty() {
        println!(
            "⚠ {} orphan page(s) (not linked from nav or any page):",
            report.orphans.len()
        );
        for p in &report.orphans {
            println!("    {}", p);
        }
    }
    if !report.broken.is_empty() {
        if !report.orphans.is_empty() {
            println!();
        }
        println!("⚠ {} broken internal link(s):", report.broken.len());
        for b in &report.broken {
            println!("    {:<40}  → {}", b.from, b.href);
        }
    }
}

pub fn write_report_md(out: &std::path::Path, report: &LinkReport) -> std::io::Result<()> {
    let target = out.join("links.md");
    if report.orphans.is_empty() && report.broken.is_empty() {
        if target.exists() {
            std::fs::remove_file(&target)?;
        }
        return Ok(());
    }

    let mut md = String::new();
    md.push_str("# Link report\n\n");
    md.push_str(
        "_Generated by `kazam build`. Point an agent at this file and ask it to \
         fix the listed issues - either link the orphan page from somewhere that's \
         reachable, delete it, or set `unlisted: true` on its frontmatter if it's \
         genuinely not meant to be navigable._\n\n",
    );

    if !report.orphans.is_empty() {
        md.push_str(&format!("## Orphan pages ({})\n\n", report.orphans.len()));
        md.push_str("These source pages aren't reachable from `index.html` or the site nav:\n\n");
        for p in &report.orphans {
            md.push_str(&format!("- **`{}`**\n", p));
        }
        md.push('\n');
    }

    if !report.broken.is_empty() {
        md.push_str(&format!(
            "## Broken internal links ({})\n\n",
            report.broken.len()
        ));
        md.push_str(
            "These `href:` values don't resolve to any built page. Fix the href \
             or add the missing page:\n\n",
        );
        for b in &report.broken {
            md.push_str(&format!("- `{}` → `{}`\n", b.from, b.href));
        }
        md.push('\n');
    }

    std::fs::write(target, md)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_dot_segments() {
        assert_eq!(normalize_path("foo/./bar"), "foo/bar");
        assert_eq!(normalize_path("foo/../bar"), "bar");
        assert_eq!(normalize_path("a/b/../../c"), "c");
        assert_eq!(normalize_path(""), "");
    }

    #[test]
    fn resolve_against_page_dir() {
        assert_eq!(resolve_href("", "index.html"), "index.html");
        assert_eq!(resolve_href("components", "../index.html"), "index.html");
        assert_eq!(resolve_href("examples", "deck.html"), "examples/deck.html");
        assert_eq!(resolve_href("examples", "/guide.html"), "guide.html");
    }

    #[test]
    fn externals_and_anchors_are_skipped() {
        assert!(is_external_or_anchor("https://example.com"));
        assert!(is_external_or_anchor("#top"));
        assert!(is_external_or_anchor("mailto:a@b.co"));
        assert!(!is_external_or_anchor("guide.html"));
        assert!(!is_external_or_anchor("/guide.html"));
    }

    #[test]
    fn fragment_and_query_stripped_before_resolve() {
        assert_eq!(resolve_href("", "guide.html#anchor"), "guide.html");
        assert_eq!(resolve_href("", "guide.html?v=2"), "guide.html");
    }
}

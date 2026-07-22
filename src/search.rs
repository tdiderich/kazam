//! Emits `_site/search.json` — a compact search index over every built page.
//! Consumed by the MCP server's `search` tool and any client-side JS widget.

use std::path::Path;

use serde::Serialize;

use crate::types::{Component, Page};

#[derive(Serialize)]
pub struct SearchIndex {
    pub pages: Vec<SearchEntry>,
}

#[derive(Serialize)]
pub struct SearchEntry {
    /// Relative HTML path, e.g. "components/content.html"
    pub path: String,
    pub title: String,
    /// Page subtitle, used as a short description.
    pub description: Option<String>,
    pub headings: Vec<String>,
    pub content_snippets: Vec<String>,
    /// Author-provided aliases / acronyms that don't appear in rendered text.
    pub search_terms: Vec<String>,
    /// Role-based persona tags. Empty means visible to everyone.
    pub personas: Vec<String>,
    /// Freshness status string: "fresh", "due_soon", "overdue", "expired", or null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freshness_status: Option<String>,
}

/// Build a `SearchEntry` from a parsed `Page` and its output-relative HTML path.
/// `freshness_status` is an optional string like "fresh", "due_soon", "overdue", "expired".
pub fn entry_for(path: &str, page: &Page, freshness_status: Option<&str>) -> SearchEntry {
    let (mut headings, mut snippets) = (Vec::new(), Vec::new());

    if let Some(comps) = &page.components {
        extract_searchable_text(comps, &mut headings, &mut snippets);
    }

    // Deck pages — walk every slide's components.
    if let Some(slides) = &page.slides {
        for slide in slides {
            extract_searchable_text(&slide.components, &mut headings, &mut snippets);
        }
    }

    SearchEntry {
        path: path.to_string(),
        title: page.title.clone(),
        description: page.subtitle.clone(),
        headings,
        content_snippets: snippets,
        search_terms: page.search_terms.clone(),
        personas: page.personas.clone(),
        freshness_status: freshness_status.map(|s| s.to_string()),
    }
}

/// Recursively walk `components` and collect headings and content snippets.
fn extract_searchable_text(
    components: &[Component],
    headings: &mut Vec<String>,
    snippets: &mut Vec<String>,
) {
    for c in components {
        match c {
            Component::Header {
                title, subtitle, ..
            } => {
                headings.push(title.clone());
                if let Some(sub) = subtitle {
                    push_snippet(snippets, sub);
                }
            }
            Component::Markdown { body } => {
                push_snippet(snippets, &strip_markdown(body));
            }
            Component::Callout { title, body, .. } => {
                if let Some(t) = title {
                    headings.push(t.clone());
                }
                push_snippet(snippets, body);
            }
            Component::Steps { items, .. } => {
                for step in items {
                    headings.push(step.title.clone());
                    if let Some(detail) = &step.detail {
                        push_snippet(snippets, detail);
                    }
                }
            }
            Component::CardGrid { cards, .. } => {
                for card in cards {
                    headings.push(card.title.clone());
                    if let Some(desc) = &card.description {
                        push_snippet(snippets, desc);
                    }
                }
            }
            Component::Table { columns, .. } => {
                for col in columns {
                    headings.push(col.label.clone());
                }
            }
            Component::Aside { body } => {
                push_snippet(snippets, body);
            }
            Component::RuleList { items } => {
                for item in items {
                    headings.push(item.label.clone());
                    push_snippet(snippets, &item.body);
                }
            }
            Component::Gauge { items, title, .. } => {
                if let Some(t) = title {
                    headings.push(t.clone());
                }
                for item in items {
                    headings.push(item.label.clone());
                }
            }
            Component::Accordion { items } => {
                for item in items {
                    headings.push(item.title.clone());
                    extract_searchable_text(&item.components, headings, snippets);
                }
            }
            Component::Section {
                heading,
                components,
                ..
            } => {
                if let Some(h) = heading {
                    headings.push(h.clone());
                }
                extract_searchable_text(components, headings, snippets);
            }
            Component::Tabs { tabs } => {
                for tab in tabs {
                    headings.push(tab.label.clone());
                    extract_searchable_text(&tab.components, headings, snippets);
                }
            }
            Component::Columns { columns, .. } => {
                for col in columns {
                    extract_searchable_text(col, headings, snippets);
                }
            }
            Component::SelectableGrid { cards, .. } => {
                for card in cards {
                    headings.push(card.title.clone());
                    if let Some(body) = &card.body {
                        push_snippet(snippets, body);
                    }
                }
            }
            Component::DefinitionList { items } => {
                for item in items {
                    headings.push(item.term.clone());
                    push_snippet(snippets, &item.definition);
                }
            }
            Component::Blockquote { body, attribution } => {
                push_snippet(snippets, body);
                if let Some(attr) = attribution {
                    push_snippet(snippets, attr);
                }
            }
            Component::EmptyState { title, body, .. } => {
                headings.push(title.clone());
                if let Some(b) = body {
                    push_snippet(snippets, b);
                }
            }
            Component::Chart { title, .. } => {
                if let Some(t) = title {
                    headings.push(t.clone());
                }
            }
            Component::Venn { title, sets, .. } => {
                if let Some(t) = title {
                    headings.push(t.clone());
                }
                for set in sets {
                    headings.push(set.label.clone());
                }
            }
            Component::StatGrid { stats, .. } => {
                for stat in stats {
                    headings.push(stat.label.clone());
                    push_snippet(snippets, &stat.value);
                }
            }
            Component::Timeline { items } => {
                for item in items {
                    headings.push(item.name.clone());
                }
            }
            Component::BeforeAfter { items, .. } => {
                for item in items {
                    headings.push(item.title.clone());
                }
            }
            Component::EventTimeline { events, .. } => {
                for ev in events {
                    headings.push(ev.title.clone());
                    if let Some(summary) = &ev.summary {
                        push_snippet(snippets, summary);
                    }
                }
            }
            Component::Resources { items } => {
                for item in items {
                    headings.push(item.title.clone());
                    if let Some(desc) = &item.description {
                        push_snippet(snippets, desc);
                    }
                }
            }
            // Code blocks skipped — not useful as search content.
            Component::Code { .. } => {}
            // Purely decorative / non-textual — skip.
            Component::Meta { .. }
            | Component::Divider { .. }
            | Component::Kbd { .. }
            | Component::Image { .. }
            | Component::Embed { .. }
            | Component::Badge { .. }
            | Component::Tag { .. }
            | Component::Status { .. }
            | Component::Breadcrumb { .. }
            | Component::ButtonGroup { .. }
            | Component::Avatar { .. }
            | Component::AvatarGroup { .. }
            | Component::ProgressBar { .. }
            | Component::Icon { .. }
            | Component::Tree { .. }
            | Component::HeroBanner { .. }
            | Component::RoleMap { .. }
            | Component::SplitCompare { .. }
            | Component::Sankey { .. }
            | Component::Radar { .. }
            | Component::Quadrant { .. }
            | Component::Architecture { .. }
            | Component::Pipeline { .. }
            | Component::Graph { .. }
            | Component::OrgChart { .. } => {}
        }
    }
}

/// Append a text snippet, truncated to ~200 chars, skipping empty strings.
fn push_snippet(snippets: &mut Vec<String>, text: &str) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    const MAX: usize = 200;
    if trimmed.len() <= MAX {
        snippets.push(trimmed.to_string());
    } else {
        // Find the last space within MAX bytes to avoid splitting multi-byte chars.
        let cutoff = trimmed
            .char_indices()
            .take_while(|(i, _)| *i < MAX)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(MAX);
        let s = trimmed[..cutoff].trim_end().to_string();
        if !s.is_empty() {
            snippets.push(s);
        }
    }
}

/// Very rough markdown stripping: remove common syntax characters.
/// We don't need a full parser — just make the text readable for search.
fn strip_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Skip heading markers at line start handled by just removing '#'
            '#' => out.push(' '),
            // Bold/italic markers
            '*' | '_' => out.push(' '),
            // Inline code
            '`' => out.push(' '),
            // Link syntax: [label](url) → keep label
            '[' => {
                // collect until ']'
                let mut label = String::new();
                for inner in chars.by_ref() {
                    if inner == ']' {
                        break;
                    }
                    label.push(inner);
                }
                // skip (url) if present
                if chars.peek() == Some(&'(') {
                    chars.next(); // consume '('
                    for inner in chars.by_ref() {
                        if inner == ')' {
                            break;
                        }
                    }
                }
                out.push_str(&label);
            }
            // Strip image syntax '!' before '['
            '!' if chars.peek() == Some(&'[') => {}
            other => out.push(other),
        }
    }
    // Collapse runs of whitespace
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Write `search.json` to `<out>/search.json` as compact JSON.
pub fn write(out: &Path, entries: &[SearchEntry]) -> std::io::Result<()> {
    let index = SearchIndex {
        pages: entries
            .iter()
            .map(|e| SearchEntry {
                path: e.path.clone(),
                title: e.title.clone(),
                description: e.description.clone(),
                headings: e.headings.clone(),
                content_snippets: e.content_snippets.clone(),
                search_terms: e.search_terms.clone(),
                personas: e.personas.clone(),
                freshness_status: e.freshness_status.clone(),
            })
            .collect(),
    };
    let json = serde_json::to_string(&index).expect("search index serialization is infallible");
    std::fs::write(out.join("search.json"), json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Shell, Step};

    fn make_page(components: Vec<Component>) -> Page {
        Page {
            title: "Test Page".to_string(),
            shell: Shell::Standard,
            eyebrow: None,
            subtitle: None,
            components: Some(components),
            slides: None,
            unlisted: false,
            texture: None,
            glow: None,
            print_flow: None,
            hub: None,
            freshness: None,
            search_terms: Vec::new(),
            owner: None,
            references: Vec::new(),
            personas: Vec::new(),
            archived: false,
            draft: false,
            nav_layout: None,
            nav: None,
            pack: None,
        }
    }

    #[test]
    fn extract_header() {
        let page = make_page(vec![Component::Header {
            title: "Hello World".to_string(),
            subtitle: Some("A subtitle".to_string()),
            eyebrow: None,
            align: Default::default(),
            id: None,
        }]);
        let entry = entry_for("index.html", &page, None);
        assert!(entry.headings.contains(&"Hello World".to_string()));
        assert!(entry
            .content_snippets
            .iter()
            .any(|s| s.contains("subtitle")));
    }

    #[test]
    fn extract_markdown_strips_syntax() {
        let page = make_page(vec![Component::Markdown {
            body: "# Heading\n**bold** text with [link](http://example.com)".to_string(),
        }]);
        let entry = entry_for("index.html", &page, None);
        assert!(!entry.content_snippets.is_empty());
        let snippet = &entry.content_snippets[0];
        assert!(!snippet.contains('#'));
        assert!(!snippet.contains("**"));
        assert!(snippet.contains("bold"));
        assert!(snippet.contains("link"));
        assert!(!snippet.contains("http://example.com"));
    }

    #[test]
    fn extract_steps() {
        let page = make_page(vec![Component::Steps {
            items: vec![
                Step {
                    title: "Step one".to_string(),
                    detail: Some("Do this first".to_string()),
                },
                Step {
                    title: "Step two".to_string(),
                    detail: None,
                },
            ],
            numbered: true,
        }]);
        let entry = entry_for("index.html", &page, None);
        assert!(entry.headings.contains(&"Step one".to_string()));
        assert!(entry.headings.contains(&"Step two".to_string()));
        assert!(entry
            .content_snippets
            .iter()
            .any(|s| s.contains("Do this first")));
    }

    #[test]
    fn nested_section_extraction() {
        let page = make_page(vec![Component::Section {
            heading: Some("Section Heading".to_string()),
            eyebrow: None,
            align: Default::default(),
            id: None,
            components: vec![Component::Markdown {
                body: "Inner content here".to_string(),
            }],
        }]);
        let entry = entry_for("index.html", &page, None);
        assert!(entry.headings.contains(&"Section Heading".to_string()));
        assert!(entry
            .content_snippets
            .iter()
            .any(|s| s.contains("Inner content here")));
    }

    #[test]
    fn search_terms_pass_through() {
        let mut page = make_page(vec![]);
        page.search_terms = vec!["alias1".to_string(), "acronym".to_string()];
        let entry = entry_for("test.html", &page, None);
        assert_eq!(entry.search_terms, vec!["alias1", "acronym"]);
    }

    #[test]
    fn content_snippets_truncated() {
        let long_text = "a".repeat(300);
        let page = make_page(vec![Component::Markdown { body: long_text }]);
        let entry = entry_for("index.html", &page, None);
        for snippet in &entry.content_snippets {
            assert!(snippet.len() <= 200, "snippet too long: {}", snippet.len());
        }
    }

    #[test]
    fn code_blocks_skipped() {
        let page = make_page(vec![Component::Code {
            language: Some("rust".to_string()),
            code: "fn main() { println!(\"secret\"); }".to_string(),
        }]);
        let entry = entry_for("index.html", &page, None);
        assert!(entry.content_snippets.is_empty());
        assert!(entry.headings.is_empty());
    }

    #[test]
    fn tabs_recurse_into_components() {
        use crate::types::Tab;
        let page = make_page(vec![Component::Tabs {
            tabs: vec![Tab {
                label: "Tab A".to_string(),
                components: vec![Component::Markdown {
                    body: "Tab A content".to_string(),
                }],
            }],
        }]);
        let entry = entry_for("index.html", &page, None);
        assert!(entry.headings.contains(&"Tab A".to_string()));
        assert!(entry
            .content_snippets
            .iter()
            .any(|s| s.contains("Tab A content")));
    }

    #[test]
    fn columns_recurse() {
        let page = make_page(vec![Component::Columns {
            columns: vec![
                vec![Component::Markdown {
                    body: "Left column".to_string(),
                }],
                vec![Component::Markdown {
                    body: "Right column".to_string(),
                }],
            ],
            equal_heights: false,
        }]);
        let entry = entry_for("index.html", &page, None);
        assert!(entry
            .content_snippets
            .iter()
            .any(|s| s.contains("Left column")));
        assert!(entry
            .content_snippets
            .iter()
            .any(|s| s.contains("Right column")));
    }

    #[test]
    fn freshness_status_included() {
        let page = make_page(vec![]);
        let entry = entry_for("index.html", &page, Some("overdue"));
        assert_eq!(entry.freshness_status.as_deref(), Some("overdue"));
    }

    #[test]
    fn freshness_status_none_omitted() {
        let page = make_page(vec![]);
        let entry = entry_for("index.html", &page, None);
        assert!(entry.freshness_status.is_none());
    }
}

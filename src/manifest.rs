//! Emits `_site/site.json` — a structured JSON index of every page in the
//! build. Lets any HTTP client treat the static site as a headless API and
//! lets drift dashboards consume per-page freshness status without parsing HTML.

use serde::Serialize;
use std::path::Path;

use crate::types::{Component, Freshness, Shell, SiteConfig};

// ── Public structs ────────────────────────────────────

#[derive(Serialize)]
pub struct PageManifestEntry {
    pub path: String,   // relative HTML path, e.g. "customers/acme.html"
    pub source: String, // relative YAML path, e.g. "customers/acme.yaml"
    pub title: String,
    pub subtitle: Option<String>,
    pub shell: String,           // "standard", "document", or "deck"
    pub components: Vec<String>, // component type names used on this page
    pub freshness: Option<FreshnessManifest>,
    pub unlisted: bool,
    pub archived: bool,
    pub draft: bool,
    pub personas: Vec<String>,
}

#[derive(Serialize)]
pub struct FreshnessManifest {
    pub updated: Option<String>,
    pub review_every: Option<String>,
    pub owner: Option<String>,
    pub days_since_update: Option<i64>,
    pub status: String, // "fresh", "due_soon", or "overdue"
}

// ── Helpers ───────────────────────────────────────────

pub fn shell_name(shell: Shell) -> &'static str {
    match shell {
        Shell::Standard => "standard",
        Shell::Document => "document",
        Shell::Deck => "deck",
        Shell::Hub => "hub",
    }
}

/// Walk a slice of `Component`s and collect the unique variant type names.
/// Recurses into containers: `Section`, `Tabs`, `Columns`, `Accordion`.
pub fn collect_component_types(components: &[Component]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    collect_into(components, &mut seen);
    seen
}

fn collect_into(components: &[Component], out: &mut Vec<String>) {
    for c in components {
        let name = component_type_name(c);
        if !out.contains(&name) {
            out.push(name);
        }
        // Recurse into containers that hold child components.
        match c {
            Component::Section {
                components: inner, ..
            } => collect_into(inner, out),
            Component::Tabs { tabs } => {
                for tab in tabs {
                    collect_into(&tab.components, out);
                }
            }
            Component::Columns { columns, .. } => {
                for col in columns {
                    collect_into(col, out);
                }
            }
            Component::Accordion { items } => {
                for item in items {
                    collect_into(&item.components, out);
                }
            }
            _ => {}
        }
    }
}

fn component_type_name(c: &Component) -> String {
    match c {
        Component::Header { .. } => "header",
        Component::Meta { .. } => "meta",
        Component::CardGrid { .. } => "card_grid",
        Component::SelectableGrid { .. } => "selectable_grid",
        Component::Timeline { .. } => "timeline",
        Component::StatGrid { .. } => "stat_grid",
        Component::BeforeAfter { .. } => "before_after",
        Component::Steps { .. } => "steps",
        Component::Markdown { .. } => "markdown",
        Component::Table { .. } => "table",
        Component::Callout { .. } => "callout",
        Component::Code { .. } => "code",
        Component::Tabs { .. } => "tabs",
        Component::Section { .. } => "section",
        Component::Columns { .. } => "columns",
        Component::Accordion { .. } => "accordion",
        Component::EventTimeline { .. } => "event_timeline",
        Component::Tree { .. } => "tree",
        Component::PriorityQueue { .. } => "priority_queue",
        Component::Venn { .. } => "venn",
        Component::Image { .. } => "image",
        Component::Badge { .. } => "badge",
        Component::Tag { .. } => "tag",
        Component::Divider { .. } => "divider",
        Component::Kbd { .. } => "kbd",
        Component::Status { .. } => "status",
        Component::Breadcrumb { .. } => "breadcrumb",
        Component::ButtonGroup { .. } => "button_group",
        Component::DefinitionList { .. } => "definition_list",
        Component::Blockquote { .. } => "blockquote",
        Component::Avatar { .. } => "avatar",
        Component::AvatarGroup { .. } => "avatar_group",
        Component::ProgressBar { .. } => "progress_bar",
        Component::EmptyState { .. } => "empty_state",
        Component::Icon { .. } => "icon",
        Component::Chart { .. } => "chart",
        Component::Embed { .. } => "embed",
        Component::Resources { .. } => "resources",
        Component::HeroBanner { .. } => "hero_banner",
        Component::RoleMap { .. } => "role_map",
        Component::SplitCompare { .. } => "split_compare",
        Component::Sankey { .. } => "sankey",
        Component::Radar { .. } => "radar",
        Component::Quadrant { .. } => "quadrant",
        Component::Architecture { .. } => "architecture",
        Component::Pipeline { .. } => "pipeline",
        Component::Graph { .. } => "graph",
        Component::OrgChart { .. } => "org_chart",
        Component::Aside { .. } => "aside",
        Component::RuleList { .. } => "rule_list",
        Component::Gauge { .. } => "gauge",
    }
    .to_string()
}

pub fn freshness_manifest(f: &Freshness, today: &str) -> FreshnessManifest {
    use crate::freshness::{info_for, FreshnessStatus};

    let info = info_for(Some(f), today);
    let days_since_update = info.as_ref().and_then(|i| i.days_since_update());
    let status = match info.as_ref().map(|i| i.status()) {
        Some(FreshnessStatus::Expired { .. }) => "expired",
        Some(FreshnessStatus::Overdue { .. }) => "overdue",
        Some(FreshnessStatus::DueSoon { .. }) => "due_soon",
        _ => "fresh",
    }
    .to_string();

    FreshnessManifest {
        updated: f.updated.clone(),
        review_every: f.review_every.clone(),
        owner: f.owner.clone(),
        days_since_update,
        status,
    }
}

// ── Write ─────────────────────────────────────────────

pub fn write(
    out: &Path,
    config: &SiteConfig,
    entries: &[PageManifestEntry],
) -> std::io::Result<()> {
    let generated_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Build a thin wrapper struct so we can serialize name/theme/generated_at
    // alongside the borrowed entries slice without cloning all entries.
    #[derive(Serialize)]
    struct RoleEntry<'a> {
        id: &'a str,
        label: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<&'a str>,
    }

    #[derive(Serialize)]
    struct Manifest<'a> {
        name: &'a str,
        theme: Option<&'a str>,
        generated_at: String,
        roles: Vec<RoleEntry<'a>>,
        pages: &'a [PageManifestEntry],
    }

    let roles: Vec<RoleEntry<'_>> = config
        .roles
        .iter()
        .map(|r| RoleEntry {
            id: &r.id,
            label: &r.label,
            description: r.description.as_deref(),
        })
        .collect();

    let manifest = Manifest {
        name: &config.name,
        theme: config.theme.as_deref(),
        generated_at,
        roles,
        pages: entries,
    };

    let json =
        serde_json::to_string_pretty(&manifest).expect("manifest serialization is infallible");
    std::fs::write(out.join("site.json"), json)
}

// ── Tests ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AccordionItem;

    fn make_header() -> Component {
        Component::Header {
            title: "Test".to_string(),
            subtitle: None,
            eyebrow: None,
            align: Default::default(),
            id: None,
        }
    }

    fn make_markdown() -> Component {
        Component::Markdown {
            body: "Hello".to_string(),
        }
    }

    fn make_section(inner: Vec<Component>) -> Component {
        Component::Section {
            heading: None,
            eyebrow: None,
            components: inner,
            align: Default::default(),
            id: None,
        }
    }

    fn make_tab(label: &str, comps: Vec<Component>) -> crate::types::Tab {
        crate::types::Tab {
            label: label.to_string(),
            components: comps,
        }
    }

    #[test]
    fn collect_component_types_flat() {
        let comps = vec![make_header(), make_markdown()];
        let names = collect_component_types(&comps);
        assert_eq!(names, vec!["header", "markdown"]);
    }

    #[test]
    fn collect_component_types_deduplicates() {
        let comps = vec![make_header(), make_header(), make_markdown()];
        let names = collect_component_types(&comps);
        assert_eq!(names, vec!["header", "markdown"]);
    }

    #[test]
    fn collect_component_types_recurses_into_section() {
        let inner = vec![make_markdown()];
        let comps = vec![make_header(), make_section(inner)];
        let names = collect_component_types(&comps);
        assert!(names.contains(&"header".to_string()));
        assert!(names.contains(&"section".to_string()));
        assert!(names.contains(&"markdown".to_string()));
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn collect_component_types_recurses_into_tabs() {
        let tab1 = make_tab("A", vec![make_header()]);
        let tab2 = make_tab("B", vec![make_markdown()]);
        let tabs = Component::Tabs {
            tabs: vec![tab1, tab2],
        };
        let names = collect_component_types(&[tabs]);
        assert!(names.contains(&"tabs".to_string()));
        assert!(names.contains(&"header".to_string()));
        assert!(names.contains(&"markdown".to_string()));
    }

    #[test]
    fn collect_component_types_recurses_into_columns() {
        let cols = Component::Columns {
            columns: vec![vec![make_header()], vec![make_markdown()]],
            equal_heights: false,
        };
        let names = collect_component_types(&[cols]);
        assert!(names.contains(&"columns".to_string()));
        assert!(names.contains(&"header".to_string()));
        assert!(names.contains(&"markdown".to_string()));
    }

    #[test]
    fn collect_component_types_recurses_into_accordion() {
        let acc = Component::Accordion {
            items: vec![AccordionItem {
                title: "Q".to_string(),
                components: vec![make_markdown()],
            }],
        };
        let names = collect_component_types(&[acc]);
        assert!(names.contains(&"accordion".to_string()));
        assert!(names.contains(&"markdown".to_string()));
    }

    #[test]
    fn freshness_manifest_overdue() {
        use crate::types::Freshness;
        let f = Freshness {
            updated: Some("2026-01-01".to_string()),
            review_every: Some("30d".to_string()),
            owner: Some("alice".to_string()),
            sources_of_truth: None,
            expires: None,
            refresh: None,
        };
        let fm = freshness_manifest(&f, "2026-03-01");
        assert_eq!(fm.status, "overdue");
        assert!(fm.days_since_update.unwrap() > 0);
        assert_eq!(fm.owner.as_deref(), Some("alice"));
    }

    #[test]
    fn freshness_manifest_fresh() {
        use crate::types::Freshness;
        let f = Freshness {
            updated: Some("2026-04-25".to_string()),
            review_every: Some("90d".to_string()),
            owner: None,
            sources_of_truth: None,
            expires: None,
            refresh: None,
        };
        let fm = freshness_manifest(&f, "2026-05-02");
        assert_eq!(fm.status, "fresh");
    }

    #[test]
    fn freshness_manifest_serializes_correctly() {
        use crate::types::Freshness;
        let f = Freshness {
            updated: Some("2026-04-01".to_string()),
            review_every: Some("7d".to_string()),
            owner: None,
            sources_of_truth: None,
            expires: None,
            refresh: None,
        };
        // Check json contains expected fields
        let fm = freshness_manifest(&f, "2026-04-20");
        let json = serde_json::to_string(&fm).unwrap();
        assert!(json.contains("\"status\""));
        assert!(json.contains("\"days_since_update\""));
        assert!(json.contains("overdue"));
    }
}

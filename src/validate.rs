//! Strict semantic/structural validation for kazam page YAML.
//!
//! serde deserialization catches type errors and missing required fields.
//! This layer catches everything serde can't:
//!   - Component-specific rules (card_grid needs at least one card, etc.)
//!   - Enum value validation (valid theme names, shell types, color names)
//!   - Structural rules (deck pages need slides, non-deck pages need components)
//!   - Cross-references (nav hrefs should point to existing pages)
//!   - Freshness field format validation
//!
//! All validation errors are returned as structured [`ValidationError`] values.
//! The caller decides whether to surface them as JSON or pretty-printed text.

use std::collections::HashSet;
use std::path::Path;

use crate::types::{Component, Page, Shell, SiteConfig, Slide};

// ── Error type ────────────────────────────────────────

#[derive(serde::Serialize, Debug)]
pub struct ValidationError {
    /// Source file that contains the error.
    pub file: String,
    /// YAML keypath, e.g. `components[2].cards[0].title`.
    pub path: String,
    /// Short category: `"missing_field"`, `"invalid_value"`, `"structural"`,
    /// `"cross_reference"`, `"format"`.
    pub error_type: String,
    /// Human-readable explanation.
    pub message: String,
    /// Optional hint to help an agent self-correct.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl ValidationError {
    fn new(
        file: impl Into<String>,
        path: impl Into<String>,
        error_type: impl Into<String>,
        message: impl Into<String>,
        suggestion: Option<String>,
    ) -> Self {
        ValidationError {
            file: file.into(),
            path: path.into(),
            error_type: error_type.into(),
            message: message.into(),
            suggestion,
        }
    }
}

// ── Public entry points ───────────────────────────────

/// Validate a single parsed [`Page`] against semantic rules.
/// `file` is included in every error for traceability.
pub fn validate_page(file: &str, page: &Page) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    validate_shell_structure(file, page, &mut errors);
    if let Some(components) = &page.components {
        validate_components(file, "components", components, &mut errors);
    }
    if let Some(slides) = &page.slides {
        for (i, slide) in slides.iter().enumerate() {
            let path = format!("slides[{}].components", i);
            validate_components(file, &path, &slide.components, &mut errors);
        }
        if matches!(page.shell, Shell::Deck) {
            validate_slide_density(file, slides, &mut errors);
        }
    }
    if let Some(fv) = &page.freshness {
        if let Some(freshness) = fv.as_full() {
            validate_freshness(file, "freshness", freshness, &mut errors);
        }
    }
    validate_pack(file, page, &mut errors);
    validate_skill(file, page, &mut errors);
    errors
}

/// Skill pages (`skill:` present) carry agent procedures. Every ```agl fence
/// in their markdown runs through the AGL static analyzer - parse, then
/// reachability / terminal completeness / branch integrity / invariant
/// soundness - so a broken graph never saves. Fences must be self-contained:
/// imports resolve against a local specs hub that servers don't have.
/// Analyzer warnings don't block saves; agents can run `kazam agl validate`
/// for the full report.
fn validate_skill(file: &str, page: &Page, errors: &mut Vec<ValidationError>) {
    if page.skill.is_none() {
        return;
    }

    fn collect_markdown<'a>(components: &'a [Component], out: &mut Vec<&'a str>) {
        for c in components {
            match c {
                Component::Markdown { body, .. } => out.push(body),
                Component::Section { components, .. } => collect_markdown(components, out),
                _ => {}
            }
        }
    }
    let mut bodies: Vec<&str> = Vec::new();
    if let Some(components) = &page.components {
        collect_markdown(components, &mut bodies);
    }

    if bodies.iter().all(|b| b.trim().is_empty()) {
        errors.push(ValidationError::new(
            file,
            "skill",
            "skill",
            "skill pages need at least one non-empty markdown component - that's the procedure agents follow",
            Some("Add a markdown component with the skill's steps (or an ```agl fence), or remove the skill: block.".into()),
        ));
        return;
    }

    let mut fence_idx = 0;
    for body in bodies {
        for fence in extract_agl_fences(body) {
            fence_idx += 1;
            let path = format!("skill.flow[{}]", fence_idx);
            let parsed = match crate::agl::parser::parse(&fence) {
                Ok(parsed) => parsed,
                Err(e) => {
                    errors.push(ValidationError::new(
                        file,
                        path,
                        "skill",
                        format!("agl parse error (fence line {}): {}", e.line, e.message),
                        Some("Fix the AGL syntax - run `kazam agl validate` on the fence for the full report.".into()),
                    ));
                    continue;
                }
            };
            if !parsed.imports.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    path,
                    "skill",
                    "agl fences in skill pages must be self-contained - imports resolve against a local specs hub the server doesn't have",
                    Some("Inline the imported invariants into the fence.".into()),
                ));
                continue;
            }
            for d in crate::agl::validator::validate(&parsed.spec, &parsed.state_lines) {
                if d.severity == crate::agl::validator::Severity::Error {
                    errors.push(ValidationError::new(
                        file,
                        path.clone(),
                        "skill",
                        format!("agl {}: {} ({})", d.code, d.message, d.location),
                        Some("Fix the flow graph - run `kazam agl validate` for the full report including warnings.".into()),
                    ));
                }
            }
        }
    }
}

/// Pulls the contents of every ```agl fenced block out of a markdown body.
fn extract_agl_fences(body: &str) -> Vec<String> {
    let mut fences = Vec::new();
    let mut current: Option<String> = None;
    for line in body.lines() {
        let trimmed = line.trim();
        match &mut current {
            None => {
                if trimmed == "```agl" {
                    current = Some(String::new());
                }
            }
            Some(buf) => {
                if trimmed == "```" {
                    fences.push(std::mem::take(buf));
                    current = None;
                } else {
                    buf.push_str(line);
                    buf.push('\n');
                }
            }
        }
    }
    fences
}

/// Pack pages (`pack:` present) must actually be installable: at least one
/// non-empty markdown component reachable the way `kazam install` collects
/// them (top level or nested in sections), and only known target names.
fn validate_pack(file: &str, page: &Page, errors: &mut Vec<ValidationError>) {
    let Some(pack) = &page.pack else { return };

    const VALID_TARGETS: [&str; 7] = [
        "claude", "cursor", "agents", "windsurf", "copilot", "gemini", "aider",
    ];
    for (i, target) in pack.targets.iter().enumerate() {
        if !VALID_TARGETS.contains(&target.as_str()) {
            errors.push(ValidationError::new(
                file,
                format!("pack.targets[{}]", i),
                "invalid_value",
                format!("unknown pack target \"{}\"", target),
                Some(format!(
                    "Valid targets: {}. Omit targets: to write all.",
                    VALID_TARGETS.join(", ")
                )),
            ));
        }
    }

    fn has_installable_markdown(components: &[Component]) -> bool {
        components.iter().any(|c| match c {
            Component::Markdown { body, .. } => !body.trim().is_empty(),
            Component::Section { components, .. } => has_installable_markdown(components),
            _ => false,
        })
    }

    let ok = page
        .components
        .as_deref()
        .is_some_and(has_installable_markdown);
    if !ok {
        errors.push(ValidationError::new(
            file,
            "pack",
            "structural",
            "pack pages need at least one non-empty markdown component - that's what `kazam install` compiles into tool config files",
            Some("Add a markdown component (top level or inside a section) with the pack's rules, or remove the pack: block.".into()),
        ));
    }

    use crate::types::{MatchMode, PackHook};
    for (i, hook) in pack.hooks.iter().enumerate() {
        let path = format!("pack.hooks[{}]", i);
        match hook {
            PackHook::BlockOnMatch { patterns, mode, .. } => {
                if patterns.is_empty() {
                    errors.push(ValidationError::new(
                        file,
                        &path,
                        "structural",
                        "block_on_match needs at least one pattern",
                        None,
                    ));
                }
                if *mode == MatchMode::Regex {
                    errors.push(ValidationError::new(
                        file,
                        &path,
                        "invalid_value",
                        "regex match mode is not supported yet; use substring or word patterns",
                        None,
                    ));
                }
            }
            PackHook::Allowlist { allow, .. } if allow.is_empty() => {
                errors.push(ValidationError::new(
                    file,
                    &path,
                    "structural",
                    "allowlist needs at least one allowed value",
                    None,
                ));
            }
            _ => {}
        }
    }
}

/// Validate a single YAML file. Parses it as a [`Page`] and runs semantic
/// checks. No cross-reference or site config validation (those need the full
/// directory). Useful for pre-write validation in curata's API layer.
pub fn validate_single_file(path: &Path) -> Vec<ValidationError> {
    let file_str = path.display().to_string();

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return vec![ValidationError::new(
                &file_str,
                "",
                "format",
                format!("Could not read file: {}", e),
                None,
            )];
        }
    };

    let page: Page = match serde_yaml::from_str(&content) {
        Ok(p) => p,
        Err(e) => {
            return vec![ValidationError::new(
                &file_str,
                "",
                "format",
                format!("YAML parse error: {}", e),
                Some("Check YAML syntax and required fields (title:, shell:).".into()),
            )];
        }
    };

    validate_page(&file_str, &page)
}

/// Validate a site directory. Parses every `.yaml` file (skipping `kazam.yaml`
/// and `404.yaml`), validates each page, and checks nav cross-references.
pub fn validate_dir(dir: &Path) -> Vec<ValidationError> {
    use std::fs;
    use walkdir::WalkDir;

    let mut errors = Vec::new();

    // Load site config for cross-reference checks.
    let config_path = dir.join("kazam.yaml");
    let config: SiteConfig = if config_path.exists() {
        match fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_yaml::from_str(&s).ok())
        {
            Some(c) => c,
            None => {
                errors.push(ValidationError::new(
                    config_path.display().to_string(),
                    "",
                    "format",
                    "kazam.yaml could not be parsed",
                    Some("Check YAML syntax and required fields (name:).".into()),
                ));
                SiteConfig::default()
            }
        }
    } else {
        SiteConfig::default()
    };

    // Collect all page hrefs (stem-relative, html extension) so we can check
    // nav cross-references. We convert `foo/bar.yaml` → `foo/bar.html`.
    let mut known_pages: HashSet<String> = HashSet::new();

    // First pass: collect pages, parse, validate individually.
    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() > 0 {
                if let Some(name) = e.file_name().to_str() {
                    if name.starts_with('.') || name == "_site" || name == "prompts" {
                        return false;
                    }
                }
            }
            true
        })
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().map(|e| e != "yaml").unwrap_or(true) {
            continue;
        }
        let fname = path.file_name().unwrap_or_default();
        if fname == "kazam.yaml" || fname == "404.yaml" {
            continue;
        }

        let rel = match path.strip_prefix(dir) {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Record as a known page (html path).
        let html_path = rel
            .with_extension("html")
            .to_string_lossy()
            .replace('\\', "/");
        known_pages.insert(html_path);

        let file_str = rel.to_string_lossy().to_string();

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(ValidationError::new(
                    &file_str,
                    "",
                    "format",
                    format!("Could not read file: {}", e),
                    None,
                ));
                continue;
            }
        };

        let page: Page = match serde_yaml::from_str(&content) {
            Ok(p) => p,
            Err(e) => {
                errors.push(ValidationError::new(
                    &file_str,
                    "",
                    "format",
                    format!("YAML parse error: {}", e),
                    Some("Check YAML syntax and required fields (title:, shell:).".into()),
                ));
                continue;
            }
        };

        errors.extend(validate_page(&file_str, &page));
    }

    // Second pass: nav cross-references.
    if let Some(nav) = &config.nav {
        validate_nav_links("kazam.yaml", "nav", nav, &known_pages, &mut errors);
    }

    // Validate site config theme value.
    validate_site_config("kazam.yaml", &config, &mut errors);

    errors
}

// ── Site config validation ────────────────────────────

fn validate_site_config(file: &str, config: &SiteConfig, errors: &mut Vec<ValidationError>) {
    // Valid theme names.
    const VALID_THEMES: &[&str] = &[
        "dark", "light", "red", "orange", "yellow", "green", "teal", "blue", "violet",
    ];
    if let Some(theme) = &config.theme {
        if !VALID_THEMES.contains(&theme.as_str()) {
            errors.push(ValidationError::new(
                file,
                "theme",
                "invalid_value",
                format!(
                    "Unknown theme {:?}. Valid themes: {}",
                    theme,
                    VALID_THEMES.join(", ")
                ),
                Some(format!("Set theme to one of: {}", VALID_THEMES.join(", "))),
            ));
        }
    }
}

// ── Shell / structure validation ──────────────────────

fn validate_shell_structure(file: &str, page: &Page, errors: &mut Vec<ValidationError>) {
    match page.shell {
        Shell::Deck => {
            // Deck pages must have slides.
            if page.slides.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                errors.push(ValidationError::new(
                    file,
                    "slides",
                    "structural",
                    "shell: deck pages must have at least one slide under slides:",
                    Some(
                        "Add a slides: list with at least one item (label: + components:).".into(),
                    ),
                ));
            }
            // Deck pages should not have top-level components.
            if page
                .components
                .as_ref()
                .map(|c| !c.is_empty())
                .unwrap_or(false)
            {
                errors.push(ValidationError::new(
                    file,
                    "components",
                    "structural",
                    "shell: deck pages use slides:, not top-level components:",
                    Some("Move components into slides[N].components:.".into()),
                ));
            }
        }
        Shell::Hub => {
            // Hub pages render components like standard pages and need a
            // hub: block for the masthead.
            if page
                .components
                .as_ref()
                .map(|c| c.is_empty())
                .unwrap_or(true)
            {
                errors.push(ValidationError::new(
                    file,
                    "components",
                    "structural",
                    "shell: hub pages must have at least one component under components:",
                    Some("Add a components: list.".into()),
                ));
            }
            if page.hub.is_none() {
                errors.push(ValidationError::new(
                    file,
                    "hub",
                    "structural",
                    "shell: hub pages need a hub: block (name + pages) for the masthead",
                    Some("Add hub: with name: and a pages: list of {label, href}.".into()),
                ));
            }
        }
        Shell::Standard | Shell::Document => {
            // Non-deck pages must have components.
            if page
                .components
                .as_ref()
                .map(|c| c.is_empty())
                .unwrap_or(true)
            {
                errors.push(ValidationError::new(
                    file,
                    "components",
                    "structural",
                    "page must have at least one component under components:",
                    Some("Add a components: list. Start with - type: header, title: ...".into()),
                ));
            }
            // Non-deck pages should not have slides.
            if page.slides.as_ref().map(|s| !s.is_empty()).unwrap_or(false) {
                errors.push(ValidationError::new(
                    file,
                    "slides",
                    "structural",
                    "slides: is only used by shell: deck pages",
                    Some("Remove slides: or change shell to deck.".into()),
                ));
            }
        }
    }
}

// ── Slide density validation ─────────────────────────

const SLIDE_DENSITY_LIMIT: u32 = 9;

fn component_height_cost(c: &Component) -> u32 {
    match c {
        Component::Header { subtitle, .. } => {
            if subtitle.is_some() {
                2
            } else {
                1
            }
        }
        Component::Callout { .. } => 2,
        Component::Blockquote { .. } => 2,
        Component::StatGrid { stats, .. } => 2 + (stats.len() as u32 / 5),
        Component::CardGrid { cards, .. } => 1 + cards.len() as u32,
        Component::SelectableGrid { cards, .. } => 2 + (cards.len() as u32 / 3),
        Component::Steps { items, .. } => 1 + items.len() as u32,
        Component::Table { rows, .. } => 2 + rows.len() as u32,
        Component::Pipeline { .. } => 5,
        Component::SplitCompare { .. } => 4,
        Component::Chart { .. } => 4,
        Component::Radar { .. } => 4,
        Component::Sankey { .. } => 4,
        Component::Graph { .. } => 4,
        Component::Architecture { .. } => 4,
        Component::Quadrant { .. } => 4,
        Component::OrgChart { .. } => 5,
        Component::Venn { .. } => 3,
        Component::Timeline { items, .. } => 1 + items.len() as u32,
        Component::EventTimeline { events, .. } => 2 + events.len() as u32,
        Component::DefinitionList { items, .. } => 1 + items.len() as u32,
        Component::Accordion { items, .. } => 1 + items.len() as u32,
        Component::BeforeAfter { items, .. } => 2 + items.len() as u32,
        Component::Columns { columns, .. } => columns
            .iter()
            .map(|col| col.iter().map(component_height_cost).sum::<u32>())
            .max()
            .unwrap_or(0),
        Component::Section { components, .. } => {
            1 + components.iter().map(component_height_cost).sum::<u32>()
        }
        Component::Tabs { tabs, .. } => 2 + tabs.len().min(1) as u32,
        Component::Markdown { body, .. } => 1 + (body.lines().count() as u32 / 4),
        Component::Code { code, .. } => 2 + (code.lines().count() as u32 / 3),
        Component::Image { .. } | Component::Embed { .. } => 4,
        Component::Gauge { items, .. } => 2 + items.len() as u32,
        Component::RuleList { items, .. } => 1 + items.len() as u32,
        Component::Tree { .. } => 4,
        Component::PriorityQueue { items, .. } => 2 + items.len() as u32,
        _ => 1,
    }
}

fn validate_slide_density(file: &str, slides: &[Slide], errors: &mut Vec<ValidationError>) {
    for (i, slide) in slides.iter().enumerate() {
        let mut cost: u32 = slide.components.iter().map(component_height_cost).sum();
        // The slide header (eyebrow + title + optional subtitle) is not a
        // component but still consumes vertical space; count it toward density.
        if slide.title.is_some() || slide.eyebrow.is_some() {
            cost += if slide.subtitle.is_some() { 2 } else { 1 };
        }
        if cost > SLIDE_DENSITY_LIMIT {
            errors.push(ValidationError::new(
                file,
                format!("slides[{}]", i),
                "density",
                format!(
                    "Slide {:?} has ~{} content units (limit {}). Content will likely overflow the viewport.",
                    slide.label, cost, SLIDE_DENSITY_LIMIT
                ),
                Some("Split into two slides, or remove components to fit one screen.".into()),
            ));
        }
    }
}

// ── Component validation ──────────────────────────────

fn validate_components(
    file: &str,
    path_prefix: &str,
    components: &[Component],
    errors: &mut Vec<ValidationError>,
) {
    for (i, component) in components.iter().enumerate() {
        let path = format!("{}[{}]", path_prefix, i);
        validate_component(file, &path, component, errors);
    }
}

fn validate_component(
    file: &str,
    path: &str,
    component: &Component,
    errors: &mut Vec<ValidationError>,
) {
    match component {
        Component::CardGrid { cards, .. } => {
            if cards.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.cards", path),
                    "missing_field",
                    "card_grid requires at least one card in cards:",
                    Some("Add at least one card with title: and optional description:.".into()),
                ));
            }
        }

        Component::SelectableGrid { cards, .. } => {
            if cards.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.cards", path),
                    "missing_field",
                    "selectable_grid requires at least one card in cards:",
                    Some("Add at least one card with title:.".into()),
                ));
            }
        }

        Component::Table { columns, rows, .. } => {
            if columns.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.columns", path),
                    "missing_field",
                    "table requires at least one column in columns:",
                    Some("Add columns with key: and label:.".into()),
                ));
            }
            if rows.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.rows", path),
                    "missing_field",
                    "table requires at least one row in rows:",
                    Some("Add at least one row object matching the column keys.".into()),
                ));
            }
            // Validate that each row contains all column keys.
            for (ri, row) in rows.iter().enumerate() {
                for col in columns.iter() {
                    if !row.contains_key(col.key.as_str()) {
                        errors.push(ValidationError::new(
                            file,
                            format!("{}.rows[{}]", path, ri),
                            "missing_field",
                            format!("row is missing column key {:?}", col.key),
                            Some(format!("Add {}: <value> to this row.", col.key)),
                        ));
                    }
                }
            }
        }

        Component::Chart { data, series, .. } => {
            let has_data = data.as_ref().map(|d| !d.is_empty()).unwrap_or(false);
            let has_series = series.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
            if !has_data && !has_series {
                errors.push(ValidationError::new(
                    file,
                    path,
                    "missing_field",
                    "chart requires either data: (single-series) or series: (multi-series)",
                    Some("Add data: [{label: X, value: N}] for a simple chart, or series: for multi-series.".into()),
                ));
            }
            if has_data && has_series {
                errors.push(ValidationError::new(
                    file,
                    path,
                    "structural",
                    "chart cannot have both data: and series: - pick one",
                    Some("Use data: for single-series charts, series: for multi-series.".into()),
                ));
            }
        }

        Component::Timeline { items, .. } => {
            if items.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.items", path),
                    "missing_field",
                    "timeline requires at least one item",
                    Some("Add items with name: and status: (completed/active/upcoming).".into()),
                ));
            }
        }

        Component::StatGrid { stats, .. } => {
            if stats.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.stats", path),
                    "missing_field",
                    "stat_grid requires at least one stat",
                    Some("Add stats with label: and value:.".into()),
                ));
            }
        }

        Component::BeforeAfter { items, .. } => {
            if items.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.items", path),
                    "missing_field",
                    "before_after requires at least one item",
                    Some("Add items with title:, before:, and after:.".into()),
                ));
            }
        }

        Component::Steps { items, .. } => {
            if items.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.items", path),
                    "missing_field",
                    "steps requires at least one item",
                    Some("Add items with title: and optional detail:.".into()),
                ));
            }
        }

        Component::Tabs { tabs, .. } => {
            if tabs.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.tabs", path),
                    "missing_field",
                    "tabs requires at least one tab",
                    Some("Add tabs with label: and components:.".into()),
                ));
            }
            for (ti, tab) in tabs.iter().enumerate() {
                if tab.components.is_empty() {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.tabs[{}].components", path, ti),
                        "missing_field",
                        format!("tab {:?} has no components", tab.label),
                        Some("Add at least one component inside this tab.".into()),
                    ));
                } else {
                    validate_components(
                        file,
                        &format!("{}.tabs[{}].components", path, ti),
                        &tab.components,
                        errors,
                    );
                }
            }
        }

        Component::Section { components, .. } => {
            // A section can be a pure heading/anchor with no nested components - valid.
            if !components.is_empty() {
                validate_components(file, &format!("{}.components", path), components, errors);
            }
        }

        Component::Columns { columns, .. } => {
            if columns.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.columns", path),
                    "missing_field",
                    "columns requires at least one column",
                    Some("Add columns as a list of component lists.".into()),
                ));
            }
            for (ci, col_components) in columns.iter().enumerate() {
                if col_components.is_empty() {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.columns[{}]", path, ci),
                        "missing_field",
                        format!("column {} has no components", ci),
                        Some("Add at least one component to each column.".into()),
                    ));
                } else {
                    validate_components(
                        file,
                        &format!("{}.columns[{}]", path, ci),
                        col_components,
                        errors,
                    );
                }
            }
        }

        Component::Accordion { items, .. } => {
            if items.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.items", path),
                    "missing_field",
                    "accordion requires at least one item",
                    Some("Add items with title: and components:.".into()),
                ));
            }
            for (ai, item) in items.iter().enumerate() {
                if item.components.is_empty() {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.items[{}].components", path, ai),
                        "missing_field",
                        format!("accordion item {:?} has no components", item.title),
                        Some("Add at least one component inside this accordion item.".into()),
                    ));
                } else {
                    validate_components(
                        file,
                        &format!("{}.items[{}].components", path, ai),
                        &item.components,
                        errors,
                    );
                }
            }
        }

        Component::EventTimeline { events, .. } => {
            if events.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.events", path),
                    "missing_field",
                    "event_timeline requires at least one event",
                    Some("Add events with date: and title:.".into()),
                ));
            }
            // Validate date formats.
            for (ei, event) in events.iter().enumerate() {
                if crate::freshness::parse_iso_date(&event.date).is_none() {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.events[{}].date", path, ei),
                        "format",
                        format!("invalid date {:?} - must be YYYY-MM-DD", event.date),
                        Some("Use ISO date format: 2026-01-15".into()),
                    ));
                }
            }
        }

        Component::PriorityQueue { items, .. } => {
            if items.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.items", path),
                    "missing_field",
                    "priority_queue requires at least one item",
                    Some("Add items with label: and optional due:.".into()),
                ));
            }
        }

        Component::Tree { nodes, .. } => {
            if nodes.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.nodes", path),
                    "missing_field",
                    "tree requires at least one node",
                    Some("Add nodes with label: and optional children:.".into()),
                ));
            }
        }

        Component::Venn { sets, .. } => {
            if sets.len() < 2 {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.sets", path),
                    "structural",
                    format!("venn requires at least 2 sets, found {}", sets.len()),
                    Some("Add at least two sets with label:.".into()),
                ));
            }
        }

        Component::Kbd { keys, .. } => {
            if keys.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.keys", path),
                    "missing_field",
                    "kbd requires at least one key",
                    Some("Add keys: [\"Ctrl\", \"C\"] for example.".into()),
                ));
            }
        }

        Component::Breadcrumb { items, .. } => {
            if items.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.items", path),
                    "missing_field",
                    "breadcrumb requires at least one item",
                    Some("Add items with label: and optional href:.".into()),
                ));
            }
        }

        Component::ButtonGroup { .. } => {
            // An empty button_group is valid - buttons may be conditionally populated.
        }

        Component::DefinitionList { items, .. } => {
            if items.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.items", path),
                    "missing_field",
                    "definition_list requires at least one item",
                    Some("Add items with term: and definition:.".into()),
                ));
            }
        }

        Component::AvatarGroup { avatars, .. } => {
            if avatars.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.avatars", path),
                    "missing_field",
                    "avatar_group requires at least one avatar",
                    Some("Add avatars with name:.".into()),
                ));
            }
        }

        Component::Meta { fields, .. } => {
            if fields.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.fields", path),
                    "missing_field",
                    "meta requires at least one field",
                    Some("Add fields with key: and value:.".into()),
                ));
            }
        }

        Component::ProgressBar { value, .. } => {
            if *value > 100 {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.value", path),
                    "invalid_value",
                    format!("progress_bar value {} exceeds 100", value),
                    Some("Set value to a number between 0 and 100.".into()),
                ));
            }
        }

        // Components with no extra validation needed beyond serde.
        Component::Header { .. }
        | Component::Markdown { .. }
        | Component::Callout { .. }
        | Component::Code { .. }
        | Component::Image { .. }
        | Component::Badge { .. }
        | Component::Tag { .. }
        | Component::Divider { .. }
        | Component::Status { .. }
        | Component::Blockquote { .. }
        | Component::Avatar { .. }
        | Component::EmptyState { .. }
        | Component::Icon { .. }
        | Component::Embed { .. }
        | Component::Resources { .. }
        | Component::HeroBanner { .. }
        | Component::RoleMap { .. }
        | Component::SplitCompare { .. } => {}

        Component::Sankey { flows, .. } => {
            if flows.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.flows", path),
                    "missing_field",
                    "sankey requires at least one flow",
                    Some("Add flows with source:, target:, and value:.".into()),
                ));
            }
            for (i, f) in flows.iter().enumerate() {
                if f.value <= 0.0 {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.flows[{}].value", path, i),
                        "structural",
                        "sankey flow value must be > 0",
                        None,
                    ));
                }
                if f.source == f.target {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.flows[{}]", path, i),
                        "structural",
                        "sankey flow source and target must differ",
                        None,
                    ));
                }
            }
        }

        Component::Radar { axes, curves, .. } => {
            if axes.len() < 3 {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.axes", path),
                    "structural",
                    "radar requires at least 3 axes",
                    Some("Add at least 3 axis labels.".into()),
                ));
            }
            if curves.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.curves", path),
                    "missing_field",
                    "radar requires at least one curve",
                    Some("Add curves with label: and values: [].".into()),
                ));
            }
            for (i, c) in curves.iter().enumerate() {
                if c.values.len() != axes.len() {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.curves[{}].values", path, i),
                        "structural",
                        format!(
                            "curve values length ({}) must match axes count ({})",
                            c.values.len(),
                            axes.len()
                        ),
                        None,
                    ));
                }
            }
        }

        Component::Quadrant {
            quadrants, points, ..
        } => {
            if quadrants.len() != 4 {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.quadrants", path),
                    "structural",
                    format!(
                        "quadrant requires exactly 4 labels, got {}",
                        quadrants.len()
                    ),
                    Some("Provide 4 labels: Q1 (top-right), Q2 (top-left), Q3 (bottom-left), Q4 (bottom-right).".into()),
                ));
            }
            if points.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.points", path),
                    "missing_field",
                    "quadrant requires at least one point",
                    Some("Add points with label:, x: (0-1), y: (0-1).".into()),
                ));
            }
        }

        Component::Architecture {
            nodes, connections, ..
        } => {
            if nodes.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.nodes", path),
                    "missing_field",
                    "architecture requires at least one node",
                    Some("Add nodes with id: and label:.".into()),
                ));
            }
            let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
            for (i, c) in connections.iter().enumerate() {
                if !ids.contains(&c.from.as_str()) {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.connections[{}].from", path, i),
                        "structural",
                        format!("connection references unknown node id {:?}", c.from),
                        None,
                    ));
                }
                if !ids.contains(&c.to.as_str()) {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.connections[{}].to", path, i),
                        "structural",
                        format!("connection references unknown node id {:?}", c.to),
                        None,
                    ));
                }
                if c.from == c.to {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.connections[{}]", path, i),
                        "structural",
                        "connection from and to must differ",
                        None,
                    ));
                }
            }
        }
        Component::Pipeline {
            inputs,
            stages,
            outputs,
            ..
        } => {
            if stages.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.stages", path),
                    "missing_field",
                    "pipeline requires at least one stage",
                    Some("Add stages with label: and capabilities:.".into()),
                ));
            }
            if inputs.is_empty() && outputs.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    path.to_string(),
                    "structural",
                    "pipeline needs at least one input or output",
                    None,
                ));
            }
        }
        Component::Graph {
            nodes,
            edges,
            groups,
            ..
        } => {
            if nodes.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.nodes", path),
                    "missing_field",
                    "graph requires at least one node",
                    Some("Add nodes with id: and label:.".into()),
                ));
            }
            let node_ids: std::collections::HashSet<&str> =
                nodes.iter().map(|n| n.id.as_str()).collect();
            for (i, edge) in edges.iter().enumerate() {
                if !node_ids.contains(edge.from.as_str()) {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.edges[{}].from", path, i),
                        "cross_reference",
                        format!("edge references non-existent node '{}'", edge.from),
                        None,
                    ));
                }
                if !node_ids.contains(edge.to.as_str()) {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.edges[{}].to", path, i),
                        "cross_reference",
                        format!("edge references non-existent node '{}'", edge.to),
                        None,
                    ));
                }
            }
            for (i, grp) in groups.iter().enumerate() {
                if let Some(parent) = &grp.parent {
                    if !groups.iter().any(|g| g.id == *parent) {
                        errors.push(ValidationError::new(
                            file,
                            format!("{}.groups[{}].parent", path, i),
                            "cross_reference",
                            format!("group parent '{}' not found", parent),
                            None,
                        ));
                    }
                }
            }
        }
        Component::OrgChart { people, .. } => {
            if people.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.people", path),
                    "missing_field",
                    "org_chart requires at least one person",
                    Some("Add people with id:, name:, and optional reports:.".into()),
                ));
            }
        }

        Component::Aside { body, .. } => {
            if body.trim().is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.body", path),
                    "missing_field",
                    "aside requires a non-empty body",
                    Some("Add body: with the aside content.".into()),
                ));
            }
        }

        Component::RuleList { items, .. } => {
            if items.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.items", path),
                    "missing_field",
                    "rule_list requires at least one item",
                    Some("Add items with label: and body:.".into()),
                ));
            }
            for (i, item) in items.iter().enumerate() {
                if item.label.trim().is_empty() {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.items[{}].label", path, i),
                        "missing_field",
                        "rule_list item requires a non-empty label",
                        Some("Add label: to this item.".into()),
                    ));
                }
                if item.body.trim().is_empty() {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.items[{}].body", path, i),
                        "missing_field",
                        "rule_list item requires a non-empty body",
                        Some("Add body: to this item.".into()),
                    ));
                }
            }
        }

        Component::Gauge { items, max, .. } => {
            if items.is_empty() {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.items", path),
                    "missing_field",
                    "gauge requires at least one item",
                    Some("Add items with label: and value:.".into()),
                ));
            }
            if *max <= 0.0 {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.max", path),
                    "invalid_value",
                    format!("gauge max must be > 0, got {}", max),
                    Some("Set max to a positive number.".into()),
                ));
            }
        }
    }
}

// ── Freshness validation ──────────────────────────────

fn validate_freshness(
    file: &str,
    path: &str,
    freshness: &crate::types::Freshness,
    errors: &mut Vec<ValidationError>,
) {
    if let Some(updated) = &freshness.updated {
        if crate::freshness::parse_iso_date(updated).is_none() {
            errors.push(ValidationError::new(
                file,
                format!("{}.updated", path),
                "format",
                format!("invalid date {:?} - must be YYYY-MM-DD", updated),
                Some("Use ISO date format: 2026-01-15".into()),
            ));
        }
    }
    if let Some(review_every) = &freshness.review_every {
        if crate::freshness::parse_duration_days(review_every).is_none() {
            errors.push(ValidationError::new(
                file,
                format!("{}.review_every", path),
                "format",
                format!(
                    "invalid duration {:?} - accepts Nd/Nw/Nm/Ny or weekly/monthly/quarterly/yearly",
                    review_every
                ),
                Some("Examples: 30d, 4w, 3m, 1y, weekly, monthly, quarterly, yearly".into()),
            ));
        }
    }
}

// ── Nav cross-reference validation ────────────────────

fn validate_nav_links(
    file: &str,
    path: &str,
    nav: &[crate::types::NavLink],
    known_pages: &HashSet<String>,
    errors: &mut Vec<ValidationError>,
) {
    for (i, link) in nav.iter().enumerate() {
        let link_path = format!("{}[{}]", path, i);
        if let Some(href) = &link.href {
            // Only check local hrefs (not external URLs).
            if !href.starts_with("http://")
                && !href.starts_with("https://")
                && !href.starts_with('#')
                && !href.starts_with("mailto:")
            {
                // Normalize: strip leading slash, ensure .html extension.
                let normalized = normalize_href(href);
                if !known_pages.contains(&normalized) {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.href", link_path),
                        "cross_reference",
                        format!("nav href {:?} does not match any known page ({})", href, normalized),
                        Some(format!(
                            "Create a page at the matching .yaml path, or fix the href. Known pages: {}",
                            {
                                let mut sorted: Vec<&String> = known_pages.iter().collect();
                                sorted.sort();
                                sorted.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                            }
                        )),
                    ));
                }
            }
        }
        // Recurse into children.
        if let Some(children) = &link.children {
            validate_nav_links(
                file,
                &format!("{}[{}].children", path, i),
                children,
                known_pages,
                errors,
            );
        }
    }
}

/// Normalize an href to a canonical `path/to/page.html` form for matching
/// against `known_pages`. Strips leading `/`, adds `.html` if missing.
fn normalize_href(href: &str) -> String {
    let href = href.trim_start_matches('/');
    if href.ends_with(".html") {
        href.to_string()
    } else if href.ends_with(".yaml") {
        format!("{}.html", href.trim_end_matches(".yaml"))
    } else if href.is_empty() || href.ends_with('/') {
        format!("{}index.html", href)
    } else {
        format!("{}.html", href)
    }
}

// ── Pretty printing ───────────────────────────────────

/// Print validation errors to stderr in a human-readable, colored format.
pub fn print_pretty(errors: &[ValidationError]) {
    if errors.is_empty() {
        eprintln!("\u{2713} No validation errors found.");
        return;
    }
    // Group by file.
    let mut by_file: std::collections::BTreeMap<&str, Vec<&ValidationError>> =
        std::collections::BTreeMap::new();
    for e in errors {
        by_file.entry(&e.file).or_default().push(e);
    }
    for (file, errs) in &by_file {
        eprintln!("\n\u{2718} {} ({} error(s))", file, errs.len());
        for e in errs {
            eprintln!("    [{:^16}] {}", e.error_type, e.message);
            if !e.path.is_empty() {
                eprintln!("    {:>18} {}", "at:", e.path);
            }
            if let Some(hint) = &e.suggestion {
                eprintln!("    {:>18} {}", "hint:", hint);
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Component, Freshness, Page, Shell};

    fn make_page(shell: Shell, components: Option<Vec<Component>>) -> Page {
        Page {
            title: "Test Page".into(),
            shell,
            eyebrow: None,
            subtitle: None,
            components,
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
            skill: None,
        }
    }

    fn pack_meta(targets: &[&str]) -> crate::types::PackMeta {
        crate::types::PackMeta {
            targets: targets.iter().map(|s| s.to_string()).collect(),
            hooks: Vec::new(),
        }
    }

    #[test]
    fn pack_with_markdown_passes() {
        let mut page = make_page(
            Shell::Standard,
            Some(vec![Component::Markdown {
                body: "rules".into(),
                scale: None,
            }]),
        );
        page.pack = Some(pack_meta(&[]));
        assert!(validate_page("p.yaml", &page).is_empty());
    }

    #[test]
    fn pack_with_markdown_in_section_passes() {
        let mut page = make_page(
            Shell::Standard,
            Some(vec![Component::Section {
                heading: Some("Rules".into()),
                eyebrow: None,
                components: vec![Component::Markdown {
                    body: "rules".into(),
                    scale: None,
                }],
                align: Default::default(),
                id: None,
                scale: None,
            }]),
        );
        page.pack = Some(pack_meta(&["claude", "cursor"]));
        assert!(validate_page("p.yaml", &page).is_empty());
    }

    #[test]
    fn pack_without_markdown_fails_structural() {
        let mut page = make_page(Shell::Standard, Some(vec![header_component()]));
        page.pack = Some(pack_meta(&[]));
        let errors = validate_page("p.yaml", &page);
        assert!(errors
            .iter()
            .any(|e| e.error_type == "structural" && e.path == "pack"));
    }

    #[test]
    fn pack_with_empty_markdown_fails_structural() {
        let mut page = make_page(
            Shell::Standard,
            Some(vec![Component::Markdown {
                body: "   ".into(),
                scale: None,
            }]),
        );
        page.pack = Some(pack_meta(&[]));
        let errors = validate_page("p.yaml", &page);
        assert!(errors
            .iter()
            .any(|e| e.error_type == "structural" && e.path == "pack"));
    }

    #[test]
    fn pack_with_unknown_target_fails() {
        let mut page = make_page(
            Shell::Standard,
            Some(vec![Component::Markdown {
                body: "rules".into(),
                scale: None,
            }]),
        );
        page.pack = Some(pack_meta(&["claude", "notatool"]));
        let errors = validate_page("p.yaml", &page);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, "pack.targets[1]");
        assert_eq!(errors[0].error_type, "invalid_value");
    }

    fn header_component() -> Component {
        Component::Header {
            title: "Hello".into(),
            subtitle: None,
            eyebrow: None,
            align: Default::default(),
            id: None,
            scale: None,
        }
    }

    // ── Valid page passes validation ──────────────────

    #[test]
    fn valid_standard_page_passes() {
        let page = make_page(Shell::Standard, Some(vec![header_component()]));
        let errors = validate_page("test.yaml", &page);
        assert!(
            errors.is_empty(),
            "expected no errors, got: {:?}",
            errors.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn valid_deck_page_passes() {
        use crate::types::Slide;
        let page = Page {
            title: "Deck".into(),
            shell: Shell::Deck,
            eyebrow: None,
            subtitle: None,
            components: None,
            slides: Some(vec![Slide {
                label: "Slide 1".into(),
                components: vec![header_component()],
                title: None,
                eyebrow: None,
                subtitle: None,
                align: None,
                valign: None,
                cover: false,
                hide_label: false,
            }]),
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
            skill: None,
        };
        let errors = validate_page("deck.yaml", &page);
        assert!(
            errors.is_empty(),
            "expected no errors, got: {:?}",
            errors.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }

    // ── Structural rules enforced ─────────────────────

    #[test]
    fn standard_page_without_components_fails() {
        let page = make_page(Shell::Standard, None);
        let errors = validate_page("test.yaml", &page);
        assert!(
            errors.iter().any(|e| e.error_type == "structural"),
            "expected structural error"
        );
    }

    #[test]
    fn deck_page_without_slides_fails() {
        let page = make_page(Shell::Deck, None);
        let errors = validate_page("test.yaml", &page);
        assert!(
            errors
                .iter()
                .any(|e| e.error_type == "structural" && e.path == "slides"),
            "expected structural error on slides"
        );
    }

    // ── Missing required fields caught ────────────────

    #[test]
    fn card_grid_with_empty_cards_fails() {
        use crate::types::Connector;
        let page = make_page(
            Shell::Standard,
            Some(vec![Component::CardGrid {
                cards: vec![],
                min_width: None,
                connector: Connector::None,
                scale: None,
            }]),
        );
        let errors = validate_page("test.yaml", &page);
        assert!(
            errors
                .iter()
                .any(|e| e.path.contains("cards") && e.error_type == "missing_field"),
            "expected missing_field on cards"
        );
    }

    #[test]
    fn table_requires_columns_and_rows() {
        let page = make_page(
            Shell::Standard,
            Some(vec![Component::Table {
                columns: vec![],
                rows: vec![],
                filterable: false,
                summary: None,
                scale: None,
            }]),
        );
        let errors = validate_page("test.yaml", &page);
        assert!(errors.iter().any(|e| e.path.contains("columns")));
        assert!(errors.iter().any(|e| e.path.contains("rows")));
    }

    // ── Component-specific rules ──────────────────────

    #[test]
    fn chart_needs_data_or_series() {
        use crate::types::ChartKind;
        let page = make_page(
            Shell::Standard,
            Some(vec![Component::Chart {
                kind: ChartKind::Bar,
                title: None,
                height: None,
                x_label: None,
                y_label: None,
                orientation: Default::default(),
                data: None,
                series: None,
                scale: None,
            }]),
        );
        let errors = validate_page("test.yaml", &page);
        assert!(
            errors.iter().any(|e| e.error_type == "missing_field"),
            "expected missing_field for chart without data or series"
        );
    }

    #[test]
    fn chart_cannot_have_both_data_and_series() {
        use crate::types::{ChartKind, ChartPoint, ChartSeries};
        let page = make_page(
            Shell::Standard,
            Some(vec![Component::Chart {
                kind: ChartKind::Bar,
                title: None,
                height: None,
                x_label: None,
                y_label: None,
                orientation: Default::default(),
                data: Some(vec![ChartPoint {
                    label: "A".into(),
                    value: 1.0,
                    color: None,
                }]),
                series: Some(vec![ChartSeries {
                    label: "S".into(),
                    color: None,
                    points: vec![ChartPoint {
                        label: "A".into(),
                        value: 1.0,
                        color: None,
                    }],
                }]),
                scale: None,
            }]),
        );
        let errors = validate_page("test.yaml", &page);
        assert!(
            errors.iter().any(|e| e.error_type == "structural"),
            "expected structural error for chart with both data and series"
        );
    }

    #[test]
    fn progress_bar_over_100_fails() {
        let page = make_page(
            Shell::Standard,
            Some(vec![Component::ProgressBar {
                value: 150,
                label: None,
                color: Default::default(),
                detail: None,
                target: None,
                thresholds: std::collections::HashMap::new(),
                scale: None,
            }]),
        );
        let errors = validate_page("test.yaml", &page);
        assert!(
            errors.iter().any(|e| e.error_type == "invalid_value"),
            "expected invalid_value for progress_bar > 100"
        );
    }

    // ── Freshness format validation ───────────────────

    #[test]
    fn freshness_bad_date_fails() {
        let mut page = make_page(Shell::Standard, Some(vec![header_component()]));
        page.freshness = Some(crate::types::FreshnessValue::Full(Freshness {
            updated: Some("not-a-date".into()),
            review_every: None,
            owner: None,
            sources_of_truth: None,
            expires: None,
            refresh: None,
        }));
        let errors = validate_page("test.yaml", &page);
        assert!(
            errors
                .iter()
                .any(|e| e.error_type == "format" && e.path.contains("updated")),
            "expected format error on freshness.updated"
        );
    }

    #[test]
    fn freshness_bad_duration_fails() {
        let mut page = make_page(Shell::Standard, Some(vec![header_component()]));
        page.freshness = Some(crate::types::FreshnessValue::Full(Freshness {
            updated: None,
            review_every: Some("once in a while".into()),
            owner: None,
            sources_of_truth: None,
            expires: None,
            refresh: None,
        }));
        let errors = validate_page("test.yaml", &page);
        assert!(
            errors
                .iter()
                .any(|e| e.error_type == "format" && e.path.contains("review_every")),
            "expected format error on freshness.review_every"
        );
    }

    #[test]
    fn freshness_valid_values_pass() {
        let mut page = make_page(Shell::Standard, Some(vec![header_component()]));
        page.freshness = Some(crate::types::FreshnessValue::Full(Freshness {
            updated: Some("2026-01-15".into()),
            review_every: Some("quarterly".into()),
            owner: Some("team@example.com".into()),
            sources_of_truth: None,
            expires: None,
            refresh: None,
        }));
        let errors = validate_page("test.yaml", &page);
        assert!(errors.is_empty(), "expected no errors for valid freshness");
    }

    // ── Error JSON format ─────────────────────────────

    #[test]
    fn error_serializes_to_json_correctly() {
        let err = ValidationError {
            file: "foo.yaml".into(),
            path: "components[0].cards".into(),
            error_type: "missing_field".into(),
            message: "needs at least one card".into(),
            suggestion: Some("add a card".into()),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"file\":\"foo.yaml\""));
        assert!(json.contains("\"path\":\"components[0].cards\""));
        assert!(json.contains("\"error_type\":\"missing_field\""));
        assert!(json.contains("\"suggestion\":\"add a card\""));
    }

    #[test]
    fn error_without_suggestion_omits_field() {
        let err = ValidationError {
            file: "foo.yaml".into(),
            path: "".into(),
            error_type: "structural".into(),
            message: "something wrong".into(),
            suggestion: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(!json.contains("suggestion"));
    }

    // ── Nav href normalization ────────────────────────

    #[test]
    fn normalize_href_various_forms() {
        assert_eq!(normalize_href("index.html"), "index.html");
        assert_eq!(normalize_href("/index.html"), "index.html");
        assert_eq!(normalize_href("foo/bar"), "foo/bar.html");
        assert_eq!(normalize_href("foo/bar.yaml"), "foo/bar.html");
        assert_eq!(normalize_href(""), "index.html");
        assert_eq!(normalize_href("/"), "index.html");
    }

    // ── skill page validation ────────────────────────────

    const GOOD_AGL: &str = "```agl\nspec Demo {\n  in: none: str\n  out: done: bool\n  description: \"demo\"\n\n  flow {\n    state START -> call(Bash, \"echo hi\") -> next\n    state FINISH -> evaluate(result vs expectation) -> TERMINATE(\"done\")\n  }\n}\n```";

    fn skill_page(body: &str) -> Page {
        let mut page = make_page(
            Shell::Document,
            Some(vec![Component::Markdown {
                body: body.into(),
                scale: None,
            }]),
        );
        page.skill = Some(crate::types::SkillMeta {
            trigger: Some("demo".into()),
            requires: Vec::new(),
        });
        page
    }

    #[test]
    fn skill_page_with_valid_agl_fence_passes() {
        let page = skill_page(&format!("Steps first.\n\n{GOOD_AGL}"));
        let errors = validate_page("skill.yaml", &page);
        assert!(errors.is_empty(), "unexpected: {errors:?}");
    }

    #[test]
    fn skill_page_without_markdown_errors() {
        let mut page = make_page(Shell::Document, Some(vec![]));
        page.skill = Some(crate::types::SkillMeta {
            trigger: None,
            requires: Vec::new(),
        });
        let errors = validate_page("skill.yaml", &page);
        assert!(errors
            .iter()
            .any(|e| e.error_type == "skill" && e.message.contains("markdown")));
    }

    #[test]
    fn skill_page_with_broken_agl_graph_errors() {
        // ORPHAN is unreachable - the analyzer must block the save.
        let body = "```agl\nspec Demo {\n  in: none: str\n  out: done: bool\n  description: \"demo\"\n\n  flow {\n    state START -> call(Bash, \"echo hi\") -> TERMINATE(\"done\")\n    state ORPHAN -> call(Bash, \"echo lost\") -> TERMINATE(\"lost\")\n  }\n}\n```";
        let page = skill_page(body);
        let errors = validate_page("skill.yaml", &page);
        assert!(
            errors.iter().any(|e| e.error_type == "skill"),
            "expected analyzer error: {errors:?}"
        );
    }

    #[test]
    fn skill_page_with_parse_error_errors() {
        let page = skill_page("```agl\nthis is not agl\n```");
        let errors = validate_page("skill.yaml", &page);
        assert!(errors.iter().any(|e| e.message.contains("parse error")));
    }

    #[test]
    fn skill_page_with_imports_errors() {
        let body = "```agl\nimport \"shared\"\nspec Demo {\n  in: none: str\n  out: done: bool\n  description: \"demo\"\n\n  flow {\n    state START -> call(Bash, \"echo hi\") -> TERMINATE(\"done\")\n  }\n}\n```";
        let page = skill_page(body);
        let errors = validate_page("skill.yaml", &page);
        assert!(errors.iter().any(|e| e.message.contains("self-contained")));
    }

    #[test]
    fn non_skill_page_ignores_agl_fences() {
        let page = make_page(
            Shell::Document,
            Some(vec![Component::Markdown {
                body: "```agl\nbroken\n```".into(),
                scale: None,
            }]),
        );
        let errors = validate_page("plain.yaml", &page);
        assert!(errors.is_empty(), "unexpected: {errors:?}");
    }
}

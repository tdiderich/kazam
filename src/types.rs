use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Shell ────────────────────────────────────────────

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Shell {
    Standard,
    Document,
    Deck,
    Hub,
}

impl Shell {
    pub fn class(&self) -> &'static str {
        match self {
            Shell::Standard => "shell-standard",
            Shell::Document => "shell-document",
            Shell::Deck => "shell-deck",
            Shell::Hub => "shell-hub",
        }
    }
}

// ── Hub shell config ─────────────────────────────────

/// Groups sibling pages (a customer's account plan, priorities, deployment
/// tracker, notes…) under a persistent masthead with tab navigation. Every
/// page in the group declares the same `hub:` block; the active tab is
/// detected from the page being rendered.
#[derive(Deserialize, Clone)]
pub struct HubConfig {
    /// Hub identity shown in the masthead - usually the customer name.
    pub name: String,
    /// Small label above the name, e.g. "CUSTOMER" or the segment.
    #[serde(default)]
    pub eyebrow: Option<String>,
    /// Status badge next to the name, e.g. "Deploying" or "Healthy".
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub status_color: Option<SemColor>,
    /// Tabs, in order. Hrefs resolve like any page link.
    #[serde(default)]
    pub pages: Vec<HubLink>,
}

#[derive(Deserialize, Clone)]
pub struct HubLink {
    pub label: String,
    pub href: String,
}

/// `skill:` block on a page - marks it as an agent skill whose procedure
/// content (markdown steps and/or ```agl fences) `kazam validate` checks.
#[derive(Deserialize)]
// trigger/requires are schema contract for the skill compile path
// (kazam install -> .claude/skills); validation only checks presence today.
#[allow(dead_code)]
pub struct SkillMeta {
    /// Phrases that should route an agent to this skill.
    #[serde(default)]
    pub trigger: Option<String>,
    /// Tools/servers the skill needs at run time (informational).
    #[serde(default)]
    pub requires: Vec<String>,
}

/// `pack:` block on a page - marks it as an AI tool pack.
#[derive(Deserialize)]
pub struct PackMeta {
    /// Which tool config files to write. Empty = all supported targets.
    /// Valid values: claude, cursor, agents, windsurf, copilot, gemini, aider.
    #[serde(default)]
    pub targets: Vec<String>,
    /// Declarative guardrail hooks. Packs never ship executable code; each hook
    /// is data the trusted kazam runner interprets. Deny/inject/review only.
    #[serde(default)]
    pub hooks: Vec<PackHook>,
}

/// Which tool call a PreToolUse/PostToolUse hook applies to. `tool` is a
/// matcher pattern passed through verbatim to the harness, so any matcher the
/// harness understands works: a single tool ("Write"), a pipe alternation
/// ("Write|Edit"), or an MCP tool name / prefix
/// ("mcp__claude_ai_Slack__slack_send_message", "mcp__.*").
#[derive(Deserialize, Serialize, Clone)]
pub struct HookMatch {
    pub tool: String,
}

#[derive(Deserialize, Serialize, Clone, Copy, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    #[default]
    Substring,
    /// Substring match constrained to word boundaries: the pattern only matches
    /// when the characters on either side are non-word (not alphanumeric or
    /// `_`). Blocks "delve" but not "fostering" for a "foster" pattern.
    Word,
    Regex,
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InjectEvent {
    SessionStart,
    UserPromptSubmit,
}

/// A declarative guardrail primitive. Tagged by `kind`. None of these can read
/// arbitrary files, make network calls, or write data anywhere: the runner that
/// executes them has no egress capability, so a hostile pack can at worst block
/// the user's own tool calls or inject visible text.
#[derive(Deserialize, Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackHook {
    /// Block a tool call if the content matches any pattern.
    BlockOnMatch {
        on: HookMatch,
        #[serde(default)]
        mode: MatchMode,
        /// Which `tool_input` field to scan. Unset = scan the whole serialized
        /// `tool_input` (fine for Write/Edit `content`). Set it to target one
        /// field on an MCP tool, e.g. `text` for a Slack message body, so the
        /// scan doesn't false-positive on other args or field names.
        #[serde(default)]
        field: Option<String>,
        patterns: Vec<String>,
        message: String,
    },
    /// Block a tool call unless the content matches a required pattern.
    BlockUnlessMatch {
        on: HookMatch,
        require: String,
        message: String,
    },
    /// Block unless a named tool_input field is in an allowed set.
    Allowlist {
        on: HookMatch,
        field: String,
        allow: Vec<String>,
        message: String,
    },
    /// Add static or templated text to context (supports {{date}}).
    Inject { event: InjectEvent, text: String },
    /// Run an LLM review with a supplied prompt (harness runs the model).
    ReviewPrompt {
        on: HookMatch,
        prompt: String,
        #[serde(default)]
        model_tier: Option<String>,
    },
}

// ── Page ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct Page {
    pub title: String,
    pub shell: Shell,
    pub eyebrow: Option<String>,
    pub subtitle: Option<String>,
    pub components: Option<Vec<Component>>,
    pub slides: Option<Vec<Slide>>,
    /// Exclude this page from llms.txt. Useful for drafts.
    #[serde(default)]
    pub unlisted: bool,
    /// Override the site-wide `texture` on this page. `Some(Texture::None)`
    /// turns the texture off on this page; any other `Some(_)` swaps in a
    /// different preset. `None` (unset) means inherit the site-wide value.
    #[serde(default)]
    pub texture: Option<Texture>,
    /// Override the site-wide `glow` on this page. Same semantics as `texture`
    /// above: unset = inherit, any Some value wins over the site config.
    #[serde(default)]
    pub glow: Option<Glow>,
    /// How `shell: deck` pages export to PDF. `slides` (default): one slide per
    /// landscape page, Keynote-style. `continuous`: all slides flow on a single
    /// scrolling document with a thin separator between them - nicer for
    /// sharing as a readable artifact rather than a presentation. `square`:
    /// one slide per square page, sized for LinkedIn-style document carousels
    /// where the viewport is near-square and landscape PDFs letterbox badly.
    #[serde(default)]
    pub print_flow: Option<PrintFlow>,
    /// Hub-shell grouping config. Required when `shell: hub`; ignored on
    /// other shells.
    #[serde(default)]
    pub hub: Option<HubConfig>,
    /// Extra search keywords that don't appear in rendered content.
    /// Useful for aliases, acronyms, internal jargon.
    #[serde(default)]
    pub search_terms: Vec<String>,
    /// Optional freshness metadata: owner, last content update, review cadence,
    /// and sources of truth the agent / reader can consult to refresh the
    /// page. When the page is past its review window, a banner is injected
    /// at the top of the rendered output and the build reports the page as
    /// stale. Zero runtime JS - staleness is computed at `kazam build` time.
    /// Set to `"never"` to explicitly opt out of freshness checks with no
    /// warning emitted.
    #[serde(default)]
    pub freshness: Option<FreshnessValue>,
    /// Who is responsible for this page. Free-form string - email, Slack
    /// handle, or team name. Serves as a fallback for `freshness.owner` in
    /// the stale-page report when no freshness block is present.
    #[serde(default)]
    pub owner: Option<String>,
    /// AI tool pack marker. Present = this page is installable via
    /// `kazam install` - its markdown components compile into local AI
    /// config files. Validation requires at least one non-empty markdown
    /// component (top-level or inside a section) when this is set.
    #[serde(default)]
    pub pack: Option<PackMeta>,
    /// Marks this page as an agent skill. Skill pages carry procedure
    /// content (markdown steps and/or ```agl fences); `kazam validate` runs
    /// the AGL static analyzer on every fence so a broken graph never saves.
    #[serde(default)]
    pub skill: Option<SkillMeta>,
    /// Links to sources of truth that inform this page's content.
    /// Each entry has a URL and an optional note explaining what it references.
    #[serde(default)]
    pub references: Vec<Reference>,
    /// Role-based persona tags for this page. Values are freeform strings
    /// matching roles defined in kazam.yaml (e.g. "everyone", "engineering",
    /// "gtm", "product", "ops"). Pages with no personas default to being
    /// visible to everyone. Used by nav filtering and role-map components.
    #[serde(default)]
    pub personas: Vec<String>,
    /// Manually archive this page. Archived pages are still rendered (accessible
    /// via direct URL) but excluded from nav, search, llms.txt, and sitemap.
    /// A banner is injected at build time. Pages past their `freshness.expires`
    /// date are auto-archived without needing this flag.
    #[serde(default)]
    pub archived: bool,
    /// Mark this page as a draft. Drafts are excluded from nav, search,
    /// llms.txt, and sitemap and get a "Draft" banner at build time.
    /// Drafts that sit unchanged for 30+ days are auto-archived.
    #[serde(default)]
    pub draft: bool,
    /// Override the site-wide `nav_layout` on this page. Unset = inherit.
    #[serde(default)]
    pub nav_layout: Option<NavLayout>,
    /// Override the site-wide `nav` on this page. Unset = inherit.
    #[serde(default)]
    pub nav: Option<Vec<NavLink>>,
}

impl Page {
    pub fn is_archived(&self, today: &str) -> bool {
        if self.archived {
            return true;
        }
        let freshness = self.freshness.as_ref().and_then(|fv| fv.as_full());
        if crate::freshness::is_expired(freshness, today) {
            return true;
        }
        if self.draft {
            return crate::freshness::is_stale_draft(freshness, today);
        }
        false
    }
}

/// Freshness value: either the bare string `"never"` (explicit opt-out -
/// no decay checks, no warning) or a full metadata struct.
#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum FreshnessValue {
    /// Bare string `"never"` - page explicitly opts out of freshness checks.
    Never(FreshnessNever),
    /// Full freshness metadata struct.
    Full(Freshness),
}

impl FreshnessValue {
    /// Return the inner `Freshness` struct if this is a `Full` variant.
    pub fn as_full(&self) -> Option<&Freshness> {
        match self {
            FreshnessValue::Full(f) => Some(f),
            FreshnessValue::Never(_) => None,
        }
    }

    /// True when the value is `"never"`.
    pub fn is_never(&self) -> bool {
        matches!(self, FreshnessValue::Never(_))
    }
}

/// Captures the bare `"never"` string via serde rename.
#[derive(Deserialize, Clone)]
pub enum FreshnessNever {
    #[serde(rename = "never")]
    Never,
}

/// One reference entry. A URL pointing to a source of truth for this page's
/// content, with an optional short note explaining what it covers.
#[derive(Deserialize, Clone)]
pub struct Reference {
    /// URL to the source (PR, Slack thread, meeting notes, doc, etc.)
    pub url: String,
    /// Short description of what this reference covers
    #[serde(default)]
    pub note: Option<String>,
}

/// Freshness metadata for a page - when was it last updated, who owns it,
/// how often should it be reviewed, and where are the sources of truth.
#[derive(Deserialize, Clone)]
pub struct Freshness {
    /// ISO date (YYYY-MM-DD) of the last content update.
    pub updated: Option<String>,
    /// Review cadence. Accepts `Nd` (days), `Nw` (weeks), `Nm` (months,
    /// 30-day approximation), `Ny` (years, 365-day approximation), or the
    /// string shortcuts `weekly`, `monthly`, `quarterly`, `yearly`,
    /// `annually`.
    pub review_every: Option<String>,
    /// Who should be contacted before changes land. Free-form - email,
    /// Slack handle, or team name.
    pub owner: Option<String>,
    /// Pointers the agent / reader should consult to refresh the content.
    /// Shorthand form is a bare URL string; expanded form accepts a label
    /// alongside the href.
    #[serde(default)]
    pub sources_of_truth: Option<Vec<SourceOfTruth>>,
    /// Hard expiration date (ISO YYYY-MM-DD). Pages past this date are
    /// treated as expired - excluded from nav/search, rendered with an
    /// "expired" banner. For time-bound content like event materials or
    /// campaign pages.
    #[serde(default)]
    pub expires: Option<String>,
    /// How this page gets refreshed - bare string (prompt shorthand) or
    /// full config with mode + steps. Not used by the build.
    #[serde(default)]
    pub refresh: Option<RefreshValue>,
}

/// One source-of-truth entry. Either a bare URL or a labeled link.
#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum SourceOfTruth {
    Simple(String),
    Full { label: String, href: String },
}

impl SourceOfTruth {
    pub fn href(&self) -> &str {
        match self {
            SourceOfTruth::Simple(h) => h,
            SourceOfTruth::Full { href, .. } => href,
        }
    }
    pub fn label(&self) -> &str {
        match self {
            SourceOfTruth::Simple(h) => h,
            SourceOfTruth::Full { label, .. } => label,
        }
    }
}

/// How a page gets refreshed: human-only, fully automated, or
/// script + LLM + human review.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum RefreshValue {
    /// Bare string shorthand - assisted mode with a single prompt step.
    Prompt(String),
    /// Full refresh configuration with mode and steps.
    Full(RefreshConfig),
}

/// Full refresh configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct RefreshConfig {
    /// human | auto | assisted. Defaults to assisted.
    #[serde(default)]
    pub mode: RefreshMode,
    /// Ordered recipe: run (shell), prompt (LLM), review (human checkpoint).
    #[serde(default)]
    pub steps: Vec<RefreshStep>,
}

/// Refresh mode.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum RefreshMode {
    Human,
    Auto,
    #[default]
    Assisted,
}

/// One step in a refresh recipe.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum RefreshStep {
    /// Shell command to run (e.g. a data-gathering script).
    Run(String),
    /// Prompt for an LLM agent.
    Prompt(String),
    /// Human review checkpoint. Value is who reviews (e.g. "owner").
    Review(String),
}

#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrintFlow {
    #[default]
    Slides,
    Continuous,
    Square,
}

#[derive(Deserialize)]
pub struct Slide {
    pub label: String,
    pub components: Vec<Component>,
    /// Slide heading, rendered left-aligned by default. When set, the slide
    /// owns its title directly and no nested section is needed.
    #[serde(default)]
    pub title: Option<String>,
    /// Kicker above the title (e.g. "STAGE 1").
    #[serde(default)]
    pub eyebrow: Option<String>,
    /// Optional supporting line under the title.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Content alignment: "left" (default) or "center".
    #[serde(default)]
    pub align: Option<String>,
    /// Vertical alignment of slide content: "center" (default) or "top".
    #[serde(default)]
    pub valign: Option<String>,
    /// Cover/title slide: centered, oversized hero layout.
    #[serde(default)]
    pub cover: bool,
    /// Hide the nav label from the slide body. Does NOT affect alignment;
    /// use `cover` or `align: center` for centered layouts.
    #[serde(default)]
    pub hide_label: bool,
}

// ── Components ───────────────────────────────────────

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Component {
    Header {
        title: String,
        subtitle: Option<String>,
        eyebrow: Option<String>,
        #[serde(default)]
        align: Align,
        /// Optional stable anchor id on the rendered wrapper. When unset,
        /// kazam auto-slugs from `title` (lowercase, hyphens, punctuation
        /// stripped) so `#deep-link` URLs Just Work. An explicit id wins
        /// over the auto-slug so copy changes don't break existing
        /// bookmarks. Collisions on the same page suffix `-2`, `-3`, etc.
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    HeroBanner {
        title: String,
        eyebrow: Option<String>,
        subtitle: Option<String>,
        buttons: Option<Vec<ButtonConfig>>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Meta {
        fields: Vec<MetaField>,
        #[serde(default)]
        scale: Option<f32>,
    },
    CardGrid {
        cards: Vec<Card>,
        #[serde(default)]
        min_width: Option<u32>,
        #[serde(default)]
        connector: Connector,
        #[serde(default)]
        scale: Option<f32>,
    },
    SelectableGrid {
        cards: Vec<SelectableCard>,
        #[serde(default)]
        interaction: Interaction,
        #[serde(default)]
        connector: Connector,
        #[serde(default)]
        scale: Option<f32>,
    },
    Timeline {
        items: Vec<TimelineItem>,
        #[serde(default)]
        scale: Option<f32>,
    },
    StatGrid {
        stats: Vec<Stat>,
        #[serde(default = "default_stat_columns")]
        columns: u32,
        #[serde(default)]
        scale: Option<f32>,
    },
    BeforeAfter {
        items: Vec<BeforeAfterItem>,
        #[serde(default)]
        before_label: Option<String>,
        #[serde(default)]
        after_label: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    SplitCompare {
        left: ComparePanel,
        right: ComparePanel,
        #[serde(default)]
        scale: Option<f32>,
    },
    Steps {
        items: Vec<Step>,
        #[serde(default = "default_true")]
        numbered: bool,
        #[serde(default)]
        scale: Option<f32>,
    },
    Markdown {
        body: String,
        #[serde(default)]
        scale: Option<f32>,
    },
    Table {
        columns: Vec<TableColumn>,
        rows: Vec<HashMap<String, serde_yaml::Value>>,
        #[serde(default)]
        filterable: bool,
        #[serde(default)]
        summary: Option<TableSummary>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Callout {
        #[serde(default)]
        variant: CalloutVariant,
        title: Option<String>,
        body: String,
        links: Option<Vec<ButtonConfig>>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Code {
        language: Option<String>,
        code: String,
        #[serde(default)]
        scale: Option<f32>,
    },
    Tabs {
        tabs: Vec<Tab>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Section {
        heading: Option<String>,
        eyebrow: Option<String>,
        components: Vec<Component>,
        #[serde(default)]
        align: Align,
        /// Optional stable anchor id on the rendered wrapper. When unset
        /// and `heading` is present, kazam auto-slugs from the heading
        /// text. Explicit id wins. Same collision handling as `header`.
        /// No heading and no explicit id → no id attribute emitted.
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Columns {
        columns: Vec<Vec<Component>>,
        #[serde(default)]
        equal_heights: bool,
        #[serde(default)]
        scale: Option<f32>,
    },
    Accordion {
        items: Vec<AccordionItem>,
        #[serde(default)]
        scale: Option<f32>,
    },
    EventTimeline {
        events: Vec<EventItem>,
        #[serde(default)]
        default_filter: EventFilter,
        #[serde(default)]
        show_filter_toggle: bool,
        #[serde(default)]
        limit: Option<u32>,
        #[serde(default)]
        filter_by: Vec<String>,
        #[serde(default)]
        group_by: Option<EventGroupBy>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Tree {
        nodes: Vec<TreeNode>,
        #[serde(default)]
        default_filter: TreeFilter,
        #[serde(default)]
        show_filter_toggle: bool,
        #[serde(default)]
        default_collapsed: bool,
        #[serde(default)]
        default_depth: Option<u32>,
        #[serde(default)]
        show_counts: bool,
        #[serde(default)]
        show_summary: bool,
        #[serde(default)]
        default_view: TreeDefaultView,
        #[serde(default)]
        scale: Option<f32>,
    },
    PriorityQueue {
        items: Vec<QueueItem>,
        #[serde(default)]
        group_by: QueueGroup,
        #[serde(default = "default_true")]
        show_dates: bool,
        #[serde(default = "default_true")]
        show_counts: bool,
        #[serde(default)]
        filterable: bool,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Venn {
        sets: Vec<VennSet>,
        #[serde(default)]
        overlaps: Vec<VennOverlap>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Image {
        src: String,
        alt: Option<String>,
        caption: Option<String>,
        max_width: Option<u32>,
        #[serde(default)]
        align: Align,
        #[serde(default)]
        scale: Option<f32>,
    },
    /// Responsive iframe embed for Loom, YouTube, Vimeo, etc.
    Embed {
        src: String,
        title: Option<String>,
        aspect: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    /// Structured link collection with per-item metadata. Consolidates
    /// the "page that's just a few links" pattern into a reviewable list.
    Resources {
        items: Vec<ResourceItem>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Badge {
        label: String,
        #[serde(default)]
        color: SemColor,
        #[serde(default)]
        scale: Option<f32>,
    },
    Tag {
        label: String,
        #[serde(default)]
        color: SemColor,
        #[serde(default)]
        scale: Option<f32>,
    },
    Divider {
        label: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Kbd {
        keys: Vec<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Status {
        label: String,
        #[serde(default)]
        color: SemColor,
        #[serde(default)]
        scale: Option<f32>,
    },
    Breadcrumb {
        items: Vec<BreadcrumbItem>,
        #[serde(default)]
        scale: Option<f32>,
    },
    ButtonGroup {
        buttons: Vec<ButtonConfig>,
        #[serde(default)]
        scale: Option<f32>,
    },
    DefinitionList {
        items: Vec<DefinitionItem>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Blockquote {
        body: String,
        attribution: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Avatar {
        name: String,
        src: Option<String>,
        #[serde(default)]
        size: AvatarSize,
        subtitle: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    AvatarGroup {
        avatars: Vec<AvatarConfig>,
        #[serde(default)]
        size: AvatarSize,
        #[serde(default = "default_avatar_max")]
        max: usize,
        #[serde(default)]
        scale: Option<f32>,
    },
    ProgressBar {
        value: u8,
        label: Option<String>,
        #[serde(default = "default_color")]
        color: String,
        detail: Option<String>,
        #[serde(default)]
        target: Option<u8>,
        #[serde(default)]
        thresholds: HashMap<String, String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    EmptyState {
        title: String,
        body: Option<String>,
        action: Option<EmptyStateAction>,
        #[serde(default)]
        icon: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Icon {
        name: String,
        #[serde(default)]
        size: IconSize,
        #[serde(default)]
        color: SemColor,
        #[serde(default)]
        scale: Option<f32>,
    },
    Chart {
        kind: ChartKind,
        title: Option<String>,
        /// Pixel height of the chart area. Width is fluid (SVG scales to the
        /// container). Defaults depend on `kind` - see the renderer.
        #[serde(default)]
        height: Option<u32>,
        /// Axis labels. Ignored by `pie`.
        #[serde(default)]
        x_label: Option<String>,
        #[serde(default)]
        y_label: Option<String>,
        /// Bar charts only: lay bars horizontally instead of vertically.
        #[serde(default)]
        orientation: ChartOrientation,
        /// Single-series data. Use for pie slices, or for bar/timeseries
        /// without a second dimension. Mutually exclusive with `series`.
        #[serde(default)]
        data: Option<Vec<ChartPoint>>,
        /// Multi-series data (one extra dimension). For bar → stacked bars.
        /// For timeseries → multi-line. Ignored by pie.
        #[serde(default)]
        series: Option<Vec<ChartSeries>>,
        /// Shrinks the rendered chart to this fraction of the container
        /// width (height follows, since the SVG keeps its aspect ratio),
        /// centered. Clamped to 0.1–2.0. Use when a chart is too tall to
        /// fit on screen at full width.
        #[serde(default)]
        scale: Option<f32>,
    },
    /// Grid of role cards read from the site's `roles:` config in kazam.yaml.
    /// Each card links to `?role=<id>` to activate persona filtering.
    RoleMap {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Sankey {
        title: Option<String>,
        #[serde(default)]
        height: Option<u32>,
        flows: Vec<SankeyFlow>,
        #[serde(default)]
        colors: HashMap<String, SemColor>,
        /// See `Chart.scale`.
        #[serde(default)]
        scale: Option<f32>,
    },
    Radar {
        title: Option<String>,
        #[serde(default)]
        height: Option<u32>,
        axes: Vec<String>,
        curves: Vec<RadarCurve>,
        #[serde(default)]
        max: Option<f64>,
        /// See `Chart.scale`.
        #[serde(default)]
        scale: Option<f32>,
    },
    Quadrant {
        title: Option<String>,
        #[serde(default)]
        height: Option<u32>,
        x_axis: String,
        y_axis: String,
        quadrants: Vec<String>,
        points: Vec<QuadrantPoint>,
        /// See `Chart.scale`.
        #[serde(default)]
        scale: Option<f32>,
    },
    Architecture {
        title: Option<String>,
        #[serde(default)]
        height: Option<u32>,
        #[serde(default)]
        direction: ArchDirection,
        nodes: Vec<ArchNode>,
        connections: Vec<ArchConnection>,
        /// See `Chart.scale`.
        #[serde(default)]
        scale: Option<f32>,
    },
    Pipeline {
        title: Option<String>,
        #[serde(default)]
        height: Option<u32>,
        inputs: Vec<PipelineItem>,
        stages: Vec<PipelineStage>,
        outputs: Vec<PipelineItem>,
        #[serde(default)]
        context: Vec<PipelineItem>,
        /// See `Chart.scale`.
        #[serde(default)]
        scale: Option<f32>,
    },
    Graph {
        title: Option<String>,
        #[serde(default)]
        height: Option<u32>,
        #[serde(default)]
        direction: ArchDirection,
        nodes: Vec<GraphNode>,
        #[serde(default)]
        edges: Vec<GraphEdge>,
        #[serde(default)]
        groups: Vec<GraphGroup>,
        /// Optional label per row, index-aligned to each node's `row`. A tiered
        /// diagram (phases / gates / stall states, one row per tier) reads
        /// this to draw a small heading + dashed rule above each row. Rows
        /// without a matching entry (or a `null`) render with no label.
        #[serde(default)]
        row_labels: Vec<Option<String>>,
        /// See `Chart.scale`.
        #[serde(default)]
        scale: Option<f32>,
    },
    OrgChart {
        title: Option<String>,
        people: Vec<OrgPerson>,
        #[serde(default)]
        default_open_depth: Option<u32>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Aside {
        body: String,
        #[serde(default)]
        scale: Option<f32>,
    },
    RuleList {
        items: Vec<RuleItem>,
        #[serde(default)]
        scale: Option<f32>,
    },
    Gauge {
        items: Vec<GaugeItem>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default = "default_gauge_columns")]
        columns: u32,
        #[serde(default = "default_gauge_max")]
        max: f64,
        #[serde(default)]
        scale: Option<f32>,
    },
}

impl Component {
    pub(crate) fn scale(&self) -> Option<f32> {
        match self {
            Component::Header { scale, .. }
            | Component::HeroBanner { scale, .. }
            | Component::Meta { scale, .. }
            | Component::CardGrid { scale, .. }
            | Component::SelectableGrid { scale, .. }
            | Component::Timeline { scale, .. }
            | Component::StatGrid { scale, .. }
            | Component::BeforeAfter { scale, .. }
            | Component::SplitCompare { scale, .. }
            | Component::Steps { scale, .. }
            | Component::Markdown { scale, .. }
            | Component::Table { scale, .. }
            | Component::Callout { scale, .. }
            | Component::Code { scale, .. }
            | Component::Tabs { scale, .. }
            | Component::Section { scale, .. }
            | Component::Columns { scale, .. }
            | Component::Accordion { scale, .. }
            | Component::EventTimeline { scale, .. }
            | Component::Tree { scale, .. }
            | Component::PriorityQueue { scale, .. }
            | Component::Venn { scale, .. }
            | Component::Image { scale, .. }
            | Component::Embed { scale, .. }
            | Component::Resources { scale, .. }
            | Component::Badge { scale, .. }
            | Component::Tag { scale, .. }
            | Component::Divider { scale, .. }
            | Component::Kbd { scale, .. }
            | Component::Status { scale, .. }
            | Component::Breadcrumb { scale, .. }
            | Component::ButtonGroup { scale, .. }
            | Component::DefinitionList { scale, .. }
            | Component::Blockquote { scale, .. }
            | Component::Avatar { scale, .. }
            | Component::AvatarGroup { scale, .. }
            | Component::ProgressBar { scale, .. }
            | Component::EmptyState { scale, .. }
            | Component::Icon { scale, .. }
            | Component::Chart { scale, .. }
            | Component::RoleMap { scale, .. }
            | Component::Sankey { scale, .. }
            | Component::Radar { scale, .. }
            | Component::Quadrant { scale, .. }
            | Component::Architecture { scale, .. }
            | Component::Pipeline { scale, .. }
            | Component::Graph { scale, .. }
            | Component::OrgChart { scale, .. }
            | Component::Aside { scale, .. }
            | Component::RuleList { scale, .. }
            | Component::Gauge { scale, .. } => *scale,
        }
    }
}

// ── Supporting types ─────────────────────────────────

#[derive(Deserialize)]
pub struct MetaField {
    pub key: String,
    pub value: String,
}

#[derive(Deserialize)]
pub struct Card {
    pub title: String,
    pub badge: Option<Badge>,
    pub description: Option<String>,
    pub links: Option<Vec<Link>>,
    pub href: Option<String>,
    #[serde(default)]
    pub color: SemColor,
}

#[derive(Deserialize)]
pub struct Badge {
    pub label: String,
    #[serde(default)]
    pub color: SemColor,
}

/// Unified semantic color palette used by badge, tag, status, progress_bar,
/// and the stat color accents. Keeps all colored decoration consistent.
#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum SemColor {
    #[default]
    Default,
    Green,
    Yellow,
    Red,
    Teal,
}

impl SemColor {
    pub fn class_suffix(&self) -> &'static str {
        match self {
            SemColor::Default => "default",
            SemColor::Green => "green",
            SemColor::Yellow => "yellow",
            SemColor::Red => "red",
            SemColor::Teal => "teal",
        }
    }

    pub fn hex(&self) -> &'static str {
        match self {
            SemColor::Default => "#3CCECE",
            SemColor::Green => "#34D399",
            SemColor::Yellow => "#FBBF24",
            SemColor::Red => "#F87171",
            SemColor::Teal => "#3CCECE",
        }
    }
}

#[derive(Deserialize)]
pub struct Link {
    pub label: String,
    pub href: String,
}

#[derive(Deserialize)]
pub struct SelectableCard {
    pub title: String,
    pub eyebrow: Option<String>,
    pub bullets: Option<Vec<String>>,
    pub body: Option<String>,
    #[serde(default)]
    pub color: SemColor,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Interaction {
    #[default]
    SingleSelect,
    MultiSelect,
    None,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Connector {
    #[default]
    None,
    DotsLine,
    Arrow,
}

#[derive(Deserialize)]
pub struct TimelineItem {
    pub name: String,
    pub status: TimelineStatus,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum TimelineStatus {
    Completed,
    Active,
    #[default]
    Upcoming,
}

#[derive(Deserialize)]
pub struct Stat {
    pub label: String,
    pub value: String,
    pub detail: Option<String>,
    #[serde(default)]
    pub color: SemColor,
    #[serde(default)]
    pub trend: Option<Trend>,
    #[serde(default)]
    pub previous: Option<String>,
    #[serde(default)]
    pub history: Option<Vec<f64>>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Trend {
    Up,
    Down,
    Flat,
}

impl Trend {
    pub fn class(&self) -> &'static str {
        match self {
            Trend::Up => "trend-up",
            Trend::Down => "trend-down",
            Trend::Flat => "trend-flat",
        }
    }

    pub fn arrow(&self) -> &'static str {
        match self {
            Trend::Up => "↑",
            Trend::Down => "↓",
            Trend::Flat => "→",
        }
    }
}

#[derive(Deserialize)]
pub struct BeforeAfterItem {
    pub title: String,
    pub before: String,
    pub after: String,
    pub after_context: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct ComparePanel {
    #[serde(default)]
    pub eyebrow: Option<String>,
    pub title: String,
    pub stats: Vec<CompareStat>,
}

#[derive(Deserialize, Clone)]
pub struct CompareStat {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub color: SemColor,
}

#[derive(Deserialize)]
pub struct Step {
    pub title: String,
    pub detail: Option<String>,
}

#[derive(Deserialize)]
pub struct TableColumn {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub sortable: bool,
    #[serde(default)]
    pub align: Align,
    #[serde(default)]
    pub color_map: HashMap<String, SemColor>,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Right,
    Center,
}

impl Align {
    pub fn class(&self) -> &'static str {
        match self {
            Align::Left => "align-left",
            Align::Right => "align-right",
            Align::Center => "align-center",
        }
    }
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum CalloutVariant {
    #[default]
    Info,
    Warn,
    Success,
    Danger,
}

impl CalloutVariant {
    pub fn class(&self) -> &'static str {
        match self {
            CalloutVariant::Info => "c-callout-info",
            CalloutVariant::Warn => "c-callout-warn",
            CalloutVariant::Success => "c-callout-success",
            CalloutVariant::Danger => "c-callout-danger",
        }
    }
}

#[derive(Deserialize)]
pub struct Tab {
    pub label: String,
    pub components: Vec<Component>,
}

#[derive(Deserialize)]
pub struct AccordionItem {
    pub title: String,
    pub components: Vec<Component>,
}

#[derive(Deserialize)]
pub struct EventItem {
    pub date: String,
    pub title: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub severity: EventSeverity,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum EventSeverity {
    Major,
    #[default]
    Minor,
    Info,
}

impl EventSeverity {
    pub fn class(&self) -> &'static str {
        match self {
            EventSeverity::Major => "severity-major",
            EventSeverity::Minor => "severity-minor",
            EventSeverity::Info => "severity-info",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            EventSeverity::Major => "major",
            EventSeverity::Minor => "minor",
            EventSeverity::Info => "info",
        }
    }
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum EventFilter {
    #[default]
    All,
    Major,
}

impl EventFilter {
    pub fn class(&self) -> &'static str {
        match self {
            EventFilter::All => "filter-all",
            EventFilter::Major => "filter-major",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            EventFilter::All => "all",
            EventFilter::Major => "major",
        }
    }
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum EventGroupBy {
    #[default]
    Month,
    Quarter,
    Source,
}

#[derive(Deserialize, Clone)]
pub struct TreeNode {
    pub label: String,
    #[serde(default)]
    pub status: TreeStatus,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub children: Vec<TreeNode>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub original_due: Option<String>,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum TreeStatus {
    #[default]
    Default,
    Completed,
    Active,
    Blocked,
    Priority,
    Upcoming,
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TreeFilter {
    #[default]
    All,
    Incomplete,
    Blocked,
    Priority,
    Overdue,
}

#[derive(Deserialize, Serialize, Default, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TreeDefaultView {
    #[default]
    Tree,
    Summary,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum QueueGroup {
    #[default]
    Urgency,
    Horizon,
    Owner,
    Status,
    None,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueueHorizon {
    Now,
    Next,
    Later,
}

#[derive(Deserialize, Clone)]
pub struct QueueTag {
    pub label: String,
    #[serde(default)]
    pub color: SemColor,
    #[serde(default)]
    pub emphasis: bool,
}

#[derive(Deserialize, Clone)]
pub struct QueueItem {
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub original_due: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub status: TreeStatus,
    #[serde(default)]
    pub tags: Vec<QueueTag>,
    #[serde(default)]
    pub href: Option<String>,
    #[serde(default)]
    pub horizon: Option<QueueHorizon>,
}

impl TreeFilter {
    pub fn class(&self) -> &'static str {
        match self {
            TreeFilter::All => "filter-all",
            TreeFilter::Incomplete => "filter-incomplete",
            TreeFilter::Blocked => "filter-blocked",
            TreeFilter::Priority => "filter-priority",
            TreeFilter::Overdue => "filter-overdue",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TreeFilter::All => "all",
            TreeFilter::Incomplete => "incomplete",
            TreeFilter::Blocked => "blocked",
            TreeFilter::Priority => "priority",
            TreeFilter::Overdue => "overdue",
        }
    }
}

impl TreeStatus {
    pub fn class(&self) -> &'static str {
        match self {
            TreeStatus::Default => "status-default",
            TreeStatus::Completed => "status-completed",
            TreeStatus::Active => "status-active",
            TreeStatus::Blocked => "status-blocked",
            TreeStatus::Priority => "status-priority",
            TreeStatus::Upcoming => "status-upcoming",
        }
    }

    pub fn glyph(&self) -> &'static str {
        match self {
            TreeStatus::Default => "•",
            TreeStatus::Completed => "✓",
            TreeStatus::Active => "▸",
            TreeStatus::Blocked => "⚠",
            TreeStatus::Priority => "★",
            TreeStatus::Upcoming => "○",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TreeStatus::Default => "default",
            TreeStatus::Completed => "completed",
            TreeStatus::Active => "active",
            TreeStatus::Blocked => "blocked",
            TreeStatus::Priority => "priority",
            TreeStatus::Upcoming => "upcoming",
        }
    }
}

#[derive(Deserialize)]
pub struct TableSummary {
    pub group_by: String,
    #[serde(default)]
    pub colors: HashMap<String, SemColor>,
}

#[derive(Deserialize)]
pub struct GaugeItem {
    pub label: String,
    pub value: f64,
    #[serde(default)]
    pub color: SemColor,
}

#[derive(Deserialize)]
pub struct RuleItem {
    pub label: String,
    pub body: String,
    #[serde(default)]
    pub color: SemColor,
}

#[derive(Deserialize)]
pub struct VennSet {
    pub label: String,
    #[serde(default)]
    pub color: SemColor,
}

#[derive(Deserialize)]
pub struct VennOverlap {
    /// Indices into `sets[]`. Length 2 or 3.
    pub sets: Vec<usize>,
    #[serde(default)]
    pub label: Option<String>,
}

// ── New component supporting types ───────────────────

#[derive(Deserialize)]
pub struct BreadcrumbItem {
    pub label: String,
    pub href: Option<String>,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Ghost,
}

#[derive(Deserialize)]
pub struct ButtonConfig {
    pub label: String,
    pub href: String,
    #[serde(default)]
    pub variant: ButtonVariant,
    #[serde(default)]
    pub external: bool,
    pub icon: Option<String>,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum IconSize {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

impl IconSize {
    pub fn pixels(&self) -> u32 {
        match self {
            IconSize::Xs => 12,
            IconSize::Sm => 14,
            IconSize::Md => 16,
            IconSize::Lg => 20,
            IconSize::Xl => 24,
        }
    }
}

#[derive(Deserialize)]
pub struct DefinitionItem {
    pub term: String,
    pub definition: String,
}

#[derive(Deserialize)]
pub struct AvatarConfig {
    pub name: String,
    pub src: Option<String>,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum AvatarSize {
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

impl AvatarSize {
    pub fn class_suffix(&self) -> &'static str {
        match self {
            AvatarSize::Sm => "sm",
            AvatarSize::Md => "md",
            AvatarSize::Lg => "lg",
            AvatarSize::Xl => "xl",
        }
    }
}

#[derive(Deserialize)]
pub struct EmptyStateAction {
    pub label: String,
    pub href: String,
}

// ── Chart supporting types ───────────────────────────

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    Pie,
    Bar,
    Timeseries,
}

#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChartOrientation {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Deserialize)]
pub struct ChartPoint {
    pub label: String,
    pub value: f64,
    /// Optional slice/bar tint. Only meaningful for single-series charts -
    /// multi-series charts color by series instead.
    #[serde(default)]
    pub color: Option<SemColor>,
}

#[derive(Deserialize)]
pub struct ChartSeries {
    pub label: String,
    /// Series tint. Defaults cycle through teal → green → yellow → red.
    #[serde(default)]
    pub color: Option<SemColor>,
    pub points: Vec<ChartPoint>,
}

// ── Sankey supporting types ─────────────────────────

#[derive(Deserialize)]
pub struct SankeyFlow {
    pub source: String,
    pub target: String,
    pub value: f64,
}

// ── Radar supporting types ──────────────────────────

#[derive(Deserialize)]
pub struct RadarCurve {
    pub label: String,
    pub values: Vec<f64>,
    #[serde(default)]
    pub color: Option<SemColor>,
}

// ── Quadrant supporting types ───────────────────────

#[derive(Deserialize)]
pub struct QuadrantPoint {
    pub label: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub color: Option<SemColor>,
}

// ── Architecture supporting types ───────────────────

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ArchDirection {
    #[default]
    LeftToRight,
    TopToBottom,
}

#[derive(Deserialize)]
pub struct ArchNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: SemColor,
}

#[derive(Deserialize)]
pub struct ArchConnection {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
}

// ── Pipeline supporting types ───────────────────────

#[derive(Deserialize)]
pub struct PipelineItem {
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub color: SemColor,
    #[serde(default)]
    pub dim: bool,
}

#[derive(Deserialize)]
pub struct PipelineStage {
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<PipelineCapability>,
}

#[derive(Deserialize)]
pub struct PipelineCapability {
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub dim: bool,
}

// ── Graph supporting types ───────────────────────────

#[derive(Deserialize, Default, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GraphShape {
    #[default]
    Box,
    Diamond,
    Pill,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum GraphEdgeStyle {
    #[default]
    Solid,
    Dashed,
}

#[derive(Deserialize, Clone)]
pub struct OrgPerson {
    #[serde(default)]
    #[allow(dead_code)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub color: SemColor,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub linkedin: Option<String>,
    #[serde(default)]
    pub tags: Vec<OrgTag>,
    #[serde(default)]
    pub reports: Vec<OrgPerson>,
}

#[derive(Deserialize, Clone)]
pub struct OrgTag {
    pub label: String,
    #[serde(default)]
    pub color: SemColor,
}

#[derive(Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub color: SemColor,
    /// Exact brand color, e.g. `#2DD4BF`. Wins over `color` when set and
    /// valid (`#RGB`, `#RRGGBB`, or `#RRGGBBAA`); an invalid value silently
    /// falls back to `color` rather than breaking the render.
    #[serde(default)]
    pub hex: Option<String>,
    /// Progress on this node: `completed` or `active` render a small badge
    /// in the box's top-right corner, `upcoming` (the default) renders no
    /// badge at all. Deliberately independent of `color`/`hex` so a node's
    /// role (what kind of thing it is) and its progress stay two separate
    /// signals instead of overloading one.
    #[serde(default)]
    pub status: TimelineStatus,
    #[serde(default)]
    pub shape: GraphShape,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub ports: Vec<PortLabel>,
    /// Pins this node to an explicit row instead of letting the topological
    /// layout compute one from edges. Setting `row` on any node in the graph
    /// switches the whole diagram into tiered/grid mode: every row's nodes
    /// line up on a shared column grid instead of being centered per-row, so
    /// a node in row 1 sits directly above its counterpart in row 2.
    #[serde(default)]
    pub row: Option<u32>,
    /// Explicit column within a row, used only in tiered/grid mode (see
    /// `row`). Nodes sharing a column across rows align vertically, which is
    /// what makes a dashed edge between them read as a straight drop instead
    /// of a diagonal.
    #[serde(default)]
    pub column: Option<u32>,
}

#[derive(Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub style: GraphEdgeStyle,
    #[serde(default)]
    pub color: Option<SemColor>,
}

#[derive(Deserialize)]
pub struct GraphGroup {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub color: Option<SemColor>,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum PortSide {
    #[default]
    Right,
    Left,
    Top,
    Bottom,
}

#[derive(Deserialize)]
pub struct PortLabel {
    pub side: PortSide,
    pub label: String,
}

// ── Site config ──────────────────────────────────────

#[derive(Deserialize)]
pub struct NavLink {
    pub label: String,
    /// Leaf href. Optional only so that a parent grouping entry with `children`
    /// can be a pure label (e.g. "Components ▾" with a dropdown of leaves).
    pub href: Option<String>,
    /// Nested children render as a top-nav dropdown or as nested sidebar
    /// entries depending on `SiteConfig.nav_layout`.
    #[serde(default)]
    pub children: Option<Vec<NavLink>>,
    /// Persona filter. When set, this link is only visible to the listed
    /// roles. Rendered as `data-personas` attributes for client-side
    /// filtering via `?role=` query param.
    #[serde(default)]
    pub personas: Vec<String>,
    /// When true, child subsections render collapsed by default. Users can
    /// click the subsection label to expand. Only meaningful on section-level
    /// entries that have children with their own children.
    #[serde(default)]
    pub collapsed: bool,
}

impl NavLink {
    /// Ensure hrefs are root-relative so sidebar links work from any page depth.
    /// Bare paths like `scanners/wiz.html` become `/scanners/wiz.html`.
    /// Already-absolute (`/…`) and external (`http…`) hrefs are left alone.
    pub fn normalize_hrefs(&mut self) {
        if let Some(ref mut h) = self.href {
            if !h.starts_with('/') && !h.starts_with("http") {
                *h = format!("/{h}");
            }
        }
        if let Some(ref mut kids) = self.children {
            for child in kids.iter_mut() {
                child.normalize_hrefs();
            }
        }
    }
}

/// How the sticky nav is laid out on `shell: standard` pages. Other shells
/// ignore this.
#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NavLayout {
    /// Horizontal top-bar nav (default). Nested entries render as dropdowns.
    #[default]
    Top,
    /// Fixed left-side sidebar. Nested entries render as labeled sections.
    Sidebar,
}

/// Base tone for the site. Only affects rainbow themes (`red`/`orange`/…/
/// `violet`), which pick up the accent color on top of either a dark or
/// light neutral base. `theme: dark` and `theme: light` are self-contained
/// and ignore this field.
#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Dark,
    Light,
}

#[derive(Deserialize)]
pub struct SiteConfig {
    pub name: String,
    pub theme: Option<String>,
    #[serde(default)]
    pub colors: std::collections::HashMap<String, String>,
    pub nav: Option<Vec<NavLink>>,
    pub favicon: Option<Favicon>,
    /// Optional logo image shown in the site bar's brand slot, replacing the
    /// text `name:` treatment. Accepts either a path (shorthand) or an
    /// object with `src`, optional `height`, and optional `alt`. The image's
    /// `src` resolves via the depth-aware rewriter so relative paths work
    /// from any subfolder page.
    #[serde(default)]
    pub logo: Option<Logo>,
    /// Source pill with edit prompt, GitHub link, and source view. On by
    /// default. Set `view_source: false` to opt out.
    #[serde(default)]
    pub view_source: Option<bool>,
    /// Subtle background pattern painted behind every page. Tinted via the
    /// theme's `--text-rgb` so it stays consistent across light/dark.
    /// Defaults to `none`.
    #[serde(default)]
    pub texture: Texture,
    /// Soft accent-colored glow painted behind the page header area.
    /// Defaults to `none`.
    #[serde(default)]
    pub glow: Glow,
    /// Nav layout for `shell: standard` pages. Defaults to `top`.
    #[serde(default)]
    pub nav_layout: NavLayout,
    /// Base tone for rainbow themes - dark (default) or light. Ignored when
    /// `theme:` is already `dark` or `light`.
    #[serde(default)]
    pub mode: Mode,
    /// Fallback `<meta name="description">` and `og:description` used when a
    /// page has no subtitle of its own. Keep it short - one sentence is ideal.
    #[serde(default)]
    pub description: Option<String>,
    /// Canonical base URL for the site, e.g. `https://tdiderich.github.io/kazam`.
    /// When set, each page gets a `<link rel="canonical">` and populated
    /// `og:url` / `twitter:url` meta. Leave unset on sites that don't care
    /// about social unfurls.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub edit_url: Option<String>,
    /// Site-wide social card image (Open Graph + Twitter card). Path is
    /// resolved relative to the site root. 1200×630 PNG is the standard;
    /// SVG works on modern platforms. Optional.
    #[serde(default)]
    pub og_image: Option<String>,
    /// Brand voice rules for consistent content authoring across agents and humans.
    #[serde(default)]
    pub voice: Option<Voice>,
    /// Persona role taxonomy. Defines the roles that pages can be tagged
    /// with via their `personas:` field. Order determines display order in
    /// the role-map component and nav filter.
    #[serde(default)]
    pub roles: Vec<Role>,
    /// Mapping from source-of-truth URL prefixes to local repo paths.
    /// Used by `kazam freshness drift` to check git history.
    #[serde(default)]
    pub drift: Option<DriftConfig>,
}

/// Brand voice configuration - tone, reading level, and terminology preferences.
/// All fields are optional; add what you want. This is config only - kazam does
/// not enforce these rules at build time.
#[derive(Deserialize, Clone, Default)]
pub struct Voice {
    /// Tone description, e.g. "direct, technical, no marketing fluff"
    #[serde(default)]
    pub tone: Option<String>,
    /// Target reading level, e.g. "senior engineer", "general audience"
    #[serde(default)]
    pub reading_level: Option<String>,
    /// Terminology preferences
    #[serde(default)]
    pub terminology: Option<Terminology>,
}

/// Preferred and avoided terms for consistent language across content authors.
#[derive(Deserialize, Clone, Default)]
pub struct Terminology {
    /// Preferred term replacements: key = avoid, value = use instead
    #[serde(default)]
    pub prefer: std::collections::HashMap<String, String>,
    /// Terms to avoid entirely
    #[serde(default)]
    pub avoid: Vec<String>,
}

/// Mapping from source-of-truth URL prefixes to local repo paths.
/// Used by `kazam freshness drift` to check git history.
#[derive(Deserialize, Clone, Default)]
pub struct DriftConfig {
    #[serde(default)]
    pub repos: Vec<DriftRepo>,
}

/// One repo mapping entry for drift detection.
#[derive(Deserialize, Clone)]
pub struct DriftRepo {
    /// URL prefix to match against sources_of_truth hrefs
    pub prefix: String,
    /// Local filesystem path to the git repo
    pub local: String,
}

/// One role in the site's persona taxonomy.
#[derive(Deserialize, Clone)]
pub struct Role {
    /// Machine identifier, matches values in page `personas:` fields.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    /// Landing page for this role. Rendered as the card href in the role_map
    /// component. When unset, the card links to `?role=<id>`.
    #[serde(default)]
    pub href: Option<String>,
}

/// One item in a `resources` component.
#[derive(Deserialize)]
pub struct ResourceItem {
    pub title: String,
    pub href: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
}

/// Site-wide background pattern. All variants are subtle by design.
#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Texture {
    #[default]
    None,
    /// 1px dots on a 24px grid.
    Dots,
    /// Thin gridlines on a 40px grid.
    Grid,
    /// SVG fractal-noise grain.
    Grain,
    /// Wavy contour-line topography.
    Topography,
    /// 45° diagonal stripes.
    Diagonal,
}

/// Soft accent-tinted radial gradient. Sits above the texture, below content.
#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Glow {
    #[default]
    None,
    /// Wide soft glow centered above the fold.
    Accent,
    /// Tighter glow tucked into the top-right corner.
    Corner,
}

/// Logo image for the site-bar brand slot. Accepts either a shorthand
/// string (a path to the image) or an object with `src`, optional
/// `height` (px - upper bound on rendered height; defaults to the
/// site-bar content height), and optional `alt` (defaults to the site
/// `name`).
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Logo {
    Simple(String),
    Full {
        src: String,
        #[serde(default)]
        height: Option<u32>,
        #[serde(default)]
        alt: Option<String>,
    },
}

impl Logo {
    pub fn src(&self) -> &str {
        match self {
            Logo::Simple(p) => p,
            Logo::Full { src, .. } => src,
        }
    }
    pub fn height(&self) -> Option<u32> {
        match self {
            Logo::Simple(_) => None,
            Logo::Full { height, .. } => *height,
        }
    }
    pub fn alt<'a>(&'a self, site_name: &'a str) -> &'a str {
        match self {
            Logo::Simple(_) => site_name,
            Logo::Full { alt, .. } => alt.as_deref().unwrap_or(site_name),
        }
    }
}

/// Favicon config: either a single path, or a struct with named slots.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Favicon {
    Simple(String),
    Full {
        svg: Option<String>,
        png: Option<String>,
        ico: Option<String>,
        apple_touch_icon: Option<String>,
    },
}

impl Favicon {
    /// Render <link> tags (already resolved against base path).
    pub fn render(&self, base: &str) -> String {
        let resolve = |p: &str| crate::render::resolve_href(p, base);
        match self {
            Favicon::Simple(path) => {
                let mime = mime_for(path);
                format!(
                    r#"<link rel="icon" type="{}" href="{}">"#,
                    mime,
                    resolve(path)
                )
            }
            Favicon::Full {
                svg,
                png,
                ico,
                apple_touch_icon,
            } => {
                let mut out = String::new();
                if let Some(p) = svg {
                    out.push_str(&format!(
                        r#"<link rel="icon" type="image/svg+xml" href="{}">"#,
                        resolve(p)
                    ));
                }
                if let Some(p) = png {
                    out.push_str(&format!(
                        r#"<link rel="icon" type="image/png" href="{}">"#,
                        resolve(p)
                    ));
                }
                if let Some(p) = ico {
                    out.push_str(&format!(
                        r#"<link rel="icon" type="image/x-icon" href="{}">"#,
                        resolve(p)
                    ));
                }
                if let Some(p) = apple_touch_icon {
                    out.push_str(&format!(
                        r#"<link rel="apple-touch-icon" href="{}">"#,
                        resolve(p)
                    ));
                }
                out
            }
        }
    }
}

fn mime_for(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "image/png"
    }
}

impl SiteConfig {
    pub fn resolved_theme(&self) -> crate::theme::Theme {
        let base = self.theme.as_deref().unwrap_or("dark");
        crate::theme::Theme::named(base, self.mode).with_overrides(&self.colors)
    }
}

impl Default for SiteConfig {
    fn default() -> Self {
        SiteConfig {
            name: String::from("My Site"),
            theme: None,
            colors: std::collections::HashMap::new(),
            nav: None,
            favicon: None,
            logo: None,
            view_source: None,
            texture: Texture::None,
            glow: Glow::None,
            nav_layout: NavLayout::Top,
            mode: Mode::Dark,
            description: None,
            url: None,
            edit_url: None,
            og_image: None,
            voice: None,
            roles: Vec::new(),
            drift: None,
        }
    }
}

// ── Defaults ─────────────────────────────────────────

fn default_color() -> String {
    "default".to_string()
}
fn default_stat_columns() -> u32 {
    3
}
fn default_true() -> bool {
    true
}
fn default_avatar_max() -> usize {
    5
}
fn default_gauge_columns() -> u32 {
    3
}
fn default_gauge_max() -> f64 {
    100.0
}

// ── Annotations ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub text: String,
    pub author: String,
    #[serde(default)]
    pub section: Option<String>,
    pub added: String,
    #[serde(default)]
    pub status: AnnotationStatus,
    #[serde(default)]
    pub source: AnnotationSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationStatus {
    #[default]
    Pending,
    Incorporated,
    Ignored,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationSource {
    #[default]
    Cli,
    Agent,
    Web,
}

pub fn value_to_string(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::Null => String::new(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Sequence(_) | serde_yaml::Value::Mapping(_) => {
            serde_yaml::to_string(v).unwrap_or_default()
        }
        serde_yaml::Value::Tagged(t) => value_to_string(&t.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_to_string_handles_all_scalar_types() {
        use serde_yaml::Value;
        assert_eq!(value_to_string(&Value::Null), "");
        assert_eq!(value_to_string(&Value::Bool(true)), "true");
        assert_eq!(value_to_string(&Value::Number(42.into())), "42");
        assert_eq!(value_to_string(&Value::String("hi".into())), "hi");
    }

    #[test]
    fn sem_color_class_suffix() {
        assert_eq!(SemColor::Default.class_suffix(), "default");
        assert_eq!(SemColor::Green.class_suffix(), "green");
        assert_eq!(SemColor::Yellow.class_suffix(), "yellow");
        assert_eq!(SemColor::Red.class_suffix(), "red");
        assert_eq!(SemColor::Teal.class_suffix(), "teal");
    }

    #[test]
    fn sem_color_hex_values() {
        assert_eq!(SemColor::Green.hex(), "#34D399");
        assert_eq!(SemColor::Red.hex(), "#F87171");
        assert_eq!(SemColor::Default.hex(), "#3CCECE");
    }
}

use serde::Deserialize;
use std::collections::HashMap;

// ── Shell ────────────────────────────────────────────

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Shell {
    Standard,
    Document,
    Deck,
}

impl Shell {
    pub fn class(&self) -> &'static str {
        match self {
            Shell::Standard => "shell-standard",
            Shell::Document => "shell-document",
            Shell::Deck => "shell-deck",
        }
    }
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
    /// scrolling document with a thin separator between them — nicer for
    /// sharing as a readable artifact rather than a presentation. `square`:
    /// one slide per square page, sized for LinkedIn-style document carousels
    /// where the viewport is near-square and landscape PDFs letterbox badly.
    #[serde(default)]
    pub print_flow: Option<PrintFlow>,
    /// Extra search keywords that don't appear in rendered content.
    /// Useful for aliases, acronyms, internal jargon.
    #[serde(default)]
    pub search_terms: Vec<String>,
    /// Optional freshness metadata: owner, last content update, review cadence,
    /// and sources of truth the agent / reader can consult to refresh the
    /// page. When the page is past its review window, a banner is injected
    /// at the top of the rendered output and the build reports the page as
    /// stale. Zero runtime JS — staleness is computed at `kazam build` time.
    /// Set to `"never"` to explicitly opt out of freshness checks with no
    /// warning emitted.
    #[serde(default)]
    pub freshness: Option<FreshnessValue>,
    /// Who is responsible for this page. Free-form string — email, Slack
    /// handle, or team name. Serves as a fallback for `freshness.owner` in
    /// the stale-page report when no freshness block is present.
    #[serde(default)]
    pub owner: Option<String>,
    /// Links to sources of truth that inform this page's content.
    /// Each entry has a URL and an optional note explaining what it references.
    #[serde(default)]
    pub references: Vec<Reference>,
}

/// Freshness value: either the bare string `"never"` (explicit opt-out —
/// no decay checks, no warning) or a full metadata struct.
#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub enum FreshnessValue {
    /// Bare string `"never"` — page explicitly opts out of freshness checks.
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

/// Freshness metadata for a page — when was it last updated, who owns it,
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
    /// Who should be contacted before changes land. Free-form — email,
    /// Slack handle, or team name.
    pub owner: Option<String>,
    /// Pointers the agent / reader should consult to refresh the content.
    /// Shorthand form is a bare URL string; expanded form accepts a label
    /// alongside the href.
    #[serde(default)]
    pub sources_of_truth: Option<Vec<SourceOfTruth>>,
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
    /// Hide the top-left "OVERVIEW"-style label on this slide. Typically used
    /// for a cover/title slide where the centered title is the only text.
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
    },
    Meta {
        fields: Vec<MetaField>,
    },
    CardGrid {
        cards: Vec<Card>,
        #[serde(default)]
        min_width: Option<u32>,
        #[serde(default)]
        connector: Connector,
    },
    SelectableGrid {
        cards: Vec<SelectableCard>,
        #[serde(default)]
        interaction: Interaction,
        #[serde(default)]
        connector: Connector,
    },
    Timeline {
        items: Vec<TimelineItem>,
    },
    StatGrid {
        stats: Vec<Stat>,
        #[serde(default = "default_stat_columns")]
        columns: u32,
    },
    BeforeAfter {
        items: Vec<BeforeAfterItem>,
    },
    Steps {
        items: Vec<Step>,
        #[serde(default = "default_true")]
        numbered: bool,
    },
    Markdown {
        body: String,
    },
    Table {
        columns: Vec<TableColumn>,
        rows: Vec<HashMap<String, serde_yaml::Value>>,
        #[serde(default)]
        filterable: bool,
    },
    Callout {
        #[serde(default)]
        variant: CalloutVariant,
        title: Option<String>,
        body: String,
        links: Option<Vec<ButtonConfig>>,
    },
    Code {
        language: Option<String>,
        code: String,
    },
    Tabs {
        tabs: Vec<Tab>,
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
    },
    Columns {
        columns: Vec<Vec<Component>>,
        #[serde(default)]
        equal_heights: bool,
    },
    Accordion {
        items: Vec<AccordionItem>,
    },
    EventTimeline {
        events: Vec<EventItem>,
        #[serde(default)]
        default_filter: EventFilter,
        #[serde(default)]
        show_filter_toggle: bool,
        #[serde(default)]
        limit: Option<u32>,
    },
    Tree {
        nodes: Vec<TreeNode>,
        #[serde(default)]
        default_filter: TreeFilter,
        #[serde(default)]
        show_filter_toggle: bool,
        #[serde(default)]
        default_collapsed: bool,
    },
    Venn {
        sets: Vec<VennSet>,
        #[serde(default)]
        overlaps: Vec<VennOverlap>,
        #[serde(default)]
        title: Option<String>,
    },
    Image {
        src: String,
        alt: Option<String>,
        caption: Option<String>,
        max_width: Option<u32>,
        #[serde(default)]
        align: Align,
    },
    Badge {
        label: String,
        #[serde(default)]
        color: SemColor,
    },
    Tag {
        label: String,
        #[serde(default)]
        color: SemColor,
    },
    Divider {
        label: Option<String>,
    },
    Kbd {
        keys: Vec<String>,
    },
    Status {
        label: String,
        #[serde(default)]
        color: SemColor,
    },
    Breadcrumb {
        items: Vec<BreadcrumbItem>,
    },
    ButtonGroup {
        buttons: Vec<ButtonConfig>,
    },
    DefinitionList {
        items: Vec<DefinitionItem>,
    },
    Blockquote {
        body: String,
        attribution: Option<String>,
    },
    Avatar {
        name: String,
        src: Option<String>,
        #[serde(default)]
        size: AvatarSize,
        subtitle: Option<String>,
    },
    AvatarGroup {
        avatars: Vec<AvatarConfig>,
        #[serde(default)]
        size: AvatarSize,
        #[serde(default = "default_avatar_max")]
        max: usize,
    },
    ProgressBar {
        value: u8,
        label: Option<String>,
        #[serde(default)]
        color: SemColor,
        detail: Option<String>,
    },
    EmptyState {
        title: String,
        body: Option<String>,
        action: Option<EmptyStateAction>,
        #[serde(default)]
        icon: Option<String>,
    },
    Icon {
        name: String,
        #[serde(default)]
        size: IconSize,
        #[serde(default)]
        color: SemColor,
    },
    Chart {
        kind: ChartKind,
        title: Option<String>,
        /// Pixel height of the chart area. Width is fluid (SVG scales to the
        /// container). Defaults depend on `kind` — see the renderer.
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
    },
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

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum TimelineStatus {
    Completed,
    Active,
    Upcoming,
}

#[derive(Deserialize)]
pub struct Stat {
    pub label: String,
    pub value: String,
    pub detail: Option<String>,
    #[serde(default)]
    pub color: SemColor,
}

#[derive(Deserialize)]
pub struct BeforeAfterItem {
    pub title: String,
    pub before: String,
    pub after: String,
    pub after_context: Option<String>,
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

#[derive(Deserialize, Clone)]
pub struct TreeNode {
    pub label: String,
    #[serde(default)]
    pub status: TreeStatus,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub children: Vec<TreeNode>,
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
}

impl TreeFilter {
    pub fn class(&self) -> &'static str {
        match self {
            TreeFilter::All => "filter-all",
            TreeFilter::Incomplete => "filter-incomplete",
            TreeFilter::Blocked => "filter-blocked",
            TreeFilter::Priority => "filter-priority",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TreeFilter::All => "all",
            TreeFilter::Incomplete => "incomplete",
            TreeFilter::Blocked => "blocked",
            TreeFilter::Priority => "priority",
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
    /// Optional slice/bar tint. Only meaningful for single-series charts —
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
#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
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
    /// When true, each page gets a companion `*.source.html` rendering of its
    /// YAML source, and a "View source" pill links to it. Off by default —
    /// useful for docs/examples sites, noise for most end-user sites.
    #[serde(default)]
    pub view_source: bool,
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
    /// Base tone for rainbow themes — dark (default) or light. Ignored when
    /// `theme:` is already `dark` or `light`.
    #[serde(default)]
    pub mode: Mode,
    /// Fallback `<meta name="description">` and `og:description` used when a
    /// page has no subtitle of its own. Keep it short — one sentence is ideal.
    #[serde(default)]
    pub description: Option<String>,
    /// Canonical base URL for the site, e.g. `https://tdiderich.github.io/kazam`.
    /// When set, each page gets a `<link rel="canonical">` and populated
    /// `og:url` / `twitter:url` meta. Leave unset on sites that don't care
    /// about social unfurls.
    #[serde(default)]
    pub url: Option<String>,
    /// Site-wide social card image (Open Graph + Twitter card). Path is
    /// resolved relative to the site root. 1200×630 PNG is the standard;
    /// SVG works on modern platforms. Optional.
    #[serde(default)]
    pub og_image: Option<String>,
    /// Brand voice rules for consistent content authoring across agents and humans.
    #[serde(default)]
    pub voice: Option<Voice>,
}

/// Brand voice configuration — tone, reading level, and terminology preferences.
/// All fields are optional; add what you want. This is config only — kazam does
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

/// Site-wide background pattern. All variants are subtle by design.
#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
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
#[derive(Deserialize, Default, Clone, Copy, PartialEq, Eq)]
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
/// `height` (px — upper bound on rendered height; defaults to the
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
            view_source: false,
            texture: Texture::None,
            glow: Glow::None,
            nav_layout: NavLayout::Top,
            mode: Mode::Dark,
            description: None,
            url: None,
            og_image: None,
            voice: None,
        }
    }
}

// ── Defaults ─────────────────────────────────────────

fn default_stat_columns() -> u32 {
    3
}
fn default_true() -> bool {
    true
}
fn default_avatar_max() -> usize {
    5
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

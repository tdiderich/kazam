use pulldown_cmark::{html as md_html, Event, Options, Parser as MdParser, Tag};

use super::{charts, esc, resolve_href, slug, Rendered};
use crate::icons;
use crate::types::*;

pub fn render(c: &Component, base: &str, config: &SiteConfig) -> Rendered {
    match c {
        Component::Header {
            title,
            subtitle,
            eyebrow,
            align,
            id,
        } => header(title, subtitle, eyebrow, *align, id.as_deref()),
        Component::HeroBanner {
            title,
            eyebrow,
            subtitle,
            buttons,
        } => hero_banner(title, eyebrow, subtitle, buttons.as_deref(), base),
        Component::Meta { fields } => meta(fields),
        Component::CardGrid {
            cards,
            min_width,
            connector,
        } => card_grid(cards, *min_width, *connector, base),
        Component::SelectableGrid {
            cards,
            interaction,
            connector,
        } => selectable_grid(cards, *interaction, *connector, base),
        Component::Timeline { items } => timeline(items),
        Component::StatGrid { stats, columns } => stat_grid(stats, *columns),
        Component::BeforeAfter {
            items,
            before_label,
            after_label,
        } => before_after(items, before_label.as_deref(), after_label.as_deref()),
        Component::SplitCompare { left, right } => split_compare(left, right),
        Component::Steps { items, numbered } => steps(items, *numbered),
        Component::Markdown { body } => markdown(body, base),
        Component::Table {
            columns,
            rows,
            filterable,
        } => table(columns, rows, *filterable),
        Component::Callout {
            variant,
            title,
            body,
            links,
        } => callout(*variant, title, body, links.as_deref(), base),
        Component::Code { language, code } => code_block(language, code),
        Component::Tabs { tabs } => tabs_component(tabs, base, config),
        Component::Section {
            heading,
            eyebrow,
            components,
            align,
            id,
        } => section(
            heading,
            eyebrow,
            components,
            *align,
            id.as_deref(),
            base,
            config,
        ),
        Component::Columns {
            columns,
            equal_heights,
        } => columns_component(columns, *equal_heights, base, config),
        Component::Accordion { items } => accordion(items, base, config),
        Component::EventTimeline {
            events,
            default_filter,
            show_filter_toggle,
            limit,
        } => event_timeline(events, *default_filter, *show_filter_toggle, *limit, base),
        Component::Tree {
            nodes,
            default_filter,
            show_filter_toggle,
            default_collapsed,
        } => tree(
            nodes,
            *default_filter,
            *show_filter_toggle,
            *default_collapsed,
        ),
        Component::Venn {
            sets,
            overlaps,
            title,
        } => venn(sets, overlaps, title.as_deref()),
        Component::Image {
            src,
            alt,
            caption,
            max_width,
            align,
        } => image(src, alt, caption, *max_width, *align, base),
        Component::Embed { src, title, aspect } => embed(src, title, aspect),
        Component::Resources { items } => resources(items),
        // Phase 1 additions
        Component::Badge { label, color } => badge(label, *color),
        Component::Tag { label, color } => tag(label, *color),
        Component::Divider { label } => divider(label),
        Component::Kbd { keys } => kbd(keys),
        Component::Status { label, color } => status(label, *color),
        Component::Breadcrumb { items } => breadcrumb(items, base),
        Component::ButtonGroup { buttons } => button_group(buttons, base),
        Component::DefinitionList { items } => definition_list(items),
        Component::Blockquote { body, attribution } => blockquote(body, attribution),
        Component::Avatar {
            name,
            src,
            size,
            subtitle,
        } => avatar(name, src, *size, subtitle),
        Component::AvatarGroup { avatars, size, max } => avatar_group(avatars, *size, *max),
        Component::ProgressBar {
            value,
            label,
            color,
            detail,
        } => progress_bar(*value, label, *color, detail),
        Component::EmptyState {
            title,
            body,
            action,
            icon,
        } => empty_state(title, body, action, icon, base),
        Component::Icon { name, size, color } => icon_component(name, *size, *color),
        Component::Chart {
            kind,
            title,
            height,
            x_label,
            y_label,
            orientation,
            data,
            series,
        } => charts::render(charts::ChartSpec {
            kind: *kind,
            title,
            height: *height,
            x_label,
            y_label,
            orientation: *orientation,
            data,
            series,
        }),
        Component::RoleMap { title } => role_map(title.as_deref(), config, base),
        Component::Sankey {
            title,
            height,
            flows,
            colors,
        } => charts::render_sankey(title, *height, flows, colors),
        Component::Radar {
            title,
            height,
            axes,
            curves,
            max,
        } => charts::render_radar(title, *height, axes, curves, *max),
        Component::Quadrant {
            title,
            height,
            x_axis,
            y_axis,
            quadrants,
            points,
        } => charts::render_quadrant(title, *height, x_axis, y_axis, quadrants, points),
        Component::Architecture {
            title,
            height,
            direction,
            nodes,
            connections,
        } => charts::render_architecture(title, *height, *direction, nodes, connections),
        Component::Pipeline {
            title,
            height,
            inputs,
            stages,
            outputs,
            context,
        } => charts::render_pipeline(title, *height, inputs, stages, outputs, context),
        Component::Graph {
            title,
            height,
            direction,
            nodes,
            edges,
            groups,
        } => charts::render_graph(title, *height, *direction, nodes, edges, groups),
    }
}

pub(super) fn sem_color_class(c: SemColor) -> &'static str {
    c.class_suffix()
}

// ── Header ────────────────────────────────────────

fn header(
    title: &str,
    subtitle: &Option<String>,
    eyebrow: &Option<String>,
    align: Align,
    id: Option<&str>,
) -> Rendered {
    let id_attr = match slug::resolve(id, Some(title)) {
        Some(slug) => format!(r#" id="{}""#, slug),
        None => String::new(),
    };
    let mut h = format!(r#"<div{} class="c-header {}">"#, id_attr, align.class());
    if let Some(e) = eyebrow {
        h.push_str(&format!(
            r#"<div class="c-header-eyebrow">{}</div>"#,
            esc(e)
        ));
    }
    h.push_str(&format!(
        r#"<h1 class="c-header-title">{}</h1>"#,
        esc(title)
    ));
    if let Some(s) = subtitle {
        h.push_str(&format!(r#"<p class="c-header-subtitle">{}</p>"#, esc(s)));
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Hero Banner ───────────────────────────────────

fn hero_banner(
    title: &str,
    eyebrow: &Option<String>,
    subtitle: &Option<String>,
    buttons: Option<&[ButtonConfig]>,
    base: &str,
) -> Rendered {
    let mut h = String::from(r#"<div class="c-hero">"#);
    if let Some(e) = eyebrow {
        h.push_str(&format!(r#"<div class="c-hero-eyebrow">{}</div>"#, esc(e)));
    }
    h.push_str(&format!(r#"<h1 class="c-hero-title">{}</h1>"#, esc(title)));
    if let Some(s) = subtitle {
        h.push_str(&format!(r#"<p class="c-hero-subtitle">{}</p>"#, esc(s)));
    }
    if let Some(btns) = buttons {
        if !btns.is_empty() {
            let inner = button_group(btns, base);
            h.push_str(&format!(
                r#"<div class="c-hero-buttons">{}</div>"#,
                inner.html
            ));
        }
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Meta ──────────────────────────────────────────

fn meta(fields: &[MetaField]) -> Rendered {
    let mut h = String::from(r#"<div class="c-meta">"#);
    for f in fields {
        h.push_str(&format!(
            r#"<div class="c-meta-item"><span class="c-meta-key">{}</span><span class="c-meta-value">{}</span></div>"#,
            esc(&f.key), esc(&f.value)
        ));
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Card Grid ─────────────────────────────────────

fn card_grid(cards: &[Card], min_width: Option<u32>, connector: Connector, base: &str) -> Rendered {
    let mw = min_width.unwrap_or(320);
    let is_arrow = matches!(connector, Connector::Arrow);
    let mut h = if is_arrow {
        String::from(r#"<div class="c-card-grid c-card-grid-arrow">"#)
    } else {
        format!(r#"<div class="c-card-grid" style="--card-min: {mw}px">"#)
    };
    for (i, card) in cards.iter().enumerate() {
        if is_arrow && i > 0 {
            h.push_str(r#"<div class="c-card-arrow" aria-hidden="true">→</div>"#);
        }
        let tag = if card.href.is_some() { "a" } else { "div" };
        let href_attr = card
            .href
            .as_ref()
            .map(|h| format!(r#" href="{}""#, esc(&resolve_href(h, base))))
            .unwrap_or_default();
        h.push_str(&format!(
            r#"<{tag} class="c-card c-card-{color}"{href_attr}>"#,
            color = sem_color_class(card.color),
        ));
        h.push_str(r#"<div class="c-card-top">"#);
        h.push_str(&format!(
            r#"<h2 class="c-card-title">{}</h2>"#,
            esc(&card.title)
        ));
        if let Some(b) = &card.badge {
            h.push_str(&format!(
                r#"<span class="c-badge c-badge-{color}">{label}</span>"#,
                color = sem_color_class(b.color),
                label = esc(&b.label)
            ));
        }
        h.push_str("</div>");
        if let Some(d) = &card.description {
            h.push_str(&format!(r#"<p class="c-card-desc">{}</p>"#, esc(d)));
        }
        if let Some(links) = &card.links {
            h.push_str(r#"<div class="c-card-links">"#);
            for l in links {
                h.push_str(&format!(
                    r#"<a href="{}" class="c-card-link">{}</a>"#,
                    esc(&resolve_href(&l.href, base)),
                    esc(&l.label)
                ));
            }
            h.push_str("</div>");
        }
        h.push_str(&format!("</{tag}>"));
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Selectable Grid ───────────────────────────────

fn selectable_grid(
    cards: &[SelectableCard],
    interaction: Interaction,
    connector: Connector,
    base: &str,
) -> Rendered {
    let interaction_attr = match interaction {
        Interaction::SingleSelect => "single_select",
        Interaction::MultiSelect => "multi_select",
        Interaction::None => "none",
    };
    let is_arrow = matches!(connector, Connector::Arrow);

    let mut h = format!(
        r#"<div class="c-selectable-grid" data-selectable-grid data-interaction="{interaction_attr}">"#
    );

    if matches!(connector, Connector::DotsLine) {
        h.push_str(r#"<div class="c-sel-dots-row"><div class="c-sel-dots-line"></div>"#);
        for (i, _) in cards.iter().enumerate() {
            let n = i + 1;
            h.push_str(&format!(
                r#"<button class="sel-dot" data-n="{n}">{n}</button>"#
            ));
        }
        h.push_str("</div>");
    }

    if is_arrow {
        h.push_str(r#"<div class="c-sel-cards c-sel-cards-arrow">"#);
    } else {
        h.push_str(&format!(
            r#"<div class="c-sel-cards" style="--sel-cols: {}">"#,
            cards.len().max(1)
        ));
    }
    for (i, card) in cards.iter().enumerate() {
        if is_arrow && i > 0 {
            h.push_str(r#"<div class="c-card-arrow" aria-hidden="true">→</div>"#);
        }
        let n = i + 1;
        h.push_str(&format!(
            r#"<button class="sel-card sel-card-{color}" data-n="{n}" aria-pressed="false">"#,
            color = sem_color_class(card.color),
        ));
        let eyebrow = card
            .eyebrow
            .as_deref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Item {n}"));
        h.push_str(&format!(
            r#"<div class="c-sel-eyebrow">{}</div>"#,
            esc(&eyebrow)
        ));
        h.push_str(&format!(
            r#"<div class="c-sel-title">{}</div>"#,
            esc(&card.title)
        ));
        if let Some(bullets) = &card.bullets {
            h.push_str(r#"<ul class="c-sel-bullets">"#);
            for b in bullets {
                h.push_str(&format!(
                    r#"<li><span class="c-sel-bullet-dot"></span><span>{}</span></li>"#,
                    esc(b)
                ));
            }
            h.push_str("</ul>");
        }
        if let Some(body) = &card.body {
            h.push_str(&format!(
                r#"<div class="c-sel-body">{}</div>"#,
                parse_markdown(body, base)
            ));
        }
        h.push_str("</button>");
    }
    h.push_str("</div></div>");
    Rendered::new(h).with_script("selectable_grid")
}

// ── Timeline ──────────────────────────────────────

fn timeline(items: &[TimelineItem]) -> Rendered {
    let mut h = String::from(r#"<div class="c-timeline">"#);
    for item in items {
        let cls = match item.status {
            TimelineStatus::Completed => "completed",
            TimelineStatus::Active => "active",
            TimelineStatus::Upcoming => "upcoming",
        };
        h.push_str(&format!(
            r#"<div class="c-timeline-phase {cls}"><div class="c-timeline-dot"></div><div class="c-timeline-label">{name}</div><div class="c-timeline-bar {cls}"></div></div>"#,
            cls = cls, name = esc(&item.name)
        ));
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Stat Grid ─────────────────────────────────────

fn stat_grid(stats: &[Stat], columns: u32) -> Rendered {
    let mut h = format!(r#"<div class="c-stat-grid" style="--stat-cols: {columns}">"#);
    for s in stats {
        h.push_str(&format!(
            r#"<div class="c-stat" style="--stat-color: {color}"><div class="c-stat-label">{label}</div><div class="c-stat-value">{value}</div>"#,
            color = s.color.hex(),
            label = esc(&s.label),
            value = esc(&s.value),
        ));
        if let Some(d) = &s.detail {
            h.push_str(&format!(r#"<div class="c-stat-detail">{}</div>"#, esc(d)));
        }
        h.push_str("</div>");
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Before / After ────────────────────────────────

fn before_after(
    items: &[BeforeAfterItem],
    before_label: Option<&str>,
    after_label: Option<&str>,
) -> Rendered {
    let bl = before_label.unwrap_or("Before");
    let al = after_label.unwrap_or("Now");
    let mut h = String::from(r#"<div class="c-before-after">"#);
    for item in items {
        let ctx = item.after_context.as_deref().unwrap_or("");
        let ctx_span = if ctx.is_empty() {
            String::new()
        } else {
            format!(" {}", esc(ctx))
        };
        h.push_str(&format!(
            r#"<div class="c-ba-card">
  <div class="c-ba-title">{title}</div>
  <div class="c-ba-before">{bl}: {before}</div>
  <div class="c-ba-after">{al}: <span class="c-ba-highlight">{after}</span>{ctx}</div>
</div>"#,
            title = esc(&item.title),
            bl = esc(bl),
            before = parse_markdown_inline(&item.before),
            al = esc(al),
            after = parse_markdown_inline(&item.after),
            ctx = ctx_span,
        ));
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Split Compare ─────────────────────────────────

fn split_compare(left: &ComparePanel, right: &ComparePanel) -> Rendered {
    let row_span = 2 + left.stats.len().max(right.stats.len());
    let mut h = String::from(r#"<div class="c-split-compare">"#);
    for (panel, side) in [(left, "left"), (right, "right")] {
        h.push_str(&format!(
            r#"<div class="c-sc-panel c-sc-{}" style="--sc-span: {}">"#,
            side, row_span
        ));
        // Always render eyebrow for subgrid row alignment
        if let Some(ey) = &panel.eyebrow {
            h.push_str(&format!(r#"<div class="c-sc-eyebrow">{}</div>"#, esc(ey)));
        } else {
            h.push_str(r#"<div class="c-sc-eyebrow"></div>"#);
        }
        h.push_str(&format!(
            r#"<div class="c-sc-title">{}</div>"#,
            esc(&panel.title)
        ));
        for stat in &panel.stats {
            let color_class = sem_color_class(stat.color);
            h.push_str(&format!(
                r#"<div class="c-sc-stat"><span class="c-sc-label">{}</span><span class="c-sc-value color-{}">{}</span></div>"#,
                esc(&stat.label),
                color_class,
                esc(&stat.value),
            ));
        }
        h.push_str("</div>");
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Steps ─────────────────────────────────────────

fn steps(items: &[Step], numbered: bool) -> Rendered {
    let tag = if numbered { "ol" } else { "ul" };
    let mut h = format!(r#"<{tag} class="c-steps">"#);
    for (i, s) in items.iter().enumerate() {
        h.push_str(r#"<li class="c-step">"#);
        if numbered {
            h.push_str(&format!(r#"<div class="c-step-num">{}</div>"#, i + 1));
        } else {
            h.push_str(r#"<div class="c-step-bullet"></div>"#);
        }
        h.push_str(&format!(
            r#"<div><div class="c-step-title">{}</div>"#,
            esc(&s.title)
        ));
        if let Some(d) = &s.detail {
            h.push_str(&format!(r#"<div class="c-step-detail">{}</div>"#, esc(d)));
        }
        h.push_str("</div></li>");
    }
    h.push_str(&format!("</{tag}>"));
    Rendered::new(h)
}

// ── Markdown ──────────────────────────────────────

fn markdown(body: &str, base: &str) -> Rendered {
    Rendered::new(format!(
        r#"<div class="c-markdown">{}</div>"#,
        parse_markdown(body, base)
    ))
}

pub(super) fn parse_markdown(md: &str, base: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = MdParser::new_ext(md, opts);
    // Rewrite link destinations through resolve_href so relative links get
    // the depth-aware base prefix and absolute/protocol hrefs pass through.
    let events = parser.map(|event| match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let resolved = resolve_href(&dest_url, base);
            Event::Start(Tag::Link {
                link_type,
                dest_url: resolved.into(),
                title,
                id,
            })
        }
        other => other,
    });
    let mut html = String::new();
    md_html::push_html(&mut html, events);
    html
}

/// Parse a short string as markdown and strip the outer `<p>...</p>` wrapping
/// so the result can be embedded inline inside another element. Falls back to
/// the full HTML if the input spans multiple blocks.
pub(super) fn parse_markdown_inline(md: &str) -> String {
    let html = parse_markdown(md, "");
    let trimmed = html.trim_end_matches('\n');
    if let Some(inner) = trimmed
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>"))
    {
        inner.to_string()
    } else {
        html
    }
}

/// Render a table cell: HTML-escape the raw value, then linkify any
/// `[text](url)` spans. Only `http(s)://`, `mailto:`, and path-like relative
/// URLs are accepted; anything else (e.g. `javascript:`) stays as literal
/// escaped text. Intentionally narrow — cells only grow links, not bold /
/// italic / code.
fn render_cell(v: &str) -> String {
    let escaped = esc(v);
    let bytes = escaped.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;
    let mut last = 0;
    while i < len {
        if bytes[i] == b'[' {
            if let Some(close_rel) = escaped[i + 1..].find(']') {
                let close = i + 1 + close_rel;
                let after = close + 1;
                if after < len && bytes[after] == b'(' {
                    if let Some(end_rel) = escaped[after + 1..].find(')') {
                        let end = after + 1 + end_rel;
                        let text = &escaped[i + 1..close];
                        let url = &escaped[after + 1..end];
                        if !text.is_empty() && is_cell_link_url(url) {
                            out.push_str(&escaped[last..i]);
                            out.push_str(&format!(r#"<a href="{}">{}</a>"#, url, text));
                            i = end + 1;
                            last = i;
                            continue;
                        }
                    }
                }
            }
        }
        i += 1;
    }
    out.push_str(&escaped[last..]);
    out
}

fn is_cell_link_url(url: &str) -> bool {
    if url.is_empty() || url.contains(char::is_whitespace) {
        return false;
    }
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.starts_with('/')
        || url.starts_with('#')
        || url.starts_with("./")
        || url.starts_with("../")
}

// ── Table ─────────────────────────────────────────

fn table(
    columns: &[TableColumn],
    rows: &[std::collections::HashMap<String, serde_yaml::Value>],
    filterable: bool,
) -> Rendered {
    let mut h = String::from(r#"<div class="c-table-wrap">"#);
    if filterable {
        h.push_str(
            r#"<input type="text" class="c-table-filter" data-table-filter placeholder="Filter…">"#,
        );
    }
    h.push_str(r#"<table class="c-table" data-kazam-table><thead><tr>"#);
    for col in columns {
        let sortable_attr = if col.sortable { " data-sortable" } else { "" };
        h.push_str(&format!(
            r#"<th class="{align}"{sortable_attr}>{label}</th>"#,
            align = col.align.class(),
            sortable_attr = sortable_attr,
            label = esc(&col.label),
        ));
    }
    h.push_str("</tr></thead><tbody>");
    for row in rows {
        h.push_str("<tr>");
        for col in columns {
            let v = row.get(&col.key).map(value_to_string).unwrap_or_default();
            h.push_str(&format!(
                r#"<td class="{}">{}</td>"#,
                col.align.class(),
                render_cell(&v)
            ));
        }
        h.push_str("</tr>");
    }
    h.push_str("</tbody></table></div>");
    Rendered::new(h).with_script("table")
}

// ── Callout ───────────────────────────────────────

fn callout(
    variant: CalloutVariant,
    title: &Option<String>,
    body: &str,
    links: Option<&[ButtonConfig]>,
    base: &str,
) -> Rendered {
    let mut h = format!(r#"<div class="c-callout {}">"#, variant.class());
    if let Some(t) = title {
        h.push_str(&format!(r#"<div class="c-callout-title">{}</div>"#, esc(t)));
    }
    h.push_str(&format!(
        r#"<div class="c-callout-body c-markdown">{}</div>"#,
        parse_markdown(body, base)
    ));
    if let Some(ls) = links {
        if !ls.is_empty() {
            h.push_str(r#"<div class="c-callout-links">"#);
            h.push_str(&button_group(ls, base).html);
            h.push_str("</div>");
        }
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Code ──────────────────────────────────────────

fn code_block(language: &Option<String>, code: &str) -> Rendered {
    let lang = language.as_deref().unwrap_or("");
    let lang_attr = if lang.is_empty() {
        String::new()
    } else {
        format!(r#" data-lang="{}""#, esc(lang))
    };
    Rendered::new(format!(
        r#"<pre class="c-code"{lang_attr}><code>{}</code></pre>"#,
        esc(code)
    ))
}

// ── Tabs ──────────────────────────────────────────

fn tabs_component(tabs: &[Tab], base: &str, config: &SiteConfig) -> Rendered {
    let mut body_html = String::from(r#"<div class="c-tabs" data-tabs>"#);
    body_html.push_str(r#"<div class="c-tab-buttons" role="tablist">"#);
    for (i, tab) in tabs.iter().enumerate() {
        body_html.push_str(&format!(
            r#"<button class="tab-btn" role="tab" aria-selected="{sel}" tabindex="{ti}">{label}</button>"#,
            sel = i == 0,
            ti = if i == 0 { "0" } else { "-1" },
            label = esc(&tab.label)
        ));
    }
    body_html.push_str("</div>");

    let mut scripts: Vec<&'static str> = vec!["tabs"];
    for tab in tabs {
        body_html.push_str(r#"<div class="tab-panel" role="tabpanel">"#);
        for c in &tab.components {
            let r = render(c, base, config);
            body_html.push_str(&r.html);
            scripts.extend(r.scripts);
        }
        body_html.push_str("</div>");
    }
    body_html.push_str("</div>");
    let mut out = Rendered::new(body_html);
    out.scripts = scripts;
    out
}

// ── Section ───────────────────────────────────────

fn section(
    heading: &Option<String>,
    eyebrow: &Option<String>,
    comps: &[Component],
    align: Align,
    id: Option<&str>,
    base: &str,
    config: &SiteConfig,
) -> Rendered {
    let mut r = Rendered::default();
    let id_attr = match slug::resolve(id, heading.as_deref()) {
        Some(slug) => format!(r#" id="{}""#, slug),
        None => String::new(),
    };
    r.html.push_str(&format!(
        r#"<section{} class="c-section {}">"#,
        id_attr,
        align.class()
    ));
    if eyebrow.is_some() || heading.is_some() {
        r.html.push_str(r#"<div class="c-section-header">"#);
        if let Some(e) = eyebrow {
            r.html.push_str(&format!(
                r#"<div class="c-section-eyebrow">{}</div>"#,
                esc(e)
            ));
        }
        if let Some(h) = heading {
            r.html
                .push_str(&format!(r#"<h2 class="c-section-heading">{}</h2>"#, esc(h)));
        }
        r.html.push_str("</div>");
    }
    for c in comps {
        r.extend(render(c, base, config));
    }
    r.html.push_str("</section>");
    r
}

// ── Columns ───────────────────────────────────────

fn columns_component(
    cols: &[Vec<Component>],
    equal_heights: bool,
    base: &str,
    config: &SiteConfig,
) -> Rendered {
    let mut r = Rendered::default();
    let class = if equal_heights {
        "c-columns c-columns-stretch"
    } else {
        "c-columns"
    };
    r.html.push_str(&format!(
        r#"<div class="{class}" style="--cols: {}">"#,
        cols.len().max(1)
    ));
    for col in cols {
        r.html.push_str(r#"<div class="c-column">"#);
        for c in col {
            r.extend(render(c, base, config));
        }
        r.html.push_str("</div>");
    }
    r.html.push_str("</div>");
    r
}

// ── Accordion ─────────────────────────────────────

fn accordion(items: &[AccordionItem], base: &str, config: &SiteConfig) -> Rendered {
    let mut r = Rendered::default();
    r.html.push_str(r#"<div class="c-accordion">"#);
    for item in items {
        r.html
            .push_str(r#"<div class="c-accordion-item" data-accordion-item>"#);
        r.html.push_str(&format!(
            r#"<button class="accordion-head" aria-expanded="false">{}<span class="accordion-chevron" aria-hidden="true">›</span></button>"#,
            esc(&item.title)
        ));
        r.html.push_str(r#"<div class="accordion-body">"#);
        for c in &item.components {
            r.extend(render(c, base, config));
        }
        r.html.push_str("</div></div>");
    }
    r.html.push_str("</div>");
    r.scripts.push("accordion");
    r
}

// ── Event Timeline ────────────────────────────────

fn event_timeline(
    events: &[EventItem],
    default_filter: EventFilter,
    show_filter_toggle: bool,
    limit: Option<u32>,
    base: &str,
) -> Rendered {
    let mut r = Rendered::default();

    // When the toggle is hidden, only render events matching the default
    // filter at build time (instead of rendering all and CSS-hiding some).
    // When the toggle is shown, all events must be in the DOM so JS can
    // switch between filters.
    let render_all = show_filter_toggle || matches!(default_filter, EventFilter::All);
    let filtered: Vec<&EventItem> = if render_all {
        events.iter().collect()
    } else {
        events
            .iter()
            .filter(|ev| matches!(ev.severity, EventSeverity::Major))
            .collect()
    };

    // Apply limit — take the last N events from the (possibly filtered) set.
    let limited: Vec<&EventItem> = match limit {
        Some(n) if (n as usize) < filtered.len() => filtered
            .into_iter()
            .rev()
            .take(n as usize)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect(),
        _ => filtered,
    };

    r.html.push_str(&format!(
        r#"<div class="c-event-timeline {}" data-filter="{}">"#,
        default_filter.class(),
        default_filter.label()
    ));

    if show_filter_toggle {
        r.html
            .push_str(r#"<div class="c-event-filter-toggle" data-event-filter-toggle>"#);
        for f in &[EventFilter::Major, EventFilter::All] {
            let active = matches!(
                (f, default_filter),
                (EventFilter::Major, EventFilter::Major) | (EventFilter::All, EventFilter::All)
            );
            let label = match f {
                EventFilter::Major => "Major only",
                EventFilter::All => "All events",
            };
            r.html.push_str(&format!(
                r#"<button type="button" data-filter="{val}"{active}>{label}</button>"#,
                val = f.label(),
                active = if active { r#" class="active""# } else { "" },
                label = label,
            ));
        }
        r.html.push_str("</div>");
    }

    r.html.push_str(r#"<ol class="c-event-list">"#);
    for ev in &limited {
        let has_summary = ev
            .summary
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

        r.html.push_str(&format!(
            r#"<li class="c-event {sev}" data-severity="{sev_label}">"#,
            sev = ev.severity.class(),
            sev_label = ev.severity.label(),
        ));
        r.html
            .push_str(r#"<div class="c-event-rail"><span class="c-event-dot"></span></div>"#);
        r.html.push_str(r#"<div class="c-event-body">"#);

        // Meta row: date · severity · source · link
        r.html.push_str(r#"<div class="c-event-meta">"#);
        r.html.push_str(&format!(
            r#"<time class="c-event-date">{}</time>"#,
            esc(&ev.date)
        ));
        r.html.push_str(&format!(
            r#"<span class="c-event-severity">{}</span>"#,
            esc(ev.severity.label())
        ));
        if let Some(src) = &ev.source {
            if !src.trim().is_empty() {
                r.html.push_str(&format!(
                    r#"<span class="c-event-source">{}</span>"#,
                    esc(src)
                ));
            }
        }
        if let Some(href) = &ev.link {
            if !href.trim().is_empty() {
                let resolved = resolve_href(href, base);
                r.html.push_str(&format!(
                    r#"<a class="c-event-link" href="{}" target="_blank" rel="noopener" aria-label="Open event source">↗</a>"#,
                    esc(&resolved)
                ));
            }
        }
        r.html.push_str("</div>");

        // Title (+ optional details if there's a summary body)
        if has_summary {
            r.html.push_str(r#"<details class="c-event-details">"#);
            r.html.push_str(&format!(
                r#"<summary class="c-event-title">{}</summary>"#,
                esc(&ev.title)
            ));
            r.html.push_str(r#"<div class="c-event-summary">"#);
            r.extend(markdown(ev.summary.as_deref().unwrap_or(""), base));
            r.html.push_str("</div></details>");
        } else {
            r.html.push_str(&format!(
                r#"<div class="c-event-title">{}</div>"#,
                esc(&ev.title)
            ));
        }

        r.html.push_str("</div></li>");
    }
    r.html.push_str("</ol></div>");

    if show_filter_toggle {
        r.scripts.push("event_timeline");
    }
    r
}

// ── Tree ──────────────────────────────────────────

fn tree(
    nodes: &[TreeNode],
    default_filter: TreeFilter,
    show_filter_toggle: bool,
    default_collapsed: bool,
) -> Rendered {
    let mut r = Rendered::default();
    let collapsed_class = if default_collapsed {
        " c-tree-collapsed"
    } else {
        ""
    };
    r.html.push_str(&format!(
        r#"<div class="c-tree {}{}" data-filter="{}">"#,
        default_filter.class(),
        collapsed_class,
        default_filter.label(),
    ));

    if show_filter_toggle {
        r.html
            .push_str(r#"<div class="c-tree-filter-toggle" data-tree-filter-toggle>"#);
        for f in &[
            TreeFilter::All,
            TreeFilter::Incomplete,
            TreeFilter::Blocked,
            TreeFilter::Priority,
        ] {
            let active = *f == default_filter;
            let label = match f {
                TreeFilter::All => "All",
                TreeFilter::Incomplete => "Incomplete only",
                TreeFilter::Blocked => "Blocked only",
                TreeFilter::Priority => "Priority only",
            };
            r.html.push_str(&format!(
                r#"<button type="button" data-filter="{val}"{active}>{label}</button>"#,
                val = f.label(),
                active = if active { r#" class="active""# } else { "" },
                label = label,
            ));
        }
        r.html.push_str("</div>");
    }

    // When the toggle is hidden and filter is non-All, prune the tree at
    // build time — only render nodes matching the filter plus their ancestor
    // chain. When the toggle is shown, all nodes must be in the DOM so JS
    // can switch between filters.
    let render_nodes: Vec<TreeNode> = if show_filter_toggle || default_filter == TreeFilter::All {
        nodes.to_vec()
    } else {
        prune_tree(nodes, default_filter)
    };

    render_tree_level(&render_nodes, &mut r.html, "c-tree-root");
    r.html.push_str("</div>");

    if show_filter_toggle || nodes.iter().any(|n| !n.children.is_empty()) {
        r.scripts.push("tree");
    }
    r
}

/// Walk the tree and return whether `node` (or any descendant) is blocked.
/// Used by `render_tree_level` to mark ancestors of blocked nodes so the
/// "blocked-only" filter can keep the path-to-root visible.
fn tree_has_blocked(node: &TreeNode) -> bool {
    if matches!(node.status, TreeStatus::Blocked) {
        return true;
    }
    node.children.iter().any(tree_has_blocked)
}

/// Walk the tree and return whether `node` (or any descendant) is priority.
fn tree_has_priority(node: &TreeNode) -> bool {
    if matches!(node.status, TreeStatus::Priority) {
        return true;
    }
    node.children.iter().any(tree_has_priority)
}

/// Prune a tree so only nodes matching the filter (plus their ancestor chain)
/// survive. Used when `show_filter_toggle: false` and a non-All filter is set,
/// so only relevant nodes are emitted at build time.
fn prune_tree(nodes: &[TreeNode], filter: TreeFilter) -> Vec<TreeNode> {
    nodes
        .iter()
        .filter_map(|node| prune_tree_node(node, filter))
        .collect()
}

fn prune_tree_node(node: &TreeNode, filter: TreeFilter) -> Option<TreeNode> {
    let child_matches = node.children.iter().any(|c| node_matches_filter(c, filter));
    let self_matches = node_matches_filter(node, filter);

    if !self_matches && !child_matches {
        return None;
    }

    let pruned_children: Vec<TreeNode> = node
        .children
        .iter()
        .filter_map(|c| prune_tree_node(c, filter))
        .collect();

    Some(TreeNode {
        label: node.label.clone(),
        status: node.status,
        note: node.note.clone(),
        children: pruned_children,
    })
}

fn node_matches_filter(node: &TreeNode, filter: TreeFilter) -> bool {
    match filter {
        TreeFilter::All => true,
        TreeFilter::Incomplete => !matches!(node.status, TreeStatus::Completed),
        TreeFilter::Blocked => matches!(node.status, TreeStatus::Blocked),
        TreeFilter::Priority => matches!(node.status, TreeStatus::Priority),
    }
}

fn render_tree_level(nodes: &[TreeNode], h: &mut String, list_class: &str) {
    h.push_str(&format!(r#"<ul class="{}">"#, list_class));
    for node in nodes {
        let has_blocked_desc = !matches!(node.status, TreeStatus::Blocked)
            && node.children.iter().any(tree_has_blocked);
        let has_priority_desc = !matches!(node.status, TreeStatus::Priority)
            && node.children.iter().any(tree_has_priority);
        let leaf_attr = if node.children.is_empty() {
            r#" data-leaf="true""#
        } else {
            ""
        };
        let blocked_desc_attr = if has_blocked_desc {
            r#" data-has-blocked-descendant="true""#
        } else {
            ""
        };
        let priority_desc_attr = if has_priority_desc {
            r#" data-has-priority-descendant="true""#
        } else {
            ""
        };
        h.push_str(&format!(
            r#"<li class="c-tree-node {status}" data-status="{status_label}"{leaf}{blocked_desc}{priority_desc}>"#,
            status = node.status.class(),
            status_label = node.status.label(),
            leaf = leaf_attr,
            blocked_desc = blocked_desc_attr,
            priority_desc = priority_desc_attr,
        ));
        let has_children = !node.children.is_empty();
        h.push_str(r#"<div class="c-tree-row">"#);
        if has_children {
            h.push_str(
                r#"<span class="c-tree-chevron" aria-hidden="true" data-tree-toggle>&#9654;</span>"#,
            );
        }
        h.push_str(&format!(
            r#"<span class="c-tree-glyph" aria-hidden="true">{}</span>"#,
            node.status.glyph()
        ));
        h.push_str(&format!(
            r#"<span class="c-tree-label">{}</span>"#,
            esc(&node.label)
        ));
        if let Some(note) = &node.note {
            if !note.trim().is_empty() {
                h.push_str(&format!(
                    r#"<span class="c-tree-note">{}</span>"#,
                    esc(note)
                ));
            }
        }
        h.push_str("</div>");
        if has_children {
            render_tree_level(&node.children, h, "c-tree-children");
        }
        h.push_str("</li>");
    }
    h.push_str("</ul>");
}

// ── Venn ──────────────────────────────────────────

fn venn(sets: &[VennSet], overlaps: &[VennOverlap], title: Option<&str>) -> Rendered {
    // Supported: 2-set or 3-set venn. Anything else degrades to a single-set
    // diagram with a warning note, so a malformed YAML doesn't break the page.
    let n = sets.len();
    let mut h = String::from(r#"<div class="c-venn">"#);
    if let Some(t) = title {
        h.push_str(&format!(r#"<div class="c-venn-title">{}</div>"#, esc(t)));
    }

    if n == 0 {
        h.push_str(r#"<div class="c-venn-empty">No sets provided.</div></div>"#);
        return Rendered::new(h);
    }

    // Geometry constants — viewBox is sized so the 3-set bounding box leaves
    // ~30-40px of breathing room on every side at default radius. Circles
    // stay at r=90 regardless of layout so 2-set and 3-set look at the same
    // visual scale.
    let (vb_w, vb_h) = (480.0, 340.0);
    let r = 90.0_f64;

    h.push_str(&format!(
        r#"<svg class="c-venn-svg" viewBox="0 0 {vb_w} {vb_h}" role="img" aria-label="{}">"#,
        title.map(esc).unwrap_or_default()
    ));

    // Compute per-set centers based on layout.
    let centers: Vec<(f64, f64)> = match n {
        1 => vec![(vb_w / 2.0, vb_h / 2.0)],
        2 => vec![
            (vb_w / 2.0 - r * 0.55, vb_h / 2.0),
            (vb_w / 2.0 + r * 0.55, vb_h / 2.0),
        ],
        _ => {
            // 3 sets: vertices of an upward-pointing triangle, recentered.
            // Distance from centroid to each vertex = r * 0.62 for healthy overlap.
            let d = r * 0.62;
            let cx = vb_w / 2.0;
            let cy = vb_h / 2.0 + d * 0.3; // slight nudge so labels fit
            vec![
                (cx, cy - d),                   // top
                (cx - d * 0.866, cy + d * 0.5), // bottom-left
                (cx + d * 0.866, cy + d * 0.5), // bottom-right
            ]
        }
    };

    // Render circles. Each set gets its theme-aware color via inline style on
    // a CSS custom property so themes can swap accents without touching here.
    for (i, set) in sets.iter().take(centers.len()).enumerate() {
        let (cx, cy) = centers[i];
        h.push_str(&format!(
            r#"<circle class="c-venn-circle c-venn-circle-{color}" cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}"/>"#,
            color = set.color.class_suffix(),
        ));
    }

    // Set labels — placed outside the central overlap so they read cleanly.
    for (i, set) in sets.iter().take(centers.len()).enumerate() {
        let (cx, cy) = centers[i];
        // Offset away from the diagram centroid, then label that point.
        let centroid_x = centers.iter().map(|c| c.0).sum::<f64>() / centers.len() as f64;
        let centroid_y = centers.iter().map(|c| c.1).sum::<f64>() / centers.len() as f64;
        let dx = cx - centroid_x;
        let dy = cy - centroid_y;
        let mag = (dx * dx + dy * dy).sqrt().max(1.0);
        let push = if n == 1 { 0.0 } else { r * 0.55 };
        let lx = cx + dx / mag * push;
        let ly = cy + dy / mag * push;
        h.push_str(&format!(
            r#"<text class="c-venn-label c-venn-label-{color}" x="{lx:.1}" y="{ly:.1}" text-anchor="middle" dominant-baseline="middle">{label}</text>"#,
            color = set.color.class_suffix(),
            label = esc(&set.label),
        ));
    }

    // Overlap labels. For pairwise overlaps in a 3-set venn the naïve centroid
    // of the two circles lands too close to the triangle centroid — every
    // pairwise label collides with the 3-way overlap label in the middle. Push
    // pairwise labels outward from the un-included set's center so they land
    // in the actual lune (the part of the overlap that excludes the third set).
    for ov in overlaps {
        if ov.sets.is_empty() {
            continue;
        }

        // Default: centroid of the involved circles.
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut count = 0;
        for &idx in &ov.sets {
            if let Some(c) = centers.get(idx) {
                sum_x += c.0;
                sum_y += c.1;
                count += 1;
            }
        }
        if count == 0 {
            continue;
        }
        let mut lx = sum_x / count as f64;
        let mut ly = sum_y / count as f64;

        // Pairwise overlap inside a 3-set venn: nudge outward from the
        // unincluded set's center so the label sits in the pairwise lune.
        if n == 3 && count == 2 {
            if let Some(third_idx) = (0..3).find(|i| !ov.sets.contains(i)) {
                let (tx, ty) = centers[third_idx];
                let dx = lx - tx;
                let dy = ly - ty;
                let mag = (dx * dx + dy * dy).sqrt().max(1.0);
                let push = r * 0.45;
                lx += dx / mag * push;
                ly += dy / mag * push;
            }
        }

        let label = ov.label.as_deref().unwrap_or("");
        if !label.is_empty() {
            h.push_str(&format!(
                r#"<text class="c-venn-overlap-label" x="{lx:.1}" y="{ly:.1}" text-anchor="middle" dominant-baseline="middle">{label}</text>"#,
                label = esc(label),
            ));
        }
    }

    h.push_str("</svg></div>");
    Rendered::new(h)
}

// ── Image ─────────────────────────────────────────

fn image(
    src: &str,
    alt: &Option<String>,
    caption: &Option<String>,
    max_width: Option<u32>,
    align: Align,
    base: &str,
) -> Rendered {
    let resolved_src = resolve_href(src, base);
    let alt_txt = alt.as_deref().unwrap_or("");
    let style = max_width
        .map(|w| format!(r#" style="--img-max: {w}px""#))
        .unwrap_or_default();
    let mut h = format!(
        r#"<figure class="c-image {align}"{style}><img src="{src}" alt="{alt}">"#,
        align = align.class(),
        style = style,
        src = esc(&resolved_src),
        alt = esc(alt_txt),
    );
    if let Some(cap) = caption {
        h.push_str(&format!(r#"<figcaption>{}</figcaption>"#, esc(cap)));
    }
    h.push_str("</figure>");
    Rendered::new(h)
}

// ── Embed ────────────────────────────────────────

fn embed(src: &str, title: &Option<String>, aspect: &Option<String>) -> Rendered {
    let ratio = aspect.as_deref().unwrap_or("16/9");
    let title_attr = title.as_deref().unwrap_or("Embedded video");
    let h = format!(
        r#"<div class="c-embed" style="--embed-ratio: {ratio}"><iframe src="{src}" title="{title}" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe></div>"#,
        ratio = esc(ratio),
        src = esc(src),
        title = esc(title_attr),
    );
    Rendered::new(h)
}

// ── Resources ────────────────────────────────────────

fn resources(items: &[crate::types::ResourceItem]) -> Rendered {
    let mut h = String::from(r#"<div class="c-resources"><ul class="c-resources-list">"#);
    for item in items {
        h.push_str(r#"<li class="c-resources-item">"#);
        h.push_str(&format!(
            r#"<a href="{href}" class="c-resources-link" target="_blank" rel="noopener">"#,
            href = esc(&item.href)
        ));
        h.push_str(&format!(
            r#"<strong class="c-resources-title">{}</strong>"#,
            esc(&item.title)
        ));
        if let Some(desc) = &item.description {
            h.push_str(&format!(
                r#"<span class="c-resources-desc">{}</span>"#,
                esc(desc)
            ));
        }
        h.push_str("</a>");
        if let Some(owner) = &item.owner {
            h.push_str(&format!(
                r#"<span class="c-resources-owner">{}</span>"#,
                esc(owner)
            ));
        }
        h.push_str("</li>");
    }
    h.push_str("</ul></div>");
    Rendered::new(h)
}

// ═════════════════════════════════════════════════════════════════════════
// Phase 1 additions
// ═════════════════════════════════════════════════════════════════════════

// ── Badge ────────────────────────────────────────

fn badge(label: &str, color: SemColor) -> Rendered {
    Rendered::new(format!(
        r#"<span class="c-badge c-badge-{color}">{label}</span>"#,
        color = sem_color_class(color),
        label = esc(label)
    ))
}

// ── Tag ──────────────────────────────────────────

fn tag(label: &str, color: SemColor) -> Rendered {
    Rendered::new(format!(
        r#"<span class="c-tag c-tag-{color}">{label}</span>"#,
        color = sem_color_class(color),
        label = esc(label)
    ))
}

// ── Divider ──────────────────────────────────────

fn divider(label: &Option<String>) -> Rendered {
    match label {
        Some(l) => Rendered::new(format!(
            r#"<div class="c-divider c-divider-labeled"><span class="c-divider-line"></span><span class="c-divider-label">{}</span><span class="c-divider-line"></span></div>"#,
            esc(l)
        )),
        None => Rendered::new(r#"<hr class="c-divider">"#.to_string()),
    }
}

// ── Kbd ──────────────────────────────────────────

fn kbd(keys: &[String]) -> Rendered {
    let mut h = String::from(r#"<span class="c-kbd-group">"#);
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            h.push_str(r#"<span class="c-kbd-sep">+</span>"#);
        }
        h.push_str(&format!(r#"<kbd class="c-kbd">{}</kbd>"#, esc(k)));
    }
    h.push_str("</span>");
    Rendered::new(h)
}

// ── Status ───────────────────────────────────────

fn status(label: &str, color: SemColor) -> Rendered {
    Rendered::new(format!(
        r#"<span class="c-status c-status-{color}"><span class="c-status-dot"></span><span>{label}</span></span>"#,
        color = sem_color_class(color),
        label = esc(label)
    ))
}

// ── Breadcrumb ───────────────────────────────────

fn breadcrumb(items: &[BreadcrumbItem], base: &str) -> Rendered {
    let mut h = String::from(r#"<nav class="c-breadcrumb" aria-label="Breadcrumb"><ol>"#);
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            h.push_str(r#"<li class="c-breadcrumb-sep" aria-hidden="true">/</li>"#);
        }
        h.push_str(r#"<li class="c-breadcrumb-item">"#);
        match &item.href {
            Some(href) => {
                h.push_str(&format!(
                    r#"<a href="{}">{}</a>"#,
                    esc(&resolve_href(href, base)),
                    esc(&item.label)
                ));
            }
            None => {
                h.push_str(&format!(
                    r#"<span aria-current="page">{}</span>"#,
                    esc(&item.label)
                ));
            }
        }
        h.push_str("</li>");
    }
    h.push_str("</ol></nav>");
    Rendered::new(h)
}

// ── Button Group ─────────────────────────────────

fn button_group(buttons: &[ButtonConfig], base: &str) -> Rendered {
    let mut h = String::from(r#"<div class="c-button-group">"#);
    for b in buttons {
        let variant_class = match b.variant {
            ButtonVariant::Primary => "c-button-primary",
            ButtonVariant::Secondary => "c-button-secondary",
            ButtonVariant::Ghost => "c-button-ghost",
        };
        let target = if b.external {
            r#" target="_blank" rel="noopener""#
        } else {
            ""
        };
        h.push_str(&format!(
            r#"<a href="{href}" class="c-button {variant}"{target}>"#,
            href = esc(&resolve_href(&b.href, base)),
            variant = variant_class,
            target = target
        ));
        if let Some(icon_name) = &b.icon {
            h.push_str(&format!(
                r#"<span class="c-button-icon">{}</span>"#,
                icons::render(icon_name, 14, "currentColor")
            ));
        }
        h.push_str(&format!(r#"<span>{}</span>"#, esc(&b.label)));
        if b.external {
            h.push_str(&format!(
                r#"<span class="c-button-icon">{}</span>"#,
                icons::render("arrow-up-right", 14, "currentColor")
            ));
        }
        h.push_str("</a>");
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Definition List ──────────────────────────────

fn definition_list(items: &[DefinitionItem]) -> Rendered {
    let mut h = String::from(r#"<dl class="c-definition-list">"#);
    for i in items {
        h.push_str(&format!(
            r#"<div class="c-dl-row"><dt class="c-dl-term">{term}</dt><dd class="c-dl-def">{def}</dd></div>"#,
            term = esc(&i.term),
            def = esc(&i.definition)
        ));
    }
    h.push_str("</dl>");
    Rendered::new(h)
}

// ── Blockquote ───────────────────────────────────

fn blockquote(body: &str, attribution: &Option<String>) -> Rendered {
    let mut h = format!(
        r#"<figure class="c-blockquote"><blockquote><p>{}</p></blockquote>"#,
        esc(body)
    );
    if let Some(a) = attribution {
        h.push_str(&format!(
            r#"<figcaption class="c-blockquote-attribution">— {}</figcaption>"#,
            esc(a)
        ));
    }
    h.push_str("</figure>");
    Rendered::new(h)
}

// ── Avatar ───────────────────────────────────────

fn initials(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .filter_map(|w| w.chars().next())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn avatar(
    name: &str,
    src: &Option<String>,
    size: AvatarSize,
    subtitle: &Option<String>,
) -> Rendered {
    let size_class = size.class_suffix();
    let wrapper_open = if subtitle.is_some() {
        format!(r#"<div class="c-avatar-row"><div class="c-avatar c-avatar-{size_class}">"#)
    } else {
        format!(r#"<div class="c-avatar c-avatar-{size_class}">"#)
    };
    let mut h = wrapper_open;
    match src {
        Some(s) => {
            h.push_str(&format!(r#"<img src="{}" alt="{}">"#, esc(s), esc(name)));
        }
        None => {
            h.push_str(&format!(
                r#"<span class="c-avatar-initials">{}</span>"#,
                esc(&initials(name))
            ));
        }
    }
    h.push_str("</div>");
    if let Some(sub) = subtitle {
        h.push_str(&format!(
            r#"<div class="c-avatar-meta"><div class="c-avatar-name">{}</div><div class="c-avatar-sub">{}</div></div></div>"#,
            esc(name), esc(sub)
        ));
    }
    Rendered::new(h)
}

// ── Avatar Group ─────────────────────────────────

fn avatar_group(avatars: &[AvatarConfig], size: AvatarSize, max: usize) -> Rendered {
    let size_class = size.class_suffix();
    let mut h = format!(r#"<div class="c-avatar-group c-avatar-group-{size_class}">"#);
    let visible = avatars.len().min(max);
    for a in avatars.iter().take(visible) {
        h.push_str(&format!(
            r#"<div class="c-avatar c-avatar-{size_class}" title="{}">"#,
            esc(&a.name)
        ));
        match &a.src {
            Some(s) => h.push_str(&format!(r#"<img src="{}" alt="{}">"#, esc(s), esc(&a.name))),
            None => h.push_str(&format!(
                r#"<span class="c-avatar-initials">{}</span>"#,
                esc(&initials(&a.name))
            )),
        }
        h.push_str("</div>");
    }
    if avatars.len() > max {
        let remaining = avatars.len() - max;
        h.push_str(&format!(
            r#"<div class="c-avatar c-avatar-{size_class} c-avatar-more"><span class="c-avatar-initials">+{}</span></div>"#,
            remaining
        ));
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Progress Bar ─────────────────────────────────

fn progress_bar(
    value: u8,
    label: &Option<String>,
    color: SemColor,
    detail: &Option<String>,
) -> Rendered {
    let clamped = value.min(100);
    let color_class = sem_color_class(color);

    let mut h = String::from(r#"<div class="c-progress">"#);
    if label.is_some() || detail.is_some() {
        h.push_str(r#"<div class="c-progress-labels">"#);
        if let Some(l) = label {
            h.push_str(&format!(
                r#"<span class="c-progress-label">{}</span>"#,
                esc(l)
            ));
        } else {
            h.push_str(r#"<span></span>"#);
        }
        h.push_str(&format!(
            r#"<span class="c-progress-value">{}%</span>"#,
            clamped
        ));
        h.push_str("</div>");
    }
    h.push_str(&format!(
        r#"<div class="c-progress-track" role="progressbar" aria-valuenow="{v}" aria-valuemin="0" aria-valuemax="100"><div class="c-progress-fill c-progress-fill-{color}" style="--progress: {v}%"></div></div>"#,
        v = clamped, color = color_class
    ));
    if let Some(d) = detail {
        h.push_str(&format!(
            r#"<div class="c-progress-detail">{}</div>"#,
            esc(d)
        ));
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Empty State ──────────────────────────────────

fn empty_state(
    title: &str,
    body: &Option<String>,
    action: &Option<EmptyStateAction>,
    icon: &Option<String>,
    base: &str,
) -> Rendered {
    let mut h = String::from(r#"<div class="c-empty-state">"#);
    let icon_name = icon.as_deref().unwrap_or("inbox");
    h.push_str(&format!(
        r#"<div class="c-empty-state-icon">{}</div>"#,
        icons::render(icon_name, 32, "currentColor")
    ));
    h.push_str(&format!(
        r#"<h3 class="c-empty-state-title">{}</h3>"#,
        esc(title)
    ));
    if let Some(b) = body {
        h.push_str(&format!(r#"<p class="c-empty-state-body">{}</p>"#, esc(b)));
    }
    if let Some(a) = action {
        h.push_str(&format!(
            r#"<a href="{href}" class="c-button c-button-primary">{label}</a>"#,
            href = esc(&resolve_href(&a.href, base)),
            label = esc(&a.label)
        ));
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Icon ─────────────────────────────────────────

fn icon_component(name: &str, size: IconSize, color: SemColor) -> Rendered {
    let px = size.pixels();
    let color_value = match color {
        SemColor::Default => "currentColor".to_string(),
        _ => color.hex().to_string(),
    };
    Rendered::new(icons::render(name, px, &color_value))
}

// ── Role Map ──────────────────────────────────────────

fn role_map(title: Option<&str>, config: &SiteConfig, base: &str) -> Rendered {
    let mut html = String::from(r#"<div class="c-role-map">"#);

    if let Some(t) = title {
        html.push_str(&format!(r#"<h2 class="c-role-map-title">{}</h2>"#, esc(t)));
    }

    html.push_str(r#"<div class="c-role-map-grid">"#);

    if config.roles.is_empty() {
        let empty = empty_state(
            "No roles defined",
            &Some("Add a roles: section to kazam.yaml to populate this map.".to_string()),
            &None,
            &Some("users".to_string()),
            base,
        );
        html.push_str(&empty.html);
    } else {
        for role in &config.roles {
            let href = role
                .href
                .as_deref()
                .map(|h| resolve_href(h, base))
                .unwrap_or_else(|| format!("?role={}", esc(&role.id)));
            html.push_str(&format!(
                r#"<a class="c-role-map-card" href="{}">"#,
                esc(&href)
            ));
            if let Some(icon) = &role.icon {
                html.push_str(&format!(
                    r#"<span class="c-role-map-icon">{}</span>"#,
                    esc(icon)
                ));
            }
            html.push_str(&format!(
                r#"<span class="c-role-map-label">{}</span>"#,
                esc(&role.label)
            ));
            if let Some(desc) = &role.description {
                html.push_str(&format!(
                    r#"<span class="c-role-map-desc">{}</span>"#,
                    esc(desc)
                ));
            }
            html.push_str("</a>");
        }
    }

    html.push_str("</div></div>");
    Rendered::new(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_cell_plain_text_is_escaped() {
        assert_eq!(render_cell("Acme Corp"), "Acme Corp");
        assert_eq!(render_cell("<script>"), "&lt;script&gt;");
        assert_eq!(render_cell("a & b"), "a &amp; b");
    }

    #[test]
    fn render_cell_linkifies_markdown_link() {
        assert_eq!(
            render_cell("[INT-169](https://linear.app/maze-sec/issue/INT-169)"),
            r#"<a href="https://linear.app/maze-sec/issue/INT-169">INT-169</a>"#
        );
    }

    #[test]
    fn render_cell_linkifies_inline_link_among_text() {
        assert_eq!(
            render_cell("See [docs](https://example.com) for more."),
            r#"See <a href="https://example.com">docs</a> for more."#
        );
    }

    #[test]
    fn render_cell_rejects_javascript_url() {
        assert_eq!(
            render_cell("[x](javascript:alert(1))"),
            "[x](javascript:alert(1))"
        );
    }

    #[test]
    fn render_cell_ignores_other_markdown() {
        // Bold / italic / code intentionally not rendered in cells.
        assert_eq!(render_cell("**bold**"), "**bold**");
        assert_eq!(render_cell("_italic_"), "_italic_");
        assert_eq!(render_cell("`code`"), "`code`");
    }

    #[test]
    fn render_cell_accepts_relative_and_mailto() {
        assert_eq!(
            render_cell("[home](/index.html)"),
            r#"<a href="/index.html">home</a>"#
        );
        assert_eq!(
            render_cell("[mail](mailto:hi@example.com)"),
            r#"<a href="mailto:hi@example.com">mail</a>"#
        );
    }

    #[test]
    fn render_cell_preserves_multibyte_text() {
        // Em dash (3 bytes in UTF-8) must survive unchanged.
        assert_eq!(render_cell("One — two"), "One — two");
        assert_eq!(
            render_cell("[résumé](https://example.com/cv)"),
            r#"<a href="https://example.com/cv">résumé</a>"#
        );
    }

    #[test]
    fn render_cell_leaves_unmatched_brackets_alone() {
        assert_eq!(render_cell("[not a link"), "[not a link");
        assert_eq!(render_cell("[text](no scheme)"), "[text](no scheme)");
    }
}

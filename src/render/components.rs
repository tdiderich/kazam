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
            summary,
        } => table(columns, rows, *filterable, summary.as_ref()),
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
            filter_by,
            group_by,
        } => event_timeline(
            events,
            *default_filter,
            *show_filter_toggle,
            *limit,
            filter_by,
            *group_by,
            base,
        ),
        Component::Tree {
            nodes,
            default_filter,
            show_filter_toggle,
            default_collapsed,
            default_depth,
            show_counts,
            show_summary,
            default_view,
        } => tree(
            nodes,
            *default_filter,
            *show_filter_toggle,
            *default_collapsed,
            *default_depth,
            *show_counts,
            *show_summary,
            *default_view,
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
            target,
            thresholds,
        } => progress_bar(*value, label, *color, detail, *target, thresholds),
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
            scale,
        } => apply_scale(
            charts::render(charts::ChartSpec {
                kind: *kind,
                title,
                height: *height,
                x_label,
                y_label,
                orientation: *orientation,
                data,
                series,
            }),
            *scale,
        ),
        Component::RoleMap { title } => role_map(title.as_deref(), config, base),
        Component::Sankey {
            title,
            height,
            flows,
            colors,
            scale,
        } => apply_scale(charts::render_sankey(title, *height, flows, colors), *scale),
        Component::Radar {
            title,
            height,
            axes,
            curves,
            max,
            scale,
        } => apply_scale(
            charts::render_radar(title, *height, axes, curves, *max),
            *scale,
        ),
        Component::Quadrant {
            title,
            height,
            x_axis,
            y_axis,
            quadrants,
            points,
            scale,
        } => apply_scale(
            charts::render_quadrant(title, *height, x_axis, y_axis, quadrants, points),
            *scale,
        ),
        Component::Architecture {
            title,
            height,
            direction,
            nodes,
            connections,
            scale,
        } => apply_scale(
            charts::render_architecture(title, *height, *direction, nodes, connections),
            *scale,
        ),
        Component::Pipeline {
            title,
            height,
            inputs,
            stages,
            outputs,
            context,
            scale,
        } => apply_scale(
            charts::render_pipeline(title, *height, inputs, stages, outputs, context),
            *scale,
        ),
        Component::Graph {
            title,
            height,
            direction,
            nodes,
            edges,
            groups,
            row_labels,
            scale,
        } => apply_scale(
            charts::render_graph(title, *height, *direction, nodes, edges, groups, row_labels),
            *scale,
        ),
        Component::OrgChart {
            title,
            people,
            default_open_depth,
        } => render_org_chart(title, people, *default_open_depth),
        Component::Aside { body } => aside(body, base),
        Component::RuleList { items } => rule_list(items),
        Component::Gauge {
            items,
            title,
            columns,
            max,
        } => gauge(items, title.as_deref(), *columns, *max),
        Component::PriorityQueue {
            items,
            group_by,
            show_dates,
            show_counts,
            filterable,
            title,
        } => priority_queue(
            items,
            *group_by,
            *show_dates,
            *show_counts,
            *filterable,
            title.as_deref(),
        ),
    }
}

pub(super) fn sem_color_class(c: SemColor) -> &'static str {
    c.class_suffix()
}

/// Wraps a chart/diagram's rendered HTML in a centered container sized to
/// `scale` (fraction of the container width; height follows since the SVG
/// keeps its aspect ratio). A no-op when `scale` is unset. Clamped to
/// 0.1–2.0 so a bad value can't collapse or blow out the layout.
fn apply_scale(mut r: Rendered, scale: Option<f32>) -> Rendered {
    if let Some(s) = scale {
        let s = s.clamp(0.1, 2.0);
        r.html = format!(
            r#"<div class="c-chart-scale" style="--kz-scale: {s}">{html}</div>"#,
            s = s,
            html = r.html,
        );
    }
    r
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
            h.push_str(&format!(
                r#"<p class="c-card-desc">{}</p>"#,
                parse_markdown_inline(d)
            ));
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
        if s.trend.is_some() || s.previous.is_some() {
            h.push_str(r#"<div class="c-stat-trend-row">"#);
            if let Some(trend) = &s.trend {
                h.push_str(&format!(
                    r#"<span class="c-stat-trend {cls}">{arrow}</span>"#,
                    cls = trend.class(),
                    arrow = trend.arrow(),
                ));
            }
            if let Some(prev) = &s.previous {
                h.push_str(&format!(
                    r#"<span class="c-stat-previous">was {}</span>"#,
                    esc(prev)
                ));
            }
            h.push_str("</div>");
        }
        if let Some(history) = &s.history {
            if !history.is_empty() {
                h.push_str(&render_sparkline(history));
            }
        }
        h.push_str("</div>");
    }
    h.push_str("</div>");
    Rendered::new(h)
}

fn render_sparkline(values: &[f64]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = (max - min).max(1.0);
    let w = 80.0;
    let h_svg = 24.0;
    let pad = 2.0;
    let usable_h = h_svg - pad * 2.0;
    let step = if values.len() > 1 {
        w / (values.len() - 1) as f64
    } else {
        w
    };

    let points: Vec<String> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = i as f64 * step;
            let y = pad + usable_h - ((v - min) / range) * usable_h;
            format!("{x:.1},{y:.1}")
        })
        .collect();
    let polyline = points.join(" ");
    let last_x = (values.len() - 1) as f64 * step;
    let last_y = pad + usable_h - ((values[values.len() - 1] - min) / range) * usable_h;

    format!(
        r#"<svg class="c-stat-sparkline" viewBox="0 0 {w} {h}" preserveAspectRatio="none"><polyline points="{pts}" fill="none" stroke="var(--stat-color, var(--teal))" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round"/><circle cx="{lx:.1}" cy="{ly:.1}" r="2" fill="var(--stat-color, var(--teal))"/></svg>"#,
        w = w,
        h = h_svg,
        pts = polyline,
        lx = last_x,
        ly = last_y,
    )
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
    opts.insert(Options::ENABLE_TASKLISTS);
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
/// escaped text. Intentionally narrow - cells only grow links, not bold /
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
    summary: Option<&TableSummary>,
) -> Rendered {
    let mut h = String::from(r#"<div class="c-table-wrap">"#);

    if let Some(sum) = summary {
        h.push_str(&render_table_summary(rows, sum));
    }

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
            let color_class = col
                .color_map
                .get(&v)
                .map(|c| format!(" cell-{}", c.class_suffix()))
                .unwrap_or_default();
            h.push_str(&format!(
                r#"<td class="{}{}">{}</td>"#,
                col.align.class(),
                color_class,
                render_cell(&v)
            ));
        }
        h.push_str("</tr>");
    }
    h.push_str("</tbody></table></div>");
    Rendered::new(h).with_script("table")
}

fn render_table_summary(
    rows: &[std::collections::HashMap<String, serde_yaml::Value>],
    summary: &TableSummary,
) -> String {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for row in rows {
        let val = row
            .get(&summary.group_by)
            .map(value_to_string)
            .unwrap_or_default();
        *counts.entry(val).or_insert(0) += 1;
    }
    let total = rows.len().max(1);
    let mut h = String::from(r#"<div class="c-table-summary">"#);
    h.push_str(r#"<div class="c-table-summary-dots">"#);
    for (val, count) in &counts {
        let color = summary
            .colors
            .get(val)
            .copied()
            .unwrap_or(SemColor::Default);
        h.push_str(&format!(
            r#"<span class="c-table-summary-item"><span class="c-table-summary-dot color-{color}"></span>{label} <strong>{count}</strong></span>"#,
            color = color.class_suffix(),
            label = esc(val),
            count = count,
        ));
    }
    h.push_str("</div>");
    h.push_str(r#"<div class="c-table-summary-bar">"#);
    for (val, count) in &counts {
        let color = summary
            .colors
            .get(val)
            .copied()
            .unwrap_or(SemColor::Default);
        let pct = (*count as f64 / total as f64) * 100.0;
        h.push_str(&format!(
            r#"<div class="c-table-summary-seg color-bg-{color}" style="width: {pct:.1}%" title="{label}: {count}"></div>"#,
            color = color.class_suffix(),
            pct = pct,
            label = esc(val),
            count = count,
        ));
    }
    h.push_str("</div></div>");
    h
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
    _limit: Option<u32>,
    filter_by: &[String],
    _group_by: Option<EventGroupBy>,
    base: &str,
) -> Rendered {
    let mut r = Rendered::default();

    // When the toggle is hidden, only render events matching the default
    // filter at build time (instead of rendering all and CSS-hiding some).
    // When the toggle is shown, all events must be in the DOM so JS can
    // switch between filters.
    // Most-recent-first, regardless of the order the author wrote them in -
    // an event timeline is a changelog/activity feed, not an ordered list an
    // author controls. Stable sort so same-date entries keep their authored
    // relative order.
    let mut sorted: Vec<&EventItem> = events.iter().collect();
    sorted.sort_by(|a, b| b.date.cmp(&a.date));

    let render_all = show_filter_toggle || matches!(default_filter, EventFilter::All);
    let filtered: Vec<&EventItem> = if render_all {
        sorted
    } else {
        sorted
            .into_iter()
            .filter(|ev| matches!(ev.severity, EventSeverity::Major))
            .collect()
    };

    let _total_filtered = filtered.len();

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

    if !filter_by.is_empty() {
        r.html
            .push_str(r#"<div class="c-event-tag-filters" data-event-tag-filter>"#);
        for tag in filter_by {
            let count = filtered
                .iter()
                .filter(|e| e.tags.iter().any(|t| t == tag))
                .count();
            r.html.push_str(&format!(
                r#"<button type="button" class="c-event-tag-pill" data-tag="{}">{} <span class="c-event-tag-count">{}</span></button>"#,
                esc(tag),
                esc(tag),
                count
            ));
        }
        r.html.push_str("</div>");
    }

    r.html.push_str(r#"<ol class="c-event-list">"#);
    for ev in &filtered {
        let has_summary = ev
            .summary
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

        let tags_attr = if !ev.tags.is_empty() {
            format!(
                r#" data-tags="{}""#,
                ev.tags.iter().map(|t| esc(t)).collect::<Vec<_>>().join(",")
            )
        } else {
            String::new()
        };

        r.html.push_str(&format!(
            r#"<li class="c-event {sev}" data-severity="{sev_label}"{tags}>"#,
            sev = ev.severity.class(),
            sev_label = ev.severity.label(),
            tags = tags_attr,
        ));
        r.html
            .push_str(r#"<div class="c-event-rail"><span class="c-event-dot"></span></div>"#);
        r.html.push_str(r#"<div class="c-event-body">"#);

        // Meta row: date · severity · source · tags · link
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
        for t in &ev.tags {
            r.html
                .push_str(&format!(r#"<span class="c-event-tag">{}</span>"#, esc(t)));
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

#[allow(clippy::too_many_arguments)]
fn tree(
    nodes: &[TreeNode],
    default_filter: TreeFilter,
    show_filter_toggle: bool,
    default_collapsed: bool,
    default_depth: Option<u32>,
    _show_counts: bool,
    show_summary: bool,
    default_view: TreeDefaultView,
) -> Rendered {
    let mut r = Rendered::default();
    let collapsed_class = if default_collapsed {
        " c-tree-collapsed"
    } else {
        ""
    };
    let depth_attr = default_depth
        .map(|d| format!(r#" data-default-depth="{}""#, d))
        .unwrap_or_default();
    let view_attr = if show_summary {
        match default_view {
            TreeDefaultView::Summary => r#" data-view="summary""#,
            TreeDefaultView::Tree => r#" data-view="tree""#,
        }
    } else {
        ""
    };
    r.html.push_str(&format!(
        r#"<div class="c-tree {}{}" data-filter="{}"{}{}>
"#,
        default_filter.class(),
        collapsed_class,
        default_filter.label(),
        depth_attr,
        view_attr,
    ));

    if show_filter_toggle || show_summary {
        r.html
            .push_str(r#"<div class="c-tree-filter-toggle" data-tree-filter-toggle>"#);
        if show_summary {
            let active_summary = default_view == TreeDefaultView::Summary;
            r.html.push_str(&format!(
                r#"<button type="button" data-filter="summary"{}>{}</button>"#,
                if active_summary {
                    r#" class="active""#
                } else {
                    ""
                },
                "Summary",
            ));
        }
        if show_filter_toggle {
            for f in &[TreeFilter::All, TreeFilter::Incomplete] {
                let active = *f == default_filter
                    && (!show_summary || default_view == TreeDefaultView::Tree);
                let label = match f {
                    TreeFilter::All => "All",
                    TreeFilter::Incomplete => "Incomplete",
                    _ => unreachable!(),
                };
                r.html.push_str(&format!(
                    r#"<button type="button" data-filter="{val}"{active}>{label}</button>"#,
                    val = f.label(),
                    active = if active { r#" class="active""# } else { "" },
                    label = label,
                ));
            }
        }
        r.html.push_str("</div>");
    }

    if show_summary {
        render_tree_summary(nodes, &mut r.html);
    }

    let tree_hidden = show_summary && default_view == TreeDefaultView::Summary;
    r.html.push_str(&format!(
        r#"<div class="c-tree-body"{}>"#,
        if tree_hidden {
            r#" style="display:none""#
        } else {
            ""
        },
    ));

    let render_nodes: Vec<TreeNode> = if show_filter_toggle || default_filter == TreeFilter::All {
        nodes.to_vec()
    } else {
        prune_tree(nodes, default_filter)
    };

    render_tree_level(&render_nodes, &mut r.html, "c-tree-root");
    r.html.push_str("</div></div>");

    if show_filter_toggle || show_summary || nodes.iter().any(|n| !n.children.is_empty()) {
        r.scripts.push("tree");
    }
    r
}

fn count_phase_progress(nodes: &[TreeNode]) -> (usize, usize) {
    let mut total = 0usize;
    let mut done = 0usize;
    for node in nodes {
        total += 1;
        if matches!(node.status, TreeStatus::Completed) {
            done += 1;
        }
        let (t, d) = count_phase_progress(&node.children);
        total += t;
        done += d;
    }
    (total, done)
}

fn render_phase_bar(node: &TreeNode, h: &mut String, depth: usize) {
    let (child_total, child_done) = count_phase_progress(&node.children);
    let total = child_total + 1;
    let done = if matches!(node.status, TreeStatus::Completed) {
        child_done + 1
    } else {
        child_done
    };
    let pct = if total > 0 {
        (done as f64 / total as f64 * 100.0) as u32
    } else {
        0
    };
    let indent_class = if depth > 0 { " c-tree-summary-sub" } else { "" };
    h.push_str(&format!(
        r#"<div class="c-tree-summary-phase{}"><div class="c-tree-summary-phase-label">{} <span class="c-tree-summary-pct">{}/{} done · {}%</span></div><div class="c-progress-track"><div class="c-progress-fill color-{}" style="width:{}%"></div></div></div>"#,
        indent_class,
        esc(&node.label),
        done,
        total,
        pct,
        if pct == 100 { "green" } else if pct >= 50 { "yellow" } else { "default" },
        pct,
    ));
    for child in &node.children {
        if !child.children.is_empty() {
            render_phase_bar(child, h, depth + 1);
        }
    }
}

fn render_tree_summary(nodes: &[TreeNode], h: &mut String) {
    h.push_str(r#"<div class="c-tree-summary" data-tree-summary>"#);

    h.push_str(r#"<div class="c-tree-summary-phases">"#);
    h.push_str(r#"<div class="c-tree-summary-heading">Phase Progress</div>"#);
    for phase in nodes {
        render_phase_bar(phase, h, 0);
    }
    h.push_str("</div>");

    h.push_str("</div>");
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
        owner: node.owner.clone(),
        due: node.due.clone(),
        original_due: node.original_due.clone(),
    })
}

fn node_matches_filter(node: &TreeNode, filter: TreeFilter) -> bool {
    match filter {
        TreeFilter::All => true,
        TreeFilter::Incomplete => !matches!(node.status, TreeStatus::Completed),
        TreeFilter::Blocked => matches!(node.status, TreeStatus::Blocked),
        TreeFilter::Priority => matches!(node.status, TreeStatus::Priority),
        TreeFilter::Overdue => false,
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
        if let Some(owner) = &node.owner {
            if !owner.trim().is_empty() {
                h.push_str(&format!(
                    r#"<span class="c-tree-owner" title="Owner: {}">&#128100; {}</span>"#,
                    esc(owner),
                    esc(owner)
                ));
            }
        }
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

// ── Priority Queue ────────────────────────────────

/// Urgency bucket derived from an item's `due` date relative to "today" plus
/// the two group boundaries (`week_end`, `two_week_end`). Drives both the
/// `Urgency`/`Horizon` group headers and the per-row stripe color.
#[derive(Clone, Copy, PartialEq, Eq)]
enum QueueBucket {
    Overdue,
    TwoWeeks,
    TwoToEight,
    Later,
    NoDate,
}

impl QueueBucket {
    fn urgency_label(&self) -> &'static str {
        match self {
            QueueBucket::Overdue => "Overdue",
            QueueBucket::TwoWeeks => "Next two weeks",
            QueueBucket::TwoToEight => "2-8 weeks",
            QueueBucket::Later => "Later",
            QueueBucket::NoDate => "No date",
        }
    }

    fn horizon_label(&self) -> &'static str {
        match self {
            QueueBucket::Overdue | QueueBucket::TwoWeeks => "Now",
            QueueBucket::TwoToEight => "Next",
            QueueBucket::Later => "Later",
            QueueBucket::NoDate => "No date",
        }
    }

    fn urgency_rank(&self) -> u8 {
        match self {
            QueueBucket::Overdue => 0,
            QueueBucket::TwoWeeks => 1,
            QueueBucket::TwoToEight => 2,
            QueueBucket::Later => 3,
            QueueBucket::NoDate => 4,
        }
    }
}

/// `TreeStatus` doesn't derive `PartialEq` (it's defined in types.rs, which
/// this module doesn't own), so grouping by status needs a manual compare.
fn tree_status_eq(a: TreeStatus, b: TreeStatus) -> bool {
    matches!(
        (a, b),
        (TreeStatus::Default, TreeStatus::Default)
            | (TreeStatus::Completed, TreeStatus::Completed)
            | (TreeStatus::Active, TreeStatus::Active)
            | (TreeStatus::Blocked, TreeStatus::Blocked)
            | (TreeStatus::Priority, TreeStatus::Priority)
            | (TreeStatus::Upcoming, TreeStatus::Upcoming)
    )
}

fn queue_bucket_from_date(
    item: &QueueItem,
    today: &str,
    two_week_end: &str,
    eight_week_end: &str,
) -> QueueBucket {
    match item.due.as_deref() {
        None => QueueBucket::NoDate,
        Some(due) => {
            let completed = matches!(item.status, TreeStatus::Completed);
            if due < today {
                if completed {
                    QueueBucket::TwoWeeks
                } else {
                    QueueBucket::Overdue
                }
            } else if due <= two_week_end {
                QueueBucket::TwoWeeks
            } else if due <= eight_week_end {
                QueueBucket::TwoToEight
            } else {
                QueueBucket::Later
            }
        }
    }
}

fn horizon_to_bucket(h: QueueHorizon) -> QueueBucket {
    match h {
        QueueHorizon::Now => QueueBucket::TwoWeeks,
        QueueHorizon::Next => QueueBucket::TwoToEight,
        QueueHorizon::Later => QueueBucket::Later,
    }
}

fn queue_bucket(
    item: &QueueItem,
    today: &str,
    two_week_end: &str,
    eight_week_end: &str,
) -> QueueBucket {
    match item.horizon {
        Some(h) => horizon_to_bucket(h),
        None => queue_bucket_from_date(item, today, two_week_end, eight_week_end),
    }
}

fn queue_has_drift(
    item: &QueueItem,
    today: &str,
    two_week_end: &str,
    eight_week_end: &str,
) -> bool {
    let explicit = match item.horizon {
        Some(h) => h,
        None => return false,
    };
    if item.due.is_none() {
        return false;
    }
    let date_bucket = queue_bucket_from_date(item, today, two_week_end, eight_week_end);
    let horizon_bucket = horizon_to_bucket(explicit);
    date_bucket.urgency_rank() < horizon_bucket.urgency_rank()
}

/// CSS stripe class for a row. Blocked items always render as blocked
/// regardless of date, overriding whatever bucket their due date implies.
fn queue_urgency_class(item: &QueueItem, bucket: QueueBucket) -> &'static str {
    if matches!(item.status, TreeStatus::Blocked) {
        return "urgency-blocked";
    }
    match bucket {
        QueueBucket::Overdue => "urgency-overdue",
        QueueBucket::TwoWeeks => "urgency-soon",
        QueueBucket::TwoToEight | QueueBucket::Later => "urgency-track",
        QueueBucket::NoDate => "urgency-none",
    }
}

/// Gregorian (y, m, d) → Julian day number.
fn queue_julian_day(y: i32, m: i32, d: i32) -> i32 {
    let a = (14 - m) / 12;
    let y2 = y + 4800 - a;
    let m2 = m + 12 * a - 3;
    d + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32045
}

/// Julian day number → Gregorian (y, m, d). Inverse of `queue_julian_day`.
fn queue_from_julian_day(jdn: i32) -> (i32, i32, i32) {
    let a = jdn + 32044;
    let b = (4 * a + 3) / 146097;
    let c = a - (146097 * b) / 4;
    let d = (4 * c + 3) / 1461;
    let e = c - (1461 * d) / 4;
    let m = (5 * e + 2) / 153;
    let day = e - (153 * m + 2) / 5 + 1;
    let month = m + 3 - 12 * (m / 10);
    let year = 100 * b + d - 4800 + m / 10;
    (year, month, day)
}

/// Add (or subtract) `days` from a `YYYY-MM-DD` string. Malformed input is
/// returned unchanged so a bad date never panics the build.
fn add_days_to_date(date: &str, days: i32) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return date.to_string();
    }
    let (y, m, d) = match (
        parts[0].parse::<i32>(),
        parts[1].parse::<i32>(),
        parts[2].parse::<i32>(),
    ) {
        (Ok(y), Ok(m), Ok(d)) => (y, m, d),
        _ => return date.to_string(),
    };
    let jdn = queue_julian_day(y, m, d) + days;
    let (ny, nm, nd) = queue_from_julian_day(jdn);
    format!("{:04}-{:02}-{:02}", ny, nm, nd)
}

/// Format a `YYYY-MM-DD` string as "Mon D" (e.g. "Jun 20"). Falls back to
/// the raw string on malformed input.
fn format_queue_date(date: &str) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return date.to_string();
    }
    let month = match parts[1] {
        "01" => "Jan",
        "02" => "Feb",
        "03" => "Mar",
        "04" => "Apr",
        "05" => "May",
        "06" => "Jun",
        "07" => "Jul",
        "08" => "Aug",
        "09" => "Sep",
        "10" => "Oct",
        "11" => "Nov",
        "12" => "Dec",
        _ => return date.to_string(),
    };
    let day: u32 = match parts[2].parse() {
        Ok(d) => d,
        Err(_) => return date.to_string(),
    };
    format!("{} {}", month, day)
}

/// Render the optional "was {date}" / "pulled in" slip indicator, comparing
/// `due` against `original_due` as `YYYY-MM-DD` strings (zero-padded, so
/// lexicographic comparison is equivalent to chronological comparison).
fn render_queue_slip(due: Option<&str>, original_due: Option<&str>) -> String {
    let (due, original) = match (due, original_due) {
        (Some(d), Some(o)) => (d, o),
        _ => return String::new(),
    };
    if due == original {
        return String::new();
    }
    if due > original {
        format!(
            r#"<span class="c-queue-slip">was {}</span>"#,
            esc(&format_queue_date(original))
        )
    } else {
        r#"<span class="c-queue-slip pulled">pulled in</span>"#.to_string()
    }
}

fn render_queue_row(
    item: &QueueItem,
    bucket: QueueBucket,
    show_dates: bool,
    drift: bool,
    h: &mut String,
) {
    let urgency_class = queue_urgency_class(item, bucket);
    let completed_class = if matches!(item.status, TreeStatus::Completed) {
        " completed"
    } else {
        ""
    };
    let drift_class = if drift { " drift" } else { "" };
    let tag = if item.href.is_some() { "a" } else { "div" };
    let href_attr = item
        .href
        .as_deref()
        .map(|href| format!(r#" href="{}""#, esc(href)))
        .unwrap_or_default();

    h.push_str(&format!(
        r#"<{tag} class="c-queue-row {status} {urgency}{completed}{drift}" data-status="{status_label}"{href}>"#,
        tag = tag,
        status = item.status.class(),
        urgency = urgency_class,
        completed = completed_class,
        drift = drift_class,
        status_label = item.status.label(),
        href = href_attr,
    ));
    h.push_str(r#"<div class="c-queue-stripe"></div>"#);
    h.push_str(r#"<div class="c-queue-main">"#);
    h.push_str(r#"<div class="c-queue-label-line">"#);
    h.push_str(&format!(
        r#"<span class="c-queue-label">{}</span>"#,
        esc(&item.label)
    ));
    if let Some(owner) = &item.owner {
        if !owner.trim().is_empty() {
            h.push_str(&format!(
                r#"<span class="c-queue-owner">{}</span>"#,
                esc(owner)
            ));
        }
    }
    h.push_str("</div>");
    if let Some(detail) = &item.detail {
        if !detail.trim().is_empty() {
            h.push_str(&format!(
                r#"<div class="c-queue-detail">{}</div>"#,
                esc(detail)
            ));
        }
    }
    if !item.tags.is_empty() {
        h.push_str(r#"<div class="c-queue-tags">"#);
        for t in &item.tags {
            let emphasis_class = if t.emphasis { " emphasis" } else { "" };
            h.push_str(&format!(
                r#"<span class="c-queue-tag color-{color}{emphasis}">{label}</span>"#,
                color = t.color.class_suffix(),
                emphasis = emphasis_class,
                label = esc(&t.label),
            ));
        }
        h.push_str("</div>");
    }
    h.push_str("</div>");
    if show_dates {
        if let Some(due) = &item.due {
            h.push_str(r#"<div class="c-queue-date">"#);
            h.push_str(&format!(
                r#"<span class="c-queue-due">{}</span>"#,
                esc(&format_queue_date(due))
            ));
            h.push_str(&render_queue_slip(
                Some(due.as_str()),
                item.original_due.as_deref(),
            ));
            h.push_str("</div>");
        }
    }
    h.push_str(&format!("</{tag}>"));
}

fn priority_queue(
    items: &[QueueItem],
    group_by: QueueGroup,
    show_dates: bool,
    show_counts: bool,
    filterable: bool,
    title: Option<&str>,
) -> Rendered {
    let mut h = String::from(r#"<div class="c-queue">"#);
    if let Some(t) = title {
        h.push_str(&format!(r#"<div class="c-queue-title">{}</div>"#, esc(t)));
    }
    if filterable {
        h.push_str(r#"<div class="c-queue-search"><input type="text" class="c-queue-search-input" placeholder="Filter items…" data-queue-search></div>"#);
    }

    let today = crate::freshness::today_iso();
    let two_week_end = add_days_to_date(&today, 13);
    let eight_week_end = add_days_to_date(&today, 55);

    let bucket_of = |item: &QueueItem| queue_bucket(item, &today, &two_week_end, &eight_week_end);
    let drift_of = |item: &QueueItem| queue_has_drift(item, &today, &two_week_end, &eight_week_end);
    match group_by {
        QueueGroup::None => {
            for item in items {
                let bucket = bucket_of(item);
                render_queue_row(item, bucket, show_dates, drift_of(item), &mut h);
            }
        }
        QueueGroup::Urgency => {
            let order = [
                QueueBucket::Overdue,
                QueueBucket::TwoWeeks,
                QueueBucket::TwoToEight,
                QueueBucket::Later,
                QueueBucket::NoDate,
            ];
            for bucket in order {
                let group_items: Vec<&QueueItem> =
                    items.iter().filter(|it| bucket_of(it) == bucket).collect();
                if group_items.is_empty() {
                    continue;
                }
                let collapse = !matches!(bucket, QueueBucket::Overdue | QueueBucket::TwoWeeks);
                render_queue_group(
                    bucket.urgency_label(),
                    &group_items,
                    show_counts,
                    collapse,
                    |it, h| {
                        render_queue_row(it, bucket, show_dates, drift_of(it), h);
                    },
                    &mut h,
                );
            }
        }
        QueueGroup::Horizon => {
            let order = ["Now", "Next", "Later", "No date"];
            for label in order {
                let group_items: Vec<&QueueItem> = items
                    .iter()
                    .filter(|it| bucket_of(it).horizon_label() == label)
                    .collect();
                if group_items.is_empty() {
                    continue;
                }
                render_queue_group(
                    label,
                    &group_items,
                    show_counts,
                    label != "Now",
                    |it, h| {
                        let bucket = bucket_of(it);
                        render_queue_row(it, bucket, show_dates, drift_of(it), h);
                    },
                    &mut h,
                );
            }
        }
        QueueGroup::Owner => {
            let mut owners: Vec<String> = Vec::new();
            for item in items {
                let owner = item
                    .owner
                    .as_deref()
                    .filter(|o| !o.trim().is_empty())
                    .unwrap_or("Unassigned")
                    .to_string();
                if !owners.contains(&owner) {
                    owners.push(owner);
                }
            }
            for owner in &owners {
                let group_items: Vec<&QueueItem> = items
                    .iter()
                    .filter(|it| {
                        let o = it
                            .owner
                            .as_deref()
                            .filter(|o| !o.trim().is_empty())
                            .unwrap_or("Unassigned");
                        o == owner
                    })
                    .collect();
                if group_items.is_empty() {
                    continue;
                }
                render_queue_group(
                    owner,
                    &group_items,
                    show_counts,
                    owner != &owners[0],
                    |it, h| {
                        let bucket = bucket_of(it);
                        render_queue_row(it, bucket, show_dates, drift_of(it), h);
                    },
                    &mut h,
                );
            }
        }
        QueueGroup::Status => {
            let order = [
                TreeStatus::Priority,
                TreeStatus::Blocked,
                TreeStatus::Active,
                TreeStatus::Upcoming,
                TreeStatus::Default,
                TreeStatus::Completed,
            ];
            let mut first_rendered = false;
            for status in order {
                let group_items: Vec<&QueueItem> = items
                    .iter()
                    .filter(|it| tree_status_eq(it.status, status))
                    .collect();
                if group_items.is_empty() {
                    continue;
                }
                let collapse = first_rendered;
                first_rendered = true;
                render_queue_group(
                    status.label(),
                    &group_items,
                    show_counts,
                    collapse,
                    |it, h| {
                        let bucket = bucket_of(it);
                        render_queue_row(it, bucket, show_dates, drift_of(it), h);
                    },
                    &mut h,
                );
            }
        }
    }

    h.push_str("</div>");
    let mut r = Rendered::new(h);
    if !matches!(group_by, QueueGroup::None) {
        r = r.with_script("queue_collapse");
    }
    if filterable {
        r = r.with_script("queue_filter");
    }
    r
}

/// Render one `c-queue-group` with a header (label + optional count) and a
/// caller-provided closure for rendering each item's row.
fn render_queue_group<F>(
    label: &str,
    items: &[&QueueItem],
    show_counts: bool,
    collapsed: bool,
    mut render_row: F,
    h: &mut String,
) where
    F: FnMut(&QueueItem, &mut String),
{
    if collapsed {
        h.push_str(r#"<div class="c-queue-group collapsed">"#);
    } else {
        h.push_str(r#"<div class="c-queue-group">"#);
    }
    h.push_str(r#"<div class="c-queue-group-header">"#);
    h.push_str(&format!(
        r#"<span class="c-queue-group-label">{}</span>"#,
        esc(&label.to_uppercase())
    ));
    if show_counts {
        h.push_str(&format!(
            r#"<span class="c-queue-group-count">{}</span>"#,
            items.len()
        ));
    }
    h.push_str("</div>");
    for item in items {
        render_row(item, h);
    }
    h.push_str("</div>");
}

// ── Venn ──────────────────────────────────────────

// Greedy word-wrap for overlap labels, which are often full phrases rather
// than single words - long ones need to break across lines or they collide
// with a neighboring pairwise label's text.
fn wrap_venn_label(s: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let candidate_len = if cur.is_empty() {
            word.len()
        } else {
            cur.len() + 1 + word.len()
        };
        if candidate_len > max_chars && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur.push_str(word);
        } else {
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

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

    // Geometry constants - viewBox is sized so the 3-set bounding box leaves
    // ~30-40px of breathing room on every side at default radius. Circles
    // stay at r=108 regardless of layout so 2-set and 3-set look at the same
    // visual scale. Sized ~20% bigger than a bare-minimum fit (r=90) because
    // overlap labels are often full phrases - the extra canvas grows the gap
    // between circles and label text without having to shrink the font.
    let (vb_w, vb_h) = (580.0, 410.0);
    let r = 108.0_f64;

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

    // Set labels - placed outside the central overlap so they read cleanly.
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
    // of the two circles lands too close to the triangle centroid - every
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
        // Pushed further than the circle radius alone would suggest (0.58 vs
        // the ~0.45 that lands right at the lune) because overlap labels are
        // often full phrases, not single words - the extra room keeps two
        // adjacent pairwise labels from running into each other's text.
        if n == 3 && count == 2 {
            if let Some(third_idx) = (0..3).find(|i| !ov.sets.contains(i)) {
                let (tx, ty) = centers[third_idx];
                let dx = lx - tx;
                let dy = ly - ty;
                let mag = (dx * dx + dy * dy).sqrt().max(1.0);
                let push = r * 0.58;
                lx += dx / mag * push;
                ly += dy / mag * push;
            }
        }

        let label = ov.label.as_deref().unwrap_or("");
        if !label.is_empty() {
            let lines = wrap_venn_label(label, 18);
            let line_h = 13.0;
            let start_dy = -(lines.len() as f64 - 1.0) * line_h / 2.0;
            h.push_str(&format!(
                r#"<text class="c-venn-overlap-label" x="{lx:.1}" y="{ly:.1}" text-anchor="middle" dominant-baseline="middle">"#,
            ));
            for (i, line) in lines.iter().enumerate() {
                let dy = if i == 0 { start_dy } else { line_h };
                h.push_str(&format!(
                    r#"<tspan x="{lx:.1}" dy="{dy:.1}">{}</tspan>"#,
                    esc(line),
                ));
            }
            h.push_str("</text>");
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
            r#"<figcaption class="c-blockquote-attribution">- {}</figcaption>"#,
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
    target: Option<u8>,
    thresholds: &std::collections::HashMap<String, SemColor>,
) -> Rendered {
    let clamped = value.min(100);
    let effective_color = if !thresholds.is_empty() {
        resolve_threshold_color(clamped, thresholds).unwrap_or(color)
    } else {
        color
    };
    let color_class = sem_color_class(effective_color);

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
        r#"<div class="c-progress-track" role="progressbar" aria-valuenow="{v}" aria-valuemin="0" aria-valuemax="100"><div class="c-progress-fill c-progress-fill-{color}" style="--progress: {v}%"></div>"#,
        v = clamped, color = color_class
    ));
    if let Some(t) = target {
        let t_clamped = t.min(100);
        h.push_str(&format!(
            r#"<div class="c-progress-target" style="left: {}%" title="Target: {}%"></div>"#,
            t_clamped, t_clamped
        ));
    }
    h.push_str("</div>");
    if let Some(d) = detail {
        h.push_str(&format!(
            r#"<div class="c-progress-detail">{}</div>"#,
            esc(d)
        ));
    }
    h.push_str("</div>");
    Rendered::new(h)
}

fn resolve_threshold_color(
    value: u8,
    thresholds: &std::collections::HashMap<String, SemColor>,
) -> Option<SemColor> {
    let mut best: Option<(u8, SemColor)> = None;
    for (key, color) in thresholds {
        if let Ok(threshold) = key.parse::<u8>() {
            if value >= threshold {
                match best {
                    Some((prev, _)) if threshold > prev => {
                        best = Some((threshold, *color));
                    }
                    None => {
                        best = Some((threshold, *color));
                    }
                    _ => {}
                }
            }
        }
    }
    best.map(|(_, c)| c)
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

fn count_all_reports(person: &OrgPerson) -> usize {
    person
        .reports
        .iter()
        .fold(0, |acc, r| acc + 1 + count_all_reports(r))
}

fn count_all_people(people: &[OrgPerson]) -> usize {
    people
        .iter()
        .fold(0, |acc, p| acc + 1 + count_all_reports(p))
}

fn render_org_person(person: &OrgPerson, depth: usize, auto_depth: usize, html: &mut String) {
    let has_kids = !person.reports.is_empty();
    let open = depth < auto_depth;
    let total = count_all_reports(person);
    let color_class = match person.color {
        SemColor::Default => "",
        SemColor::Green => " c-org-node--green",
        SemColor::Yellow => " c-org-node--yellow",
        SemColor::Red => " c-org-node--red",
        SemColor::Teal => " c-org-node--teal",
    };

    html.push_str("<div class=\"c-org-branch\">");
    html.push_str(&format!("<div class=\"c-org-node{}", color_class));
    if has_kids {
        html.push_str(" c-org-node--parent");
    }
    if open && has_kids {
        html.push_str(" c-org-node--open");
    }
    html.push_str("\">");
    html.push_str("<div class=\"c-org-node-name\">");
    html.push_str(&esc(&person.name));
    html.push_str("</div>");
    if let Some(ref title) = person.title {
        html.push_str("<div class=\"c-org-node-title\">");
        html.push_str(&esc(title));
        html.push_str("</div>");
    }
    if !person.tags.is_empty() {
        html.push_str("<div class=\"c-org-node-tags\">");
        for tag in &person.tags {
            html.push_str(&format!(
                "<span class=\"c-badge c-badge-{}\">{}</span>",
                tag.color.class_suffix(),
                esc(&tag.label)
            ));
        }
        html.push_str("</div>");
    }
    if person.email.is_some() || person.linkedin.is_some() {
        html.push_str("<div class=\"c-org-node-contact\">");
        if let Some(ref email) = person.email {
            html.push_str(&format!(
                "<a href=\"mailto:{}\" title=\"{}\" class=\"c-org-contact-link\">✉</a>",
                esc(email),
                esc(email)
            ));
        }
        if let Some(ref linkedin) = person.linkedin {
            html.push_str(&format!(
                "<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\" title=\"LinkedIn\" class=\"c-org-contact-link\">in</a>",
                esc(linkedin)
            ));
        }
        html.push_str("</div>");
    }
    if has_kids && !open {
        let plural = if total != 1 { "s" } else { "" };
        html.push_str(&format!(
            "<div class=\"c-org-node-count\">{} report{}</div>",
            total, plural
        ));
    }
    html.push_str("</div>");

    if has_kids && open {
        html.push_str("<div class=\"c-org-children\">");
        for r in &person.reports {
            render_org_person(r, depth + 1, auto_depth, html);
        }
        html.push_str("</div>");
    }
    html.push_str("</div>");
}

// ── Aside ────────────────────────────────────────

fn aside(body: &str, base: &str) -> Rendered {
    Rendered::new(format!(
        r#"<div class="c-aside"><div class="c-aside-body c-markdown">{}</div></div>"#,
        parse_markdown(body, base)
    ))
}

// ── Rule List ────────────────────────────────────

fn rule_list(items: &[RuleItem]) -> Rendered {
    let mut h = String::from(r#"<div class="c-rule-list">"#);
    for item in items {
        let color_class = if matches!(item.color, SemColor::Default) {
            String::new()
        } else {
            format!(" c-rule-{}", item.color.class_suffix())
        };
        h.push_str(&format!(
            r#"<div class="c-rule-item{}"><span class="c-rule-label">{}</span><span class="c-rule-body">{}</span></div>"#,
            color_class,
            esc(&item.label),
            esc(&item.body),
        ));
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Gauge ────────────────────────────────────────

fn gauge(items: &[GaugeItem], title: Option<&str>, columns: u32, max: f64) -> Rendered {
    let max_val = if max <= 0.0 { 100.0 } else { max };
    let mut h = format!(
        r#"<div class="c-gauge-grid" style="--gauge-cols: {}">"#,
        columns
    );
    if let Some(t) = title {
        h.push_str(&format!(r#"<div class="c-gauge-title">{}</div>"#, esc(t)));
    }
    for item in items {
        let pct = (item.value / max_val).min(1.0);
        let r = 26.0;
        let circumference = 2.0 * std::f64::consts::PI * r;
        let dash = circumference * pct;
        let gap = circumference - dash;
        let color = item.color.hex();
        h.push_str(r#"<div class="c-gauge-item">"#);
        h.push_str(&format!(
            r#"<svg class="c-gauge-ring" viewBox="0 0 64 64"><circle cx="32" cy="32" r="{r}" fill="none" stroke="rgba(255,255,255,0.08)" stroke-width="6"/><circle cx="32" cy="32" r="{r}" fill="none" stroke="{color}" stroke-width="6" stroke-dasharray="{dash:.1} {gap:.1}" stroke-dashoffset="{offset:.1}" stroke-linecap="round" transform="rotate(-90 32 32)"/><text x="32" y="32" text-anchor="middle" dominant-baseline="central" class="c-gauge-value">{val}</text></svg>"#,
            r = r,
            color = color,
            dash = dash,
            gap = gap,
            offset = 0.0,
            val = format_gauge_value(item.value),
        ));
        h.push_str(&format!(
            r#"<div class="c-gauge-label">{}</div>"#,
            esc(&item.label)
        ));
        h.push_str("</div>");
    }
    h.push_str("</div>");
    Rendered::new(h)
}

fn format_gauge_value(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{:.1}", v)
    }
}

fn render_org_chart(
    title: &Option<String>,
    people: &[OrgPerson],
    default_open_depth: Option<u32>,
) -> Rendered {
    let total = count_all_people(people);
    let auto_depth = match default_open_depth {
        Some(d) => d as usize,
        None => {
            if total <= 15 {
                99
            } else if total <= 40 {
                2
            } else {
                1
            }
        }
    };

    let mut html = String::from("<div class=\"c-org-chart\">");
    if let Some(ref t) = title {
        html.push_str("<div class=\"c-org-chart-title\">");
        html.push_str(&esc(t));
        html.push_str("</div>");
    }
    html.push_str("<div class=\"c-org-root\">");
    let multi_root = people.len() > 1;
    if multi_root {
        html.push_str("<div class=\"c-org-children c-org-children--root\">");
    }
    for p in people {
        render_org_person(p, 0, auto_depth, &mut html);
    }
    if multi_root {
        html.push_str("</div>");
    }
    html.push_str("</div></div>");
    Rendered::new(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_scale_noop_when_unset() {
        let r = apply_scale(
            Rendered::new("<figure class=\"c-chart\"></figure>".into()),
            None,
        );
        assert_eq!(r.html, "<figure class=\"c-chart\"></figure>");
    }

    #[test]
    fn apply_scale_wraps_and_clamps() {
        let r = apply_scale(
            Rendered::new("<figure class=\"c-chart\"></figure>".into()),
            Some(0.5),
        );
        assert_eq!(
            r.html,
            "<div class=\"c-chart-scale\" style=\"--kz-scale: 0.5\"><figure class=\"c-chart\"></figure></div>"
        );

        let too_small = apply_scale(Rendered::new("<x></x>".into()), Some(0.0));
        assert!(too_small.html.contains("--kz-scale: 0.1"));

        let too_big = apply_scale(Rendered::new("<x></x>".into()), Some(10.0));
        assert!(too_big.html.contains("--kz-scale: 2"));
    }

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
        assert_eq!(render_cell("One - two"), "One - two");
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

    #[test]
    fn add_days_to_date_crosses_month_and_year_boundaries() {
        assert_eq!(add_days_to_date("2026-07-28", 6), "2026-08-03");
        assert_eq!(add_days_to_date("2026-12-30", 5), "2027-01-04");
        assert_eq!(add_days_to_date("2026-02-27", 2), "2026-03-01");
    }

    #[test]
    fn format_queue_date_drops_leading_zero_on_day() {
        assert_eq!(format_queue_date("2026-06-20"), "Jun 20");
        assert_eq!(format_queue_date("2026-07-09"), "Jul 9");
    }

    fn item(due: Option<&str>, original_due: Option<&str>, status: TreeStatus) -> QueueItem {
        QueueItem {
            label: "Item".to_string(),
            detail: None,
            due: due.map(|s| s.to_string()),
            original_due: original_due.map(|s| s.to_string()),
            owner: None,
            status,
            tags: Vec::new(),
            href: None,
            horizon: None,
        }
    }

    #[test]
    fn queue_bucket_classifies_relative_to_today() {
        let today = "2026-07-28";
        let two_week_end = add_days_to_date(today, 13); // 2026-08-10
        let eight_week_end = add_days_to_date(today, 55); // 2026-09-21

        let overdue = item(Some("2026-07-20"), None, TreeStatus::Active);
        assert!(matches!(
            queue_bucket(&overdue, today, &two_week_end, &eight_week_end),
            QueueBucket::Overdue
        ));

        let due_today = item(Some(today), None, TreeStatus::Active);
        assert!(matches!(
            queue_bucket(&due_today, today, &two_week_end, &eight_week_end),
            QueueBucket::TwoWeeks
        ));

        let two_to_eight = item(Some("2026-08-20"), None, TreeStatus::Active);
        assert!(matches!(
            queue_bucket(&two_to_eight, today, &two_week_end, &eight_week_end),
            QueueBucket::TwoToEight
        ));

        let later = item(Some("2026-10-01"), None, TreeStatus::Active);
        assert!(matches!(
            queue_bucket(&later, today, &two_week_end, &eight_week_end),
            QueueBucket::Later
        ));

        let no_date = item(None, None, TreeStatus::Active);
        assert!(matches!(
            queue_bucket(&no_date, today, &two_week_end, &eight_week_end),
            QueueBucket::NoDate
        ));

        let completed_overdue = item(Some("2026-07-01"), None, TreeStatus::Completed);
        assert!(matches!(
            queue_bucket(&completed_overdue, today, &two_week_end, &eight_week_end),
            QueueBucket::TwoWeeks
        ));
    }

    #[test]
    fn queue_urgency_class_blocked_overrides_bucket() {
        let blocked = item(Some("2026-09-01"), None, TreeStatus::Blocked);
        assert_eq!(
            queue_urgency_class(&blocked, QueueBucket::Later),
            "urgency-blocked"
        );
        let active = item(Some("2026-09-01"), None, TreeStatus::Active);
        assert_eq!(
            queue_urgency_class(&active, QueueBucket::Later),
            "urgency-track"
        );
    }

    #[test]
    fn render_queue_slip_detects_pull_and_push() {
        assert_eq!(
            render_queue_slip(Some("2026-07-09"), Some("2026-06-20")),
            r#"<span class="c-queue-slip">was Jun 20</span>"#
        );
        assert_eq!(
            render_queue_slip(Some("2026-06-01"), Some("2026-06-20")),
            r#"<span class="c-queue-slip pulled">pulled in</span>"#
        );
        assert_eq!(
            render_queue_slip(Some("2026-06-20"), Some("2026-06-20")),
            ""
        );
        assert_eq!(render_queue_slip(None, Some("2026-06-20")), "");
    }

    #[test]
    fn priority_queue_groups_by_urgency_with_counts() {
        std::env::set_var("KAZAM_TODAY", "2026-07-28");
        let items = vec![
            item(Some("2026-07-20"), None, TreeStatus::Active),
            item(None, None, TreeStatus::Active),
        ];
        let r = priority_queue(
            &items,
            QueueGroup::Urgency,
            true,
            true,
            false,
            Some("Tracker"),
        );
        std::env::remove_var("KAZAM_TODAY");
        assert!(r.html.contains("c-queue-title\">Tracker"));
        assert!(r.html.contains("OVERDUE"));
        assert!(r.html.contains("NO DATE"));
        assert!(r.html.contains("urgency-overdue"));
        assert!(r.html.contains("urgency-none"));
        assert!(r
            .html
            .contains(r#"<span class="c-queue-group-count">1</span>"#));
    }

    #[test]
    fn priority_queue_none_grouping_has_no_group_headers() {
        std::env::set_var("KAZAM_TODAY", "2026-07-28");
        let items = vec![item(Some("2026-07-20"), None, TreeStatus::Active)];
        let r = priority_queue(&items, QueueGroup::None, true, true, false, None);
        std::env::remove_var("KAZAM_TODAY");
        assert!(!r.html.contains("c-queue-group-header"));
        assert!(r.html.contains("c-queue-row"));
    }

    #[test]
    fn horizon_field_deserializes_from_yaml() {
        let yaml = "label: Test\nhorizon: later\n";
        let item: QueueItem = serde_yaml::from_str(yaml).unwrap();
        assert!(item.horizon.is_some(), "horizon should deserialize");
        assert!(matches!(item.horizon.unwrap(), QueueHorizon::Later));
    }

    #[test]
    fn horizon_in_component_deserializes() {
        let yaml = "type: priority_queue\nitems:\n  - label: Test\n    due: \"2026-07-30\"\n    horizon: later\n  - label: Dateless\n    horizon: next\n";
        let comp: Component = serde_yaml::from_str(yaml).unwrap();
        if let Component::PriorityQueue { items, .. } = &comp {
            assert!(items[0].horizon.is_some(), "item 0 horizon should be Some");
            assert!(items[1].horizon.is_some(), "item 1 horizon should be Some");
        } else {
            panic!("Expected PriorityQueue");
        }
    }

    #[test]
    fn explicit_horizon_overrides_date_bucket() {
        let today = "2026-07-28";
        let two_week_end = add_days_to_date(today, 13);
        let eight_week_end = add_days_to_date(today, 55);
        let mut it = item(Some("2026-07-30"), None, TreeStatus::Active);
        it.horizon = Some(QueueHorizon::Later);
        let bucket = queue_bucket(&it, today, &two_week_end, &eight_week_end);
        assert!(
            matches!(bucket, QueueBucket::Later),
            "explicit horizon should override date"
        );
        assert!(
            queue_has_drift(&it, today, &two_week_end, &eight_week_end),
            "should detect drift"
        );
    }
}

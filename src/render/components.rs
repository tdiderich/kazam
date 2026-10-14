use pulldown_cmark::{html as md_html, Event, Options, Parser as MdParser, Tag};

use super::{charts, esc, resolve_href, safe_href, slug, Rendered};
use crate::icons;
use crate::types::*;

pub fn render(c: &Component, base: &str, config: &SiteConfig) -> Rendered {
    let rendered = match c {
        Component::Header {
            title,
            subtitle,
            eyebrow,
            align,
            id,
            ..
        } => header(title, subtitle, eyebrow, *align, id.as_deref()),
        Component::HeroBanner {
            title,
            eyebrow,
            subtitle,
            buttons,
            ..
        } => hero_banner(title, eyebrow, subtitle, buttons.as_deref(), base),
        Component::Meta { fields, .. } => meta(fields),
        Component::CardGrid {
            cards,
            min_width,
            connector,
            ..
        } => card_grid(cards, *min_width, *connector, base),
        Component::SelectableGrid {
            cards,
            interaction,
            connector,
            ..
        } => selectable_grid(cards, *interaction, *connector, base),
        Component::Timeline { items, .. } => timeline(items),
        Component::StatGrid { stats, columns, .. } => stat_grid(stats, *columns),
        Component::BeforeAfter {
            items,
            before_label,
            after_label,
            ..
        } => before_after(items, before_label.as_deref(), after_label.as_deref()),
        Component::SplitCompare { left, right, .. } => split_compare(left, right),
        Component::Steps {
            items, numbered, ..
        } => steps(items, *numbered),
        Component::Markdown { body, .. } => markdown(body, base),
        Component::Table {
            columns,
            rows,
            filterable,
            summary,
            ..
        } => table(columns, rows, *filterable, summary.as_ref()),
        Component::Callout {
            variant,
            title,
            body,
            links,
            ..
        } => callout(*variant, title, body, links.as_deref(), base),
        Component::Code { language, code, .. } => code_block(language, code),
        Component::Tabs { tabs, .. } => tabs_component(tabs, base, config),
        Component::Section {
            heading,
            eyebrow,
            components,
            align,
            id,
            ..
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
            ..
        } => columns_component(columns, *equal_heights, base, config),
        Component::Grid {
            columns,
            rows,
            gap,
            children,
            id,
            ..
        } => grid(*columns, *rows, *gap, children, id.as_deref(), base, config),
        Component::Box {
            title,
            tag,
            body,
            components,
            color,
            hex,
            border,
            id,
            ..
        } => box_component(
            title.as_deref(),
            tag.as_deref(),
            body.as_deref(),
            components,
            *color,
            hex.as_deref(),
            *border,
            id.as_deref(),
            base,
            config,
        ),
        Component::Connector {
            label,
            direction,
            color,
            hex,
            ..
        } => connector(label.as_deref(), *direction, *color, hex.as_deref()),
        Component::Sequence {
            target, steps, id, ..
        } => sequence(target, steps, id.as_deref(), base),
        Component::Accordion { items, .. } => accordion(items, base, config),
        Component::EventTimeline {
            events,
            default_filter,
            show_filter_toggle,
            limit,
            filter_by,
            group_by,
            ..
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
            ..
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
            default_view,
            ..
        } => venn(sets, overlaps, title.as_deref(), *default_view),
        Component::Image {
            src,
            alt,
            caption,
            max_width,
            align,
            ..
        } => image(src, alt, caption, *max_width, *align, base),
        Component::Embed {
            src, title, aspect, ..
        } => embed(src, title, aspect),
        Component::Resources { items, .. } => resources(items),
        // Phase 1 additions
        Component::Badge { label, color, .. } => badge(label, *color),
        Component::Tag { label, color, .. } => tag(label, *color),
        Component::Divider { label, .. } => divider(label),
        Component::Kbd { keys, .. } => kbd(keys),
        Component::Status { label, color, .. } => status(label, *color),
        Component::Breadcrumb { items, .. } => breadcrumb(items, base),
        Component::ButtonGroup { buttons, .. } => button_group(buttons, base),
        Component::DefinitionList { items, .. } => definition_list(items),
        Component::Blockquote {
            body, attribution, ..
        } => blockquote(body, attribution),
        Component::Avatar {
            name,
            src,
            size,
            subtitle,
            ..
        } => avatar(name, src, *size, subtitle),
        Component::AvatarGroup {
            avatars, size, max, ..
        } => avatar_group(avatars, *size, *max),
        Component::ProgressBar {
            value,
            label,
            color,
            detail,
            target,
            thresholds,
            ..
        } => progress_bar(*value, label, color, detail, *target, thresholds),
        Component::EmptyState {
            title,
            body,
            action,
            icon,
            ..
        } => empty_state(title, body, action, icon, base),
        Component::Icon {
            name, size, color, ..
        } => icon_component(name, *size, *color),
        Component::Chart {
            kind,
            title,
            height,
            x_label,
            y_label,
            orientation,
            data,
            series,
            ..
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
        Component::RoleMap { title, .. } => role_map(title.as_deref(), config, base),
        Component::Sankey {
            title,
            height,
            flows,
            colors,
            ..
        } => charts::render_sankey(title, *height, flows, colors),
        Component::Radar {
            title,
            height,
            axes,
            curves,
            max,
            ..
        } => charts::render_radar(title, *height, axes, curves, *max),
        Component::Quadrant {
            title,
            height,
            x_axis,
            y_axis,
            quadrants,
            points,
            ..
        } => charts::render_quadrant(title, *height, x_axis, y_axis, quadrants, points),
        Component::Architecture {
            title,
            height,
            direction,
            nodes,
            connections,
            ..
        } => charts::render_architecture(title, *height, *direction, nodes, connections),
        Component::Pipeline {
            title,
            height,
            inputs,
            stages,
            outputs,
            context,
            ..
        } => charts::render_pipeline(title, *height, inputs, stages, outputs, context),
        Component::Graph {
            title,
            height,
            direction,
            nodes,
            edges,
            groups,
            row_labels,
            ..
        } => charts::render_graph(title, *height, *direction, nodes, edges, groups, row_labels),
        Component::OrgChart {
            title,
            people,
            default_open_depth,
            ..
        } => render_org_chart(title, people, *default_open_depth),
        Component::Aside { body, .. } => aside(body, base),
        Component::RuleList { items, .. } => rule_list(items),
        Component::Gauge {
            items,
            title,
            columns,
            max,
            ..
        } => gauge(items, title.as_deref(), *columns, *max),
        Component::PriorityQueue {
            items,
            group_by,
            show_dates,
            show_counts,
            filterable,
            title,
            ..
        } => priority_queue(
            items,
            *group_by,
            *show_dates,
            *show_counts,
            *filterable,
            title.as_deref(),
        ),
        Component::TierSummary {
            tiers,
            count_label,
            share_label,
            id,
            ..
        } => tier_summary(
            tiers,
            count_label.as_deref(),
            share_label.as_deref(),
            id.as_deref(),
            base,
        ),
        Component::HealthBreakdown { items, id, .. } => health_breakdown(items, id.as_deref()),
        Component::NextSteps {
            steps,
            accounts,
            owners,
            order,
            title,
            id,
            ..
        } => next_steps(
            steps,
            accounts,
            owners,
            *order,
            title.as_deref(),
            id.as_deref(),
            base,
        ),
        Component::JourneyTrack {
            stages,
            detours,
            current,
            touchpoints,
            history,
            default_view,
            title,
            start,
            end,
            end_label,
            id,
            ..
        } => journey_track(
            stages,
            detours,
            current.as_ref(),
            touchpoints,
            history.as_ref(),
            *default_view,
            title.as_deref(),
            start,
            end,
            end_label.as_deref(),
            id.as_deref(),
        ),
    };
    apply_motion(apply_scale(rendered, c.scale()), c.animate())
}

pub(super) fn sem_color_class(c: SemColor) -> &'static str {
    c.class_suffix()
}

/// Wraps a component in a motion carrier when it opts into `animate`. The
/// wrapper only does anything under `body.kz-motion`, so pages that never
/// turn motion on render the same HTML plus one inert div.
fn apply_motion(mut r: Rendered, animate: Option<Animate>) -> Rendered {
    if let Some(a) = animate.filter(|a| *a != Animate::None) {
        r.html = format!(
            r#"<div class="kz-anim" data-animate="{}">{}</div>"#,
            a.name(),
            r.html
        );
    }
    r
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

fn card_grid(
    cards: &[Card],
    min_width: Option<u32>,
    connector: CardConnector,
    base: &str,
) -> Rendered {
    let mw = min_width.unwrap_or(320);
    let is_arrow = matches!(connector, CardConnector::Arrow);
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
    connector: CardConnector,
    base: &str,
) -> Rendered {
    let interaction_attr = match interaction {
        Interaction::SingleSelect => "single_select",
        Interaction::MultiSelect => "multi_select",
        Interaction::None => "none",
    };
    let is_arrow = matches!(connector, CardConnector::Arrow);

    let mut h = format!(
        r#"<div class="c-selectable-grid" data-selectable-grid data-interaction="{interaction_attr}">"#
    );

    if matches!(connector, CardConnector::DotsLine) {
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
        // Raw HTML in a body would land in the page unescaped. Downgrade it
        // to text so markup shows literally instead of executing.
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
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

// ── Grid / Box / Connector ────────────────────────

fn grid(
    columns: u32,
    rows: Option<u32>,
    gap: Option<u32>,
    children: &[GridChild],
    id: Option<&str>,
    base: &str,
    config: &SiteConfig,
) -> Rendered {
    let mut r = Rendered::default();
    let id_attr = id
        .map(|i| format!(r#" id="{}""#, esc(i)))
        .unwrap_or_default();
    let mut style = format!("--cols: {}", columns.max(1));
    if let Some(rows) = rows {
        style.push_str(&format!("; --rows: {}", rows));
    }
    if let Some(gap) = gap {
        style.push_str(&format!("; --gap: {}px", gap));
    }
    r.html.push_str(&format!(
        r#"<div{} class="c-grid" style="{}">"#,
        id_attr, style
    ));
    for child in children {
        r.html.push_str(&format!(
            r#"<div class="c-grid-cell" style="{}">"#,
            grid_cell_style(child)
        ));
        r.extend(render(&child.component, base, config));
        r.html.push_str("</div>");
    }
    r.html.push_str("</div>");
    r
}

/// Explicit `grid-column`/`grid-row` when the child pins itself, span-only
/// otherwise so CSS grid auto-flow places it.
pub(crate) fn grid_cell_style(child: &GridChild) -> String {
    let mut parts = Vec::new();
    match child.col {
        Some(c) => parts.push(format!(
            "grid-column: {} / span {}",
            c,
            child.colspan.max(1)
        )),
        None if child.colspan > 1 => parts.push(format!("grid-column: span {}", child.colspan)),
        None => {}
    }
    match child.row {
        Some(r) => parts.push(format!("grid-row: {} / span {}", r, child.rowspan.max(1))),
        None if child.rowspan > 1 => parts.push(format!("grid-row: span {}", child.rowspan)),
        None => {}
    }
    parts.join("; ")
}

#[allow(clippy::too_many_arguments)]
fn box_component(
    title: Option<&str>,
    tag: Option<&str>,
    body: Option<&str>,
    comps: &[Component],
    color: SemColor,
    hex: Option<&str>,
    border: BorderStyle,
    id: Option<&str>,
    base: &str,
    config: &SiteConfig,
) -> Rendered {
    let mut r = Rendered::default();
    let id_attr = id
        .map(|i| format!(r#" id="{}""#, esc(i)))
        .unwrap_or_default();
    let accent = crate::types::resolve_hex(hex, color);
    r.html.push_str(&format!(
        r#"<div{} class="c-box c-box-{} c-box-border-{}" style="--box-accent: {}">"#,
        id_attr,
        color.class_suffix(),
        border.class_suffix(),
        accent
    ));
    if title.is_some() || tag.is_some() {
        r.html.push_str(r#"<div class="c-box-h">"#);
        if let Some(t) = title {
            r.html
                .push_str(&format!(r#"<b class="c-box-title">{}</b>"#, esc(t)));
        }
        if let Some(t) = tag {
            r.html
                .push_str(&format!(r#"<span class="c-box-tag">{}</span>"#, esc(t)));
        }
        r.html.push_str("</div>");
    }
    if let Some(b) = body {
        r.html.push_str(&format!(
            r#"<div class="c-box-body">{}</div>"#,
            parse_markdown(b, base)
        ));
    }
    for c in comps {
        r.extend(render(c, base, config));
    }
    r.html.push_str("</div>");
    r
}

fn connector(
    label: Option<&str>,
    direction: Direction,
    color: SemColor,
    hex: Option<&str>,
) -> Rendered {
    let mut r = Rendered::default();
    let accent = hex
        .filter(|h| crate::types::valid_hex_color(h))
        .map(|h| format!(r#" style="--connector-accent: {}""#, h))
        .unwrap_or_else(|| match color {
            SemColor::Default => String::new(),
            c => format!(r#" style="--connector-accent: {}""#, c.hex()),
        });
    r.html.push_str(&format!(
        r#"<div class="c-connector c-connector-{}"{}>"#,
        direction.class_suffix(),
        accent
    ));
    if let Some(l) = label {
        r.html.push_str(&format!(
            r#"<span class="c-connector-label">{}</span>"#,
            esc(l)
        ));
    }
    r.html.push_str("</div>");
    r
}

// ── Sequence ──────────────────────────────────────

fn sequence(target: &str, steps: &[SeqStep], id: Option<&str>, base: &str) -> Rendered {
    let mut h = String::new();
    let id_attr = id
        .map(|i| format!(r#" id="{}""#, esc(i)))
        .unwrap_or_default();
    h.push_str(&format!(
        r#"<div{} class="c-sequence" data-sequence data-target="{}" tabindex="0" aria-label="Walkthrough">"#,
        id_attr,
        esc(target)
    ));
    h.push_str(r#"<div class="c-seq-bar">"#);
    h.push_str(r#"<button type="button" class="c-seq-btn" data-seq-prev aria-label="Previous step">&larr;</button>"#);
    h.push_str(&format!(
        r#"<span class="c-seq-count"><span data-seq-index>1</span> / {}</span>"#,
        steps.len().max(1)
    ));
    h.push_str(r#"<button type="button" class="c-seq-btn" data-seq-next aria-label="Next step">&rarr;</button>"#);
    h.push_str(r#"<button type="button" class="c-seq-btn c-seq-reset" data-seq-reset aria-label="Show everything">Show all</button>"#);
    h.push_str("</div>");
    h.push_str(r#"<div class="c-seq-steps">"#);
    for (i, step) in steps.iter().enumerate() {
        let hidden = if i == 0 { "" } else { " hidden" };
        h.push_str(&format!(
            r#"<div class="c-seq-step" data-highlight="{}"{}>"#,
            esc(&step.highlight.join(" ")),
            hidden
        ));
        if let Some(n) = &step.note {
            h.push_str(&format!(
                r#"<div class="c-seq-note c-markdown">{}</div>"#,
                parse_markdown(n, base)
            ));
        }
        h.push_str("</div>");
    }
    h.push_str("</div></div>");
    Rendered::new(h).with_script("sequence")
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

// Approximate glyph advance widths (em units) for the UI font. Good enough to
// pick a font size that keeps a label inside its circle without a DOM to
// measure against. Mirrors `vennTextWidth` in the React SDK renderer.
fn venn_text_width(text: &str, font_size: f64, weight: f64) -> f64 {
    let mut w = 0.0;
    for ch in text.chars() {
        w += match ch {
            'A'..='Z' => 0.68,
            '0'..='9' => 0.6,
            'a'..='z' => 0.54,
            ' ' | ',' | '.' | '\'' => 0.3,
            _ => 0.5,
        };
    }
    w * font_size * weight
}

struct VennFit {
    lines: Vec<String>,
    font_size: f64,
}

// Fit a set label into `avail` px: shrink from max toward min, and if breaking
// at word boundaries into 2 or 3 lines ("INSPECTOR" over "(8,726)", or
// "CROWDSTRIKE" / "FALCON" / "(4,219)") keeps the text larger, use that. Ties
// go to fewer lines.
fn venn_fit_label(text: &str, avail: f64, max_size: f64, min_size: f64, weight: f64) -> VennFit {
    let size_for = |lines: &[String]| -> f64 {
        let widest = lines
            .iter()
            .map(|l| venn_text_width(l, 1.0, weight))
            .fold(0.001_f64, f64::max);
        (avail / widest).min(max_size)
    };
    let single = vec![text.trim().to_string()];
    let mut best_lines = single.clone();
    let mut best_size = size_for(&single);
    if best_size >= max_size {
        return VennFit {
            lines: single,
            font_size: max_size,
        };
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut consider = |lines: Vec<String>| {
        let s = size_for(&lines);
        if s > best_size + 0.01 || ((s - best_size).abs() <= 0.01 && lines.len() < best_lines.len())
        {
            best_lines = lines;
            best_size = s;
        }
    };
    for i in 1..words.len() {
        consider(vec![words[..i].join(" "), words[i..].join(" ")]);
        for j in (i + 1)..words.len() {
            consider(vec![
                words[..i].join(" "),
                words[i..j].join(" "),
                words[j..].join(" "),
            ]);
        }
    }
    VennFit {
        lines: best_lines,
        font_size: best_size.clamp(min_size, max_size),
    }
}

// Word-wrap an overlap label into an `avail` x `avail_h` box. Tries the largest
// font first and steps down until both the widest line and the stacked height
// fit - so a phrase shrinks a little rather than towering out of a narrow lune.
// Falls back to the minimum size (tspans squeeze any line still too wide).
fn venn_wrap_label(
    text: &str,
    avail: f64,
    avail_h: f64,
    max_size: f64,
    min_size: f64,
    weight: f64,
) -> VennFit {
    let words: Vec<&str> = text.split_whitespace().collect();
    let wrap_at = |size: f64| -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for word in &words {
            let candidate = if cur.is_empty() {
                (*word).to_string()
            } else {
                format!("{cur} {word}")
            };
            if !cur.is_empty() && venn_text_width(&candidate, size, weight) > avail {
                lines.push(std::mem::take(&mut cur));
                cur.push_str(word);
            } else {
                cur = candidate;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        if lines.is_empty() {
            lines.push(text.to_string());
        }
        lines
    };
    let mut size = max_size;
    while size >= min_size {
        let lines = wrap_at(size);
        let widest = lines
            .iter()
            .map(|l| venn_text_width(l, size, weight))
            .fold(0.0_f64, f64::max);
        if widest <= avail && lines.len() as f64 * size * 1.2 <= avail_h {
            return VennFit {
                lines,
                font_size: size,
            };
        }
        size -= 0.5;
    }
    VennFit {
        lines: wrap_at(min_size),
        font_size: min_size,
    }
}

// Emit fitted lines as centered tspans. A line that still overflows at the
// minimum size is squeezed with textLength so it never escapes its region.
fn venn_tspans(h: &mut String, fit: &VennFit, x: f64, avail: f64, weight: f64) {
    let line_h = fit.font_size * 1.2;
    let start_dy = -(fit.lines.len() as f64 - 1.0) * line_h / 2.0;
    for (i, line) in fit.lines.iter().enumerate() {
        let dy = if i == 0 { start_dy } else { line_h };
        let squeeze = if venn_text_width(line, fit.font_size, weight) > avail {
            format!(r#" textLength="{avail:.1}" lengthAdjust="spacingAndGlyphs""#)
        } else {
            String::new()
        };
        h.push_str(&format!(
            r#"<tspan x="{x:.1}" dy="{dy:.1}"{squeeze}>{}</tspan>"#,
            esc(line),
        ));
    }
}

// Split "NAME (count)" into its parts for the matrix view. Labels without a
// trailing parenthetical keep the whole label as the name and no count.
fn venn_split_label(label: &str) -> (String, String) {
    let t = label.trim();
    if t.ends_with(')') {
        if let Some(open) = t.rfind('(') {
            let name = t[..open].trim();
            let count = t[open + 1..t.len() - 1].trim();
            if !name.is_empty() && !count.contains(['(', ')']) {
                return (name.to_string(), count.to_string());
            }
        }
    }
    (t.to_string(), String::new())
}

fn venn(
    sets: &[VennSet],
    overlaps: &[VennOverlap],
    title: Option<&str>,
    default_view: VennView,
) -> Rendered {
    // Supported: 1-, 2- or 3-set venn. Extra sets are dropped from the diagram
    // (validate flags them) so a malformed YAML doesn't break the page.
    let n = sets.len();
    let mut h = String::from(r#"<div class="c-venn" data-venn>"#);

    if n == 0 {
        if let Some(t) = title {
            h.push_str(&format!(r#"<div class="c-venn-title">{}</div>"#, esc(t)));
        }
        h.push_str(r#"<div class="c-venn-empty">No sets provided.</div></div>"#);
        return Rendered::new(h);
    }

    let can_toggle = n >= 2;
    let table_first = can_toggle && default_view == VennView::Table;

    // Header row: title centered, view toggle pinned to the right.
    if title.is_some() || can_toggle {
        h.push_str(r#"<div class="c-venn-head">"#);
        if let Some(t) = title {
            h.push_str(&format!(r#"<div class="c-venn-title">{}</div>"#, esc(t)));
        }
        if can_toggle {
            let (venn_active, table_active) = if table_first {
                ("", " active")
            } else {
                (" active", "")
            };
            h.push_str(&format!(
                concat!(
                    r#"<div class="c-venn-toggle" role="group" aria-label="Venn view switcher">"#,
                    r#"<button type="button" class="{va}" data-venn-view="venn" aria-pressed="{vp}" title="Venn diagram">"#,
                    r#"<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><circle cx="6" cy="8" r="4.5" fill="none" stroke="currentColor" stroke-width="1.6"/><circle cx="10" cy="8" r="4.5" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>"#,
                    r#"</button>"#,
                    r#"<button type="button" class="{ta}" data-venn-view="table" aria-pressed="{tp}" title="Overlap matrix">"#,
                    r#"<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><rect x="2" y="2" width="5" height="5" rx="1" fill="currentColor"/><rect x="9" y="2" width="5" height="5" rx="1" fill="currentColor"/><rect x="2" y="9" width="5" height="5" rx="1" fill="currentColor"/><rect x="9" y="9" width="5" height="5" rx="1" fill="currentColor"/></svg>"#,
                    r#"</button></div>"#,
                ),
                va = venn_active.trim(),
                vp = !table_first,
                ta = table_active.trim(),
                tp = table_first,
            ));
        }
        h.push_str("</div>");
    }

    // Geometry. Circles stay at r=108 regardless of layout so 2-set and 3-set
    // read at the same visual scale; the viewBox hugs the circles' bounding
    // box (plus padding) so the diagram fills its container instead of
    // floating small inside a fixed canvas.
    let r = 108.0_f64;
    let (base_w, base_h) = (580.0, 410.0);
    let centers: Vec<(f64, f64)> = match n {
        1 => vec![(base_w / 2.0, base_h / 2.0)],
        2 => vec![
            (base_w / 2.0 - r * 0.55, base_h / 2.0),
            (base_w / 2.0 + r * 0.55, base_h / 2.0),
        ],
        _ => {
            // 3 sets: vertices of an upward-pointing triangle, recentered.
            // Distance from centroid to each vertex = r * 0.62 for healthy overlap.
            let d = r * 0.62;
            let cx = base_w / 2.0;
            let cy = base_h / 2.0 + d * 0.3;
            vec![
                (cx, cy - d),
                (cx - d * 0.866, cy + d * 0.5),
                (cx + d * 0.866, cy + d * 0.5),
            ]
        }
    };
    let pad = 14.0;
    let min_x = centers.iter().map(|c| c.0).fold(f64::INFINITY, f64::min) - r - pad;
    let max_x = centers
        .iter()
        .map(|c| c.0)
        .fold(f64::NEG_INFINITY, f64::max)
        + r
        + pad;
    let min_y = centers.iter().map(|c| c.1).fold(f64::INFINITY, f64::min) - r - pad;
    let max_y = centers
        .iter()
        .map(|c| c.1)
        .fold(f64::NEG_INFINITY, f64::max)
        + r
        + pad;
    let (vb_w, vb_h) = (max_x - min_x, max_y - min_y);

    // Where a set label sits (pushed from its circle's center away from the
    // diagram centroid) and how much horizontal room the uncovered crescent
    // gives it there. 2-set: the crescent runs from the circle edge (-1.0R) to
    // the other circle's edge (+0.1R), so center at 0.45R with ~1.0R of width.
    // 3-set: the top circle's crescent is wide open; the bottom two are hemmed
    // in by their neighbor.
    let set_label_push = match n {
        1 => 0.0,
        2 => r * 0.45,
        _ => r * 0.55,
    };
    let set_label_avail = |i: usize| -> f64 {
        match n {
            1 => r * 1.7,
            2 => r * 1.0,
            _ => {
                if i == 0 {
                    r * 1.5
                } else {
                    r * 0.95
                }
            }
        }
    };
    // Room inside an intersection region: width, and how tall a label block
    // may stack before it pokes out of the lune.
    let overlap_avail = |count: usize| -> f64 {
        if n == 2 {
            r * 0.8
        } else if count >= 3 {
            r * 0.75
        } else {
            r * 0.62
        }
    };
    let overlap_avail_h = |count: usize| -> f64 {
        if n == 2 {
            r * 1.2
        } else if count >= 3 {
            r * 0.7
        } else {
            r * 0.45
        }
    };
    // Keep on-screen scale constant across layouts: ~1.85px per viewBox unit,
    // so a lone circle doesn't balloon to fill the whole column.
    let svg_max_width = (vb_w * 1.85).round().min(680.0);

    let svg_hidden = if table_first { " hidden" } else { "" };
    h.push_str(&format!(
        r#"<svg class="c-venn-svg" data-venn-panel="venn" viewBox="{min_x:.1} {min_y:.1} {vb_w:.1} {vb_h:.1}" style="max-width:{svg_max_width:.0}px" role="img" aria-label="{}"{svg_hidden}>"#,
        title.map(esc).unwrap_or_default()
    ));

    for (i, set) in sets.iter().take(centers.len()).enumerate() {
        let (cx, cy) = centers[i];
        h.push_str(&format!(
            r#"<circle class="c-venn-circle c-venn-circle-{color}" cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}"/>"#,
            color = set.color.class_suffix(),
        ));
    }

    // Set labels - pushed away from the diagram centroid into the part of the
    // circle nothing else covers, then shrunk / split to fit that width.
    let centroid_x = centers.iter().map(|c| c.0).sum::<f64>() / centers.len() as f64;
    let centroid_y = centers.iter().map(|c| c.1).sum::<f64>() / centers.len() as f64;
    for (i, set) in sets.iter().take(centers.len()).enumerate() {
        let (cx, cy) = centers[i];
        let dx = cx - centroid_x;
        let dy = cy - centroid_y;
        let mag = (dx * dx + dy * dy).sqrt().max(1.0);
        let lx = cx + dx / mag * set_label_push;
        let ly = cy + dy / mag * set_label_push;
        let avail = set_label_avail(i);
        let fit = venn_fit_label(&set.label, avail, 13.0, 9.0, 1.05);
        h.push_str(&format!(
            r#"<text class="c-venn-label c-venn-label-{color}" x="{lx:.1}" y="{ly:.1}" text-anchor="middle" dominant-baseline="middle" style="font-size:{fs:.1}px">"#,
            color = set.color.class_suffix(),
            fs = fit.font_size,
        ));
        venn_tspans(&mut h, &fit, lx, avail, 1.05);
        h.push_str("</text>");
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
        if n == 3 && count == 2 {
            if let Some(third_idx) = (0..3).find(|i| !ov.sets.contains(i)) {
                let (tx, ty) = centers[third_idx];
                let dx = lx - tx;
                let dy = ly - ty;
                let mag = (dx * dx + dy * dy).sqrt().max(1.0);
                // 0.5R centers the label in the lune's usable span (third
                // circle's edge at 0.38R from the centroid, lune tip at 1.15R).
                let push = r * 0.5;
                lx += dx / mag * push;
                ly += dy / mag * push;
            }
        }

        let label = ov.label.as_deref().unwrap_or("");
        if !label.is_empty() {
            let avail = overlap_avail(count);
            let fit = venn_wrap_label(label, avail, overlap_avail_h(count), 12.0, 9.0, 1.0);
            h.push_str(&format!(
                r#"<text class="c-venn-overlap-label" x="{lx:.1}" y="{ly:.1}" text-anchor="middle" dominant-baseline="middle" style="font-size:{fs:.1}px">"#,
                fs = fit.font_size,
            ));
            venn_tspans(&mut h, &fit, lx, avail, 1.0);
            h.push_str("</text>");
        }
    }
    h.push_str("</svg>");

    // Matrix view: sets as both rows and columns. Diagonal = each set's own
    // total (parsed from a "NAME (count)" label), off-diagonal = the pairwise
    // overlap label, plus an "All N" row for the full intersection.
    if can_toggle {
        let shown: Vec<&VennSet> = sets.iter().take(centers.len()).collect();
        let parsed: Vec<(String, String, &'static str)> = shown
            .iter()
            .map(|s| {
                let (name, count) = venn_split_label(&s.label);
                (name, count, s.color.class_suffix())
            })
            .collect();
        let overlap_for = |idx: &[usize]| -> String {
            let mut key: Vec<usize> = idx.to_vec();
            key.sort_unstable();
            overlaps
                .iter()
                .find(|o| {
                    let mut k = o.sets.clone();
                    k.sort_unstable();
                    k == key
                })
                .and_then(|o| o.label.clone())
                .unwrap_or_default()
        };
        let table_hidden = if table_first { "" } else { " hidden" };
        h.push_str(&format!(
            r#"<div class="c-venn-matrix-wrap" data-venn-panel="table"{table_hidden}><table class="c-venn-matrix"><thead><tr><th scope="col" aria-label="Set"></th>"#
        ));
        for (name, _, color) in &parsed {
            h.push_str(&format!(
                r#"<th scope="col" class="c-venn-th-{color}">{}</th>"#,
                esc(name)
            ));
        }
        h.push_str("</tr></thead><tbody>");
        for (i, (name, count, color)) in parsed.iter().enumerate() {
            h.push_str(&format!(
                r#"<tr><th scope="row" class="c-venn-th-{color}">{}</th>"#,
                esc(name)
            ));
            for (j, other) in parsed.iter().enumerate() {
                if i == j {
                    let shown_count = if count.is_empty() {
                        "—"
                    } else {
                        count.as_str()
                    };
                    h.push_str(&format!(
                        r#"<td class="c-venn-cell c-venn-cell-total" title="{} total">{}</td>"#,
                        esc(name),
                        esc(shown_count)
                    ));
                } else {
                    let v = overlap_for(&[i, j]);
                    let cls = if v.is_empty() {
                        "c-venn-cell-empty"
                    } else {
                        "c-venn-cell-overlap"
                    };
                    let shown_v = if v.is_empty() { "—" } else { v.as_str() };
                    h.push_str(&format!(
                        r#"<td class="c-venn-cell {cls}" title="{} ∩ {}">{}</td>"#,
                        esc(name),
                        esc(&other.0),
                        esc(shown_v)
                    ));
                }
            }
            h.push_str("</tr>");
        }
        if parsed.len() >= 3 {
            let all_idx: Vec<usize> = (0..parsed.len()).collect();
            let v = overlap_for(&all_idx);
            let cls = if v.is_empty() {
                "c-venn-cell-empty"
            } else {
                "c-venn-cell-all"
            };
            let shown_v = if v.is_empty() { "—" } else { v.as_str() };
            h.push_str(&format!(
                r#"<tr class="c-venn-matrix-all"><th scope="row">All {}</th><td colspan="{}" class="c-venn-cell {cls}">{}</td></tr>"#,
                parsed.len(),
                parsed.len(),
                esc(shown_v)
            ));
        }
        h.push_str("</tbody></table></div>");
    }

    h.push_str("</div>");
    if can_toggle {
        Rendered::new(h).with_script("venn")
    } else {
        Rendered::new(h)
    }
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

fn progress_color_style(color: &str) -> String {
    match color {
        "default" => "var(--teal)".to_string(),
        "green" => "var(--green)".to_string(),
        "yellow" => "var(--yellow)".to_string(),
        "red" => "var(--red)".to_string(),
        "teal" => "var(--teal)".to_string(),
        hex if hex.starts_with('#') => hex.to_string(),
        _ => "var(--teal)".to_string(),
    }
}

fn progress_bar(
    value: u8,
    label: &Option<String>,
    color: &str,
    detail: &Option<String>,
    target: Option<u8>,
    thresholds: &std::collections::HashMap<String, String>,
) -> Rendered {
    let clamped = value.min(100);
    let effective_color = if !thresholds.is_empty() {
        resolve_threshold_color(clamped, thresholds).unwrap_or_else(|| color.to_string())
    } else {
        color.to_string()
    };
    let bg = progress_color_style(&effective_color);

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
        r#"<div class="c-progress-track" role="progressbar" aria-valuenow="{v}" aria-valuemin="0" aria-valuemax="100"><div class="c-progress-fill" style="--progress: {v}%; background: {bg}"></div>"#,
        v = clamped, bg = bg
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
    thresholds: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let mut best: Option<(u8, &String)> = None;
    for (key, color) in thresholds {
        if let Ok(threshold) = key.parse::<u8>() {
            if value >= threshold {
                match best {
                    Some((prev, _)) if threshold > prev => {
                        best = Some((threshold, color));
                    }
                    None => {
                        best = Some((threshold, color));
                    }
                    _ => {}
                }
            }
        }
    }
    best.map(|(_, c)| c.clone())
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

// ── Tier Summary ──────────────────────────────────

/// `Mar 3, 2027` from an ISO date; the raw text when it doesn't parse.
fn fmt_long_date(iso: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let s = iso.trim();
    let s = s.get(..10).unwrap_or(s);
    let mut p = s.split('-');
    let (Some(y), Some(m), Some(d)) = (p.next(), p.next(), p.next()) else {
        return iso.to_string();
    };
    match (m.parse::<usize>(), d.parse::<u32>()) {
        (Ok(m), Ok(d)) if (1..=12).contains(&m) => format!("{} {}, {}", MONTHS[m - 1], d, y),
        _ => iso.to_string(),
    }
}

/// Accounts sorted by renewal date, soonest first; undated ones last.
fn sorted_accounts(accounts: &[TierAccount]) -> Vec<&TierAccount> {
    let mut v: Vec<&TierAccount> = accounts.iter().collect();
    v.sort_by_key(|a| a.renewal.as_deref().and_then(jt_day).unwrap_or(i64::MAX));
    v
}

/// Color class for a tier: its explicit color, or an accent shade by
/// position (tier 1 solid, 2 at 62%, 3 at 32%, 4+ hollow).
fn tier_color_class(color: Option<SemColor>, index: usize) -> String {
    match color {
        Some(c) => format!("c-tier-{}", sem_color_class(c)),
        None => format!("c-tier-pos-{}", (index + 1).min(4)),
    }
}

fn tier_summary(
    tiers: &[Tier],
    count_label: Option<&str>,
    share_label: Option<&str>,
    id: Option<&str>,
    base: &str,
) -> Rendered {
    let count_label = count_label.unwrap_or("accounts");
    // With a dollar value the label becomes a small unit suffix ("$406K ARR"),
    // and only when the author set one.
    let unit_label = share_label;
    let share_label = share_label.unwrap_or("Share");
    let any_share = tiers.iter().any(|t| t.share.is_some());
    let link = |t: &Tier| t.href.as_deref().and_then(|h| safe_href(h, base));
    let expands = |t: &Tier| link(t).is_none() && !t.accounts.is_empty();
    let any_panel = tiers.iter().any(expands);
    let prefix = hud_prefix(id, "tier-summary");
    let id_attr = id
        .map(|i| format!(r#" id="{}""#, esc(i)))
        .unwrap_or_default();
    let mut h = format!(
        r#"<div{} class="c-tier-summary" data-tier-summary{}><div class="c-tier-row" data-kz-list="tiers">"#,
        id_attr,
        hud_param_attr(id)
    );
    for (i, t) in tiers.iter().enumerate() {
        let expandable = expands(t);
        let mut cls = format!("c-tier {}", tier_color_class(t.color, i));
        if t.active {
            cls.push_str(" is-active");
        }
        if t.count.trim() == "0" {
            cls.push_str(" is-empty");
        }
        if expandable {
            cls.push_str(" is-expandable");
        }
        let (open, close) = if let Some(href) = link(t) {
            (
                format!(r#"<a class="{}" href="{}">"#, cls, esc(&href)),
                "</a>",
            )
        } else if expandable {
            (
                format!(
                    r#"<button type="button" class="{}" aria-expanded="false" aria-controls="{}-panel-{}" data-tier-toggle="{}" data-tier-label="{}">"#,
                    cls,
                    esc(&prefix),
                    i,
                    i,
                    esc(&t.label)
                ),
                "</button>",
            )
        } else {
            (format!(r#"<div class="{}">"#, cls), "</div>")
        };
        h.push_str(&open);
        h.push_str(&format!(
            r#"<span class="c-tier-head"><span class="c-tier-swatch"></span><span class="c-tier-label">{}</span>"#,
            esc(&t.label)
        ));
        if expandable {
            h.push_str(r#"<span class="c-tier-chevron" aria-hidden="true"></span>"#);
        }
        h.push_str("</span>");
        h.push_str(&format!(
            r#"<span class="c-tier-count">{}</span><span class="c-tier-count-label">{}</span>"#,
            esc(&t.count),
            esc(count_label)
        ));
        if t.value.is_some() || any_share {
            h.push_str(&tier_money_row(t, any_share, share_label, unit_label));
        }
        if let Some(d) = &t.detail {
            h.push_str(&format!(r#"<span class="c-tier-detail">{}</span>"#, esc(d)));
        }
        h.push_str(close);
    }
    h.push_str("</div>");
    for (i, t) in tiers.iter().enumerate() {
        if expands(t) {
            h.push_str(&tier_panel(t, i, &prefix, base));
        }
    }
    h.push_str("</div>");
    let r = Rendered::new(h);
    if any_panel {
        r.with_script("tier_summary")
    } else {
        r
    }
}

/// The row under the count: dollars lead with the share on the right, or
/// (no `value`) the share label and number. The bar sits under it.
fn tier_money_row(t: &Tier, any_share: bool, share_label: &str, unit: Option<&str>) -> String {
    let mut h = String::from(r#"<span class="c-tier-share"><span class="c-tier-share-head">"#);
    match &t.value {
        Some(v) => {
            h.push_str(&format!(r#"<span class="c-tier-money">{}"#, esc(v)));
            if let Some(u) = unit {
                h.push_str(&format!(
                    r#"<span class="c-tier-money-unit">{}</span>"#,
                    esc(u)
                ));
            }
            h.push_str("</span>");
        }
        None => h.push_str(&format!(
            r#"<span class="c-tier-share-label">{}</span>"#,
            esc(share_label)
        )),
    }
    if let Some(s) = hud_share(t.share) {
        h.push_str(&format!(r#"<span class="c-tier-share-num">{}%</span>"#, s));
    }
    h.push_str("</span>");
    if any_share {
        let width = hud_share(t.share).unwrap_or(0.0);
        h.push_str(&format!(
            r#"<span class="c-tier-track"><span class="c-tier-fill{}" style="width: {}%"></span></span>"#,
            if width <= 0.0 { " is-zero" } else { "" },
            width
        ));
    }
    h.push_str("</span>");
    h
}

fn tier_panel(t: &Tier, i: usize, prefix: &str, base: &str) -> String {
    let n = t.accounts.len();
    let mut h = format!(
        r#"<div class="c-tier-panel {}" id="{}-panel-{}" data-tier-panel="{}"><div class="c-tier-panel-head"><span class="c-tier-swatch"></span><span class="c-tier-panel-title">{}</span><span class="c-tier-panel-count">{} {}{}</span></div><ul class="c-tier-accounts">"#,
        tier_color_class(t.color, i),
        esc(prefix),
        i,
        i,
        esc(&t.label),
        n,
        if n == 1 { "account" } else { "accounts" },
        t.value
            .as_deref()
            .map(|v| format!(" · {}", esc(v)))
            .unwrap_or_default()
    );
    for a in sorted_accounts(&t.accounts) {
        let color = match a.color {
            Some(c) => format!("c-tier-{}", sem_color_class(c)),
            None => tier_color_class(t.color, i),
        };
        let renewal_attr = a
            .renewal
            .as_deref()
            .map(|r| format!(r#" data-renewal="{}""#, esc(r.trim())))
            .unwrap_or_default();
        h.push_str(&format!(
            r#"<li class="c-tier-acct {}"{}><span class="c-tier-acct-main"><span class="c-tier-swatch"></span><span class="c-tier-acct-text">"#,
            color,
            renewal_attr
        ));
        match a.href.as_deref().and_then(|h| safe_href(h, base)) {
            Some(href) => h.push_str(&format!(
                r#"<a class="c-tier-acct-name" href="{}">{}</a>"#,
                esc(&href),
                esc(&a.name)
            )),
            None => h.push_str(&format!(
                r#"<span class="c-tier-acct-name">{}</span>"#,
                esc(&a.name)
            )),
        }
        if let Some(d) = &a.detail {
            h.push_str(&format!(
                r#"<span class="c-tier-acct-detail">{}</span>"#,
                esc(d)
            ));
        }
        h.push_str(r#"</span></span><span class="c-tier-acct-side">"#);
        if let Some(v) = &a.value {
            h.push_str(&format!(r#"<span class="c-tier-acct-value">{}"#, esc(v)));
            if let Some(sh) = hud_share(a.share) {
                h.push_str(&format!(
                    r#"<span class="c-tier-acct-share"> · {}%</span>"#,
                    sh
                ));
            }
            h.push_str("</span>");
        }
        if let Some(r) = &a.renewal {
            h.push_str(&format!(
                r#"<span class="c-tier-acct-renewal">Renews {}<span class="c-tier-acct-soon" data-tier-soon hidden></span></span>"#,
                esc(&fmt_long_date(r))
            ));
        }
        h.push_str("</span></li>");
    }
    h.push_str("</ul></div>");
    h
}

// ── HUD color helper ──────────────────────────────

/// `c-hud-<color>` for an explicit color, else `c-hud-shade-<1..4>`
/// (accent solid, 62%, 32%, hollow). Default is shade 1.
fn hud_color_class(color: Option<SemColor>, shade: Option<u8>) -> String {
    match color {
        Some(c) => format!("c-hud-{}", sem_color_class(c)),
        None => format!("c-hud-shade-{}", shade.unwrap_or(1).clamp(1, 4)),
    }
}

/// `Oct 17` from an ISO date; the raw text when it doesn't parse.
fn fmt_short_date(iso: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let s = iso.trim();
    let s = s.get(..10).unwrap_or(s);
    let mut p = s.split('-');
    let (Some(_), Some(m), Some(d)) = (p.next(), p.next(), p.next()) else {
        return iso.to_string();
    };
    match (m.parse::<usize>(), d.parse::<u32>()) {
        (Ok(m), Ok(d)) if (1..=12).contains(&m) => format!("{} {}", MONTHS[m - 1], d),
        _ => iso.to_string(),
    }
}

// ── Health Breakdown ──────────────────────────────

fn health_breakdown(items: &[HealthItem], id: Option<&str>) -> Rendered {
    let id_attr = id
        .map(|i| format!(r#" id="{}""#, esc(i)))
        .unwrap_or_default();
    let mut h = format!(r#"<div{} class="c-health" data-kz-list="items">"#, id_attr);
    for it in items {
        h.push_str(&format!(
            r#"<div class="c-health-item {}"><div class="c-health-main"><div class="c-health-label">{}</div>"#,
            hud_color_class(it.color, it.shade),
            esc(&it.label)
        ));
        if it.value.is_some() || it.detail.is_some() {
            h.push_str(r#"<div class="c-health-detail">"#);
            if let Some(v) = &it.value {
                h.push_str(&format!(
                    r#"<span class="c-health-value">{}</span>"#,
                    esc(v)
                ));
            }
            if let Some(d) = &it.detail {
                h.push_str(&format!(r#"<span class="c-health-text">{}</span>"#, esc(d)));
            }
            h.push_str("</div>");
        }
        h.push_str("</div>");
        if let Some(t) = &it.tier {
            h.push_str(&format!(
                r#"<div class="c-health-tier"><span class="c-hud-swatch"></span><span class="c-health-tier-name">{}</span></div>"#,
                esc(t)
            ));
        }
        h.push_str("</div>");
    }
    h.push_str("</div>");
    Rendered::new(h)
}

// ── Next Steps ────────────────────────────────────

fn next_step_owner<'a>(step: &'a NextStep, accounts: &'a [NextAccount]) -> Option<&'a str> {
    match step
        .account
        .as_deref()
        .and_then(|a| accounts.iter().find(|x| x.id == a))
    {
        Some(acct) => acct.owner.as_deref().or(step.owner.as_deref()),
        None => step.owner.as_deref(),
    }
}

fn owner_attr(owner: Option<&str>) -> String {
    owner
        .map(|o| format!(r#" data-owner="{}""#, esc(o)))
        .unwrap_or_default()
}

fn next_link(cls: &str, label: &str, href: Option<&str>, base: &str) -> String {
    match href.and_then(|h| safe_href(h, base)) {
        Some(h) => format!(
            r#"<a class="{}" href="{}">{}</a>"#,
            cls,
            esc(&h),
            esc(label)
        ),
        None => format!(r#"<span class="{}">{}</span>"#, cls, esc(label)),
    }
}

fn next_steps(
    steps: &[NextStep],
    accounts: &[NextAccount],
    owners: &[String],
    order: NextOrder,
    title: Option<&str>,
    id: Option<&str>,
    base: &str,
) -> Rendered {
    let board = !accounts.is_empty();
    let id_attr = id
        .map(|i| format!(r#" id="{}""#, esc(i)))
        .unwrap_or_default();
    let today = hud_today();
    let mut h = format!(
        r#"<div{} class="c-next{}" data-next-steps{}>"#,
        id_attr,
        if board { " is-board" } else { "" },
        hud_param_attr(id)
    );
    if !owners.is_empty() {
        h.push_str(r#"<div class="c-next-chips" role="group" aria-label="Filter by owner"><button type="button" class="c-next-chip is-active" data-owner-chip="" aria-pressed="true">Owner: All</button>"#);
        for o in owners {
            h.push_str(&format!(
                r#"<button type="button" class="c-next-chip" data-owner-chip="{}" aria-pressed="false">Owner: {}</button>"#,
                esc(o),
                esc(o)
            ));
        }
        h.push_str("</div>");
    }
    if let Some(t) = title {
        h.push_str(&format!(r#"<div class="c-next-title">{}</div>"#, esc(t)));
    }
    if board {
        let prefix = hud_prefix(id, "next-steps");
        h.push_str(r#"<ul class="c-next-accounts" data-kz-list="accounts">"#);
        for (i, a) in next_account_order(accounts, steps, order, today) {
            h.push_str(&next_account_row(
                a, i, steps, accounts, &prefix, today, base,
            ));
        }
        h.push_str("</ul>");
        let loose: Vec<&NextStep> = steps
            .iter()
            .filter(|s| {
                s.account
                    .as_deref()
                    .is_none_or(|a| !accounts.iter().any(|x| x.id == a))
            })
            .collect();
        if !loose.is_empty() {
            h.push_str(r#"<ul class="c-next-steps c-next-loose" data-kz-list="steps">"#);
            for s in loose {
                h.push_str(&next_step_card(s, None, accounts, today, base));
            }
            h.push_str("</ul>");
        }
    } else {
        h.push_str(r#"<ul class="c-next-steps" data-kz-list="steps">"#);
        if steps.is_empty() {
            h.push_str(r#"<li class="c-next-empty">Nothing due.</li>"#);
        }
        for s in steps {
            h.push_str(&next_step_card(s, None, accounts, today, base));
        }
        h.push_str("</ul>");
    }
    h.push_str("</div>");
    let r = Rendered::new(h);
    if board || !owners.is_empty() {
        r.with_script("next_steps")
    } else {
        r
    }
}

fn next_step_card(
    s: &NextStep,
    account_name: Option<&str>,
    accounts: &[NextAccount],
    today: Option<i64>,
    base: &str,
) -> String {
    let status = match s.status {
        Some(x) => format!(" {}", x.class()),
        None if step_overdue(s, today) => " is-overdue".to_string(),
        None => String::new(),
    };
    let mut h = format!(
        r#"<li class="c-next-step{}"{}><span class="c-next-dot"></span><span class="c-next-text">{}"#,
        status,
        owner_attr(next_step_owner(s, accounts)),
        next_link("c-next-label", &s.label, s.href.as_deref(), base)
    );
    if let Some(d) = &s.detail {
        h.push_str(&format!(
            r#"<span class="c-next-detail">{}</span>"#,
            esc(strip_account_prefix(d, account_name))
        ));
    }
    h.push_str("</span>");
    if let Some(d) = &s.due {
        h.push_str(&format!(
            r#"<span class="c-next-due">{}</span>"#,
            esc(&fmt_short_date(d))
        ));
    }
    h.push_str("</li>");
    h
}

/// Inside its own account row, "Larkspur: day 52" reads as "day 52".
fn strip_account_prefix<'a>(detail: &'a str, account_name: Option<&str>) -> &'a str {
    account_name
        .and_then(|n| detail.strip_prefix(n))
        .and_then(|rest| rest.strip_prefix(": "))
        .unwrap_or(detail)
}

/// Open (not done) steps, overdue count, and soonest due day for an account.
/// Overdue by status, or (no status given) by a due date before today.
fn step_overdue(s: &NextStep, today: Option<i64>) -> bool {
    match s.status {
        Some(st) => st == StepStatus::Overdue,
        None => matches!(
            (s.due.as_deref().and_then(jt_day), today),
            (Some(d), Some(t)) if d < t
        ),
    }
}

fn next_account_summary(
    id: &str,
    steps: &[NextStep],
    today: Option<i64>,
) -> (usize, usize, Option<i64>) {
    let mine = steps
        .iter()
        .filter(|s| s.account.as_deref() == Some(id) && s.status != Some(StepStatus::Done));
    let mut open = 0;
    let mut overdue = 0;
    let mut soonest: Option<i64> = None;
    for s in mine {
        open += 1;
        if step_overdue(s, today) {
            overdue += 1;
        }
        if let Some(d) = s.due.as_deref().and_then(jt_day) {
            soonest = Some(soonest.map_or(d, |x| x.min(d)));
        }
    }
    (open, overdue, soonest)
}

/// Default order: accounts with an overdue step, then the rest with open
/// steps, each by soonest due date; then accounts with nothing due. Names
/// break ties (case-insensitive). `as_written` keeps the authored order.
fn next_account_order<'a>(
    accounts: &'a [NextAccount],
    steps: &[NextStep],
    order: NextOrder,
    today: Option<i64>,
) -> Vec<(usize, &'a NextAccount)> {
    let mut v: Vec<(usize, &NextAccount)> = accounts.iter().enumerate().collect();
    if order == NextOrder::AsWritten {
        return v;
    }
    v.sort_by_key(|(_, a)| {
        let (open, overdue, soonest) = next_account_summary(&a.id, steps, today);
        let group = if overdue > 0 {
            0
        } else if open > 0 {
            1
        } else {
            2
        };
        let due = if group == 2 {
            0
        } else {
            soonest.unwrap_or(i64::MAX)
        };
        (group, due, a.name.to_lowercase())
    });
    v
}

fn next_account_row(
    a: &NextAccount,
    index: usize,
    steps: &[NextStep],
    accounts: &[NextAccount],
    prefix: &str,
    today: Option<i64>,
    base: &str,
) -> String {
    let (open, overdue, _) = next_account_summary(&a.id, steps, today);
    let panel = format!("{}-acct-{}", prefix, a.id);
    // Expanded in the static markup so no-JS and print read everything;
    // the script collapses rows on load.
    let mut h = format!(
        r#"<li class="c-next-acct-item" data-acct="{}"{}><button type="button" class="c-next-acct {}" aria-expanded="true" aria-controls="{}" data-acct-toggle="{}" data-acct-i="{}"><span class="c-next-chevron" aria-hidden="true"></span><span class="c-hud-swatch"></span><span class="c-next-acct-name">{}</span>"#,
        esc(&a.id),
        owner_attr(a.owner.as_deref()),
        hud_color_class(a.color, a.shade),
        esc(&panel),
        esc(&a.id),
        index,
        esc(&a.name)
    );
    if let Some(st) = &a.stage {
        h.push_str(&format!(
            r#"<span class="c-next-acct-stage">{}</span>"#,
            esc(st)
        ));
    }
    if open == 0 {
        h.push_str(r#"<span class="c-next-acct-sum is-clear">nothing due</span>"#);
    } else {
        h.push_str(&format!(r#"<span class="c-next-acct-sum">{} open"#, open));
        if overdue > 0 {
            h.push_str(&format!(
                r#"<span class="c-next-acct-overdue"> · {} overdue</span>"#,
                overdue
            ));
        }
        h.push_str("</span>");
    }
    h.push_str(&format!(
        r#"</button><div class="c-next-acct-steps" id="{}"><ul class="c-next-steps">"#,
        esc(&panel)
    ));
    let mine: Vec<&NextStep> = steps
        .iter()
        .filter(|s| s.account.as_deref() == Some(a.id.as_str()))
        .collect();
    if mine.is_empty() {
        h.push_str(r#"<li class="c-next-empty">Nothing due.</li>"#);
    }
    for s in mine {
        h.push_str(&next_step_card(s, Some(&a.name), accounts, today, base));
    }
    h.push_str("</ul>");
    if let Some(href) = a.href.as_deref().and_then(|h| safe_href(h, base)) {
        h.push_str(&format!(
            r#"<a class="c-next-acct-link" href="{}">Open account</a>"#,
            esc(&href)
        ));
    }
    h.push_str("</div></li>");
    h
}

// ── Journey Track ─────────────────────────────────
//
// Layout math is mirrored line for line in the React renderer (sdk.rs,
// `journeyLayout`); change both together.

/// Rail width (px) the label-collision pass assumes: about curata's column.
const JT_RAIL_W: f64 = 640.0;
/// Narrow pass for a half-width column; CSS switches below a 560px track.
const JT_RAIL_W_NARROW: f64 = 380.0;
/// How far (px) a centered label may run past the rail before it flips to
/// start/end alignment.
const JT_EDGE: f64 = 24.0;
const JT_GAP: f64 = 10.0;
const JT_MAX_ROWS: usize = 4;
const JT_STAGE_CHAR_W: f64 = 8.0;
const JT_TP_CHAR_W: f64 = 7.0;
const JT_MONO_CHAR_W: f64 = 10.0;

struct JourneyLayout {
    /// Every element has a date: x is linear in time.
    dated: bool,
    /// x of `end` in dated mode.
    end: Option<f64>,
    stages: Vec<f64>,
    detours: Vec<(f64, f64)>,
    current: Option<f64>,
    touchpoints: Vec<f64>,
    done: f64,
}

fn jt_date(s: &Option<String>) -> Option<i64> {
    s.as_deref().and_then(jt_day)
}

/// Days since epoch for an ISO date, ignoring any time part.
fn jt_day(d: &str) -> Option<i64> {
    let t = d.trim();
    crate::freshness::parse_iso_date(t.get(..10).unwrap_or(t))
}

/// Today as days since epoch (honors `KAZAM_TODAY`), for build-time rules.
fn hud_today() -> Option<i64> {
    crate::freshness::parse_iso_date(&crate::freshness::today_iso())
}

/// A finite share clamped to 0-100, or None.
fn hud_share(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 100.0))
}

/// Per-page unique prefix for ids when the author gave none.
fn hud_prefix(id: Option<&str>, base: &str) -> String {
    match id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(i) => i.to_string(),
        None => slug::resolve(None, Some(base)).unwrap_or_else(|| base.to_string()),
    }
}

/// `data-kz-param` names the URL params `owner.<id>` etc. when the
/// component has an explicit id; bare params still work as a fallback.
fn hud_param_attr(id: Option<&str>) -> String {
    id.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|i| format!(r#" data-kz-param="{}""#, esc(i)))
        .unwrap_or_default()
}

fn journey_layout(
    stages: &[JourneyStage],
    detours: &[JourneyDetour],
    current: Option<&JourneyCurrent>,
    touchpoints: &[JourneyTouchpoint],
    start: &Option<String>,
    end: &Option<String>,
) -> JourneyLayout {
    let dated = stages.iter().all(|s| jt_date(&s.date).is_some())
        && touchpoints.iter().all(|t| jt_date(&t.date).is_some())
        && current.is_none_or(|c| jt_date(&c.date).is_some())
        && detours.iter().all(|d| jt_date(&d.from).is_some());
    let (stage_x, detour_x, current_x, tp_x, end_x) = if dated {
        let mut all: Vec<i64> = Vec::new();
        all.extend(stages.iter().filter_map(|s| jt_date(&s.date)));
        all.extend(touchpoints.iter().filter_map(|t| jt_date(&t.date)));
        all.extend(current.and_then(|c| jt_date(&c.date)));
        for d in detours {
            all.extend(jt_date(&d.from));
            all.extend(jt_date(&d.to));
        }
        let lo = jt_date(start).unwrap_or_else(|| all.iter().copied().min().unwrap_or(0));
        let mut hi = jt_date(end).unwrap_or_else(|| all.iter().copied().max().unwrap_or(lo));
        if hi <= lo {
            hi = lo + 1;
        }
        let f = |d: i64| ((d - lo) as f64 / (hi - lo) as f64).clamp(0.0, 1.0);
        (
            stages
                .iter()
                .map(|s| f(jt_date(&s.date).unwrap_or(lo)))
                .collect::<Vec<_>>(),
            detours
                .iter()
                .map(|d| {
                    let a = f(jt_date(&d.from).unwrap_or(lo));
                    let b = jt_date(&d.to).map(f).unwrap_or(a + 0.06);
                    let (a, b) = if b < a { (b, a) } else { (a, b) };
                    let b = b.max(a + 0.03).min(1.0);
                    (a, b)
                })
                .collect::<Vec<_>>(),
            current.map(|c| f(jt_date(&c.date).unwrap_or(hi))),
            touchpoints
                .iter()
                .map(|t| f(jt_date(&t.date).unwrap_or(lo)))
                .collect::<Vec<_>>(),
            jt_date(end).map(f),
        )
    } else {
        let n = stages.len();
        let slots = n + usize::from(current.is_some());
        let pos = |i: usize| {
            if slots <= 1 {
                0.5
            } else {
                i as f64 / (slots - 1) as f64
            }
        };
        let detour_x = detours
            .iter()
            .map(|d| {
                let k = d.after.unwrap_or(0).min(n.saturating_sub(1));
                let xa = pos(k);
                let xb = if k + 1 < slots { pos(k + 1) } else { 1.0 };
                let gap = xb - xa;
                if gap <= 0.0 {
                    ((xa - 0.12).max(0.0), xa)
                } else {
                    (xa + 0.18 * gap, xa + 0.72 * gap)
                }
            })
            .collect::<Vec<_>>();
        let m = touchpoints.len();
        (
            (0..n).map(pos).collect::<Vec<_>>(),
            detour_x,
            current.map(|_| pos(n)),
            (0..m)
                .map(|i| (i as f64 + 0.5) / m as f64)
                .collect::<Vec<_>>(),
            None,
        )
    };
    let done = current_x.unwrap_or_else(|| {
        stages
            .iter()
            .zip(&stage_x)
            .filter(|(s, _)| s.status == JourneyStatus::Completed)
            .map(|(_, x)| *x)
            .fold(0.0, f64::max)
    });
    JourneyLayout {
        dated,
        end: end_x,
        stages: stage_x,
        detours: detour_x,
        current: current_x,
        touchpoints: tp_x,
        done,
    }
}

/// Row (0-based) and alignment for each label so neighbours don't overlap
/// at `JT_RAIL_W`. Returns the per-label placement and the rows used.
fn jt_place_labels(items: &[(f64, &str)], char_w: f64) -> (Vec<(usize, &'static str)>, usize) {
    jt_place_labels_at(items, char_w, JT_RAIL_W)
}

/// Same pass at an explicit rail width (the narrow pass uses `JT_RAIL_W_NARROW`).
fn jt_place_labels_at(
    items: &[(f64, &str)],
    char_w: f64,
    rail_w: f64,
) -> (Vec<(usize, &'static str)>, usize) {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|a, b| items[*a].0.total_cmp(&items[*b].0).then(a.cmp(b)));
    let mut ends = [f64::NEG_INFINITY; JT_MAX_ROWS];
    let mut out = vec![(0usize, "center"); items.len()];
    let mut rows = 0;
    for i in order {
        let (x, label) = items[i];
        let c = x * rail_w;
        let w = label.chars().count() as f64 * char_w + 6.0;
        let (l, r, align) = if c - w / 2.0 < -JT_EDGE {
            (c - 10.0, c - 10.0 + w, "start")
        } else if c + w / 2.0 > rail_w + JT_EDGE {
            (c + 10.0 - w, c + 10.0, "end")
        } else {
            (c - w / 2.0, c + w / 2.0, "center")
        };
        let mut row = None;
        for (ri, end) in ends.iter().enumerate() {
            if l >= end + JT_GAP {
                row = Some(ri);
                break;
            }
        }
        let row = row.unwrap_or_else(|| {
            let mut best = 0;
            for ri in 1..JT_MAX_ROWS {
                if ends[ri] < ends[best] {
                    best = ri;
                }
            }
            best
        });
        ends[row] = r;
        rows = rows.max(row + 1);
        out[i] = (row, align);
    }
    (out, rows)
}

fn jt_pct(x: f64) -> String {
    format!("{:.2}", x * 100.0)
}

/// Rough span for a duration: whole days under 60, weeks under 120 days,
/// then months. Mirrored by `jtSpan` in the React renderer and the inline
/// script.
fn jt_span(days: i64) -> String {
    let d = days.max(0);
    if d < 60 {
        format!("{}d", d)
    } else if d < 120 {
        format!("{}w", (d as f64 / 7.0).round() as i64)
    } else {
        format!("{}mo", (d as f64 / 30.44).round() as i64)
    }
}

/// Past form of the end label: "Renews" reads "Renewed", "Contract ends"
/// reads "Contract ended"; anything else is kept as written.
fn jt_past(label: &str) -> String {
    if label == "Renews" {
        "Renewed".into()
    } else if let Some(stem) = label.strip_suffix(" ends") {
        format!("{} ended", stem)
    } else {
        label.to_string()
    }
}

fn jt_iso(s: &str) -> &str {
    let t = s.trim();
    t.get(..10).unwrap_or(t)
}

/// Focusable tooltip target: the styled tooltip reads the data attributes;
/// title/aria-label carry the exact date for no-JS and assistive tech.
fn jt_tip(name: &str, from: &str, to: Option<&str>) -> String {
    let when = match to {
        Some(t) => format!("{} to {}", fmt_long_date(from), fmt_long_date(t)),
        None => fmt_long_date(from),
    };
    format!(
        r#" tabindex="0" data-jt-tip="{}"{} data-jt-tip-name="{}" title="{} · {}" aria-label="{}, {}""#,
        esc(jt_iso(from)),
        to.map(|t| format!(r#" data-jt-tip-to="{}""#, esc(jt_iso(t))))
            .unwrap_or_default(),
        esc(name),
        esc(name),
        esc(&when),
        esc(name),
        esc(&when)
    )
}

/// Days from each stage's date to the next stage's (or the current
/// marker's) date. Dated mode only; detours count inside their stage.
fn jt_stage_spans(stages: &[JourneyStage], current: Option<&JourneyCurrent>) -> Vec<Option<i64>> {
    let next_dates: Vec<Option<i64>> = (0..stages.len())
        .map(|i| match stages.get(i + 1) {
            Some(n) => jt_date(&n.date),
            None => current.and_then(|c| jt_date(&c.date)),
        })
        .collect();
    stages
        .iter()
        .zip(next_dates)
        .map(|(s, next)| match (jt_date(&s.date), next) {
            (Some(a), Some(b)) if b >= a => Some(b - a),
            _ => None,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn journey_track(
    stages: &[JourneyStage],
    detours: &[JourneyDetour],
    current: Option<&JourneyCurrent>,
    touchpoints: &[JourneyTouchpoint],
    history: Option<&JourneyHistory>,
    default_view: JourneyView,
    title: Option<&str>,
    start: &Option<String>,
    end: &Option<String>,
    end_label: Option<&str>,
    id: Option<&str>,
) -> Rendered {
    let lay = journey_layout(stages, detours, current, touchpoints, start, end);
    let dated = lay.dated;
    let end_iso = end.as_deref().filter(|_| dated && lay.end.is_some());
    let end_label = end_label.unwrap_or("Renews");
    let id_attr = id
        .map(|i| format!(r#" id="{}""#, esc(i)))
        .unwrap_or_default();
    let above = !detours.is_empty() || current.is_some_and(|c| c.temporary);
    let hist = history.filter(|x| !x.events.is_empty());
    let scripted = hist.is_some() || dated;
    let mut h = format!(
        r#"<div{} class="c-journey{}"{}{}>"#,
        id_attr,
        if dated { " is-dated" } else { "" },
        if scripted { " data-journey" } else { "" },
        if hist.is_some() && default_view == JourneyView::Details {
            r#" data-default-view="details""#
        } else {
            ""
        }
    );
    if title.is_some() || hist.is_some() {
        h.push_str(r#"<div class="c-jt-head">"#);
        if let Some(t) = title {
            h.push_str(&format!(r#"<div class="c-jt-title">{}</div>"#, esc(t)));
        }
        if let Some(hs) = hist {
            // Same control as venn's view switcher, with text labels.
            h.push_str(&format!(
                concat!(
                    r#"<div class="c-venn-toggle c-jt-toggle" role="group" aria-label="Journey view switcher">"#,
                    r#"<button type="button" class="active" data-jt-view="timeline" aria-pressed="true">Timeline</button>"#,
                    r#"<button type="button" data-jt-view="details" aria-pressed="false">Details<span class="c-jt-toggle-count">{}</span></button>"#,
                    r#"</div>"#
                ),
                hs.events.len()
            ));
        }
        h.push_str("</div>");
    }
    if hist.is_some() {
        h.push_str(r#"<div class="c-jt-panel" data-jt-panel="timeline">"#);
    }
    h.push_str(&format!(
        r#"<div class="c-jt-track{}"><div class="c-jt-line"></div><div class="c-jt-rail"><div class="c-jt-done" style="width: calc({}% + 16px)"></div><div class="c-jt-detours" data-kz-list="detours">"#,
        if above { " has-above" } else { "" },
        jt_pct(lay.done)
    ));
    for (d, (a, b)) in detours.iter().zip(&lay.detours) {
        let tip = match (&d.from, dated) {
            (Some(f), true) => jt_tip(&d.label, f, d.to.as_deref()),
            _ => String::new(),
        };
        h.push_str(&format!(
            r#"<div class="c-jt-detour c-jt-c-{}" style="left: {}%; width: {}%"{}><svg viewBox="0 0 100 64" preserveAspectRatio="none" aria-hidden="true"><path d="M0 64 A50 62 0 0 1 100 64" vector-effect="non-scaling-stroke"/></svg><span class="c-jt-detour-label">{}</span></div>"#,
            sem_color_class(d.color),
            jt_pct(*a),
            jt_pct(b - a),
            tip,
            esc(&d.label)
        ));
    }
    h.push_str("</div>");
    for (s, x) in stages.iter().zip(&lay.stages) {
        let tip = match (&s.date, dated) {
            (Some(d), true) => jt_tip(&s.label, d, None),
            _ => format!(r#" title="{}""#, esc(&s.label)),
        };
        h.push_str(&format!(
            r#"<span class="c-jt-stage {}" style="left: {}%"{}></span>"#,
            s.status.class(),
            jt_pct(*x),
            tip
        ));
    }
    if let Some(e) = end_iso {
        h.push_str(&format!(
            r#"<span class="c-jt-end" style="left: 100%"{}></span>"#,
            jt_tip(end_label, e, None)
        ));
    }
    if let (Some(c), Some(x)) = (current, lay.current) {
        let tip = match (&c.date, dated) {
            (Some(d), true) => jt_tip(&c.label, d, None),
            _ => format!(r#" title="{}""#, esc(&c.label)),
        };
        h.push_str(&format!(
            r#"<span class="c-jt-current c-jt-c-{}" style="left: {}%"{}></span>"#,
            sem_color_class(c.color),
            jt_pct(x),
            tip
        ));
        if c.temporary {
            let (places, _) = jt_place_labels(&[(x, c.label.as_str())], JT_MONO_CHAR_W);
            let (narrow, _) =
                jt_place_labels_at(&[(x, c.label.as_str())], JT_MONO_CHAR_W, JT_RAIL_W_NARROW);
            h.push_str(&format!(
                r#"<span class="c-jt-current-label c-jt-c-{}" data-align="{}" data-align-n="{}" style="left: {}%">{}{}</span>"#,
                sem_color_class(c.color),
                places[0].1,
                narrow[0].1,
                jt_pct(x),
                esc(&c.label),
                jt_since(c, dated)
            ));
        }
    }
    h.push_str("</div></div>");

    // Stage labels, a non-temporary current, and the end label share one
    // collision pass. The end label's text is filled in the browser, so the
    // pass sizes it for its longest form.
    let mut items: Vec<(f64, String)> = stages
        .iter()
        .zip(&lay.stages)
        .map(|(s, x)| (*x, s.label.clone()))
        .collect();
    let current_below = match (current, lay.current) {
        (Some(c), Some(x)) if !c.temporary => {
            items.push((x, c.label.clone()));
            true
        }
        _ => false,
    };
    // Sized for the longer of the static text and the live "... in 999d".
    let end_at = end_iso.map(|e| {
        let live = format!("{} in 999d", jt_past(end_label));
        let fixed = format!("{} {}", end_label, fmt_long_date(e));
        items.push((
            1.0,
            if fixed.len() > live.len() {
                fixed
            } else {
                live
            },
        ));
        items.len() - 1
    });
    let refs: Vec<(f64, &str)> = items.iter().map(|(x, l)| (*x, l.as_str())).collect();
    let (places, rows) = jt_place_labels(&refs, JT_STAGE_CHAR_W);
    let (narrow, rows_n) = jt_place_labels_at(&refs, JT_STAGE_CHAR_W, JT_RAIL_W_NARROW);
    let spans = if dated {
        jt_stage_spans(stages, current)
    } else {
        vec![None; stages.len()]
    };
    h.push_str(&format!(
        r#"<div class="c-jt-stage-labels" data-kz-list="stages" style="--jt-rows: {}; --jt-rows-n: {}">"#,
        rows.max(1),
        rows_n.max(1)
    ));
    let place = |i: usize| {
        let (row, align) = places[i];
        let (row_n, align_n) = narrow[i];
        format!(
            r#" data-align="{}" data-align-n="{}" style="left: {}%; --jt-row: {}; --jt-row-n: {}""#,
            align,
            align_n,
            jt_pct(refs[i].0),
            row,
            row_n
        )
    };
    for (i, s) in stages.iter().enumerate() {
        let span = spans[i]
            .map(|d| format!(r#"<span class="c-jt-span">{}</span>"#, jt_span(d)))
            .unwrap_or_default();
        h.push_str(&format!(
            r#"<span class="c-jt-label {}"{}>{}{}</span>"#,
            s.status.class(),
            place(i),
            esc(&s.label),
            span
        ));
    }
    if current_below {
        if let Some(c) = current {
            h.push_str(&format!(
                r#"<span class="c-jt-label is-current c-jt-c-{}"{}>{}{}</span>"#,
                sem_color_class(c.color),
                place(stages.len()),
                esc(&c.label),
                jt_since(c, dated)
            ));
        }
    }
    if let (Some(i), Some(e)) = (end_at, end_iso) {
        h.push_str(&format!(
            r#"<span class="c-jt-label c-jt-end-label"{} data-jt-until="{}" data-jt-end="{}" data-jt-end-past="{}">{} {}</span>"#,
            place(i),
            esc(jt_iso(e)),
            esc(end_label),
            esc(&jt_past(end_label)),
            esc(end_label),
            esc(&fmt_long_date(e))
        ));
    }
    h.push_str("</div>");

    if !touchpoints.is_empty() {
        let items: Vec<(f64, &str)> = touchpoints
            .iter()
            .zip(&lay.touchpoints)
            .map(|(t, x)| (*x, t.label.as_str()))
            .collect();
        let (places, rows) = jt_place_labels(&items, JT_TP_CHAR_W);
        let (narrow, rows_n) = jt_place_labels_at(&items, JT_TP_CHAR_W, JT_RAIL_W_NARROW);
        h.push_str(&format!(
            r#"<div class="c-jt-tp-head">Touchpoints</div><div class="c-jt-tps" data-kz-list="touchpoints" style="--jt-rows: {}; --jt-rows-n: {}">"#,
            rows.max(1),
            rows_n.max(1)
        ));
        for (i, t) in touchpoints.iter().enumerate() {
            let (row, align) = places[i];
            let (row_n, align_n) = narrow[i];
            let x = jt_pct(lay.touchpoints[i]);
            let tip = match (&t.date, dated) {
                (Some(d), true) => jt_tip(&t.label, d, None),
                _ => String::new(),
            };
            h.push_str(&format!(
                r#"<span class="c-jt-tp {}" style="left: {}%"{}></span><span class="c-jt-tp-label" data-align="{}" data-align-n="{}" style="left: {}%; --jt-row: {}; --jt-row-n: {}">{}</span>"#,
                t.status.class(),
                x,
                tip,
                align,
                align_n,
                x,
                row,
                row_n,
                esc(&t.label)
            ));
        }
        h.push_str("</div>");
    }
    if let Some(hs) = hist {
        h.push_str("</div>");
        h.push_str(&journey_history(hs));
    }
    if dated {
        h.push_str(r#"<div class="c-jt-tip" role="tooltip" hidden></div>"#);
    }
    h.push_str("</div>");
    let r = Rendered::new(h);
    if scripted {
        r.with_script("journey_track")
    } else {
        r
    }
}

/// "13d so far" slot under the current label, filled in the browser.
fn jt_since(c: &JourneyCurrent, dated: bool) -> String {
    match (&c.date, dated) {
        (Some(d), true) => format!(
            r#"<span class="c-jt-span c-jt-since" data-jt-since="{}" hidden></span>"#,
            esc(jt_iso(d))
        ),
        _ => String::new(),
    }
}

/// The Details view: the history log, newest first. Hidden until the
/// toggle (or `default_view: details`, applied by the script) shows it.
fn journey_history(hist: &JourneyHistory) -> String {
    let mut events: Vec<&HistoryEvent> = hist.events.iter().collect();
    events.sort_by_key(|e| std::cmp::Reverse(jt_date(&Some(e.date.clone())).unwrap_or(i64::MIN)));
    let mut h = String::from(
        r#"<div class="c-jt-panel c-jt-details" data-jt-panel="details"><ol class="c-jt-history-list" data-kz-list="history.events">"#,
    );
    for e in events {
        let kind = e
            .kind
            .map(|k| format!(" {}", k.class()))
            .unwrap_or_default();
        let d = e.date.as_str();
        // date | dot | label + detail | relative (filled in the browser).
        h.push_str(&format!(
            r#"<li class="c-jt-hist{}" title="{}"><span class="c-jt-hist-date">{}</span><span class="c-jt-hist-dot"></span><span class="c-jt-hist-text"><span class="c-jt-hist-label">{}</span>"#,
            kind,
            esc(&fmt_long_date(d)),
            esc(&fmt_long_date(d)),
            esc(&e.label)
        ));
        if let Some(det) = &e.detail {
            h.push_str(&format!(
                r#" <span class="c-jt-hist-detail">{}</span>"#,
                esc(det)
            ));
        }
        h.push_str(&format!(
            r#"</span><span class="c-jt-hist-rel" data-jt-rel="{}" hidden></span></li>"#,
            esc(jt_iso(d))
        ));
    }
    h.push_str("</ol></div>");
    h
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

    #[test]
    fn journey_layout_is_linear_in_dated_mode_and_even_otherwise() {
        let st = |l: &str, d: Option<&str>| JourneyStage {
            label: l.into(),
            date: d.map(String::from),
            status: JourneyStatus::Completed,
        };
        let stages = vec![
            st("A", Some("2026-01-01")),
            st("B", Some("2026-01-11")),
            st("C", Some("2026-01-21")),
        ];
        let lay = journey_layout(&stages, &[], None, &[], &None, &None);
        assert_eq!(lay.stages, vec![0.0, 0.5, 1.0]);
        assert_eq!(lay.done, 1.0);

        let stages = vec![st("A", None), st("B", None), st("C", None)];
        let cur = JourneyCurrent {
            label: "Renewal".into(),
            date: None,
            temporary: true,
            color: SemColor::Teal,
        };
        let det = JourneyDetour {
            label: "Save".into(),
            from: None,
            to: None,
            after: Some(1),
            color: SemColor::Yellow,
        };
        let lay = journey_layout(&stages, &[det], Some(&cur), &[], &None, &None);
        assert_eq!(lay.current, Some(1.0));
        let (a, b) = lay.detours[0];
        assert!(a > lay.stages[1] && b < lay.stages[2], "{a} {b}");
    }

    #[test]
    fn journey_labels_that_collide_take_another_row() {
        let (places, rows) =
            jt_place_labels(&[(0.5, "Config review"), (0.51, "Exec call")], JT_TP_CHAR_W);
        assert_eq!(rows, 2);
        assert_ne!(places[0].0, places[1].0);
        let (places, rows) = jt_place_labels(&[(0.0, "Onboarding"), (1.0, "Renewal")], 8.0);
        assert_eq!(rows, 1);
        assert_eq!(places[0].1, "start");
        assert_eq!(places[1].1, "end");
    }

    #[test]
    fn journey_span_formats_days_then_weeks_then_months() {
        assert_eq!(jt_span(0), "0d");
        assert_eq!(jt_span(59), "59d");
        assert_eq!(jt_span(60), "9w");
        assert_eq!(jt_span(119), "17w");
        assert_eq!(jt_span(120), "4mo");
        assert_eq!(jt_span(152), "5mo");
        assert_eq!(jt_span(-3), "0d");
        assert_eq!(jt_past("Renews"), "Renewed");
        assert_eq!(jt_past("Contract ends"), "Contract ended");
        assert_eq!(jt_past("Expires"), "Expires");
    }

    #[test]
    fn journey_stage_spans_run_to_the_next_stage_or_current() {
        let st = |d: &str| JourneyStage {
            label: "S".into(),
            date: Some(d.into()),
            status: JourneyStatus::Completed,
        };
        let cur = JourneyCurrent {
            label: "Renewal".into(),
            date: Some("2026-03-12".into()),
            temporary: true,
            color: SemColor::Teal,
        };
        let spans = jt_stage_spans(&[st("2026-01-01"), st("2026-02-10")], Some(&cur));
        assert_eq!(spans, vec![Some(40), Some(30)]);
        assert_eq!(jt_stage_spans(&[st("2026-01-01")], None), vec![None]);
    }

    #[test]
    fn hud_unsafe_hrefs_render_as_plain_text() {
        let yaml = "type: tier_summary\ntiers:\n  - { label: A, count: 1, href: \"javascript:alert(1)\" }\n  - { label: B, count: 1, accounts: [{ name: N, href: \"data:text/html,x\" }] }\n";
        let comp: Component = serde_yaml::from_str(yaml).unwrap();
        let Component::TierSummary { tiers, .. } = &comp else {
            panic!("expected TierSummary")
        };
        let r = tier_summary(tiers, None, None, Some("t"), "");
        assert!(!r.html.contains("javascript:"), "{}", r.html);
        assert!(!r.html.contains("data:text"), "{}", r.html);
        assert!(r
            .html
            .contains(r#"<span class="c-tier-acct-name">N</span>"#));
    }
}

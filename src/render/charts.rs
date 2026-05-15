//! Build-time SVG chart renderer. Three kinds — pie, bar, timeseries — with an
//! optional second dimension via `series:`. No JS, no runtime deps.
//!
//! Chart colors use the canonical `SemColor` hexes (teal/green/yellow/red)
//! rather than the theme's CSS vars on purpose: themes like `dark` remap
//! `--teal` and `--green` onto the same sage tone, which destroys stack
//! contrast. Charts must stay distinguishable across every theme, so they
//! bypass theme overrides even when the user writes `color: green`.

use super::{esc, Rendered};
use crate::types::{ChartKind, ChartOrientation, ChartPoint, ChartSeries, SemColor};

const VB_W: f64 = 720.0;

/// Bundle of chart render inputs. Mirrors the `Component::Chart` fields —
/// passed as one arg so the entry point isn't a 7-positional-parameter blob.
pub struct ChartSpec<'a> {
    pub kind: ChartKind,
    pub title: &'a Option<String>,
    pub height: Option<u32>,
    pub x_label: &'a Option<String>,
    pub y_label: &'a Option<String>,
    pub orientation: ChartOrientation,
    pub data: &'a Option<Vec<ChartPoint>>,
    pub series: &'a Option<Vec<ChartSeries>>,
}

pub fn render(spec: ChartSpec<'_>) -> Rendered {
    let series_vec = coerce_series(spec.data, spec.series);
    let h = spec.height.unwrap_or_else(|| default_height(spec.kind));
    let aria = spec
        .title
        .clone()
        .unwrap_or_else(|| default_aria(spec.kind));

    let svg = match spec.kind {
        ChartKind::Pie => render_pie(&series_vec, h),
        ChartKind::Bar => render_bar(&series_vec, h, spec.orientation, spec.x_label, spec.y_label),
        ChartKind::Timeseries => render_timeseries(&series_vec, h, spec.x_label, spec.y_label),
    };

    let title_html = spec
        .title
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|t| {
            format!(
                r#"<figcaption class="c-chart-title">{}</figcaption>"#,
                esc(t)
            )
        })
        .unwrap_or_default();

    let legend = render_legend(spec.kind, &series_vec);

    let html = format!(
        r#"<figure class="c-chart c-chart-{k}" role="img" aria-label="{aria}">{title}{svg}{legend}</figure>"#,
        k = kind_class(spec.kind),
        aria = esc(&aria),
        title = title_html,
        svg = svg,
        legend = legend,
    );

    Rendered::new(html)
}

fn kind_class(k: ChartKind) -> &'static str {
    match k {
        ChartKind::Pie => "pie",
        ChartKind::Bar => "bar",
        ChartKind::Timeseries => "timeseries",
    }
}

fn default_height(k: ChartKind) -> u32 {
    match k {
        ChartKind::Pie => 280,
        ChartKind::Bar => 300,
        ChartKind::Timeseries => 280,
    }
}

fn default_aria(k: ChartKind) -> String {
    match k {
        ChartKind::Pie => "Pie chart".into(),
        ChartKind::Bar => "Bar chart".into(),
        ChartKind::Timeseries => "Time series chart".into(),
    }
}

/// Normalize `data:` / `series:` into a single `Vec<Series>` so the renderers
/// only ever deal with one shape. A lone `data:` becomes a nameless series of
/// index 0; a `series:` list passes through as-is.
struct NormSeries<'a> {
    label: &'a str,
    color: Option<SemColor>,
    points: &'a [ChartPoint],
}

fn coerce_series<'a>(
    data: &'a Option<Vec<ChartPoint>>,
    series: &'a Option<Vec<ChartSeries>>,
) -> Vec<NormSeries<'a>> {
    if let Some(s) = series {
        return s
            .iter()
            .map(|s| NormSeries {
                label: &s.label,
                color: s.color,
                points: &s.points,
            })
            .collect();
    }
    if let Some(d) = data {
        return vec![NormSeries {
            label: "",
            color: None,
            points: d,
        }];
    }
    Vec::new()
}

fn series_color(series_color: Option<SemColor>, idx: usize) -> &'static str {
    let c = series_color.unwrap_or_else(|| cycle_color(idx));
    c.hex()
}

/// Default palette for multi-series charts when the author hasn't set colors.
/// Skips the `Default` alias (which is the same hex as `Teal`) so four
/// consecutive series land on four distinct tones.
fn cycle_color(idx: usize) -> SemColor {
    match idx % 4 {
        0 => SemColor::Teal,
        1 => SemColor::Green,
        2 => SemColor::Yellow,
        _ => SemColor::Red,
    }
}

// ── Pie ──────────────────────────────────────────────

fn render_pie(series: &[NormSeries], height: u32) -> String {
    // Pie is always single-series. If the user accidentally passed `series:`
    // with multiple entries, flatten the first one — matches the "pie = one
    // ring of slices" mental model rather than silently dropping data.
    let Some(first) = series.first() else {
        return empty_svg(height);
    };
    let slices = first.points;
    let total: f64 = slices.iter().map(|p| p.value.max(0.0)).sum();
    if total <= 0.0 || slices.is_empty() {
        return empty_svg(height);
    }

    let h = height as f64;
    let cx = h / 2.0 + 20.0;
    let cy = h / 2.0;
    let r = (h / 2.0) - 16.0;

    let mut out = format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg">"#,
        vb_w = VB_W,
        h = h,
    );

    // Single 100% slice can't be drawn with an arc (start==end). Draw a
    // circle instead so the fill shows up.
    if slices.len() == 1 {
        let p = &slices[0];
        let fill = series_color(p.color.or(first.color), 0);
        out.push_str(&format!(
            r#"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{fill}"><title>{t}</title></circle>"#,
            cx = cx,
            cy = cy,
            r = r,
            fill = fill,
            t = esc(&format!("{}: {}", p.label, fmt_num(p.value))),
        ));
        out.push_str("</svg>");
        return out;
    }

    let mut angle_acc = 0.0_f64;
    for (i, p) in slices.iter().enumerate() {
        let v = p.value.max(0.0);
        if v <= 0.0 {
            continue;
        }
        let frac = v / total;
        let angle = frac * std::f64::consts::TAU;
        let a0 = angle_acc;
        let a1 = angle_acc + angle;
        let (x0, y0) = polar(cx, cy, r, a0);
        let (x1, y1) = polar(cx, cy, r, a1);
        let large = if angle > std::f64::consts::PI { 1 } else { 0 };
        let fill = series_color(p.color.or(first.color), i);
        let title = format!("{}: {} ({:.1}%)", p.label, fmt_num(v), frac * 100.0);
        out.push_str(&format!(
            r#"<path d="M {cx} {cy} L {x0:.2} {y0:.2} A {r} {r} 0 {large} 1 {x1:.2} {y1:.2} Z" fill="{fill}" class="c-chart-slice"><title>{title}</title></path>"#,
            cx = cx,
            cy = cy,
            r = r,
            x0 = x0,
            y0 = y0,
            x1 = x1,
            y1 = y1,
            large = large,
            fill = fill,
            title = esc(&title),
        ));
        angle_acc = a1;
    }

    out.push_str("</svg>");
    out
}

/// Convert (angle from 12 o'clock, clockwise) to (x, y) on the circle.
fn polar(cx: f64, cy: f64, r: f64, theta: f64) -> (f64, f64) {
    (cx + r * theta.sin(), cy - r * theta.cos())
}

// ── Bar ──────────────────────────────────────────────

fn render_bar(
    series: &[NormSeries],
    height: u32,
    orientation: ChartOrientation,
    x_label: &Option<String>,
    y_label: &Option<String>,
) -> String {
    if series.is_empty() || series.iter().all(|s| s.points.is_empty()) {
        return empty_svg(height);
    }
    match orientation {
        ChartOrientation::Vertical => render_bar_vertical(series, height, x_label, y_label),
        ChartOrientation::Horizontal => render_bar_horizontal(series, height, x_label, y_label),
    }
}

fn render_bar_vertical(
    series: &[NormSeries],
    height: u32,
    _x_label: &Option<String>,
    _y_label: &Option<String>,
) -> String {
    let h = height as f64;
    let left = 56.0;
    let right = 20.0;
    let top = 16.0;
    let bottom = 40.0;
    let plot_w = VB_W - left - right;
    let plot_h = h - top - bottom;

    let buckets = collect_buckets(series);
    if buckets.is_empty() {
        return empty_svg(height);
    }

    // Max value across stacked sums per bucket.
    let max_val = buckets
        .iter()
        .map(|b| stacked_total(series, b))
        .fold(0.0_f64, f64::max);
    if max_val <= 0.0 {
        return empty_svg(height);
    }
    let (_nmin, nmax, step) = nice_scale(0.0, max_val, 5);
    let axis_max = nmax.max(step);

    let mut out = format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg">"#,
        vb_w = VB_W,
        h = h,
    );

    // Gridlines + y tick labels
    let mut v = 0.0_f64;
    while v <= axis_max + 1e-9 {
        let y = top + plot_h - (v / axis_max) * plot_h;
        out.push_str(&format!(
            r#"<line x1="{x1}" y1="{y:.2}" x2="{x2}" y2="{y:.2}" class="c-chart-grid"/>"#,
            x1 = left,
            x2 = left + plot_w,
            y = y,
        ));
        out.push_str(&format!(
            r#"<text x="{x:.2}" y="{y:.2}" class="c-chart-axis c-chart-axis-y">{t}</text>"#,
            x = left - 8.0,
            y = y + 3.5,
            t = esc(&fmt_num(v)),
        ));
        v += step;
    }

    // Bars per bucket
    let n = buckets.len() as f64;
    let bucket_w = plot_w / n;
    let bar_w = (bucket_w * 0.62).min(60.0);

    for (i, bucket) in buckets.iter().enumerate() {
        let cx = left + bucket_w * (i as f64 + 0.5);
        let x = cx - bar_w / 2.0;
        // Stack from bottom → up.
        let mut stacked = 0.0_f64;
        for (s_idx, s) in series.iter().enumerate() {
            let v = s
                .points
                .iter()
                .find(|p| p.label == *bucket)
                .map(|p| p.value.max(0.0))
                .unwrap_or(0.0);
            if v <= 0.0 {
                continue;
            }
            let seg_h = (v / axis_max) * plot_h;
            let y = top + plot_h - ((stacked + v) / axis_max) * plot_h;
            let fill = series_color(s.color, s_idx);
            let title = if s.label.is_empty() {
                format!("{}: {}", bucket, fmt_num(v))
            } else {
                format!("{} — {}: {}", s.label, bucket, fmt_num(v))
            };
            out.push_str(&format!(
                r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{seg_h:.2}" fill="{fill}" class="c-chart-bar"><title>{title}</title></rect>"#,
                x = x,
                y = y,
                w = bar_w,
                seg_h = seg_h,
                fill = fill,
                title = esc(&title),
            ));
            stacked += v;
        }
        // X tick label
        out.push_str(&format!(
            r#"<text x="{x:.2}" y="{y:.2}" class="c-chart-axis c-chart-axis-x">{t}</text>"#,
            x = cx,
            y = top + plot_h + 20.0,
            t = esc(bucket),
        ));
    }

    out.push_str("</svg>");
    out
}

fn render_bar_horizontal(
    series: &[NormSeries],
    height: u32,
    _x_label: &Option<String>,
    _y_label: &Option<String>,
) -> String {
    let h = height as f64;
    let left = 120.0;
    let right = 24.0;
    let top = 16.0;
    let bottom = 32.0;
    let plot_w = VB_W - left - right;
    let plot_h = h - top - bottom;

    let buckets = collect_buckets(series);
    if buckets.is_empty() {
        return empty_svg(height);
    }

    let max_val = buckets
        .iter()
        .map(|b| stacked_total(series, b))
        .fold(0.0_f64, f64::max);
    if max_val <= 0.0 {
        return empty_svg(height);
    }
    let (_nmin, nmax, step) = nice_scale(0.0, max_val, 5);
    let axis_max = nmax.max(step);

    let mut out = format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg">"#,
        vb_w = VB_W,
        h = h,
    );

    // Gridlines + x-axis labels (values on the bottom)
    let mut v = 0.0_f64;
    while v <= axis_max + 1e-9 {
        let x = left + (v / axis_max) * plot_w;
        out.push_str(&format!(
            r#"<line x1="{x:.2}" y1="{y1:.2}" x2="{x:.2}" y2="{y2:.2}" class="c-chart-grid"/>"#,
            x = x,
            y1 = top,
            y2 = top + plot_h,
        ));
        out.push_str(&format!(
            r#"<text x="{x:.2}" y="{y:.2}" class="c-chart-axis c-chart-axis-x">{t}</text>"#,
            x = x,
            y = top + plot_h + 18.0,
            t = esc(&fmt_num(v)),
        ));
        v += step;
    }

    let n = buckets.len() as f64;
    let bucket_h = plot_h / n;
    let bar_h = (bucket_h * 0.62).min(38.0);

    for (i, bucket) in buckets.iter().enumerate() {
        let cy = top + bucket_h * (i as f64 + 0.5);
        let y = cy - bar_h / 2.0;
        let mut stacked = 0.0_f64;
        for (s_idx, s) in series.iter().enumerate() {
            let val = s
                .points
                .iter()
                .find(|p| p.label == *bucket)
                .map(|p| p.value.max(0.0))
                .unwrap_or(0.0);
            if val <= 0.0 {
                continue;
            }
            let seg_w = (val / axis_max) * plot_w;
            let x = left + (stacked / axis_max) * plot_w;
            let fill = series_color(s.color, s_idx);
            let title = if s.label.is_empty() {
                format!("{}: {}", bucket, fmt_num(val))
            } else {
                format!("{} — {}: {}", s.label, bucket, fmt_num(val))
            };
            out.push_str(&format!(
                r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{seg_h:.2}" fill="{fill}" class="c-chart-bar"><title>{title}</title></rect>"#,
                x = x,
                y = y,
                w = seg_w,
                seg_h = bar_h,
                fill = fill,
                title = esc(&title),
            ));
            stacked += val;
        }
        out.push_str(&format!(
            r#"<text x="{x:.2}" y="{y:.2}" class="c-chart-axis c-chart-axis-y-right">{t}</text>"#,
            x = left - 10.0,
            y = cy + 3.5,
            t = esc(bucket),
        ));
    }

    out.push_str("</svg>");
    out
}

// ── Timeseries ───────────────────────────────────────

fn render_timeseries(
    series: &[NormSeries],
    height: u32,
    _x_label: &Option<String>,
    _y_label: &Option<String>,
) -> String {
    if series.is_empty() || series.iter().all(|s| s.points.is_empty()) {
        return empty_svg(height);
    }
    let h = height as f64;
    let left = 56.0;
    let right = 20.0;
    let top = 16.0;
    let bottom = 40.0;
    let plot_w = VB_W - left - right;
    let plot_h = h - top - bottom;

    let buckets = collect_buckets(series);
    if buckets.is_empty() {
        return empty_svg(height);
    }

    // Timeseries = multi-line (not stacked). Max is the largest single value
    // across all series — each line needs its own y-space, not a summed one.
    let max_val = series
        .iter()
        .flat_map(|s| s.points.iter())
        .map(|p| p.value)
        .fold(0.0_f64, f64::max);
    if max_val <= 0.0 {
        return empty_svg(height);
    }
    let (_nmin, nmax, step) = nice_scale(0.0, max_val, 5);
    let axis_max = nmax.max(step);

    let mut out = format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg">"#,
        vb_w = VB_W,
        h = h,
    );

    // Gridlines + y tick labels
    let mut v = 0.0_f64;
    while v <= axis_max + 1e-9 {
        let y = top + plot_h - (v / axis_max) * plot_h;
        out.push_str(&format!(
            r#"<line x1="{x1}" y1="{y:.2}" x2="{x2}" y2="{y:.2}" class="c-chart-grid"/>"#,
            x1 = left,
            x2 = left + plot_w,
            y = y,
        ));
        out.push_str(&format!(
            r#"<text x="{x:.2}" y="{y:.2}" class="c-chart-axis c-chart-axis-y">{t}</text>"#,
            x = left - 8.0,
            y = y + 3.5,
            t = esc(&fmt_num(v)),
        ));
        v += step;
    }

    // X-axis labels (one per bucket). If there are many buckets, thin them
    // out so labels don't collide.
    let n = buckets.len();
    let stride = (n as f64 / 10.0).ceil() as usize;
    let stride = stride.max(1);
    let x_of = |i: usize| -> f64 {
        if n == 1 {
            left + plot_w / 2.0
        } else {
            left + plot_w * (i as f64) / ((n - 1) as f64)
        }
    };
    for (i, bucket) in buckets.iter().enumerate() {
        if i % stride != 0 && i != n - 1 {
            continue;
        }
        out.push_str(&format!(
            r#"<text x="{x:.2}" y="{y:.2}" class="c-chart-axis c-chart-axis-x">{t}</text>"#,
            x = x_of(i),
            y = top + plot_h + 20.0,
            t = esc(bucket),
        ));
    }

    // One polyline per series, plus point markers with tooltips.
    for (s_idx, s) in series.iter().enumerate() {
        let stroke = series_color(s.color, s_idx);
        let mut points_attr = String::new();
        let mut dots = String::new();
        for (i, bucket) in buckets.iter().enumerate() {
            let Some(p) = s.points.iter().find(|p| p.label == *bucket) else {
                continue;
            };
            let x = x_of(i);
            let y = top + plot_h - (p.value / axis_max) * plot_h;
            if !points_attr.is_empty() {
                points_attr.push(' ');
            }
            points_attr.push_str(&format!("{:.2},{:.2}", x, y));
            let title = if s.label.is_empty() {
                format!("{}: {}", bucket, fmt_num(p.value))
            } else {
                format!("{} — {}: {}", s.label, bucket, fmt_num(p.value))
            };
            dots.push_str(&format!(
                r#"<circle cx="{x:.2}" cy="{y:.2}" r="3" fill="{stroke}" class="c-chart-dot"><title>{title}</title></circle>"#,
                x = x,
                y = y,
                stroke = stroke,
                title = esc(&title),
            ));
        }
        if !points_attr.is_empty() {
            out.push_str(&format!(
                r#"<polyline points="{pts}" fill="none" stroke="{stroke}" stroke-width="2" stroke-linejoin="round" stroke-linecap="round" class="c-chart-line"/>"#,
                pts = points_attr,
                stroke = stroke,
            ));
        }
        out.push_str(&dots);
    }

    out.push_str("</svg>");
    out
}

// ── Shared helpers ───────────────────────────────────

/// Union of bucket labels across all series, preserving first-seen order so
/// months stay in January→December even when one series has holes.
fn collect_buckets(series: &[NormSeries]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in series {
        for p in s.points {
            if !out.iter().any(|b| b == &p.label) {
                out.push(p.label.clone());
            }
        }
    }
    out
}

fn stacked_total(series: &[NormSeries], bucket: &str) -> f64 {
    series
        .iter()
        .flat_map(|s| s.points.iter())
        .filter(|p| p.label == bucket)
        .map(|p| p.value.max(0.0))
        .sum()
}

fn render_legend(kind: ChartKind, series: &[NormSeries]) -> String {
    // Pie legend = slice labels. Bar/timeseries legend = series labels, but
    // only when there's more than one (single-series charts don't need one).
    let items: Vec<(String, String)> = match kind {
        ChartKind::Pie => {
            let Some(first) = series.first() else {
                return String::new();
            };
            first
                .points
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let color = series_color(p.color.or(first.color), i).to_string();
                    (p.label.clone(), color)
                })
                .collect()
        }
        _ => {
            if series.len() < 2 {
                return String::new();
            }
            series
                .iter()
                .enumerate()
                .map(|(i, s)| (s.label.to_string(), series_color(s.color, i).to_string()))
                .collect()
        }
    };
    if items.is_empty() {
        return String::new();
    }
    let mut out = String::from(r#"<ul class="c-chart-legend">"#);
    for (label, color) in items {
        out.push_str(&format!(
            r#"<li class="c-chart-legend-item"><span class="c-chart-swatch" style="background:{c}"></span><span>{l}</span></li>"#,
            c = color,
            l = esc(&label),
        ));
    }
    out.push_str("</ul>");
    out
}

fn empty_svg(height: u32) -> String {
    format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg"><text x="50%" y="50%" class="c-chart-empty" text-anchor="middle">No data</text></svg>"#,
        vb_w = VB_W,
        h = height,
    )
}

// ── Numeric helpers ──────────────────────────────────

fn fmt_num(v: f64) -> String {
    if v.is_nan() || v.is_infinite() {
        return "—".into();
    }
    if v.fract().abs() < 1e-9 {
        return format!("{}", v as i64);
    }
    let s = format!("{:.2}", v);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

/// Round `raw` to a "nice" number — one of {1,2,5} * 10^n. `round=true` picks
/// the nearest nice step for tick spacing; `round=false` picks the next nice
/// number ≥ raw for the axis extent.
fn nice_number(raw: f64, round: bool) -> f64 {
    if raw <= 0.0 {
        return 1.0;
    }
    let exp = raw.log10().floor();
    let f = raw / 10_f64.powf(exp);
    let nf = if round {
        if f < 1.5 {
            1.0
        } else if f < 3.0 {
            2.0
        } else if f < 7.0 {
            5.0
        } else {
            10.0
        }
    } else if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    nf * 10_f64.powf(exp)
}

fn nice_scale(min: f64, max: f64, target_ticks: usize) -> (f64, f64, f64) {
    let range = nice_number((max - min).max(1e-9), false);
    let ticks = target_ticks.max(2);
    let step = nice_number(range / (ticks as f64 - 1.0), true);
    let nice_min = (min / step).floor() * step;
    let nice_max = (max / step).ceil() * step;
    (nice_min, nice_max, step)
}

// ── Shared chart wrapper ───────────────────────────

fn wrap_chart(kind: &str, title: &Option<String>, svg: &str) -> Rendered {
    let title_html = title
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|t| {
            format!(
                r#"<figcaption class="c-chart-title">{}</figcaption>"#,
                esc(t)
            )
        })
        .unwrap_or_default();
    let aria = title
        .as_deref()
        .map(esc)
        .unwrap_or_else(|| format!("{} chart", kind));
    Rendered::new(format!(
        r#"<figure class="c-chart c-chart-{kind}" role="img" aria-label="{aria}">{title}{svg}</figure>"#,
        kind = kind,
        aria = aria,
        title = title_html,
        svg = svg,
    ))
}

// ── Sankey ──────────────────────────────────────────

pub fn render_sankey(
    title: &Option<String>,
    height: Option<u32>,
    flows: &[crate::types::SankeyFlow],
    colors: &std::collections::HashMap<String, crate::types::SemColor>,
) -> Rendered {
    let h = height.unwrap_or(400) as f64;

    if flows.is_empty() {
        return wrap_chart("sankey", title, &empty_svg(h as u32));
    }

    let mut node_names: Vec<String> = Vec::new();
    for f in flows {
        if !node_names.contains(&f.source) {
            node_names.push(f.source.clone());
        }
        if !node_names.contains(&f.target) {
            node_names.push(f.target.clone());
        }
    }

    let mut in_edges: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for f in flows {
        in_edges.entry(&f.target).or_default().push(&f.source);
    }

    let mut col: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for name in &node_names {
        if !in_edges.contains_key(name.as_str()) {
            col.insert(name, 0);
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for f in flows {
            if let Some(&sc) = col.get(f.source.as_str()) {
                let tc = col.entry(&f.target).or_insert(0);
                if *tc <= sc {
                    *tc = sc + 1;
                    changed = true;
                }
            }
        }
    }
    for name in &node_names {
        col.entry(name).or_insert(0);
    }

    let max_col = col.values().copied().max().unwrap_or(0);
    let num_cols = max_col + 1;

    let mut node_in: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    let mut node_out: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for f in flows {
        *node_out.entry(&f.source).or_default() += f.value;
        *node_in.entry(&f.target).or_default() += f.value;
    }
    let mut node_total: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for name in &node_names {
        let i = node_in.get(name.as_str()).copied().unwrap_or(0.0);
        let o = node_out.get(name.as_str()).copied().unwrap_or(0.0);
        node_total.insert(name, i.max(o));
    }

    let total_max: f64 = {
        let mut by_col: std::collections::HashMap<usize, f64> = std::collections::HashMap::new();
        for name in &node_names {
            let c = col[name.as_str()];
            *by_col.entry(c).or_default() += node_total[name.as_str()];
        }
        by_col.values().copied().fold(0.0_f64, f64::max)
    };

    if total_max <= 0.0 {
        return wrap_chart("sankey", title, &empty_svg(h as u32));
    }

    let pad_x = 140.0;
    let pad_y = 20.0;
    let node_w = 18.0;
    let node_gap = 8.0;
    let plot_w = VB_W - pad_x * 2.0;
    let plot_h = h - pad_y * 2.0;

    let col_spacing = if num_cols > 1 {
        plot_w / (num_cols as f64 - 1.0)
    } else {
        0.0
    };

    struct NodePos {
        x: f64,
        y: f64,
        h: f64,
    }
    let mut positions: std::collections::HashMap<&str, NodePos> = std::collections::HashMap::new();

    for c in 0..num_cols {
        let col_nodes: Vec<&str> = node_names
            .iter()
            .filter(|n| col[n.as_str()] == c)
            .map(|n| n.as_str())
            .collect();
        let col_total: f64 = col_nodes.iter().map(|n| node_total[n]).sum();
        let gaps = if col_nodes.len() > 1 {
            (col_nodes.len() - 1) as f64 * node_gap
        } else {
            0.0
        };
        let avail_h = plot_h - gaps;
        let scale = if col_total > 0.0 {
            avail_h / col_total
        } else {
            1.0
        };

        let x = pad_x + col_spacing * c as f64 - node_w / 2.0;
        let mut y = pad_y;
        for name in col_nodes {
            let nh = (node_total[name] * scale).max(2.0);
            positions.insert(name, NodePos { x, y, h: nh });
            y += nh + node_gap;
        }
    }

    let mut svg = format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg">"#,
        vb_w = VB_W,
        h = h,
    );

    let mut source_offset: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    let mut target_offset: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();

    for f in flows {
        let sp = &positions[f.source.as_str()];
        let tp = &positions[f.target.as_str()];
        let s_total = node_total[f.source.as_str()];
        let t_total = node_total[f.target.as_str()];

        let s_off = source_offset.entry(&f.source).or_insert(0.0);
        let link_h_source = if s_total > 0.0 {
            (f.value / s_total) * sp.h
        } else {
            0.0
        };
        let sy = sp.y + *s_off;
        *s_off += link_h_source;

        let t_off = target_offset.entry(&f.target).or_insert(0.0);
        let link_h_target = if t_total > 0.0 {
            (f.value / t_total) * tp.h
        } else {
            0.0
        };
        let ty = tp.y + *t_off;
        *t_off += link_h_target;

        let sx = sp.x + node_w;
        let tx = tp.x;
        let cpx = (sx + tx) / 2.0;

        let color = colors
            .get(&f.source)
            .or_else(|| colors.get(&f.target))
            .copied()
            .unwrap_or_else(|| {
                cycle_color(node_names.iter().position(|n| n == &f.source).unwrap_or(0))
            });

        let title_text = format!("{} \u{2192} {}: {}", f.source, f.target, fmt_num(f.value));

        svg.push_str(&format!(
            r#"<path d="M {sx:.1} {sy0:.1} C {cpx:.1} {sy0:.1}, {cpx:.1} {ty0:.1}, {tx:.1} {ty0:.1} L {tx:.1} {ty1:.1} C {cpx:.1} {ty1:.1}, {cpx:.1} {sy1:.1}, {sx:.1} {sy1:.1} Z" fill="{fill}" fill-opacity="0.35" class="c-sankey-link"><title>{t}</title></path>"#,
            sx = sx,
            sy0 = sy,
            sy1 = sy + link_h_source,
            tx = tx,
            ty0 = ty,
            ty1 = ty + link_h_target,
            cpx = cpx,
            fill = color.hex(),
            t = esc(&title_text),
        ));
    }

    for name in &node_names {
        let pos = &positions[name.as_str()];
        let color = colors
            .get(name.as_str())
            .copied()
            .unwrap_or_else(|| cycle_color(node_names.iter().position(|n| n == name).unwrap_or(0)));

        svg.push_str(&format!(
            r#"<rect x="{x:.1}" y="{y:.1}" width="{w}" height="{h:.1}" fill="{fill}" rx="2" class="c-sankey-node"><title>{name}: {total}</title></rect>"#,
            x = pos.x,
            y = pos.y,
            w = node_w,
            h = pos.h,
            fill = color.hex(),
            name = esc(name),
            total = fmt_num(node_total[name.as_str()]),
        ));

        let c = col[name.as_str()];
        let (lx, anchor) = if c == 0 {
            (pos.x - 6.0, "end")
        } else if c == max_col {
            (pos.x + node_w + 6.0, "start")
        } else {
            (pos.x + node_w / 2.0, "middle")
        };
        let ly = pos.y + pos.h / 2.0;
        let total_str = fmt_num(node_total[name.as_str()]);
        svg.push_str(&format!(
            r#"<text x="{lx:.1}" y="{ly:.1}" text-anchor="{anchor}" dominant-baseline="middle" class="c-sankey-label">{name}</text>"#,
            lx = lx,
            ly = ly,
            anchor = anchor,
            name = esc(name),
        ));
        svg.push_str(&format!(
            r#"<text x="{lx:.1}" y="{ly2:.1}" text-anchor="{anchor}" dominant-baseline="middle" class="c-sankey-value">{val}</text>"#,
            lx = lx,
            ly2 = ly + 14.0,
            anchor = anchor,
            val = total_str,
        ));
    }

    svg.push_str("</svg>");
    wrap_chart("sankey", title, &svg)
}

// ── Radar ───────────────────────────────────────────

pub fn render_radar(
    title: &Option<String>,
    height: Option<u32>,
    axes: &[String],
    curves: &[crate::types::RadarCurve],
    max: Option<f64>,
) -> Rendered {
    let h = height.unwrap_or(360) as f64;
    let n = axes.len();

    if n < 3 || curves.is_empty() {
        return wrap_chart("radar", title, &empty_svg(h as u32));
    }

    let auto_max = curves
        .iter()
        .flat_map(|c| c.values.iter())
        .copied()
        .fold(0.0_f64, f64::max);
    let max_val = max.unwrap_or(auto_max).max(1.0);

    let cx = VB_W / 2.0;
    let cy = h / 2.0;
    let r = (VB_W.min(h) / 2.0) - 60.0;
    let rings = 5;

    let angle_of = |i: usize| -> f64 {
        -std::f64::consts::FRAC_PI_2 + (i as f64) * std::f64::consts::TAU / (n as f64)
    };

    let point_at = |i: usize, val: f64| -> (f64, f64) {
        let a = angle_of(i);
        let d = (val / max_val) * r;
        (cx + d * a.cos(), cy + d * a.sin())
    };

    let mut svg = format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg">"#,
        vb_w = VB_W,
        h = h,
    );

    for ring in 1..=rings {
        let frac = ring as f64 / rings as f64;
        let mut pts = String::new();
        for i in 0..n {
            let (px, py) = point_at(i, max_val * frac);
            if !pts.is_empty() {
                pts.push(' ');
            }
            pts.push_str(&format!("{:.1},{:.1}", px, py));
        }
        svg.push_str(&format!(
            r#"<polygon points="{pts}" fill="none" stroke="rgba(var(--text-rgb),0.1)" stroke-width="1" class="c-radar-ring"/>"#,
        ));
    }

    for (i, axis_label) in axes.iter().enumerate() {
        let (ex, ey) = point_at(i, max_val);
        svg.push_str(&format!(
            r#"<line x1="{cx:.1}" y1="{cy:.1}" x2="{ex:.1}" y2="{ey:.1}" stroke="rgba(var(--text-rgb),0.12)" stroke-width="1" class="c-radar-axis"/>"#,
        ));
        let label_d = r + 16.0;
        let a = angle_of(i);
        let lx = cx + label_d * a.cos();
        let ly = cy + label_d * a.sin();
        let anchor = if (a.cos()).abs() < 0.1 {
            "middle"
        } else if a.cos() > 0.0 {
            "start"
        } else {
            "end"
        };
        svg.push_str(&format!(
            r#"<text x="{lx:.1}" y="{ly:.1}" text-anchor="{anchor}" dominant-baseline="middle" class="c-radar-label">{label}</text>"#,
            label = esc(axis_label),
        ));
    }

    for (ci, curve) in curves.iter().enumerate() {
        let color = series_color(curve.color, ci);
        let mut pts = String::new();
        for (i, &val) in curve.values.iter().enumerate() {
            let (px, py) = point_at(i, val.min(max_val));
            if !pts.is_empty() {
                pts.push(' ');
            }
            pts.push_str(&format!("{:.1},{:.1}", px, py));
        }
        svg.push_str(&format!(
            r#"<polygon points="{pts}" fill="{color}" fill-opacity="0.18" stroke="{color}" stroke-width="2" class="c-radar-curve"/>"#,
        ));
        for (i, &val) in curve.values.iter().enumerate() {
            let (px, py) = point_at(i, val.min(max_val));
            let tip = format!("{} \u{2014} {}: {}", curve.label, axes[i], fmt_num(val));
            svg.push_str(&format!(
                r#"<circle cx="{px:.1}" cy="{py:.1}" r="3.5" fill="{color}" class="c-chart-dot"><title>{tip}</title></circle>"#,
                tip = esc(&tip),
            ));
        }
    }

    svg.push_str("</svg>");

    let legend = if curves.len() > 1 {
        let mut leg = String::from(r#"<ul class="c-chart-legend">"#);
        for (i, c) in curves.iter().enumerate() {
            let color = series_color(c.color, i);
            leg.push_str(&format!(
                r#"<li class="c-chart-legend-item"><span class="c-chart-swatch" style="background:{c}"></span><span>{l}</span></li>"#,
                c = color,
                l = esc(&c.label),
            ));
        }
        leg.push_str("</ul>");
        leg
    } else {
        String::new()
    };

    let title_html = title
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|t| {
            format!(
                r#"<figcaption class="c-chart-title">{}</figcaption>"#,
                esc(t)
            )
        })
        .unwrap_or_default();

    Rendered::new(format!(
        r#"<figure class="c-chart c-chart-radar" role="img" aria-label="{aria}">{title}{svg}{legend}</figure>"#,
        aria = title
            .as_deref()
            .map(esc)
            .unwrap_or_else(|| "Radar chart".into()),
        title = title_html,
        svg = svg,
        legend = legend,
    ))
}

// ── Quadrant ────────────────────────────────────────

pub fn render_quadrant(
    title: &Option<String>,
    height: Option<u32>,
    x_axis: &str,
    y_axis: &str,
    quadrants: &[String],
    points: &[crate::types::QuadrantPoint],
) -> Rendered {
    let h = height.unwrap_or(400) as f64;

    if quadrants.len() != 4 || points.is_empty() {
        return wrap_chart("quadrant", title, &empty_svg(h as u32));
    }

    let left = 90.0;
    let right = 20.0;
    let top = 10.0;
    let bottom = 40.0;
    let plot_w = VB_W - left - right;
    let plot_h = h - top - bottom;
    let mid_x = left + plot_w / 2.0;
    let mid_y = top + plot_h / 2.0;

    let mut svg = format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg">"#,
        vb_w = VB_W,
        h = h,
    );

    let quad_colors = [
        "rgba(52,211,153,0.06)",
        "rgba(251,191,36,0.04)",
        "rgba(248,113,113,0.06)",
        "rgba(60,206,206,0.04)",
    ];
    let quad_rects = [
        (mid_x, top, plot_w / 2.0, plot_h / 2.0),
        (left, top, plot_w / 2.0, plot_h / 2.0),
        (left, mid_y, plot_w / 2.0, plot_h / 2.0),
        (mid_x, mid_y, plot_w / 2.0, plot_h / 2.0),
    ];
    for (i, (qx, qy, qw, qh)) in quad_rects.iter().enumerate() {
        svg.push_str(&format!(
            r#"<rect x="{qx:.1}" y="{qy:.1}" width="{qw:.1}" height="{qh:.1}" fill="{fill}" class="c-quadrant-bg"/>"#,
            fill = quad_colors[i],
        ));
        let lx = qx + qw / 2.0;
        let ly = qy + qh / 2.0;
        svg.push_str(&format!(
            r#"<text x="{lx:.1}" y="{ly:.1}" text-anchor="middle" dominant-baseline="middle" class="c-quadrant-zone-label">{label}</text>"#,
            label = esc(&quadrants[i]),
        ));
    }

    svg.push_str(&format!(
        r#"<line x1="{mid_x:.1}" y1="{top:.1}" x2="{mid_x:.1}" y2="{bot:.1}" class="c-quadrant-cross"/>"#,
        bot = top + plot_h,
    ));
    svg.push_str(&format!(
        r#"<line x1="{left:.1}" y1="{mid_y:.1}" x2="{rr:.1}" y2="{mid_y:.1}" class="c-quadrant-cross"/>"#,
        rr = left + plot_w,
    ));

    let x_parts: Vec<&str> = x_axis.split(" \u{2192} ").collect();
    if x_parts.len() == 2 {
        svg.push_str(&format!(
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="start" class="c-chart-axis">{t}</text>"#,
            x = left,
            y = top + plot_h + 24.0,
            t = esc(x_parts[0]),
        ));
        svg.push_str(&format!(
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="end" class="c-chart-axis">{t}</text>"#,
            x = left + plot_w,
            y = top + plot_h + 24.0,
            t = esc(x_parts[1]),
        ));
    } else {
        svg.push_str(&format!(
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="middle" class="c-chart-axis">{t}</text>"#,
            x = mid_x,
            y = top + plot_h + 24.0,
            t = esc(x_axis),
        ));
    }

    svg.push_str(&format!(
        r#"<text x="{x:.1}" y="{y:.1}" text-anchor="middle" transform="rotate(-90,{x:.1},{y:.1})" class="c-chart-axis">{t}</text>"#,
        x = left - 50.0,
        y = mid_y,
        t = esc(y_axis),
    ));

    for (i, pt) in points.iter().enumerate() {
        let px = left + pt.x.clamp(0.0, 1.0) * plot_w;
        let py = top + (1.0 - pt.y.clamp(0.0, 1.0)) * plot_h;
        let color = series_color(pt.color, i);
        let tip = format!("{} ({:.0}%, {:.0}%)", pt.label, pt.x * 100.0, pt.y * 100.0);
        svg.push_str(&format!(
            r#"<circle cx="{px:.1}" cy="{py:.1}" r="6" fill="{color}" class="c-chart-dot"><title>{tip}</title></circle>"#,
            tip = esc(&tip),
        ));
        svg.push_str(&format!(
            r#"<text x="{lx:.1}" y="{py:.1}" dominant-baseline="middle" class="c-quadrant-point-label">{label}</text>"#,
            lx = px + 10.0,
            label = esc(&pt.label),
        ));
    }

    svg.push_str("</svg>");
    wrap_chart("quadrant", title, &svg)
}

// ── Architecture ────────────────────────────────────

pub fn render_architecture(
    title: &Option<String>,
    height: Option<u32>,
    direction: crate::types::ArchDirection,
    nodes: &[crate::types::ArchNode],
    connections: &[crate::types::ArchConnection],
) -> Rendered {
    use crate::types::ArchDirection;

    let h = height.unwrap_or(300) as f64;

    if nodes.is_empty() {
        return wrap_chart("arch", title, &empty_svg(h as u32));
    }

    let mut col: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut in_set: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for c in connections {
        in_set.insert(&c.to);
    }
    for n in nodes {
        if !in_set.contains(n.id.as_str()) {
            col.insert(&n.id, 0);
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for c in connections {
            if let Some(&sc) = col.get(c.from.as_str()) {
                let tc = col.entry(&c.to).or_insert(0);
                if *tc <= sc {
                    *tc = sc + 1;
                    changed = true;
                }
            }
        }
    }
    for n in nodes {
        col.entry(&n.id).or_insert(0);
    }

    let max_col = col.values().copied().max().unwrap_or(0);
    let num_cols = max_col + 1;

    let is_lr = matches!(direction, ArchDirection::LeftToRight);
    let pad = 40.0;
    let node_w = 130.0;
    let node_h = 56.0;

    let mut cols_nodes: Vec<Vec<&crate::types::ArchNode>> = vec![Vec::new(); num_cols];
    for n in nodes {
        let c = col.get(n.id.as_str()).copied().unwrap_or(0);
        cols_nodes[c].push(n);
    }

    struct NPos {
        cx: f64,
        cy: f64,
    }
    let mut positions: std::collections::HashMap<&str, NPos> = std::collections::HashMap::new();

    if is_lr {
        let col_spacing = if num_cols > 1 {
            (VB_W - pad * 2.0 - node_w) / (num_cols as f64 - 1.0).max(1.0)
        } else {
            0.0
        };
        for (ci, col_nodes) in cols_nodes.iter().enumerate() {
            let n_count = col_nodes.len();
            let total_h = n_count as f64 * node_h + (n_count as f64 - 1.0).max(0.0) * 16.0;
            let start_y = (h - total_h) / 2.0;
            for (ni, node) in col_nodes.iter().enumerate() {
                positions.insert(
                    &node.id,
                    NPos {
                        cx: pad + node_w / 2.0 + col_spacing * ci as f64,
                        cy: start_y + ni as f64 * (node_h + 16.0) + node_h / 2.0,
                    },
                );
            }
        }
    } else {
        let row_spacing = if num_cols > 1 {
            (h - pad * 2.0 - node_h) / (num_cols as f64 - 1.0).max(1.0)
        } else {
            0.0
        };
        for (ci, col_nodes) in cols_nodes.iter().enumerate() {
            let n_count = col_nodes.len();
            let total_w = n_count as f64 * node_w + (n_count as f64 - 1.0).max(0.0) * 24.0;
            let start_x = (VB_W - total_w) / 2.0;
            for (ni, node) in col_nodes.iter().enumerate() {
                positions.insert(
                    &node.id,
                    NPos {
                        cx: start_x + ni as f64 * (node_w + 24.0) + node_w / 2.0,
                        cy: pad + node_h / 2.0 + row_spacing * ci as f64,
                    },
                );
            }
        }
    }

    let mut svg = format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg">"#,
        vb_w = VB_W,
        h = h,
    );

    svg.push_str(r#"<defs><marker id="arch-arrow" viewBox="0 0 10 7" refX="10" refY="3.5" markerWidth="8" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 3.5 L 0 7 z" fill="rgba(var(--text-rgb),0.5)"/></marker></defs>"#);

    for conn in connections {
        let Some(fp) = positions.get(conn.from.as_str()) else {
            continue;
        };
        let Some(tp) = positions.get(conn.to.as_str()) else {
            continue;
        };

        let (x1, y1, x2, y2) = if is_lr {
            (fp.cx + node_w / 2.0, fp.cy, tp.cx - node_w / 2.0, tp.cy)
        } else {
            (fp.cx, fp.cy + node_h / 2.0, tp.cx, tp.cy - node_h / 2.0)
        };

        svg.push_str(&format!(
            r#"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="rgba(var(--text-rgb),0.3)" stroke-width="1.5" marker-end="url(#arch-arrow)" class="c-arch-edge"/>"#,
        ));

        if let Some(label) = &conn.label {
            let mx = (x1 + x2) / 2.0;
            let my = (y1 + y2) / 2.0 - 8.0;
            svg.push_str(&format!(
                r#"<text x="{mx:.1}" y="{my:.1}" text-anchor="middle" class="c-arch-edge-label">{l}</text>"#,
                l = esc(label),
            ));
        }
    }

    for n in nodes {
        let Some(pos) = positions.get(n.id.as_str()) else {
            continue;
        };
        let rx = pos.cx - node_w / 2.0;
        let ry = pos.cy - node_h / 2.0;

        svg.push_str(&format!(
            r#"<rect x="{rx:.1}" y="{ry:.1}" width="{nw}" height="{nh}" rx="8" fill="rgba(var(--text-rgb),0.06)" stroke="{stroke}" stroke-width="1.5" class="c-arch-node"/>"#,
            nw = node_w,
            nh = node_h,
            stroke = n.color.hex(),
        ));
        svg.push_str(&format!(
            r#"<text x="{cx:.1}" y="{cy:.1}" text-anchor="middle" dominant-baseline="middle" class="c-arch-node-label">{label}</text>"#,
            cx = pos.cx,
            cy = if n.detail.is_some() {
                pos.cy - 7.0
            } else {
                pos.cy
            },
            label = esc(&n.label),
        ));
        if let Some(detail) = &n.detail {
            svg.push_str(&format!(
                r#"<text x="{cx:.1}" y="{cy:.1}" text-anchor="middle" dominant-baseline="middle" class="c-arch-node-detail">{d}</text>"#,
                cx = pos.cx,
                cy = pos.cy + 10.0,
                d = esc(detail),
            ));
        }
    }

    svg.push_str("</svg>");
    wrap_chart("arch", title, &svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_num_trims_integer_and_decimal() {
        assert_eq!(fmt_num(42.0), "42");
        assert_eq!(fmt_num(42.5), "42.5");
        assert_eq!(fmt_num(0.0), "0");
        assert_eq!(fmt_num(1.10), "1.1");
    }

    #[test]
    fn nice_scale_produces_round_steps() {
        let (_, max, step) = nice_scale(0.0, 125.0, 5);
        assert!(max >= 125.0);
        assert!(step > 0.0);
        assert!((max / step).round() * step - max < 1e-6);
    }

    #[test]
    fn polar_places_zero_angle_at_twelve_oclock() {
        let (x, y) = polar(100.0, 100.0, 50.0, 0.0);
        assert!((x - 100.0).abs() < 1e-6);
        assert!((y - 50.0).abs() < 1e-6);
    }

    #[test]
    fn collect_buckets_preserves_first_seen_order() {
        let a_points = vec![
            ChartPoint {
                label: "Jan".into(),
                value: 1.0,
                color: None,
            },
            ChartPoint {
                label: "Feb".into(),
                value: 2.0,
                color: None,
            },
        ];
        let b_points = vec![
            ChartPoint {
                label: "Feb".into(),
                value: 3.0,
                color: None,
            },
            ChartPoint {
                label: "Mar".into(),
                value: 4.0,
                color: None,
            },
        ];
        let series = vec![
            NormSeries {
                label: "A",
                color: None,
                points: &a_points,
            },
            NormSeries {
                label: "B",
                color: None,
                points: &b_points,
            },
        ];
        let buckets = collect_buckets(&series);
        assert_eq!(buckets, vec!["Jan", "Feb", "Mar"]);
    }

    #[test]
    fn sankey_renders_svg_with_flows() {
        let flows = vec![
            crate::types::SankeyFlow {
                source: "A".into(),
                target: "B".into(),
                value: 80.0,
            },
            crate::types::SankeyFlow {
                source: "A".into(),
                target: "C".into(),
                value: 20.0,
            },
        ];
        let colors = std::collections::HashMap::new();
        let result = render_sankey(&Some("Test".into()), None, &flows, &colors);
        assert!(result.html.contains("<svg"), "should contain SVG element");
        assert!(result.html.contains("c-sankey"), "should have sankey class");
        assert!(result.html.contains("Test"), "should contain title");
    }

    #[test]
    fn sankey_empty_flows_renders_empty() {
        let flows: Vec<crate::types::SankeyFlow> = vec![];
        let colors = std::collections::HashMap::new();
        let result = render_sankey(&None, None, &flows, &colors);
        assert!(result.html.contains("No data"), "should show empty state");
    }

    #[test]
    fn radar_renders_svg_with_curves() {
        let curves = vec![
            crate::types::RadarCurve {
                label: "Before".into(),
                values: vec![1.0, 4.0, 2.0],
                color: None,
            },
            crate::types::RadarCurve {
                label: "After".into(),
                values: vec![9.0, 6.0, 8.0],
                color: None,
            },
        ];
        let axes = vec!["A".into(), "B".into(), "C".into()];
        let result = render_radar(&Some("Test".into()), None, &axes, &curves, Some(10.0));
        assert!(result.html.contains("<svg"), "should contain SVG");
        assert!(result.html.contains("c-radar"), "should have radar class");
        assert!(
            result.html.contains("polygon"),
            "should render data polygons"
        );
    }

    #[test]
    fn quadrant_renders_svg_with_points() {
        let points = vec![
            crate::types::QuadrantPoint {
                label: "A".into(),
                x: 0.9,
                y: 0.9,
                color: Some(crate::types::SemColor::Red),
            },
            crate::types::QuadrantPoint {
                label: "B".into(),
                x: 0.2,
                y: 0.3,
                color: None,
            },
        ];
        let quads = vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()];
        let result = render_quadrant(&Some("Test".into()), None, "X", "Y", &quads, &points);
        assert!(result.html.contains("<svg"), "should contain SVG");
        assert!(
            result.html.contains("c-quadrant"),
            "should have quadrant class"
        );
        assert!(result.html.contains("Q1"), "should render quadrant labels");
    }

    #[test]
    fn architecture_renders_svg_with_nodes() {
        let nodes = vec![
            crate::types::ArchNode {
                id: "a".into(),
                label: "Source".into(),
                detail: None,
                icon: None,
                color: crate::types::SemColor::Teal,
            },
            crate::types::ArchNode {
                id: "b".into(),
                label: "Sink".into(),
                detail: Some("Details".into()),
                icon: None,
                color: crate::types::SemColor::Green,
            },
        ];
        let conns = vec![crate::types::ArchConnection {
            from: "a".into(),
            to: "b".into(),
            label: Some("flow".into()),
        }];
        let result = render_architecture(
            &Some("Test".into()),
            None,
            crate::types::ArchDirection::LeftToRight,
            &nodes,
            &conns,
        );
        assert!(result.html.contains("<svg"), "should contain SVG");
        assert!(result.html.contains("c-arch"), "should have arch class");
        assert!(result.html.contains("Source"), "should contain node label");
        assert!(result.html.contains("flow"), "should contain edge label");
    }
}

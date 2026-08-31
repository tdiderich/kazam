# Chart Components Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Sankey, Radar, Quadrant, and Architecture diagram components to kazam as build-time SVG - no JavaScript.

**Architecture:** Four new top-level `Component` enum variants (following `Venn` precedent, not extending `ChartKind`). Each has its own data model in `types.rs`, SVG renderer in `render/charts.rs`, CSS in `theme.rs`, validation in `validate.rs`, and SDK output in `sdk.rs`. A demo page at `examples/kb/demo/charts.yaml` showcases all four with security-themed example data.

**Tech Stack:** Rust, SVG, CSS. No new dependencies.

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/types.rs` | Modify | Add 4 Component variants + supporting structs/enums |
| `src/render/charts.rs` | Modify | Add 4 SVG render functions |
| `src/render/components.rs` | Modify | Add 4 match arms dispatching to renderers |
| `src/theme.rs` | Modify | Add CSS for `.c-sankey-*`, `.c-radar-*`, `.c-quadrant-*`, `.c-arch-*` |
| `src/validate.rs` | Modify | Add validation rules for each component |
| `src/sdk.rs` | Modify | Add TS interfaces, enum values, React placeholders, and icons |
| `examples/kb/demo/charts.yaml` | Create | Demo page with all 4 components |

---

### Task 1: Add Sankey data model to types.rs

**Files:**
- Modify: `src/types.rs`

- [ ] **Step 1: Add SankeyFlow struct after the Chart supporting types section (~line 1036)**

In `src/types.rs`, after the `ChartSeries` struct (around line 1036), add:

```rust
// ── Sankey supporting types ─────────────────────────

#[derive(Deserialize)]
pub struct SankeyFlow {
    pub source: String,
    pub target: String,
    pub value: f64,
}
```

- [ ] **Step 2: Add the Sankey variant to the Component enum**

In the `Component` enum (after the `Chart` variant, around line 524), add:

```rust
    Sankey {
        title: Option<String>,
        #[serde(default)]
        height: Option<u32>,
        flows: Vec<SankeyFlow>,
        #[serde(default)]
        colors: HashMap<String, SemColor>,
    },
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Warnings about unused fields but no errors. (The match in `components.rs` will need a wildcard or arm eventually, but `#[serde(tag)]` deserialization compiles without a match arm.)

Actually - the existing `render()` match is exhaustive, so this will fail with a non-exhaustive match error. That's expected. We'll fix it in Task 5 after all 4 types are added.

- [ ] **Step 4: Commit**

```bash
git add src/types.rs
git commit -m "feat(types): add Sankey component variant and SankeyFlow struct"
```

---

### Task 2: Add Radar data model to types.rs

**Files:**
- Modify: `src/types.rs`

- [ ] **Step 1: Add RadarCurve struct after SankeyFlow**

```rust
// ── Radar supporting types ──────────────────────────

#[derive(Deserialize)]
pub struct RadarCurve {
    pub label: String,
    pub values: Vec<f64>,
    #[serde(default)]
    pub color: Option<SemColor>,
}
```

- [ ] **Step 2: Add the Radar variant to the Component enum (after Sankey)**

```rust
    Radar {
        title: Option<String>,
        #[serde(default)]
        height: Option<u32>,
        axes: Vec<String>,
        curves: Vec<RadarCurve>,
        #[serde(default)]
        max: Option<f64>,
    },
```

- [ ] **Step 3: Commit**

```bash
git add src/types.rs
git commit -m "feat(types): add Radar component variant and RadarCurve struct"
```

---

### Task 3: Add Quadrant data model to types.rs

**Files:**
- Modify: `src/types.rs`

- [ ] **Step 1: Add QuadrantPoint struct after RadarCurve**

```rust
// ── Quadrant supporting types ───────────────────────

#[derive(Deserialize)]
pub struct QuadrantPoint {
    pub label: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub color: Option<SemColor>,
}
```

- [ ] **Step 2: Add the Quadrant variant to the Component enum (after Radar)**

```rust
    Quadrant {
        title: Option<String>,
        #[serde(default)]
        height: Option<u32>,
        x_axis: String,
        y_axis: String,
        quadrants: Vec<String>,
        points: Vec<QuadrantPoint>,
    },
```

- [ ] **Step 3: Commit**

```bash
git add src/types.rs
git commit -m "feat(types): add Quadrant component variant and QuadrantPoint struct"
```

---

### Task 4: Add Architecture data model to types.rs

**Files:**
- Modify: `src/types.rs`

- [ ] **Step 1: Add supporting structs and enum after QuadrantPoint**

```rust
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
```

- [ ] **Step 2: Add the Architecture variant to the Component enum (after Quadrant)**

```rust
    Architecture {
        title: Option<String>,
        #[serde(default)]
        height: Option<u32>,
        #[serde(default)]
        direction: ArchDirection,
        nodes: Vec<ArchNode>,
        connections: Vec<ArchConnection>,
    },
```

- [ ] **Step 3: Commit**

```bash
git add src/types.rs
git commit -m "feat(types): add Architecture component variant with ArchNode, ArchConnection, ArchDirection"
```

---

### Task 5: Add match arms in components.rs (stub dispatch)

**Files:**
- Modify: `src/render/components.rs`

- [ ] **Step 1: Add 4 match arms to the render() function**

In `src/render/components.rs`, in the `render()` function match block, after the `Component::RoleMap` arm (around line 156), add:

```rust
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
```

- [ ] **Step 2: Add stub functions in charts.rs so it compiles**

In `src/render/charts.rs`, add these stubs at the bottom (before `#[cfg(test)]`):

```rust
// ── Sankey ──────────────────────────────────────────

pub fn render_sankey(
    title: &Option<String>,
    height: Option<u32>,
    flows: &[crate::types::SankeyFlow],
    colors: &std::collections::HashMap<String, crate::types::SemColor>,
) -> Rendered {
    let _ = (height, flows, colors);
    let html = title.as_deref().map(|t| format!(r#"<div class="c-sankey"><div class="c-sankey-title">{}</div></div>"#, esc(t))).unwrap_or_default();
    Rendered::new(html)
}

// ── Radar ───────────────────────────────────────────

pub fn render_radar(
    title: &Option<String>,
    height: Option<u32>,
    axes: &[String],
    curves: &[crate::types::RadarCurve],
    max: Option<f64>,
) -> Rendered {
    let _ = (height, axes, curves, max);
    let html = title.as_deref().map(|t| format!(r#"<div class="c-radar"><div class="c-radar-title">{}</div></div>"#, esc(t))).unwrap_or_default();
    Rendered::new(html)
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
    let _ = (height, x_axis, y_axis, quadrants, points);
    let html = title.as_deref().map(|t| format!(r#"<div class="c-quadrant"><div class="c-quadrant-title">{}</div></div>"#, esc(t))).unwrap_or_default();
    Rendered::new(html)
}

// ── Architecture ────────────────────────────────────

pub fn render_architecture(
    title: &Option<String>,
    height: Option<u32>,
    direction: crate::types::ArchDirection,
    nodes: &[crate::types::ArchNode],
    connections: &[crate::types::ArchConnection],
) -> Rendered {
    let _ = (height, direction, nodes, connections);
    let html = title.as_deref().map(|t| format!(r#"<div class="c-arch"><div class="c-arch-title">{}</div></div>"#, esc(t))).unwrap_or_default();
    Rendered::new(html)
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: Clean compilation (possibly with unused-variable warnings from stubs - fine).

- [ ] **Step 4: Commit**

```bash
git add src/render/components.rs src/render/charts.rs
git commit -m "feat(render): wire up Sankey, Radar, Quadrant, Architecture dispatch with stub renderers"
```

---

### Task 6: Implement Sankey SVG renderer

**Files:**
- Modify: `src/render/charts.rs`

- [ ] **Step 1: Write the sankey test**

In `src/render/charts.rs`, in the `#[cfg(test)] mod tests` block, add:

```rust
    #[test]
    fn sankey_renders_svg_with_flows() {
        let flows = vec![
            crate::types::SankeyFlow { source: "A".into(), target: "B".into(), value: 80.0 },
            crate::types::SankeyFlow { source: "A".into(), target: "C".into(), value: 20.0 },
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test sankey_renders_svg`
Expected: FAIL - current stub doesn't output SVG.

- [ ] **Step 3: Replace the stub with full Sankey renderer**

Replace the `render_sankey` function in `src/render/charts.rs` with:

```rust
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

    // Collect unique node names and assign to columns via topology.
    let mut node_names: Vec<String> = Vec::new();
    for f in flows {
        if !node_names.contains(&f.source) {
            node_names.push(f.source.clone());
        }
        if !node_names.contains(&f.target) {
            node_names.push(f.target.clone());
        }
    }

    // Build adjacency: who does each node flow TO?
    let mut out_edges: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    let mut in_edges: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for f in flows {
        out_edges.entry(&f.source).or_default().push(&f.target);
        in_edges.entry(&f.target).or_default().push(&f.source);
    }

    // Assign columns: sources (no in-edges) = col 0, then BFS forward.
    let mut col: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for name in &node_names {
        if !in_edges.contains_key(name.as_str()) {
            col.insert(name, 0);
        }
    }
    // BFS to assign columns
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
    // Any unassigned nodes get col 0
    for name in &node_names {
        col.entry(name).or_insert(0);
    }

    let max_col = col.values().copied().max().unwrap_or(0);
    let num_cols = max_col + 1;

    // Compute total flow per node
    let mut node_total: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for f in flows {
        *node_total.entry(&f.source).or_default() += f.value;
        *node_total.entry(&f.target).or_default() += f.value;
    }
    // For intermediate nodes, use max(in, out) to avoid double-counting
    let mut node_in: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    let mut node_out: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    for f in flows {
        *node_out.entry(&f.source).or_default() += f.value;
        *node_in.entry(&f.target).or_default() += f.value;
    }
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

    // Position nodes: x from column, y stacked within column
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
        let gaps = if col_nodes.len() > 1 { (col_nodes.len() - 1) as f64 * node_gap } else { 0.0 };
        let avail_h = plot_h - gaps;
        let scale = if col_total > 0.0 { avail_h / col_total } else { 1.0 };

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
        vb_w = VB_W, h = h,
    );

    // Render links first (behind nodes)
    // Track how much of each node's height has been "used" by links
    let mut source_offset: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    let mut target_offset: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();

    for f in flows {
        let sp = &positions[f.source.as_str()];
        let tp = &positions[f.target.as_str()];
        let s_total = node_total[f.source.as_str()];
        let t_total = node_total[f.target.as_str()];

        let s_off = source_offset.entry(&f.source).or_insert(0.0);
        let link_h_source = if s_total > 0.0 { (f.value / s_total) * sp.h } else { 0.0 };
        let sy = sp.y + *s_off;
        *s_off += link_h_source;

        let t_off = target_offset.entry(&f.target).or_insert(0.0);
        let link_h_target = if t_total > 0.0 { (f.value / t_total) * tp.h } else { 0.0 };
        let ty = tp.y + *t_off;
        *t_off += link_h_target;

        let sx = sp.x + node_w;
        let tx = tp.x;
        let cpx = (sx + tx) / 2.0;

        let color = colors.get(&f.source)
            .or_else(|| colors.get(&f.target))
            .copied()
            .unwrap_or_else(|| cycle_color(node_names.iter().position(|n| n == &f.source).unwrap_or(0)));

        let title_text = format!("{} → {}: {}", f.source, f.target, fmt_num(f.value));

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

    // Render nodes
    for name in &node_names {
        let pos = &positions[name.as_str()];
        let color = colors.get(name.as_str()).copied()
            .unwrap_or_else(|| cycle_color(node_names.iter().position(|n| n == name).unwrap_or(0)));

        svg.push_str(&format!(
            r#"<rect x="{x:.1}" y="{y:.1}" width="{w}" height="{h:.1}" fill="{fill}" rx="2" class="c-sankey-node"><title>{name}: {total}</title></rect>"#,
            x = pos.x, y = pos.y, w = node_w, h = pos.h,
            fill = color.hex(),
            name = esc(name),
            total = fmt_num(node_total[name.as_str()]),
        ));

        // Label
        let c = col[name.as_str()];
        let (lx, anchor) = if c == 0 {
            (pos.x - 6.0, "end")
        } else if c == max_col {
            (pos.x + node_w + 6.0, "start")
        } else {
            (pos.x + node_w / 2.0, "middle")
        };
        let ly = pos.y + pos.h / 2.0;
        svg.push_str(&format!(
            r#"<text x="{lx:.1}" y="{ly:.1}" text-anchor="{anchor}" dominant-baseline="middle" class="c-sankey-label">{name}</text>"#,
            lx = lx, ly = ly, anchor = anchor, name = esc(name),
        ));
    }

    svg.push_str("</svg>");
    wrap_chart("sankey", title, &svg)
}

fn wrap_chart(kind: &str, title: &Option<String>, svg: &str) -> Rendered {
    let title_html = title
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|t| format!(r#"<figcaption class="c-chart-title">{}</figcaption>"#, esc(t)))
        .unwrap_or_default();
    Rendered::new(format!(
        r#"<figure class="c-chart c-chart-{kind}" role="img" aria-label="{aria}">{title}{svg}</figure>"#,
        kind = kind,
        aria = title.as_deref().map(|t| esc(t)).unwrap_or_else(|| format!("{} chart", kind)),
        title = title_html,
        svg = svg,
    ))
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test sankey`
Expected: Both sankey tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/render/charts.rs
git commit -m "feat(charts): implement Sankey SVG renderer with topological layout and Bezier links"
```

---

### Task 7: Implement Radar SVG renderer

**Files:**
- Modify: `src/render/charts.rs`

- [ ] **Step 1: Write the radar tests**

```rust
    #[test]
    fn radar_renders_svg_with_curves() {
        let curves = vec![
            crate::types::RadarCurve { label: "Before".into(), values: vec![1.0, 4.0, 2.0], color: None },
            crate::types::RadarCurve { label: "After".into(), values: vec![9.0, 6.0, 8.0], color: None },
        ];
        let axes = vec!["A".into(), "B".into(), "C".into()];
        let result = render_radar(&Some("Test".into()), None, &axes, &curves, Some(10.0));
        assert!(result.html.contains("<svg"), "should contain SVG");
        assert!(result.html.contains("c-radar"), "should have radar class");
        assert!(result.html.contains("polygon"), "should render data polygons");
    }

    #[test]
    fn radar_auto_max_from_data() {
        let curves = vec![
            crate::types::RadarCurve { label: "X".into(), values: vec![3.0, 7.0, 5.0], color: None },
        ];
        let axes = vec!["A".into(), "B".into(), "C".into()];
        let result = render_radar(&None, None, &axes, &curves, None);
        assert!(result.html.contains("<svg"), "should render even without explicit max");
    }
```

- [ ] **Step 2: Replace the radar stub with full implementation**

Replace `render_radar` in `src/render/charts.rs`:

```rust
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

    let auto_max = curves.iter()
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
        vb_w = VB_W, h = h,
    );

    // Graticule rings (polygon)
    for ring in 1..=rings {
        let frac = ring as f64 / rings as f64;
        let mut pts = String::new();
        for i in 0..n {
            let (px, py) = point_at(i, max_val * frac);
            if !pts.is_empty() { pts.push(' '); }
            pts.push_str(&format!("{:.1},{:.1}", px, py));
        }
        svg.push_str(&format!(
            r#"<polygon points="{pts}" fill="none" stroke="rgba(var(--text-rgb),0.1)" stroke-width="1" class="c-radar-ring"/>"#,
        ));
    }

    // Axis lines + labels
    for i in 0..n {
        let (ex, ey) = point_at(i, max_val);
        svg.push_str(&format!(
            r#"<line x1="{cx:.1}" y1="{cy:.1}" x2="{ex:.1}" y2="{ey:.1}" stroke="rgba(var(--text-rgb),0.12)" stroke-width="1" class="c-radar-axis"/>"#,
        ));
        let label_d = r + 16.0;
        let a = angle_of(i);
        let lx = cx + label_d * a.cos();
        let ly = cy + label_d * a.sin();
        let anchor = if (a.cos()).abs() < 0.1 { "middle" } else if a.cos() > 0.0 { "start" } else { "end" };
        svg.push_str(&format!(
            r#"<text x="{lx:.1}" y="{ly:.1}" text-anchor="{anchor}" dominant-baseline="middle" class="c-radar-label">{label}</text>"#,
            label = esc(&axes[i]),
        ));
    }

    // Data curves
    for (ci, curve) in curves.iter().enumerate() {
        let color = series_color(curve.color, ci);
        let mut pts = String::new();
        for (i, &val) in curve.values.iter().enumerate() {
            let (px, py) = point_at(i, val.min(max_val));
            if !pts.is_empty() { pts.push(' '); }
            pts.push_str(&format!("{:.1},{:.1}", px, py));
        }
        svg.push_str(&format!(
            r#"<polygon points="{pts}" fill="{color}" fill-opacity="0.18" stroke="{color}" stroke-width="2" class="c-radar-curve"/>"#,
        ));
        // Dots at each vertex
        for (i, &val) in curve.values.iter().enumerate() {
            let (px, py) = point_at(i, val.min(max_val));
            let tip = format!("{} - {}: {}", curve.label, axes[i], fmt_num(val));
            svg.push_str(&format!(
                r#"<circle cx="{px:.1}" cy="{py:.1}" r="3.5" fill="{color}" class="c-chart-dot"><title>{tip}</title></circle>"#,
                tip = esc(&tip),
            ));
        }
    }

    svg.push_str("</svg>");

    // Legend
    let legend = if curves.len() > 1 {
        let items: Vec<(String, String)> = curves.iter().enumerate()
            .map(|(i, c)| (c.label.clone(), series_color(c.color, i).to_string()))
            .collect();
        let mut leg = String::from(r#"<ul class="c-chart-legend">"#);
        for (label, color) in items {
            leg.push_str(&format!(
                r#"<li class="c-chart-legend-item"><span class="c-chart-swatch" style="background:{c}"></span><span>{l}</span></li>"#,
                c = color, l = esc(&label),
            ));
        }
        leg.push_str("</ul>");
        leg
    } else {
        String::new()
    };

    let title_html = title.as_deref().filter(|s| !s.is_empty())
        .map(|t| format!(r#"<figcaption class="c-chart-title">{}</figcaption>"#, esc(t)))
        .unwrap_or_default();

    Rendered::new(format!(
        r#"<figure class="c-chart c-chart-radar" role="img" aria-label="{aria}">{title}{svg}{legend}</figure>"#,
        aria = title.as_deref().map(|t| esc(t)).unwrap_or_else(|| "Radar chart".into()),
        title = title_html, svg = svg, legend = legend,
    ))
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test radar`
Expected: Both tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/render/charts.rs
git commit -m "feat(charts): implement Radar SVG renderer with polygon graticule and multi-curve overlay"
```

---

### Task 8: Implement Quadrant SVG renderer

**Files:**
- Modify: `src/render/charts.rs`

- [ ] **Step 1: Write the quadrant test**

```rust
    #[test]
    fn quadrant_renders_svg_with_points() {
        let points = vec![
            crate::types::QuadrantPoint { label: "A".into(), x: 0.9, y: 0.9, color: Some(crate::types::SemColor::Red) },
            crate::types::QuadrantPoint { label: "B".into(), x: 0.2, y: 0.3, color: None },
        ];
        let quads = vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()];
        let result = render_quadrant(&Some("Test".into()), None, "X", "Y", &quads, &points);
        assert!(result.html.contains("<svg"), "should contain SVG");
        assert!(result.html.contains("c-quadrant"), "should have quadrant class");
        assert!(result.html.contains("Q1"), "should render quadrant labels");
    }
```

- [ ] **Step 2: Replace the quadrant stub with full implementation**

Replace `render_quadrant` in `src/render/charts.rs`:

```rust
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

    let left = 60.0;
    let right = 20.0;
    let top = 20.0;
    let bottom = 40.0;
    let plot_w = VB_W - left - right;
    let plot_h = h - top - bottom;
    let mid_x = left + plot_w / 2.0;
    let mid_y = top + plot_h / 2.0;

    let mut svg = format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg">"#,
        vb_w = VB_W, h = h,
    );

    // Quadrant background fills
    let quad_colors = ["rgba(52,211,153,0.06)", "rgba(251,191,36,0.04)", "rgba(248,113,113,0.06)", "rgba(60,206,206,0.04)"];
    // Q1=top-right, Q2=top-left, Q3=bottom-left, Q4=bottom-right
    let quad_rects = [
        (mid_x, top, plot_w / 2.0, plot_h / 2.0),         // Q1 top-right
        (left, top, plot_w / 2.0, plot_h / 2.0),           // Q2 top-left
        (left, mid_y, plot_w / 2.0, plot_h / 2.0),         // Q3 bottom-left
        (mid_x, mid_y, plot_w / 2.0, plot_h / 2.0),        // Q4 bottom-right
    ];
    for (i, (qx, qy, qw, qh)) in quad_rects.iter().enumerate() {
        svg.push_str(&format!(
            r#"<rect x="{qx:.1}" y="{qy:.1}" width="{qw:.1}" height="{qh:.1}" fill="{fill}" class="c-quadrant-bg"/>"#,
            fill = quad_colors[i],
        ));
        // Quadrant label in center of each zone
        let lx = qx + qw / 2.0;
        let ly = qy + qh / 2.0;
        svg.push_str(&format!(
            r#"<text x="{lx:.1}" y="{ly:.1}" text-anchor="middle" dominant-baseline="middle" class="c-quadrant-zone-label">{label}</text>"#,
            label = esc(&quadrants[i]),
        ));
    }

    // Crosshair lines
    svg.push_str(&format!(
        r#"<line x1="{mid_x:.1}" y1="{top:.1}" x2="{mid_x:.1}" y2="{bot:.1}" class="c-quadrant-cross"/>"#,
        bot = top + plot_h,
    ));
    svg.push_str(&format!(
        r#"<line x1="{left:.1}" y1="{mid_y:.1}" x2="{right:.1}" y2="{mid_y:.1}" class="c-quadrant-cross"/>"#,
        right = left + plot_w,
    ));

    // Axis labels
    let x_parts: Vec<&str> = x_axis.split(" → ").collect();
    if x_parts.len() == 2 {
        svg.push_str(&format!(
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="start" class="c-chart-axis">{t}</text>"#,
            x = left, y = top + plot_h + 24.0, t = esc(x_parts[0]),
        ));
        svg.push_str(&format!(
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="end" class="c-chart-axis">{t}</text>"#,
            x = left + plot_w, y = top + plot_h + 24.0, t = esc(x_parts[1]),
        ));
    } else {
        svg.push_str(&format!(
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="middle" class="c-chart-axis">{t}</text>"#,
            x = mid_x, y = top + plot_h + 24.0, t = esc(x_axis),
        ));
    }

    let y_parts: Vec<&str> = y_axis.split(" → ").collect();
    if y_parts.len() == 2 {
        svg.push_str(&format!(
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="middle" transform="rotate(-90,{x:.1},{y:.1})" class="c-chart-axis">{t}</text>"#,
            x = left - 30.0, y = top + plot_h, t = esc(y_parts[0]),
        ));
        svg.push_str(&format!(
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="middle" transform="rotate(-90,{x:.1},{y:.1})" class="c-chart-axis">{t}</text>"#,
            x = left - 30.0, y = top, t = esc(y_parts[1]),
        ));
    } else {
        svg.push_str(&format!(
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="middle" transform="rotate(-90,{x:.1},{y:.1})" class="c-chart-axis">{t}</text>"#,
            x = left - 30.0, y = mid_y, t = esc(y_axis),
        ));
    }

    // Points
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
            lx = px + 10.0, label = esc(&pt.label),
        ));
    }

    svg.push_str("</svg>");
    wrap_chart("quadrant", title, &svg)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test quadrant`
Expected: Pass.

- [ ] **Step 4: Commit**

```bash
git add src/render/charts.rs
git commit -m "feat(charts): implement Quadrant SVG renderer with zone backgrounds and positioned points"
```

---

### Task 9: Implement Architecture SVG renderer

**Files:**
- Modify: `src/render/charts.rs`

- [ ] **Step 1: Write the architecture test**

```rust
    #[test]
    fn architecture_renders_svg_with_nodes() {
        let nodes = vec![
            crate::types::ArchNode { id: "a".into(), label: "Source".into(), detail: None, icon: None, color: crate::types::SemColor::Teal },
            crate::types::ArchNode { id: "b".into(), label: "Sink".into(), detail: Some("Details".into()), icon: None, color: crate::types::SemColor::Green },
        ];
        let conns = vec![
            crate::types::ArchConnection { from: "a".into(), to: "b".into(), label: Some("flow".into()) },
        ];
        let result = render_architecture(&Some("Test".into()), None, crate::types::ArchDirection::LeftToRight, &nodes, &conns);
        assert!(result.html.contains("<svg"), "should contain SVG");
        assert!(result.html.contains("c-arch"), "should have arch class");
        assert!(result.html.contains("Source"), "should contain node label");
        assert!(result.html.contains("flow"), "should contain edge label");
    }
```

- [ ] **Step 2: Replace the architecture stub with full implementation**

Replace `render_architecture` in `src/render/charts.rs`:

```rust
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

    // Assign columns based on topology
    let mut col: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut in_edges: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for c in connections {
        in_edges.insert(&c.to);
    }
    // Sources (no in-edges) start at column 0
    for n in nodes {
        if !in_edges.contains(n.id.as_str()) {
            col.insert(&n.id, 0);
        }
    }
    // BFS forward
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
    let node_w = if is_lr { 130.0 } else { 130.0 };
    let node_h = 56.0;

    // Group nodes by column
    let mut cols_nodes: Vec<Vec<&crate::types::ArchNode>> = vec![Vec::new(); num_cols];
    for n in nodes {
        let c = col.get(n.id.as_str()).copied().unwrap_or(0);
        cols_nodes[c].push(n);
    }

    // Position nodes
    struct NPos { cx: f64, cy: f64 }
    let mut positions: std::collections::HashMap<&str, NPos> = std::collections::HashMap::new();

    if is_lr {
        let col_spacing = if num_cols > 1 { (VB_W - pad * 2.0 - node_w) / (num_cols as f64 - 1.0).max(1.0) } else { 0.0 };
        for (ci, col_nodes) in cols_nodes.iter().enumerate() {
            let n_count = col_nodes.len();
            let total_h = n_count as f64 * node_h + (n_count as f64 - 1.0).max(0.0) * 16.0;
            let start_y = (h - total_h) / 2.0;
            for (ni, node) in col_nodes.iter().enumerate() {
                let cx = pad + node_w / 2.0 + col_spacing * ci as f64;
                let cy = start_y + ni as f64 * (node_h + 16.0) + node_h / 2.0;
                positions.insert(&node.id, NPos { cx, cy });
            }
        }
    } else {
        let row_spacing = if num_cols > 1 { (h - pad * 2.0 - node_h) / (num_cols as f64 - 1.0).max(1.0) } else { 0.0 };
        for (ci, col_nodes) in cols_nodes.iter().enumerate() {
            let n_count = col_nodes.len();
            let total_w = n_count as f64 * node_w + (n_count as f64 - 1.0).max(0.0) * 24.0;
            let start_x = (VB_W - total_w) / 2.0;
            for (ni, node) in col_nodes.iter().enumerate() {
                let cx = start_x + ni as f64 * (node_w + 24.0) + node_w / 2.0;
                let cy = pad + node_h / 2.0 + row_spacing * ci as f64;
                positions.insert(&node.id, NPos { cx, cy });
            }
        }
    }

    let mut svg = format!(
        r#"<svg viewBox="0 0 {vb_w} {h}" preserveAspectRatio="xMidYMid meet" class="c-chart-svg">"#,
        vb_w = VB_W, h = h,
    );

    // Arrow marker
    svg.push_str(r#"<defs><marker id="arch-arrow" viewBox="0 0 10 7" refX="10" refY="3.5" markerWidth="8" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 3.5 L 0 7 z" fill="rgba(var(--text-rgb),0.5)"/></marker></defs>"#);

    // Connections
    for conn in connections {
        let Some(fp) = positions.get(conn.from.as_str()) else { continue };
        let Some(tp) = positions.get(conn.to.as_str()) else { continue };

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

    // Nodes
    for n in nodes {
        let Some(pos) = positions.get(n.id.as_str()) else { continue };
        let rx = pos.cx - node_w / 2.0;
        let ry = pos.cy - node_h / 2.0;
        let color = n.color;

        svg.push_str(&format!(
            r#"<rect x="{rx:.1}" y="{ry:.1}" width="{nw}" height="{nh}" rx="8" fill="rgba(var(--text-rgb),0.06)" stroke="{stroke}" stroke-width="1.5" class="c-arch-node"/>"#,
            nw = node_w, nh = node_h, stroke = color.hex(),
        ));
        svg.push_str(&format!(
            r#"<text x="{cx:.1}" y="{cy:.1}" text-anchor="middle" dominant-baseline="middle" class="c-arch-node-label">{label}</text>"#,
            cx = pos.cx, cy = if n.detail.is_some() { pos.cy - 7.0 } else { pos.cy }, label = esc(&n.label),
        ));
        if let Some(detail) = &n.detail {
            svg.push_str(&format!(
                r#"<text x="{cx:.1}" y="{cy:.1}" text-anchor="middle" dominant-baseline="middle" class="c-arch-node-detail">{d}</text>"#,
                cx = pos.cx, cy = pos.cy + 10.0, d = esc(detail),
            ));
        }
    }

    svg.push_str("</svg>");
    wrap_chart("arch", title, &svg)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test architecture`
Expected: Pass.

- [ ] **Step 4: Commit**

```bash
git add src/render/charts.rs
git commit -m "feat(charts): implement Architecture SVG renderer with topological layout and arrow connections"
```

---

### Task 10: Add CSS for all 4 components

**Files:**
- Modify: `src/theme.rs`

- [ ] **Step 1: Add CSS rules after the existing chart CSS block (after `.c-chart-swatch`, around line 2357)**

Find the line `/* Source pill */` in `src/theme.rs` and insert before it:

```css
/* Sankey */
.c-sankey-link { transition: fill-opacity 0.15s; }
.c-sankey-link:hover { fill-opacity: 0.55; }
.c-sankey-node { stroke: var(--bg); stroke-width: 1; }
.c-sankey-label {
  fill: rgba(var(--text-rgb),0.75);
  font-size: 12px;
  font-weight: 500;
}

/* Radar */
.c-radar-ring { pointer-events: none; }
.c-radar-axis { pointer-events: none; }
.c-radar-label {
  fill: rgba(var(--text-rgb),0.65);
  font-size: 11px;
  font-weight: 500;
}
.c-radar-curve { pointer-events: none; }

/* Quadrant */
.c-quadrant-bg { pointer-events: none; }
.c-quadrant-zone-label {
  fill: rgba(var(--text-rgb),0.15);
  font-size: 18px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 1px;
  pointer-events: none;
}
.c-quadrant-cross {
  stroke: rgba(var(--text-rgb),0.15);
  stroke-width: 1;
  stroke-dasharray: 6 4;
}
.c-quadrant-point-label {
  fill: rgba(var(--text-rgb),0.75);
  font-size: 11px;
  font-weight: 500;
}

/* Architecture */
.c-arch-node { transition: stroke-opacity 0.15s; }
.c-arch-node-label {
  fill: var(--snow);
  font-size: 13px;
  font-weight: 600;
}
.c-arch-node-detail {
  fill: rgba(var(--text-rgb),0.55);
  font-size: 10px;
  font-weight: 400;
}
.c-arch-edge { pointer-events: none; }
.c-arch-edge-label {
  fill: rgba(var(--text-rgb),0.55);
  font-size: 10px;
  font-weight: 500;
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: Clean.

- [ ] **Step 3: Commit**

```bash
git add src/theme.rs
git commit -m "feat(theme): add CSS for Sankey, Radar, Quadrant, and Architecture components"
```

---

### Task 11: Add validation rules

**Files:**
- Modify: `src/validate.rs`

- [ ] **Step 1: Add validation arms for each new component**

In `src/validate.rs`, find the end of the existing component validation match block. After the last existing arm (before any catch-all `_ =>`), add:

```rust
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
                        &format!("curve values length ({}) must match axes count ({})", c.values.len(), axes.len()),
                        None,
                    ));
                }
            }
        }

        Component::Quadrant { quadrants, points, .. } => {
            if quadrants.len() != 4 {
                errors.push(ValidationError::new(
                    file,
                    format!("{}.quadrants", path),
                    "structural",
                    &format!("quadrant requires exactly 4 labels, got {}", quadrants.len()),
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

        Component::Architecture { nodes, connections, .. } => {
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
                        &format!("connection references unknown node id {:?}", c.from),
                        None,
                    ));
                }
                if !ids.contains(&c.to.as_str()) {
                    errors.push(ValidationError::new(
                        file,
                        format!("{}.connections[{}].to", path, i),
                        "structural",
                        &format!("connection references unknown node id {:?}", c.to),
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
```

- [ ] **Step 2: Run validation tests**

Run: `cargo test validate`
Expected: All existing validation tests pass (new components don't break existing tests).

- [ ] **Step 3: Commit**

```bash
git add src/validate.rs
git commit -m "feat(validate): add validation rules for Sankey, Radar, Quadrant, and Architecture"
```

---

### Task 12: Update SDK emitter

**Files:**
- Modify: `src/sdk.rs`

- [ ] **Step 1: Add new enum to the enums list**

Find the `enums` vec (around line 1693) and add after `ChartOrientation`:

```rust
        ("ArchDirection", vec!["left_to_right", "top_to_bottom"]),
```

- [ ] **Step 2: Add new interfaces to the interfaces list**

Find the `interfaces` vec and add after the existing entries:

```rust
        (
            "SankeyFlow",
            vec![
                ("source", "string", true),
                ("target", "string", true),
                ("value", "number", true),
            ],
        ),
        (
            "RadarCurve",
            vec![
                ("label", "string", true),
                ("values", "number[]", true),
                ("color", "SemColor", false),
            ],
        ),
        (
            "QuadrantPoint",
            vec![
                ("label", "string", true),
                ("x", "number", true),
                ("y", "number", true),
                ("color", "SemColor", false),
            ],
        ),
        (
            "ArchNode",
            vec![
                ("id", "string", true),
                ("label", "string", true),
                ("detail", "string", false),
                ("icon", "string", false),
                ("color", "SemColor", false),
            ],
        ),
        (
            "ArchConnection",
            vec![
                ("from", "string", true),
                ("to", "string", true),
                ("label", "string", false),
            ],
        ),
```

- [ ] **Step 3: Add icons for new component types**

Find the `icons` vec (around line 1828) and add:

```rust
        ("sankey", "\u{21c9}"),
        ("radar", "\u{25ce}"),
        ("quadrant", "\u{229e}"),
        ("architecture", "\u{2b1a}"),
```

- [ ] **Step 4: Add React renderer cases in the ComponentView switch**

Find the `default:` case in the `ComponentView` switch (around line 1532) and add before it:

```typescript
    case "sankey":
    case "radar":
    case "quadrant":
    case "architecture": {
      return (
        <div id={id} className={`c-chart c-chart-${comp.type}`}>
          <em>{comp.type} chart (server-rendered SVG)</em>
        </div>
      );
    }
```

- [ ] **Step 5: Verify it compiles**

Run: `cargo check`
Expected: Clean.

- [ ] **Step 6: Commit**

```bash
git add src/sdk.rs
git commit -m "feat(sdk): add TS types, enum values, icons, and React stubs for new chart components"
```

---

### Task 13: Create demo page and test locally

**Files:**
- Create: `examples/kb/demo/charts.yaml`

- [ ] **Step 1: Create the demo page**

Create `examples/kb/demo/charts.yaml`:

```yaml
title: "Chart Components Showcase"
shell: standard

components:
- type: section
  eyebrow: "SANKEY"
  heading: "Finding Flow - Vulnerability Triage Pipeline"
  components:
  - type: sankey
    title: "Scanner → Maze → Outcome"
    height: 380
    flows:
    - source: "Wiz"
      target: "Not Exploitable"
      value: 35000
    - source: "Wiz"
      target: "Exploitable"
      value: 5100
    - source: "AWS Inspector"
      target: "Not Exploitable"
      value: 9000
    - source: "AWS Inspector"
      target: "Exploitable"
      value: 2000
    - source: "Exploitable"
      target: "Critical"
      value: 92
    - source: "Exploitable"
      target: "High"
      value: 3200
    - source: "Exploitable"
      target: "Medium"
      value: 2500
    - source: "Exploitable"
      target: "Low"
      value: 1308
    colors:
      "Not Exploitable": green
      "Exploitable": red
      "Critical": red
      "High": yellow
      "Medium": teal
      "Low": default

- type: divider

- type: section
  eyebrow: "RADAR"
  heading: "Security Program Maturity - Before & After"
  components:
  - type: radar
    title: "Maturity Scorecard"
    height: 400
    max: 10
    axes:
    - "Noise Reduction"
    - "Asset Coverage"
    - "Triage Automation"
    - "SLA Tracking"
    - "Remediation Speed"
    - "Team Strategic Focus"
    curves:
    - label: "Before Maze"
      values: [1, 4, 2, 1, 2, 2]
      color: red
    - label: "Today"
      values: [9, 6, 8, 5, 8, 9]
      color: green

- type: divider

- type: section
  eyebrow: "QUADRANT"
  heading: "Finding Prioritization Matrix"
  components:
  - type: quadrant
    title: "Exploitability vs Business Impact"
    height: 420
    x_axis: "Low Exploitability → High Exploitability"
    y_axis: "Low Business Impact → High Business Impact"
    quadrants:
    - "Fix Now"
    - "Monitor"
    - "Accept Risk"
    - "Defer"
    points:
    - label: "EKS Critical CVEs"
      x: 0.92
      y: 0.95
      color: red
    - label: "EC2 High Findings"
      x: 0.72
      y: 0.65
      color: yellow
    - label: "S3 Misconfigs"
      x: 0.55
      y: 0.45
      color: yellow
    - label: "Dev Cluster Medium"
      x: 0.18
      y: 0.15
      color: green
    - label: "Staging Low"
      x: 0.12
      y: 0.08
      color: green

- type: divider

- type: section
  eyebrow: "ARCHITECTURE"
  heading: "Deployment Topology"
  components:
  - type: architecture
    title: "Alloy - Maze Deployment"
    height: 280
    direction: left_to_right
    nodes:
    - id: wiz
      label: "Wiz"
      detail: "Cloud Scanner"
      color: red
    - id: inspector
      label: "AWS Inspector"
      detail: "Vuln Scanner"
      color: red
    - id: maze
      label: "Maze"
      detail: "AI Triage Engine"
      color: teal
    - id: jira
      label: "Jira"
      detail: "Ticket Routing"
      color: green
    - id: slack
      label: "Slack"
      detail: "Critical Alerts"
      color: yellow
    - id: github
      label: "GitHub"
      detail: "Team Ownership"
      color: default
    connections:
    - from: wiz
      to: maze
      label: "40K findings"
    - from: inspector
      to: maze
      label: "11K findings"
    - from: maze
      to: jira
      label: "7.1K exploitable"
    - from: maze
      to: slack
      label: "92 critical"
    - from: maze
      to: github
      label: "ownership"
```

- [ ] **Step 2: Build the example site**

Run: `cargo run -- build examples/kb`
Expected: Clean build, chart-showcase page generated in `examples/kb/_site/demo/charts.html`.

- [ ] **Step 3: Serve locally and verify**

Run: `cargo run -- dev examples/kb`
Expected: Dev server starts. Open `http://localhost:3000/demo/charts.html` in a browser. Verify:
- Sankey shows flows from scanners through Maze to outcomes with proportional widths
- Radar shows two overlaid polygons (Before vs Today) on a 6-axis spider
- Quadrant shows 5 points in their correct zones with subtle background tints
- Architecture shows a left-to-right flow: Wiz/Inspector → Maze → Jira/Slack/GitHub

- [ ] **Step 4: Commit**

```bash
git add examples/kb/demo/charts.yaml
git commit -m "feat(examples): add chart components showcase page with security-themed demo data"
```

---

### Task 14: Run full test suite and final build

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: All tests pass including new sankey/radar/quadrant/architecture tests.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy`
Expected: No warnings (or only pre-existing ones).

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 4: Full build of example site**

Run: `cargo run -- build examples/kb`
Expected: Clean build, no validation errors.

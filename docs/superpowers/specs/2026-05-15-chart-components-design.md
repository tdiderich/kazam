# Chart Components: Sankey, Radar, Quadrant, Architecture

Four new top-level Component variants rendered as build-time SVG. No JS. Follows the Venn precedent (separate component, not under ChartKind).

## 1. Sankey

Shows proportional flow between categories. Primary use: vulnerability findings flowing from scanners through Maze triage to outcomes.

### YAML Schema

```yaml
- type: sankey
  title: "Finding Flow"        # optional
  height: 300                  # optional, default 400
  flows:
    - source: "Wiz"
      target: "Not Exploitable"
      value: 35000
    - source: "Wiz"
      target: "Exploitable"
      value: 5000
    - source: "AWS Inspector"
      target: "Not Exploitable"
      value: 9000
    - source: "AWS Inspector"
      target: "Exploitable"
      value: 2100
  colors:                      # optional per-node color overrides
    "Not Exploitable": green
    "Exploitable": red
```

### Data Model (types.rs)

```rust
Component::Sankey {
    title: Option<String>,
    height: Option<u32>,
    flows: Vec<SankeyFlow>,
    #[serde(default)]
    colors: HashMap<String, SemColor>,
}

struct SankeyFlow {
    source: String,
    target: String,
    value: f64,
}
```

### Rendering

- Nodes laid out in columns via topological sort (sources left, sinks right, intermediaries in between).
- Node height proportional to total flow through that node.
- Links rendered as cubic Bezier paths (`C` command), width proportional to flow value.
- Link fill: gradient from source color to target color (or source color at 40% opacity).
- Node labels rendered to the left of first-column nodes and right of last-column nodes, centered for middle columns.
- SVG viewBox: 720 x height (matching existing VB_W constant).

### Validation

- Must have at least one flow.
- All values must be > 0.
- No self-referential flows (source == target).

---

## 2. Radar

Multi-axis comparison chart. Primary use: security maturity scorecard (before/after).

### YAML Schema

```yaml
- type: radar
  title: "Security Maturity"   # optional
  height: 360                  # optional, default 360
  axes:
    - "Noise Reduction"
    - "Coverage"
    - "Automation"
    - "SLA Tracking"
    - "Remediation Speed"
    - "Team Focus"
  max: 10                      # optional, default auto from data
  curves:
    - label: "Before Maze"
      values: [1, 4, 2, 1, 2, 2]
      color: red               # optional
    - label: "Today"
      values: [9, 6, 8, 5, 8, 9]
      color: green             # optional
```

### Data Model (types.rs)

```rust
Component::Radar {
    title: Option<String>,
    height: Option<u32>,
    axes: Vec<String>,
    curves: Vec<RadarCurve>,
    #[serde(default)]
    max: Option<f64>,
}

struct RadarCurve {
    label: String,
    values: Vec<f64>,
    #[serde(default)]
    color: Option<SemColor>,
}
```

### Rendering

- Polygon graticule (not circles) — 5 concentric rings from center to max value.
- Axis lines from center to each vertex, labeled at the outer edge.
- Each curve rendered as a filled polygon with low opacity (0.2) and a solid stroke (2px).
- Legend rendered below the chart (reuse existing `render_legend` pattern).
- Center of chart at (VB_W/2, h/2). Radius = min(VB_W, h)/2 - 60 (padding for labels).

### Validation

- At least 3 axes (fewer doesn't make visual sense).
- Each curve's values.len() must equal axes.len().
- At least one curve.

---

## 3. Quadrant

2D scatter with four labeled zones. Primary use: finding prioritization matrix.

### YAML Schema

```yaml
- type: quadrant
  title: "Finding Prioritization"  # optional
  height: 400                      # optional, default 400
  x_axis: "Low Exploitability → High Exploitability"
  y_axis: "Low Impact → High Impact"
  quadrants:
    - "Fix Now"         # top-right (Q1)
    - "Monitor"         # top-left (Q2)
    - "Accept Risk"     # bottom-left (Q3)
    - "Defer"           # bottom-right (Q4)
  points:
    - label: "EKS Critical"
      x: 0.9
      y: 0.95
      color: red               # optional
    - label: "EC2 High"
      x: 0.7
      y: 0.6
      color: yellow            # optional
    - label: "Dev Medium"
      x: 0.2
      y: 0.15
      color: green             # optional
```

### Data Model (types.rs)

```rust
Component::Quadrant {
    title: Option<String>,
    height: Option<u32>,
    x_axis: String,
    y_axis: String,
    quadrants: Vec<String>,    // exactly 4: Q1 (top-right), Q2 (top-left), Q3 (bottom-left), Q4 (bottom-right)
    points: Vec<QuadrantPoint>,
}

struct QuadrantPoint {
    label: String,
    x: f64,     // 0.0 to 1.0
    y: f64,     // 0.0 to 1.0
    #[serde(default)]
    color: Option<SemColor>,
}
```

### Rendering

- Plot area with left/bottom padding for axis labels.
- Four quadrants filled with subtle background tints (top-right = green-tinted, bottom-left = red-tinted, etc.).
- Dashed crosshair lines at x=0.5, y=0.5.
- Quadrant labels rendered large and faint in each zone center.
- Points rendered as filled circles (r=6) with label text offset to the right.
- Axis labels rendered along the bottom (x) and left (y) edges.
- x_axis text split on " → " to place left/right labels at axis extremes.

### Validation

- Exactly 4 quadrant labels.
- At least one point.
- All x, y values in [0.0, 1.0].

---

## 4. Architecture

System topology diagram. Primary use: deployment architecture showing AWS accounts, scanners, Maze, remediation tools.

### YAML Schema

```yaml
- type: architecture
  title: "Deployment Architecture"  # optional
  height: 300                       # optional, default 300
  direction: left_to_right          # optional, default left_to_right (or top_to_bottom)
  nodes:
    - id: scanners
      label: "Scanners"
      detail: "Wiz + Inspector"     # optional subtitle
      icon: shield                  # optional icon name
      color: red                    # optional
    - id: maze
      label: "Maze"
      detail: "AI Triage Engine"
      icon: cpu
      color: teal
    - id: jira
      label: "Jira"
      detail: "Ticket Creation"
      icon: ticket
    - id: slack
      label: "Slack"
      detail: "Alerts"
      icon: message
  connections:
    - from: scanners
      to: maze
      label: "51K findings"         # optional edge label
    - from: maze
      to: jira
      label: "Exploitable"
    - from: maze
      to: slack
      label: "Critical"
```

### Data Model (types.rs)

```rust
Component::Architecture {
    title: Option<String>,
    height: Option<u32>,
    #[serde(default)]
    direction: ArchDirection,
    nodes: Vec<ArchNode>,
    connections: Vec<ArchConnection>,
}

#[derive(Default)]
enum ArchDirection {
    #[default]
    LeftToRight,
    TopToBottom,
}

struct ArchNode {
    id: String,
    label: String,
    detail: Option<String>,
    icon: Option<String>,
    #[serde(default)]
    color: SemColor,
}

struct ArchConnection {
    from: String,
    to: String,
    label: Option<String>,
}
```

### Rendering

- Layout: auto-column assignment based on connection topology (sources in column 0, sinks in last column). Nodes with no in-edges go left; nodes with no out-edges go right.
- Node rendering: rounded rectangle (rx=8) with label text centered, optional detail text below in smaller font. Optional icon above label.
- Connections: straight arrows with arrowhead markers. Edge labels rendered at midpoint.
- Column spacing: equal divisions of VB_W. Row spacing: equal divisions of height within each column.

### Validation

- At least one node.
- All connection `from`/`to` ids must reference existing node ids.
- No self-connections.

---

## Files Modified

| File | Changes |
|------|---------|
| `src/types.rs` | Add 4 Component variants + 7 supporting structs + 1 enum |
| `src/render/charts.rs` | Add `render_sankey`, `render_radar`, `render_quadrant`, `render_architecture` functions + helpers |
| `src/render/components.rs` | Add 4 match arms dispatching to chart renderers |
| `src/theme.rs` | Add CSS for `.c-sankey-*`, `.c-radar-*`, `.c-quadrant-*`, `.c-arch-*` |
| `src/validate.rs` | Add validation rules for each new component |
| `src/sdk.rs` | Add TS types and React renderers for each new component |

## Demo Page

A YAML page at `sites/demo/pages/chart-showcase.yaml` exercising all 4 new components with security/vulnerability blueprint examples. Served via `kazam dev sites/demo`.

## Non-Goals

- Animation or interactivity (consistent with existing charts)
- Drag-and-drop node positioning for architecture diagrams
- Custom color palettes beyond SemColor

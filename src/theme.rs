use crate::types::{Glow, Mode, Texture};
use std::collections::HashMap;

/// A theme is a set of named color tokens. Any page rendered with this theme
/// gets its CSS `:root` block populated from these tokens; the rest of the CSS
/// references them via `var(--token)` so component styles are theme-agnostic.
#[derive(Clone)]
pub struct Theme {
    pub bg: String,
    pub surface: String,        // card backgrounds (low-contrast overlay)
    pub surface_strong: String, // stronger surface (code, kbd)
    pub border: String,         // default border color
    pub border_strong: String,  // stronger/active border
    pub accent: String,         // primary brand color (teal by default)
    pub accent_soft: String,    // accent on a translucent background
    pub text: String,           // primary text
    pub text_muted: String,     // secondary text
    pub text_subtle: String,    // tertiary (labels, captions)
    pub overlay_hover: String,  // hover/active surface overlay
    pub green: String,
    pub yellow: String,
    pub red: String,
    pub header_border: String,
}

impl Theme {
    /// Resolve a theme name + mode to a concrete Theme. `dark` and `light`
    /// are self-contained and ignore `mode`. Rainbow themes pick up the
    /// mode-appropriate base and swap the accent on top.
    pub fn named(name: &str, mode: Mode) -> Theme {
        match name {
            "dark" => dark(),
            "light" => light(),
            other => {
                let base = match mode {
                    Mode::Dark => dark(),
                    Mode::Light => light(),
                };
                // Muted, earthy accents sibling to the dark theme's sage
                // (#899878). ~45% saturation, ~60% lightness — they sit on
                // a dark bg without screaming. Users can still override any
                // accent via `colors:` for a brighter brand pop.
                let accent = match other {
                    "red" => "#BB7777",
                    "orange" => "#BB8C66",
                    "yellow" => "#B8A866",
                    "green" => "#7A9878",
                    "blue" => "#7897B8",
                    "indigo" => "#8A7FBB",
                    "violet" => "#AB7FBB",
                    _ => return dark(),
                };
                Theme {
                    accent: accent.into(),
                    ..base
                }
            }
        }
    }

    /// Apply a map of user overrides on top of this theme. Keys that don't
    /// match any known token are silently ignored.
    pub fn with_overrides(mut self, colors: &HashMap<String, String>) -> Theme {
        macro_rules! apply {
            ($key:literal, $field:ident) => {
                if let Some(v) = colors.get($key) {
                    self.$field = v.clone();
                }
            };
        }
        apply!("bg", bg);
        apply!("surface", surface);
        apply!("surface_strong", surface_strong);
        apply!("border", border);
        apply!("border_strong", border_strong);
        apply!("accent", accent);
        apply!("accent_soft", accent_soft);
        apply!("text", text);
        apply!("text_muted", text_muted);
        apply!("text_subtle", text_subtle);
        apply!("overlay_hover", overlay_hover);
        apply!("green", green);
        apply!("yellow", yellow);
        apply!("red", red);
        apply!("header_border", header_border);
        self
    }

    fn root_block(&self) -> String {
        let accent_rgb = hex_to_rgb_triple(&self.accent).unwrap_or_else(|| "60, 206, 206".into());
        let bg_rgb = hex_to_rgb_triple(&self.bg).unwrap_or_else(|| "9, 13, 24".into());
        let text_rgb = hex_to_rgb_triple(&self.text).unwrap_or_else(|| "255, 255, 255".into());
        format!(
            ":root {{\
             --bg: {bg};\
             --bg-rgb: {bg_rgb};\
             --card-bg: {surface};\
             --card-border: {border};\
             --card-hover-border: {border_strong};\
             --teal: {accent};\
             --accent-rgb: {accent_rgb};\
             --accent-soft: {accent_soft};\
             --snow: {text};\
             --text-rgb: {text_rgb};\
             --muted: {text_subtle};\
             --light-muted: {text_muted};\
             --header-border: {header_border};\
             --surface-strong: {surface_strong};\
             --overlay-hover: {overlay_hover};\
             --green: {green};\
             --yellow: {yellow};\
             --red: {red};\
             }}\n",
            bg = self.bg,
            bg_rgb = bg_rgb,
            surface = self.surface,
            surface_strong = self.surface_strong,
            border = self.border,
            border_strong = self.border_strong,
            accent = self.accent,
            accent_rgb = accent_rgb,
            accent_soft = self.accent_soft,
            text = self.text,
            text_rgb = text_rgb,
            text_muted = self.text_muted,
            text_subtle = self.text_subtle,
            overlay_hover = self.overlay_hover,
            green = self.green,
            yellow = self.yellow,
            red = self.red,
            header_border = self.header_border,
        )
    }
}

fn hex_to_rgb_triple(hex: &str) -> Option<String> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(format!("{}, {}, {}", r, g, b))
}

pub fn dark() -> Theme {
    // Surface/border at the old 3–7% range read too subtle against
    // #121113 — cards, code blocks, and meta grids faded into the bg.
    // Bumped to 5/9/11% for clearer card definition without making the
    // chrome feel heavy.
    Theme {
        bg: "#121113".into(),
        surface: "rgba(var(--text-rgb), 0.08)".into(),
        surface_strong: "rgba(var(--text-rgb), 0.09)".into(),
        border: "rgba(var(--text-rgb), 0.11)".into(),
        border_strong: "rgba(var(--accent-rgb), 0.35)".into(),
        accent: "#899878".into(),
        accent_soft: "rgba(var(--accent-rgb), 0.10)".into(),
        text: "#F7F7F2".into(),
        text_muted: "#B0B3AD".into(),
        text_subtle: "#6E726C".into(),
        overlay_hover: "rgba(var(--text-rgb), 0.10)".into(),
        green: "#899878".into(),
        yellow: "#E4E6C3".into(),
        red: "#C97B8A".into(),
        header_border: "rgba(var(--accent-rgb), 0.15)".into(),
    }
}

pub fn light() -> Theme {
    // Light mode needs roughly 2x the overlay opacity dark mode uses — a
    // 4% near-black wash on paper reads as invisible, while a 4% white
    // wash on #121113 reads clearly. Bumping surface/border/overlay values
    // keeps card definition, code blocks, and meta grids from washing out.
    // `text_subtle` also moves off sage onto a neutral muted gray so label
    // chrome ("AUTHOR", table headers) stays legible on paper.
    Theme {
        bg: "#F7F7F2".into(),
        surface: "rgba(var(--text-rgb), 0.08)".into(),
        surface_strong: "rgba(var(--text-rgb), 0.10)".into(),
        border: "rgba(var(--text-rgb), 0.16)".into(),
        border_strong: "rgba(var(--accent-rgb), 0.45)".into(),
        accent: "#222725".into(),
        accent_soft: "rgba(var(--accent-rgb), 0.08)".into(),
        text: "#121113".into(),
        text_muted: "#3D423F".into(),
        text_subtle: "#6B6F65".into(),
        overlay_hover: "rgba(var(--text-rgb), 0.12)".into(),
        green: "#5A7A4A".into(),
        yellow: "#9A9540".into(),
        red: "#8B4A5A".into(),
        header_border: "rgba(var(--accent-rgb), 0.25)".into(),
    }
}

pub fn render_css(theme: &Theme, texture: Texture, glow: Glow) -> String {
    let mut out = theme.root_block();
    out.push_str(STATIC_CSS);
    out.push_str(&decoration_css(theme, texture, glow));
    out
}

/// Site-wide decorations (texture + glow) painted on `body::before` and
/// `body::after`. Each layer sits at `z-index: -1` so it covers body's
/// background-color but stays behind all in-flow content. Both layers are
/// stripped under `@media print` to keep PDFs/exports clean.
fn decoration_css(theme: &Theme, texture: Texture, glow: Glow) -> String {
    if matches!(texture, Texture::None) && matches!(glow, Glow::None) {
        return String::new();
    }
    let text_rgb = hex_to_rgb_triple(&theme.text).unwrap_or_else(|| "255, 255, 255".into());
    let accent_rgb = hex_to_rgb_triple(&theme.accent).unwrap_or_else(|| "60, 206, 206".into());

    let mut out = String::new();
    out.push_str(&texture_css(texture, &text_rgb));
    out.push_str(&glow_css(glow, &accent_rgb));
    if !out.is_empty() {
        out.push_str("@media print { body::before, body::after { display: none !important; } }\n");
    }
    out
}

fn texture_css(texture: Texture, text_rgb: &str) -> String {
    let layer = "content: ''; position: fixed; inset: 0; pointer-events: none; z-index: -1;";
    match texture {
        Texture::None => String::new(),
        Texture::Dots => format!(
            "body::before {{ {layer} \
             background-image: radial-gradient(rgba({rgb}, 0.07) 1px, transparent 1px); \
             background-size: 24px 24px; }}\n",
            layer = layer,
            rgb = text_rgb,
        ),
        Texture::Grid => format!(
            "body::before {{ {layer} \
             background-image: \
               linear-gradient(rgba({rgb}, 0.04) 1px, transparent 1px), \
               linear-gradient(90deg, rgba({rgb}, 0.04) 1px, transparent 1px); \
             background-size: 44px 44px; }}\n",
            layer = layer,
            rgb = text_rgb,
        ),
        Texture::Diagonal => format!(
            "body::before {{ {layer} \
             background-image: repeating-linear-gradient(45deg, \
               rgba({rgb}, 0.04) 0 1px, transparent 1px 14px); }}\n",
            layer = layer,
            rgb = text_rgb,
        ),
        Texture::Grain => {
            let svg = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='220' height='220'>\
<filter id='n'>\
<feTurbulence type='fractalNoise' baseFrequency='0.85' numOctaves='2' stitchTiles='stitch'/>\
<feColorMatrix values='0 0 0 0 {r} 0 0 0 0 {g} 0 0 0 0 {b} 0 0 0 0.55 0'/>\
</filter>\
<rect width='100%' height='100%' filter='url(#n)'/></svg>",
                r = rgb_component_to_unit(text_rgb, 0),
                g = rgb_component_to_unit(text_rgb, 1),
                b = rgb_component_to_unit(text_rgb, 2),
            );
            format!(
                "body::before {{ {layer} opacity: 0.18; \
                 background-image: url(\"data:image/svg+xml;utf8,{enc}\"); }}\n",
                layer = layer,
                enc = url_encode_svg(&svg),
            )
        }
        Texture::Topography => {
            // Two stacked wavy contours with offsets — gives a calm topo feel.
            let svg = format!(
                "<svg xmlns='http://www.w3.org/2000/svg' width='240' height='160' viewBox='0 0 240 160'>\
<g fill='none' stroke='rgb({rgb})' stroke-opacity='0.07' stroke-width='1'>\
<path d='M -10 30 Q 60 10 120 30 T 250 30'/>\
<path d='M -10 60 Q 60 40 120 60 T 250 60'/>\
<path d='M -10 90 Q 60 70 120 90 T 250 90'/>\
<path d='M -10 120 Q 60 100 120 120 T 250 120'/>\
<path d='M -10 150 Q 60 130 120 150 T 250 150'/>\
</g></svg>",
                rgb = text_rgb,
            );
            format!(
                "body::before {{ {layer} \
                 background-image: url(\"data:image/svg+xml;utf8,{enc}\"); }}\n",
                layer = layer,
                enc = url_encode_svg(&svg),
            )
        }
    }
}

fn glow_css(glow: Glow, accent_rgb: &str) -> String {
    let layer = "content: ''; position: fixed; pointer-events: none; z-index: -1;";
    match glow {
        Glow::None => String::new(),
        Glow::Accent => format!(
            "body::after {{ {layer} \
             top: -320px; left: 50%; width: 1200px; height: 720px; \
             margin-left: -600px; \
             background: radial-gradient(ellipse at center, \
               rgba({rgb}, 0.10) 0%, transparent 65%); }}\n",
            layer = layer,
            rgb = accent_rgb,
        ),
        Glow::Corner => format!(
            "body::after {{ {layer} \
             top: -220px; right: -220px; width: 720px; height: 620px; \
             background: radial-gradient(circle at top right, \
               rgba({rgb}, 0.14) 0%, transparent 60%); }}\n",
            layer = layer,
            rgb = accent_rgb,
        ),
    }
}

fn url_encode_svg(svg: &str) -> String {
    svg.replace('%', "%25")
        .replace('#', "%23")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('"', "%22")
        .replace('\'', "%27")
        .replace(' ', "%20")
        .replace('\n', "%0A")
}

/// Pull one channel out of a `"r, g, b"` triple and return it as a 0..1 float
/// for use in an SVG `feColorMatrix` row.
fn rgb_component_to_unit(triple: &str, idx: usize) -> String {
    let val = triple
        .split(',')
        .nth(idx)
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(255);
    let unit = (val.min(255) as f32) / 255.0;
    format!("{:.4}", unit)
}

const STATIC_CSS: &str = r#"

* { margin: 0; padding: 0; box-sizing: border-box; }

body {
  background-color: var(--bg);
  color: var(--snow);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  line-height: 1.6;
}
body.shell-standard, body.shell-document { min-height: 100vh; }
body.shell-deck { height: 100vh; overflow: hidden; }

a { color: inherit; text-decoration: none; }
h1, h2, h3 { font-weight: 600; color: var(--snow); }

.container { max-width: 1200px; margin: 0 auto; padding: 0 40px; }

/* ──────────────────── Shared site bar ──────────────────── */

.site-bar {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 0 48px;
  height: 56px;
  flex-shrink: 0;
  border-bottom: 1.5px solid var(--teal);
  background: var(--bg);
  position: relative; /* Anchors the mobile nav panel below the bar. */
}
.site-bar-name {
  font-size: 14px; font-weight: 500;
  opacity: 0.6;
  transition: opacity 0.15s, color 0.15s;
}
a.site-bar-name:hover { opacity: 1; color: var(--teal); }

/* Logo variant of the brand slot: hard ceiling on rendered height so a
   tall image can't push the 56px bar taller. Width flows from aspect
   ratio, capped at 240px so a billboard SVG doesn't crush the nav. */
.site-bar-brand {
  display: inline-flex;
  align-items: center;
  opacity: 0.9;
  transition: opacity 0.15s;
  line-height: 0; /* Strip the text-line baseline so img sits flush. */
}
a.site-bar-brand:hover { opacity: 1; }
.site-bar-logo {
  display: block;
  max-height: 32px;
  max-width: 240px;
  width: auto;
  height: auto;
  object-fit: contain;
}
.site-bar-divider { color: rgba(var(--text-rgb),0.15); font-size: 20px; font-weight: 300; }
.site-bar-eyebrow {
  font-size: 11px;
  font-weight: 700;
  color: var(--teal);
  text-transform: uppercase;
  letter-spacing: 1.5px;
}
.site-bar-right { display: flex; align-items: center; gap: 16px; margin-left: auto; }
.site-bar-subtitle { font-size: 12px; color: var(--muted); }
.site-bar-print-btn {
  font-size: 12px; font-weight: 500;
  color: var(--teal);
  border: 1px solid rgba(var(--accent-rgb), 0.3);
  border-radius: 6px;
  background: none;
  padding: 5px 12px;
  cursor: pointer;
  transition: all 0.15s;
}
.site-bar-print-btn:hover { background: rgba(var(--accent-rgb), 0.08); border-color: rgba(var(--accent-rgb), 0.6); }

.site-search-btn {
  display: flex; align-items: center; justify-content: center;
  background: none; border: 1px solid rgba(var(--text-rgb), 0.15);
  border-radius: 6px; padding: 5px 8px; cursor: pointer;
  color: rgba(var(--text-rgb), 0.5); transition: all 0.15s;
}
.site-search-btn:hover { color: var(--snow); border-color: rgba(var(--text-rgb), 0.3); background: rgba(var(--text-rgb), 0.06); }
.site-search-overlay {
  position: fixed; inset: 0; z-index: 9999;
  display: flex; align-items: flex-start; justify-content: center;
  padding-top: min(20vh, 120px);
}
.site-search-overlay[hidden] { display: none; }
.site-search-backdrop { position: absolute; inset: 0; background: rgba(0,0,0,0.5); }
.site-search-dialog {
  position: relative; width: 90%; max-width: 560px;
  background: var(--bg); border: 1px solid rgba(var(--text-rgb), 0.15);
  border-radius: 12px; box-shadow: 0 16px 40px rgba(0,0,0,0.3);
  overflow: hidden;
}
.site-search-input-wrap {
  display: flex; align-items: center; gap: 10px;
  padding: 14px 16px; border-bottom: 1px solid rgba(var(--text-rgb), 0.1);
}
.site-search-icon { flex-shrink: 0; color: rgba(var(--text-rgb), 0.4); }
.site-search-input {
  flex: 1; background: none; border: none; outline: none;
  font: inherit; font-size: 15px; color: var(--snow);
}
.site-search-input::placeholder { color: rgba(var(--text-rgb), 0.35); }
.site-search-kbd {
  font-size: 11px; font-family: inherit; color: rgba(var(--text-rgb), 0.35);
  border: 1px solid rgba(var(--text-rgb), 0.15); border-radius: 4px;
  padding: 2px 6px; flex-shrink: 0;
}
.site-search-results { max-height: 360px; overflow-y: auto; padding: 6px 0; }
.site-search-hit {
  display: flex; flex-direction: column; gap: 2px;
  padding: 10px 16px; text-decoration: none; color: var(--snow);
  transition: background 0.1s;
}
.site-search-hit:hover, .site-search-hit-active { background: rgba(var(--accent-rgb), 0.1); }
.site-search-hit-title { font-size: 14px; font-weight: 500; }
.site-search-hit-desc { font-size: 12px; color: rgba(var(--text-rgb), 0.5); line-height: 1.4; }
.site-search-empty { padding: 24px 16px; text-align: center; font-size: 13px; color: rgba(var(--text-rgb), 0.4); }

.site-bar nav { display: flex; align-items: center; gap: 4px; }
.site-bar .site-nav-links { display: flex; align-items: center; gap: 4px; }
.site-bar .nav-menu-toggle {
  display: none; /* desktop: hidden. Mobile rule below shows it. */
  font: inherit;
  background: none;
  border: 1px solid transparent;
  border-radius: 6px;
  padding: 6px;
  cursor: pointer;
  color: rgba(var(--text-rgb), 0.7);
  width: 36px; height: 36px;
  align-items: center; justify-content: center;
  transition: all 0.15s;
}
.site-bar .nav-menu-toggle:hover {
  color: var(--snow);
  background: rgba(var(--text-rgb), 0.08);
}
.site-bar .nav-menu-icon {
  display: inline-block;
  width: 18px; height: 2px;
  background: currentColor;
  position: relative;
  transition: background 0.15s;
}
.site-bar .nav-menu-icon::before,
.site-bar .nav-menu-icon::after {
  content: '';
  position: absolute;
  left: 0; right: 0;
  height: 2px;
  background: currentColor;
  transition: transform 0.2s;
}
.site-bar .nav-menu-icon::before { top: -6px; }
.site-bar .nav-menu-icon::after  { top:  6px; }
/* Open state: collapse the three bars into an × */
.site-bar nav[data-open] .nav-menu-icon { background: transparent; }
.site-bar nav[data-open] .nav-menu-icon::before { transform: translateY(6px) rotate(45deg); }
.site-bar nav[data-open] .nav-menu-icon::after  { transform: translateY(-6px) rotate(-45deg); }
.site-bar .nav-link {
  font-size: 13px; font-weight: 500;
  padding: 6px 12px;
  border-radius: 6px;
  color: rgba(var(--text-rgb), 0.7);
  transition: all 0.15s;
}
.site-bar .nav-link:hover { color: var(--snow); background: rgba(var(--text-rgb), 0.08); }
.site-bar .nav-link-active { color: var(--teal) !important; background: rgba(var(--accent-rgb), 0.08) !important; }

body.shell-standard .site-bar, body.shell-document .site-bar {
  position: sticky;
  top: 0;
  z-index: 10;
  background: rgba(var(--bg-rgb), 0.92);
  backdrop-filter: blur(12px);
}

/* Deep-link scroll offset. Any element with an id (section wrappers,
   header wrappers, and inline markdown heading anchors) gets a
   scroll-margin-top that clears the sticky 56px site bar plus some
   breathing room, so `/guide.html#outcomes` doesn't land with the
   heading tucked under the bar. Deck shell has no sticky bar, so the
   rule is scoped to the shells that do. */
body.shell-standard [id],
body.shell-document [id] {
  scroll-margin-top: 72px;
}

/* ──────── Nav dropdowns (parents with children, top layout) ──────── */

.site-bar nav .nav-link-group { position: relative; display: flex; align-items: center; }
.site-bar nav .nav-link-parent {
  display: inline-flex; align-items: center; gap: 4px;
  font: inherit;
  font-size: 13px; font-weight: 500;
  padding: 6px 12px;
  border-radius: 6px;
  color: rgba(var(--text-rgb), 0.7);
  background: none; border: none;
  cursor: pointer;
  transition: all 0.15s;
}
.site-bar nav .nav-link-parent:hover { color: var(--snow); background: rgba(var(--text-rgb), 0.08); }
.site-bar nav .nav-chevron { font-size: 9px; opacity: 0.6; margin-top: 1px; }
.site-bar nav .nav-dropdown {
  position: absolute;
  /* Touch the bottom of the button — no hover gap between trigger and
     panel, otherwise the pointer leaves the :hover region while moving
     toward the menu and the dropdown snaps shut. */
  top: 100%;
  right: 0;
  min-width: 180px;
  background: var(--bg);
  border: 1px solid var(--card-border);
  border-radius: 10px;
  /* The 6px top padding gives visual breathing room without a dead hover
     zone — the whole panel edge-to-edge is still a hover target. */
  padding: 6px 4px 4px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.25);
  opacity: 0;
  pointer-events: none;
  transform: translateY(-4px);
  transition: opacity 0.15s, transform 0.15s;
  z-index: 100;
  display: flex; flex-direction: column; gap: 2px;
}
/* Safety bridge: a transparent strip above the dropdown that keeps the
   cursor inside the parent's :hover region while traversing. */
.site-bar nav .nav-dropdown::before {
  content: '';
  position: absolute;
  top: -8px;
  left: 0;
  right: 0;
  height: 8px;
}
.site-bar nav .nav-link-group:hover .nav-dropdown,
.site-bar nav .nav-link-group:focus-within .nav-dropdown {
  opacity: 1;
  pointer-events: auto;
  transform: translateY(0);
}
.site-bar nav .nav-dropdown .nav-link {
  display: block;
  padding: 7px 12px;
  font-size: 13px;
  white-space: nowrap;
}

/* ──────── Sidebar nav layout ──────── */

body.nav-layout-sidebar .site-sidebar {
  position: fixed;
  top: 56px;
  bottom: 0;
  left: 0;
  width: 240px;
  overflow-y: auto;
  padding: 24px 12px;
  border-right: 1px solid var(--card-border);
  background: var(--bg);
  z-index: 5;
}
body.nav-layout-sidebar .site-sidebar nav {
  display: flex; flex-direction: column; gap: 2px;
}
body.nav-layout-sidebar .sidebar-section { margin-top: 20px; }
body.nav-layout-sidebar .sidebar-section:first-child { margin-top: 0; }
body.nav-layout-sidebar .sidebar-section-label {
  font-size: 11px; font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 1px;
  color: var(--muted);
  padding: 0 12px;
  margin-bottom: 6px;
}
body.nav-layout-sidebar .sidebar-subsection { margin-top: 12px; margin-bottom: 2px; }
body.nav-layout-sidebar .sidebar-subsection-label {
  font-size: 13px; font-weight: 500;
  color: rgba(var(--text-rgb), 0.7);
  padding: 6px 12px 6px 2px;
  cursor: pointer;
  user-select: none;
  border-radius: 6px;
  transition: color 0.15s, background 0.15s;
  display: flex;
  align-items: center;
}
body.nav-layout-sidebar .sidebar-subsection-label:hover {
  color: var(--snow);
  background: rgba(var(--text-rgb), 0.08);
}
body.nav-layout-sidebar .sidebar-subsection-label svg {
  width: 12px; height: 12px;
  flex-shrink: 0;
  transition: transform 0.15s;
  opacity: 0.5;
  margin-left: 4px;
  margin-right: 2px;
}
body.nav-layout-sidebar .sidebar-subsection[data-collapsed] .sidebar-subsection-label svg {
  transform: rotate(-90deg);
}
body.nav-layout-sidebar .sidebar-subsection[data-collapsed] > .sidebar-link-nested,
body.nav-layout-sidebar .sidebar-subsection[data-collapsed] > a {
  display: none;
}
body.nav-layout-sidebar .sidebar-link-nested {
  padding-left: 28px;
  font-size: 12.5px;
}
body.nav-layout-sidebar .sidebar-link {
  display: block;
  padding: 7px 12px;
  border-radius: 6px;
  font-size: 13px;
  color: rgba(var(--text-rgb), 0.7);
  transition: color 0.15s, background 0.15s;
}
body.nav-layout-sidebar .sidebar-link-top { font-weight: 500; }
body.nav-layout-sidebar .sidebar-link:hover {
  color: var(--snow);
  background: rgba(var(--text-rgb), 0.08);
}
body.nav-layout-sidebar .sidebar-link.nav-link-active {
  color: var(--teal);
  background: rgba(var(--accent-rgb), 0.08);
}
body.nav-layout-sidebar .main-content {
  margin-left: 240px;
}
body.nav-layout-sidebar .container {
  max-width: none;
  padding-left: 48px;
  padding-right: 48px;
}
@media (max-width: 768px) {
  body.nav-layout-sidebar .site-sidebar {
    position: static;
    width: 100%;
    height: auto;
    border-right: none;
    border-bottom: 1px solid var(--card-border);
    padding: 16px 24px;
  }
  body.nav-layout-sidebar .main-content { margin-left: 0; }
}

/* ──────────────────── Standard shell ──────────────────── */

body.shell-standard .main-content { padding-top: 60px; padding-bottom: 100px; }

/* ──────────────────── Document shell ──────────────────── */

body.shell-document .doc-root {
  width: 100%;
  max-width: 720px;
  margin: 0 auto;
  padding: 40px 20px 100px;
}
body.shell-document .doc-card {
  background: rgba(var(--text-rgb), 0.05);
  border: 1px solid rgba(var(--text-rgb), 0.09);
  border-radius: 16px;
  padding: 40px 48px;
  box-shadow: 0 4px 40px rgba(0,0,0,0.3);
}
body.shell-document .doc-body { line-height: 1.7; color: rgba(var(--text-rgb),0.9); font-size: 15px; }
body.shell-document .doc-footer {
  margin-top: 40px;
  padding-top: 20px;
  border-top: 1px solid rgba(var(--text-rgb),0.08);
}

/* ──────────────────── Deck shell ──────────────────── */

body.shell-deck .deck-root {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
}

body.shell-deck .deck-viewport { flex: 1; overflow: hidden; position: relative; }
body.shell-deck .deck-track {
  display: flex;
  height: 100%;
  transition: transform 0.4s cubic-bezier(0.4, 0, 0.2, 1);
}
body.shell-deck .deck-slide { min-width: 100%; height: 100%; overflow: hidden; }
body.shell-deck .deck-inner {
  max-width: 1100px;
  margin: 0 auto;
  padding: 56px 56px 88px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  min-height: 100%;
  gap: 28px;
}
body.shell-deck .deck-slide-cover .deck-inner {
  align-items: center;
  text-align: center;
}

/* Deck-scale typography: every content primitive steps up one tier so a
   slide reads as a slide, not a doc page. */
body.shell-deck .c-header-title { font-size: 36px; line-height: 1.2; }
body.shell-deck .c-header-subtitle { font-size: 18px; }
body.shell-deck .c-stat-value { font-size: 40px; }
body.shell-deck .c-stat-label { font-size: 12px; letter-spacing: 2px; }
body.shell-deck .c-card-title { font-size: 20px; }
body.shell-deck .c-card-description { font-size: 15px; line-height: 1.55; }
body.shell-deck .c-callout-title { font-size: 18px; }
body.shell-deck .c-callout-body { font-size: 17px; line-height: 1.55; }
body.shell-deck .c-markdown p { font-size: 17px; line-height: 1.65; }

/* Cover slide: oversized headline + softer subtitle for the first impression. */
body.shell-deck .deck-slide-cover .c-header-title { font-size: 56px; letter-spacing: -0.01em; }
body.shell-deck .deck-slide-cover .c-header-subtitle { font-size: 20px; color: var(--light-muted); }
body.shell-deck .deck-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--teal);
  text-transform: uppercase;
  letter-spacing: 2px;
  margin-bottom: 12px;
}
body.shell-deck .deck-nav {
  height: 52px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 48px;
  border-top: 1px solid rgba(var(--text-rgb),0.08);
}
body.shell-deck .deck-nav-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 1.5px;
  color: rgba(var(--text-rgb),0.25);
}
body.shell-deck .deck-arrow {
  font-size: 12px; font-weight: 500;
  color: rgba(var(--text-rgb),0.4);
  background: none; border: none;
  cursor: pointer;
  padding: 4px 0;
  transition: color 0.15s;
  min-width: 200px;
}
body.shell-deck .deck-prev { text-align: left; }
body.shell-deck .deck-next { text-align: right; }
body.shell-deck .deck-arrow:hover { color: var(--teal); }

/* ──────────────────── Components ──────────────────── */

/* — stack spacing for components in main flow — */
.main-content > *, .deck-inner > *, .doc-body > *, .c-section > *:not(.c-section-header), .tab-panel > * {
  margin-bottom: 32px;
}
.main-content > *:last-child, .deck-inner > *:last-child, .doc-body > *:last-child { margin-bottom: 0; }

/* Header component */
.c-header { }
.c-header.align-center { text-align: center; }
.c-header.align-right { text-align: right; }
.c-header-eyebrow {
  font-size: 11px;
  font-weight: 600;
  color: var(--teal);
  margin-bottom: 6px;
  text-transform: uppercase;
  letter-spacing: 1px;
}
.c-header-title { font-size: 24px; font-weight: 700; margin-bottom: 8px; }
.c-header-subtitle { font-size: 14px; color: var(--light-muted); }

/* Hero banner */
.c-hero {
  text-align: center;
  padding: 64px 0 48px;
}
.c-hero-eyebrow {
  font-size: 12px;
  font-weight: 600;
  color: var(--teal);
  margin-bottom: 12px;
  text-transform: uppercase;
  letter-spacing: 1.5px;
}
.c-hero-title {
  font-size: 44px;
  font-weight: 700;
  line-height: 1.15;
  letter-spacing: -0.02em;
  margin: 0 auto 16px;
  max-width: 720px;
}
.c-hero-subtitle {
  font-size: 17px;
  line-height: 1.6;
  color: var(--light-muted);
  max-width: 600px;
  margin: 0 auto;
}
.c-hero-buttons {
  margin-top: 32px;
  display: flex;
  gap: 12px;
  justify-content: center;
  flex-wrap: wrap;
}

/* Meta */
.c-meta {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 1px;
  background: var(--card-border);
  border: 1px solid var(--card-border);
  border-radius: 10px;
  overflow: hidden;
}
.c-meta-item { background: var(--card-bg); padding: 16px 20px; display: flex; flex-direction: column; gap: 4px; }
.c-meta-key {
  font-size: 11px; font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--muted);
}
.c-meta-value { font-size: 15px; font-weight: 500; color: var(--snow); }

/* Role Map */
.c-role-map { margin: 2rem 0; }
.c-role-map-title { font-size: 1.5rem; margin-bottom: 1rem; color: var(--text); }
.c-role-map-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 1rem;
}
.c-role-map-card {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 1.5rem;
  border-radius: 12px;
  border: 1px solid rgba(var(--text-rgb), 0.1);
  background: rgba(var(--text-rgb), 0.03);
  text-decoration: none;
  color: var(--text);
  transition: border-color 0.2s, background 0.2s, transform 0.15s;
}
.c-role-map-card:hover {
  border-color: var(--accent);
  background: rgba(var(--accent-rgb), 0.08);
  transform: translateY(-2px);
}
.c-role-map-icon { font-size: 1.5rem; }
.c-role-map-label { font-size: 1.125rem; font-weight: 600; }
.c-role-map-desc { font-size: 0.875rem; color: var(--text-muted); line-height: 1.4; }
.c-role-map-empty { color: var(--text-muted); font-style: italic; }

/* Card Grid */
.c-card-grid { display: grid; gap: 20px; }
.c-card-grid-arrow { display: flex; flex-direction: row; align-items: stretch; gap: 16px; grid-template-columns: unset !important; }
.c-card-grid-arrow .c-card { flex: 1 1 0; min-width: 0; }
.c-card-arrow {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--light-muted);
  font-size: 24px;
  font-weight: 300;
  user-select: none;
  padding: 0 4px;
}
.c-card {
  background: var(--card-bg);
  border: 1px solid var(--card-border);
  border-radius: 12px;
  padding: 24px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  transition: border-color 0.2s;
}
a.c-card { color: inherit; }
.c-card:hover { border-color: var(--card-hover-border); }
.c-card-teal { border-color: rgba(var(--accent-rgb), 0.35); }
.c-card-teal:hover { border-color: rgba(var(--accent-rgb), 0.6); }
.c-card-green { border-color: rgba(52, 211, 153, 0.35); }
.c-card-green:hover { border-color: rgba(52, 211, 153, 0.6); }
.c-card-yellow { border-color: rgba(251, 191, 36, 0.35); }
.c-card-yellow:hover { border-color: rgba(251, 191, 36, 0.6); }
.c-card-red { border-color: rgba(248, 113, 113, 0.4); }
.c-card-red:hover { border-color: rgba(248, 113, 113, 0.65); }
.c-card-top { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
.c-card-title { font-size: 18px; font-weight: 600; }
.c-card-desc { font-size: 14px; color: var(--light-muted); line-height: 1.5; }
.c-card-links { display: flex; gap: 8px; flex-wrap: wrap; margin-top: auto; }
.c-card-link {
  display: inline-flex; align-items: center;
  padding: 6px 12px;
  border-radius: 6px;
  font-size: 12px; font-weight: 500;
  color: var(--teal);
  border: 1px solid rgba(var(--accent-rgb), 0.2);
  background: rgba(var(--accent-rgb), 0.05);
  transition: background 0.15s;
}
.c-card-link:hover { background: rgba(var(--accent-rgb), 0.1); }

/* Badges */
.c-badge {
  font-size: 11px; font-weight: 600;
  padding: 4px 10px;
  border-radius: 100px;
  white-space: nowrap;
  background: rgba(var(--text-rgb), 0.09);
  opacity: 0.8;
}
.c-badge-default { background: rgba(var(--text-rgb), 0.09); color: var(--snow); opacity: 0.8; }
.c-badge-green { background: rgba(52, 211, 153, 0.12); color: var(--green); opacity: 1; }
.c-badge-yellow { background: rgba(251, 191, 36, 0.12); color: var(--yellow); opacity: 1; }
.c-badge-red { background: rgba(248, 113, 113, 0.12); color: var(--red); opacity: 1; }
.c-badge-teal { background: rgba(var(--accent-rgb), 0.12); color: var(--teal); opacity: 1; }

/* Selectable Grid */
.c-selectable-grid { position: relative; }
.c-sel-dots-row {
  position: relative;
  display: flex;
  justify-content: space-around;
  align-items: center;
  margin-bottom: 32px;
  padding: 20px 0;
}
.c-sel-dots-line {
  position: absolute;
  top: 50%;
  left: calc(100% / 8);
  right: calc(100% / 8);
  height: 1px;
  background: rgba(var(--accent-rgb), 0.25);
  transform: translateY(-50%);
  pointer-events: none;
}
.sel-dot {
  width: 40px; height: 40px;
  border-radius: 50%;
  border: 2px solid rgba(var(--text-rgb), 0.12);
  background: var(--bg);
  color: rgba(var(--text-rgb), 0.4);
  font-size: 13px; font-weight: 700;
  cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  transition: all 0.2s;
  position: relative;
  z-index: 1;
}
.sel-dot:hover { border-color: rgba(var(--accent-rgb), 0.5); color: var(--teal); }
.sel-dot.sel-active {
  border-color: var(--teal);
  background: rgba(14, 42, 42, 1);
  color: var(--teal);
}
.c-sel-cards { display: grid; gap: 16px; }
.c-sel-cards-arrow { display: flex; flex-direction: row; align-items: stretch; gap: 16px; }
.c-sel-cards-arrow .sel-card { flex: 1 1 0; min-width: 0; }
.sel-card {
  text-align: left;
  background: rgba(var(--text-rgb), 0.05);
  border: 1px solid rgba(var(--text-rgb), 0.10);
  border-radius: 12px;
  padding: 24px 20px;
  cursor: pointer;
  transition: all 0.2s;
  display: flex; flex-direction: column; gap: 16px;
  font: inherit; color: inherit;
}
.sel-card:hover { border-color: rgba(var(--text-rgb), 0.14); }
.sel-card.sel-active {
  background: rgba(var(--accent-rgb), 0.06);
  border-color: rgba(var(--accent-rgb), 0.35);
}
.sel-card.sel-dimmed { opacity: 0.35; }
.sel-card-teal { border-color: rgba(var(--accent-rgb), 0.35); }
.sel-card-green { border-color: rgba(52, 211, 153, 0.35); }
.sel-card-yellow { border-color: rgba(251, 191, 36, 0.35); }
.sel-card-red { border-color: rgba(248, 113, 113, 0.4); }
.c-sel-eyebrow {
  font-size: 11px; font-weight: 600;
  color: var(--teal);
  opacity: 0.7;
  letter-spacing: 0.5px;
  text-transform: uppercase;
}
.c-sel-title { font-size: 15px; font-weight: 600; color: var(--snow); line-height: 1.3; }
.c-sel-bullets { list-style: none; display: flex; flex-direction: column; gap: 10px; }
.c-sel-bullets li { display: flex; gap: 10px; align-items: flex-start; }
.c-sel-bullet-dot {
  width: 5px; height: 5px;
  border-radius: 50%;
  background: var(--teal);
  opacity: 0.5;
  flex-shrink: 0;
  margin-top: 7px;
}
.c-sel-bullets span:last-child { font-size: 13px; color: rgba(var(--text-rgb), 0.65); line-height: 1.5; }
.c-sel-body { font-size: 13px; color: rgba(var(--text-rgb), 0.65); line-height: 1.5; }

/* Timeline */
.c-timeline { display: flex; }
.c-timeline-phase { flex: 1; text-align: center; padding: 12px 6px 0; }
.c-timeline-dot {
  width: 8px; height: 8px;
  border-radius: 50%;
  margin: 0 auto 8px;
  background: rgba(var(--text-rgb),0.12);
}
.c-timeline-phase.completed .c-timeline-dot { background: var(--green); }
.c-timeline-phase.active .c-timeline-dot { background: var(--teal); box-shadow: 0 0 8px rgba(var(--accent-rgb),0.5); }
.c-timeline-label {
  font-size: 11px; font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: rgba(var(--text-rgb),0.4);
  margin-bottom: 10px;
}
.c-timeline-phase.completed .c-timeline-label { color: var(--green); }
.c-timeline-phase.active .c-timeline-label { color: var(--teal); }
.c-timeline-bar { height: 3px; background: rgba(var(--text-rgb),0.1); }
.c-timeline-bar.completed { background: var(--green); }
.c-timeline-bar.active { background: var(--teal); }

/* Stat Grid */
.c-stat-grid { display: grid; gap: 14px; }
.c-stat {
  background: rgba(var(--text-rgb),0.05);
  border: 1px solid rgba(var(--text-rgb),0.1);
  border-radius: 12px;
  padding: 20px 22px;
  position: relative;
  overflow: hidden;
  display: flex; flex-direction: column; gap: 6px;
}
.c-stat::before {
  content: '';
  position: absolute;
  top: 0; left: 0; right: 0;
  height: 3px;
  background: var(--stat-color);
}
.c-stat-label {
  font-size: 11px; font-weight: 600;
  color: rgba(var(--text-rgb),0.55);
  text-transform: uppercase;
  letter-spacing: 1px;
}
.c-stat-value { font-size: 28px; font-weight: 700; line-height: 1.1; }
.c-stat-detail { font-size: 13px; color: rgba(var(--text-rgb),0.6); line-height: 1.4; }

/* Before / After */
.c-before-after { display: flex; flex-direction: column; gap: 20px; }
.c-ba-card {
  padding: 32px 36px;
  background: rgba(var(--text-rgb),0.05);
  border: 1px solid rgba(var(--accent-rgb),0.08);
  border-radius: 14px;
  display: flex; flex-direction: column; gap: 12px;
}
.c-ba-title { font-size: 22px; font-weight: 700; }
.c-ba-before { font-size: 16px; color: rgba(var(--text-rgb),0.55); line-height: 1.5; }
.c-ba-after { font-size: 16px; color: rgba(var(--text-rgb),0.85); line-height: 1.5; }
.c-ba-highlight { color: var(--teal); font-weight: 600; }

/* Split Compare */
.c-split-compare { display: grid; grid-template-columns: 1fr 1fr; gap: 2px; border-radius: 12px; overflow: hidden; }
.c-sc-panel { padding: 32px; background: rgba(var(--text-rgb),0.04); }
.c-sc-left { border-right: 1px solid rgba(var(--text-rgb),0.08); }
.c-sc-eyebrow { font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: rgba(var(--text-rgb),0.4); margin-bottom: 8px; }
.c-sc-right .c-sc-eyebrow { color: var(--teal); }
.c-sc-title { font-size: 22px; font-weight: 700; line-height: 1.3; margin-bottom: 20px; }
.c-sc-stats { display: flex; flex-direction: column; gap: 0; }
.c-sc-stat { display: flex; justify-content: space-between; align-items: baseline; padding: 8px 0; border-bottom: 1px solid rgba(var(--text-rgb),0.06); }
.c-sc-stat:last-child { border-bottom: none; }
.c-sc-label { font-size: 14px; color: rgba(var(--text-rgb),0.55); }
.c-sc-value { font-size: 14px; font-weight: 600; text-align: right; }
.c-sc-value.color-default { color: rgba(var(--text-rgb),0.85); }
.c-sc-value.color-green { color: #34D399; }
.c-sc-value.color-yellow { color: #FBBF24; }
.c-sc-value.color-red { color: #F87171; }
.c-sc-value.color-teal { color: #3CCECE; }

/* Steps */
.c-steps { list-style: none; display: flex; flex-direction: column; gap: 12px; }
.c-step {
  display: flex;
  align-items: flex-start;
  gap: 16px;
  padding: 20px 24px;
  background: rgba(var(--text-rgb),0.05);
  border: 1px solid rgba(var(--accent-rgb),0.08);
  border-radius: 12px;
}
.c-step-num {
  width: 24px; height: 24px;
  border-radius: 6px;
  flex-shrink: 0;
  background: rgba(var(--accent-rgb),0.12);
  color: var(--teal);
  font-size: 11px; font-weight: 700;
  display: flex; align-items: center; justify-content: center;
  margin-top: 2px;
}
.c-step-bullet {
  width: 6px; height: 6px;
  border-radius: 50%;
  background: var(--teal);
  flex-shrink: 0;
  margin-top: 10px;
  opacity: 0.6;
}
.c-step-title { font-size: 17px; font-weight: 600; margin-bottom: 4px; }
.c-step-detail { font-size: 14px; color: rgba(var(--text-rgb),0.7); line-height: 1.5; }

/* Markdown */
.c-markdown {
  color: var(--light-muted);
  font-size: 15px;
  line-height: 1.75;
}
.c-markdown h1, .c-markdown h2, .c-markdown h3 { color: var(--snow); margin-top: 2em; margin-bottom: 0.75em; }
.c-markdown h1 { font-size: 24px; }
.c-markdown h2 { font-size: 18px; }
.c-markdown h3 { font-size: 16px; color: var(--teal); }
.c-markdown h1:first-child, .c-markdown h2:first-child, .c-markdown h3:first-child { margin-top: 0; }
.c-markdown p { margin-bottom: 1em; }
.c-markdown ul, .c-markdown ol { padding-left: 1.5em; margin-bottom: 1em; }
.c-markdown li { margin-bottom: 0.4em; }
.c-markdown strong { color: var(--snow); font-weight: 600; }
.c-markdown code {
  font-family: 'SF Mono', 'Monaco', monospace;
  font-size: 13px;
  background: rgba(var(--text-rgb), 0.09);
  padding: 2px 6px;
  border-radius: 4px;
  color: var(--teal);
}
.c-markdown pre {
  background: rgba(var(--text-rgb), 0.07);
  border: 1px solid var(--card-border);
  border-radius: 8px;
  padding: 20px;
  overflow-x: auto;
  margin-bottom: 1.5em;
}
.c-markdown pre code { background: none; padding: 0; color: var(--snow); }
.c-markdown table { width: 100%; border-collapse: collapse; margin-bottom: 1.5em; }
.c-markdown th {
  text-align: left;
  font-size: 11px; font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--muted);
  padding: 10px 16px;
  border-bottom: 1px solid var(--card-border);
}
.c-markdown td {
  padding: 12px 16px;
  border-bottom: 1px solid rgba(var(--text-rgb), 0.07);
  color: var(--light-muted);
}
.c-markdown tr:last-child td { border-bottom: none; }
.c-markdown img { max-width: 100%; height: auto; border-radius: 8px; display: block; margin: 1.5em 0; }
.c-markdown a { color: var(--teal); }
.c-markdown a:hover { text-decoration: underline; }
.c-markdown blockquote {
  border-left: 3px solid rgba(var(--accent-rgb), 0.3);
  padding-left: 20px;
  color: var(--muted);
  font-style: italic;
  margin-bottom: 1em;
}

/* Document body inherits markdown styling for h3 in teal etc. */
body.shell-document .doc-body h3 { color: var(--teal); }
body.shell-document .doc-body h1 { font-size: 24px; margin-bottom: 20px; }
body.shell-document .doc-body h2 { font-size: 18px; margin: 24px 0 12px; }
body.shell-document .doc-body h3 { font-size: 16px; margin: 20px 0 10px; }
body.shell-document .doc-body p { margin-bottom: 12px; }
body.shell-document .doc-body strong { color: #fff; }

/* Table */
.c-table-wrap { display: flex; flex-direction: column; gap: 12px; }
.c-table-filter {
  padding: 10px 14px;
  border: 1px solid var(--card-border);
  border-radius: 8px;
  background: rgba(var(--text-rgb),0.05);
  color: var(--snow);
  font-size: 14px;
  font-family: inherit;
  max-width: 320px;
}
.c-table-filter:focus { outline: none; border-color: var(--card-hover-border); }
.c-table {
  width: 100%;
  border-collapse: collapse;
  background: rgba(var(--text-rgb),0.05);
  border: 1px solid var(--card-border);
  border-radius: 10px;
  overflow: hidden;
}
.c-table th {
  text-align: left;
  font-size: 11px; font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--muted);
  padding: 14px 18px;
  border-bottom: 1px solid var(--card-border);
  background: rgba(var(--text-rgb),0.05);
  user-select: none;
}
.c-table th[data-sortable] { cursor: pointer; transition: color 0.15s; }
.c-table th[data-sortable]:hover { color: var(--snow); }
.c-table th[data-sortable]::after {
  content: ' ↕';
  opacity: 0.3;
  font-size: 10px;
}
.c-table th.sort-asc::after { content: ' ↑'; opacity: 1; color: var(--teal); }
.c-table th.sort-desc::after { content: ' ↓'; opacity: 1; color: var(--teal); }
.c-table td {
  padding: 14px 18px;
  border-bottom: 1px solid rgba(var(--text-rgb), 0.07);
  color: var(--light-muted);
  font-size: 14px;
}
.c-table tr:last-child td { border-bottom: none; }
.c-table .align-left { text-align: left; }
.c-table .align-right { text-align: right; }
.c-table .align-center { text-align: center; }

/* Callout */
.c-callout {
  padding: 16px 20px;
  border-radius: 0 10px 10px 0;
  border-left: 3px solid;
  background: rgba(var(--text-rgb), 0.08);
}
.c-callout-info { border-left-color: var(--teal); }
.c-callout-warn { border-left-color: var(--yellow); background: rgba(251, 191, 36, 0.04); }
.c-callout-success { border-left-color: var(--green); background: rgba(52, 211, 153, 0.04); }
.c-callout-danger { border-left-color: var(--red); background: rgba(248, 113, 113, 0.04); }
.c-callout-title { font-weight: 600; margin-bottom: 4px; color: var(--snow); }
.c-callout-info .c-callout-title { color: var(--teal); }
.c-callout-warn .c-callout-title { color: var(--yellow); }
.c-callout-success .c-callout-title { color: var(--green); }
.c-callout-danger .c-callout-title { color: var(--red); }
.c-callout-body { font-size: 14px; color: var(--light-muted); line-height: 1.6; }
.c-callout-body > *:last-child { margin-bottom: 0 !important; }
.c-callout-body p { margin-bottom: 0.5em; }
.c-callout-body a { color: var(--teal); text-decoration: underline; text-decoration-color: rgba(var(--accent-rgb), 0.3); text-underline-offset: 2px; }
.c-callout-body a:hover { text-decoration-color: var(--teal); }
.c-callout-links { margin-top: 12px; }

/* Freshness banner: the review-overdue / due-soon nudge that kazam
   injects at the top of a page when its freshness metadata is expired.
   Builds on `c-callout` for colors — yellow for "due soon", red for
   "overdue" — and adds a sources-of-truth list underneath. */
.c-freshness-banner { margin-bottom: 24px; }
.c-freshness-sources {
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px solid rgba(var(--text-rgb), 0.08);
  font-size: 13px;
}
.c-freshness-sources-label {
  color: var(--muted);
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 1px;
  font-size: 11px;
}
.c-freshness-sources ul {
  list-style: none;
  margin: 6px 0 0;
  padding: 0;
  display: flex;
  flex-wrap: wrap;
  gap: 8px 16px;
}
.c-freshness-sources li { margin: 0; }
.c-freshness-sources a {
  color: var(--teal);
  text-decoration: underline;
  text-decoration-color: rgba(var(--accent-rgb), 0.3);
  text-underline-offset: 2px;
}
.c-freshness-sources a:hover { text-decoration-color: var(--teal); }

/* Code */
.c-code {
  background: rgba(var(--text-rgb), 0.07);
  border: 1px solid var(--card-border);
  border-radius: 8px;
  padding: 20px;
  overflow-x: auto;
  font-family: 'SF Mono', 'Monaco', monospace;
  font-size: 13px;
  color: var(--snow);
  line-height: 1.6;
}

/* Tabs */
.c-tabs { }
.c-tab-buttons {
  display: flex;
  gap: 4px;
  border-bottom: 1px solid var(--card-border);
  margin-bottom: 24px;
}
.tab-btn {
  font: inherit;
  font-size: 13px; font-weight: 500;
  padding: 10px 16px;
  color: rgba(var(--text-rgb), 0.5);
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  cursor: pointer;
  margin-bottom: -1px;
  transition: all 0.15s;
}
.tab-btn:hover { color: var(--snow); }
.tab-btn.tab-btn-active { color: var(--teal); border-bottom-color: var(--teal); }
.tab-panel { }

/* Section */
.c-section { }
.c-section.align-center { text-align: center; display: flex; flex-direction: column; align-items: center; }
.c-section.align-center > * { width: 100%; max-width: 800px; }
.c-section.align-center .c-section-header { align-items: center; }
.c-section.align-center ul, .c-section.align-center ol, .c-section.align-center .c-code, .c-section.align-center pre, .c-section.align-center .c-card, .c-section.align-center .c-callout { text-align: left; }
.c-section.align-right { text-align: right; display: flex; flex-direction: column; align-items: flex-end; }
.c-section.align-right > * { width: 100%; max-width: 800px; }
.c-section.align-right .c-section-header { align-items: flex-end; }
.c-section-header { margin-bottom: 24px; }
.c-section-eyebrow {
  font-size: 11px;
  font-weight: 600;
  color: var(--teal);
  text-transform: uppercase;
  letter-spacing: 1px;
  margin-bottom: 6px;
}
.c-section-heading { font-size: 18px; font-weight: 600; }

/* Columns */
.c-columns { display: grid; gap: 24px; }
.c-column { display: flex; flex-direction: column; gap: 20px; }
.c-column > *:last-child { margin-bottom: 0; }
.c-columns-stretch .c-column > * { flex: 1; }

/* Accordion */
.c-accordion { display: flex; flex-direction: column; gap: 8px; }
.c-accordion-item {
  background: rgba(var(--text-rgb),0.05);
  border: 1px solid var(--card-border);
  border-radius: 10px;
  overflow: hidden;
}
.accordion-head {
  width: 100%;
  padding: 14px 20px;
  text-align: left;
  background: none;
  border: none;
  color: var(--snow);
  font: inherit;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  display: flex;
  justify-content: space-between;
  align-items: center;
  transition: color 0.15s;
}
.accordion-head:hover { color: var(--teal); }
.accordion-chevron {
  font-size: 18px;
  opacity: 0.5;
  transition: transform 0.2s;
}
.c-accordion-item.accordion-open .accordion-chevron { transform: rotate(90deg); color: var(--teal); opacity: 1; }
.accordion-body {
  padding: 0 20px 20px;
  border-top: 1px solid var(--card-border);
  padding-top: 16px;
}

/* Event Timeline */
.c-event-timeline { display: flex; flex-direction: column; gap: 12px; }
.c-event-filter-toggle {
  display: inline-flex;
  align-self: flex-start;
  gap: 4px;
  padding: 4px;
  background: rgba(var(--text-rgb),0.05);
  border: 1px solid var(--card-border);
  border-radius: 8px;
}
.c-event-filter-toggle button {
  appearance: none;
  background: none;
  border: none;
  color: rgba(var(--text-rgb),0.6);
  font: inherit;
  font-size: 12px;
  font-weight: 500;
  padding: 6px 12px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}
.c-event-filter-toggle button:hover { color: var(--snow); }
.c-event-filter-toggle button.active {
  background: rgba(var(--accent-rgb),0.15);
  color: var(--teal);
}
.c-event-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
}
.c-event {
  display: grid;
  grid-template-columns: 24px 1fr;
  gap: 12px;
  padding: 12px 0;
}
.c-event + .c-event { border-top: 1px solid rgba(var(--text-rgb),0.06); }
/* The dot sits 6px from the top of the rail; with 12px row padding the rail
   line needs to start at the dot center (~18px from rail top) and end at
   the dot bottom (~30px from rail top). */
.c-event:first-child .c-event-rail::before { top: 18px; }
.c-event:last-child .c-event-rail::before { bottom: calc(100% - 30px); }
/* Filter visibility — when filter=major, hide non-major events */
.c-event-timeline.filter-major .c-event[data-severity="minor"],
.c-event-timeline.filter-major .c-event[data-severity="info"] { display: none; }
.c-event-rail {
  position: relative;
  display: flex;
  justify-content: center;
}
.c-event-rail::before {
  content: "";
  position: absolute;
  top: 0;
  bottom: 0;
  left: 50%;
  width: 2px;
  background: rgba(var(--text-rgb),0.1);
  transform: translateX(-50%);
}
.c-event-dot {
  position: relative;
  z-index: 1;
  width: 12px;
  height: 12px;
  margin-top: 6px;
  border-radius: 50%;
  background: rgba(var(--text-rgb),0.3);
  border: 2px solid var(--card-bg);
}
.c-event.severity-major .c-event-dot {
  background: var(--teal);
  box-shadow: 0 0 6px rgba(var(--accent-rgb),0.5);
}
.c-event.severity-info .c-event-dot { background: var(--muted); }
.c-event-body {
  min-width: 0;
  padding-bottom: 4px;
}
.c-event-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: rgba(var(--text-rgb),0.6);
  margin-bottom: 4px;
}
.c-event-date { font-variant-numeric: tabular-nums; }
.c-event-severity {
  text-transform: uppercase;
  font-size: 10px;
  letter-spacing: 0.06em;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 4px;
  background: rgba(var(--text-rgb),0.08);
  color: rgba(var(--text-rgb),0.7);
}
.c-event.severity-major .c-event-severity {
  background: rgba(var(--accent-rgb),0.15);
  color: var(--teal);
}
.c-event.severity-info .c-event-severity {
  background: rgba(var(--text-rgb),0.08);
  color: var(--muted);
}
.c-event-source {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  background: rgba(var(--text-rgb),0.05);
  color: rgba(var(--text-rgb),0.55);
}
.c-event-link {
  margin-left: auto;
  color: rgba(var(--text-rgb),0.5);
  text-decoration: none;
  font-size: 14px;
  line-height: 1;
  transition: color 0.15s;
}
.c-event-link:hover { color: var(--teal); }
.c-event-title {
  font-size: 14px;
  font-weight: 500;
  color: var(--snow);
  line-height: 1.4;
}
.c-event-details > summary {
  cursor: pointer;
  list-style: none;
  display: flex;
  align-items: baseline;
  gap: 6px;
}
.c-event-details > summary::-webkit-details-marker { display: none; }
.c-event-details > summary::before {
  content: "›";
  display: inline-block;
  width: 10px;
  color: rgba(var(--text-rgb),0.4);
  transition: transform 0.15s;
}
.c-event-details[open] > summary::before {
  transform: rotate(90deg);
  color: var(--teal);
}
.c-event-summary {
  margin-top: 6px;
  margin-left: 16px;
  font-size: 13px;
  color: rgba(var(--text-rgb),0.75);
  line-height: 1.5;
}
.c-event-summary > :first-child { margin-top: 0; }
.c-event-summary > :last-child { margin-bottom: 0; }

/* Tree */
.c-tree { font-size: 14px; line-height: 1.5; }
.c-tree-root, .c-tree-children { list-style: none; }
.c-tree-root { margin: 0; padding: 4px 0; }
.c-tree-children {
  margin: 0 0 0 14px;
  padding: 0 0 0 14px;
  border-left: 1.5px solid rgba(var(--text-rgb),0.12);
}
.c-tree-node {
  position: relative;
  padding: 6px 0;
}
.c-tree-node.collapsed > .c-tree-children { display: none; }
.c-tree-chevron {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  flex-shrink: 0;
  font-size: 9px;
  color: rgba(var(--text-rgb),0.4);
  cursor: pointer;
  transition: transform 0.15s ease;
  transform: rotate(90deg);
  user-select: none;
}
.c-tree-node.collapsed > .c-tree-row > .c-tree-chevron { transform: rotate(0deg); }
.c-tree-chevron:hover { color: rgba(var(--text-rgb),0.8); }
.c-tree-children > .c-tree-node::before {
  content: "";
  position: absolute;
  top: 17px;
  left: -14px;
  width: 12px;
  height: 1.5px;
  background: rgba(var(--text-rgb),0.12);
}
.c-tree-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  flex-wrap: wrap;
}
.c-tree-glyph {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  font-size: 12px;
  border-radius: 4px;
  color: rgba(var(--text-rgb),0.5);
  background: rgba(var(--text-rgb),0.05);
}
.c-tree-node.status-completed > .c-tree-row > .c-tree-glyph {
  color: var(--green);
  background: rgba(126,217,87,0.12);
}
.c-tree-node.status-active > .c-tree-row > .c-tree-glyph {
  color: var(--teal);
  background: rgba(var(--accent-rgb),0.15);
  box-shadow: 0 0 6px rgba(var(--accent-rgb),0.4);
}
.c-tree-node.status-blocked > .c-tree-row > .c-tree-glyph {
  color: var(--red);
  background: rgba(255,107,107,0.12);
}
.c-tree-node.status-priority > .c-tree-row > .c-tree-glyph {
  color: var(--yellow);
  background: rgba(251,191,36,0.12);
}
.c-tree-node.status-upcoming > .c-tree-row > .c-tree-glyph {
  color: rgba(var(--text-rgb),0.4);
  background: transparent;
  border: 1px dashed rgba(var(--text-rgb),0.2);
}
.c-tree-label {
  color: var(--snow);
  font-weight: 500;
}
.c-tree-node.status-upcoming > .c-tree-row > .c-tree-label {
  color: rgba(var(--text-rgb),0.6);
  font-weight: 400;
}
.c-tree-node.status-completed > .c-tree-row > .c-tree-label {
  color: rgba(var(--text-rgb),0.75);
}
.c-tree-note {
  font-size: 12px;
  color: rgba(var(--text-rgb),0.6);
  font-style: italic;
}
.c-tree-node.status-blocked > .c-tree-row > .c-tree-note {
  color: var(--red);
  font-style: normal;
  font-weight: 500;
}
.c-tree-node.status-priority > .c-tree-row > .c-tree-note {
  color: var(--yellow);
  font-style: normal;
  font-weight: 500;
}
.c-tree-filter-toggle {
  display: inline-flex;
  align-self: flex-start;
  gap: 4px;
  padding: 4px;
  margin-bottom: 12px;
  background: rgba(var(--text-rgb),0.05);
  border: 1px solid var(--card-border);
  border-radius: 8px;
}
.c-tree-filter-toggle button {
  appearance: none;
  background: none;
  border: none;
  color: rgba(var(--text-rgb),0.6);
  font: inherit;
  font-size: 12px;
  font-weight: 500;
  padding: 6px 12px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}
.c-tree-filter-toggle button:hover { color: var(--snow); }
.c-tree-filter-toggle button.active {
  background: rgba(var(--accent-rgb),0.15);
  color: var(--teal);
}
/* filter-incomplete: hide every node whose status is `completed`.
   A `completed` branch correctly hides its descendants — they're "done",
   the user didn't ask for them. The non-completed siblings stay visible. */
.c-tree.filter-incomplete .c-tree-node.status-completed { display: none; }
/* filter-blocked: show only blocked nodes + their ancestor chain.
   Server-side renders `data-has-blocked-descendant` on each ancestor of a
   blocked node, so the path-to-root keeps full context while non-relevant
   branches collapse. */
.c-tree.filter-blocked .c-tree-node:not(.status-blocked):not([data-has-blocked-descendant="true"]) {
  display: none;
}
/* filter-priority: show only priority nodes + their ancestor chain. */
.c-tree.filter-priority .c-tree-node:not(.status-priority):not([data-has-priority-descendant="true"]) {
  display: none;
}

/* Venn */
.c-venn {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}
.c-venn-title {
  font-size: 14px;
  font-weight: 500;
  color: var(--snow);
}
.c-venn-svg {
  width: 100%;
  max-width: 560px;
  height: auto;
}
.c-venn-circle {
  fill-opacity: 0.18;
  stroke-width: 2;
}
.c-venn-circle-default { fill: rgba(var(--text-rgb),0.6); stroke: rgba(var(--text-rgb),0.7); }
.c-venn-circle-teal    { fill: var(--teal); stroke: var(--teal); }
.c-venn-circle-green   { fill: var(--green); stroke: var(--green); }
.c-venn-circle-yellow  { fill: var(--yellow); stroke: var(--yellow); }
.c-venn-circle-red     { fill: var(--red); stroke: var(--red); }
.c-venn-label {
  font-size: 13px;
  font-weight: 600;
  pointer-events: none;
}
.c-venn-label-default { fill: rgba(var(--text-rgb),0.85); }
.c-venn-label-teal    { fill: var(--teal); }
.c-venn-label-green   { fill: var(--green); }
.c-venn-label-yellow  { fill: var(--yellow); }
.c-venn-label-red     { fill: var(--red); }
.c-venn-overlap-label {
  font-size: 12px;
  font-weight: 500;
  fill: rgba(var(--text-rgb),0.85);
  pointer-events: none;
}
.c-venn-empty {
  font-size: 13px;
  color: rgba(var(--text-rgb),0.5);
  font-style: italic;
}

/* Image */
.c-image { margin: 1.5em 0; }
.c-image.align-center { margin-left: auto; margin-right: auto; align-self: center; }
.c-image.align-right { margin-left: auto; margin-right: 0; align-self: flex-end; }
.c-image img { width: 100%; height: auto; border-radius: 8px; display: block; }
.c-image figcaption {
  font-size: 13px;
  color: var(--muted);
  margin-top: 8px;
  text-align: center;
}
.c-embed {
  position: relative;
  width: 100%;
  border-radius: 8px;
  overflow: hidden;
}
.c-embed iframe {
  position: absolute;
  top: 0; left: 0;
  width: 100%; height: 100%;
  border: 0;
}

/* ──────────────────── Phase 1 additions ──────────────────── */

/* Tag (pill-shaped decorative) */
.c-tag {
  display: inline-flex; align-items: center;
  font-size: 12px; font-weight: 500;
  padding: 3px 10px;
  border-radius: 6px;
  background: rgba(var(--text-rgb), 0.07);
  border: 1px solid rgba(var(--text-rgb), 0.12);
  color: var(--light-muted);
  font-family: 'SF Mono', 'Monaco', monospace;
}
.c-tag-green { background: rgba(52, 211, 153, 0.08); border-color: rgba(52, 211, 153, 0.25); color: var(--green); }
.c-tag-yellow { background: rgba(251, 191, 36, 0.08); border-color: rgba(251, 191, 36, 0.25); color: var(--yellow); }
.c-tag-red { background: rgba(248, 113, 113, 0.08); border-color: rgba(248, 113, 113, 0.25); color: var(--red); }
.c-tag-teal { background: rgba(var(--accent-rgb), 0.08); border-color: rgba(var(--accent-rgb), 0.25); color: var(--teal); }

/* Divider */
.c-divider {
  border: none;
  border-top: 1px solid var(--card-border);
  width: 100%;
  margin: 32px 0;
}
.c-divider-labeled {
  display: flex; align-items: center; gap: 16px;
  margin: 32px 0;
}
.c-divider-labeled .c-divider-line {
  flex: 1;
  height: 1px;
  background: var(--card-border);
}
.c-divider-label {
  font-size: 12px; font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 1px;
  color: var(--muted);
}

/* Kbd */
.c-kbd-group { display: inline-flex; align-items: center; gap: 4px; }
.c-kbd {
  display: inline-block;
  font-family: 'SF Mono', 'Monaco', monospace;
  font-size: 11px; font-weight: 600;
  padding: 2px 6px;
  border-radius: 4px;
  background: rgba(var(--text-rgb), 0.12);
  border: 1px solid rgba(var(--text-rgb), 0.12);
  border-bottom-width: 2px;
  color: var(--snow);
  line-height: 1;
}
.c-kbd-sep { font-size: 11px; color: var(--muted); }

/* Status */
.c-status {
  display: inline-flex; align-items: center; gap: 8px;
  font-size: 13px; font-weight: 500;
}
.c-status-dot {
  width: 8px; height: 8px;
  border-radius: 50%;
  background: rgba(var(--text-rgb), 0.3);
}
.c-status-green .c-status-dot { background: var(--green); box-shadow: 0 0 0 3px rgba(52, 211, 153, 0.15); }
.c-status-yellow .c-status-dot { background: var(--yellow); box-shadow: 0 0 0 3px rgba(251, 191, 36, 0.15); }
.c-status-red .c-status-dot { background: var(--red); box-shadow: 0 0 0 3px rgba(248, 113, 113, 0.15); }
.c-status-teal .c-status-dot { background: var(--teal); box-shadow: 0 0 0 3px rgba(var(--accent-rgb), 0.15); }

/* Breadcrumb */
.c-breadcrumb ol {
  list-style: none;
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  font-size: 13px;
}
.c-breadcrumb-item a { color: var(--light-muted); transition: color 0.15s; }
.c-breadcrumb-item a:hover { color: var(--teal); }
.c-breadcrumb-item span[aria-current="page"] { color: var(--snow); font-weight: 500; }
.c-breadcrumb-sep { color: var(--muted); opacity: 0.5; }

/* Button Group */
.c-button-group {
  display: flex; gap: 10px; flex-wrap: wrap;
}
.c-button {
  display: inline-flex; align-items: center; gap: 8px;
  font-size: 13px; font-weight: 500;
  padding: 8px 16px;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.15s;
  text-decoration: none;
  line-height: 1.4;
}
.c-button-icon { display: inline-flex; align-items: center; }
.c-button-primary {
  background: var(--teal);
  color: var(--bg);
  border: 1px solid var(--teal);
}
.c-button-primary:hover { background: #5fdada; border-color: #5fdada; }
.c-button-secondary {
  background: transparent;
  color: var(--teal);
  border: 1px solid rgba(var(--accent-rgb), 0.3);
}
.c-button-secondary:hover { background: rgba(var(--accent-rgb), 0.08); border-color: rgba(var(--accent-rgb), 0.5); }
.c-button-ghost {
  background: transparent;
  color: var(--light-muted);
  border: 1px solid transparent;
}
.c-button-ghost:hover { color: var(--snow); background: rgba(var(--text-rgb), 0.07); }

/* Definition List */
.c-definition-list {
  display: flex; flex-direction: column;
  border: 1px solid var(--card-border);
  border-radius: 10px;
  overflow: hidden;
}
.c-dl-row {
  display: grid;
  grid-template-columns: 200px 1fr;
  gap: 20px;
  padding: 16px 20px;
  border-bottom: 1px solid var(--card-border);
  background: var(--card-bg);
}
.c-dl-row:last-child { border-bottom: none; }
.c-dl-term {
  font-weight: 600;
  color: var(--snow);
  font-size: 14px;
}
.c-dl-def {
  margin: 0;
  color: var(--light-muted);
  font-size: 14px;
  line-height: 1.6;
}
@media (max-width: 640px) {
  .c-dl-row { grid-template-columns: 1fr; gap: 4px; }
}

/* Blockquote */
.c-blockquote {
  margin: 0;
  padding: 24px 32px;
  background: rgba(var(--accent-rgb), 0.03);
  border-left: 3px solid var(--teal);
  border-radius: 0 10px 10px 0;
}
.c-blockquote blockquote {
  margin: 0;
  border: none;
  padding: 0;
}
.c-blockquote blockquote p {
  font-size: 18px;
  font-style: italic;
  color: var(--snow);
  line-height: 1.6;
  margin: 0;
}
.c-blockquote-attribution {
  margin-top: 12px;
  font-size: 13px;
  color: var(--light-muted);
  font-style: normal;
}

/* Avatar */
.c-avatar {
  display: inline-flex; align-items: center; justify-content: center;
  border-radius: 50%;
  background: rgba(var(--accent-rgb), 0.12);
  color: var(--teal);
  font-weight: 600;
  flex-shrink: 0;
  overflow: hidden;
  border: 1px solid rgba(var(--accent-rgb), 0.2);
}
.c-avatar img { width: 100%; height: 100%; object-fit: cover; display: block; }
.c-avatar-sm { width: 24px; height: 24px; font-size: 10px; }
.c-avatar-md { width: 40px; height: 40px; font-size: 13px; }
.c-avatar-lg { width: 56px; height: 56px; font-size: 18px; }
.c-avatar-xl { width: 80px; height: 80px; font-size: 24px; }
.c-avatar-row { display: inline-flex; align-items: center; gap: 12px; }
.c-avatar-meta { display: flex; flex-direction: column; gap: 2px; }
.c-avatar-name { font-size: 14px; font-weight: 600; color: var(--snow); line-height: 1.3; }
.c-avatar-sub { font-size: 12px; color: var(--muted); line-height: 1.3; }

/* Avatar Group */
.c-avatar-group { display: inline-flex; align-items: center; }
.c-avatar-group .c-avatar {
  margin-left: -8px;
  border: 2px solid var(--bg);
  box-sizing: content-box;
}
.c-avatar-group .c-avatar:first-child { margin-left: 0; }
.c-avatar-more {
  background: rgba(var(--text-rgb), 0.09) !important;
  color: var(--light-muted) !important;
  border-color: rgba(var(--text-rgb), 0.1) !important;
}

/* Progress Bar */
.c-progress { display: flex; flex-direction: column; gap: 8px; }
.c-progress-labels {
  display: flex; justify-content: space-between; align-items: baseline;
  font-size: 13px;
}
.c-progress-label { color: var(--light-muted); font-weight: 500; }
.c-progress-value { color: var(--snow); font-weight: 600; font-variant-numeric: tabular-nums; }
.c-progress-track {
  height: 8px;
  background: rgba(var(--text-rgb), 0.09);
  border-radius: 100px;
  overflow: hidden;
}
.c-progress-fill {
  height: 100%;
  border-radius: 100px;
  transition: width 0.3s ease;
}
.c-progress-fill-default { background: var(--teal); }
.c-progress-fill-green { background: var(--green); }
.c-progress-fill-yellow { background: var(--yellow); }
.c-progress-fill-red { background: var(--red); }
.c-progress-fill-teal { background: var(--teal); }
.c-progress-detail { font-size: 12px; color: var(--muted); }

/* Empty State */
.c-empty-state {
  text-align: center;
  padding: 48px 24px;
  background: var(--card-bg);
  border: 1px dashed var(--card-border);
  border-radius: 12px;
  display: flex; flex-direction: column; align-items: center; gap: 12px;
}
.c-empty-state-icon {
  color: var(--muted);
  margin-bottom: 4px;
}
.c-empty-state-title {
  font-size: 16px; font-weight: 600;
  color: var(--snow);
  margin: 0;
}
.c-empty-state-body {
  font-size: 14px;
  color: var(--light-muted);
  margin: 0;
  max-width: 400px;
}
.c-empty-state .c-button { margin-top: 4px; }

/* Icon component (standalone) */
/* SVG styling is handled via the render; no extra CSS needed */

/* Chart */
.c-chart {
  margin: 0;
  padding: 20px 22px 18px;
  background: rgba(var(--text-rgb),0.04);
  border: 1px solid rgba(var(--text-rgb),0.1);
  border-radius: 12px;
  display: flex; flex-direction: column; gap: 14px;
}
.c-chart-title {
  font-size: 13px; font-weight: 600;
  color: var(--snow);
  letter-spacing: 0.2px;
  margin: 0;
}
.c-chart-svg {
  width: 100%;
  height: auto;
  display: block;
  overflow: visible;
}
.c-chart-grid {
  stroke: rgba(var(--text-rgb),0.08);
  stroke-width: 1;
}
.c-chart-axis {
  fill: rgba(var(--text-rgb),0.55);
  font-size: 11px;
  font-weight: 500;
  font-variant-numeric: tabular-nums;
}
.c-chart-axis-y { text-anchor: end; }
.c-chart-axis-y-right { text-anchor: end; }
.c-chart-axis-x { text-anchor: middle; }
.c-chart-bar {
  rx: 2px;
  transition: opacity 0.15s;
}
.c-chart-bar:hover, .c-chart-slice:hover, .c-chart-dot:hover { opacity: 0.85; }
.c-chart-slice { stroke: var(--bg); stroke-width: 2; }
.c-chart-line { pointer-events: none; }
.c-chart-dot { cursor: default; }
.c-chart-empty {
  fill: rgba(var(--text-rgb),0.35);
  font-size: 13px;
}
.c-chart-legend {
  display: flex; flex-wrap: wrap;
  gap: 6px 16px;
  padding: 0; margin: 0;
  list-style: none;
  font-size: 12px;
  color: var(--light-muted);
}
.c-chart-legend-item {
  display: inline-flex; align-items: center; gap: 6px;
}
.c-chart-swatch {
  width: 10px; height: 10px;
  border-radius: 2px;
  display: inline-block;
}

/* Source pill (bottom-right floating dropdown) */
.source-pill {
  position: fixed;
  bottom: 20px;
  right: 20px;
  z-index: 200;
  overflow: visible;
}
.source-pill-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 8px 14px;
  font-size: 12px;
  font-weight: 500;
  color: var(--light-muted);
  background: var(--card-bg);
  border: 1px solid var(--card-border);
  border-radius: 100px;
  backdrop-filter: blur(8px);
  cursor: pointer;
  transition: all 0.15s;
  font-family: inherit;
}
.source-pill-btn:hover {
  color: var(--teal);
  border-color: var(--card-hover-border);
}
.source-pill-btn svg:last-child {
  width: 12px; height: 12px;
  transition: transform 0.15s;
}
.source-pill[data-open] .source-pill-btn svg:last-child {
  transform: rotate(180deg);
}
.source-pill-menu {
  position: absolute;
  bottom: 100%;
  right: 0;
  margin-bottom: 6px;
  min-width: 180px;
  background: var(--bg);
  border: 1px solid var(--card-border);
  border-radius: 10px;
  padding: 4px;
  opacity: 0;
  visibility: hidden;
  transform: translateY(6px);
  pointer-events: none;
  transition: opacity 0.15s, transform 0.15s, visibility 0.15s;
  backdrop-filter: blur(12px);
}
.source-pill[data-open] .source-pill-menu {
  opacity: 1;
  visibility: visible;
  transform: translateY(0);
  pointer-events: auto;
}
.source-pill-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 12px;
  font-size: 13px;
  font-family: inherit;
  color: var(--light-muted);
  background: none;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  text-decoration: none;
  white-space: nowrap;
  transition: background 0.1s, color 0.1s;
}
.source-pill-item:hover {
  background: var(--overlay-hover);
  color: var(--snow);
}
.source-pill-item svg { flex-shrink: 0; }
.source-pill-copied {
  color: var(--teal) !important;
}
@media print {
  .source-pill { display: none !important; }
}

/* Source editor (dev mode) */
.source-edit-wrap { margin-top: 0; }
.source-edit-textarea {
  display: block;
  width: 100%;
  min-height: 400px;
  background: var(--card-bg);
  color: var(--snow);
  border: 1px solid var(--card-border);
  border-radius: 10px;
  padding: 16px;
  font-family: 'SF Mono', 'Fira Code', 'Fira Mono', Menlo, Consolas, monospace;
  font-size: 13px;
  line-height: 1.6;
  resize: none;
  tab-size: 2;
  white-space: pre;
  overflow-x: auto;
  box-sizing: border-box;
}
.source-edit-textarea:focus {
  outline: none;
  border-color: var(--teal);
}
.source-edit-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 12px;
}
.source-edit-status {
  font-size: 13px;
  color: var(--light-muted);
}

/* ──────────────────── Print ──────────────────── */

/* Default @page — portrait content pages; deck in slides mode forces
   landscape full-bleed. `print-continuous` decks stay on the default
   portrait page. Standard shell uses a zero-margin named page so the
   theme background reaches the sheet edges; breathing room is added
   back as padding on .main-content inside @media print. */
@page { margin: 0.5in; }
@page deck-page { size: landscape; margin: 0; }
@page deck-page-square { size: 8.5in 8.5in; margin: 0; }
@page standard-page { margin: 0; }
body.shell-deck.print-slides { page: deck-page; }
body.shell-deck.print-square { page: deck-page-square; }
body.shell-standard { page: standard-page; }

@media print {
  .no-print { display: none !important; }
  .source-pill { display: none !important; }

  /* ── Shared: preserve accent colors, drop the site bar + sidebar ── */
  *, *::before, *::after { -webkit-print-color-adjust: exact !important; print-color-adjust: exact !important; }
  .site-bar { display: none !important; }
  .site-sidebar { display: none !important; }
  body.nav-layout-sidebar .main-content { margin-left: 0 !important; }

  /* ── Standard shell ── (dark theme preserved, edge-to-edge) ── */
  html, body.shell-standard { background: var(--bg) !important; }
  body.shell-standard { min-height: auto !important; }
  body.shell-standard .main-content { padding: 0.5in !important; }
  /* Avoid awkward page breaks inside structured blocks */
  body.shell-standard .c-card,
  body.shell-standard .c-stat,
  body.shell-standard .c-callout,
  body.shell-standard .c-step,
  body.shell-standard .c-ba-card,
  body.shell-standard .c-meta-item,
  body.shell-standard .c-empty-state { break-inside: avoid; page-break-inside: avoid; }
  body.shell-standard h1, body.shell-standard h2, body.shell-standard h3 { break-after: avoid; page-break-after: avoid; }

  /* ── Document shell ── (clean white-paper print) ── */
  body.shell-document { background: #fff !important; color: #000 !important; min-height: auto !important; }
  body.shell-document .doc-root { padding: 0 !important; max-width: 100% !important; }
  body.shell-document .doc-card { box-shadow: none !important; border: none !important; background: transparent !important; max-width: 100% !important; padding: 0 !important; }
  body.shell-document .doc-body { color: #000 !important; }
  body.shell-document .doc-body strong { color: #000 !important; }
  body.shell-document .doc-body h1, body.shell-document .doc-body h2 { color: #000 !important; }
  body.shell-document .c-step,
  body.shell-document .c-callout,
  body.shell-document .c-card { break-inside: avoid; page-break-inside: avoid; }

  /* ── Deck shell: print ── */
  html:has(body.shell-deck), body.shell-deck { background: var(--bg) !important; }
  body.shell-deck { height: auto !important; overflow: visible !important; }
  body.shell-deck .deck-root { position: static !important; height: auto !important; }
  body.shell-deck .deck-viewport { overflow: visible !important; height: auto !important; }
  body.shell-deck .deck-track { transform: none !important; flex-direction: column !important; height: auto !important; }
  body.shell-deck .deck-slide { min-width: 100% !important; overflow: visible !important; }
  body.shell-deck .deck-nav { display: none !important; }

  /* Drop the JS-applied scale transform when printing — the screen-fit
     scale calculation has nothing to do with the print page size and
     leaves content top-anchored on the printed page. Browsers honor
     `!important` on regular CSS over inline styles set via JS. */
  body.shell-deck .deck-inner {
    transform: none !important;
    transform-origin: unset !important;
  }

  /* Default print mode: one slide per landscape page, Keynote-style.
     Pin slide height to the page so flex centering inside .deck-inner
     actually has a container to center against — otherwise content hugs
     the top of each page. Width is the @page deck-page landscape size. */
  body.shell-deck.print-slides .deck-slide {
    height: 7.5in !important;
    min-height: 7.5in !important;
    page-break-after: always;
  }
  body.shell-deck.print-slides .deck-slide:last-child { page-break-after: avoid; }
  body.shell-deck.print-slides .deck-inner {
    min-height: 7.5in !important;
    padding: 0.4in 0.5in !important;
  }

  /* Continuous print mode: slides flow as one scrolling document with a
     thin separator between them. Portrait orientation, no forced page
     breaks. Nicer for sharing a readable artifact than a presentation. */
  body.shell-deck.print-continuous { page: auto !important; }
  body.shell-deck.print-continuous .deck-slide {
    height: auto !important;
    min-height: 0 !important;
    page-break-after: auto;
    border-bottom: 1px solid rgba(var(--text-rgb), 0.12);
  }
  body.shell-deck.print-continuous .deck-slide:last-child { border-bottom: none; }
  body.shell-deck.print-continuous .deck-inner {
    min-height: 0 !important;
    padding: 0.5in 0.6in !important;
  }

  /* Square print mode: one slide per 8.5in × 8.5in page, sized to fit
     LinkedIn document carousels (and other near-square viewports) without
     letterboxing. Uses CSS grid + place-items: center on the slide so
     content lands in the middle regardless of how short it is, with the
     inner card itself behaving as a flex column that holds component
     spacing. */
  body.shell-deck.print-square .deck-slide {
    height: 8.5in !important;
    min-height: 8.5in !important;
    padding: 0 !important;
    display: grid !important;
    place-items: center !important;
    box-sizing: border-box !important;
    page-break-after: always;
  }
  body.shell-deck.print-square .deck-slide:last-child { page-break-after: avoid; }
  body.shell-deck.print-square .deck-inner {
    height: auto !important;
    min-height: 0 !important;
    width: 100% !important;
    max-width: 100% !important;
    padding: 0.6in 0.7in !important;
    margin: 0 !important;
    display: flex !important;
    flex-direction: column !important;
    justify-content: center !important;
    box-sizing: border-box !important;
  }
}


/* ──────────────────── Mobile responsiveness ──────────────────── */

/* Tablet-ish (≤768px): stack multi-column components, tighten chrome,
   collapse the top-bar nav into a hamburger menu. The sidebar layout has
   its own responsive rules above and is exempt. */
@media (max-width: 768px) {
  .container { padding: 0 20px; }
  .site-bar { padding: 0 20px; gap: 12px; }
  .site-bar-subtitle { display: none; }
  /* Slightly tighter logo cap on phones so it never fights the hamburger. */
  .site-bar-logo { max-height: 28px; max-width: 160px; }

  /* Top-bar nav → hamburger. Hide the inline link row; show the toggle. */
  body:not(.nav-layout-sidebar) .site-bar .nav-menu-toggle {
    display: inline-flex;
  }
  body:not(.nav-layout-sidebar) .site-bar .site-nav-links {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    background: var(--bg);
    border-bottom: 1px solid var(--card-border);
    box-shadow: 0 8px 24px rgba(0,0,0,0.25);
    flex-direction: column;
    align-items: stretch;
    gap: 0;
    padding: 8px 12px 16px;
    max-height: calc(100vh - 56px);
    overflow-y: auto;
    /* Hidden by default; revealed via [data-open] below. */
    display: none;
    z-index: 20;
  }
  body:not(.nav-layout-sidebar) .site-bar nav[data-open] .site-nav-links {
    display: flex;
  }
  /* Mobile panel link styling: full-width rows, no pill hover. */
  body:not(.nav-layout-sidebar) .site-bar .site-nav-links .nav-link,
  body:not(.nav-layout-sidebar) .site-bar .site-nav-links .nav-link-parent {
    width: 100%;
    justify-content: flex-start;
    padding: 12px 10px;
    font-size: 15px;
    border-radius: 0;
    border-bottom: 1px solid rgba(var(--text-rgb), 0.06);
  }
  body:not(.nav-layout-sidebar) .site-bar .site-nav-links .nav-link:last-child,
  body:not(.nav-layout-sidebar) .site-bar .site-nav-links .nav-link-group:last-child .nav-link-parent {
    border-bottom: none;
  }
  /* Dropdown groups: show children inline (no hover), indented. */
  body:not(.nav-layout-sidebar) .site-bar .site-nav-links .nav-link-group {
    display: flex;
    flex-direction: column;
    align-items: stretch;
  }
  body:not(.nav-layout-sidebar) .site-bar .site-nav-links .nav-chevron {
    display: none;
  }
  body:not(.nav-layout-sidebar) .site-bar .site-nav-links .nav-dropdown {
    position: static;
    opacity: 1;
    pointer-events: auto;
    transform: none;
    box-shadow: none;
    background: transparent;
    border: none;
    padding: 0 0 8px 16px;
    min-width: 0;
  }
  body:not(.nav-layout-sidebar) .site-bar .site-nav-links .nav-dropdown::before {
    display: none;
  }
  body:not(.nav-layout-sidebar) .site-bar .site-nav-links .nav-dropdown .nav-link {
    padding: 8px 10px;
    font-size: 14px;
    color: rgba(var(--text-rgb), 0.6);
    border-bottom: none;
  }

  /* Stack any inline-style multi-column grids: c-columns and c-stat-grid
     both set grid-template-columns inline, so the override needs !important. */
  .c-columns[style*="grid-template-columns"] { grid-template-columns: 1fr !important; }

  /* Card-grid with arrows: stack vertically, rotate arrows to point down. */
  .c-card-grid-arrow { flex-direction: column; }
  .c-card-arrow { transform: rotate(90deg); padding: 4px 0; }

  /* Before/after cards: tighter padding so they don't feel oversized. */
  .c-ba-card { padding: 24px; }

  /* Section + card padding step-down. */
  .c-card { padding: 20px; }

  /* Deck shell on tablet: tighter padding, step typography down one tier
     from the big desktop sizes. */
  body.shell-deck .deck-inner { padding: 36px 28px 64px; gap: 20px; }
  body.shell-deck .c-header-title { font-size: 28px; }
  body.shell-deck .c-header-subtitle { font-size: 16px; }
  body.shell-deck .c-stat-value { font-size: 32px; }
  body.shell-deck .c-card-title { font-size: 18px; }
  body.shell-deck .deck-slide-cover .c-header-title { font-size: 40px; }
  body.shell-deck .deck-slide-cover .c-header-subtitle { font-size: 16px; }
}

/* Phone (≤640px): collapse remaining grids, step down type, let tables scroll. */
@media (max-width: 640px) {
  /* Stat grid stacks to 1 column. The inline style needs !important. */
  .c-stat-grid[style*="grid-template-columns"] { grid-template-columns: 1fr !important; }
  .c-stat { padding: 16px 18px; }
  .c-stat-value { font-size: 24px; }

  /* Header type step-down. */
  .c-header-title { font-size: 26px; }
  .c-header-subtitle { font-size: 15px; }

  .c-hero { padding: 40px 0 32px; }
  .c-hero-title { font-size: 32px; }
  .c-hero-subtitle { font-size: 15px; }

  /* Deck shell on phone: smallest step. Keeps slides legible without
     overflowing the viewport on vertical phone screens. */
  body.shell-deck .c-header-title { font-size: 24px; }
  body.shell-deck .c-header-subtitle { font-size: 14px; }
  body.shell-deck .c-stat-value { font-size: 26px; }
  body.shell-deck .c-card-title { font-size: 17px; }
  body.shell-deck .deck-slide-cover .c-header-title { font-size: 32px; }
  body.shell-deck .deck-slide-cover .c-header-subtitle { font-size: 15px; }

  /* Section heading tighter. */
  .c-section-header { margin-bottom: 16px; }

  /* Before/after final step. */
  .c-ba-card { padding: 20px; }
  .c-ba-title { font-size: 18px; }
  .c-ba-before, .c-ba-after { font-size: 15px; }

  /* Split compare: stack vertically on mobile. */
  .c-split-compare { grid-template-columns: 1fr; }
  .c-sc-left { border-right: none; border-bottom: 1px solid rgba(var(--text-rgb),0.08); }

  /* Tables: preserve shape, scroll horizontally. */
  .c-table-wrap { overflow-x: auto; -webkit-overflow-scrolling: touch; }
  .c-table { min-width: 480px; }
  .c-table th, .c-table td { padding: 10px 12px; font-size: 13px; }

  /* Tab buttons: scroll horizontally instead of wrapping. */
  .c-tab-buttons {
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    flex-wrap: nowrap;
  }
  .c-tab-buttons::-webkit-scrollbar { display: none; }
  .tab-btn { flex-shrink: 0; }

  /* Accordion tightens. */
  .c-accordion-item { padding: 0; }

  /* Code blocks: slightly smaller type on phone. */
  .c-code { font-size: 12px; padding: 16px; }
  .c-markdown pre { padding: 16px; }

  /* Document shell: less generous padding on narrow screens. */
  body.shell-document .doc-root { padding: 24px 12px 60px; }
  body.shell-document .doc-card { padding: 24px 20px; }
}

/* Deck shell on narrow phones in portrait: nudge the viewer to rotate. The
   deck is designed for landscape, but we respect pinch-zoom and swipe so
   this is a soft hint, not a wall. */
@media (max-width: 640px) and (orientation: portrait) {
  body.shell-deck::before {
    content: "↻ Tip: rotate your phone to landscape for the full deck view.";
    position: fixed;
    top: 0; left: 0; right: 0;
    padding: 8px 12px;
    text-align: center;
    font-size: 12px;
    font-weight: 500;
    background: rgba(var(--accent-rgb), 0.12);
    color: var(--teal);
    border-bottom: 1px solid rgba(var(--accent-rgb), 0.25);
    z-index: 1000;
    letter-spacing: 0.2px;
  }
  body.shell-deck .deck-root { top: 34px; }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_decoration_css_when_both_none() {
        assert!(decoration_css(&dark(), Texture::None, Glow::None).is_empty());
    }

    #[test]
    fn dots_uses_text_rgb_and_print_strips() {
        let css = decoration_css(&dark(), Texture::Dots, Glow::None);
        assert!(css.contains("body::before"));
        assert!(css.contains("radial-gradient(rgba(247, 247, 242"));
        assert!(css.contains("@media print"));
    }

    #[test]
    fn glow_uses_accent_rgb() {
        let css = decoration_css(&dark(), Texture::None, Glow::Accent);
        assert!(css.contains("body::after"));
        assert!(css.contains("rgba(137, 152, 120"));
    }

    #[test]
    fn grain_emits_url_encoded_svg_data_uri() {
        let css = decoration_css(&dark(), Texture::Grain, Glow::None);
        assert!(css.contains("data:image/svg+xml;utf8,"));
        // SVG body must be URL-encoded — no raw <, >, # in the URL payload.
        let uri_start = css.find("data:image/svg+xml;utf8,").unwrap();
        let uri_end = css[uri_start..].find('"').unwrap() + uri_start;
        let payload = &css[uri_start..uri_end];
        assert!(!payload.contains('<'));
        assert!(!payload.contains('>'));
    }

    #[test]
    fn rainbow_themes_swap_accent_only() {
        // Each rainbow theme keeps the chosen base — same bg, same text — and
        // swaps just the accent hex. Default mode is Dark.
        let cases = [
            ("red", "#BB7777"),
            ("orange", "#BB8C66"),
            ("yellow", "#B8A866"),
            ("green", "#7A9878"),
            ("blue", "#7897B8"),
            ("indigo", "#8A7FBB"),
            ("violet", "#AB7FBB"),
        ];
        let d = dark();
        for (name, hex) in cases {
            let t = Theme::named(name, Mode::Dark);
            assert_eq!(t.accent, hex, "{} accent", name);
            assert_eq!(t.bg, d.bg, "{} keeps dark bg", name);
            assert_eq!(t.text, d.text, "{} keeps dark text", name);
        }
    }

    #[test]
    fn rainbow_themes_on_light_mode_use_light_base() {
        // Same accent swap, but base comes from light(), not dark().
        let l = light();
        let t = Theme::named("red", Mode::Light);
        assert_eq!(t.accent, "#BB7777");
        assert_eq!(t.bg, l.bg, "light-mode red uses light bg");
        assert_eq!(t.text, l.text, "light-mode red uses light text");
    }

    #[test]
    fn dark_and_light_themes_ignore_mode() {
        // `theme: dark` + mode: light should still return the full dark theme.
        // Same for `theme: light` + mode: dark.
        assert_eq!(Theme::named("dark", Mode::Light).bg, dark().bg);
        assert_eq!(Theme::named("light", Mode::Dark).bg, light().bg);
    }

    #[test]
    fn unknown_theme_falls_back_to_dark() {
        let t = Theme::named("purple-mountain-majesty", Mode::Dark);
        assert_eq!(t.accent, dark().accent);
    }

    #[test]
    fn topography_bakes_text_color_into_stroke() {
        let css = decoration_css(&dark(), Texture::Topography, Glow::None);
        // URL-encoded SVG keeps commas/parens raw, only spaces become %20.
        assert!(css.contains("rgb(247,%20247,%20242)"));
    }
}

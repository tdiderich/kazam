//! `kazam init <name>` scaffolds a new site directory with a minimal,
//! well-commented starter - kazam.yaml, index.yaml, AGENTS.md, .gitignore.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

pub fn run(path: &str) -> Result<()> {
    let dir = Path::new(path);
    if dir.exists() {
        bail!(
            "'{}' already exists - choose another name or remove it first",
            path
        );
    }

    let site_name = dir
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("my-site")
        .to_string();

    fs::create_dir_all(dir).with_context(|| format!("creating {:?}", dir))?;

    fs::write(
        dir.join("kazam.yaml"),
        KAZAM_YAML.replace("{{SITE_NAME}}", &site_name),
    )?;
    fs::write(
        dir.join("index.yaml"),
        INDEX_YAML.replace("{{SITE_NAME}}", &site_name),
    )?;
    fs::write(dir.join("AGENTS.md"), AGENTS_MD)?;
    fs::write(dir.join(".gitignore"), GITIGNORE)?;

    if let Err(e) = crate::workspace::ensure(dir) {
        eprintln!("  ⚠ workspace setup failed: {e:#}");
    }

    println!("\n  ✓ Created {} with:", path);
    println!("    kazam.yaml       site config (name, theme, nav)");
    println!("    index.yaml       home page");
    println!("    AGENTS.md        LLM authoring guide");
    println!("    .gitignore");
    println!("    .kazam/          workspace (anatomy index, task tracking)");
    println!();
    println!("  Next:");
    println!("    cd {}", path);
    println!("    kazam dev .          # watch + serve at localhost:3000");
    println!();

    Ok(())
}

const KAZAM_YAML: &str = r##"# Site configuration. Shared across every page.
# Every available field is listed below - uncomment the ones you want.

name: {{SITE_NAME}}

# Theme: "dark" (default), "light", or one of the rainbow accents:
# red | orange | yellow | green | blue | indigo | violet. Rainbow themes
# sit on a neutral base and only swap the accent color.
theme: dark

# Base tone for rainbow themes: "dark" (default) or "light". Ignored when
# theme is already dark or light.
# mode: light

# Override individual theme tokens. Keys not listed here fall back to the
# base theme's default. Full list of tokens: bg, surface, surface_strong,
# border, border_strong, accent, accent_soft, text, text_muted, text_subtle,
# overlay_hover, green, yellow, red, header_border.
# colors:
#   accent: "#3CCECE"          # primary brand color (links, eyebrows, borders)
#   bg: "#090D18"              # page background
#   text: "#F0F0F7"             # primary text
#   text_muted: "#ABABC1"       # secondary text
#   text_subtle: "#4C556A"      # labels, captions
#   green: "#34D399"
#   yellow: "#FBBF24"
#   red: "#F87171"

# Point to a favicon. Any non-.yaml file in this directory is copied
# verbatim into the build output, so a relative path just works.
# Simple form: one file for every slot.
# favicon: favicon.svg
# Full form: named slots for each icon variant.
# favicon:
#   svg: favicon.svg
#   png: favicon.png
#   ico: favicon.ico
#   apple_touch_icon: apple-touch-icon.png

# Optional logo image shown in the site bar instead of the text `name:`.
# Shorthand: a path (SVG recommended; PNG/JPEG/WebP work). Lead with `/`
# for site-root paths so the depth-aware rewriter keeps them portable
# on nested pages.
# logo: /assets/logo.svg
# Expanded form with optional height (px) + alt text:
# logo:
#   src: /assets/logo.svg
#   height: 32               # caps rendered height; defaults to site-bar content
#   alt: Your Company Name   # defaults to `name:` above

# Source pill (bottom-right dropdown) is on by default. It offers
# "Copy edit prompt" + "View source" for every page. Set to false
# to hide it entirely.
# view_source: false

# Link the "Edit on GitHub" option in the source pill to your repo.
# Each page's YAML path is appended automatically.
# edit_url: https://github.com/your-org/your-repo/edit/main/docs

# Subtle background pattern painted behind every page. Tinted via the
# theme's text color so it adapts to dark/light. Off by default.
# Options: none | dots | grid | grain | topography | diagonal
# texture: dots

# Soft accent-colored radial glow painted behind the page header area.
# Off by default. Options: none | accent | corner
# glow: accent

# Nav layout: "top" (default - sticky top bar, dropdowns for nested
# entries) or "sidebar" (fixed left-side sidebar, nested entries become
# labeled sections). Only applies to shell: standard pages.
# nav_layout: sidebar

# Nav appears in the sticky header of every `shell: standard` page.
# Lead each in-site href with `/` so the renderer treats it as a
# site-root path and prepends the depth base on nested pages - that's
# what keeps the nav working from any subdirectory. Bare names are
# page-relative; `http(s)://` passes through.
nav:
  - label: Home
    href: /index.html
  # - label: Docs
  #   href: /docs.html
  # - label: Reference
  #   children:
  #     - label: API
  #       href: /reference/api.html
  #     - label: Config
  #       href: /reference/config.html
  # - label: GitHub
  #   href: https://github.com/your-org/your-repo
"##;

const INDEX_YAML: &str = r#"# The home page. See AGENTS.md for the full component catalog.
title: {{SITE_NAME}}
shell: standard

components:
  - type: header
    title: {{SITE_NAME}}
    eyebrow: Welcome
    subtitle: A starter site scaffolded by `kazam init`.

  - type: section
    eyebrow: Next steps
    heading: Make it yours
    components:
      - type: markdown
        body: |
          1. Edit this file (`index.yaml`) - every page is a list of components.
          2. Add new pages - any `.yaml` file in this directory becomes a page.
          3. Read `AGENTS.md` for the full component reference.

      - type: button_group
        buttons:
          - label: Read AGENTS.md
            href: AGENTS.md
            variant: primary
          - label: kazam source
            href: https://github.com/tdiderich/kazam
            variant: secondary
            external: true

  - type: section
    eyebrow: Components in action
    heading: A few primitives
    components:
      - type: stat_grid
        columns: 3
        stats:
          - label: Pages
            value: "1"
            detail: Only this one, for now.
            color: default
          - label: Components
            value: "30+"
            detail: All documented in AGENTS.md.
            color: green
          - label: Runtime
            value: "0"
            detail: Static HTML. Serve from anywhere.
            color: teal

      - type: callout
        variant: info
        title: Tip
        body: "Run `kazam dev .` to watch your files and live-reload the browser on every save."
"#;

const GITIGNORE: &str = r#"/_site
"#;

const AGENTS_MD: &str = include_str!("../AGENTS.md.template");

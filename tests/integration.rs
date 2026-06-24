//! Integration tests — invoke the kazam binary end-to-end.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> PathBuf {
    // cargo sets CARGO_BIN_EXE_<name> env var for the test runner
    PathBuf::from(env!("CARGO_BIN_EXE_kazam"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn tmp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kazam-test-{}", name));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).expect("read file")
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(haystack.contains(needle), "expected to find {:?}", needle);
}

#[test]
fn build_kb_example() {
    let out = tmp_dir("kb");
    let src = repo_root().join("examples/kb");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let index = read(&out.join("index.html"));
    assert_contains(&index, "Customer Portfolio");
    assert_contains(&index, "Acme Corp");
    // Badge color classes render
    assert_contains(&index, "c-badge-green");
    assert_contains(&index, "c-badge-yellow");
}

#[test]
fn build_docs_site() {
    let out = tmp_dir("docs");
    let src = repo_root().join("docs");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());

    // llms.txt should exist and list known pages
    let llms = read(&out.join("llms.txt"));
    assert_contains(&llms, "# kazam");
    assert_contains(&llms, "Content components");
    assert_contains(&llms, "Why kazam");

    // Each page has the source pill
    let index = read(&out.join("index.html"));
    assert_contains(&index, r#"class="source-pill""#);

    // Source YAMLs copied next to rendered HTML
    assert!(out.join("components/content.yaml").exists());

    // Site-wide texture + glow layers landed in CSS
    assert_contains(&index, "body::before");
    assert_contains(&index, "body::after");
}

#[test]
fn build_release_minifies() {
    let out = tmp_dir("release");
    let src = repo_root().join("docs");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .arg("--release")
        .status()
        .expect("run kazam build --release");
    assert!(status.success());

    let html = read(&out.join("index.html"));
    // HTML comments stripped (we don't emit any but this guards future regressions)
    assert!(!html.contains("<!-- "));
    // Multi-byte content preserved
    assert_contains(&html, "—");
    // Release builds must NOT inject the dev hot-reload poller (issue #28) —
    // the site is served from static hosts where /__kazam_version__ 404s.
    assert!(
        !html.contains("__kazam_version__"),
        "release build leaked the dev hot-reload poller"
    );
    // Standard shell print CSS uses the zero-margin named page so PDF
    // exports reach the sheet edges (issue #27).
    assert_contains(&html, "@page standard-page");
    assert_contains(&html, "body.shell-standard{page:standard-page}");
}

#[test]
fn dev_build_still_injects_hot_reload_poller() {
    // Counterpart to the release assertion above: without --release, the
    // dev poller must still be injected so `kazam dev` can hot-reload.
    let out = tmp_dir("dev-poller");
    let src = repo_root().join("docs");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());

    let html = read(&out.join("index.html"));
    assert_contains(&html, "__kazam_version__");
}

#[test]
fn init_creates_minimal_site_that_builds() {
    let dir = tmp_dir("init");
    let status = Command::new(bin())
        .args(["init"])
        .arg(&dir)
        .status()
        .expect("run kazam init");
    assert!(status.success());

    // Scaffold has expected files
    assert!(dir.join("kazam.yaml").exists());
    assert!(dir.join("index.yaml").exists());
    assert!(dir.join("AGENTS.md").exists());
    assert!(dir.join(".gitignore").exists());

    // Building it should succeed
    let out = dir.join("_site");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("build scaffolded site");
    assert!(status.success());

    assert!(out.join("index.html").exists());
    assert!(out.join("llms.txt").exists());
}

#[test]
fn page_level_texture_and_glow_override_site_config() {
    // Site-wide sets texture: grid + glow: accent. One page sets texture:
    // none (opt out) and another sets glow: corner (different preset). The
    // rendered HTML for each page must reflect the per-page override, not
    // the site-wide default.
    let dir = tmp_dir("overrides");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kazam.yaml"),
        "name: Overrides\ntheme: dark\ntexture: grid\nglow: accent\n",
    )
    .unwrap();
    // Default page: inherits site-wide (should have grid + accent glow).
    std::fs::write(
        dir.join("index.yaml"),
        "title: Index\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    // Opts out of texture entirely.
    std::fs::write(
        dir.join("plain.yaml"),
        "title: Plain\nshell: standard\ntexture: none\ncomponents:\n  - type: header\n    title: Plain\n",
    )
    .unwrap();
    // Swaps to the corner glow variant + different texture.
    std::fs::write(
        dir.join("corner.yaml"),
        "title: Corner\nshell: standard\ntexture: dots\nglow: corner\ncomponents:\n  - type: header\n    title: Corner\n",
    )
    .unwrap();

    let out = tmp_dir("overrides-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());

    let index = read(&out.join("index.html"));
    let plain = read(&out.join("plain.html"));
    let corner = read(&out.join("corner.html"));

    // Inherits both site-wide layers.
    assert_contains(&index, "linear-gradient"); // grid texture signature
    assert_contains(&index, "ellipse at center"); // accent glow signature

    // plain.yaml turned texture off — the grid texture signature should
    // be absent even though the site-wide config specifies it. (The print
    // `body::before, body::after { display: none }` rule still appears
    // because glow is still active, so we check for the texture signature
    // specifically rather than any mention of body::before.)
    assert!(
        !plain.contains("linear-gradient"),
        "plain page should not render the grid texture"
    );
    // But plain still inherits the site-wide accent glow.
    assert_contains(&plain, "ellipse at center");

    // corner.yaml swapped to dots + corner glow.
    assert_contains(&corner, "radial-gradient(rgba"); // dots texture signature
    assert_contains(&corner, "circle at top right"); // corner glow signature
    assert!(
        !corner.contains("ellipse at center"),
        "corner page should not have the accent ellipse glow"
    );
}

#[test]
fn nested_nav_renders_dropdown_in_top_layout() {
    let dir = tmp_dir("nav-dropdown");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kazam.yaml"),
        "name: NavTest\ntheme: dark\nnav:\n  - label: Home\n    href: index.html\n  - label: Docs\n    children:\n      - label: Guide\n        href: guide.html\n      - label: Reference\n        href: ref.html\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();

    let out = tmp_dir("nav-dropdown-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());

    let index = read(&out.join("index.html"));
    assert_contains(&index, r#"class="nav-link-group""#);
    assert_contains(&index, r#"class="nav-link nav-link-parent""#);
    assert_contains(&index, r#"class="nav-dropdown""#);
    // Both children render inside the dropdown
    assert_contains(&index, "Guide");
    assert_contains(&index, "Reference");
}

#[test]
fn sidebar_layout_renders_aside_and_hides_top_nav() {
    let dir = tmp_dir("nav-sidebar");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kazam.yaml"),
        "name: SideTest\ntheme: dark\nnav_layout: sidebar\nnav:\n  - label: Overview\n    href: index.html\n  - label: Guides\n    children:\n      - label: Intro\n        href: intro.html\n      - label: Advanced\n        href: advanced.html\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();

    let out = tmp_dir("nav-sidebar-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());

    let index = read(&out.join("index.html"));
    // Sidebar aside + body class present
    assert_contains(&index, "nav-layout-sidebar");
    assert_contains(&index, r#"class="site-sidebar""#);
    assert_contains(&index, r#"class="sidebar-section""#);
    assert_contains(&index, r#"class="sidebar-section-label">Guides"#);
    // Both nested children render in the sidebar
    assert_contains(&index, "Intro");
    assert_contains(&index, "Advanced");
}

#[test]
fn build_skips_hidden_entries_and_is_idempotent() {
    // Source directory with a hidden dir (simulating .git) alongside the
    // yaml files. Kazam should not copy the hidden dir into the output,
    // and running build twice in a row should succeed both times.
    let dir = tmp_dir("hidden");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Hidden\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();

    // Hidden directory with a nested file — must be skipped.
    let hidden = dir.join(".stealth");
    std::fs::create_dir_all(hidden.join("nested")).unwrap();
    std::fs::write(hidden.join("nested/file.bin"), b"should-not-copy").unwrap();

    let out = tmp_dir("hidden-out");

    let run = || {
        Command::new(bin())
            .args(["build"])
            .arg(&dir)
            .arg("--out")
            .arg(&out)
            .status()
            .expect("run kazam build")
    };

    assert!(run().success(), "first build failed");
    assert!(
        run().success(),
        "second build failed — walker not idempotent"
    );

    // Hidden dir must not be present in output.
    assert!(
        !out.join(".stealth").exists(),
        "hidden dir leaked into output"
    );
}

#[test]
fn chart_component_renders_svg_for_every_kind() {
    // One page exercises pie, vertical bar, stacked bar, horizontal bar,
    // single-series timeseries, and multi-series timeseries. Each kind must
    // produce SVG, and the multi-series variants must render a legend.
    let dir = tmp_dir("charts");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Charts\ntheme: dark\n").unwrap();
    let page = r#"
title: Charts
shell: standard
components:
  - type: chart
    kind: pie
    title: Pie
    data:
      - { label: A, value: 60 }
      - { label: B, value: 40, color: green }
  - type: chart
    kind: bar
    title: VBar
    data:
      - { label: Jan, value: 100 }
      - { label: Feb, value: 200 }
  - type: chart
    kind: bar
    title: StackedBar
    series:
      - label: Organic
        points:
          - { label: Jan, value: 80 }
          - { label: Feb, value: 110 }
      - label: Paid
        color: green
        points:
          - { label: Jan, value: 30 }
          - { label: Feb, value: 50 }
  - type: chart
    kind: bar
    orientation: horizontal
    title: HBar
    data:
      - { label: Docs, value: 2840 }
      - { label: Pricing, value: 1720 }
  - type: chart
    kind: timeseries
    title: Line
    data:
      - { label: W1, value: 10 }
      - { label: W2, value: 20 }
      - { label: W3, value: 15 }
  - type: chart
    kind: timeseries
    title: MultiLine
    series:
      - label: A
        points:
          - { label: W1, value: 10 }
          - { label: W2, value: 20 }
      - label: B
        color: green
        points:
          - { label: W1, value: 5 }
          - { label: W2, value: 9 }
"#;
    std::fs::write(dir.join("index.yaml"), page).unwrap();

    let out = tmp_dir("charts-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());

    let html = read(&out.join("index.html"));

    // Wrappers for each kind present
    assert_contains(&html, r#"class="c-chart c-chart-pie""#);
    assert_contains(&html, r#"class="c-chart c-chart-bar""#);
    assert_contains(&html, r#"class="c-chart c-chart-timeseries""#);

    // Pie rendered as SVG paths with titles (accessible tooltips)
    assert_contains(&html, r#"class="c-chart-slice""#);

    // Bar rendered as SVG rects
    assert_contains(&html, r#"class="c-chart-bar""#);

    // Timeseries rendered as a polyline
    assert_contains(&html, r#"class="c-chart-line""#);

    // Multi-series charts render a legend; single-series bar/timeseries don't
    assert_contains(&html, r#"class="c-chart-legend""#);

    // SemColor threading through: green was requested explicitly somewhere.
    // Charts use the canonical hex palette (not theme CSS vars) so stacks
    // stay distinguishable on themes that remap --green.
    assert_contains(&html, "#34D399");

    // Titles rendered as figcaptions
    assert_contains(&html, r#"class="c-chart-title">Pie</figcaption>"#);
    assert_contains(&html, r#"class="c-chart-title">StackedBar</figcaption>"#);

    // ARIA role + label on the figure
    assert_contains(&html, r#"role="img""#);
}

#[test]
fn wish_list_succeeds() {
    let output = Command::new(bin())
        .args(["wish", "list"])
        .output()
        .expect("run kazam wish list");
    assert!(output.status.success(), "wish list failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_contains(&stdout, "hubspot-icp");
}

#[test]
fn wish_list_json_returns_valid_json() {
    let output = Command::new(bin())
        .args(["wish", "list", "--json"])
        .output()
        .expect("run kazam wish list --json");
    assert!(output.status.success(), "wish list --json failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(parsed.get("registry").is_some(), "should have registry key");
}

#[test]
fn wish_init_rejects_unknown_name() {
    let output = Command::new(bin())
        .args(["wish", "init", "nope-does-not-exist"])
        .output()
        .expect("run kazam wish init <bogus>");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_contains(&stderr, "not found in registry");
}

#[test]
fn wish_list_shows_local_after_install() {
    let dir = tmp_dir("wish-local-list");
    std::fs::create_dir_all(dir.join("wishes/test-wish")).unwrap();
    std::fs::write(
        dir.join("wishes/test-wish/wish.yaml"),
        "name: test-wish\ndescription: A test wish\ntags: [test]\n",
    )
    .unwrap();

    let output = Command::new(bin())
        .args(["wish", "list"])
        .current_dir(&dir)
        .output()
        .expect("run kazam wish list with local wish");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_contains(&stdout, "LOCAL");
    assert_contains(&stdout, "test-wish");
}

#[test]
fn build_skips_nested_site_directories() {
    // Running `kazam build` from a directory that contains previously-built
    // sub-sites (each with their own `_site/` full of .html and .yaml) must
    // not recursively ingest those outputs as if they were source. This is
    // the bug where running `kazam dev` in /tmp caused 181 pages of
    // cross-contamination.
    let dir = tmp_dir("build-skips-nested-site");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Outer\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Outer home\n",
    )
    .unwrap();

    // Simulate a nested previously-built sub-site with its own _site/ that
    // happens to contain yaml files (e.g. source-view YAMLs, wish
    // reference/example-deck.yaml, whatever).
    let nested_site = dir.join("sub").join("_site");
    std::fs::create_dir_all(&nested_site).unwrap();
    std::fs::write(
        nested_site.join("contaminating.yaml"),
        "title: SHOULD_NOT_BUILD\nshell: standard\ncomponents:\n  - type: header\n    title: bad\n",
    )
    .unwrap();
    std::fs::write(
        nested_site.join("contaminating.html"),
        "<html>pollution</html>",
    )
    .unwrap();

    let out = tmp_dir("build-skips-nested-site-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());

    // Outer site built.
    assert!(out.join("index.html").exists());
    // Nested _site content was NOT ingested.
    assert!(
        !out.join("sub/_site/contaminating.html").exists(),
        "nested _site leaked into output"
    );
    assert!(
        !out.join("contaminating.html").exists(),
        "nested _site yaml got ingested"
    );
}

#[test]
fn logo_shorthand_renders_img_in_site_bar() {
    let dir = tmp_dir("logo-shorthand");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kazam.yaml"),
        "name: Acme\ntheme: dark\nlogo: assets/logo.svg\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());
    let html = read(&out.join("index.html"));
    assert_contains(&html, r#"class="site-bar-brand""#);
    assert_contains(
        &html,
        r#"class="site-bar-logo" src="assets/logo.svg" alt="Acme""#,
    );
    assert!(
        !html.contains(r#"class="site-bar-name""#),
        "text name treatment should be replaced by the logo img"
    );
}

#[test]
fn logo_expanded_form_respects_height_and_alt() {
    let dir = tmp_dir("logo-expanded");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kazam.yaml"),
        "name: Acme\ntheme: dark\nlogo:\n  src: assets/logo.svg\n  height: 40\n  alt: Acme Corporation\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());
    let html = read(&out.join("index.html"));
    assert_contains(&html, r#"alt="Acme Corporation""#);
    assert_contains(&html, r#"height="40""#);
    assert_contains(&html, r#"style="max-height:40px""#);
    assert_contains(&html, r#"aria-label="Acme Corporation""#);
}

#[test]
fn logo_src_site_root_path_resolves_depth_aware() {
    // Site-root paths (leading `/`) are the portable form for `kazam.yaml`
    // site config: the renderer prepends the depth base on every page so a
    // single source path keeps working under subpath deployments.
    let dir = tmp_dir("logo-depth");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(
        dir.join("kazam.yaml"),
        "name: Acme\ntheme: dark\nlogo: /assets/logo.svg\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("sub/page.yaml"),
        "title: Sub\nshell: standard\ncomponents:\n  - type: header\n    title: Sub\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());
    let root = read(&out.join("index.html"));
    assert_contains(&root, r#"src="assets/logo.svg""#);
    let sub = read(&out.join("sub/page.html"));
    assert_contains(&sub, r#"src="../assets/logo.svg""#);
}

#[test]
fn logo_bare_path_is_page_relative() {
    // Bare paths (no leading `/`) are page-relative, matching standard
    // HTML semantics — the browser resolves them against the current page,
    // so the renderer leaves them alone.
    let dir = tmp_dir("logo-bare");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(
        dir.join("kazam.yaml"),
        "name: Acme\ntheme: dark\nlogo: assets/logo.png\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("sub/page.yaml"),
        "title: Sub\nshell: standard\ncomponents:\n  - type: header\n    title: Sub\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());
    let sub = read(&out.join("sub/page.html"));
    assert_contains(&sub, r#"src="assets/logo.png""#);
    assert!(
        !sub.contains("../assets/logo.png"),
        "bare path must not be rewritten with depth base"
    );
}

#[test]
fn absent_logo_falls_back_to_text_name() {
    let dir = tmp_dir("logo-absent");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: PlainSite\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());
    let html = read(&out.join("index.html"));
    assert_contains(&html, r#"class="site-bar-name""#);
    assert_contains(&html, ">PlainSite</a>");
    // No <a class="site-bar-brand"> anchor and no <img class="site-bar-logo">
    // tag should appear in the body markup. The class names themselves live
    // in the inlined stylesheet for every page, so we assert on the full
    // opening tag pattern instead of a bare substring.
    assert!(
        !html.contains(r#"<a class="site-bar-brand""#),
        "absent logo should not emit the brand <a> wrapper"
    );
    assert!(
        !html.contains(r#"<img class="site-bar-logo""#),
        "absent logo should not emit any <img class=site-bar-logo>"
    );
}

/// Build `dir` with a fixed `KAZAM_TODAY`, returning (stdout, rendered HTML).
fn build_with_today(dir: &Path, today: &str, out: &Path) -> (String, String) {
    let output = Command::new(bin())
        .args(["build"])
        .arg(dir)
        .arg("--out")
        .arg(out)
        .env("KAZAM_TODAY", today)
        .output()
        .expect("run kazam build");
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let html = read(&out.join("index.html"));
    (stdout, html)
}

#[test]
fn freshness_overdue_injects_red_banner_and_reports_stale() {
    let dir = tmp_dir("fresh-overdue");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Docs\ntheme: dark\n").unwrap();
    // Updated Jan 1, reviewed every 30 days. On Apr 21 that's 110 days
    // later → 80 days overdue → red banner.
    std::fs::write(
        dir.join("index.yaml"),
        "title: Overdue page\nshell: standard\nfreshness:\n  updated: '2026-01-01'\n  review_every: 30d\n  owner: owner@example.com\n  sources_of_truth:\n    - https://notion.so/abc\n    - label: '#ts-hub'\n      href: https://slack.com/archives/C01\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let (stdout, html) = build_with_today(&dir, "2026-04-21", &out);

    assert_contains(
        &html,
        r#"<div class="c-callout c-callout-danger c-freshness-banner""#,
    );
    assert_contains(&html, "Review overdue");
    assert_contains(&html, "owner@example.com");
    // sources_of_truth list renders
    assert_contains(&html, r#"href="https://notion.so/abc""#);
    assert_contains(&html, r#"href="https://slack.com/archives/C01""#);
    assert_contains(&html, "#ts-hub");

    // Build report surfaces the overdue page.
    assert_contains(&stdout, "overdue page(s)");
    assert_contains(&stdout, "index.html");
    assert_contains(&stdout, "owner@example.com");
}

#[test]
fn freshness_due_soon_injects_yellow_banner() {
    let dir = tmp_dir("fresh-due-soon");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Docs\ntheme: dark\n").unwrap();
    // Updated Jan 23, reviewed every 90 days → due Apr 23. Today is Apr
    // 21 → 2 days until due → yellow banner.
    std::fs::write(
        dir.join("index.yaml"),
        "title: Due soon\nshell: standard\nfreshness:\n  updated: '2026-01-23'\n  review_every: 90d\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let (stdout, html) = build_with_today(&dir, "2026-04-21", &out);

    assert_contains(
        &html,
        r#"<div class="c-callout c-callout-warn c-freshness-banner""#,
    );
    assert_contains(&html, "Review due soon");
    assert!(
        !html.contains(r#"<div class="c-callout c-callout-danger c-freshness-banner""#),
        "due-soon should emit the yellow warn banner, not the red danger one"
    );

    // Build report shows the due-soon page, not the overdue section.
    assert_contains(&stdout, "due for review soon");
    assert!(
        !stdout.contains("overdue page(s)"),
        "no overdue pages expected here"
    );
}

#[test]
fn freshness_fresh_page_has_no_banner_and_report_stays_silent() {
    let dir = tmp_dir("fresh-fresh");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Docs\ntheme: dark\n").unwrap();
    // Updated today, 90-day cadence → no banner, no report line.
    std::fs::write(
        dir.join("index.yaml"),
        "title: Fresh\nshell: standard\nfreshness:\n  updated: '2026-04-21'\n  review_every: 90d\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let (stdout, html) = build_with_today(&dir, "2026-04-21", &out);

    // The `.c-freshness-banner` CSS class is inlined in every page's
    // stylesheet; match the full banner opening tag instead.
    assert!(
        !html.contains(r#"<div class="c-callout c-callout-warn c-freshness-banner""#)
            && !html.contains(r#"<div class="c-callout c-callout-danger c-freshness-banner""#),
        "fresh page should not emit a banner div"
    );
    assert!(!stdout.contains("overdue page(s)"));
    assert!(!stdout.contains("due for review soon"));
}

#[test]
fn freshness_writes_stale_md_for_overdue_and_removes_when_clean() {
    // Overdue run → _site/stale.md exists with the overdue details.
    let dir = tmp_dir("fresh-stalemd");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Docs\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Overdue\nshell: standard\nfreshness:\n  updated: '2026-01-01'\n  review_every: 30d\n  owner: docs@example.com\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    let out = dir.join("_site");
    build_with_today(&dir, "2026-04-21", &out);

    let stale_md = out.join("stale.md");
    assert!(
        stale_md.exists(),
        "stale.md should be written for overdue pages"
    );
    let content = read(&stale_md);
    assert_contains(&content, "# Stale page report");
    assert_contains(&content, "## Overdue");
    assert_contains(&content, "index.html");
    assert_contains(&content, "docs@example.com");

    // Now rewrite the page to have a fresh updated date and rebuild into
    // the same output dir. stale.md should be deleted so dirty state from
    // a previous build never leaks into a healthy one.
    std::fs::write(
        dir.join("index.yaml"),
        "title: Fresh\nshell: standard\nfreshness:\n  updated: '2026-04-21'\n  review_every: 30d\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    build_with_today(&dir, "2026-04-21", &out);
    assert!(
        !stale_md.exists(),
        "stale.md must be removed when nothing is stale"
    );
}

#[test]
fn freshness_page_without_metadata_is_silent() {
    // No `freshness:` block at all → no banner, no report entry, exactly
    // as a pre-feature page would render.
    let dir = tmp_dir("fresh-none");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Docs\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Plain\nshell: standard\ncomponents:\n  - type: header\n    title: Plain\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let (stdout, html) = build_with_today(&dir, "2026-04-21", &out);

    assert!(!html.contains(r#"<div class="c-callout c-callout-warn c-freshness-banner""#));
    assert!(!html.contains(r#"<div class="c-callout c-callout-danger c-freshness-banner""#));
    assert!(!stdout.contains("overdue"));
    assert!(!stdout.contains("due for review"));
}

#[test]
fn wish_init_rejects_duplicate_without_force() {
    let dir = tmp_dir("wish-dup-reject");
    std::fs::create_dir_all(dir.join("wishes/hubspot-icp")).unwrap();
    std::fs::write(
        dir.join("wishes/hubspot-icp/wish.yaml"),
        "name: hubspot-icp\ndescription: Already here\ntags: []\n",
    )
    .unwrap();

    let output = Command::new(bin())
        .args(["wish", "init", "hubspot-icp"])
        .current_dir(&dir)
        .output()
        .expect("run kazam wish init hubspot-icp (dup)");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_contains(&stderr, "already exists");
}

#[test]
fn hrefs_honor_verbatim_prefix_rule() {
    // Page at tsp/demo.yaml is depth-1 so base = "../".
    //
    // - Site-root paths (leading `/`) get the depth base prepended so the
    //   link still resolves under subpath deployments.
    // - `../‑relative`, hash, mailto, and `https://` hrefs pass through
    //   verbatim — they're already explicit.
    // - Bare names are page-relative — the browser resolves them against
    //   the current page, so the renderer leaves them alone.
    let dir = tmp_dir("href-verbatim");
    std::fs::create_dir_all(dir.join("tsp")).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: HrefTest\ntheme: dark\n").unwrap();

    let page = r##"title: Demo
shell: standard
components:
  - type: button_group
    buttons:
      - label: Site Root
        href: /customers/demo.html
      - label: Already Canonical
        href: ../customers/demo.html
      - label: Hash
        href: "#section"
      - label: Mail
        href: mailto:hi@example.com
      - label: External
        href: https://example.com
  - type: card_grid
    cards:
      - title: Card
        href: /abs-card.html
        links:
          - label: Link
            href: /abs-link.html
  - type: breadcrumb
    items:
      - label: Home
        href: /abs-crumb.html
      - label: Current
  - type: empty_state
    title: Nothing here
    action:
      label: Go
      href: /abs-action.html
  - type: markdown
    body: "[click](/abs-md.html) and [rel](relative.html)"
"##;
    std::fs::write(dir.join("tsp/demo.yaml"), page).unwrap();

    let out = tmp_dir("href-verbatim-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "build failed");

    let html = read(&out.join("tsp/demo.html"));

    // Leading-`/` site-root paths get the depth base prepended.
    assert_contains(&html, r#"href="../customers/demo.html""#);
    assert_contains(&html, r#"href="../abs-card.html""#);
    assert_contains(&html, r#"href="../abs-link.html""#);
    assert_contains(&html, r#"href="../abs-crumb.html""#);
    assert_contains(&html, r#"href="../abs-action.html""#);
    assert_contains(&html, r#"href="../abs-md.html""#);
    // The button labelled "Already Canonical" uses `../customers/demo.html`
    // which also resolves to `../customers/demo.html` — same target as the
    // site-root form above, so we verify the absence of any unrewritten
    // `/customers/demo.html` slash-prefixed survivor in the output.
    assert!(
        !html.contains(r#"href="/customers/demo.html""#),
        "site-root paths should be rewritten with depth base, not emitted verbatim"
    );
    // Hash, mailto, https pass through verbatim.
    assert_contains(&html, "href=\"#section\"");
    assert_contains(&html, r#"href="mailto:hi@example.com""#);
    assert_contains(&html, r#"href="https://example.com""#);
    // Bare relative href in markdown is page-relative — passes through
    // unchanged for the browser to resolve.
    assert_contains(&html, r#"href="relative.html""#);
    assert!(
        !html.contains(r#"href="../relative.html""#),
        "bare names should not be rewritten as site-root"
    );
}

#[test]
fn deck_print_flow_square_emits_print_square_class_and_page() {
    // `print_flow: square` is the LinkedIn-carousel-friendly mode — one
    // 8.5×8.5in page per slide, content centered, no letterbox. Verify the
    // body class and the @page rule both land in the rendered output.
    let dir = tmp_dir("deck-square");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Sq\ntheme: dark\n").unwrap();
    let page = r##"title: Square Demo
shell: deck
print_flow: square
slides:
  - label: Cover
    components:
      - type: header
        title: Square Print
"##;
    std::fs::write(dir.join("index.yaml"), page).unwrap();

    let out = tmp_dir("deck-square-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "build failed");

    let html = read(&out.join("index.html"));
    assert_contains(&html, "print-square");
    assert_contains(&html, "@page deck-page-square");
    assert_contains(&html, "size: 8.5in 8.5in");
    // The transform-reset that lets vertical centering actually work in
    // print mode should always be present on the deck shell.
    assert_contains(&html, "transform: none !important");
}

// ── Link report ──────────────────────────────────────────────────────

fn plain_build(dir: &Path, out: &Path) -> String {
    let output = Command::new(bin())
        .args(["build"])
        .arg(dir)
        .arg("--out")
        .arg(out)
        .output()
        .expect("run kazam build");
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn links_flags_orphan_page_and_writes_report() {
    // index.yaml links to /guide.html. `draft.yaml` is built but nothing
    // links to it — it should surface as an orphan in stdout and in
    // _site/links.md, but not block the build.
    let dir = tmp_dir("links-orphan");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: T\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: callout\n    body: Go read the guide.\n    links:\n      - label: Guide\n        href: guide.html\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("guide.yaml"),
        "title: Guide\nshell: standard\ncomponents:\n  - type: header\n    title: Guide\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("draft.yaml"),
        "title: Draft\nshell: standard\ncomponents:\n  - type: header\n    title: Draft\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let stdout = plain_build(&dir, &out);

    assert_contains(&stdout, "1 orphan page(s)");
    assert_contains(&stdout, "draft.html");

    let links_md = read(&out.join("links.md"));
    assert_contains(&links_md, "## Orphan pages (1)");
    assert_contains(&links_md, "draft.html");
}

#[test]
fn links_unlisted_pages_excluded_from_orphans() {
    // A page with `unlisted: true` is an explicit opt-out. Skipping llms.txt
    // should also mean skipping the orphan check — the author knows it's
    // not meant to be navigable.
    let dir = tmp_dir("links-unlisted");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: T\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("hidden.yaml"),
        "title: Hidden\nshell: standard\nunlisted: true\ncomponents:\n  - type: header\n    title: Hidden\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let stdout = plain_build(&dir, &out);

    assert!(
        !stdout.contains("orphan page(s)"),
        "unlisted page must not be flagged"
    );
    assert!(!out.join("links.md").exists());
}

#[test]
fn links_reports_broken_internal_href() {
    // A callout links to `missing.html` that doesn't exist. Must be reported
    // as a broken link; non-.html and external hrefs are ignored.
    let dir = tmp_dir("links-broken");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: T\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: callout\n    body: see missing\n    links:\n      - label: Missing\n        href: missing.html\n      - label: External\n        href: https://example.com\n      - label: Asset\n        href: /favicon.svg\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let stdout = plain_build(&dir, &out);

    assert_contains(&stdout, "broken internal link(s)");
    assert_contains(&stdout, "missing.html");
    assert!(!stdout.contains("example.com"), "externals must be skipped");
    assert!(!stdout.contains("favicon.svg"), "assets must be skipped");

    let links_md = read(&out.join("links.md"));
    assert_contains(&links_md, "## Broken internal links");
    assert_contains(&links_md, "missing.html");
}

#[test]
fn links_silent_on_clean_build_removes_stale_report() {
    // Seed a build with an orphan so links.md exists, then remove the
    // orphan and rebuild into the same output dir — links.md must be
    // deleted so a clean build never carries stale state forward.
    let dir = tmp_dir("links-clean");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: T\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("stray.yaml"),
        "title: Stray\nshell: standard\ncomponents:\n  - type: header\n    title: Stray\n",
    )
    .unwrap();
    let out = dir.join("_site");
    plain_build(&dir, &out);
    assert!(
        out.join("links.md").exists(),
        "orphan should produce links.md"
    );

    std::fs::remove_file(dir.join("stray.yaml")).unwrap();
    let stdout = plain_build(&dir, &out);
    assert!(!stdout.contains("orphan page(s)"));
    assert!(
        !out.join("links.md").exists(),
        "links.md must be removed on a clean build"
    );
}

#[test]
fn links_allow_orphans_flag_suppresses_orphans_but_not_broken() {
    // --allow-orphans silences orphan detection entirely but still surfaces
    // broken internal links, which are never legitimate.
    let dir = tmp_dir("links-allow-orphans");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: T\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: callout\n    body: broken\n    links:\n      - label: Missing\n        href: missing.html\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("orphan.yaml"),
        "title: Orphan\nshell: standard\ncomponents:\n  - type: header\n    title: Orphan\n",
    )
    .unwrap();
    let out = dir.join("_site");

    let output = Command::new(bin())
        .args(["build", "--allow-orphans"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .output()
        .expect("run kazam build --allow-orphans");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(!stdout.contains("orphan page(s)"));
    assert_contains(&stdout, "broken internal link(s)");
    assert_contains(&stdout, "missing.html");
}

// ── Anchor ids on section / header ──────────────────────────────────

fn build_one_page(name: &str, page_yaml: &str, extra_config: &str) -> String {
    let dir = tmp_dir(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kazam.yaml"),
        format!("name: T\ntheme: dark\n{extra_config}"),
    )
    .unwrap();
    std::fs::write(dir.join("index.yaml"), page_yaml).unwrap();
    let out = dir.join("_site");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());
    read(&out.join("index.html"))
}

#[test]
fn section_auto_slugs_id_from_heading() {
    let html = build_one_page(
        "anchor-auto",
        "title: Home\nshell: standard\ncomponents:\n  - type: section\n    heading: Success outcomes\n    components: []\n",
        "",
    );
    assert_contains(&html, r#"<section id="success-outcomes""#);
}

#[test]
fn section_explicit_id_overrides_heading_slug() {
    // Author locks `id: outcomes` — the stable anchor must win over the
    // auto-slug from the heading text, so deep-links survive copy edits.
    let html = build_one_page(
        "anchor-explicit",
        "title: Home\nshell: standard\ncomponents:\n  - type: section\n    heading: Success outcomes\n    id: outcomes\n    components: []\n",
        "",
    );
    assert_contains(&html, r#"<section id="outcomes""#);
    assert!(
        !html.contains(r#"id="success-outcomes""#),
        "auto-slug must not duplicate when an explicit id is set"
    );
}

#[test]
fn header_auto_slugs_id_from_title() {
    let html = build_one_page(
        "anchor-header",
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Platform Health\n",
        "",
    );
    assert_contains(&html, r#"<div id="platform-health" class="c-header"#);
}

#[test]
fn section_without_heading_or_id_emits_no_id() {
    // A bare section (no heading, no explicit id) should stay anchor-less
    // so snapshots of pre-feature sites don't shift.
    let html = build_one_page(
        "anchor-none",
        "title: Home\nshell: standard\ncomponents:\n  - type: section\n    eyebrow: Quiet\n    components:\n      - type: markdown\n        body: body\n",
        "",
    );
    assert!(
        !html.contains("<section id="),
        "section without heading/id must not emit an id attribute"
    );
}

#[test]
fn colliding_headings_get_suffixed_ids() {
    // Two sections with the same heading on the same page must dedupe —
    // first wins `outcomes`, second becomes `outcomes-2`, third `outcomes-3`.
    let html = build_one_page(
        "anchor-collide",
        "title: Home\nshell: standard\ncomponents:\n  - type: section\n    heading: Outcomes\n    components: []\n  - type: section\n    heading: Outcomes\n    components: []\n  - type: section\n    heading: Outcomes\n    components: []\n",
        "",
    );
    assert_contains(&html, r#"<section id="outcomes""#);
    assert_contains(&html, r#"<section id="outcomes-2""#);
    assert_contains(&html, r#"<section id="outcomes-3""#);
}

#[test]
fn emoji_and_punctuation_stripped_from_slug() {
    let html = build_one_page(
        "anchor-emoji",
        "title: Home\nshell: standard\ncomponents:\n  - type: section\n    heading: \"⚡ Move at Machine Speed\"\n    components: []\n",
        "",
    );
    assert_contains(&html, r#"<section id="move-at-machine-speed""#);
}

#[test]
fn scroll_margin_top_css_clears_sticky_site_bar() {
    // The CSS rule that makes #deep-link jumps clear the sticky bar must
    // land in the generated stylesheet for shell-standard / shell-document.
    let html = build_one_page(
        "anchor-scroll",
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
        "",
    );
    assert_contains(&html, "body.shell-standard [id]");
    assert_contains(&html, "scroll-margin-top");
}

#[test]
fn slug_counter_resets_between_pages() {
    // The dedup tracker is per-page: page A having `outcomes` must not
    // push page B's `outcomes` to `outcomes-2`.
    let dir = tmp_dir("anchor-reset");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: T\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: section\n    heading: Outcomes\n    components: []\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("other.yaml"),
        "title: Other\nshell: standard\ncomponents:\n  - type: section\n    heading: Outcomes\n    components: []\n",
    )
    .unwrap();
    let out = dir.join("_site");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());

    let index = read(&out.join("index.html"));
    let other = read(&out.join("other.html"));
    assert_contains(&index, r#"id="outcomes""#);
    assert_contains(&other, r#"id="outcomes""#);
    assert!(!index.contains(r#"id="outcomes-2""#));
    assert!(!other.contains(r#"id="outcomes-2""#));
}

#[test]
fn init_refuses_existing_dir() {
    let dir = tmp_dir("init-exists");
    std::fs::create_dir_all(&dir).unwrap();

    let status = Command::new(bin())
        .args(["init"])
        .arg(&dir)
        .status()
        .expect("run kazam init");
    assert!(!status.success(), "init should fail on existing dir");
}

#[test]
fn event_timeline_renders_with_filter_toggle() {
    let dir = tmp_dir("event-timeline");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: event_timeline
    default_filter: major
    show_filter_toggle: true
    events:
      - date: 2026-04-27
        severity: major
        title: Weekly sync
        summary: |
          Working session booked.
        source: granola
        link: https://example.com/notes
      - date: 2026-04-26
        severity: minor
        title: ANSYS-322 done
        source: linear
      - date: 2026-04-25
        severity: info
        title: Cadence moved to Thursdays
"#,
    )
    .unwrap();

    let out = tmp_dir("event-timeline-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("index.html"));
    // Container + default filter class
    assert_contains(&html, r#"class="c-event-timeline filter-major""#);
    // Filter toggle markup + active button
    assert_contains(&html, r#"data-event-filter-toggle"#);
    assert_contains(&html, r#"data-filter="major""#);
    assert_contains(&html, r#"data-filter="all""#);
    // Severity classes per event
    assert_contains(&html, r#"class="c-event severity-major""#);
    assert_contains(&html, r#"class="c-event severity-minor""#);
    assert_contains(&html, r#"class="c-event severity-info""#);
    // Severity data attributes drive the CSS filter
    assert_contains(&html, r#"data-severity="major""#);
    assert_contains(&html, r#"data-severity="minor""#);
    // Event with summary collapses into <details>
    assert_contains(&html, r#"<details class="c-event-details">"#);
    // Event without summary stays as plain title div
    assert_contains(&html, r#"ANSYS-322 done"#);
    // Source chip + external link
    assert_contains(&html, r#"class="c-event-source""#);
    assert_contains(&html, r#"href="https://example.com/notes""#);
    // Filter toggle JS got registered
    assert_contains(&html, "data-event-filter-toggle");
    assert_contains(&html, "filter-major");
}

#[test]
fn event_timeline_without_toggle_skips_script() {
    let dir = tmp_dir("event-timeline-no-toggle");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: event_timeline
    events:
      - date: 2026-04-27
        title: A thing happened
"#,
    )
    .unwrap();

    let out = tmp_dir("event-timeline-no-toggle-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());

    let html = read(&out.join("index.html"));
    // Default filter = all; no toggle markup
    assert_contains(&html, r#"class="c-event-timeline filter-all""#);
    assert!(
        !html.contains("data-event-filter-toggle"),
        "toggle should be absent when show_filter_toggle is false"
    );
    // Default severity is minor when omitted
    assert_contains(&html, r#"data-severity="minor""#);
}

#[test]
fn tree_renders_nested_status_with_branch_lines() {
    let dir = tmp_dir("tree");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: tree
    nodes:
      - label: "Phase 1"
        status: completed
        children:
          - label: Identify stakeholders
            status: completed
          - label: Deploy stack
            status: blocked
            note: "Waiting on change-window"
      - label: "Phase 2"
        status: active
        children:
          - label: Generate External ID
            status: upcoming
"#,
    )
    .unwrap();

    let out = tmp_dir("tree-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("index.html"));
    // Container (default filter renders as a class) + nested ul classes
    assert_contains(&html, r#"class="c-tree filter-all""#);
    assert_contains(&html, r#"data-filter="all""#);
    assert_contains(&html, r#"class="c-tree-root""#);
    assert_contains(&html, r#"class="c-tree-children""#);
    // Status classes per node
    assert_contains(&html, r#"c-tree-node status-completed"#);
    assert_contains(&html, r#"c-tree-node status-blocked"#);
    assert_contains(&html, r#"c-tree-node status-active"#);
    assert_contains(&html, r#"c-tree-node status-upcoming"#);
    // data-status for downstream styling/inspection
    assert_contains(&html, r#"data-status="completed""#);
    assert_contains(&html, r#"data-status="blocked""#);
    // Glyphs land
    assert_contains(&html, r#"✓"#);
    assert_contains(&html, r#"⚠"#);
    // Note renders on the blocked node
    assert_contains(&html, r#"class="c-tree-note""#);
    assert_contains(&html, "Waiting on change-window");
}

#[test]
fn tree_filter_toggle_marks_blocked_ancestors() {
    let dir = tmp_dir("tree-filter");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: tree
    default_filter: blocked
    show_filter_toggle: true
    nodes:
      - label: "Phase 1"
        status: completed
      - label: "Phase 2"
        status: active
        children:
          - label: "Sub A"
            status: active
            children:
              - label: "Leaf 1"
                status: blocked
              - label: "Leaf 2"
                status: completed
          - label: "Sub B"
            status: active
"#,
    )
    .unwrap();

    let out = tmp_dir("tree-filter-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success());

    let html = read(&out.join("index.html"));
    // Default-filter class + toggle markup
    assert_contains(&html, r#"class="c-tree filter-blocked""#);
    assert_contains(&html, r#"data-tree-filter-toggle"#);
    assert_contains(&html, r#"data-filter="all""#);
    assert_contains(&html, r#"data-filter="incomplete""#);
    assert_contains(&html, r#"data-filter="blocked""#);
    // Phase 2 + Sub A both have a blocked descendant — both must be marked
    // so the filter-blocked CSS keeps the path-to-root visible.
    let blocked_anc_count = html
        .matches(r#"data-has-blocked-descendant="true""#)
        .count();
    assert!(
        blocked_anc_count >= 2,
        "expected ≥2 ancestors marked, got {}",
        blocked_anc_count
    );
    // The blocked node itself must NOT carry the descendant attr — only ancestors.
    assert!(
        html.contains(r#"class="c-tree-node status-blocked""#)
            && !html.contains(
                r#"class="c-tree-node status-blocked" data-status="blocked" data-leaf="true" data-has-blocked-descendant"#
            ),
        "blocked leaf shouldn't be flagged as a blocked-ancestor"
    );
    // Leaves get data-leaf so the incomplete-filter CSS can target them.
    assert_contains(&html, r#"data-leaf="true""#);
}

#[test]
fn venn_two_set_renders_circles_and_overlap_label() {
    let dir = tmp_dir("venn-2");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: venn
    title: "Two-set"
    sets:
      - label: Frontend
        color: teal
      - label: Backend
        color: red
    overlaps:
      - sets: [0, 1]
        label: APIs
"#,
    )
    .unwrap();

    let out = tmp_dir("venn-2-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("index.html"));
    // SVG container + two themed circles
    assert_contains(&html, r#"class="c-venn""#);
    assert_contains(&html, r#"<svg class="c-venn-svg""#);
    assert_contains(&html, r#"c-venn-circle c-venn-circle-teal"#);
    assert_contains(&html, r#"c-venn-circle c-venn-circle-red"#);
    // Set labels
    assert_contains(&html, "Frontend");
    assert_contains(&html, "Backend");
    // Overlap label
    assert_contains(&html, r#"class="c-venn-overlap-label""#);
    assert_contains(&html, ">APIs</text>");
    // Title
    assert_contains(&html, r#"class="c-venn-title""#);
}

#[test]
fn venn_three_set_places_three_circles() {
    let dir = tmp_dir("venn-3");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: venn
    sets:
      - label: A
      - label: B
      - label: C
    overlaps:
      - sets: [0, 1, 2]
        label: All three
"#,
    )
    .unwrap();

    let out = tmp_dir("venn-3-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("index.html"));
    // Three circles
    let circle_count = html.matches(r#"<circle class="c-venn-circle"#).count();
    assert_eq!(
        circle_count, 3,
        "expected 3 venn circles, found {}",
        circle_count
    );
    // 3-way overlap label centered at centroid
    assert_contains(&html, ">All three</text>");
}

// ── 404 page ──────────────────────────────────────────

#[test]
fn build_generates_default_404_page() {
    let dir = tmp_dir("404-default");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();

    let out = tmp_dir("404-default-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let not_found = out.join("404.html");
    assert!(not_found.exists(), "404.html should be generated");

    let html = read(&not_found);
    // Uses the site's theme
    assert_contains(&html, "shell-standard");
    // Shows the site name in the site bar
    assert_contains(&html, r#"class="site-bar-name""#);
    assert_contains(&html, ">Test</a>");
    // Has the "not found" empty state
    assert_contains(&html, "c-empty-state");
    assert_contains(&html, "Page not found");
    // Home link is absolute (root-relative) so it works from any URL
    assert_contains(&html, r#"href="/index.html""#);
    // "Go home" button
    assert_contains(&html, "Go home");
}

#[test]
fn build_404_page_with_site_url_uses_absolute_urls() {
    let dir = tmp_dir("404-url");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kazam.yaml"),
        "name: UrlTest\ntheme: dark\nurl: https://example.com/kazam\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();

    let out = tmp_dir("404-url-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("404.html"));
    // When site URL is configured, the 404 page uses full absolute URLs
    assert_contains(&html, r#"href="https://example.com/kazam/index.html""#);
}

#[test]
fn build_404_yaml_customizes_404_page() {
    let dir = tmp_dir("404-custom");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Custom404\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("404.yaml"),
        r#"title: Oops
shell: standard
components:
  - type: callout
    variant: danger
    title: Something went wrong
    body: We couldn't find that page.
  - type: button_group
    buttons:
      - label: Back to safety
        href: /index.html
        variant: primary
"#,
    )
    .unwrap();

    let out = tmp_dir("404-custom-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("404.html"));
    // Custom content from 404.yaml is rendered
    assert_contains(&html, "Something went wrong");
    assert_contains(&html, "Back to safety");
    // Internal links are absolute (root-relative) so they work from any URL
    assert_contains(&html, r#"href="/index.html""#);
}

#[test]
fn build_404_page_not_listed_in_llms_txt() {
    let dir = tmp_dir("404-llms");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();

    let out = tmp_dir("404-llms-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let llms = read(&out.join("llms.txt"));
    assert!(
        !llms.contains("404"),
        "404 page should not appear in llms.txt"
    );
}

#[test]
fn build_404_page_not_flagged_as_orphan() {
    let dir = tmp_dir("404-orphan");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();

    let out = tmp_dir("404-orphan-out");
    let output = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .output()
        .expect("run kazam build");
    assert!(output.status.success(), "kazam build failed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("orphan page(s)"),
        "404 page should not be flagged as orphan"
    );
}

#[test]
fn build_404_page_with_nav_uses_absolute_links() {
    let dir = tmp_dir("404-nav");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("kazam.yaml"),
        "name: Nav404\ntheme: dark\nnav:\n  - label: Home\n    href: /index.html\n  - label: Guide\n    href: /guide.html\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Home\n",
    )
    .unwrap();

    let out = tmp_dir("404-nav-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("404.html"));
    // Nav links in the 404 page are root-relative so they work from any URL
    assert_contains(&html, r#"href="/index.html""#);
    assert_contains(&html, r#"href="/guide.html""#);
    // Site bar brand link is also absolute
    assert_contains(&html, r#"class="site-bar-name" href="/index.html""#);
}

// ── Event timeline: limit + filter-at-render ────────

#[test]
fn event_timeline_limit_renders_all_events() {
    let dir = tmp_dir("timeline-limit");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: event_timeline
    limit: 2
    events:
      - date: 2026-04-27
        severity: major
        title: First
      - date: 2026-04-26
        severity: minor
        title: Second
      - date: 2026-04-25
        severity: major
        title: Third
      - date: 2026-04-24
        severity: info
        title: Fourth
"#,
    )
    .unwrap();

    let out = tmp_dir("timeline-limit-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("index.html"));
    assert_contains(&html, "First");
    assert_contains(&html, "Second");
    assert_contains(&html, "Third");
    assert_contains(&html, "Fourth");
}

#[test]
fn event_timeline_without_toggle_filters_at_build_time() {
    let dir = tmp_dir("timeline-filter-render");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: event_timeline
    default_filter: major
    show_filter_toggle: false
    events:
      - date: 2026-04-27
        severity: major
        title: Major event
      - date: 2026-04-26
        severity: minor
        title: Minor event
      - date: 2026-04-25
        severity: info
        title: Info event
"#,
    )
    .unwrap();

    let out = tmp_dir("timeline-filter-render-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("index.html"));
    // Major event should be rendered
    assert_contains(&html, "Major event");
    // Minor/info events should NOT be in the DOM at all (no toggle = build-time filter)
    assert!(
        !html.contains("Minor event"),
        "non-major events should not render when toggle is hidden"
    );
    assert!(
        !html.contains("Info event"),
        "non-major events should not render when toggle is hidden"
    );
}

#[test]
fn event_timeline_with_toggle_renders_all_events() {
    let dir = tmp_dir("timeline-toggle-all");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: event_timeline
    default_filter: major
    show_filter_toggle: true
    events:
      - date: 2026-04-27
        severity: major
        title: Major event
      - date: 2026-04-26
        severity: minor
        title: Minor event
"#,
    )
    .unwrap();

    let out = tmp_dir("timeline-toggle-all-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("index.html"));
    // When toggle is shown, ALL events must be in the DOM for JS switching
    assert_contains(&html, "Major event");
    assert_contains(&html, "Minor event");
}

// ── Tree: priority status + filter-at-render ────────

#[test]
fn tree_priority_status_renders() {
    let dir = tmp_dir("tree-priority");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: tree
    nodes:
      - label: "Phase 1"
        status: completed
      - label: "Critical path item"
        status: priority
        note: "Must ship before Q3"
      - label: "Phase 2"
        status: active
"#,
    )
    .unwrap();

    let out = tmp_dir("tree-priority-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("index.html"));
    // Priority status class + data attribute
    assert_contains(&html, r#"c-tree-node status-priority"#);
    assert_contains(&html, r#"data-status="priority""#);
    // Star glyph
    assert_contains(&html, "★");
    // Note renders with priority emphasis
    assert_contains(&html, r#"class="c-tree-note""#);
    assert_contains(&html, "Must ship before Q3");
}

#[test]
fn tree_priority_filter_shows_priority_and_ancestors() {
    let dir = tmp_dir("tree-priority-filter");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: tree
    show_filter_toggle: true
    default_filter: priority
    nodes:
      - label: "Phase 1"
        status: completed
      - label: "Phase 2"
        status: active
        children:
          - label: "Key deliverable"
            status: priority
          - label: "Nice-to-have"
            status: upcoming
"#,
    )
    .unwrap();

    let out = tmp_dir("tree-priority-filter-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("index.html"));
    // Priority filter class + toggle
    assert_contains(&html, r#"class="c-tree filter-priority""#);
    assert_contains(&html, r#"data-tree-filter-toggle"#);
    assert_contains(&html, r#"data-filter="priority""#);
    // Ancestor of priority node is marked
    assert_contains(&html, r#"data-has-priority-descendant="true""#);
    // Priority node itself has status
    assert_contains(&html, r#"c-tree-node status-priority"#);
}

#[test]
fn tree_without_toggle_prunes_at_build_time() {
    let dir = tmp_dir("tree-prune");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        r#"title: Test
shell: standard
components:
  - type: tree
    default_filter: blocked
    show_filter_toggle: false
    nodes:
      - label: "Phase 1"
        status: completed
        children:
          - label: "Done item"
            status: completed
      - label: "Phase 2"
        status: active
        children:
          - label: "Stuck item"
            status: blocked
          - label: "Moving item"
            status: active
"#,
    )
    .unwrap();

    let out = tmp_dir("tree-prune-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");
    assert!(status.success(), "kazam build failed");

    let html = read(&out.join("index.html"));
    // Blocked item and its parent (Phase 2) should be rendered
    assert_contains(&html, "Stuck item");
    assert_contains(&html, "Phase 2");
    // Phase 1 and its completed children should NOT be rendered as tree nodes
    // (check for the label inside a tree-node, not in the CSS comment)
    assert!(
        !html.contains(r#"><span class="c-tree-label">Phase 1</span>"#),
        "unrelated branch should be pruned at build time"
    );
    assert!(
        !html.contains(r#"><span class="c-tree-label">Done item</span>"#),
        "completed items should be pruned at build time"
    );
    // "Moving item" is under Phase 2 but is not blocked — it should also be pruned
    assert!(
        !html.contains(r#"><span class="c-tree-label">Moving item</span>"#),
        "non-blocked sibling should be pruned at build time"
    );
}

// ── kazam validate ───────────────────────────────────

#[test]
fn validate_kb_example_succeeds() {
    // The bundled kb example is a well-formed site; `kazam validate` must
    // exit 0 and output a JSON empty array.
    let src = repo_root().join("examples/kb");
    let output = Command::new(bin())
        .args(["validate"])
        .arg(&src)
        .output()
        .expect("run kazam validate");
    assert!(
        output.status.success(),
        "kazam validate failed on examples/kb: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Default output is JSON.
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("validate output should be valid JSON");
    assert!(
        parsed.as_array().map(|a| a.is_empty()).unwrap_or(false),
        "expected empty JSON array, got: {}",
        stdout
    );
}

#[test]
fn validate_invalid_yaml_dir_fails_with_json_errors() {
    // Build a tiny dir with a page that violates structural rules.
    let dir = tmp_dir("validate-invalid");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();

    // Standard page with no components — structural error.
    std::fs::write(dir.join("bad.yaml"), "title: Bad\nshell: standard\n").unwrap();

    let output = Command::new(bin())
        .args(["validate"])
        .arg(&dir)
        .output()
        .expect("run kazam validate on invalid dir");

    assert!(
        !output.status.success(),
        "kazam validate should exit non-zero for invalid site"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let errors: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON even on failure");
    let arr = errors.as_array().expect("should be a JSON array");
    assert!(!arr.is_empty(), "expected at least one validation error");

    // Verify error shape: file, path, error_type, message all present.
    let first = &arr[0];
    assert!(first.get("file").is_some(), "error should have file field");
    assert!(
        first.get("error_type").is_some(),
        "error should have error_type field"
    );
    assert!(
        first.get("message").is_some(),
        "error should have message field"
    );
}

#[test]
fn validate_pretty_output_is_human_readable() {
    let dir = tmp_dir("validate-pretty");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(dir.join("page.yaml"), "title: Bad\nshell: standard\n").unwrap();

    let output = Command::new(bin())
        .args(["validate", "--pretty"])
        .arg(&dir)
        .output()
        .expect("run kazam validate --pretty");

    // --pretty goes to stderr.
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should contain the file name and a human-readable error marker.
    assert_contains(&stderr, "page.yaml");
    // Should NOT be raw JSON.
    assert!(
        !stderr.trim_start().starts_with('['),
        "--pretty output should not be a JSON array"
    );
}

#[test]
fn build_fails_on_structurally_invalid_yaml() {
    // A page with an empty card_grid (zero cards) should fail the build.
    let dir = tmp_dir("build-validate-fail");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("kazam.yaml"), "name: Test\ntheme: dark\n").unwrap();
    std::fs::write(
        dir.join("index.yaml"),
        "title: Bad\nshell: standard\ncomponents:\n  - type: card_grid\n    cards: []\n",
    )
    .unwrap();

    let out = tmp_dir("build-validate-fail-out");
    let status = Command::new(bin())
        .args(["build"])
        .arg(&dir)
        .arg("--out")
        .arg(&out)
        .status()
        .expect("run kazam build");

    assert!(
        !status.success(),
        "build should fail when a page has a validation error"
    );
}

// ── JSON output tests ──────────────────────────────────────────────────────

#[test]
fn build_json_output_is_valid_ndjson() {
    let out = tmp_dir("kb-json");
    let src = repo_root().join("examples/kb");
    let output = Command::new(bin())
        .args(["build"])
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run kazam build --json");
    assert!(output.status.success(), "kazam build --json failed");

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");
    // Every non-empty line must be valid JSON
    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }
        let val: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("invalid JSON line {:?}: {}", line, e));
        assert!(
            val.get("event").is_some(),
            "each event must have an 'event' field, got: {}",
            line
        );
    }
}

#[test]
fn build_json_first_event_is_build_start() {
    let out = tmp_dir("kb-json-start");
    let src = repo_root().join("examples/kb");
    let output = Command::new(bin())
        .args(["build"])
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run kazam build --json");
    assert!(output.status.success(), "kazam build --json failed");

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");
    let first_line = stdout.lines().next().expect("at least one output line");
    let val: serde_json::Value = serde_json::from_str(first_line).expect("valid JSON");
    assert_eq!(
        val["event"].as_str(),
        Some("build_start"),
        "first event must be build_start"
    );
    assert!(
        val.get("timestamp").is_some(),
        "build_start must have timestamp"
    );
}

#[test]
fn build_json_last_event_is_build_complete() {
    let out = tmp_dir("kb-json-complete");
    let src = repo_root().join("examples/kb");
    let output = Command::new(bin())
        .args(["build"])
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .arg("--json")
        .output()
        .expect("run kazam build --json");
    assert!(output.status.success(), "kazam build --json failed");

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");
    let last_line = stdout
        .lines()
        .rfind(|l| !l.is_empty())
        .expect("at least one output line");
    let val: serde_json::Value = serde_json::from_str(last_line).expect("valid JSON");
    assert_eq!(
        val["event"].as_str(),
        Some("build_complete"),
        "last event must be build_complete"
    );
    assert!(
        val["pages"].as_u64().is_some(),
        "build_complete must have 'pages' count"
    );
    assert!(
        val["duration_ms"].as_u64().is_some(),
        "build_complete must have 'duration_ms'"
    );
    assert!(
        val.get("timestamp").is_some(),
        "build_complete must have timestamp"
    );
}

#[test]
fn build_json_page_count_matches() {
    let out_json = tmp_dir("kb-json-count-json");
    let out_plain = tmp_dir("kb-json-count-plain");
    let src = repo_root().join("examples/kb");

    // Run with --json to get the build_complete count
    let json_output = Command::new(bin())
        .args(["build"])
        .arg(&src)
        .arg("--out")
        .arg(&out_json)
        .arg("--json")
        .output()
        .expect("run kazam build --json");
    assert!(json_output.status.success());

    let stdout = String::from_utf8(json_output.stdout).expect("stdout is utf-8");
    let last_line = stdout.lines().rfind(|l| !l.is_empty()).unwrap();
    let complete: serde_json::Value = serde_json::from_str(last_line).unwrap();
    let json_pages = complete["pages"].as_u64().expect("pages field");

    // Run without --json; count page_built events from file system
    let plain_status = Command::new(bin())
        .args(["build"])
        .arg(&src)
        .arg("--out")
        .arg(&out_plain)
        .status()
        .expect("run kazam build");
    assert!(plain_status.success());

    // Count .html files in output (excluding 404.html and .source.html) to verify page count
    let html_count = walkdir::WalkDir::new(&out_plain)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy();
            e.file_type().is_file()
                && name.ends_with(".html")
                && name != "404.html"
                && name != "_health.html"
                && !name.ends_with(".source.html")
        })
        .count() as u64;

    assert_eq!(
        json_pages, html_count,
        "build_complete pages count should match HTML files in output"
    );
}

#[test]
fn build_json_human_output_unchanged() {
    // When --json is NOT passed, stdout should contain human-readable markers,
    // not JSON lines.
    let out = tmp_dir("kb-no-json");
    let src = repo_root().join("examples/kb");
    let output = Command::new(bin())
        .args(["build"])
        .arg(&src)
        .arg("--out")
        .arg(&out)
        .output()
        .expect("run kazam build");
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");
    assert_contains(&stdout, "page(s)");
    // Must not look like NDJSON
    assert!(
        !stdout.trim_start().starts_with('{'),
        "human output should not start with JSON object"
    );
}

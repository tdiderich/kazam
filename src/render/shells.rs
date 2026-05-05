use super::{collect_scripts, components, esc, resolve_href, Rendered};
use crate::theme;
use crate::types::{Page, Shell, SiteConfig, Slide};

fn head(page: &Page, config: &SiteConfig, base: &str, rel_path: &str) -> String {
    let theme = config.resolved_theme();
    let favicon = match config.favicon.as_ref() {
        Some(f) => f.render(base),
        None => default_favicon(&theme),
    };
    // Page-level texture/glow overrides beat the site-wide defaults. An
    // explicit `none` at the page level turns the effect off on that page.
    let texture = page.texture.unwrap_or(config.texture);
    let glow = page.glow.unwrap_or(config.glow);
    let social = social_meta(page, config, base, rel_path);
    format!(
        r#"<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title} — {site}</title>
{social}{favicon}
<style>{css}</style>
</head>"#,
        title = esc(&page.title),
        site = esc(&config.name),
        social = social,
        favicon = favicon,
        css = theme::render_css(&theme, texture, glow),
    )
}

/// Render the SEO + Open Graph + Twitter card meta block. Uses the page's
/// subtitle as the description when present, falling back to the site-wide
/// `description:`. Canonical + og:url are only emitted when `url:` is set on
/// the site config. Social images are emitted when `og_image:` is set.
///
/// Unlisted pages (source views, drafts) get `robots: noindex` so we don't
/// leak internal working pages into search results.
fn social_meta(page: &Page, config: &SiteConfig, base: &str, rel_path: &str) -> String {
    let mut out = String::new();
    // Social titles use the page title on its own — og:site_name already
    // conveys the site, so duplicating it here produces ugly "Foo — Site — Site"
    // strings in unfurls.
    let title = page.title.as_str();
    let description = page
        .subtitle
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(config.description.as_deref())
        .unwrap_or("");

    if !description.is_empty() {
        out.push_str(&format!(
            r#"<meta name="description" content="{}">
"#,
            esc(description)
        ));
    }
    if page.unlisted {
        out.push_str(
            r#"<meta name="robots" content="noindex,nofollow">
"#,
        );
    }

    // Canonical + og:url require a site url. Without one, skip the URL-shaped
    // tags; the rest still unfurl reasonably.
    let canonical = config
        .url
        .as_deref()
        .map(|u| format!("{}/{}", u.trim_end_matches('/'), rel_path));

    if let Some(c) = &canonical {
        out.push_str(&format!(
            r#"<link rel="canonical" href="{}">
"#,
            esc(c)
        ));
    }

    // Open Graph
    out.push_str(&format!(
        r#"<meta property="og:type" content="website">
<meta property="og:site_name" content="{}">
<meta property="og:title" content="{}">
"#,
        esc(&config.name),
        esc(title),
    ));
    if !description.is_empty() {
        out.push_str(&format!(
            r#"<meta property="og:description" content="{}">
"#,
            esc(description)
        ));
    }
    if let Some(c) = &canonical {
        out.push_str(&format!(
            r#"<meta property="og:url" content="{}">
"#,
            esc(c)
        ));
    }
    if let Some(img) = config.og_image.as_deref() {
        let img_url = if img.starts_with("http://") || img.starts_with("https://") {
            img.to_string()
        } else if let Some(u) = config.url.as_deref() {
            format!(
                "{}/{}",
                u.trim_end_matches('/'),
                img.trim_start_matches('/')
            )
        } else {
            // Fall back to the base-relative path so the asset at least
            // resolves when the page is opened in a browser.
            resolve_href(img, base)
        };
        out.push_str(&format!(
            r#"<meta property="og:image" content="{}">
"#,
            esc(&img_url)
        ));
        // Twitter card uses summary_large_image when an image is present,
        // otherwise falls back to the basic summary card.
        out.push_str(
            r#"<meta name="twitter:card" content="summary_large_image">
"#,
        );
        out.push_str(&format!(
            r#"<meta name="twitter:image" content="{}">
"#,
            esc(&img_url)
        ));
    } else {
        out.push_str(
            r#"<meta name="twitter:card" content="summary">
"#,
        );
    }
    out.push_str(&format!(
        r#"<meta name="twitter:title" content="{}">
"#,
        esc(title)
    ));
    if !description.is_empty() {
        out.push_str(&format!(
            r#"<meta name="twitter:description" content="{}">
"#,
            esc(description)
        ));
    }
    out
}

/// When a site doesn't declare a `favicon:`, synthesize one from theme colors.
/// Produces the kazam genie-bottle mark as an inline data-URI SVG — accent on
/// bg. Stopper + narrow neck + bulbous body, sized for 32px and 16px alike.
fn default_favicon(theme: &theme::Theme) -> String {
    let svg = format!(
        r##"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'><rect width='32' height='32' rx='6' fill='{bg}'/><rect x='13' y='5' width='6' height='3' rx='1' fill='{accent}'/><path d='M 14 8 L 18 8 L 18 12 Q 23 13 23 19 Q 23 27 16 27 Q 9 27 9 19 Q 9 13 14 12 Z' fill='{accent}'/></svg>"##,
        bg = theme.bg,
        accent = theme.accent
    );
    let encoded = svg
        .replace('#', "%23")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace(' ', "%20");
    format!(r#"<link rel="icon" type="image/svg+xml" href="data:image/svg+xml;utf8,{encoded}">"#)
}

/// Top-bar nav (horizontal). Parent entries with `children:` render as a
/// hover/focus-within dropdown; leaf entries render as a plain link. Returns
/// `(html, has_any_nav)` so the caller can decide whether to bundle the
/// nav-related JS.
fn nav_html(links: Option<&Vec<crate::types::NavLink>>, base: &str) -> (String, bool) {
    let Some(links) = links else {
        return (String::new(), false);
    };
    if links.is_empty() {
        return (String::new(), false);
    }
    // Toggle button is hidden on desktop via CSS, visible on mobile. Clicking
    // flips `data-open` on the wrapping <nav>, which the stylesheet uses to
    // slide the link panel in. The toggle itself must live inside <nav> so
    // the closest('nav') lookup in the JS handler resolves.
    let mut out = String::from(r#"<nav aria-label="Main">"#);
    out.push_str(
        r#"<button type="button" class="nav-menu-toggle" aria-label="Menu" aria-expanded="false" aria-controls="site-nav-links"><span class="nav-menu-icon" aria-hidden="true"></span></button>"#,
    );
    out.push_str(r#"<div class="site-nav-links" id="site-nav-links">"#);
    for link in links {
        out.push_str(&render_nav_entry(link, base));
    }
    out.push_str("</div></nav>");
    (out, true)
}

fn personas_attr(personas: &[String]) -> String {
    if personas.is_empty() {
        String::new()
    } else {
        format!(r#" data-personas="{}""#, esc(&personas.join(" ")))
    }
}

fn render_nav_entry(link: &crate::types::NavLink, base: &str) -> String {
    match &link.children {
        Some(children) if !children.is_empty() => {
            let mut dd = String::from(r#"<div class="nav-dropdown">"#);
            for child in children {
                // Children render as plain links even if they themselves
                // have `children:` — we don't nest dropdowns beyond one
                // level, to keep the UX predictable.
                let href = child
                    .href
                    .as_deref()
                    .map(|h| resolve_href(h, base))
                    .unwrap_or_default();
                dd.push_str(&format!(
                    r#"<a href="{}" class="nav-link"{personas}>{}</a>"#,
                    esc(&href),
                    esc(&child.label),
                    personas = personas_attr(&child.personas),
                ));
            }
            dd.push_str("</div>");
            // The outer `<button>` is focusable so keyboard users can open
            // the dropdown via Tab + Enter. `focus-within` on the parent
            // keeps the panel open while focus is inside.
            format!(
                r#"<div class="nav-link-group"{personas}><button type="button" class="nav-link nav-link-parent" aria-haspopup="true">{label}<span class="nav-chevron">▾</span></button>{dd}</div>"#,
                label = esc(&link.label),
                dd = dd,
                personas = personas_attr(&link.personas),
            )
        }
        _ => {
            let href = link
                .href
                .as_deref()
                .map(|h| resolve_href(h, base))
                .unwrap_or_default();
            format!(
                r#"<a href="{}" class="nav-link"{personas}>{}</a>"#,
                esc(&href),
                esc(&link.label),
                personas = personas_attr(&link.personas),
            )
        }
    }
}

/// Sidebar nav (vertical, fixed to the left). Renders every `NavLink`. Parent
/// entries with `children:` become labeled sections; leaf entries at the top
/// level become standalone links. Only emitted when `nav_layout: sidebar`.
fn sidebar_html(links: Option<&Vec<crate::types::NavLink>>, base: &str) -> String {
    let Some(links) = links else {
        return String::new();
    };
    if links.is_empty() {
        return String::new();
    }
    let mut out = String::from(r#"<aside class="site-sidebar"><nav>"#);
    for link in links {
        match &link.children {
            Some(children) if !children.is_empty() => {
                out.push_str(&format!(
                    r#"<div class="sidebar-section"{personas}><div class="sidebar-section-label">{label}</div>"#,
                    label = esc(&link.label),
                    personas = personas_attr(&link.personas),
                ));
                let section_collapsed = link.collapsed;
                for child in children {
                    match &child.children {
                        Some(grandchildren) if !grandchildren.is_empty() => {
                            let collapsed_attr = if section_collapsed {
                                " data-collapsed"
                            } else {
                                ""
                            };
                            out.push_str(&format!(
                                r#"<div class="sidebar-subsection"{collapsed}{personas}><div class="sidebar-subsection-label" data-sidebar-toggle><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>{label}</div>"#,
                                label = esc(&child.label),
                                collapsed = collapsed_attr,
                                personas = personas_attr(&child.personas),
                            ));
                            for gc in grandchildren {
                                let href = gc
                                    .href
                                    .as_deref()
                                    .map(|h| resolve_href(h, base))
                                    .unwrap_or_default();
                                out.push_str(&format!(
                                    r#"<a href="{}" class="sidebar-link sidebar-link-nested"{personas}>{}</a>"#,
                                    esc(&href),
                                    esc(&gc.label),
                                    personas = personas_attr(&gc.personas),
                                ));
                            }
                            out.push_str("</div>");
                        }
                        _ => {
                            let href = child
                                .href
                                .as_deref()
                                .map(|h| resolve_href(h, base))
                                .unwrap_or_default();
                            out.push_str(&format!(
                                r#"<a href="{}" class="sidebar-link"{personas}>{}</a>"#,
                                esc(&href),
                                esc(&child.label),
                                personas = personas_attr(&child.personas),
                            ));
                        }
                    }
                }
                out.push_str("</div>");
            }
            _ => {
                let href = link
                    .href
                    .as_deref()
                    .map(|h| resolve_href(h, base))
                    .unwrap_or_default();
                out.push_str(&format!(
                    r#"<a href="{}" class="sidebar-link sidebar-link-top"{personas}>{}</a>"#,
                    esc(&href),
                    esc(&link.label),
                    personas = personas_attr(&link.personas),
                ));
            }
        }
    }
    out.push_str("</nav></aside>");
    out
}

fn search_button() -> &'static str {
    r#"<button type="button" class="site-search-btn" aria-label="Search" title="Search (⌘K)"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg></button>"#
}

fn search_overlay(base: &str) -> String {
    format!(
        r#"<div class="site-search-overlay" id="site-search" hidden>
<div class="site-search-backdrop"></div>
<div class="site-search-dialog" role="dialog" aria-label="Search">
<div class="site-search-input-wrap">
<svg class="site-search-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
<input type="search" class="site-search-input" id="site-search-input" placeholder="Search pages..." autocomplete="off" data-base="{base}">
<kbd class="site-search-kbd">esc</kbd>
</div>
<div class="site-search-results" id="site-search-results"></div>
</div>
</div>"#,
        base = base,
    )
}

fn site_bar(page: &Page, config: &SiteConfig, base: &str, right_html: &str) -> String {
    let home_href = resolve_href("/index.html", base);
    let eyebrow_html = page.eyebrow.as_deref()
        .filter(|s| !s.is_empty())
        .map(|e| format!(
            r#" <span class="site-bar-divider">/</span> <span class="site-bar-eyebrow">{}</span>"#,
            esc(e)
        ))
        .unwrap_or_default();

    // Brand slot: <img> when `logo:` is configured, falling back to the
    // text name. The logo's `src` passes through resolve_href so relative
    // paths work from any subfolder page. An explicit `height:` becomes
    // an inline style ceiling; without it, CSS caps the rendered height
    // at the site-bar content height.
    let brand_html = match &config.logo {
        Some(logo) => {
            let src = resolve_href(logo.src(), base);
            let alt = logo.alt(&config.name);
            let height_attr = logo
                .height()
                .map(|h| format!(r#" height="{h}" style="max-height:{h}px""#))
                .unwrap_or_default();
            format!(
                r#"<a class="site-bar-brand" href="{home}" aria-label="{alt}"><img class="site-bar-logo" src="{src}" alt="{alt}"{height_attr}></a>"#,
                home = esc(&home_href),
                src = esc(&src),
                alt = esc(alt),
                height_attr = height_attr,
            )
        }
        None => format!(
            r#"<a class="site-bar-name" href="{home}">{site}</a>"#,
            home = esc(&home_href),
            site = esc(&config.name),
        ),
    };

    format!(
        r#"<div class="site-bar">
  {brand}{eyebrow}
  <div class="site-bar-right">{right}</div>
</div>
"#,
        brand = brand_html,
        eyebrow = eyebrow_html,
        right = right_html,
    )
}

fn subtitle_span(page: &Page) -> String {
    page.subtitle
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!(r#"<span class="site-bar-subtitle">{}</span>"#, esc(s)))
        .unwrap_or_default()
}

// ── Standard shell ────────────────────────────────

pub mod standard {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    pub fn wrap(
        page: &Page,
        config: &SiteConfig,
        body: Rendered,
        base: &str,
        source_href: &str,
        rel_path: &str,
        release: bool,
        yaml_path: &str,
        page_title: &str,
        edit_url: Option<&str>,
    ) -> String {
        let effective_nav = page.nav_layout.unwrap_or(config.nav_layout);
        let is_sidebar = matches!(effective_nav, crate::types::NavLayout::Sidebar);
        let effective_links = page.nav.as_ref().or(config.nav.as_ref());
        // Sidebar layout moves the full nav (including nested children) into
        // a left-side <aside>; the top bar then only shows site name +
        // subtitle. Top layout keeps the existing inline nav in the bar.
        let (nav_in_bar, has_nav) = if is_sidebar {
            (
                String::new(),
                effective_links.is_some_and(|n| !n.is_empty()),
            )
        } else {
            nav_html(effective_links, base)
        };
        let mut right = subtitle_span(page);
        right.push_str(&nav_in_bar);
        right.push_str(search_button());
        let bar = site_bar(page, config, base, &right);
        let search = search_overlay(base);

        let sidebar = if is_sidebar {
            sidebar_html(effective_links, base)
        } else {
            String::new()
        };

        let mut scripts = body.scripts.clone();
        if has_nav {
            scripts.push("nav");
        }
        scripts.push("search");
        if !release {
            scripts.push("reload");
        }
        if !source_href.is_empty() {
            scripts.push("source_pill");
        }
        let view_src = view_source_html(source_href, release, yaml_path, page_title, edit_url);

        let body_class = if is_sidebar {
            format!("{} nav-layout-sidebar", Shell::Standard.class())
        } else {
            Shell::Standard.class().to_string()
        };

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
{head}
<body class="{cls}">
{bar}{sidebar}<main class="container main-content">
{body}
</main>
{view_src}
{search}
{scripts}
</body>
</html>"#,
            head = head(page, config, base, rel_path),
            cls = body_class,
            bar = bar,
            sidebar = sidebar,
            body = body.html,
            view_src = view_src,
            search = search,
            scripts = collect_scripts(&scripts),
        )
    }
}

// ── Document shell ────────────────────────────────

pub mod document {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    pub fn wrap(
        page: &Page,
        config: &SiteConfig,
        body: Rendered,
        base: &str,
        source_href: &str,
        rel_path: &str,
        release: bool,
        yaml_path: &str,
        page_title: &str,
        edit_url: Option<&str>,
    ) -> String {
        let mut right = subtitle_span(page);
        right.push_str(search_button());
        let bar = site_bar(page, config, base, &right);
        let search = search_overlay(base);

        let mut scripts = body.scripts.clone();
        scripts.push("search");
        if !release {
            scripts.push("reload");
        }
        if !source_href.is_empty() {
            scripts.push("source_pill");
        }
        let view_src = view_source_html(source_href, release, yaml_path, page_title, edit_url);

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
{head}
<body class="{cls}">
{bar}<div class="doc-root">
<article class="doc-card">
<div class="doc-body">
{body}
</div>
<footer class="doc-footer"></footer>
</article>
</div>
{view_src}
{search}
{scripts}
</body>
</html>"#,
            head = head(page, config, base, rel_path),
            cls = Shell::Document.class(),
            bar = bar,
            body = body.html,
            view_src = view_src,
            search = search,
            scripts = collect_scripts(&scripts),
        )
    }
}

// ── Deck shell ────────────────────────────────────

pub mod deck {
    use super::*;

    pub fn render(
        _page: &Page,
        config: &SiteConfig,
        slides: &[Slide],
        base: &str,
        out: &mut Rendered,
    ) {
        out.html
            .push_str(r#"<div class="deck-viewport"><div class="deck-track">"#);
        for slide in slides {
            let (label_html, slide_cls) = if slide.hide_label {
                (String::new(), " deck-slide-cover")
            } else {
                (
                    format!(r#"<div class="deck-label">{}</div>"#, esc(&slide.label)),
                    "",
                )
            };
            out.html.push_str(&format!(
                r#"<div class="deck-slide{cls}" data-label="{label}"><div class="deck-inner">{label_html}"#,
                cls = slide_cls,
                label = esc(&slide.label),
                label_html = label_html,
            ));
            for c in &slide.components {
                out.extend(components::render(c, base, config));
            }
            out.html.push_str("</div></div>");
        }
        out.html.push_str("</div></div>");
        out.scripts.push("deck");
    }

    #[allow(clippy::too_many_arguments)]
    pub fn wrap(
        page: &Page,
        config: &SiteConfig,
        body: Rendered,
        base: &str,
        _source_href: &str,
        rel_path: &str,
        release: bool,
        _yaml_path: &str,
        _page_title: &str,
        _edit_url: Option<&str>,
    ) -> String {
        let mut right = subtitle_span(page);
        right.push_str(
            r#"<button class="site-bar-print-btn" onclick="window.print()">Download PDF</button>"#,
        );
        let bar = site_bar(page, config, base, &right);

        let mut scripts = body.scripts.clone();
        if !release {
            scripts.push("reload");
        }

        let flow_class = match page.print_flow.unwrap_or_default() {
            crate::types::PrintFlow::Slides => "print-slides",
            crate::types::PrintFlow::Continuous => "print-continuous",
            crate::types::PrintFlow::Square => "print-square",
        };

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
{head}
<body class="{cls} {flow_class}">
<div class="deck-root">

{bar}
{body}

<div class="deck-nav">
  <button class="deck-arrow deck-prev" id="deck-prev"></button>
  <span class="deck-nav-label" id="deck-label"></span>
  <button class="deck-arrow deck-next" id="deck-next"></button>
</div>

</div>
{scripts}
</body>
</html>"#,
            head = head(page, config, base, rel_path),
            cls = Shell::Deck.class(),
            flow_class = flow_class,
            bar = bar,
            body = body.html,
            scripts = collect_scripts(&scripts),
        )
    }
}

fn svg14(inner: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{inner}</svg>"#,
        inner = inner,
    )
}

const ICON_PENCIL: &str =
    r#"<path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/><path d="m15 5 4 4"/>"#;
const ICON_CLIPBOARD: &str = r#"<rect width="8" height="4" x="8" y="2" rx="1" ry="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/>"#;
const ICON_EXTERNAL: &str = r#"<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>"#;
const ICON_CODE: &str =
    r#"<polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/>"#;
const ICON_CHEVRON: &str = r#"<polyline points="6 9 12 15 18 9"/>"#;

fn view_source_html(
    source_href: &str,
    release: bool,
    yaml_path: &str,
    page_title: &str,
    edit_url: Option<&str>,
) -> String {
    if source_href.is_empty() {
        return String::new();
    }

    let github_href = edit_url.map(|u| {
        let base = u.trim_end_matches('/');
        format!("{}/{}", base, yaml_path)
    });

    let local_href = if !source_href.starts_with("http") {
        Some(source_href)
    } else {
        None
    };

    let primary_label = if !release || github_href.is_some() {
        "Edit"
    } else {
        "Source"
    };

    let prompt = format!(
        "Edit the kazam page \u{201c}{}\u{201d} ({}): ",
        esc(page_title),
        esc(yaml_path),
    );

    let mut items = String::new();

    // Copy edit prompt — always first
    items.push_str(&format!(
        r#"<button class="source-pill-item" role="menuitem" data-copy-prompt="{prompt}">{icon} Copy edit prompt</button>"#,
        prompt = esc(&prompt),
        icon = svg14(ICON_CLIPBOARD),
    ));

    // Edit on GitHub
    if let Some(ref href) = github_href {
        items.push_str(&format!(
            r#"<a class="source-pill-item" role="menuitem" href="{href}" target="_blank" rel="noopener">{icon} Edit on GitHub</a>"#,
            href = esc(href),
            icon = svg14(ICON_EXTERNAL),
        ));
    }

    // Edit page (dev mode local editor)
    if !release {
        if let Some(href) = local_href {
            items.push_str(&format!(
                r#"<a class="source-pill-item" role="menuitem" href="{href}">{icon} Edit page</a>"#,
                href = esc(href),
                icon = svg14(ICON_PENCIL),
            ));
        }
    }

    // View source (release mode, local .source.html)
    if release {
        if let Some(href) = local_href {
            items.push_str(&format!(
                r#"<a class="source-pill-item" role="menuitem" href="{href}">{icon} View source</a>"#,
                href = esc(href),
                icon = svg14(ICON_CODE),
            ));
        }
    }

    format!(
        r##"<div class="source-pill">
<button class="source-pill-btn" aria-expanded="false" aria-haspopup="true">
  {icon}
  <span>{label}</span>
  {caret}
</button>
<div class="source-pill-menu" role="menu">
{items}
</div>
</div>"##,
        icon = svg14(ICON_PENCIL),
        label = primary_label,
        caret = svg14(ICON_CHEVRON),
        items = items,
    )
}

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
fn nav_html(config: &SiteConfig, base: &str) -> (String, bool) {
    let Some(links) = &config.nav else {
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
fn sidebar_html(config: &SiteConfig, base: &str) -> String {
    let Some(links) = &config.nav else {
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
                for child in children {
                    match &child.children {
                        Some(grandchildren) if !grandchildren.is_empty() => {
                            out.push_str(&format!(
                                r#"<div class="sidebar-subsection"{personas}><div class="sidebar-subsection-label">{label}</div>"#,
                                label = esc(&child.label),
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

    pub fn wrap(
        page: &Page,
        config: &SiteConfig,
        body: Rendered,
        base: &str,
        source_href: &str,
        rel_path: &str,
        release: bool,
    ) -> String {
        let is_sidebar = matches!(config.nav_layout, crate::types::NavLayout::Sidebar);
        // Sidebar layout moves the full nav (including nested children) into
        // a left-side <aside>; the top bar then only shows site name +
        // subtitle. Top layout keeps the existing inline nav in the bar.
        let (nav_in_bar, has_nav) = if is_sidebar {
            (
                String::new(),
                config.nav.as_ref().is_some_and(|n| !n.is_empty()),
            )
        } else {
            nav_html(config, base)
        };
        let mut right = subtitle_span(page);
        right.push_str(&nav_in_bar);
        let bar = site_bar(page, config, base, &right);

        let sidebar = if is_sidebar {
            sidebar_html(config, base)
        } else {
            String::new()
        };

        let mut scripts = body.scripts.clone();
        if has_nav {
            scripts.push("nav");
        }
        if !release {
            scripts.push("reload");
        }
        let view_src = view_source_html(source_href);

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
{scripts}
</body>
</html>"#,
            head = head(page, config, base, rel_path),
            cls = body_class,
            bar = bar,
            sidebar = sidebar,
            body = body.html,
            view_src = view_src,
            scripts = collect_scripts(&scripts),
        )
    }
}

// ── Document shell ────────────────────────────────

pub mod document {
    use super::*;

    pub fn wrap(
        page: &Page,
        config: &SiteConfig,
        body: Rendered,
        base: &str,
        source_href: &str,
        rel_path: &str,
        release: bool,
    ) -> String {
        let bar = site_bar(page, config, base, &subtitle_span(page));

        let mut scripts = body.scripts.clone();
        if !release {
            scripts.push("reload");
        }
        let view_src = view_source_html(source_href);

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
{scripts}
</body>
</html>"#,
            head = head(page, config, base, rel_path),
            cls = Shell::Document.class(),
            bar = bar,
            body = body.html,
            view_src = view_src,
            scripts = collect_scripts(&scripts),
        )
    }
}

// ── Deck shell ────────────────────────────────────

pub mod deck {
    use super::*;

    pub fn render(
        _page: &Page,
        _config: &SiteConfig,
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
                out.extend(components::render(c, base));
            }
            out.html.push_str("</div></div>");
        }
        out.html.push_str("</div></div>");
        out.scripts.push("deck");
    }

    pub fn wrap(
        page: &Page,
        config: &SiteConfig,
        body: Rendered,
        base: &str,
        _source_href: &str,
        rel_path: &str,
        release: bool,
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

fn view_source_html(source_href: &str) -> String {
    if source_href.is_empty() {
        return String::new();
    }
    format!(
        r##"<a class="view-source" href="{src}" title="View raw YAML source">
  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
  <span>View source</span>
</a>"##,
        src = esc(source_href)
    )
}

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};

use crate::server;
use crate::theme::{self, Theme};
use crate::types::{Glow, Mode, Texture};

/// `kazam open`'s theme preference, persisted at `~/.kazam/open-theme.json`
/// so it survives across invocations even though each one binds a fresh
/// (possibly different) port - a per-origin store like localStorage would
/// silently lose the setting the moment the port changes.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct OpenThemeConfig {
    /// One of the curata rainbow accents (`red`..`violet`), or `"default"`
    /// for the neutral dark/light base with no accent swap.
    color: String,
    mode: String,
    texture: String,
    glow: String,
    /// Custom accent hex, e.g. `#14b8a6`. Overrides `color`'s accent when
    /// set. Validated as `#` + 6 hex digits before it's ever persisted.
    accent_hex: Option<String>,
}

impl Default for OpenThemeConfig {
    fn default() -> Self {
        Self {
            color: "teal".into(),
            mode: "dark".into(),
            texture: "dots".into(),
            glow: "none".into(),
            accent_hex: None,
        }
    }
}

fn theme_config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".kazam").join("open-theme.json"))
}

fn load_theme_config() -> OpenThemeConfig {
    theme_config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_theme_config(cfg: &OpenThemeConfig) -> Result<()> {
    let path = theme_config_path().context("cannot resolve $HOME to save theme preference")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(cfg)?)
        .with_context(|| format!("cannot write {}", path.display()))
}

fn parse_texture(s: &str) -> Texture {
    match s {
        "dots" => Texture::Dots,
        "grid" => Texture::Grid,
        "grain" => Texture::Grain,
        "topography" => Texture::Topography,
        "diagonal" => Texture::Diagonal,
        _ => Texture::None,
    }
}

fn parse_glow(s: &str) -> Glow {
    match s {
        "accent" => Glow::Accent,
        "corner" => Glow::Corner,
        _ => Glow::None,
    }
}

const VALID_COLORS: &[&str] = &[
    "default", "red", "orange", "yellow", "green", "teal", "blue", "indigo", "violet",
];
const VALID_TEXTURES: &[&str] = &["none", "dots", "grid", "grain", "topography", "diagonal"];
const VALID_GLOWS: &[&str] = &["none", "accent", "corner"];

fn is_valid_hex_color(s: &str) -> bool {
    let h = s.strip_prefix('#').unwrap_or(s);
    h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit())
}

fn resolve_theme(cfg: &OpenThemeConfig) -> (Theme, Texture, Glow) {
    let mode = if cfg.mode == "light" {
        Mode::Light
    } else {
        Mode::Dark
    };
    let name: &str = if cfg.color == "default" {
        if cfg.mode == "light" {
            "light"
        } else {
            "dark"
        }
    } else {
        &cfg.color
    };
    let mut t = Theme::named(name, mode);
    if let Some(hex) = cfg.accent_hex.as_deref().filter(|h| !h.is_empty()) {
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("accent".to_string(), hex.to_string());
        t = t.with_overrides(&overrides);
    }
    (t, parse_texture(&cfg.texture), parse_glow(&cfg.glow))
}

pub fn run(path: &Path, port: u16) -> Result<()> {
    let path = std::fs::canonicalize(path)
        .with_context(|| format!("cannot resolve: {}", path.display()))?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "md" | "yaml" | "yml" | "json" | "agl" => {}
        _ => anyhow::bail!(
            "unsupported format '.{ext}' - kazam open supports .md, .yaml, .yml, .json, .agl"
        ),
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read: {}", path.display()))?;

    validate_content(&content, &ext)?;

    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let state = Arc::new(State {
        version: AtomicU64::new(1),
        disk: std::sync::RwLock::new(content),
        buffer: std::sync::RwLock::new(None),
        conflict: AtomicBool::new(false),
        path: path.clone(),
        file_name,
        ext,
    });

    // File watcher
    let watch_state = state.clone();
    let watch_path = path.clone();
    thread::spawn(move || watch_file(watch_path, watch_state));

    server::install_shutdown_handler();

    // HTTP server
    let (srv, actual_port) = server::bind_next_available(port)?;
    if actual_port != port {
        println!("\n  ⚠ port {port} is in use - serving on {actual_port} instead");
    }
    let url = format!("http://localhost:{actual_port}");
    println!("\n  ➜ {url}");
    println!("  watching {}\n", path.display());
    server::open_browser(&url);

    for req in srv.incoming_requests() {
        let st = state.clone();
        thread::spawn(move || {
            if let Err(e) = handle(req, &st) {
                eprintln!("  request error: {e}");
            }
        });
    }
    Ok(())
}

/// Shared server state. `buffer` holds in-browser edits that have not been
/// written to disk; `conflict` is set when the file changed underneath one.
struct State {
    version: AtomicU64,
    disk: std::sync::RwLock<String>,
    buffer: std::sync::RwLock<Option<String>>,
    conflict: AtomicBool,
    path: PathBuf,
    file_name: String,
    ext: String,
}

impl State {
    /// Current text: unsaved edits win over what is on disk.
    fn text(&self) -> String {
        match self.buffer.read().unwrap().as_ref() {
            Some(s) => s.clone(),
            None => self.disk.read().unwrap().clone(),
        }
    }

    fn is_dirty(&self) -> bool {
        self.buffer.read().unwrap().is_some()
    }

    fn has_conflict(&self) -> bool {
        self.conflict.load(Ordering::SeqCst)
    }
}

fn handle(req: tiny_http::Request, st: &State) -> Result<()> {
    let url = req.url().split('?').next().unwrap_or("/").to_string();
    let get = server::is_get(&req);
    let post = server::is_post(&req);

    match (url.as_str(), get, post) {
        ("/__version__", true, _) => server::respond_version(req, &st.version),

        // Raw text - what an agent reads. Unsaved browser edits win.
        ("/api/content", true, _) => server::respond_plain(req, &st.text()),

        ("/api/content", _, true) => handle_post_content(req, st),

        ("/api/rendered", true, _) => server::respond_html(req, &render_body(&st.ext, &st.text())),

        // Advisory for agents: check before writing the file on disk.
        ("/api/status", true, _) => {
            let err = syntax_error(&st.text(), &st.ext);
            let json = format!(
                r#"{{"dirty":{},"conflict":{},"version":{},"valid":{},"error":{}}}"#,
                st.is_dirty(),
                st.has_conflict(),
                st.version.load(Ordering::SeqCst),
                err.is_none(),
                json_string(err.as_deref()),
            );
            server::respond_plain(req, &json)
        }

        ("/api/save", _, true) => handle_save(req, st),

        // Stateless: highlights whatever the browser currently has in the
        // textarea, not st.text(), so the overlay tracks keystrokes without
        // waiting on the save debounce or touching the edit buffer.
        ("/api/highlight", _, true) => handle_highlight(req, st),

        // Conflict resolution: keep the buffer, or throw it away for disk.
        ("/api/keep-mine", _, true) => {
            st.conflict.store(false, Ordering::SeqCst);
            server::respond_plain(req, r#"{"ok":true}"#)
        }
        ("/api/take-disk", _, true) => {
            *st.buffer.write().unwrap() = None;
            st.conflict.store(false, Ordering::SeqCst);
            st.version.fetch_add(1, Ordering::SeqCst);
            server::respond_plain(req, r#"{"ok":true}"#)
        }

        ("/", true, _) => {
            let disk = st.disk.read().unwrap().clone();
            let edited = st.buffer.read().unwrap().clone();
            let html = render_page(
                &st.file_name,
                &st.ext,
                &disk,
                edited.as_deref(),
                st.has_conflict(),
            );
            server::respond_html(req, &html)
        }

        // Sibling assets referenced by relative path from the opened file
        // (mainly images: `kazam open notes.md` with `![x](screenshot.png)`
        // in the same folder). Scoped to image extensions and confined to
        // the opened file's directory, not a general static file server.
        (_, true, _) if is_image_asset(&url) => serve_asset(req, st, &url),

        ("/api/theme", true, _) => {
            server::respond_plain(req, &serde_json::to_string(&load_theme_config())?)
        }
        ("/api/theme", _, true) => handle_set_theme(req),

        (_, false, false) => server::respond_405(req),
        _ => server::respond_404(req),
    }
}

fn is_image_asset(url: &str) -> bool {
    let ext = Path::new(url)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" | "bmp" | "avif"
    )
}

fn image_content_type(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Serves a file relative to the opened file's directory. Resolves
/// `..` and symlinks via `canonicalize` and refuses anything that lands
/// outside that directory, so `/api/*`-style probing can't read arbitrary
/// files off the host.
fn serve_asset(req: tiny_http::Request, st: &State, url: &str) -> Result<()> {
    let Some(base_dir) = st.path.parent() else {
        return server::respond_404(req);
    };
    let rel = percent_decode(url.trim_start_matches('/'));
    let candidate = base_dir.join(&rel);
    let Ok(resolved) = std::fs::canonicalize(&candidate) else {
        return server::respond_404(req);
    };
    let Ok(base_canonical) = std::fs::canonicalize(base_dir) else {
        return server::respond_404(req);
    };
    if !resolved.starts_with(&base_canonical) {
        return server::respond_404(req);
    }
    let Ok(bytes) = std::fs::read(&resolved) else {
        return server::respond_404(req);
    };
    let ext = resolved
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    server::respond_bytes(req, bytes, image_content_type(&ext))
}

fn handle_post_content(mut req: tiny_http::Request, st: &State) -> Result<()> {
    let mut body = String::new();
    req.as_reader()
        .read_to_string(&mut body)
        .context("read POST body")?;
    // Syntax errors are reported, not rejected: text is transiently invalid
    // while you type, and refusing the save would lose keystrokes.
    let err = syntax_error(&body, &st.ext);
    *st.buffer.write().unwrap() = Some(body);
    let payload = format!(
        r#"{{"ok":true,"valid":{},"error":{}}}"#,
        err.is_none(),
        json_string(err.as_deref()),
    );
    let resp = tiny_http::Response::from_string(payload)
        .with_header(server::hdr("Content-Type", "application/json"))
        .with_header(server::hdr("Cache-Control", "no-store"));
    req.respond(resp).context("respond")
}

/// Persists a theme choice to `~/.kazam/open-theme.json`. Rejects anything
/// outside the known color/texture/glow sets or a malformed custom hex,
/// since this file is hand-editable and a bad value would otherwise surface
/// as a silent fallback to defaults on the next launch instead of an error
/// now, while the user is looking at the panel that caused it.
fn handle_set_theme(mut req: tiny_http::Request) -> Result<()> {
    let mut body = String::new();
    req.as_reader()
        .read_to_string(&mut body)
        .context("read POST body")?;
    let cfg: OpenThemeConfig = match serde_json::from_str(&body) {
        Ok(c) => c,
        Err(e) => {
            return server::respond_plain(
                req,
                &format!(r#"{{"ok":false,"error":"invalid JSON: {e}"}}"#),
            )
        }
    };
    let bad = if !VALID_COLORS.contains(&cfg.color.as_str()) {
        Some(format!("unknown color: {}", cfg.color))
    } else if !VALID_TEXTURES.contains(&cfg.texture.as_str()) {
        Some(format!("unknown texture: {}", cfg.texture))
    } else if !VALID_GLOWS.contains(&cfg.glow.as_str()) {
        Some(format!("unknown glow: {}", cfg.glow))
    } else if cfg.mode != "dark" && cfg.mode != "light" {
        Some(format!("unknown mode: {}", cfg.mode))
    } else if let Some(hex) = cfg.accent_hex.as_deref().filter(|h| !h.is_empty()) {
        if is_valid_hex_color(hex) {
            None
        } else {
            Some(format!("invalid hex color: {hex}"))
        }
    } else {
        None
    };
    if let Some(msg) = bad {
        return server::respond_plain(req, &format!(r#"{{"ok":false,"error":"{msg}"}}"#));
    }
    if let Err(e) = save_theme_config(&cfg) {
        return server::respond_plain(
            req,
            &format!(r#"{{"ok":false,"error":"{e}"}}"#).replace('\n', " "),
        );
    }
    server::respond_plain(req, r#"{"ok":true}"#)
}

/// Highlights posted text for the edit-mode overlay. Pure compute, no side
/// effects on `st.buffer` - the save debounce owns writing to the buffer, this
/// just needs `st.ext` to pick a highlighter.
fn handle_highlight(mut req: tiny_http::Request, st: &State) -> Result<()> {
    let mut body = String::new();
    req.as_reader()
        .read_to_string(&mut body)
        .context("read POST body")?;
    let html = match st.ext.as_str() {
        "yaml" | "yml" | "json" | "agl" => render_code_inline(&body, &st.ext),
        _ => html_escape(&body),
    };
    server::respond_html(req, &html)
}

/// Write the edit buffer to disk. Refuses while a conflict is unresolved,
/// since the file moved underneath the buffer and saving would clobber it.
fn handle_save(req: tiny_http::Request, st: &State) -> Result<()> {
    if st.has_conflict() {
        let payload = format!(
            r#"{{"ok":false,"error":{}}}"#,
            json_string(Some(
                "the file changed on disk, resolve the conflict before saving"
            )),
        );
        return server::respond_plain(req, &payload);
    }

    let Some(text) = st.buffer.read().unwrap().clone() else {
        // Nothing unsaved, so saving is a no-op rather than an error.
        return server::respond_plain(req, r#"{"ok":true,"saved":false}"#);
    };

    let payload = match write_atomic(&st.path, &text) {
        Ok(()) => {
            // Adopt the text as the new disk state and drop the buffer, so the
            // watcher event this write triggers is recognized as our own.
            *st.disk.write().unwrap() = text;
            *st.buffer.write().unwrap() = None;
            println!("  ✓ saved {}", st.path.display());
            r#"{"ok":true,"saved":true}"#.to_string()
        }
        Err(e) => {
            eprintln!("  save failed: {e:#}");
            format!(
                r#"{{"ok":false,"error":{}}}"#,
                json_string(Some(&e.to_string()))
            )
        }
    };
    server::respond_plain(req, &payload)
}

/// Write via temp file plus rename so a crash cannot leave the file truncated.
fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let tmp = dir.join(format!(".{stem}.kazam-tmp"));
    std::fs::write(&tmp, content).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("rename into {}", path.display()))?;
    Ok(())
}

/// Minimal JSON string encoder for the handful of strings we emit.
fn json_string(s: Option<&str>) -> String {
    match s {
        None => "null".to_string(),
        Some(s) => {
            let mut out = String::with_capacity(s.len() + 2);
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
    }
}

fn render_body(ext: &str, content: &str) -> String {
    match ext {
        "md" => render_markdown(content),
        "yaml" | "yml" => render_code(content, "yaml"),
        "json" => render_code(content, "json"),
        "agl" => render_code(content, "agl"),
        _ => format!("<pre>{}</pre>", html_escape(content)),
    }
}

fn render_page(
    file_name: &str,
    ext: &str,
    content: &str,
    edited: Option<&str>,
    conflict: bool,
) -> String {
    // The edit buffer wins for both panes, so a reload after an unsaved edit
    // shows the same text in the rendered view and the textarea.
    let raw_for_edit = edited.unwrap_or(content);
    let rendered = render_body(ext, raw_for_edit);
    let banner = if conflict {
        r#"<div class="conflict" id="conflict">
  <span><strong>This file changed on disk</strong> while you had unsaved edits. Yours are still here.</span>
  <span class="spacer"></span>
  <button onclick="resolve('keep-mine')">Keep mine</button>
  <button onclick="resolve('take-disk')">Load from disk</button>
</div>"#
    } else {
        ""
    };

    let cfg = load_theme_config();
    let (theme, texture, glow) = resolve_theme(&cfg);
    let theme_css = theme::render_css(&theme, texture, glow);
    let (syn_key, syn_str, syn_num, syn_bool, syn_null) = if cfg.mode == "light" {
        ("#0369a1", "#15803d", "#a16207", "#7e22ce", "#b91c1c")
    } else {
        ("#7dd3fc", "#86efac", "#fde68a", "#c4b5fd", "#f87171")
    };
    let hex_value = cfg.accent_hex.clone().unwrap_or_default();
    let hex_or_default = if hex_value.is_empty() {
        "#14b8a6".to_string()
    } else {
        hex_value.clone()
    };
    let cfg_accent_hex_js = match &cfg.accent_hex {
        Some(h) if !h.is_empty() => format!("'{}'", h),
        _ => "null".to_string(),
    };
    let swatches_html = color_swatch_html(&cfg.color);
    let mode_html = mode_toggle_html(&cfg.mode);
    let texture_options = select_options(
        &[
            ("none", "None"),
            ("dots", "Dots"),
            ("grid", "Grid"),
            ("grain", "Grain"),
            ("topography", "Topography"),
            ("diagonal", "Diagonal"),
        ],
        &cfg.texture,
    );
    let glow_options = select_options(
        &[("none", "None"), ("accent", "Accent"), ("corner", "Corner")],
        &cfg.glow,
    );

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{file_name} - kazam</title>
<style>
{theme_css}
</style>
<style>
/* kazam open chrome - everything below has no curata equivalent (toolbar,
   editor overlay, theme panel). Typography/texture for rendered markdown
   comes from the shared stylesheet above via .c-markdown, so this tool
   looks and updates exactly like curata's own page rendering. */
* {{ margin:0; padding:0; box-sizing:border-box; }}
html {{ background: var(--bg); }}
body {{
  color: var(--snow);
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  line-height: 1.6;
  min-height: 100vh;
}}
.toolbar {{
  position: sticky;
  top: 0;
  z-index: 10;
  background: var(--card-bg);
  border-bottom: 1px solid var(--card-border);
  padding: 8px 24px;
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
}}
.toolbar .filename {{
  font-weight: 600;
  color: var(--teal);
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
}}
.toolbar .badge {{
  background: rgba(var(--text-rgb),0.09);
  color: var(--muted);
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}}
.toolbar .spacer {{ flex: 1; }}
.toolbar {{ position: relative; }}
.toolbar button {{
  background: rgba(var(--text-rgb),0.09);
  color: var(--snow);
  border: none;
  padding: 4px 12px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 13px;
  font-family: inherit;
}}
.toolbar button:hover {{ background: var(--teal); color: var(--bg); }}
.toolbar button.active {{ background: var(--teal); color: var(--bg); }}
.container {{
  max-width: 820px;
  margin: 32px auto;
  padding: 40px 56px;
  background: var(--card-bg);
  border: 1px solid var(--card-border);
  border-radius: 16px;
  box-shadow: 0 4px 40px rgba(0,0,0,0.3);
}}
.container.wide {{
  max-width: 1400px;
}}
.code-view::-webkit-scrollbar {{ height: 8px; }}
.code-view::-webkit-scrollbar-thumb {{ background: var(--card-border); border-radius: 4px; }}
.code-view::-webkit-scrollbar-track {{ background: transparent; }}
.view-mode {{ display: block; }}
.edit-mode {{ display: none; }}
body.editing .view-mode {{ display: none; }}
body.editing .edit-mode {{ display: block; }}
.frontmatter {{
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
  font-size: 12px;
  line-height: 1.6;
  white-space: pre;
  color: rgba(var(--text-rgb), 0.35);
  padding-bottom: 0.8em;
  margin-bottom: 1em;
  border-bottom: 1px solid var(--card-border);
}}
.frontmatter .syn-key {{ color: rgba(var(--text-rgb), 0.45); }}
.frontmatter .syn-punct {{ color: rgba(var(--text-rgb), 0.3); }}
.frontmatter .syn-string {{ color: rgba(var(--text-rgb), 0.35); }}
.frontmatter .syn-number {{ color: rgba(var(--text-rgb), 0.35); }}
.frontmatter .syn-bool {{ color: rgba(var(--text-rgb), 0.35); }}
/* Code view (yaml/json/agl) - plain formatted text, not a boxed card */
.code-view {{
  overflow-x: auto;
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
  font-size: 14px;
  line-height: 1.5;
  tab-size: 2;
  white-space: pre;
  cursor: text;
}}
.code-view .ln {{
  color: var(--muted);
  user-select: none;
  display: inline-block;
  width: 3em;
  text-align: right;
  margin-right: 1.5em;
  opacity: 0.5;
}}
/* Syntax colors - fixed, not accent-driven, same reasoning a code editor's
   syntax theme doesn't follow the UI accent. Picked per-mode for contrast
   (server-resolved at render time, not switched live). */
.syn-key {{ color: {syn_key}; }}
.syn-str {{ color: {syn_str}; }}
.syn-num {{ color: {syn_num}; }}
.syn-bool {{ color: {syn_bool}; }}
.syn-null {{ color: {syn_null}; }}
.syn-comment {{ color: var(--muted); font-style: italic; }}
.syn-punct {{ color: var(--muted); }}
/* Edit mode: a highlighted <pre> sits behind a transparent-text textarea,
   both sharing the exact font metrics and padding so keystrokes land on the
   right character. The box (background/border/radius) lives on the wrapper
   only - the boxed look the .code-view used to have, moved to where editing
   actually happens. */
.edit-wrap {{
  position: relative;
  min-height: calc(100vh - 120px);
  background: rgba(var(--text-rgb),0.07);
  border: 1px solid var(--card-border);
  border-radius: 6px;
  overflow: hidden;
}}
.edit-wrap:focus-within {{
  border-color: var(--teal);
}}
.edit-highlight, .edit-area {{
  position: absolute;
  inset: 0;
  margin: 0;
  width: 100%;
  height: 100%;
  padding: 20px;
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
  font-size: 14px;
  line-height: 21px;
  tab-size: 2;
  white-space: pre-wrap;
  word-wrap: break-word;
  overflow-wrap: break-word;
  letter-spacing: normal;
  word-spacing: normal;
  font-variant-ligatures: none;
  -webkit-text-size-adjust: none;
  text-size-adjust: none;
}}
.edit-highlight {{
  color: var(--snow);
  pointer-events: none;
  overflow: hidden;
}}
.edit-area {{
  -webkit-appearance: none;
  appearance: none;
  background: transparent;
  color: transparent;
  caret-color: var(--snow);
  border: none;
  outline: none;
  resize: none;
  overflow: auto;
}}
.edit-area::selection {{
  background: rgba(var(--accent-rgb),0.3);
}}
.status {{
  position: fixed;
  bottom: 16px;
  right: 16px;
  background: var(--card-bg);
  border: 1px solid var(--card-border);
  padding: 4px 12px;
  border-radius: 4px;
  font-size: 12px;
  color: var(--muted);
  opacity: 0;
  transition: opacity 0.2s;
}}
.status.show {{ opacity: 1; }}
::selection {{ background: rgba(var(--accent-rgb),0.25); }}
.conflict {{
  background: #422006;
  border-bottom: 1px solid #a16207;
  color: #fde68a;
  padding: 10px 24px;
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
}}
.conflict .spacer {{ flex: 1; }}
.conflict button {{
  background: #a16207;
  color: #fff;
  border: none;
  padding: 4px 12px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 13px;
  font-family: inherit;
}}
.conflict button:hover {{ background: #ca8a04; }}
.syntax-err {{
  background: #450a0a;
  border-bottom: 1px solid #b91c1c;
  color: #fecaca;
  padding: 8px 24px;
  font-size: 13px;
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
  display: none;
}}
.syntax-err.show {{ display: block; }}
/* Theme panel */
.theme-panel {{
  display: none;
  position: absolute;
  top: 44px;
  right: 24px;
  z-index: 20;
  background: var(--card-bg);
  border: 1px solid var(--card-border);
  border-radius: 10px;
  padding: 14px 16px;
  box-shadow: 0 4px 24px rgba(0,0,0,0.35);
  min-width: 250px;
  font-size: 13px;
}}
.theme-panel.show {{ display: block; }}
.theme-panel-row {{ display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 10px; }}
.theme-panel-row:last-child {{ margin-bottom: 0; }}
.theme-panel-label {{ color: var(--muted); font-size: 12px; }}
.theme-swatches {{ display: flex; gap: 6px; flex-wrap: wrap; max-width: 168px; justify-content: flex-end; }}
.theme-swatch {{
  width: 18px; height: 18px;
  border-radius: 50%;
  border: 2px solid transparent;
  background: var(--swatch);
  cursor: pointer;
  padding: 0;
}}
.theme-swatch.active {{ border-color: var(--snow); }}
.theme-toggle-group {{ display: flex; gap: 4px; }}
.theme-toggle-group button {{
  background: rgba(var(--text-rgb),0.08);
  color: var(--snow);
  border: none;
  padding: 3px 10px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 12px;
  font-family: inherit;
}}
.theme-toggle-group button.active {{ background: var(--teal); color: var(--bg); }}
.theme-panel select {{
  background: rgba(var(--text-rgb),0.08);
  color: var(--snow);
  border: none;
  padding: 3px 8px;
  border-radius: 4px;
  font-size: 12px;
  font-family: inherit;
}}
.theme-hex-row {{ display: flex; gap: 6px; align-items: center; }}
.theme-hex-row input[type="color"] {{ width: 22px; height: 22px; border: none; padding: 0; background: none; cursor: pointer; }}
.theme-hex-row input[type="text"] {{
  width: 84px;
  background: rgba(var(--text-rgb),0.08);
  border: none;
  color: var(--snow);
  padding: 3px 6px;
  border-radius: 4px;
  font-size: 12px;
  font-family: 'SF Mono', Monaco, monospace;
}}
</style>
</head>
<body>
<div class="toolbar">
  <span class="filename">{file_name}</span>
  <span class="badge">{ext}</span>
  <span class="spacer"></span>
  <button id="themeBtn" onclick="toggleThemePanel()" title="Theme">🎨</button>
  <div class="theme-panel" id="themePanel">
    <div class="theme-panel-row">
      <span class="theme-panel-label">Color</span>
      <div class="theme-swatches">{swatches_html}</div>
    </div>
    <div class="theme-panel-row">
      <span class="theme-panel-label">Mode</span>
      <div class="theme-toggle-group">{mode_html}</div>
    </div>
    <div class="theme-panel-row">
      <span class="theme-panel-label">Texture</span>
      <select id="textureSelect" onchange="applyTheme({{texture:this.value}})">{texture_options}</select>
    </div>
    <div class="theme-panel-row">
      <span class="theme-panel-label">Glow</span>
      <select id="glowSelect" onchange="applyTheme({{glow:this.value}})">{glow_options}</select>
    </div>
    <div class="theme-panel-row">
      <span class="theme-panel-label">Custom hex</span>
      <div class="theme-hex-row">
        <input type="color" id="hexPicker" value="{hex_or_default}" onchange="applyTheme({{color:'default',accent_hex:this.value}})">
        <input type="text" id="hexText" value="{hex_value}" placeholder='#14b8a6' onkeydown="if(event.key==='Enter')applyTheme({{color:'default',accent_hex:this.value}})">
      </div>
    </div>
  </div>
  <button id="viewBtn" class="active" onclick="setMode('view')">View</button>
  <button id="editBtn" onclick="setMode('edit')">Edit</button>
  <button id="copyBtn" onclick="copyFile()">Copy</button>
  <button id="saveBtn" onclick="saveFile()">Save</button>
</div>
{banner}
<div class="syntax-err" id="syntaxErr"></div>
<div class="container{container_wide}">
  <div class="view-mode c-markdown" id="viewPane">{rendered}</div>
  <div class="edit-mode">
    <div class="edit-wrap">
      <pre class="edit-highlight" id="editorHighlight" aria-hidden="true">{edit_highlighted}</pre>
      <textarea class="edit-area" id="editor" spellcheck="false">{edit_escaped}</textarea>
    </div>
  </div>
</div>
<div class="status" id="status"></div>
<script>
// Poll for disk changes. Never reload over unsaved edits - surface the
// conflict in place instead, so typing and cursor position survive.
(function(){{
  var v=0;
  setInterval(function(){{
    fetch('/__version__').then(function(r){{return r.text()}}).then(function(t){{
      var n=parseInt(t,10);
      if(!v){{v=n;return;}}
      if(n===v)return;
      v=n;
      fetch('/api/status').then(function(r){{return r.json()}}).then(function(s){{
        if(s.conflict){{showConflict();}}
        else{{location.reload();}}
      }}).catch(function(){{location.reload();}});
    }}).catch(function(){{}});
  }},500);
}})();
function showConflict(){{
  if(document.getElementById('conflict'))return;
  var d=document.createElement('div');
  d.className='conflict';d.id='conflict';
  d.innerHTML='<span><strong>This file changed on disk</strong> while you had unsaved edits. Yours are still here.</span>'+
    '<span class="spacer"></span>'+
    '<button onclick="resolve(\'keep-mine\')">Keep mine</button>'+
    '<button onclick="resolve(\'take-disk\')">Load from disk</button>';
  document.querySelector('.toolbar').insertAdjacentElement('afterend',d);
}}
</script>
<script>
var dirty=false, queue=Promise.resolve();
function setMode(m){{
  document.body.className=m==='edit'?'editing':'';
  document.getElementById('viewBtn').className=m==='view'?'active':'';
  document.getElementById('editBtn').className=m==='edit'?'active':'';
  if(m==='edit'){{ document.getElementById('editor').focus(); return; }}
  // Leaving edit mode: flush any debounced save, then re-render from the buffer
  clearTimeout(debounce);
  (dirty?postContent():queue).then(refreshView);
}}
function resolve(how){{
  fetch('/api/'+how,{{method:'POST'}}).then(function(){{
    if(how==='take-disk'){{ location.reload(); return; }}
    var c=document.getElementById('conflict');
    if(c) c.remove();
  }});
}}
function refreshView(){{
  return fetch('/api/rendered')
    .then(function(r){{return r.text()}})
    .then(function(h){{document.getElementById('viewPane').innerHTML=h;}})
    .catch(function(){{}});
}}
var editor=document.getElementById('editor');
var highlightPane=document.getElementById('editorHighlight');
var debounce=null,hlRaf=null;
var FILE_EXT='{ext}';
function esc(s){{return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');}}
function colorVal(v){{
  if(!v)return'';
  if(v==='true'||v==='false')return'<span class="syn-bool">'+v+'</span>';
  if(v==='null'||v==='~')return'<span class="syn-null">'+v+'</span>';
  if(v.charAt(0)==='"'||v.charAt(0)==="'")return'<span class="syn-str">'+esc(v)+'</span>';
  if(v!==''&&!isNaN(v))return'<span class="syn-num">'+v+'</span>';
  return esc(v);
}}
function hlYaml(ln){{
  var t=ln.trimStart(),sp=ln.length-t.length,pad=' '.repeat(sp);
  if(t.charAt(0)==='#')return pad+'<span class="syn-comment">'+esc(t)+'</span>';
  var ci=t.indexOf(':');
  if(ci>=0){{var k=t.substring(0,ci),r=t.substring(ci+1).trim();return pad+'<span class="syn-key">'+esc(k)+'</span><span class="syn-punct">:</span> '+colorVal(r);}}
  if(t.substring(0,2)==='- ')return pad+'<span class="syn-punct">-</span> '+colorVal(t.substring(2).trim());
  return pad+esc(t);
}}
function hlJson(ln){{
  var o='',i=0;
  while(i<ln.length){{
    var c=ln.charAt(i);
    if(c==='"'){{var s='"',e2=false;i++;while(i<ln.length){{var x=ln.charAt(i);s+=x;i++;if(e2){{e2=false;continue;}}if(x==='\\'){{e2=true;continue;}}if(x==='"')break;}}var rest=ln.substring(i).trimStart();o+='<span class="'+(rest.charAt(0)===':'?'syn-key':'syn-str')+'">'+esc(s)+'</span>';}}
    else if((c>='0'&&c<='9')||c==='-'){{var n='';while(i<ln.length&&/[\d.eE+\-]/.test(ln.charAt(i))){{n+=ln.charAt(i);i++;}}o+='<span class="syn-num">'+esc(n)+'</span>';}}
    else if(c==='t'||c==='f'||c==='n'){{var w='';while(i<ln.length&&/[a-z]/.test(ln.charAt(i))){{w+=ln.charAt(i);i++;}}if(w==='true'||w==='false')o+='<span class="syn-bool">'+w+'</span>';else if(w==='null')o+='<span class="syn-null">'+w+'</span>';else o+=esc(w);}}
    else if('{{}}[]:,'.indexOf(c)>=0){{o+='<span class="syn-punct">'+esc(c)+'</span>';i++;}}
    else{{o+=esc(c);i++;}}
  }}
  return o;
}}
var AGL_KW='spec in out requires skill cache invariant flow state branch if deny without gate import call map evaluate fan watch TERMINATE next'.split(' ');
function hlAgl(ln){{
  var o='',chars=ln.split(''),i=0;
  while(i<chars.length){{
    var c=chars[i];
    if(c==='"'){{var p=i;i++;while(i<chars.length&&chars[i]!=='"')i++;if(i<chars.length)i++;o+='<span class="syn-str">'+esc(chars.slice(p,i).join(''))+'</span>';}}
    else if(c==='/'&&chars[i+1]==='/'){{o+='<span class="syn-comment">'+esc(chars.slice(i).join(''))+'</span>';i=chars.length;}}
    else if(c==='-'&&chars[i+1]==='>'){{o+='<span class="syn-punct">-&gt;</span>';i+=2;}}
    else if(/[a-zA-Z_]/.test(c)){{var p2=i;while(i<chars.length&&(/[\w.]/.test(chars[i])||(chars[i]==='-'&&chars[i+1]!=='>')))i++;var wd=chars.slice(p2,i).join('');o+=(AGL_KW.indexOf(wd)>=0?'<span class="syn-key">'+esc(wd)+'</span>':esc(wd));}}
    else if('{{}}(),:'.indexOf(c)>=0){{o+='<span class="syn-punct">'+esc(c)+'</span>';i++;}}
    else{{o+=esc(c);i++;}}
  }}
  return o;
}}
function updateHighlight(){{
  var text=editor.value;
  if(FILE_EXT==='md'){{highlightPane.innerHTML=esc(text);return;}}
  highlightPane.innerHTML=text.split('\n').map(function(ln){{
    if(FILE_EXT==='yaml'||FILE_EXT==='yml')return hlYaml(ln);
    if(FILE_EXT==='json')return hlJson(ln);
    if(FILE_EXT==='agl')return hlAgl(ln);
    return esc(ln);
  }}).join('\n');
}}
editor.addEventListener('input',function(){{
  dirty=true;
  clearTimeout(debounce);
  debounce=setTimeout(postContent,400);
  if(hlRaf)cancelAnimationFrame(hlRaf);
  hlRaf=requestAnimationFrame(updateHighlight);
}});
editor.addEventListener('scroll',function(){{
  highlightPane.scrollTop=editor.scrollTop;
  highlightPane.scrollLeft=editor.scrollLeft;
}});
// Tab key inserts spaces
editor.addEventListener('keydown',function(e){{
  if(e.key==='Tab'){{
    e.preventDefault();
    var s=this.selectionStart,end=this.selectionEnd;
    this.value=this.value.substring(0,s)+'  '+this.value.substring(end);
    this.selectionStart=this.selectionEnd=s+2;
    this.dispatchEvent(new Event('input'));
  }}
}});
function postContent(){{
  dirty=false;
  var body=editor.value;
  var st=document.getElementById('status');
  st.textContent='saving…';st.className='status show';
  // Serialize posts so the last edit always lands last
  queue=queue.then(function(){{
    return fetch('/api/content',{{method:'POST',body:body}})
      .then(function(r){{return r.json()}})
      .then(function(j){{
        showSyntaxError(j.valid?null:j.error);
        st.textContent=j.valid?'saved':'saved · invalid {ext}';
        if(j.valid) setTimeout(function(){{st.className='status';}},1200);
      }})
      .catch(function(){{st.textContent='save failed';st.className='status show';}});
  }});
  return queue;
}}
function showSyntaxError(msg){{
  var e=document.getElementById('syntaxErr');
  if(!msg){{e.className='syntax-err';e.textContent='';return;}}
  e.textContent='invalid {ext}: '+msg;
  e.className='syntax-err show';
}}
function flash(msg){{
  var s=document.getElementById('status');
  s.textContent=msg;s.className='status show';
  setTimeout(function(){{s.className='status';}},900);
}}
function copyText(t,label){{
  if(!t)return;
  navigator.clipboard.writeText(t).then(function(){{flash(label);}}).catch(function(){{}});
}}
// Whole-file copy. Reads the raw text so gutter line numbers stay out of it.
function copyFile(){{
  fetch('/api/content').then(function(r){{return r.text()}})
    .then(function(t){{copyText(t,'copied file');}}).catch(function(){{}});
}}
// Writes the buffer to disk. Flushes any debounced POST first so the save
// includes the most recent keystroke.
function saveFile(){{
  (dirty?postContent():queue).then(function(){{
    return fetch('/api/save',{{method:'POST'}}).then(function(r){{return r.json()}});
  }}).then(function(j){{
    if(!j.ok){{flash('save failed: '+j.error);return;}}
    flash(j.saved?'saved to disk':'nothing to save');
    if(j.saved) refreshView();
  }}).catch(function(){{flash('save failed');}});
}}
document.addEventListener('keydown',function(e){{
  if((e.metaKey||e.ctrlKey)&&e.key==='s'){{e.preventDefault();saveFile();}}
}});
// Selecting text copies it, in the rendered view and the textarea both.
function copySelection(){{
  if(document.body.className==='editing') return;
  var t=String(window.getSelection()||'');
  if(!t.trim())return;
  copyText(t,'copied '+t.length+' chars');
}}
document.addEventListener('mouseup',copySelection);
document.addEventListener('keyup',function(e){{if(e.key==='Shift')copySelection();}});
// Flag a file that was already invalid when it loaded.
fetch('/api/status').then(function(r){{return r.json()}})
  .then(function(s){{showSyntaxError(s.valid?null:s.error);}}).catch(function(){{}});
</script>
<script>
var currentTheme={{color:'{cfg_color}',mode:'{cfg_mode}',texture:'{cfg_texture}',glow:'{cfg_glow}',accent_hex:{cfg_accent_hex_js}}};
function toggleThemePanel(){{
  document.getElementById('themePanel').classList.toggle('show');
}}
function applyTheme(patch){{
  for(var k in patch) currentTheme[k]=patch[k];
  fetch('/api/theme',{{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify(currentTheme)}})
    .then(function(r){{return r.json();}})
    .then(function(j){{ if(j.ok) location.reload(); else flash(j.error||'invalid theme'); }})
    .catch(function(){{flash('theme save failed');}});
}}
document.addEventListener('click',function(e){{
  var p=document.getElementById('themePanel'), b=document.getElementById('themeBtn');
  if(p && p.classList.contains('show') && !p.contains(e.target) && e.target!==b){{ p.classList.remove('show'); }}
}});
</script>
</body>
</html>"#,
        file_name = html_escape(file_name),
        ext = ext,
        rendered = rendered,
        banner = banner,
        container_wide = match ext {
            "yaml" | "yml" | "json" | "agl" => " wide",
            _ => "",
        },
        edit_escaped = html_escape(raw_for_edit),
        edit_highlighted = match ext {
            "yaml" | "yml" | "json" | "agl" => render_code_inline(raw_for_edit, ext),
            _ => html_escape(raw_for_edit),
        },
        theme_css = theme_css,
        syn_key = syn_key,
        syn_str = syn_str,
        syn_num = syn_num,
        syn_bool = syn_bool,
        syn_null = syn_null,
        swatches_html = swatches_html,
        mode_html = mode_html,
        texture_options = texture_options,
        glow_options = glow_options,
        hex_or_default = hex_or_default,
        hex_value = hex_value,
        cfg_color = cfg.color,
        cfg_mode = cfg.mode,
        cfg_texture = cfg.texture,
        cfg_glow = cfg.glow,
        cfg_accent_hex_js = cfg_accent_hex_js,
    )
}

/// Renders the theme panel's color swatches (curata's 8 rainbow accents
/// plus a neutral "Default"), marking whichever one matches `current`.
fn color_swatch_html(current: &str) -> String {
    let colors: &[(&str, &str, &str)] = &[
        ("default", "Default", "#899878"),
        ("red", "Red", "#BB7777"),
        ("orange", "Orange", "#BB8C66"),
        ("yellow", "Yellow", "#B8A866"),
        ("green", "Green", "#7A9878"),
        ("teal", "Teal", "#3CCECE"),
        ("blue", "Blue", "#7897B8"),
        ("indigo", "Indigo", "#8A7FBB"),
        ("violet", "Violet", "#AB7FBB"),
    ];
    let mut out = String::new();
    for (value, label, hex) in colors {
        let active = if *value == current { " active" } else { "" };
        out.push_str(&format!(
            r#"<button class="theme-swatch{active}" title="{label}" style="--swatch: {hex}" onclick="applyTheme({{color:'{value}',accent_hex:null}})"></button>"#,
        ));
    }
    out
}

fn mode_toggle_html(current: &str) -> String {
    let dark_cls = if current == "light" { "" } else { "active" };
    let light_cls = if current == "light" { "active" } else { "" };
    format!(
        r#"<button class="{dark_cls}" onclick="applyTheme({{mode:'dark'}})">Dark</button><button class="{light_cls}" onclick="applyTheme({{mode:'light'}})">Light</button>"#,
    )
}

fn select_options(options: &[(&str, &str)], current: &str) -> String {
    let mut out = String::new();
    for (value, label) in options {
        let selected = if *value == current { " selected" } else { "" };
        out.push_str(&format!(
            r#"<option value="{value}"{selected}>{label}</option>"#,
        ));
    }
    out
}

fn strip_frontmatter(src: &str) -> (&str, &str) {
    if !src.starts_with("---") {
        return ("", src);
    }
    let after_open = &src[3..];
    if let Some(close) = after_open.find("\n---") {
        let fm = after_open[..close].trim();
        let body = after_open[close + 4..].trim_start_matches('\n');
        (fm, body)
    } else {
        ("", src)
    }
}

fn render_markdown(src: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let (fm, body) = strip_frontmatter(src);
    let mut out = String::new();
    if !fm.is_empty() {
        out.push_str("<div class=\"frontmatter\">");
        for line in fm.lines() {
            out.push_str(&highlight_yaml(line));
            out.push('\n');
        }
        out.push_str("</div>");
    }
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let body = escape_spaced_image_paths(body);
    let parser = Parser::new_ext(&body, opts);
    html::push_html(&mut out, parser);
    out
}

/// CommonMark link destinations can't contain a raw space unless wrapped in
/// `<...>` - without that, pulldown_cmark treats `![x](my screenshot.png)`
/// as literal text, not an image. Screenshot tool filenames (CleanShot,
/// macOS's default, VS Code's paste-image) almost always have spaces, so
/// wrap any image destination that has one and isn't already bracketed.
fn escape_spaced_image_paths(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'!' && bytes.get(i + 1) == Some(&b'[') {
            if let Some(alt_end) = src[i + 2..].find(']') {
                let alt_end = i + 2 + alt_end;
                if src.as_bytes().get(alt_end + 1) == Some(&b'(') {
                    let dest_start = alt_end + 2;
                    if let Some(rel_close) = src[dest_start..].find(')') {
                        let dest_close = dest_start + rel_close;
                        let dest = &src[dest_start..dest_close];
                        let already_wrapped = dest.starts_with('<') && dest.ends_with('>');
                        let (path_part, title_part) = split_dest_title(dest);
                        if !already_wrapped && path_part.contains(' ') {
                            out.push_str(&src[i..dest_start]);
                            out.push('<');
                            out.push_str(path_part);
                            out.push('>');
                            out.push_str(title_part);
                            out.push(')');
                            i = dest_close + 1;
                            continue;
                        }
                    }
                }
            }
        }
        let ch = src[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Splits `path "title"` into (`path`, ` "title"`); no title returns
/// (`dest`, ``). Only recognizes a trailing double-quoted title, the
/// common case, so anything odder is left alone rather than mis-split.
fn split_dest_title(dest: &str) -> (&str, &str) {
    let trimmed = dest.trim_end();
    if trimmed.ends_with('"') {
        if let Some(space_quote) = trimmed.rfind(" \"") {
            return (&dest[..space_quote], &dest[space_quote..]);
        }
    }
    (dest, "")
}

fn render_code(src: &str, lang: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = String::from("<div class=\"code-view\">");
    for (i, line) in lines.iter().enumerate() {
        out.push_str(&format!(
            "<span class=\"ln\">{}</span>{}\n",
            i + 1,
            syntax_highlight(line, lang)
        ));
    }
    out.push_str("</div>");
    out
}

/// Same per-line highlighting as `render_code`, without the line-number
/// gutter or wrapping div - so the output lines up character-for-character
/// with a plain `<textarea>` behind it, for the edit-mode overlay.
fn render_code_inline(src: &str, lang: &str) -> String {
    src.lines()
        .map(|line| syntax_highlight(line, lang))
        .collect::<Vec<_>>()
        .join("\n")
}

fn syntax_highlight(line: &str, lang: &str) -> String {
    match lang {
        "yaml" | "yml" => highlight_yaml(line),
        "json" => highlight_json(line),
        "agl" => highlight_agl(line),
        _ => html_escape(line),
    }
}

/// Every bare keyword in the Agent Graph Language (`.agl`) grammar - see
/// `kazam`'s `src/agl/parser.rs` on the `claude/agl-imports-mcp-skills`
/// branch (unmerged as of this writing). Kept as a flat list here rather
/// than importing that module: `.agl` doesn't exist on `main` yet, and
/// this view is cosmetic only - `kazam agl validate` is the real parser.
const AGL_KEYWORDS: &[&str] = &[
    "spec",
    "in",
    "out",
    "requires",
    "skill",
    "cache",
    "invariant",
    "flow",
    "state",
    "branch",
    "if",
    "deny",
    "without",
    "gate",
    "import",
    "call",
    "map",
    "evaluate",
    "fan",
    "watch",
    "TERMINATE",
    "next",
];

fn highlight_agl(line: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            if i < chars.len() {
                i += 1; // consume the closing quote
            }
            let s: String = chars[start..i].iter().collect();
            out.push_str(&format!(
                "<span class=\"syn-str\">{}</span>",
                html_escape(&s)
            ));
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            let comment: String = chars[i..].iter().collect();
            out.push_str(&format!(
                "<span class=\"syn-comment\">{}</span>",
                html_escape(&comment)
            ));
            i = chars.len();
        } else if c == '-' && chars.get(i + 1) == Some(&'>') {
            out.push_str("<span class=\"syn-punct\">-&gt;</span>");
            i += 2;
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            // Same lookahead the real lexer uses: a '-' that's actually the
            // start of an immediately-following "->" doesn't get absorbed,
            // so `next->TERMINATE(...)` (no space) still shows an arrow.
            while i < chars.len() {
                let ch = chars[i];
                if !(ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '-') {
                    break;
                }
                if ch == '-' && chars.get(i + 1) == Some(&'>') {
                    break;
                }
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if AGL_KEYWORDS.contains(&word.as_str()) {
                out.push_str(&format!(
                    "<span class=\"syn-key\">{}</span>",
                    html_escape(&word)
                ));
            } else {
                out.push_str(&html_escape(&word));
            }
        } else if "{}(),:".contains(c) {
            out.push_str(&format!(
                "<span class=\"syn-punct\">{}</span>",
                html_escape(&c.to_string())
            ));
            i += 1;
        } else {
            out.push_str(&html_escape(&c.to_string()));
            i += 1;
        }
    }
    out
}

fn highlight_yaml(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return format!(
            "{}<span class=\"syn-comment\">{}</span>",
            leading_space(line),
            html_escape(trimmed)
        );
    }
    if let Some(colon_pos) = trimmed.find(':') {
        let key = &trimmed[..colon_pos];
        let rest = &trimmed[colon_pos + 1..];
        let val = colorize_value(rest.trim());
        format!(
            "{}<span class=\"syn-key\">{}</span><span class=\"syn-punct\">:</span> {}",
            leading_space(line),
            html_escape(key),
            val
        )
    } else if let Some(item) = trimmed.strip_prefix("- ") {
        let val = colorize_value(item.trim());
        format!(
            "{}<span class=\"syn-punct\">-</span> {}",
            leading_space(line),
            val
        )
    } else {
        format!("{}{}", leading_space(line), html_escape(trimmed))
    }
}

fn highlight_json(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            '"' => {
                let mut s = String::new();
                s.push(chars.next().unwrap());
                let mut escaped = false;
                for ch in chars.by_ref() {
                    s.push(ch);
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        break;
                    }
                }
                // Check if this is a key (followed by colon)
                let remaining: String = chars.clone().collect();
                let is_key = remaining.trim_start().starts_with(':');
                if is_key {
                    out.push_str(&format!(
                        "<span class=\"syn-key\">{}</span>",
                        html_escape(&s)
                    ));
                } else {
                    out.push_str(&format!(
                        "<span class=\"syn-str\">{}</span>",
                        html_escape(&s)
                    ));
                }
            }
            '0'..='9' | '-' => {
                let mut num = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_digit()
                        || ch == '.'
                        || ch == '-'
                        || ch == 'e'
                        || ch == 'E'
                        || ch == '+'
                    {
                        num.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                out.push_str(&format!(
                    "<span class=\"syn-num\">{}</span>",
                    html_escape(&num)
                ));
            }
            // Peek-driven, not take_while: take_while would swallow the
            // delimiter that ends the word (dropping trailing commas).
            't' | 'f' | 'n' => {
                let mut word = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_alphabetic() {
                        word.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                match word.as_str() {
                    "true" | "false" => {
                        out.push_str(&format!("<span class=\"syn-bool\">{word}</span>"))
                    }
                    "null" => out.push_str(&format!("<span class=\"syn-null\">{word}</span>")),
                    _ => out.push_str(&html_escape(&word)),
                }
            }
            '{' | '}' | '[' | ']' | ':' | ',' => {
                chars.next();
                out.push_str(&format!("<span class=\"syn-punct\">{}</span>", c));
            }
            _ => {
                out.push(chars.next().unwrap());
            }
        }
    }
    out
}

fn colorize_value(val: &str) -> String {
    if val.is_empty() {
        return String::new();
    }
    if val == "true" || val == "false" {
        return format!("<span class=\"syn-bool\">{val}</span>");
    }
    if val == "null" || val == "~" {
        return format!("<span class=\"syn-null\">{val}</span>");
    }
    if val.starts_with('"') || val.starts_with('\'') {
        return format!("<span class=\"syn-str\">{}</span>", html_escape(val));
    }
    if val.parse::<f64>().is_ok() {
        return format!("<span class=\"syn-num\">{val}</span>");
    }
    html_escape(val)
}

fn leading_space(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    " ".repeat(indent)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Parse error for the format, or None if it parses (or has no parser).
fn syntax_error(content: &str, ext: &str) -> Option<String> {
    match ext {
        "yaml" | "yml" => serde_yaml::from_str::<serde_yaml::Value>(content)
            .err()
            .map(|e| e.to_string()),
        "json" => serde_json::from_str::<serde_json::Value>(content)
            .err()
            .map(|e| e.to_string()),
        _ => None,
    }
}

fn validate_content(content: &str, ext: &str) -> Result<()> {
    if let Some(e) = syntax_error(content, ext) {
        let label = if ext == "json" { "JSON" } else { "YAML" };
        eprintln!("  ⚠ invalid {label}: {e}");
        eprintln!("  opening anyway - fix the syntax and the view will reload\n");
    }
    Ok(())
}

fn watch_file(path: PathBuf, st: Arc<State>) {
    let watch_dir = path.parent().unwrap_or(Path::new("."));
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("watcher init failed: {e}");
            return;
        }
    };
    if let Err(e) = watcher.watch(watch_dir, RecursiveMode::NonRecursive) {
        eprintln!("watch failed: {e}");
        return;
    }

    let mut last_reload = Instant::now() - Duration::from_secs(10);

    for event in rx {
        let Ok(event) = event else { continue };
        let relevant = event.paths.iter().any(|p| p == &path);
        if !relevant {
            continue;
        }
        if last_reload.elapsed() < Duration::from_millis(200) {
            continue;
        }
        last_reload = Instant::now();

        match std::fs::read_to_string(&path) {
            Ok(new_content) => {
                // Never discard unsaved browser edits. If the file moved
                // underneath one, flag a conflict and let the human pick.
                // Our own save already adopted this content, so there is
                // nothing to tell the browser about.
                if *st.disk.read().unwrap() == new_content {
                    continue;
                }

                let dirty = st.is_dirty();
                let same = *st.buffer.read().unwrap() == Some(new_content.clone());
                *st.disk.write().unwrap() = new_content;
                if dirty && !same {
                    st.conflict.store(true, Ordering::SeqCst);
                    println!("  ⚠ file changed on disk - you have unsaved edits");
                } else {
                    // Buffer matched disk, so it is no longer an unsaved edit.
                    if same {
                        *st.buffer.write().unwrap() = None;
                    }
                    println!("  ↻ reloaded");
                }
                st.version.fetch_add(1, Ordering::SeqCst);
            }
            Err(e) => {
                eprintln!("  read failed: {e}");
            }
        }
    }
}

#[cfg(test)]
mod theme_config_tests {
    use super::*;

    #[test]
    fn is_valid_hex_color_accepts_with_or_without_hash() {
        assert!(is_valid_hex_color("#14b8a6"));
        assert!(is_valid_hex_color("14b8a6"));
        assert!(is_valid_hex_color("ABCDEF"));
    }

    #[test]
    fn is_valid_hex_color_rejects_bad_input() {
        assert!(!is_valid_hex_color("#14b8a"));
        assert!(!is_valid_hex_color("#14b8a6f"));
        assert!(!is_valid_hex_color("teal"));
        assert!(!is_valid_hex_color("#gggggg"));
        assert!(!is_valid_hex_color(""));
    }

    #[test]
    fn parse_texture_falls_back_to_none_for_unknown_value() {
        assert_eq!(parse_texture("dots"), Texture::Dots);
        assert_eq!(parse_texture("bogus"), Texture::None);
    }

    #[test]
    fn parse_glow_falls_back_to_none_for_unknown_value() {
        assert_eq!(parse_glow("corner"), Glow::Corner);
        assert_eq!(parse_glow("bogus"), Glow::None);
    }

    #[test]
    fn resolve_theme_default_color_picks_neutral_base_for_mode() {
        let dark_default = OpenThemeConfig {
            color: "default".into(),
            mode: "dark".into(),
            ..Default::default()
        };
        let light_default = OpenThemeConfig {
            color: "default".into(),
            mode: "light".into(),
            ..Default::default()
        };
        let (dark_theme, _, _) = resolve_theme(&dark_default);
        let (light_theme, _, _) = resolve_theme(&light_default);
        // dark()/light() bases have different bg values; "default" + mode
        // should resolve to the self-contained base, not a rainbow accent.
        assert_ne!(dark_theme.bg, light_theme.bg);
    }

    #[test]
    fn resolve_theme_rainbow_color_ignores_mode_for_the_accent_itself() {
        let cfg = OpenThemeConfig {
            color: "violet".into(),
            mode: "dark".into(),
            ..Default::default()
        };
        let (theme, _, _) = resolve_theme(&cfg);
        assert_eq!(theme.accent, "#AB7FBB");
    }

    #[test]
    fn resolve_theme_custom_hex_overrides_the_named_accent() {
        let cfg = OpenThemeConfig {
            color: "violet".into(),
            mode: "dark".into(),
            accent_hex: Some("#123456".into()),
            ..Default::default()
        };
        let (theme, _, _) = resolve_theme(&cfg);
        assert_eq!(theme.accent, "#123456");
    }

    #[test]
    fn resolve_theme_texture_and_glow_pass_through() {
        let cfg = OpenThemeConfig {
            texture: "grid".into(),
            glow: "corner".into(),
            ..Default::default()
        };
        let (_, texture, glow) = resolve_theme(&cfg);
        assert_eq!(texture, Texture::Grid);
        assert_eq!(glow, Glow::Corner);
    }

    #[test]
    fn default_config_has_sane_fallbacks() {
        let cfg = OpenThemeConfig::default();
        assert_eq!(cfg.color, "teal");
        assert_eq!(cfg.mode, "dark");
        assert_eq!(cfg.texture, "dots");
        assert_eq!(cfg.glow, "none");
        assert!(cfg.accent_hex.is_none());
    }
}

#[cfg(test)]
mod image_asset_tests {
    use super::*;

    #[test]
    fn escapes_spaced_image_path() {
        let out = escape_spaced_image_paths("![shot](CleanShot 2026-08-10 at 15.19.44.png)");
        assert_eq!(out, "![shot](<CleanShot 2026-08-10 at 15.19.44.png>)");
    }

    #[test]
    fn leaves_unspaced_image_path_alone() {
        let src = "![shot](shot.png)";
        assert_eq!(escape_spaced_image_paths(src), src);
    }

    #[test]
    fn leaves_already_bracketed_path_alone() {
        let src = "![shot](<my shot.png>)";
        assert_eq!(escape_spaced_image_paths(src), src);
    }

    #[test]
    fn preserves_title_when_wrapping() {
        let out = escape_spaced_image_paths(r#"![shot](my shot.png "a title")"#);
        assert_eq!(out, r#"![shot](<my shot.png> "a title")"#);
    }

    #[test]
    fn does_not_touch_plain_links() {
        let src = "[a link](some page.html)";
        assert_eq!(escape_spaced_image_paths(src), src);
    }

    #[test]
    fn preserves_non_ascii_text_around_it() {
        let src = "café ☕ ![shot](my shot.png) 日本語";
        let out = escape_spaced_image_paths(src);
        assert_eq!(out, "café ☕ ![shot](<my shot.png>) 日本語");
    }

    #[test]
    fn render_markdown_produces_img_tag_for_spaced_path() {
        let html = render_markdown("![shot](my shot.png)");
        assert!(html.contains(r#"<img src="my%20shot.png" alt="shot""#));
    }

    #[test]
    fn is_image_asset_matches_known_extensions() {
        assert!(is_image_asset("/shot.png"));
        assert!(is_image_asset("/dir/shot.JPG"));
        assert!(!is_image_asset("/notes.md"));
        assert!(!is_image_asset("/api/content"));
    }

    #[test]
    fn percent_decode_handles_spaces() {
        assert_eq!(percent_decode("my%20shot.png"), "my shot.png");
        assert_eq!(percent_decode("no-escapes.png"), "no-escapes.png");
    }
}

#[cfg(test)]
mod agl_highlight_tests {
    use super::*;

    #[test]
    fn highlights_keywords() {
        let out = highlight_agl("spec Foo {");
        assert!(out.contains("<span class=\"syn-key\">spec</span>"));
        assert!(!out.contains("<span class=\"syn-key\">Foo</span>"));
    }

    #[test]
    fn highlights_string_literals() {
        let out = highlight_agl(r#"call(Bash, customer, "https://example.com")"#);
        assert!(out.contains("<span class=\"syn-str\">&quot;https://example.com&quot;</span>"));
    }

    #[test]
    fn highlights_line_comments() {
        let out = highlight_agl("// a comment");
        assert!(out.contains("<span class=\"syn-comment\">// a comment</span>"));
    }

    #[test]
    fn does_not_treat_a_url_slash_slash_as_a_comment() {
        // The comment-scanner only fires at top level, but a string
        // literal's contents are consumed by the string-scanning branch
        // first, so a URL's "//" inside quotes must never start a comment.
        let out = highlight_agl(r#"state A -> call(Bash, "https://x") -> next"#);
        assert!(!out.contains("syn-comment"));
    }

    #[test]
    fn no_space_arrow_after_a_word_still_highlights_as_an_arrow() {
        // Same lookahead the real lexer needed (kz-e243): a '-' that's
        // actually the start of "->" must not get absorbed into the
        // preceding word, even with no space before it.
        let out = highlight_agl(r#"next->TERMINATE("done")"#);
        assert!(out.contains("<span class=\"syn-punct\">-&gt;</span>"));
        assert!(out.contains("<span class=\"syn-key\">next</span>"));
        assert!(out.contains("<span class=\"syn-key\">TERMINATE</span>"));
    }

    #[test]
    fn hyphenated_identifiers_are_not_split() {
        let out = highlight_agl("cache slack-lookups {");
        assert!(out.contains("slack-lookups"));
        assert!(!out.contains("slack</span>-<span"));
    }

    #[test]
    fn highlights_fan_and_watch_keywords() {
        let fan = highlight_agl(r#"state SCAN -> fan(WorkflowDeal, targets) -> next"#);
        assert!(fan.contains("<span class=\"syn-key\">fan</span>"));
        assert!(!fan.contains("<span class=\"syn-key\">WorkflowDeal</span>"));

        let watch = highlight_agl("state BUILD -> watch(ci status) -> next");
        assert!(watch.contains("<span class=\"syn-key\">watch</span>"));
    }
}

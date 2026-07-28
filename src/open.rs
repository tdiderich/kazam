use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{RecursiveMode, Watcher};

use crate::server;

pub fn run(path: &Path, port: u16) -> Result<()> {
    let path = std::fs::canonicalize(path)
        .with_context(|| format!("cannot resolve: {}", path.display()))?;

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "md" | "yaml" | "yml" | "json" => {}
        _ => anyhow::bail!(
            "unsupported format '.{ext}' — kazam open supports .md, .yaml, .yml, .json"
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
        println!("\n  ⚠ port {port} is in use — serving on {actual_port} instead");
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

        // Raw text — what an agent reads. Unsaved browser edits win.
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

        (_, false, false) => server::respond_405(req),
        _ => server::respond_404(req),
    }
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

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{file_name} — kazam</title>
<style>
:root {{
  --bg: #0f1117;
  --fg: #e4e4e7;
  --muted: #71717a;
  --border: #27272a;
  --accent: #14b8a6;
  --surface: #18181b;
  --code-bg: #1e1e22;
  --selection: rgba(20,184,166,0.25);
}}
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{
  background: var(--bg);
  color: var(--fg);
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  line-height: 1.6;
  min-height: 100vh;
}}
.toolbar {{
  position: sticky;
  top: 0;
  z-index: 10;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  padding: 8px 24px;
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
}}
.toolbar .filename {{
  font-weight: 600;
  color: var(--accent);
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
}}
.toolbar .badge {{
  background: var(--border);
  color: var(--muted);
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}}
.toolbar .spacer {{ flex: 1; }}
.toolbar button {{
  background: var(--border);
  color: var(--fg);
  border: none;
  padding: 4px 12px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 13px;
  font-family: inherit;
}}
.toolbar button:hover {{ background: var(--accent); color: #000; }}
.toolbar button.active {{ background: var(--accent); color: #000; }}
.container {{
  max-width: 820px;
  margin: 0 auto;
  padding: 32px 24px;
}}
.view-mode {{ display: block; }}
.edit-mode {{ display: none; }}
body.editing .view-mode {{ display: none; }}
body.editing .edit-mode {{ display: block; }}
/* Markdown styles */
.markdown h1 {{ font-size: 1.8em; margin: 0.8em 0 0.4em; font-weight: 700; }}
.markdown h2 {{ font-size: 1.4em; margin: 0.8em 0 0.4em; font-weight: 600; }}
.markdown h3 {{ font-size: 1.15em; margin: 0.8em 0 0.4em; font-weight: 600; }}
.markdown p {{ margin: 0.6em 0; }}
.markdown ul, .markdown ol {{ margin: 0.6em 0; padding-left: 1.5em; }}
.markdown li {{ margin: 0.2em 0; }}
.markdown code {{
  background: var(--code-bg);
  padding: 2px 6px;
  border-radius: 3px;
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
  font-size: 0.9em;
}}
.markdown pre {{
  background: var(--code-bg);
  padding: 16px;
  border-radius: 6px;
  overflow-x: auto;
  margin: 0.8em 0;
}}
.markdown pre code {{
  background: none;
  padding: 0;
}}
.markdown blockquote {{
  border-left: 3px solid var(--accent);
  padding-left: 16px;
  color: var(--muted);
  margin: 0.8em 0;
}}
.markdown a {{ color: var(--accent); text-decoration: none; }}
.markdown a:hover {{ text-decoration: underline; }}
.markdown table {{
  border-collapse: collapse;
  width: 100%;
  margin: 0.8em 0;
}}
.markdown th, .markdown td {{
  border: 1px solid var(--border);
  padding: 8px 12px;
  text-align: left;
}}
.markdown th {{ background: var(--surface); font-weight: 600; }}
.markdown hr {{ border: none; border-top: 1px solid var(--border); margin: 1.5em 0; }}
/* Code view (yaml/json) */
.code-view {{
  background: var(--code-bg);
  padding: 20px;
  border-radius: 6px;
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
/* Syntax colors */
.syn-key {{ color: #7dd3fc; }}
.syn-str {{ color: #86efac; }}
.syn-num {{ color: #fde68a; }}
.syn-bool {{ color: #c4b5fd; }}
.syn-null {{ color: #f87171; }}
.syn-comment {{ color: var(--muted); font-style: italic; }}
.syn-punct {{ color: var(--muted); }}
/* Edit textarea */
.edit-area {{
  width: 100%;
  min-height: calc(100vh - 120px);
  background: var(--code-bg);
  color: var(--fg);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 20px;
  font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace;
  font-size: 14px;
  line-height: 1.5;
  tab-size: 2;
  resize: vertical;
  outline: none;
}}
.edit-area:focus {{
  border-color: var(--accent);
}}
.status {{
  position: fixed;
  bottom: 16px;
  right: 16px;
  background: var(--surface);
  border: 1px solid var(--border);
  padding: 4px 12px;
  border-radius: 4px;
  font-size: 12px;
  color: var(--muted);
  opacity: 0;
  transition: opacity 0.2s;
}}
.status.show {{ opacity: 1; }}
::selection {{ background: var(--selection); }}
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
</style>
</head>
<body>
<div class="toolbar">
  <span class="filename">{file_name}</span>
  <span class="badge">{ext}</span>
  <span class="spacer"></span>
  <button id="viewBtn" class="active" onclick="setMode('view')">View</button>
  <button id="editBtn" onclick="setMode('edit')">Edit</button>
  <button id="copyBtn" onclick="copyFile()">Copy</button>
</div>
{banner}
<div class="syntax-err" id="syntaxErr"></div>
<div class="container">
  <div class="view-mode markdown" id="viewPane">{rendered}</div>
  <div class="edit-mode">
    <textarea class="edit-area" id="editor" spellcheck="false">{edit_escaped}</textarea>
  </div>
</div>
<div class="status" id="status"></div>
<script>
// Poll for disk changes. Never reload over unsaved edits — surface the
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
var debounce=null;
editor.addEventListener('input',function(){{
  dirty=true;
  clearTimeout(debounce);
  debounce=setTimeout(postContent,400);
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
// Selecting text copies it, in the rendered view and the textarea both.
function copySelection(){{
  var a=document.activeElement, t='';
  if(a&&a.id==='editor') t=a.value.substring(a.selectionStart,a.selectionEnd);
  else t=String(window.getSelection()||'');
  if(!t.trim())return;
  copyText(t,'copied '+t.length+' chars');
}}
document.addEventListener('mouseup',copySelection);
document.addEventListener('keyup',function(e){{if(e.key==='Shift')copySelection();}});
// Flag a file that was already invalid when it loaded.
fetch('/api/status').then(function(r){{return r.json()}})
  .then(function(s){{showSyntaxError(s.valid?null:s.error);}}).catch(function(){{}});
</script>
</body>
</html>"#,
        file_name = html_escape(file_name),
        ext = ext,
        rendered = rendered,
        banner = banner,
        edit_escaped = html_escape(raw_for_edit),
    )
}

fn render_markdown(src: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let parser = Parser::new_ext(src, opts);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
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

fn syntax_highlight(line: &str, lang: &str) -> String {
    match lang {
        "yaml" | "yml" => highlight_yaml(line),
        "json" => highlight_json(line),
        _ => html_escape(line),
    }
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
    } else if trimmed.starts_with("- ") {
        let val = colorize_value(trimmed[2..].trim());
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
        eprintln!("  opening anyway — fix the syntax and the view will reload\n");
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
                let dirty = st.is_dirty();
                let same = *st.buffer.read().unwrap() == Some(new_content.clone());
                *st.disk.write().unwrap() = new_content;
                if dirty && !same {
                    st.conflict.store(true, Ordering::SeqCst);
                    println!("  ⚠ file changed on disk — you have unsaved edits");
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

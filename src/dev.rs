use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{Event, RecursiveMode, Watcher};
use tiny_http::{Header, Method, Response, Server};

use crate::build;

/// How many ports to walk past the requested one before giving up. A run
/// of 10 covers typical dev-server clustering (ports 3000-3009) without
/// silently drifting to something surprising.
const PORT_FALLBACK_ATTEMPTS: u16 = 10;

/// Try to bind `0.0.0.0:<port>`, walking forward on conflict. Returns the
/// live server and the port it actually bound to.
fn bind_next_available(start: u16) -> Result<(Server, u16)> {
    use std::net::TcpListener;
    let mut last_err: Option<String> = None;
    for p in start..start.saturating_add(PORT_FALLBACK_ATTEMPTS) {
        let addr = format!("0.0.0.0:{p}");
        if TcpListener::bind(&addr).is_err() {
            last_err = Some(format!("port {p} already in use"));
            continue;
        }
        match Server::http(&addr) {
            Ok(s) => return Ok((s, p)),
            Err(e) => last_err = Some(e.to_string()),
        }
    }
    let tail = start
        .saturating_add(PORT_FALLBACK_ATTEMPTS)
        .saturating_sub(1);
    anyhow::bail!(
        "no free port in range {start}..={tail} — last error: {}",
        last_err.unwrap_or_else(|| "unknown".to_string())
    );
}

pub fn run(dir: &Path, out: &Path, port: u16) -> Result<()> {
    // Dev builds silence the orphan check — work-in-progress pages show up
    // as orphans until you wire them into nav, and that'd be noisy.
    build::run(dir, out, false, true, false, false)?;

    let version = Arc::new(AtomicU64::new(1));

    // ── Watcher thread ────────────────────────────
    let dir_clone = dir.to_path_buf();
    let out_clone = out.to_path_buf();
    let ver_clone = version.clone();
    thread::spawn(move || watch_loop(dir_clone, out_clone, ver_clone));

    // ── HTTP server ───────────────────────────────
    // Try the requested port first; if it's in use, walk forward up to
    // PORT_FALLBACK_ATTEMPTS before giving up. Matches Vite / Next.js /
    // Parcel UX — a port being busy shouldn't kill the dev loop.
    let (server, actual_port) = bind_next_available(port)?;

    if actual_port != port {
        println!("\n  ⚠ port {port} is in use — serving on {actual_port} instead");
    }
    println!("\n  ➜ http://localhost:{actual_port}");
    println!("  watching {}\n", dir.display());

    for req in server.incoming_requests() {
        let out_path = out.to_path_buf();
        let ver = version.clone();
        thread::spawn(move || {
            if let Err(e) = handle(req, &out_path, &ver) {
                eprintln!("  request error: {e}");
            }
        });
    }
    Ok(())
}

// ── File watcher ─────────────────────────────────

fn watch_loop(dir: PathBuf, out: PathBuf, version: Arc<AtomicU64>) {
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("watcher init failed: {e}");
            return;
        }
    };
    if let Err(e) = watcher.watch(&dir, RecursiveMode::Recursive) {
        eprintln!("watch failed: {e}");
        return;
    }

    // notify emits absolute paths. `out` is usually relative (e.g. "_site"
    // or "docs/_site"), so a direct `starts_with(&out)` never matches and
    // every rebuild re-triggers itself in an infinite loop. Canonicalize
    // out once up front so the comparison actually works. If canonicalize
    // fails (dir doesn't exist yet), fall back to current_dir().join(out).
    let out_abs = out
        .canonicalize()
        .or_else(|_| std::env::current_dir().map(|cwd| cwd.join(&out)))
        .unwrap_or_else(|_| out.clone());

    let mut last_build = Instant::now() - Duration::from_secs(10);

    for event in rx {
        let Ok(event) = event else { continue };
        let relevant = event.paths.iter().any(|p| {
            // Ignore anything inside the output directory — both the
            // configured `out` and any nested `_site` (e.g. a previously
            // built sub-site). Otherwise every rebuild retriggers itself.
            if p.starts_with(&out_abs) || p.components().any(|c| c.as_os_str() == "_site") {
                return false;
            }
            p.extension().map(|e| e == "yaml").unwrap_or(false)
                || p.file_name().map(|n| n == "kazam.yaml").unwrap_or(false)
        });
        if !relevant {
            continue;
        }
        if last_build.elapsed() < Duration::from_millis(150) {
            continue;
        }
        last_build = Instant::now();

        print!("  rebuild…");
        match build::run(&dir, &out, false, true, false, false) {
            Ok(_) => {
                version.fetch_add(1, Ordering::SeqCst);
                println!(" ✓");
            }
            Err(e) => {
                println!(" ✗\n    {e:#}");
            }
        }
    }
}

// ── Request handler ──────────────────────────────

fn handle(req: tiny_http::Request, root: &Path, version: &AtomicU64) -> Result<()> {
    if req.method() != &Method::Get {
        return req
            .respond(Response::from_string("method not allowed").with_status_code(405))
            .context("respond");
    }

    let url = req.url().split('?').next().unwrap_or("/");

    // Live-reload version endpoint
    if url == "/__kazam_version__" {
        let v = version.load(Ordering::SeqCst).to_string();
        let resp = Response::from_string(v)
            .with_header(hdr("Content-Type", "text/plain"))
            .with_header(hdr("Cache-Control", "no-store"));
        return req.respond(resp).context("respond");
    }

    let rel = url.trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };

    // Prevent escape
    if rel.contains("..") {
        return req
            .respond(Response::from_string("bad path").with_status_code(400))
            .context("respond");
    }

    let mut path = root.join(rel);
    if path.is_dir() {
        path = path.join("index.html");
    }

    match std::fs::read(&path) {
        Ok(data) => {
            let ct = content_type(&path);
            let resp = Response::from_data(data)
                .with_header(hdr("Content-Type", ct))
                .with_header(hdr("Cache-Control", "no-store"));
            req.respond(resp).context("respond")
        }
        Err(_) => {
            let not_found_path = root.join("404.html");
            if not_found_path.exists() {
                match std::fs::read(&not_found_path) {
                    Ok(data) => {
                        let resp = Response::from_data(data)
                            .with_header(hdr("Content-Type", "text/html; charset=utf-8"))
                            .with_header(hdr("Cache-Control", "no-store"))
                            .with_status_code(404);
                        req.respond(resp).context("respond")
                    }
                    Err(_) => {
                        let body = format!("404 — not found: {rel}");
                        let resp = Response::from_string(body).with_status_code(404);
                        req.respond(resp).context("respond")
                    }
                }
            } else {
                let body = format!("404 — not found: {rel}");
                let resp = Response::from_string(body).with_status_code(404);
                req.respond(resp).context("respond")
            }
        }
    }
}

fn hdr(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Bind a free port the kernel picks, then ask for the same port.
    /// bind_next_available should fall back to the next open one.
    #[test]
    fn bind_next_available_falls_back_on_conflict() {
        // Grab an ephemeral port from the OS.
        let squatter = TcpListener::bind("0.0.0.0:0").expect("bind ephemeral");
        let taken = squatter.local_addr().unwrap().port();

        let (server, got) = bind_next_available(taken).expect("fallback bind");
        assert_ne!(got, taken, "expected to walk forward past the taken port");
        assert!(
            got > taken && got <= taken.saturating_add(PORT_FALLBACK_ATTEMPTS),
            "fell back to {got}, expected {}..={}",
            taken + 1,
            taken.saturating_add(PORT_FALLBACK_ATTEMPTS)
        );

        // Tidy up — drop the server and the squatter explicitly so the
        // sockets close before the test exits.
        drop(server);
        drop(squatter);
    }

    #[test]
    fn bind_next_available_succeeds_when_port_free() {
        // High ephemeral-range start — very unlikely to be taken.
        let start = 49_999;
        let (server, got) = bind_next_available(start).expect("bind free port");
        assert_eq!(got, start);
        drop(server);
    }
}

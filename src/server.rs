use anyhow::{Context, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use tiny_http::{Header, Method, Response, Server};

pub fn bind_next_available(start: u16) -> Result<(Server, u16)> {
    use std::net::TcpListener;

    // Try the requested port first
    let addr = format!("0.0.0.0:{start}");
    if TcpListener::bind(&addr).is_ok() {
        if let Ok(s) = Server::http(&addr) {
            return Ok((s, start));
        }
    }

    // Fall back to OS-assigned random port
    let listener = TcpListener::bind("0.0.0.0:0").context("bind random port")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let addr = format!("0.0.0.0:{port}");
    let s = Server::http(&addr).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok((s, port))
}

pub fn install_shutdown_handler() {
    ctrlc_handler(|| {
        println!("\n  ✓ shutting down");
        std::process::exit(0);
    });
}

fn ctrlc_handler<F: Fn() + Send + 'static>(f: F) {
    unsafe {
        HANDLER = Some(Box::new(f));
        libc_signal(SIGINT, signal_trampoline as *const () as usize);
    }
}

static mut HANDLER: Option<Box<dyn Fn() + Send>> = None;
const SIGINT: i32 = 2;

extern "C" {
    fn signal(sig: i32, handler: usize) -> usize;
}

use std::sync::atomic::AtomicBool;
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

extern "C" fn signal_trampoline(_sig: i32) {
    if SHUTTING_DOWN.swap(true, Ordering::SeqCst) {
        std::process::exit(1);
    }
    unsafe {
        if let Some(ref f) = HANDLER {
            f();
        }
    }
}

fn libc_signal(sig: i32, handler: usize) {
    unsafe {
        signal(sig, handler);
    }
}

pub fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    let _ = std::process::Command::new(cmd)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

pub fn hdr(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

pub fn respond_version(req: tiny_http::Request, version: &AtomicU64) -> Result<()> {
    let v = version.load(Ordering::SeqCst).to_string();
    let resp = Response::from_string(v)
        .with_header(hdr("Content-Type", "text/plain"))
        .with_header(hdr("Cache-Control", "no-store"));
    req.respond(resp).context("respond")
}

pub fn respond_html(req: tiny_http::Request, html: &str) -> Result<()> {
    let resp = Response::from_string(html)
        .with_header(hdr("Content-Type", "text/html; charset=utf-8"))
        .with_header(hdr("Cache-Control", "no-store"));
    req.respond(resp).context("respond")
}

pub fn respond_plain(req: tiny_http::Request, text: &str) -> Result<()> {
    let resp = Response::from_string(text)
        .with_header(hdr("Content-Type", "text/plain; charset=utf-8"))
        .with_header(hdr("Cache-Control", "no-store"));
    req.respond(resp).context("respond")
}

pub fn respond_404(req: tiny_http::Request) -> Result<()> {
    req.respond(Response::from_string("404").with_status_code(404))
        .context("respond")
}

pub fn respond_405(req: tiny_http::Request) -> Result<()> {
    req.respond(Response::from_string("method not allowed").with_status_code(405))
        .context("respond")
}

pub fn is_get(req: &tiny_http::Request) -> bool {
    req.method() == &Method::Get
}

pub fn is_post(req: &tiny_http::Request) -> bool {
    req.method() == &Method::Post
}

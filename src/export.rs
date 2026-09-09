//! `kazam export pdf`: build one page and print it to PDF with headless
//! Chrome. Uses the same `@media print` rules the browser "Download PDF"
//! button does, so the CLI output and a manual print match exactly.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate the Chrome binary: explicit flag, `KAZAM_CHROME`, then well-known
/// install paths, then anything on PATH.
pub fn find_chrome(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        bail!("--chrome path does not exist: {}", p.display());
    }
    if let Ok(env) = std::env::var("KAZAM_CHROME") {
        let p = PathBuf::from(env);
        if p.exists() {
            return Ok(p);
        }
    }
    let candidates = [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Ok(p);
        }
    }
    for name in ["google-chrome", "chromium", "chrome"] {
        if let Ok(out) = Command::new("which").arg(name).output() {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    return Ok(PathBuf::from(s));
                }
            }
        }
    }
    bail!(
        "no Chrome found. Install Google Chrome or Chromium, or pass --chrome <path> \
         (or set KAZAM_CHROME)."
    )
}

/// Walk up from `page` until a directory holding `kazam.yaml` is found.
/// Falls back to the page's own directory when no config exists, matching
/// how `kazam build` treats a bare folder of pages.
pub fn site_root(page: &Path) -> Result<PathBuf> {
    let abs = page
        .canonicalize()
        .with_context(|| format!("resolving {}", page.display()))?;
    let mut dir = abs
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let start = dir.clone();
    loop {
        if dir.join("kazam.yaml").exists() {
            return Ok(dir);
        }
        match dir.parent() {
            Some(p) => dir = p.to_path_buf(),
            None => return Ok(start),
        }
    }
}

pub fn run_pdf(page: &Path, out: &Path, chrome: Option<&Path>, quiet: bool) -> Result<()> {
    if page.extension().and_then(|e| e.to_str()) != Some("yaml") {
        bail!("export pdf expects a .yaml page, got {}", page.display());
    }
    let chrome = find_chrome(chrome)?;
    let root = site_root(page)?;
    let page_abs = page.canonicalize()?;
    let rel = page_abs
        .strip_prefix(&root)
        .with_context(|| "page is outside its site root")?;

    let tmp = std::env::temp_dir().join(format!("kazam-export-{}", std::process::id()));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;

    crate::build::run(&root, &tmp, false, true, false, true, true, true)
        .context("building site for export")?;

    let html = tmp.join(rel).with_extension("html");
    if !html.exists() {
        bail!("built page not found at {}", html.display());
    }

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let out_abs = if out.is_absolute() {
        out.to_path_buf()
    } else {
        std::env::current_dir()?.join(out)
    };
    let url = format!("file://{}", html.display());
    let status = Command::new(&chrome)
        .args([
            "--headless=new",
            "--disable-gpu",
            "--no-pdf-header-footer",
            "--virtual-time-budget=5000",
            "--run-all-compositor-stages-before-draw",
        ])
        .arg(format!("--print-to-pdf={}", out_abs.display()))
        .arg(&url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .with_context(|| format!("running {}", chrome.display()))?;
    let _ = std::fs::remove_dir_all(&tmp);
    if !status.success() {
        bail!("Chrome exited with {}", status);
    }
    if !out_abs.exists() {
        bail!("Chrome finished but wrote no file at {}", out_abs.display());
    }
    if !quiet {
        println!("✓ {}", out_abs.display());
    }
    Ok(())
}

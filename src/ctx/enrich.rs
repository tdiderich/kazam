//! `kazam ctx enrich`: whole-file descriptions from a local model.
//!
//! Reads each file in full (outline + head past MAX_FILE_TOKENS) and asks an
//! OpenAI-compatible endpoint - `mlx_lm.server` by default - for a one-line
//! description and up to two gotchas. Results are cached globally by content
//! sha under `~/.kazam/cache/enrich/`, so renames and every worktree of the
//! same repo reuse them without another model call.
//!
//! Precedence: agent-written descriptions (`ctx describe`) are never replaced.
//! Heuristic ones and stale model ones (content changed since) are.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::scan;
use super::types::{AnatomyStore, DescSource};
use crate::workspace;

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8765/v1/chat/completions";
pub const DEFAULT_MODEL: &str = "mlx-community/Qwen3-1.7B-4bit";
const MAX_FILE_TOKENS: u64 = 24_000;
/// Files past this are data dumps, not worth a description pass.
const SKIP_ABOVE_TOKENS: u64 = 120_000;
const SAVE_EVERY: usize = 5;

#[derive(Serialize, Deserialize, Clone)]
pub struct Enrichment {
    pub description: String,
    #[serde(default)]
    pub gotchas: Vec<String>,
    #[serde(default)]
    pub model: String,
}

pub struct Options {
    pub max: usize,
    pub endpoint: String,
    pub model: String,
}

#[derive(Serialize, Default)]
pub struct Report {
    pub from_cache: usize,
    pub described: usize,
    pub failed: usize,
    pub remaining: usize,
    pub backend: String,
}

pub fn cache_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".kazam/cache/enrich"))
}

pub fn cached(sha: &str) -> Option<Enrichment> {
    let p = cache_dir()?.join(format!("{sha}.json"));
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

fn store_cache(sha: &str, e: &Enrichment) {
    if let Some(dir) = cache_dir() {
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string(e) {
            let _ = std::fs::write(dir.join(format!("{sha}.json")), s);
        }
    }
}

fn skip_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with(".lock")
        || name.ends_with("-lock.json")
        || name.ends_with(".min.js")
        || name.ends_with(".map")
}

/// Apply cached enrichments to the store in place. Returns how many entries
/// changed, and the paths still needing a model pass (priority order).
fn apply_cache(store: &mut AnatomyStore) -> (usize, Vec<String>) {
    let mut applied = 0;
    let mut todo: Vec<(u32, u64, String)> = Vec::new();
    for f in store.files.iter_mut() {
        if f.desc_source == Some(DescSource::Agent) || skip_path(&f.path) {
            continue;
        }
        let Some(sha) = f.sha.clone() else { continue };
        if f.tokens == 0 || f.tokens > SKIP_ABOVE_TOKENS {
            continue;
        }
        match cached(&sha) {
            Some(e) => {
                if f.description.as_deref() != Some(e.description.as_str())
                    || f.desc_source != Some(DescSource::Model)
                {
                    f.description = Some(e.description);
                    f.desc_source = Some(DescSource::Model);
                    applied += 1;
                }
            }
            None => todo.push((f.reads, f.tokens, f.path.clone())),
        }
    }
    // Most-read first, then cheapest: the index gets useful fastest.
    todo.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    (applied, todo.into_iter().map(|t| t.2).collect())
}

/// Write enrichments into the current on-disk store. Re-reads it first so a
/// refresh that ran meanwhile isn't clobbered; only entries whose sha still
/// matches get the new description.
fn save(project: &Path) -> Result<()> {
    let _lock = scan::StoreLock::acquire(project);
    let mut store = scan::load_flat(project);
    apply_cache(&mut store);
    workspace::write_yaml(
        &workspace::root(project).join("ctx/anatomy.flat.yaml"),
        &store,
    )?;
    scan::write_layered(project, &store)
}

fn prompt_for(path: &str, text: &str, outline: &[String], tokens: u64) -> String {
    let body = if tokens > MAX_FILE_TOKENS {
        let cut = (MAX_FILE_TOKENS * 3) as usize;
        let cut = (0..=cut.min(text.len()))
            .rev()
            .find(|&n| text.is_char_boundary(n))
            .unwrap_or(0);
        format!(
            "OUTLINE:\n{}\n\nHEAD:\n{}",
            outline.join("\n"),
            &text[..cut]
        )
    } else {
        text.to_string()
    };
    format!(
        "/no_think Read this whole file. Reply with ONLY a JSON object: \
         {{\"description\": \"<one sentence: what this file does and its role in the repo>\", \
         \"gotchas\": [\"<up to 2 non-obvious behaviors or footguns, empty if none>\"]}}\n\n\
         FILE: {path}\n```\n{body}\n```"
    )
}

/// Pull the first `{...}` object out of a completion, tolerating `<think>`
/// blocks and code fences around it.
fn parse_completion(content: &str) -> Option<Enrichment> {
    let content = match content.rfind("</think>") {
        Some(i) => &content[i + 8..],
        None => content,
    };
    let start = content.find('{')?;
    let end = content.rfind('}')?;
    if end <= start {
        return None;
    }
    #[derive(Deserialize)]
    struct Raw {
        description: String,
        #[serde(default)]
        gotchas: Vec<String>,
    }
    let raw: Raw = serde_json::from_str(&content[start..=end]).ok()?;
    let description = raw.description.trim().replace(['\n', '\t'], " ");
    if description.is_empty() {
        return None;
    }
    Some(Enrichment {
        description,
        gotchas: raw
            .gotchas
            .into_iter()
            .map(|g| g.trim().to_string())
            .filter(|g| !g.is_empty())
            .take(2)
            .collect(),
        model: String::new(),
    })
}

fn describe(opts: &Options, prompt: &str) -> Result<Enrichment> {
    let body = serde_json::json!({
        "model": opts.model,
        "messages": [{ "role": "user", "content": prompt }],
        "max_tokens": 200,
        "temperature": 0,
        "chat_template_kwargs": { "enable_thinking": false },
    });
    let resp = crate::http::post_text(
        &opts.endpoint,
        &[("Content-Type", "application/json")],
        &body.to_string(),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let v: serde_json::Value = serde_json::from_str(&resp).context("parse completion")?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default();
    let mut e = parse_completion(content).context("no JSON object in completion")?;
    e.model = opts.model.clone();
    Ok(e)
}

fn models_url(endpoint: &str) -> String {
    match endpoint.find("/v1/") {
        Some(i) => format!("{}/v1/models", &endpoint[..i]),
        None => endpoint.to_string(),
    }
}

fn lock_path(project: &Path) -> PathBuf {
    workspace::root(project).join("ctx/enrich.lock")
}

fn pid_alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Take the enrich lock atomically (create_new), or return false if another
/// live enrich holds it. A lock whose pid is dead, or that's older than
/// MAX_RUN (pid reuse after a crash), is broken and retaken once.
fn try_lock(project: &Path) -> bool {
    const MAX_RUN: std::time::Duration = std::time::Duration::from_secs(30 * 60);
    let p = lock_path(project);
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(mut f) => {
                use std::io::Write;
                let _ = write!(f, "{}", std::process::id());
                return true;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let pid = std::fs::read_to_string(&p).unwrap_or_default();
                let too_old = std::fs::metadata(&p)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > MAX_RUN);
                if pid_alive(pid.trim()) && !too_old {
                    return false;
                }
                let _ = std::fs::remove_file(&p);
            }
            Err(_) => return false,
        }
    }
    false
}

pub fn run(project: &Path, opts: &Options) -> Result<Report> {
    let mut report = Report {
        backend: opts.endpoint.clone(),
        ..Default::default()
    };
    // Cache hits first: free, and they work with the backend offline.
    let mut store = scan::load_flat(project);
    let (applied, todo) = apply_cache(&mut store);
    report.from_cache = applied;
    if applied > 0 {
        save(project)?;
    }
    report.remaining = todo.len();
    if todo.is_empty() || opts.max == 0 {
        return Ok(report);
    }
    if crate::http::get_text(&models_url(&opts.endpoint), &[]).is_err() {
        report.backend = format!("offline: {}", opts.endpoint);
        return Ok(report);
    }
    if !try_lock(project) {
        report.backend = "busy: another enrich is running".into();
        return Ok(report);
    }

    let by_path: std::collections::HashMap<&str, &super::types::FileEntry> =
        store.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut since_save = 0;
    for path in todo.iter().take(opts.max) {
        let f = by_path[path.as_str()];
        let Ok(bytes) = std::fs::read(project.join(path)) else {
            report.failed += 1;
            continue;
        };
        // Content moved on since the scan: leave it for the next refresh.
        if f.sha.as_deref() != Some(scan::content_sha(&bytes).as_str()) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        match describe(opts, &prompt_for(path, &text, &f.outline, f.tokens)) {
            Ok(e) => {
                store_cache(f.sha.as_deref().unwrap_or_default(), &e);
                report.described += 1;
                since_save += 1;
                if since_save >= SAVE_EVERY {
                    save(project)?;
                    since_save = 0;
                }
            }
            Err(_) => report.failed += 1,
        }
    }
    if since_save > 0 {
        save(project)?;
    }
    let _ = std::fs::remove_file(lock_path(project));
    report.remaining = report.remaining.saturating_sub(report.described);
    Ok(report)
}

/// Re-exec `kazam ctx enrich` detached at low priority, logging to
/// `ctx/enrich.log`, and return immediately. Used by the SessionStart hook.
pub fn spawn_background(project: &Path, opts: &Options) -> Result<()> {
    let exe = std::env::current_exe().context("locate kazam binary")?;
    let log = workspace::root(project).join("ctx/enrich.log");
    let out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .with_context(|| format!("open {}", log.display()))?;
    let err = out.try_clone()?;
    std::process::Command::new("nice")
        .arg("-n")
        .arg("15")
        .arg(exe)
        .args(["ctx", "enrich", "--json", "--max"])
        .arg(opts.max.to_string())
        .args([
            "--endpoint",
            &opts.endpoint,
            "--model",
            &opts.model,
            "--dir",
        ])
        .arg(project)
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .context("spawn background enrich")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_after_think_block_and_fence() {
        let c = "<think>\nhmm\n</think>\n```json\n{\"description\": \"Scans files.\", \"gotchas\": [\"a\", \"\", \"b\", \"c\"]}\n```";
        let e = parse_completion(c).unwrap();
        assert_eq!(e.description, "Scans files.");
        assert_eq!(e.gotchas, vec!["a", "b"]);
    }

    #[test]
    fn rejects_empty_or_missing_description() {
        assert!(parse_completion("no json here").is_none());
        assert!(parse_completion("{\"description\": \"  \"}").is_none());
    }

    #[test]
    fn models_url_from_chat_endpoint() {
        assert_eq!(
            models_url("http://127.0.0.1:8765/v1/chat/completions"),
            "http://127.0.0.1:8765/v1/models"
        );
    }

    #[test]
    fn skips_lockfiles_and_minified() {
        assert!(skip_path("Cargo.lock"));
        assert!(skip_path("web/package-lock.json"));
        assert!(skip_path("dist/app.min.js"));
        assert!(!skip_path("src/main.rs"));
    }
}

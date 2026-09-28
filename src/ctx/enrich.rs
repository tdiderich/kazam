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
/// In anatomy units (bytes / 4). Real tokens run ~1.4x that on code, so this
/// keeps a whole-file prompt around 17k tokens, inside a small model's 32k
/// context. Bigger files send outline + head instead. At 24_000 the largest
/// files overflowed the context and the server rejected them.
const MAX_FILE_TOKENS: u64 = 12_000;
/// A file whose describe fails this many times (model rejected it, or the
/// reply had no JSON) stops being queued, until its content changes.
const MAX_FILE_FAILURES: u32 = 2;
/// Files past this are data dumps, not worth a description pass.
const SKIP_ABOVE_TOKENS: u64 = 120_000;
const SAVE_EVERY: usize = 5;
/// One file's completion. A 24k-token prompt takes ~10-20 s on a 1.7B model;
/// anything past this is a wedged server, not a slow one.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);
/// Consecutive failures before a run gives up: the backend is down or stuck,
/// and hammering it just burns the whole --max budget on timeouts.
const MAX_CONSECUTIVE_FAILURES: usize = 3;

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
    /// Bypass the default source-and-docs policy.
    pub all: bool,
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

fn fail_path(sha: &str) -> Option<PathBuf> {
    Some(cache_dir()?.join(format!("{sha}.fail")))
}

fn failures(sha: &str) -> u32 {
    fail_path(sha)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn record_failure(sha: &str) {
    if let Some(p) = fail_path(sha) {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, (failures(sha) + 1).to_string());
    }
}

fn store_cache(sha: &str, e: &Enrichment) {
    if let Some(dir) = cache_dir() {
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string(e) {
            let _ = std::fs::write(dir.join(format!("{sha}.json")), s);
        }
    }
}

/// Extensions worth a model description by default: source, docs, config.
const ENRICH_EXTS: &[&str] = &[
    "rs", "py", "ts", "tsx", "js", "jsx", "mjs", "cjs", "go", "java", "kt", "swift", "rb", "php",
    "c", "h", "cc", "cpp", "hpp", "cs", "scala", "ex", "exs", "sh", "bash", "zsh", "sql", "tf",
    "proto", "graphql", "prisma", "md", "mdx", "yaml", "yml", "toml", "agl",
];
/// Extensionless or data-format files still worth describing.
const ENRICH_NAMES: &[&str] = &[
    "Dockerfile",
    "Makefile",
    "Justfile",
    "package.json",
    "tsconfig.json",
    ".mcp.json",
];
/// Path segments whose files are tests, fixtures, or generated/vendored
/// output: indexed and outlined, but not worth a model pass by default.
const SKIP_SEGMENTS: &[&str] = &[
    "test",
    "tests",
    "__tests__",
    "spec",
    "fixtures",
    "testdata",
    "snapshots",
    "__snapshots__",
    "vendor",
    "dist",
    "build",
    "generated",
    "static",
    "assets",
    "public",
    "migrations",
];

/// Default enrichment policy: source and docs, not tests, data, or build
/// output. `--all` bypasses it (lockfiles and minified files stay skipped).
fn wanted(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if ENRICH_NAMES.contains(&name) {
        return true;
    }
    let mut dirs = path.split('/').collect::<Vec<_>>();
    dirs.pop();
    if dirs.iter().any(|d| SKIP_SEGMENTS.contains(d)) {
        return false;
    }
    let is_test_file = name.starts_with("test_")
        || [
            "_test.go",
            "_test.py",
            ".test.ts",
            ".test.tsx",
            ".test.js",
            ".spec.ts",
            ".spec.tsx",
            ".spec.js",
            ".d.ts",
        ]
        .iter()
        .any(|suf| name.ends_with(suf));
    if is_test_file {
        return false;
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    ENRICH_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

/// Paths changed in the last 90 days, used to order the queue. Empty
/// outside git.
fn recently_changed(project: &Path) -> std::collections::HashSet<String> {
    std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["log", "--since=90.days", "--name-only", "--pretty=format:"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
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
    // `todo` is every uncached candidate; `run` narrows it with the policy.
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
            None if failures(&sha) >= MAX_FILE_FAILURES => {}
            None => todo.push((f.reads, f.tokens, f.path.clone())),
        }
    }
    // Most-read first, then cheapest. `run` re-orders by recency on top.
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

/// Why a describe failed: the backend (down, wedged, timed out) or this one
/// file (rejected by the model, unusable reply). Only backend failures count
/// toward stopping the run; file failures get marked and skipped.
enum Failure {
    Backend,
    File,
}

fn describe(opts: &Options, prompt: &str) -> std::result::Result<Enrichment, Failure> {
    let body = serde_json::json!({
        "model": opts.model,
        "messages": [{ "role": "user", "content": prompt }],
        "max_tokens": 200,
        "temperature": 0,
        "chat_template_kwargs": { "enable_thinking": false },
    });
    let resp = crate::http::post_text_timeout(
        &opts.endpoint,
        &[("Content-Type", "application/json")],
        &body.to_string(),
        REQUEST_TIMEOUT,
    )
    .map_err(|e| match e {
        // 4xx: the server refused this request (context overflow, bad input).
        crate::http::Error::Status(code, _) if (400..500).contains(&code) => Failure::File,
        _ => Failure::Backend,
    })?;
    let v: serde_json::Value = serde_json::from_str(&resp).map_err(|_| Failure::File)?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default();
    let mut e = parse_completion(content).ok_or(Failure::File)?;
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
    let (applied, mut todo) = apply_cache(&mut store);
    report.from_cache = applied;
    if applied > 0 {
        save(project)?;
    }
    if !opts.all {
        todo.retain(|p| wanted(p));
    }
    // Stable sort keeps most-read-first within each group: recent work first.
    let recent = recently_changed(project);
    todo.sort_by_key(|p| !recent.contains(p));
    report.remaining = todo.len();
    if todo.is_empty() || opts.max == 0 {
        return Ok(report);
    }
    if crate::http::get_text_timeout(
        &models_url(&opts.endpoint),
        &[],
        std::time::Duration::from_secs(5),
    )
    .is_err()
    {
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
    let mut consecutive_failures = 0;
    for path in todo.iter().take(opts.max) {
        if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
            report.backend = format!(
                "stopped after {MAX_CONSECUTIVE_FAILURES} consecutive failures: {}",
                opts.endpoint
            );
            break;
        }
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
                consecutive_failures = 0;
                store_cache(f.sha.as_deref().unwrap_or_default(), &e);
                report.described += 1;
                since_save += 1;
                if since_save >= SAVE_EVERY {
                    save(project)?;
                    since_save = 0;
                }
            }
            Err(Failure::Backend) => {
                report.failed += 1;
                consecutive_failures += 1;
            }
            Err(Failure::File) => {
                report.failed += 1;
                consecutive_failures = 0;
                record_failure(f.sha.as_deref().unwrap_or_default());
            }
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
        .args(if opts.all { &["--all"][..] } else { &[][..] })
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
    fn policy_keeps_source_and_docs_only() {
        assert!(wanted("src/ctx/scan.rs"));
        assert!(wanted("docs/guide.md"));
        assert!(wanted("deploy/Dockerfile"));
        assert!(wanted("web/package.json"));
        assert!(!wanted("data/customers.json"));
        assert!(!wanted("tests/test_scan.py"));
        assert!(!wanted("pkg/scan_test.go"));
        assert!(!wanted("web/src/app.test.tsx"));
        assert!(!wanted("web/dist/bundle.js"));
        assert!(!wanted("types/index.d.ts"));
        assert!(!wanted("stubs/boto3.pyi"));
        assert!(!wanted("db/migrations/0001_init.sql"));
    }

    #[test]
    fn skips_lockfiles_and_minified() {
        assert!(skip_path("Cargo.lock"));
        assert!(skip_path("web/package-lock.json"));
        assert!(skip_path("dist/app.min.js"));
        assert!(!skip_path("src/main.rs"));
    }
}

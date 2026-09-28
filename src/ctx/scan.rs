use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

use crate::workspace;

use super::types::{AnatomyStore, DescSource, FileEntry};

/// Files above this are indexed by size only: no hash, no outline.
const MAX_HASH_BYTES: u64 = 2_000_000;

const SKIP_DIRS: &[&str] = &[
    ".kazam",
    "_site",
    "target",
    ".git",
    "node_modules",
    "__pycache__",
    ".venv",
];

const BINARY_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "ico", "svg", "woff", "woff2", "ttf", "eot", "otf", "mp3",
    "mp4", "wav", "ogg", "pdf", "zip", "tar", "gz", "br", "exe", "dll", "so", "dylib", "o", "a",
    "pyc", "class", "wasm",
];

/// Drain `.kazam/ctx/reads.log` into (path -> (count, latest timestamp)).
///
/// The read hook appends `path\ttimestamp` lines rather than mutating
/// anatomy.flat.yaml directly: appends are cheap and survive parallel
/// subagents, where a read-modify-write of the whole store would lose
/// updates. Counts are folded into the anatomy on the next scan.
fn drain_reads_log(project: &Path) -> HashMap<String, (u32, String)> {
    let log_path = workspace::root(project).join("ctx/reads.log");
    let Ok(text) = std::fs::read_to_string(&log_path) else {
        return HashMap::new();
    };

    let mut pending: HashMap<String, (u32, String)> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (path, ts) = line.split_once('\t').unwrap_or((line, ""));
        let entry = pending
            .entry(path.to_string())
            .or_insert((0, String::new()));
        entry.0 += 1;
        if ts > entry.1.as_str() {
            entry.1 = ts.to_string();
        }
    }

    // Truncate rather than delete so the hook's `>>` target keeps existing.
    // A read logged between the parse above and this write is lost; that is an
    // acceptable trade for not holding a lock on the hot path.
    let _ = std::fs::write(&log_path, "");
    pending
}

pub fn scan(project: &Path) -> Result<AnatomyStore> {
    scan_with(project, true)
}

/// Content fingerprint: first 20 hex chars of sha256. Short enough for YAML,
/// long enough that collisions within one repo are not a practical concern.
pub fn content_sha(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().take(10).map(|b| format!("{b:02x}")).collect()
}

fn mtime_ms(meta: &std::fs::Metadata) -> Option<u64> {
    meta.modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// Files to index. In a git repo: tracked plus untracked-but-not-ignored
/// (`git ls-files -co --exclude-standard`), so ignored build output, caches,
/// and vendored installs stay out. Nested repos (their own `.git`) show up
/// as a single directory entry and are listed recursively the same way.
/// Outside git, or if git fails, walk the tree with the SKIP_DIRS filter.
fn candidate_files(project: &Path) -> Vec<std::path::PathBuf> {
    match git_files(project, 0) {
        Some(files) => files,
        None => walk_files(project),
    }
}

fn git_files(dir: &Path, depth: usize) -> Option<Vec<std::path::PathBuf>> {
    if !dir.join(".git").exists() {
        return None;
    }
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["ls-files", "-co", "--exclude-standard", "-z"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let mut files = Vec::new();
    for rel in out.stdout.split(|&b| b == 0).filter(|r| !r.is_empty()) {
        let rel = String::from_utf8_lossy(rel);
        if rel.split('/').any(|seg| SKIP_DIRS.contains(&seg)) {
            continue;
        }
        let p = dir.join(rel.as_ref());
        if rel.ends_with('/') {
            // A nested repository: git reports it as one entry.
            if depth < 2 {
                if let Some(inner) = git_files(&p, depth + 1) {
                    files.extend(inner);
                }
            }
            continue;
        }
        files.push(p);
    }
    // Subrepos are often gitignored in the parent ("each has its own
    // history"). `--directory` collapses ignored dirs to one entry each, so
    // this stays cheap even with node_modules around; keep the ones that are
    // repositories and list them like any nested repo.
    if depth < 2 {
        if let Ok(ignored) = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "ls-files",
                "-o",
                "-i",
                "--exclude-standard",
                "--directory",
                "-z",
            ])
            .output()
        {
            for rel in ignored.stdout.split(|&b| b == 0).filter(|r| !r.is_empty()) {
                let rel = String::from_utf8_lossy(rel);
                if !rel.ends_with('/') || rel.split('/').any(|seg| SKIP_DIRS.contains(&seg)) {
                    continue;
                }
                let sub = dir.join(rel.as_ref());
                if let Some(inner) = git_files(&sub, depth + 1) {
                    files.extend(inner);
                }
            }
        }
    }
    Some(files)
}

fn walk_files(project: &Path) -> Vec<std::path::PathBuf> {
    WalkDir::new(project)
        .into_iter()
        .filter_entry(|e| {
            // The root is the project itself, even when its own name is dotted.
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_str().unwrap_or("");
            if name.starts_with('.') && name != "." {
                return false;
            }
            !SKIP_DIRS.contains(&name)
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect()
}

/// `drain_reads`: fold and truncate `ctx/reads.log`. Only callers that write
/// the resulting store may drain, or the read counts are lost.
fn scan_with(project: &Path, drain_reads: bool) -> Result<AnatomyStore> {
    // The flat store lives at anatomy.flat.yaml (used by board + check + describe).
    // anatomy.yaml is the agent-facing layered summary written by write_layered().
    let flat_path = workspace::root(project).join("ctx/anatomy.flat.yaml");
    // Fall back to anatomy.yaml (legacy path) if flat doesn't exist yet
    let anatomy_path = workspace::root(project).join("ctx/anatomy.yaml");
    let existing: AnatomyStore = if flat_path.exists() {
        workspace::read_yaml(&flat_path)?
    } else if anatomy_path.exists() {
        // Try to parse as flat AnatomyStore (legacy); if it fails (new summary format), start fresh
        workspace::read_yaml::<AnatomyStore>(&anatomy_path).unwrap_or(AnatomyStore {
            scanned: String::new(),
            files: vec![],
        })
    } else {
        AnatomyStore {
            scanned: String::new(),
            files: vec![],
        }
    };

    // Index existing entries by path for description preservation
    let existing_by_path: std::collections::HashMap<&str, &FileEntry> = existing
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();

    // Reads recorded by the hook since the last scan, folded in below.
    let pending_reads = if drain_reads {
        drain_reads_log(project)
    } else {
        HashMap::new()
    };

    let mut files: Vec<FileEntry> = Vec::new();
    let now = chrono::Local::now().to_rfc3339();

    for path in candidate_files(project) {
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if BINARY_EXTS.contains(&ext) {
            continue;
        }
        let rel = path
            .strip_prefix(project)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        let meta = Some(meta);
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = meta.as_ref().and_then(mtime_ms);
        let tokens = size / 4;
        let prev = existing_by_path.get(rel.as_str()).copied();

        // Fast path: same size and mtime as last scan means same content, so
        // reuse the stored hash and outline without reading the file. Same
        // trade-off as git's stat cache: a tool that rewrites content but
        // restores the old mtime at the same size (cp -p, rsync -t) is missed.
        let unchanged = prev.is_some_and(|p| {
            p.sha.is_some() && p.size == Some(size) && mtime.is_some() && p.mtime_ms == mtime
        });
        let (sha, outline) = if unchanged {
            let p = prev.unwrap();
            (p.sha.clone(), p.outline.clone())
        } else if size <= MAX_HASH_BYTES {
            match std::fs::read(&path) {
                Ok(bytes) if !bytes.iter().take(4096).any(|&b| b == 0) => {
                    let text = String::from_utf8_lossy(&bytes);
                    (
                        Some(content_sha(&bytes)),
                        super::outline::outline(&ext.to_ascii_lowercase(), &text),
                    )
                }
                _ => (None, vec![]),
            }
        } else {
            (None, vec![])
        };

        // Preserve agent- or model-written descriptions. Legacy entries have no
        // desc_source: a description that isn't the heuristic one came from an
        // agent via `ctx describe`.
        let heuristic = heuristic_description(&rel, ext);
        let (description, desc_source) = match prev.and_then(|f| f.description.clone()) {
            Some(d) => {
                let src = prev.and_then(|f| f.desc_source).unwrap_or(
                    if heuristic.as_deref() == Some(d.as_str()) {
                        DescSource::Heuristic
                    } else {
                        DescSource::Agent
                    },
                );
                (Some(d), Some(src))
            }
            None => {
                let src = heuristic.as_ref().map(|_| DescSource::Heuristic);
                (heuristic, src)
            }
        };

        let carried_reads = existing_by_path
            .get(rel.as_str())
            .map(|f| f.reads)
            .unwrap_or(0);
        let carried_last_read = existing_by_path
            .get(rel.as_str())
            .and_then(|f| f.last_read.clone());

        let (reads, last_read) = match pending_reads.get(&rel) {
            Some((count, ts)) => (
                carried_reads.saturating_add(*count),
                if ts.is_empty() {
                    carried_last_read
                } else {
                    Some(ts.clone())
                },
            ),
            None => (carried_reads, carried_last_read),
        };

        files.push(FileEntry {
            path: rel,
            description,
            tokens,
            reads,
            last_read,
            last_scanned: now.clone(),
            sha,
            size: Some(size),
            mtime_ms: mtime,
            outline,
            desc_source,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(AnatomyStore {
        scanned: now,
        files,
    })
}

/// Convert a directory path to a safe filename stem (replace `/` with `--`).
fn dir_to_filename(dir_path: &str) -> String {
    dir_path.replace('/', "--")
}

/// Strip tabs from a string so it's safe to embed in a TSV field.
fn sanitize_tsv(s: &str) -> String {
    s.replace('\t', "  ")
}

/// Derive a human-readable description for a directory given its files' descriptions.
fn derive_dir_description(files: &[&FileEntry]) -> Option<String> {
    // Collect non-None descriptions
    let descs: Vec<&str> = files
        .iter()
        .filter_map(|f| f.description.as_deref())
        .collect();
    if descs.is_empty() {
        return None;
    }

    // Find common suffix pattern: "X route handler", "X controller", etc.
    let suffixes = [
        "route handler",
        "controller",
        "model",
        "middleware",
        "service",
        "library module",
        "utility",
        "component",
        "view/page",
        "migration",
        "tests",
        "configuration",
        "data/seed file",
    ];
    for suffix in &suffixes {
        let matching = descs.iter().filter(|d| d.ends_with(suffix)).count();
        if matching > 0 && matching * 2 >= descs.len() {
            // Majority (>= 50%) share this suffix pattern
            let plural = match *suffix {
                "route handler" => "route handlers",
                "controller" => "controllers",
                "model" => "models",
                "middleware" => "middleware",
                "service" => "services",
                "library module" => "library modules",
                "utility" => "utilities",
                "component" => "components",
                "view/page" => "views/pages",
                "migration" => "migrations",
                "tests" => "tests",
                "configuration" => "configuration files",
                "data/seed file" => "data/seed files",
                _ => suffix,
            };
            return Some(plural.to_string());
        }
    }

    None
}

/// Build and write the two-tier layered anatomy files (TSV format).
/// Writes:
///   - `ctx/anatomy.tsv`  - summary (root files + directory rollups)
///   - `ctx/anatomy/<dir>.tsv` - per-directory file listings
///   - Also removes stale `.yaml` anatomy files (except `anatomy.flat.yaml`).
pub fn write_layered(project: &Path, store: &AnatomyStore) -> Result<()> {
    let ctx_dir = workspace::root(project).join("ctx");
    let anatomy_dir = ctx_dir.join("anatomy");
    std::fs::create_dir_all(&anatomy_dir).context("create ctx/anatomy dir")?;

    // Separate root files from files in directories
    let mut root_files: Vec<FileEntry> = Vec::new();
    // Group by leaf directory (for detail files)
    let mut by_leaf_dir: HashMap<String, Vec<&FileEntry>> = HashMap::new();
    // Group by top-level directory (for summary)
    let mut by_top_dir: HashMap<String, Vec<&FileEntry>> = HashMap::new();

    for file in &store.files {
        let path = &file.path;
        if let Some(slash_pos) = path.find('/') {
            let top_dir = &path[..slash_pos];
            by_top_dir
                .entry(top_dir.to_string())
                .or_default()
                .push(file);
            if let Some(leaf_pos) = path.rfind('/') {
                let leaf_dir = &path[..leaf_pos];
                by_leaf_dir
                    .entry(leaf_dir.to_string())
                    .or_default()
                    .push(file);
            }
        } else {
            root_files.push(file.clone());
        }
    }

    root_files.sort_by(|a, b| a.path.cmp(&b.path));

    // Summary uses top-level directories only (compact)
    let mut sorted_top_dirs: Vec<String> = by_top_dir.keys().cloned().collect();
    sorted_top_dirs.sort();

    // Detail files use leaf directories (granular)
    let mut sorted_leaf_dirs: Vec<String> = by_leaf_dir.keys().cloned().collect();
    sorted_leaf_dirs.sort();

    for dir_path in &sorted_leaf_dirs {
        let files = by_leaf_dir.get(dir_path.as_str()).unwrap();
        let mut sorted_files: Vec<FileEntry> = files.iter().map(|f| (*f).clone()).collect();
        sorted_files.sort_by(|a, b| a.path.cmp(&b.path));

        let mut tsv = String::new();
        tsv.push_str("path\ttokens\treads\tdescription\n");
        for f in &sorted_files {
            let desc = f.description.as_deref().unwrap_or("");
            tsv.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                f.path,
                f.tokens,
                f.reads,
                sanitize_tsv(desc)
            ));
        }

        let filename = format!("{}.tsv", dir_to_filename(dir_path));
        let tsv_path = anatomy_dir.join(&filename);
        let tmp = tsv_path.with_extension("tsv.tmp");
        std::fs::write(&tmp, &tsv).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, &tsv_path)
            .with_context(|| format!("rename to {}", tsv_path.display()))?;
    }

    // Build and write summary TSV
    let mut summary_tsv = String::new();
    summary_tsv.push_str(&format!("# scanned: {}\n", store.scanned));
    summary_tsv.push_str("# root_files\n");
    summary_tsv.push_str("path\ttokens\treads\tdescription\n");
    for f in &root_files {
        let desc = f.description.as_deref().unwrap_or("");
        summary_tsv.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            f.path,
            f.tokens,
            f.reads,
            sanitize_tsv(desc)
        ));
    }
    summary_tsv.push_str("\n# directories\n");
    summary_tsv.push_str("path\tfiles\ttokens\tdescription\n");
    for dir_path in &sorted_top_dirs {
        let files = by_top_dir.get(dir_path.as_str()).unwrap();
        let file_count = files.len();
        let total_tokens: u64 = files.iter().map(|f| f.tokens).sum();
        let description = derive_dir_description(files);
        let desc = description.as_deref().unwrap_or("");
        summary_tsv.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            dir_path,
            file_count,
            total_tokens,
            sanitize_tsv(desc)
        ));
    }

    let summary_path = ctx_dir.join("anatomy.tsv");
    let tmp = summary_path.with_extension("tsv.tmp");
    std::fs::write(&tmp, &summary_tsv).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &summary_path)
        .with_context(|| format!("rename to {}", summary_path.display()))?;

    // Remove stale YAML anatomy files (anatomy.yaml summary and all anatomy/<dir>.yaml detail files)
    let old_summary = ctx_dir.join("anatomy.yaml");
    if old_summary.exists() {
        let _ = std::fs::remove_file(&old_summary);
    }
    if let Ok(entries) = std::fs::read_dir(&anatomy_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("yaml") {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    Ok(())
}

pub fn check(project: &Path) -> Result<ScanDiff> {
    let flat_path = workspace::root(project).join("ctx/anatomy.flat.yaml");
    let anatomy_path = workspace::root(project).join("ctx/anatomy.yaml");
    // Prefer flat file; fall back to legacy anatomy.yaml
    let stored_path = if flat_path.exists() {
        flat_path
    } else if anatomy_path.exists() {
        anatomy_path
    } else {
        return Ok(ScanDiff {
            new_files: vec![],
            deleted_files: vec![],
            changed_files: vec![],
        });
    };

    // If parse fails (ex. anatomy.yaml is now a summary not a flat store), return empty diff
    let stored: AnatomyStore = match workspace::read_yaml(&stored_path).context("read anatomy") {
        Ok(s) => s,
        Err(_) => {
            return Ok(ScanDiff {
                new_files: vec![],
                deleted_files: vec![],
                changed_files: vec![],
            })
        }
    };
    let current = scan_with(project, false)?;
    Ok(diff(&stored, &current).into_scan_diff())
}

/// Path-level diff between two stores. A file counts as changed when both
/// sides have a sha and they differ, or (legacy entries) when the token
/// estimate moved. An added path whose sha matches a deleted path is a rename.
pub fn diff(stored: &AnatomyStore, current: &AnatomyStore) -> RefreshDiff {
    let stored_set: HashMap<&str, &FileEntry> =
        stored.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let current_set: HashMap<&str, &FileEntry> =
        current.files.iter().map(|f| (f.path.as_str(), f)).collect();

    let mut added: Vec<String> = current
        .files
        .iter()
        .filter(|f| !stored_set.contains_key(f.path.as_str()))
        .map(|f| f.path.clone())
        .collect();
    let mut deleted: Vec<String> = stored
        .files
        .iter()
        .filter(|f| !current_set.contains_key(f.path.as_str()))
        .map(|f| f.path.clone())
        .collect();
    let changed: Vec<String> = current
        .files
        .iter()
        .filter(|f| {
            stored_set
                .get(f.path.as_str())
                .is_some_and(|s| match (&s.sha, &f.sha) {
                    (Some(a), Some(b)) => a != b,
                    _ => s.tokens != f.tokens,
                })
        })
        .map(|f| f.path.clone())
        .collect();

    // Pair only when a sha is unique among deleted files and across the
    // current tree. Duplicate content (boilerplate, empty files) would
    // otherwise invent renames.
    let mut deleted_by_sha: HashMap<&str, Vec<&str>> = HashMap::new();
    for p in &deleted {
        if let Some(sha) = stored_set[p.as_str()].sha.as_deref() {
            deleted_by_sha.entry(sha).or_default().push(p.as_str());
        }
    }
    // Counted over the whole current tree, not just the added files: content
    // that still exists elsewhere is boilerplate, and pairing it means nothing.
    let mut added_by_sha: HashMap<&str, Vec<&str>> = HashMap::new();
    for f in &current.files {
        if let Some(sha) = f.sha.as_deref() {
            added_by_sha.entry(sha).or_default().push(f.path.as_str());
        }
    }
    let mut renamed: Vec<(String, String)> = Vec::new();
    for p in &added {
        let Some(sha) = current_set[p.as_str()].sha.as_deref() else {
            continue;
        };
        if let (Some([from]), Some([_])) = (
            deleted_by_sha.get(sha).map(Vec::as_slice),
            added_by_sha.get(sha).map(Vec::as_slice),
        ) {
            renamed.push((from.to_string(), p.clone()));
        }
    }
    added.retain(|p| !renamed.iter().any(|(_, to)| to == p));
    deleted.retain(|p| !renamed.iter().any(|(from, _)| from == p));

    RefreshDiff {
        added,
        changed,
        deleted,
        renamed,
    }
}

#[derive(serde::Serialize, Default, Debug, PartialEq)]
pub struct RefreshDiff {
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub deleted: Vec<String>,
    /// (from, to)
    pub renamed: Vec<(String, String)>,
}

impl RefreshDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.changed.is_empty()
            && self.deleted.is_empty()
            && self.renamed.is_empty()
    }

    /// Legacy `scan --check` shape: renames show as delete + add.
    fn into_scan_diff(self) -> ScanDiff {
        let mut new_files = self.added;
        let mut deleted_files = self.deleted;
        for (from, to) in self.renamed {
            deleted_files.push(from);
            new_files.push(to);
        }
        ScanDiff {
            new_files,
            deleted_files,
            changed_files: self.changed,
        }
    }
}

/// Load the stored flat anatomy, or an empty one.
pub fn load_flat(project: &Path) -> AnatomyStore {
    let flat_path = workspace::root(project).join("ctx/anatomy.flat.yaml");
    workspace::read_yaml(&flat_path).unwrap_or(AnatomyStore {
        scanned: String::new(),
        files: vec![],
    })
}

/// Scan, diff against the stored anatomy, write flat + layered stores, and
/// append a non-empty diff to `ctx/changes.log` (one JSON object per line).
pub fn refresh(project: &Path) -> Result<(AnatomyStore, RefreshDiff)> {
    let _lock = StoreLock::acquire(project);
    let stored = load_flat(project);
    let mut current = scan_with(project, true)?;
    let d = diff(&stored, &current);
    carry_renamed(&stored, &mut current, &d.renamed);
    let flat_path = workspace::root(project).join("ctx/anatomy.flat.yaml");
    workspace::write_yaml(&flat_path, &current)?;
    write_layered(project, &current)?;
    if !d.is_empty() && !stored.files.is_empty() {
        use std::io::Write;
        let log = workspace::root(project).join("ctx/changes.log");
        let line = serde_json::json!({ "ts": current.scanned, "diff": &d });
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
        {
            let _ = writeln!(f, "{line}");
        }
    }
    Ok((current, d))
}

/// A renamed file keeps its description, its source, and its read history.
/// `scan_with` matches prior entries by path, so without this a rename would
/// drop an agent-written description for a fresh heuristic one.
/// (A file renamed *and* edited can't be paired and starts over.)
fn carry_renamed(stored: &AnatomyStore, current: &mut AnatomyStore, renamed: &[(String, String)]) {
    for (from, to) in renamed {
        let Some(old) = stored.files.iter().find(|f| &f.path == from) else {
            continue;
        };
        if let Some(new) = current.files.iter_mut().find(|f| &f.path == to) {
            if old.description.is_some() && old.desc_source != Some(DescSource::Heuristic) {
                new.description = old.description.clone();
                new.desc_source = old.desc_source;
            }
            new.reads = new.reads.saturating_add(old.reads);
            if new.last_read.is_none() {
                new.last_read = old.last_read.clone();
            }
        }
    }
}

/// Exclusive lock over the anatomy store's read-modify-write. `refresh` and
/// `enrich` both rewrite `anatomy.flat.yaml` whole, so without it one can
/// silently overwrite the other's changes. Held for milliseconds; a lock
/// file older than STALE is a crashed holder and gets broken.
pub struct StoreLock(Option<std::path::PathBuf>);

impl StoreLock {
    const STALE: std::time::Duration = std::time::Duration::from_secs(30);
    const WAIT: std::time::Duration = std::time::Duration::from_secs(3);

    pub fn acquire(project: &Path) -> Self {
        let path = workspace::root(project).join("ctx/anatomy.lock");
        let start = std::time::Instant::now();
        loop {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return StoreLock(Some(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|age| age > Self::STALE);
                    if stale || start.elapsed() > Self::WAIT {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                // No .kazam/ctx yet, read-only fs: proceed unlocked rather than fail a hook.
                Err(_) => return StoreLock(None),
            }
        }
    }
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        if let Some(p) = &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[derive(serde::Serialize)]
pub struct ScanDiff {
    pub new_files: Vec<String>,
    pub deleted_files: Vec<String>,
    pub changed_files: Vec<String>,
}

impl ScanDiff {
    pub fn is_empty(&self) -> bool {
        self.new_files.is_empty() && self.deleted_files.is_empty() && self.changed_files.is_empty()
    }
}

fn heuristic_description(path: &str, ext: &str) -> Option<String> {
    let filename = path.rsplit('/').next().unwrap_or(path);

    // Well-known filenames first
    let desc: Option<&str> = match filename {
        "Cargo.toml" => Some("Rust package manifest"),
        "Cargo.lock" => Some("Rust dependency lock file"),
        "package.json" => Some("Node.js package manifest"),
        "package-lock.json" => Some("Node.js dependency lock file"),
        "tsconfig.json" => Some("TypeScript configuration"),
        "README.md" | "readme.md" => Some("Project readme"),
        "CHANGELOG.md" | "changelog.md" => Some("Release changelog"),
        "LICENSE" | "LICENSE.md" => Some("License file"),
        "Makefile" => Some("Make build rules"),
        "Dockerfile" => Some("Docker container definition"),
        ".gitignore" => Some("Git ignore rules"),
        "CLAUDE.md" => Some("Claude Code project instructions"),
        "AGENTS.md" => Some("LLM authoring guide"),
        _ => None,
    };
    if let Some(d) = desc {
        return Some(d.to_string());
    }

    // Path-aware descriptions: use directory context to say *what* the file does
    if let Some(d) = path_aware_description(path, filename, ext) {
        return Some(d);
    }

    // Bare extension fallback
    match ext {
        "rs" => Some("Rust source".to_string()),
        "ts" | "tsx" => Some("TypeScript source".to_string()),
        "js" | "jsx" => Some("JavaScript source".to_string()),
        "py" => Some("Python source".to_string()),
        "go" => Some("Go source".to_string()),
        "yaml" | "yml" => Some("YAML configuration/data".to_string()),
        "json" => Some("JSON data".to_string()),
        "toml" => Some("TOML configuration".to_string()),
        "md" => Some("Markdown document".to_string()),
        "html" => Some("HTML document".to_string()),
        "css" => Some("Stylesheet".to_string()),
        "sql" => Some("SQL query/migration".to_string()),
        "sh" | "bash" | "zsh" => Some("Shell script".to_string()),
        _ => None,
    }
}

fn path_aware_description(path: &str, filename: &str, ext: &str) -> Option<String> {
    let stem = filename
        .strip_suffix(&format!(".{ext}"))
        .unwrap_or(filename);
    let parts: Vec<&str> = path.split('/').collect();

    // Detect parent directory patterns
    let parent = if parts.len() >= 2 {
        parts[parts.len() - 2]
    } else {
        ""
    };

    let label = match parent {
        "routes" | "route" => format!("{stem} route handler"),
        "controllers" | "controller" => format!("{stem} controller"),
        "models" | "model" => format!("{stem} model"),
        "middleware" | "middlewares" => format!("{stem} middleware"),
        "services" | "service" => format!("{stem} service"),
        "lib" => format!("{stem} library module"),
        "utils" | "util" | "helpers" | "helper" => format!("{stem} utility"),
        "components" => format!("{stem} component"),
        "pages" | "views" => format!("{stem} view/page"),
        "migrations" => format!("{stem} migration"),
        "tests" | "test" | "__tests__" | "spec" => format!("{stem} tests"),
        "config" | "configs" => format!("{stem} configuration"),
        "data" => format!("{stem} data/seed file"),
        _ => return None,
    };
    Some(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        workspace::ensure(dir.path()).unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/a.rs"), "pub fn alpha() {}\n").unwrap();
        fs::write(dir.path().join("src/b.rs"), "struct Beta;\n").unwrap();
        fs::write(dir.path().join("README.md"), "# Readme\n").unwrap();
        dir
    }

    #[test]
    fn refresh_hashes_and_outlines() {
        let dir = project();
        let (store, diff) = refresh(dir.path()).unwrap();
        assert_eq!(diff.added.len(), 3);
        let a = store.files.iter().find(|f| f.path == "src/a.rs").unwrap();
        assert_eq!(a.sha.as_deref().map(str::len), Some(20));
        assert_eq!(a.outline, vec!["L1 fn alpha"]);
        assert_eq!(a.desc_source, Some(DescSource::Heuristic));
    }

    #[test]
    fn refresh_detects_same_size_edit_delete_and_rename() {
        let dir = project();
        refresh(dir.path()).unwrap();
        // Same byte length: the old token-count check would have missed this.
        fs::write(dir.path().join("src/a.rs"), "pub fn omega() {}\n").unwrap();
        fs::remove_file(dir.path().join("README.md")).unwrap();
        fs::rename(dir.path().join("src/b.rs"), dir.path().join("src/beta.rs")).unwrap();
        let (_, diff) = refresh(dir.path()).unwrap();
        assert_eq!(diff.changed, vec!["src/a.rs"]);
        assert_eq!(diff.deleted, vec!["README.md"]);
        assert_eq!(
            diff.renamed,
            vec![("src/b.rs".to_string(), "src/beta.rs".to_string())]
        );
        assert!(diff.added.is_empty());
        let log = fs::read_to_string(workspace::root(dir.path()).join("ctx/changes.log")).unwrap();
        assert_eq!(log.lines().count(), 1);
    }

    #[test]
    fn warm_refresh_is_empty_and_keeps_agent_descriptions() {
        let dir = project();
        let (mut store, _) = refresh(dir.path()).unwrap();
        let a = store
            .files
            .iter_mut()
            .find(|f| f.path == "src/a.rs")
            .unwrap();
        a.description = Some("hand written".into());
        a.desc_source = Some(DescSource::Agent);
        workspace::write_yaml(
            &workspace::root(dir.path()).join("ctx/anatomy.flat.yaml"),
            &store,
        )
        .unwrap();
        let (store, diff) = refresh(dir.path()).unwrap();
        assert!(diff.is_empty());
        let a = store.files.iter().find(|f| f.path == "src/a.rs").unwrap();
        assert_eq!(a.description.as_deref(), Some("hand written"));
        assert_eq!(a.desc_source, Some(DescSource::Agent));
    }

    #[test]
    fn legacy_custom_description_counts_as_agent() {
        let dir = project();
        let (mut store, _) = refresh(dir.path()).unwrap();
        for f in store.files.iter_mut() {
            f.desc_source = None;
            f.sha = None;
            if f.path == "src/b.rs" {
                f.description = Some("from ctx describe".into());
            }
        }
        workspace::write_yaml(
            &workspace::root(dir.path()).join("ctx/anatomy.flat.yaml"),
            &store,
        )
        .unwrap();
        let (store, _) = refresh(dir.path()).unwrap();
        let src = |p: &str| {
            store
                .files
                .iter()
                .find(|f| f.path == p)
                .unwrap()
                .desc_source
        };
        assert_eq!(src("src/b.rs"), Some(DescSource::Agent));
        assert_eq!(src("src/a.rs"), Some(DescSource::Heuristic));
    }

    #[test]
    fn rename_keeps_agent_description_and_reads() {
        let dir = project();
        let (mut store, _) = refresh(dir.path()).unwrap();
        let b = store
            .files
            .iter_mut()
            .find(|f| f.path == "src/b.rs")
            .unwrap();
        b.description = Some("the beta struct".into());
        b.desc_source = Some(DescSource::Agent);
        b.reads = 4;
        workspace::write_yaml(
            &workspace::root(dir.path()).join("ctx/anatomy.flat.yaml"),
            &store,
        )
        .unwrap();
        fs::rename(dir.path().join("src/b.rs"), dir.path().join("src/beta.rs")).unwrap();
        let (store, diff) = refresh(dir.path()).unwrap();
        assert_eq!(diff.renamed.len(), 1);
        let beta = store
            .files
            .iter()
            .find(|f| f.path == "src/beta.rs")
            .unwrap();
        assert_eq!(beta.description.as_deref(), Some("the beta struct"));
        assert_eq!(beta.desc_source, Some(DescSource::Agent));
        assert_eq!(beta.reads, 4);
    }

    #[test]
    fn duplicate_content_is_not_a_rename() {
        let dir = project();
        fs::write(dir.path().join("src/c.rs"), "struct Beta;\n").unwrap();
        refresh(dir.path()).unwrap();
        // b.rs and c.rs share a sha; delete one, add an unrelated same-content file.
        fs::remove_file(dir.path().join("src/b.rs")).unwrap();
        fs::write(dir.path().join("src/d.rs"), "struct Beta;\n").unwrap();
        let (_, diff) = refresh(dir.path()).unwrap();
        assert!(diff.renamed.is_empty());
        assert_eq!(diff.deleted, vec!["src/b.rs"]);
        assert_eq!(diff.added, vec!["src/d.rs"]);
    }

    #[test]
    fn store_lock_serializes_and_breaks_when_released() {
        let dir = project();
        let lock_path = workspace::root(dir.path()).join("ctx/anatomy.lock");
        {
            let _held = StoreLock::acquire(dir.path());
            assert!(lock_path.exists());
            let t = std::time::Instant::now();
            let p = dir.path().to_path_buf();
            let waiter = std::thread::spawn(move || {
                let _l = StoreLock::acquire(&p);
                std::time::Instant::now()
            });
            std::thread::sleep(std::time::Duration::from_millis(150));
            drop(_held);
            let got = waiter.join().unwrap();
            assert!(got.duration_since(t) >= std::time::Duration::from_millis(150));
        }
        assert!(!lock_path.exists());
    }

    #[test]
    fn git_repo_skips_ignored_files_and_includes_nested_repos() {
        let dir = project();
        let git = |d: &std::path::Path, args: &[&str]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(d)
                .args(args)
                .output()
                .unwrap()
        };
        git(dir.path(), &["init", "-q"]);
        fs::write(dir.path().join(".gitignore"), "out/\nignored-sub/\n").unwrap();
        let ignored_sub = dir.path().join("ignored-sub");
        fs::create_dir_all(&ignored_sub).unwrap();
        git(&ignored_sub, &["init", "-q"]);
        fs::write(ignored_sub.join("core.rs"), "fn core() {}\n").unwrap();
        fs::create_dir_all(dir.path().join("out")).unwrap();
        fs::write(dir.path().join("out/gen.rs"), "fn generated() {}\n").unwrap();
        let nested = dir.path().join("sub");
        fs::create_dir_all(&nested).unwrap();
        git(&nested, &["init", "-q"]);
        fs::write(nested.join("lib.rs"), "fn nested() {}\n").unwrap();
        let (store, _) = refresh(dir.path()).unwrap();
        let paths: Vec<&str> = store.files.iter().map(|f| f.path.as_str()).collect();
        assert!(
            paths.contains(&"src/a.rs"),
            "untracked, not ignored: {paths:?}"
        );
        assert!(paths.contains(&"sub/lib.rs"), "nested repo: {paths:?}");
        assert!(!paths.contains(&"out/gen.rs"), "gitignored: {paths:?}");
        assert!(
            paths.contains(&"ignored-sub/core.rs"),
            "gitignored subrepo is still a repo: {paths:?}"
        );
    }

    #[test]
    fn check_does_not_drain_reads_log() {
        let dir = project();
        refresh(dir.path()).unwrap();
        let log = workspace::root(dir.path()).join("ctx/reads.log");
        fs::write(&log, "src/a.rs\t2026-09-27T10:00:00Z\n").unwrap();
        check(dir.path()).unwrap();
        assert!(!fs::read_to_string(&log).unwrap().is_empty());
        let (store, _) = refresh(dir.path()).unwrap();
        assert_eq!(
            store
                .files
                .iter()
                .find(|f| f.path == "src/a.rs")
                .unwrap()
                .reads,
            1
        );
    }
}

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

use crate::workspace;

use super::types::{AnatomyStore, FileEntry};

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
    let pending_reads = drain_reads_log(project);

    let mut files: Vec<FileEntry> = Vec::new();
    let now = chrono::Local::now().to_rfc3339();

    for entry in WalkDir::new(project)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_str().unwrap_or("");
            if name.starts_with('.') && name != "." {
                return false;
            }
            !SKIP_DIRS.contains(&name)
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if BINARY_EXTS.contains(&ext) {
            continue;
        }

        let rel = entry
            .path()
            .strip_prefix(project)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .to_string();

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let tokens = size / 4;

        // Preserve agent-enriched description if it exists
        let description = existing_by_path
            .get(rel.as_str())
            .and_then(|f| f.description.clone())
            .or_else(|| heuristic_description(&rel, ext));

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
    let current = scan(project)?;

    let stored_set: std::collections::HashMap<&str, &FileEntry> =
        stored.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let current_set: std::collections::HashMap<&str, &FileEntry> =
        current.files.iter().map(|f| (f.path.as_str(), f)).collect();

    let new_files: Vec<String> = current
        .files
        .iter()
        .filter(|f| !stored_set.contains_key(f.path.as_str()))
        .map(|f| f.path.clone())
        .collect();

    let deleted_files: Vec<String> = stored
        .files
        .iter()
        .filter(|f| !current_set.contains_key(f.path.as_str()))
        .map(|f| f.path.clone())
        .collect();

    let changed_files: Vec<String> = current
        .files
        .iter()
        .filter(|f| {
            stored_set
                .get(f.path.as_str())
                .map(|s| s.tokens != f.tokens)
                .unwrap_or(false)
        })
        .map(|f| f.path.clone())
        .collect();

    Ok(ScanDiff {
        new_files,
        deleted_files,
        changed_files,
    })
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

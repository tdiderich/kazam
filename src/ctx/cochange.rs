//! Git co-change graph: which files tend to change in the same commit.
//! `ctx research` uses it to pull in the files that move with a strong hit.
//!
//! Built from `git log --name-only` over the last MAX_COMMITS non-merge
//! commits and cached in `ctx/cochange.json` keyed by HEAD, so it's rebuilt
//! only when history moves. Bulk commits (formatting, syncs, renames) touch
//! too many files to say anything about coupling and are skipped.

use std::collections::HashMap;
use std::path::Path;

const MAX_COMMITS: usize = 1_500;
const MAX_FILES_PER_COMMIT: usize = 12;
const MAX_PARTNERS: usize = 8;
/// Pairs seen together fewer times than this are noise.
const MIN_TOGETHER: u32 = 2;

/// path -> [(partner, weight in 0..=1)], weight = together / changes-of-path.
pub type CoChange = HashMap<String, Vec<(String, f64)>>;

fn cache_path(project: &Path) -> std::path::PathBuf {
    crate::workspace::root(project).join("ctx/cochange.json")
}

fn git(project: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(project)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `skip` newest commits are left out. Only the research eval sets it
/// (KAZAM_COCHANGE_SKIP), so the graph can't see the commits being tested.
pub fn build(project: &Path, skip: usize) -> CoChange {
    let skip_arg = format!("--skip={skip}");
    let n_arg = format!("-n{MAX_COMMITS}");
    let Some(log) = git(
        project,
        &[
            "log",
            "--no-merges",
            &n_arg,
            &skip_arg,
            "--name-only",
            "--pretty=format:@@",
        ],
    ) else {
        return CoChange::new();
    };
    let mut changes: HashMap<String, u32> = HashMap::new();
    let mut pairs: HashMap<(String, String), u32> = HashMap::new();
    for commit in log.split("@@") {
        let files: Vec<&str> = commit
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if files.len() < 2 || files.len() > MAX_FILES_PER_COMMIT {
            for f in &files {
                *changes.entry(f.to_string()).or_default() += 1;
            }
            continue;
        }
        for f in &files {
            *changes.entry(f.to_string()).or_default() += 1;
        }
        for (i, a) in files.iter().enumerate() {
            for b in &files[i + 1..] {
                let key = if a < b {
                    (a.to_string(), b.to_string())
                } else {
                    (b.to_string(), a.to_string())
                };
                *pairs.entry(key).or_default() += 1;
            }
        }
    }
    let mut graph: CoChange = HashMap::new();
    for ((a, b), together) in pairs {
        if together < MIN_TOGETHER {
            continue;
        }
        let wa = together as f64 / changes[&a].max(1) as f64;
        let wb = together as f64 / changes[&b].max(1) as f64;
        graph.entry(a.clone()).or_default().push((b.clone(), wa));
        graph.entry(b).or_default().push((a, wb));
    }
    for partners in graph.values_mut() {
        partners.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal));
        partners.truncate(MAX_PARTNERS);
    }
    graph
}

/// Cached graph for the current HEAD; rebuilt when HEAD moves. Empty outside git.
pub fn load(project: &Path) -> CoChange {
    let skip: usize = std::env::var("KAZAM_COCHANGE_SKIP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let Some(head) = git(project, &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()) else {
        return CoChange::new();
    };
    let key = format!("{head}:{skip}");
    let path = cache_path(project);
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok((k, g)) = serde_json::from_str::<(String, CoChange)>(&text) {
            if k == key {
                return g;
            }
        }
    }
    let g = build(project, skip);
    if let Ok(s) = serde_json::to_string(&(key, &g)) {
        let _ = std::fs::write(&path, s);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_files_that_change_together() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        crate::workspace::ensure(p).unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(p)
                .args(args)
                .output()
                .unwrap()
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        for i in 0..3 {
            std::fs::write(p.join("a.rs"), format!("{i}")).unwrap();
            std::fs::write(p.join("b.rs"), format!("{i}")).unwrap();
            git(&["add", "-A"]);
            git(&["commit", "-qm", "ab"]);
        }
        std::fs::write(p.join("c.rs"), "x").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "c"]);
        let g = build(p, 0);
        assert_eq!(g["a.rs"][0].0, "b.rs");
        assert!(!g.contains_key("c.rs"));
    }
}

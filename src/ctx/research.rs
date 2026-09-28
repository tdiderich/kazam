//! `kazam ctx research "<task>"`: a token-budgeted brief of the files most
//! likely relevant to a task, ranked by BM25 over path, description, and
//! outline. Each hit carries the outline lines that match the task, so the
//! agent can Read exact line ranges instead of spending turns on grep.
//!
//! Plain lexical ranking on purpose: it runs in milliseconds at session start
//! and gets its quality from the enriched descriptions, not from a model.

use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use super::enrich;
use super::scan;
use super::types::BugStore;

const STOP: &[&str] = &[
    "the", "and", "or", "of", "to", "in", "for", "on", "with", "is", "are", "does", "do", "how",
    "what", "where", "which", "when", "why", "this", "that", "it", "be", "by", "from", "as", "at",
    "file", "files", "function", "code", "give", "me", "show", "find", "list", "an",
];

// BM25 parameters and per-field weights (path terms count 3x, description 2x).
const K1: f64 = 1.2;
const B: f64 = 0.75;
const W_PATH: usize = 3;
const W_DESC: usize = 2;
const MAX_HITS_PER_FILE: usize = 6;
const SHOW_GOTCHAS: bool = false;

#[derive(Serialize)]
pub struct Hit {
    pub path: String,
    pub tokens: u64,
    pub score: f64,
    pub description: String,
    pub outline: Vec<String>,
    pub gotchas: Vec<String>,
    pub open_bugs: Vec<String>,
}

pub fn tokenize(s: &str) -> Vec<String> {
    // Split camelCase before lowercasing so `writeLayered` matches "write layered".
    let mut spaced = String::with_capacity(s.len() + 8);
    let mut prev_lower = false;
    for c in s.chars() {
        if c.is_uppercase() && prev_lower {
            spaced.push(' ');
        }
        prev_lower = c.is_lowercase();
        spaced.push(c);
    }
    spaced
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() > 1 && !STOP.contains(t))
        .map(stem)
        .collect()
}

/// Crude suffix stripping so "writes"/"writing"/"written" meet "write" and
/// "hooks" meets "hook". Applied to both queries and documents, so it only
/// has to be consistent, not linguistically right.
fn stem(t: &str) -> String {
    let n = t.len();
    if n > 6 && t.ends_with("ing") {
        return t[..n - 3].to_string();
    }
    if n > 5 && t.ends_with("ed") {
        return t[..n - 2].to_string();
    }
    // "es" only after a sibilant ("classes", "matches"); otherwise "writes"
    // would lose its e and miss "write".
    if n > 5
        && ["ses", "xes", "zes", "ches", "shes"]
            .iter()
            .any(|s| t.ends_with(s))
    {
        return t[..n - 2].to_string();
    }
    if n > 4 && t.ends_with('s') && !t.ends_with("ss") {
        return t[..n - 1].to_string();
    }
    t.to_string()
}

pub fn research(project: &Path, task: &str, k: usize, budget: usize) -> Vec<Hit> {
    let store = scan::load_flat(project);
    let q = tokenize(task);
    if q.is_empty() {
        return vec![];
    }

    let docs: Vec<Vec<String>> = store
        .files
        .iter()
        .map(|f| {
            let mut d = Vec::new();
            let path_t = tokenize(&f.path);
            for _ in 0..W_PATH {
                d.extend(path_t.iter().cloned());
            }
            let desc_t = tokenize(f.description.as_deref().unwrap_or(""));
            for _ in 0..W_DESC {
                d.extend(desc_t.iter().cloned());
            }
            d.extend(tokenize(&f.outline.join(" ")));
            d
        })
        .collect();

    let n = docs.len() as f64;
    let avg = docs.iter().map(|d| d.len()).sum::<usize>() as f64 / n.max(1.0);
    let mut df: HashMap<&str, usize> = HashMap::new();
    for d in &docs {
        let mut seen: Vec<&str> = d.iter().map(String::as_str).collect();
        seen.sort_unstable();
        seen.dedup();
        for t in seen {
            *df.entry(t).or_default() += 1;
        }
    }

    let mut scored: Vec<(usize, f64)> = docs
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let mut tf: HashMap<&str, usize> = HashMap::new();
            for t in d {
                *tf.entry(t.as_str()).or_default() += 1;
            }
            let len = d.len() as f64;
            let s: f64 = q
                .iter()
                .filter_map(|t| {
                    let f = *tf.get(t.as_str())? as f64;
                    let dfi = *df.get(t.as_str()).unwrap_or(&0) as f64;
                    let idf = (1.0 + (n - dfi + 0.5) / (dfi + 0.5)).ln();
                    Some(idf * f * (K1 + 1.0) / (f + K1 * (1.0 - B + B * len / avg)))
                })
                .sum();
            (s > 0.0).then_some((i, s))
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let bugs: BugStore =
        crate::workspace::read_yaml(&crate::workspace::root(project).join("ctx/bugs.yaml"))
            .unwrap_or(BugStore { bugs: vec![] });

    let mut hits = Vec::new();
    let mut used = 0usize;
    for (i, score) in scored.into_iter().take(k) {
        let f = &store.files[i];
        let matching: Vec<String> = f
            .outline
            .iter()
            .filter(|l| tokenize(l).iter().any(|t| q.contains(t)))
            .take(MAX_HITS_PER_FILE)
            .cloned()
            .collect();
        let outline = if matching.is_empty() {
            f.outline.iter().take(4).cloned().collect()
        } else {
            matching
        };
        // Model gotchas stay out of the brief until they're evaluated: a 1.7B
        // model invents plausible-sounding ones (nonexistent functions), and a
        // confident wrong hint costs more than no hint.
        let gotchas: Vec<String> = if SHOW_GOTCHAS {
            f.sha
                .as_deref()
                .and_then(enrich::cached)
                .map(|e| e.gotchas.into_iter().take(1).collect())
                .unwrap_or_default()
        } else {
            vec![]
        };
        let open_bugs = bugs
            .bugs
            .iter()
            .filter(|b| b.resolved.is_none() && b.file_path.as_deref() == Some(f.path.as_str()))
            .map(|b| format!("{}: {}", b.id, b.symptom))
            .collect();
        let hit = Hit {
            path: f.path.clone(),
            tokens: f.tokens,
            score: (score * 100.0).round() / 100.0,
            description: f.description.clone().unwrap_or_default(),
            outline,
            gotchas,
            open_bugs,
        };
        let cost = render_hit(&hit).len() / 4;
        if used + cost > budget && !hits.is_empty() {
            break;
        }
        used += cost;
        hits.push(hit);
    }
    hits
}

pub fn render_hit(h: &Hit) -> String {
    let mut s = format!("\n## {}  (~{} tok)\n{}\n", h.path, h.tokens, h.description);
    for l in &h.outline {
        s.push_str(&format!("  {l}\n"));
    }
    for g in &h.gotchas {
        s.push_str(&format!("  ! {g}\n"));
    }
    for b in &h.open_bugs {
        s.push_str(&format!("  bug {b}\n"));
    }
    s
}

pub fn render(task: &str, hits: &[Hit]) -> String {
    let mut s = format!("# kazam research: {task}\n");
    if hits.is_empty() {
        s.push_str("No indexed file matches. Search normally.\n");
        return s;
    }
    s.push_str("Read the cited line ranges directly (Read with offset/limit); search only if nothing here fits.\n");
    for h in hits {
        s.push_str(&render_hit(h));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_splits_camel_snake_and_drops_stopwords() {
        assert_eq!(
            tokenize("Where is writeLayered in ctx/scan_rs?"),
            vec!["write", "layer", "ctx", "scan", "rs"]
        );
        assert_eq!(tokenize("writes hooks"), tokenize("write hook"));
    }
}

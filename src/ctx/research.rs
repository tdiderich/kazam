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

/// How much to trust a brief. Computed from signals research already has, so
/// a hook can drop a guess and an agent can weigh the rest.
#[derive(Serialize, Clone)]
pub struct Confidence {
    /// 0..1
    pub score: f64,
    /// "high" | "medium" | "low"
    pub level: &'static str,
    pub reasons: Vec<String>,
    /// Distinct app roots among the full-tier hits.
    pub spread: usize,
    /// Top score / 4th score.
    pub margin: f64,
    /// idf-weighted share of query terms found in the top two hits.
    pub coverage: f64,
    /// The prompt names an app root.
    pub named_scope: Option<String>,
}

#[derive(Serialize)]
pub struct Brief {
    pub confidence: Confidence,
    pub hits: Vec<Hit>,
}

/// Monorepo-aware root: `apps/reports/...` -> `apps/reports`,
/// `src/ctx/scan.rs` -> `src`, `README.md` -> `(root)`.
pub fn app_root(path: &str) -> String {
    let mut parts = path.split('/');
    let first = parts.next().unwrap_or("");
    let Some(second) = parts.next() else {
        return "(root)".into();
    };
    if parts.next().is_some()
        && matches!(
            first,
            "apps"
                | "packages"
                | "services"
                | "libs"
                | "crates"
                | "shared"
                | "projects"
                | "modules"
        )
    {
        format!("{first}/{second}")
    } else {
        first.to_string()
    }
}

const SCOPE_BOOST: f64 = 2.5;

#[derive(Serialize)]
pub struct Hit {
    /// "full": description + outline lines. "line": path + short description,
    /// the cheap tier for the files around the top hits.
    pub tier: &'static str,
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

/// Drop what isn't the request: URLs, HTML tags, markdown heading markers,
/// co-author/sign-off trailers, and PR/issue numbers. They're common in
/// pasted commit messages and PR text and only add noise terms.
fn clean_query(task: &str) -> String {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let rx = R.get_or_init(|| {
        regex::Regex::new(
            r"(?mi)https?://\S+|<[^>]+>|^#+\s*|^(co-authored-by|signed-off-by|claude-session):.*$|\(#\d+\)|#\d+",
        )
        .unwrap()
    });
    rx.replace_all(task, " ").into_owned()
}

/// Test files match task words well (their names describe behavior) but are
/// rarely where the change goes; rank them below source.
fn is_test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.split('/')
        .any(|seg| matches!(seg, "test" | "tests" | "__tests__" | "spec" | "fixtures"))
        || name.starts_with("test_")
        || [
            "_test.go",
            "_test.py",
            ".test.ts",
            ".test.tsx",
            ".spec.ts",
            ".spec.tsx",
            ".stories.tsx",
        ]
        .iter()
        .any(|s| name.ends_with(s))
}
const TEST_WEIGHT: f64 = 0.35;
/// How much a strong hit lends to files that historically change with it.
const COCHANGE_WEIGHT: f64 = 0.6;
const COCHANGE_SEEDS: usize = 3;

/// How strongly a file must co-change with a top hit to be listed as
/// "usually changes with" in the line tier.
const LINE_COCHANGE_MIN: f64 = 0.4;
const LINE_DESC_CHARS: usize = 110;

/// Full-tier hits plus up to `lines` one-line entries: co-change partners of
/// the top hits first (docs and siblings that usually move with them), then
/// the next-ranked files. ~25 tokens a line, so the agent sees the
/// neighborhood without a turn spent exploring it.
pub fn research_brief(project: &Path, task: &str, k: usize, budget: usize, lines: usize) -> Brief {
    let store = scan::load_flat(project);
    let q = tokenize(&clean_query(task));
    if q.is_empty() {
        return Brief {
            confidence: Confidence {
                score: 0.0,
                level: "low",
                reasons: vec!["empty query".into()],
                spread: 0,
                margin: 0.0,
                coverage: 0.0,
                named_scope: None,
            },
            hits: vec![],
        };
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
    for (i, sc) in scored.iter_mut() {
        if is_test_path(&store.files[*i].path) {
            *sc *= TEST_WEIGHT;
        }
    }
    // A prompt that names an app ("outbound-activity: ...", "in apps/reports")
    // is scoped: rank that app's files well above generic-word matches elsewhere.
    let lower_task = clean_query(task).to_lowercase();
    let named_scope: Option<String> = {
        let mut roots: Vec<String> = store.files.iter().map(|f| app_root(&f.path)).collect();
        roots.sort();
        roots.dedup();
        roots
            .into_iter()
            // Only monorepo app roots (apps/x, packages/x). A plain top-level
            // folder name (src, docs, render) collides with everyday words.
            .filter(|r| r.contains('/'))
            .filter(|r| {
                let leaf = r.rsplit('/').next().unwrap_or(r);
                leaf.len() >= 4
                    && (lower_task.contains(&leaf.to_lowercase())
                        || lower_task.contains(&r.to_lowercase()))
            })
            .max_by_key(|r| r.len())
    };
    if let Some(root) = &named_scope {
        for (i, sc) in scored.iter_mut() {
            if app_root(&store.files[*i].path) == *root {
                *sc *= SCOPE_BOOST;
            }
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Co-change expansion: tasks usually touch a file *and* the files that
    // move with it (workflow + activities, handler + route). Seed from the top
    // non-test hits and lend each partner a share of the seed's score scaled
    // by how often they changed together.
    let cc = super::cochange::load(project);
    if !cc.is_empty() {
        let index: HashMap<&str, usize> = store
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.path.as_str(), i))
            .collect();
        let mut bonus: HashMap<usize, f64> = HashMap::new();
        for &(i, sc) in scored
            .iter()
            .filter(|(i, _)| !is_test_path(&store.files[*i].path))
            .take(COCHANGE_SEEDS)
        {
            if let Some(partners) = cc.get(&store.files[i].path) {
                for (p, w) in partners {
                    if let Some(&j) = index.get(p.as_str()) {
                        if !is_test_path(p) {
                            *bonus.entry(j).or_default() += sc * COCHANGE_WEIGHT * w;
                        }
                    }
                }
            }
        }
        for (i, sc) in scored.iter_mut() {
            if let Some(b) = bonus.remove(i) {
                *sc += b;
            }
        }
        scored.extend(bonus);
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    }

    let bugs: BugStore =
        crate::workspace::read_yaml(&crate::workspace::root(project).join("ctx/bugs.yaml"))
            .unwrap_or(BugStore { bugs: vec![] });

    let mut hits = Vec::new();
    let mut used = 0usize;
    let mut consumed = 0usize;
    for &(i, score) in scored.iter().take(k) {
        consumed += 1;
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
            tier: "full",
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
    let confidence = confidence_of(&store, &docs, &df, n, &q, &scored, &hits, named_scope);
    if lines == 0 || hits.is_empty() {
        return Brief { confidence, hits };
    }

    let taken: std::collections::HashSet<String> = hits.iter().map(|h| h.path.clone()).collect();
    let mut line_paths: Vec<String> = Vec::new();
    let cc = super::cochange::load(project);
    for h in &hits {
        for (p, w) in cc.get(&h.path).into_iter().flatten() {
            if *w >= LINE_COCHANGE_MIN && !taken.contains(p) && !line_paths.contains(p) {
                line_paths.push(p.clone());
            }
        }
    }
    line_paths.truncate(lines / 2);
    for &(i, _) in scored.iter().skip(consumed) {
        if line_paths.len() >= lines {
            break;
        }
        let p = &store.files[i].path;
        if !taken.contains(p) && !line_paths.contains(p) && !is_test_path(p) {
            line_paths.push(p.clone());
        }
    }
    let by_path: HashMap<&str, &super::types::FileEntry> =
        store.files.iter().map(|f| (f.path.as_str(), f)).collect();
    for p in line_paths {
        let Some(f) = by_path.get(p.as_str()) else {
            continue;
        };
        let mut desc = f.description.clone().unwrap_or_default();
        if desc.len() > LINE_DESC_CHARS {
            let cut = (0..=LINE_DESC_CHARS)
                .rev()
                .find(|&n| desc.is_char_boundary(n))
                .unwrap_or(0);
            desc.truncate(cut);
            desc.push_str("...");
        }
        hits.push(Hit {
            tier: "line",
            path: p,
            tokens: f.tokens,
            score: 0.0,
            description: desc,
            outline: vec![],
            gotchas: vec![],
            open_bugs: vec![],
        });
    }
    Brief { confidence, hits }
}

#[allow(clippy::too_many_arguments)]
fn confidence_of(
    store: &super::types::AnatomyStore,
    docs: &[Vec<String>],
    df: &HashMap<&str, usize>,
    n: f64,
    q: &[String],
    scored: &[(usize, f64)],
    hits: &[Hit],
    named_scope: Option<String>,
) -> Confidence {
    let full: Vec<&Hit> = hits.iter().filter(|h| h.tier == "full").collect();
    if full.is_empty() {
        return Confidence {
            score: 0.0,
            level: "low",
            reasons: vec!["nothing in the index matched".into()],
            spread: 0,
            margin: 0.0,
            coverage: 0.0,
            named_scope,
        };
    }
    let mut roots: Vec<String> = full.iter().take(4).map(|h| app_root(&h.path)).collect();
    roots.sort();
    roots.dedup();
    let spread = roots.len();
    let top = scored.first().map(|x| x.1).unwrap_or(0.0);
    let fourth = scored.get(3).or(scored.last()).map(|x| x.1).unwrap_or(top);
    let margin = if fourth > 0.0 { top / fourth } else { 1.0 };
    let idf = |t: &str| {
        let d = *df.get(t).unwrap_or(&0) as f64;
        (1.0 + (n - d + 0.5) / (d + 0.5)).ln()
    };
    let mut uniq: Vec<&String> = q.iter().collect();
    uniq.sort();
    uniq.dedup();
    let total: f64 = uniq.iter().map(|t| idf(t)).sum();
    let top_docs: Vec<&Vec<String>> = scored.iter().take(2).map(|(i, _)| &docs[*i]).collect();
    let covered: f64 = uniq
        .iter()
        .filter(|t| top_docs.iter().any(|d| d.contains(t)))
        .map(|t| idf(t))
        .sum();
    let coverage = if total > 0.0 { covered / total } else { 0.0 };
    let top_path = &full[0].path;
    let described = store
        .files
        .iter()
        .find(|f| &f.path == top_path)
        .and_then(|f| f.desc_source)
        .is_some_and(|s| s != super::types::DescSource::Heuristic);

    // Weights from calibration on 120 commit tasks across 4 enriched repos:
    // a clear top match (margin) and concentration (spread) separate right
    // briefs from wrong ones; term coverage didn't, so it's reported only.
    let mut score: f64 = 0.5;
    let mut reasons = Vec::new();
    if margin >= 1.6 {
        score += 0.2;
        reasons.push("clear top match".into());
    } else if margin < 1.2 {
        score -= 0.15;
        reasons.push("no clear top match".into());
    }
    match spread {
        1 => {
            score += 0.1;
            reasons.push(format!("top hits all in {}", roots[0]));
        }
        2 => {}
        _ => {
            score -= 0.2;
            reasons.push(format!("top hits spread across {spread} areas"));
        }
    }
    match &named_scope {
        Some(root) if app_root(top_path) == *root => {
            score += 0.1;
            reasons.push(format!("prompt names {root}"));
        }
        Some(root) => {
            score -= 0.2;
            reasons.push(format!("prompt names {root} but the top hit is elsewhere"));
        }
        None => {}
    }
    score += if described { 0.05 } else { -0.05 };
    let score = score.clamp(0.0, 1.0);
    let level = if score >= HIGH_CONFIDENCE {
        "high"
    } else if score >= LOW_CONFIDENCE {
        "medium"
    } else {
        "low"
    };
    Confidence {
        score: (score * 100.0).round() / 100.0,
        level,
        reasons,
        spread,
        margin: (margin * 100.0).round() / 100.0,
        coverage: (coverage * 100.0).round() / 100.0,
        named_scope,
    }
}

pub const HIGH_CONFIDENCE: f64 = 0.7;
pub const LOW_CONFIDENCE: f64 = 0.45;

/// One line for the top of a brief: level, why, and how to use it.
pub fn render_confidence(c: &Confidence) -> String {
    let how = match c.level {
        "high" => "read these first",
        "medium" => "good leads, verify before relying on them",
        _ => "weak match, search normally",
    };
    let why = if c.reasons.is_empty() {
        String::new()
    } else {
        format!(": {}", c.reasons.join("; "))
    };
    format!("confidence: {} ({:.2}){why}. {how}.\n", c.level, c.score)
}

pub fn render_hit(h: &Hit) -> String {
    if h.tier == "line" {
        return format!("- {}  {}\n", h.path, h.description);
    }
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

pub fn render(task: &str, brief: &Brief) -> String {
    let hits = &brief.hits;
    let mut s = format!("# kazam research: {task}\n");
    if hits.is_empty() {
        s.push_str("No indexed file matches. Search normally.\n");
        return s;
    }
    s.push_str(&render_confidence(&brief.confidence));
    s.push_str("Read the cited line ranges directly (Read with offset/limit); search only if nothing here fits.\n");
    s.push_str(&render_hits(hits));
    s
}

/// Full-tier entries, then the one-line tier under its own heading.
pub fn render_hits(hits: &[Hit]) -> String {
    let mut s = String::new();
    let mut in_lines = false;
    for h in hits {
        if h.tier == "line" && !in_lines {
            s.push_str("\n## Also nearby (one line each; open only if needed)\n");
            in_lines = true;
        }
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

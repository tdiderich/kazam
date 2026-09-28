//! `kazam ctx brief-hook`: the automatic version of `ctx research`, run from
//! a UserPromptSubmit hook. Reads the hook payload on stdin, decides whether
//! the prompt is a code task worth a brief, and if so prints the brief as
//! `additionalContext` so it lands before the agent's first turn: zero extra
//! turns, unlike a tool call.
//!
//! The gate matters more than the brief. A reminder that fires on chat,
//! approvals, or writing tasks is pure context bloat (the lesson of the old
//! voice hook), so this errs toward silence: short prompts, notifications,
//! slash commands, and writing requests never fire, and a prompt with no
//! code-shaped words needs a clearly stronger match.

use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::research::{self, Hit};

const MIN_WORDS: usize = 6;
/// Top BM25 score below this: nothing in the index clearly matches.
const MIN_SCORE: f64 = 10.0;
/// Without code-shaped words in the prompt, require a much stronger match.
const MIN_SCORE_NO_CODE_SIGNAL: f64 = 20.0;
const K: usize = 4;
/// One-line tier size: neighbors of the top hits, ~25 tokens each.
const LINES: usize = 6;
const AGENT_LINES: usize = 8;
const BUDGET: usize = 700;
/// Subagents explore more than the main agent's first turn: a wider brief.
const AGENT_K: usize = 6;
const AGENT_BUDGET: usize = 1_100;

fn writing_intent() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?i)\b(draft|slack|e-?mail|linkedin|newsletter|announce(ment)?|reply to|respond to|write (me )?(a|an|the) (message|post|note|summary|update)|call (prep|debrief)|customer|deal|pipeline)\b",
        )
        .unwrap()
    })
}

fn code_signal() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            // Case-sensitive on purpose for camelCase; keywords are (?i:) scoped.
            r"(\w+\.(rs|py|ts|tsx|js|go|md|ya?ml|toml|json|sh|sql)\b|\w+::\w+|\w+_\w+|\b[a-z]+[A-Z]\w*|(?i:\b(function|fn|struct|class|method|module|file|code|bug|error|panic|stack ?trace|test|build|compile|implement|refactor|fix|where is|defined|endpoint|schema|hook|config|cli|flag|query|migration)\b))",
        )
        .unwrap()
    })
}

const NOTIFICATION_MARKERS: &[&str] = &[
    "<task-notification>",
    "[system notification",
    "<agent-message",
    "<cross-session-message",
    "[subagent hand-back]",
    "<bash-input>",
    "<command-name>",
    "this session is being continued from a previous conversation",
];

/// Only the start of a prompt drives the gate and the ranking: the request is
/// usually up front, and a long untagged paste after it would otherwise
/// rank files on the pasted content.
const RANK_WORDS: usize = 60;

/// Why a prompt got no brief. `None` from `gate` means: brief it.
#[derive(Debug, PartialEq)]
pub enum Skip {
    Notification,
    SlashCommand,
    TooShort,
    WritingTask,
    NoIndex,
    WeakMatch,
    SameAsLast,
    LowConfidence,
}

/// The part of a prompt the user actually typed. Pasted blobs, shell
/// output, and command echoes are stripped: their words would match the
/// index on content, not intent (a pasted stack trace or table ranks files
/// the task isn't about).
pub fn typed_text(prompt: &str) -> String {
    static R: OnceLock<Regex> = OnceLock::new();
    let rx = R.get_or_init(|| {
        Regex::new(
            r"(?s)<(pasted_content|bash-stdout|bash-stderr|bash-input|local-command-stdout|command-message|command-args|system-reminder)[^>]*>.*?</(pasted_content|bash-stdout|bash-stderr|bash-input|local-command-stdout|command-message|command-args|system-reminder)[^>]*>",
        )
        .unwrap()
    });
    let stripped = rx.replace_all(prompt, " ");
    // Unwrapped pastes: drop lines that look like terminal or log output.
    stripped
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !(t.starts_with('$')
                || t.contains("@") && t.contains(" % ")
                || t.starts_with("at ")
                || t.starts_with("Traceback")
                || t.starts_with("20") && t.chars().nth(4) == Some('-'))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .split_whitespace()
        .take(RANK_WORDS)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn prompt_gate(prompt: &str) -> Option<Skip> {
    let lower = prompt.to_lowercase();
    if NOTIFICATION_MARKERS.iter().any(|m| lower.contains(m)) {
        return Some(Skip::Notification);
    }
    if prompt.trim_start().starts_with('/') || lower.contains("<local-command-stdout>") {
        return Some(Skip::SlashCommand);
    }
    let prompt = &typed_text(prompt);
    if prompt.split_whitespace().count() < MIN_WORDS {
        return Some(Skip::TooShort);
    }
    if writing_intent().is_match(prompt) && !code_signal().is_match(prompt) {
        return Some(Skip::WritingTask);
    }
    None
}

pub fn score_gate(prompt: &str, hits: &[Hit]) -> Option<Skip> {
    let top = hits.first().map(|h| h.score).unwrap_or(0.0);
    let need = if code_signal().is_match(prompt) {
        MIN_SCORE
    } else {
        MIN_SCORE_NO_CODE_SIGNAL
    };
    (top < need).then_some(Skip::WeakMatch)
}

/// Nearest ancestor of `start` holding an anatomy store.
fn project_root(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    start
        .ancestors()
        .find(|a| a.join(".kazam/ctx/anatomy.flat.yaml").is_file())
        .map(Path::to_path_buf)
}

/// Follow-up prompts in the same thread usually rank the same files; don't
/// inject the same brief twice in a row.
fn same_as_last(project: &Path, session: &str, hits: &[Hit]) -> bool {
    if session.is_empty() {
        return false;
    }
    let safe: String = session
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let dir = crate::workspace::root(project).join("ctx/brief-last");
    let path = dir.join(safe);
    let key = hits
        .iter()
        .map(|h| h.path.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let prev = std::fs::read_to_string(&path).unwrap_or_default();
    if prev == key {
        return true;
    }
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&path, key);
    false
}

/// One JSON line per brief in `ctx/briefs.log`: session and briefed paths.
/// Joined later against the session transcript to measure whether the agent
/// actually read what it was briefed on (the hit rate), without an A/B.
fn log_fire(project: &Path, session: &str, kind: &str, hits: &[Hit], c: &research::Confidence) {
    use std::io::Write;
    let line = serde_json::json!({
        "ts": chrono::Local::now().to_rfc3339(),
        "session": session,
        "kind": kind,
        "files": hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
        "top_score": hits.first().map(|h| h.score),
        "confidence": c.score,
        "level": c.level,
    });
    let log = crate::workspace::root(project).join("ctx/briefs.log");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
    {
        let _ = writeln!(f, "{line}");
    }
}

pub struct Decision {
    pub skip: Option<Skip>,
    pub brief: Option<String>,
    pub confidence: Option<f64>,
}

/// `record`: write the fire to briefs.log and the per-session dedupe marker.
/// Off for `--dry-run`, so replays leave no trace.
pub fn decide(prompt: &str, cwd: &Path, session: &str, record: bool) -> Decision {
    if let Some(skip) = prompt_gate(prompt) {
        return Decision {
            skip: Some(skip),
            brief: None,
            confidence: None,
        };
    }
    // Rank on what the user typed, not on pasted output.
    let prompt = &typed_text(prompt);
    let Some(project) = project_root(cwd) else {
        return Decision {
            skip: Some(Skip::NoIndex),
            brief: None,
            confidence: None,
        };
    };
    let brief_r = research::research_brief(&project, prompt, K, BUDGET, LINES);
    let hits = &brief_r.hits;
    if let Some(skip) = score_gate(prompt, hits) {
        return Decision {
            skip: Some(skip),
            brief: None,
            confidence: Some(brief_r.confidence.score),
        };
    }
    // A low-confidence brief is a guess; a wrong brief costs more than none.
    if brief_r.confidence.level == "low" {
        return Decision {
            skip: Some(Skip::LowConfidence),
            brief: None,
            confidence: Some(brief_r.confidence.score),
        };
    }
    if record && same_as_last(&project, session, hits) {
        return Decision {
            skip: Some(Skip::SameAsLast),
            brief: None,
            confidence: Some(brief_r.confidence.score),
        };
    }
    if record {
        log_fire(&project, session, "prompt", hits, &brief_r.confidence);
    }
    let mut brief = String::from(
        "[kazam brief] Files the index ranks as most relevant to this prompt, with \
         line-numbered outlines. Read the cited ranges directly (offset/limit) before \
         searching; ignore this if the task isn't about this code.\n",
    );
    brief.push_str(&research::render_confidence(&brief_r.confidence));
    brief.push_str(&research::render_hits(hits));
    Decision {
        skip: None,
        brief: Some(brief),
        confidence: Some(brief_r.confidence.score),
    }
}

/// PreToolUse on the Agent tool: append a brief to the subagent's prompt via
/// `updatedInput`. Subagents never see the workspace rules, so this is the
/// only way an explore subagent starts from the index instead of grepping.
/// Tested live: Claude Code applies `updatedInput` to Agent calls.
fn agent_hook(v: &serde_json::Value, dry_run: bool) {
    let input = &v["tool_input"];
    let task = input["prompt"].as_str().unwrap_or_default();
    let cwd = v["cwd"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    let skip = |why: &str| {
        if dry_run {
            println!("{}", serde_json::json!({ "fire": false, "skip": why }));
        }
    };
    if task.contains("[kazam brief]") || task.split_whitespace().count() < MIN_WORDS {
        return skip("AlreadyBriefedOrShort");
    }
    let Some(project) = project_root(&cwd) else {
        return skip("NoIndex");
    };
    let brief_r = research::research_brief(&project, task, AGENT_K, AGENT_BUDGET, AGENT_LINES);
    let hits = &brief_r.hits;
    // Subagent prompts are written by the main agent as task descriptions,
    // so the code-signal bar is the normal one.
    if hits.first().map(|h| h.score).unwrap_or(0.0) < MIN_SCORE {
        return skip("WeakMatch");
    }
    if brief_r.confidence.level == "low" {
        return skip("LowConfidence");
    }
    let mut brief = String::from(
        "\n\n[kazam brief] The project index ranks these files as most relevant to this \
         task, with line-numbered outlines. Start by Reading the cited ranges \
         (offset/limit); search only if they don't cover it.\n",
    );
    brief.push_str(&research::render_confidence(&brief_r.confidence));
    brief.push_str(&research::render_hits(hits));
    if dry_run {
        println!(
            "{}",
            serde_json::json!({ "fire": true, "chars": brief.len(), "files": hits.len() })
        );
        return;
    }
    log_fire(
        &project,
        v["session_id"].as_str().unwrap_or_default(),
        "agent",
        hits,
        &brief_r.confidence,
    );
    let mut updated = input.clone();
    updated["prompt"] = serde_json::Value::String(format!("{task}{brief}"));
    println!(
        "{}",
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": updated,
            }
        })
    );
}

/// Hook entry point: payload on stdin, hook JSON (or nothing) on stdout.
/// Never fails the hook: any error is silence.
pub fn run_hook(dry_run: bool, agent: bool) {
    use std::io::Read;
    if std::env::var("KAZAM_BRIEF").as_deref() == Ok("0") && !dry_run {
        return;
    }
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let v: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    if agent {
        return agent_hook(&v, dry_run);
    }
    let prompt = v["prompt"]
        .as_str()
        .or_else(|| v["user_prompt"].as_str())
        .unwrap_or_default();
    let cwd = v["cwd"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    let session = v["session_id"].as_str().unwrap_or_default();
    let d = decide(prompt, &cwd, session, !dry_run);
    if dry_run {
        println!(
            "{}",
            serde_json::json!({
                "fire": d.brief.is_some(),
                "skip": d.skip.map(|s| format!("{s:?}")),
                "chars": d.brief.as_ref().map(|b| b.len()),
                "confidence": d.confidence,
            })
        );
        return;
    }
    if let Some(brief) = d.brief {
        println!(
            "{}",
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "UserPromptSubmit",
                    "additionalContext": brief,
                }
            })
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_skips_chat_notifications_and_writing() {
        assert_eq!(prompt_gate("Yep"), Some(Skip::TooShort));
        assert_eq!(
            prompt_gate(
                "<task-notification> done with the thing you asked for </task-notification>"
            ),
            Some(Skip::Notification)
        );
        assert_eq!(
            prompt_gate("/review the latest changes on this branch please"),
            Some(Skip::SlashCommand)
        );
        assert_eq!(
            prompt_gate("can you draft a slack message to the team about the release"),
            Some(Skip::WritingTask)
        );
    }

    #[test]
    fn pasted_and_tool_output_is_ignored() {
        let p = "this first split compare isn't needed <pasted_content id=\"x\"> SEVERITY BEFORE MAZE Scanner View Critical config file build error </pasted_content>";
        let typed = typed_text(p);
        assert_eq!(typed.split_whitespace().count(), 6);
        assert!(!typed.contains("SEVERITY") && !typed.contains("config"));
        let short = "fix this <pasted_content id=\"y\"> a long pasted stack trace with many words in it </pasted_content>";
        assert_eq!(prompt_gate(short), Some(Skip::TooShort));
        assert_eq!(
            prompt_gate("<local-command-stdout>Compacted (ctrl+o to see full summary)</local-command-stdout>"),
            Some(Skip::SlashCommand)
        );
    }

    #[test]
    fn gate_passes_code_questions_even_with_writing_words() {
        assert_eq!(
            prompt_gate(
                "Which hook events does kazam register in .claude/settings.json for Claude Code?"
            ),
            None
        );
        assert_eq!(
            prompt_gate("fix the bug where the slack send hook blocks every message"),
            None
        );
    }

    #[test]
    fn weak_match_needs_more_without_code_signal() {
        let hit = |score| Hit {
            tier: "full",
            path: "a".into(),
            tokens: 1,
            score,
            description: String::new(),
            outline: vec![],
            gotchas: vec![],
            open_bugs: vec![],
        };
        let chatty = "okay I think we will have enough data to get started here";
        assert_eq!(score_gate(chatty, &[hit(15.0)]), Some(Skip::WeakMatch));
        assert_eq!(
            score_gate("where is the config loader defined", &[hit(15.0)]),
            None
        );
        assert_eq!(
            score_gate("where is the config loader defined", &[hit(5.0)]),
            Some(Skip::WeakMatch)
        );
    }
}

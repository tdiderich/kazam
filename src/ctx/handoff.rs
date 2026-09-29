//! Clear handoff: `/clear` as instant compaction.
//!
//! The Stop hook rebuilds a snapshot of the session after every turn
//! (`handoff stop`) into a per-session file, plus a short core. On /clear the
//! SessionStart hook prints only the core (`handoff load`), which points at the
//! full file: hook output over ~10k chars gets spilled to disk by Claude Code,
//! so injecting the whole snapshot silently lost most of it. Clearing costs nothing at clear time because
//! the work already happened, and nothing on either path calls a model: the
//! snapshot is the user's prompts verbatim, the agent's own final reports,
//! files touched, commits, the uncommitted diff, and kazam's task and
//! correction stores.
//!
//! Snapshots are written per session id (`<sid>.md`, `<sid>.core.md`), so a
//! new session never overwrites the one it was cleared from. The Claude Code
//! process gets a small pointer (`pid-N.json`) naming its current session:
//! /clear starts a new session id in the same process, and looking up by
//! process keeps two sessions open in one repo from reloading each other's
//! state.
//!
//! Replay A/B (2026-09-28, maze-apps): /clear plus a ~1.1k-token version of
//! this matched full-context quality at 11.7% of the tokens. The budget here
//! is larger on purpose (quality over thrift; the startup prefix is ~75k).

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Approximate token cap for the full snapshot file. Only the core is
/// injected, so this can run long.
pub const BUDGET_TOKENS: usize = 25_000;
/// Nudge once context passes this and a task just wrapped.
const NUDGE_TOKENS: u64 = 150_000;
/// A fallback snapshot older than this isn't trusted for a keyless reload.
const FALLBACK_MAX_AGE_SECS: u64 = 12 * 3600;

// ── Transcript model ─────────────────────────────

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub n: usize,
    pub ts: String,
    pub prompt: String,
    /// Read calls as `path` or `path:start-end`.
    pub reads: Vec<String>,
    pub edits: Vec<String>,
    pub bash: Vec<String>,
    pub agents: Vec<String>,
    /// The agent's last text block in the turn.
    pub report: String,
    /// The user stopped the turn before the agent finished.
    #[serde(default)]
    pub interrupted: bool,
}

/// Work the session left running: a `run_in_background` Bash, a Monitor, or a
/// detached (`nohup`, trailing `&`) process. Recorded so the next session has
/// a handle to check instead of a "still watching" it can't follow.
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bg {
    /// `bash`, `monitor`, or `detached`.
    pub kind: String,
    /// Claude Code's task id, when it gave one.
    #[serde(default)]
    pub id: String,
    pub what: String,
    pub command: String,
    /// Where a background Bash writes its output.
    #[serde(default)]
    pub output: String,
    pub started: String,
    /// From the task's notification: completed, failed, killed, expired.
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_event: String,
    #[serde(default)]
    pub timeout_ms: u64,
}

#[derive(Default, Debug)]
pub struct Session {
    pub turns: Vec<Turn>,
    pub bg: Vec<Bg>,
    /// Log and status files named in commands, most recent last.
    pub logs: Vec<String>,
    /// The newest compaction summary, if the session was compacted.
    pub compact_summary: Option<String>,
    /// Turns up to this number are covered by `compact_summary`.
    pub compacted_through: usize,
    /// Context size of the last assistant call (input + cache read + write).
    pub context_tokens: u64,
}

fn strip_tag(s: &str, tag: &str) -> String {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find(&open) {
        out.push_str(&rest[..i]);
        match rest[i..].find(&close) {
            Some(j) => rest = &rest[i + j + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Claude Code's own slash commands: harness actions, not requests.
const BUILTIN_COMMANDS: &[&str] = &[
    "clear",
    "compact",
    "model",
    "config",
    "help",
    "cost",
    "status",
    "context",
    "resume",
    "fast",
    "login",
    "logout",
    "memory",
    "permissions",
    "hooks",
    "agents",
    "mcp",
    "ide",
    "theme",
    "vim",
    "exit",
    "doctor",
    "usage",
    "export",
    "plugin",
    "rewind",
    "statusline",
];

/// Text of a message or tool-result content: a string or text blocks.
fn text_of(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// A command minus heredoc bodies, which are data (scripts, file
/// contents), not shell.
fn shell_only(cmd: &str) -> String {
    let mut out = String::new();
    let mut delim: Option<String> = None;
    for l in cmd.lines() {
        if let Some(d) = &delim {
            if l.trim() == d {
                delim = None;
            }
            continue;
        }
        out.push_str(l);
        out.push('\n');
        if let Some((_, rest)) = l.split_once("<<") {
            let d = rest
                .trim_start_matches('-')
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_matches(['\'', '"']);
            if !d.is_empty() {
                delim = Some(d.to_string());
            }
        }
    }
    out
}

/// A shell command that leaves a process running after it returns.
fn is_detached(cmd: &str) -> bool {
    let cmd = &shell_only(cmd);
    cmd.contains("nohup ")
        || cmd.contains("setsid ")
        || cmd.contains("disown")
        || cmd.contains(" & ")
        || cmd.lines().any(|l| {
            let l = l.trim_end();
            l.ends_with('&') && !l.ends_with("&&")
        })
}

/// A string `pgrep -f` can find a detached command by: the program it
/// launched, past `nohup`, env assignments, flags, and interpreters.
fn detached_key(cmd: &str) -> Option<String> {
    let cmd = &shell_only(cmd);
    let seg = cmd
        .split(['\n', ';'])
        .flat_map(|l| l.split("&&"))
        .find(|s| s.contains("nohup ") || s.contains("setsid ") || is_detached(&format!("{s}\n")))
        .unwrap_or(cmd);
    let skip = [
        "nohup", "setsid", "exec", "env", "bash", "sh", "zsh", "python", "python3", "node", "uv",
        "run", "cargo",
    ];
    seg.split_whitespace()
        .filter(|t| !t.contains('=') && !t.starts_with('-') && !skip.contains(t))
        .find(|t| !t.starts_with('>') && !t.starts_with('&') && !t.starts_with('2'))
        .map(|t| {
            Path::new(t.trim_matches(['"', '\'']))
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| t.to_string())
        })
        .filter(|k| k.len() > 2)
}

/// Path-ish tokens in free text, trailing punctuation trimmed.
fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_whitespace() || "\"'`()[]<>,|;".contains(c))
        .map(|t| t.trim_end_matches(['.', ':', ',', ')', '*']))
        .filter(|t| !t.is_empty())
}

/// Log and status files a command reads or writes.
fn log_paths(cmd: &str) -> Vec<String> {
    tokens(cmd)
        .filter(|t| {
            (t.ends_with(".log") || t.ends_with(".status")) && !is_scratch(t) && !t.starts_with('*')
        })
        .map(str::to_string)
        .collect()
}

/// Things a later session may need that the core may not carry: URLs,
/// host:port, home-relative paths, log/status files, docs.
fn mentions(text: &str) -> Vec<String> {
    tokens(text)
        .filter(|t| {
            t.starts_with("http://")
                || t.starts_with("https://")
                || t.starts_with("localhost:")
                || t.starts_with("127.0.0.1:")
                // Home-relative files, not bare directories.
                || (t.starts_with("~/")
                    && Path::new(t).extension().is_some_and(|e| e.len() <= 5))
                || ((t.ends_with(".log") || t.ends_with(".status")) && !t.starts_with(".."))
        })
        .filter(|t| !is_scratch(t))
        .map(str::to_string)
        .collect()
}

/// What the human typed, minus harness wrappers. None for anything that isn't
/// a real prompt (tool results, notifications, local command output).
fn prompt_text(d: &serde_json::Value) -> Option<String> {
    if d["isMeta"].as_bool() == Some(true) || d["isSidechain"].as_bool() == Some(true) {
        return None;
    }
    if let Some(kind) = d["origin"]["kind"].as_str() {
        if kind != "human" {
            return None;
        }
    }
    let content = &d["message"]["content"];
    let raw = match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => {
            if blocks.iter().any(|b| b["type"] == "tool_result") {
                return None;
            }
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => return None,
    };
    let t = raw.trim_start();
    if t.starts_with("<local-command")
        || t.starts_with("<task-notification")
        || t.starts_with("Caveat: The messages below")
        || t.starts_with("[Request interrupted")
        || t.starts_with("<bash-input>")
        || t.starts_with("<bash-stdout>")
        || t.starts_with("<bash-stderr>")
    {
        return None;
    }
    let mut s = strip_tag(&raw, "system-reminder");
    // `/cmd args` arrives as tags; keep it readable. Harness commands
    // (/clear, /model, ...) aren't requests; skills and custom commands are.
    if s.contains("<command-name>") {
        let name = between(&s, "<command-name>", "</command-name>").unwrap_or_default();
        if BUILTIN_COMMANDS.contains(&name.trim_start_matches('/')) {
            return None;
        }
        let args = between(&s, "<command-args>", "</command-args>").unwrap_or_default();
        s = format!("{name} {args}");
    }
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn between(s: &str, a: &str, b: &str) -> Option<String> {
    let i = s.find(a)? + a.len();
    let j = s[i..].find(b)? + i;
    Some(s[i..j].trim().to_string())
}

pub fn parse_transcript(text: &str, root: &Path) -> Session {
    let root_prefix = format!("{}/", root.display());
    let rel = |p: &str| p.strip_prefix(&root_prefix).unwrap_or(p).to_string();
    let mut s = Session::default();
    // tool_use id -> index in s.bg, until its result names the task.
    let mut pending: std::collections::HashMap<String, usize> = Default::default();
    for line in text.lines() {
        let Ok(d) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match d["type"].as_str() {
            Some("user") if d["isSidechain"].as_bool() != Some(true) => {
                let content = &d["message"]["content"];
                for b in content.as_array().into_iter().flatten() {
                    if b["type"] != "tool_result" {
                        continue;
                    }
                    let Some(&i) = b["tool_use_id"].as_str().and_then(|id| pending.get(id)) else {
                        continue;
                    };
                    let tr = &d["toolUseResult"];
                    if let Some(id) = tr["backgroundTaskId"].as_str().or(tr["taskId"].as_str()) {
                        s.bg[i].id = id.to_string();
                    }
                    let body = text_of(&b["content"]);
                    if let Some((_, rest)) = body.split_once("Output is being written to: ") {
                        let p = rest.split_whitespace().next().unwrap_or_default();
                        s.bg[i].output = p.trim_end_matches('.').to_string();
                    }
                }
                if d["origin"]["kind"] == "task-notification" {
                    let body = text_of(content);
                    if let Some(tid) = between(&body, "<task-id>", "</task-id>") {
                        for bg in s.bg.iter_mut().filter(|b| b.id == tid) {
                            if let Some(st) = between(&body, "<status>", "</status>") {
                                bg.status = st;
                            } else if body.contains("Monitor expired") {
                                bg.status = "expired".into();
                            }
                            if let Some(ev) = between(&body, "<event>", "</event>") {
                                bg.last_event = one_line(&ev, 200);
                            }
                        }
                    }
                    continue;
                }
                if text_of(content)
                    .trim_start()
                    .starts_with("[Request interrupted")
                {
                    if let Some(t) = s.turns.last_mut() {
                        t.interrupted = true;
                    }
                    continue;
                }
                // `! cmd` from the user: part of the running conversation, not a turn.
                if let Some(cmd) = content
                    .as_str()
                    .and_then(|c| between(c, "<bash-input>", "</bash-input>"))
                {
                    if let Some(t) = s.turns.last_mut() {
                        t.prompt
                            .push_str(&format!("\n\n[user ran `{}`]", clip(&cmd, 200)));
                    }
                    continue;
                }
                if d["isCompactSummary"].as_bool() == Some(true) {
                    if let Some(t) = d["message"]["content"].as_str() {
                        s.compact_summary = Some(t.to_string());
                        s.compacted_through = s.turns.len();
                    }
                    continue;
                }
                if let Some(p) = prompt_text(&d) {
                    let n = s.turns.len() + 1;
                    s.turns.push(Turn {
                        n,
                        ts: d["timestamp"].as_str().unwrap_or_default().to_string(),
                        prompt: p,
                        ..Default::default()
                    });
                }
            }
            // Messages typed while the agent was working land as attachments
            // on the running turn. They often carry the decisions.
            Some("attachment")
                if d["attachment"]["type"] == "queued_command"
                    && d["attachment"]["origin"]["kind"] == "human" =>
            {
                let p = d["attachment"]["prompt"]
                    .as_str()
                    .unwrap_or_default()
                    .trim();
                if let (Some(turn), false) = (s.turns.last_mut(), p.is_empty()) {
                    turn.prompt.push_str("\n\n[sent mid-turn] ");
                    turn.prompt.push_str(p);
                }
            }
            Some("assistant") if d["isSidechain"].as_bool() != Some(true) => {
                let msg = &d["message"];
                let u = &msg["usage"];
                let ctx = u["input_tokens"].as_u64().unwrap_or(0)
                    + u["cache_read_input_tokens"].as_u64().unwrap_or(0)
                    + u["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                if ctx > 0 {
                    s.context_tokens = ctx;
                }
                let Some(turn) = s.turns.last_mut() else {
                    continue;
                };
                for b in msg["content"].as_array().into_iter().flatten() {
                    match b["type"].as_str() {
                        Some("text") => {
                            let t = b["text"].as_str().unwrap_or_default().trim();
                            if !t.is_empty() {
                                turn.report = t.to_string();
                            }
                        }
                        Some("tool_use") => {
                            let inp = &b["input"];
                            let fp = inp["file_path"].as_str().map(rel);
                            let ts = d["timestamp"].as_str().unwrap_or(&turn.ts).to_string();
                            let tool_id = b["id"].as_str().unwrap_or_default().to_string();
                            let name = b["name"].as_str().unwrap_or_default();
                            if matches!(name, "Bash" | "Monitor") {
                                let c = inp["command"].as_str().unwrap_or_default();
                                s.logs.extend(log_paths(c));
                                let what = inp["description"]
                                    .as_str()
                                    .map(|d| one_line(d, 120))
                                    .unwrap_or_else(|| one_line(c, 120));
                                let kind = if name == "Monitor" {
                                    Some("monitor")
                                } else if inp["run_in_background"].as_bool() == Some(true) {
                                    Some("bash")
                                } else if is_detached(c) && detached_key(c).is_some() {
                                    Some("detached")
                                } else {
                                    None
                                };
                                if let Some(kind) = kind {
                                    if kind != "detached" {
                                        pending.insert(tool_id, s.bg.len());
                                    }
                                    s.bg.push(Bg {
                                        kind: kind.into(),
                                        what,
                                        command: clip(c, 400),
                                        started: ts,
                                        timeout_ms: inp["timeout_ms"].as_u64().unwrap_or(0),
                                        ..Default::default()
                                    });
                                }
                            }
                            match (name, fp) {
                                ("Read", Some(p)) => {
                                    let r = match (inp["offset"].as_u64(), inp["limit"].as_u64()) {
                                        (Some(o), Some(l)) => format!("{p}:{o}-{}", o + l),
                                        (Some(o), None) => format!("{p}:{o}-"),
                                        _ => p,
                                    };
                                    turn.reads.push(r);
                                }
                                ("Edit" | "Write" | "MultiEdit" | "NotebookEdit", Some(p)) => {
                                    turn.edits.push(p)
                                }
                                ("Bash", _) => {
                                    if let Some(c) = inp["command"].as_str() {
                                        turn.bash.push(clip(c.lines().next().unwrap_or(c), 160));
                                    }
                                }
                                ("Agent" | "Task", _) => {
                                    if let Some(c) = inp["description"].as_str() {
                                        turn.agents.push(clip(c, 100));
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    s
}

// ── Rendering ────────────────────────────────────

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str(" […]");
    out
}

fn one_line(s: &str, max: usize) -> String {
    clip(&s.split_whitespace().collect::<Vec<_>>().join(" "), max)
}

fn est_tokens(s: &str) -> usize {
    s.len() / 4 + 1
}

fn uniq_recent(items: impl DoubleEndedIterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<String> = items.rev().filter(|x| seen.insert(x.clone())).collect();
    out.truncate(40);
    out
}

fn git(root: &Path, args: &[&str]) -> String {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Session-local scratch (Claude's scratchpad, /tmp) isn't worth reloading.
fn is_scratch(p: &str) -> bool {
    p.starts_with("/tmp/") || p.starts_with("/private/tmp/") || p.starts_with("/var/folders/")
}

/// Transcript timestamps are `...Z`; older task records use a space for `T`.
fn parse_ts(s: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    let s = s.trim();
    let fixed = if s.len() > 10 && s.as_bytes()[10] == b' ' {
        format!("{}T{}", &s[..10], &s[11..])
    } else {
        s.to_string()
    };
    chrono::DateTime::parse_from_rfc3339(&fixed).ok()
}

fn cut_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    &s[..cut]
}

/// Git state of one repo this session edited in. Nested repos (a workspace
/// wrapping several clones) each get their own, so the branch and commits of
/// the repo actually being worked on aren't hidden behind the wrapper's.
#[derive(Default)]
pub struct RepoState {
    /// Path relative to the project root, `.` for the root itself.
    pub label: String,
    pub branch: String,
    pub commits: String,
    /// `git status --short` lines for the files this session changed.
    pub status: String,
    /// Dirty files in the repo that this session didn't change.
    pub other_dirty: usize,
    /// Whether `other_dirty` is measured against the session-start baseline
    /// (they really were dirty before) or only against main-thread edits
    /// (subagent and Bash edits may be among them).
    pub baseline_known: bool,
    /// `3 unpushed`, `no upstream`, or empty when in sync.
    pub upstream: String,
    pub diff: String,
}

/// Everything outside the transcript: git state and kazam's stores.
#[derive(Default)]
pub struct Surroundings {
    pub repos: Vec<RepoState>,
    pub tasks: Vec<String>,
    pub corrections: Vec<String>,
    /// The subset worth injecting: on touched files, in active repos, general.
    pub core_corrections: Vec<String>,
    pub learnings: Vec<String>,
    /// `kazam save` entries from this session, and recent decisions from any.
    pub saves: Vec<String>,
    pub decisions: Vec<String>,
    /// Background work and watched files, with status checked at render time.
    pub bg: Vec<String>,
    pub logs: Vec<String>,
}

/// Paths in `git status --short` output (rename targets, quotes stripped).
fn status_paths(status: &str) -> Vec<String> {
    status
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| {
            let p = &l[3..];
            let p = p.rsplit_once(" -> ").map_or(p, |(_, b)| b);
            p.trim_matches('"').to_string()
        })
        .collect()
}

fn repo_state(
    top: &Path,
    label: String,
    edited: &[String],
    baseline: Option<&[String]>,
    since: &str,
) -> RepoState {
    let commits = if since.is_empty() {
        String::new()
    } else {
        git(
            top,
            &[
                "log",
                "--oneline",
                "--no-decorate",
                &format!("--since={since}"),
                "-25",
            ],
        )
    };
    let all = git(top, &["status", "--short", "--untracked-files=normal"]);
    // This session's files: its main-thread edits, plus (when the dirty set at
    // session start is known) anything dirtied since, which is where subagent
    // and Bash edits show up.
    let mut files: Vec<String> = edited.to_vec();
    if let Some(base) = baseline {
        for p in status_paths(&all) {
            if !base.contains(&p) && !files.contains(&p) {
                files.push(p);
            }
        }
    }
    let status: String = all
        .lines()
        .filter(|l| {
            status_paths(l).first().is_some_and(|p| {
                files
                    .iter()
                    .any(|f| f == p || (p.ends_with('/') && f.starts_with(p.as_str())))
            })
        })
        .map(|l| format!("{l}\n"))
        .collect();
    let mut diff = String::new();
    if !files.is_empty() {
        let mut args = vec!["diff", "HEAD", "--"];
        args.extend(files.iter().map(String::as_str));
        diff = git(top, &args);
    }
    let branch = git(top, &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_string();
    let upstream = if branch.is_empty() || branch == "HEAD" {
        String::new()
    } else {
        match git(top, &["rev-list", "--count", "@{u}..HEAD"]).trim() {
            "" => "no upstream".to_string(),
            "0" => String::new(),
            n => format!("{n} unpushed"),
        }
    };
    RepoState {
        label,
        branch,
        commits,
        other_dirty: all.lines().count().saturating_sub(status.lines().count()),
        baseline_known: baseline.is_some(),
        upstream,
        status,
        diff,
    }
}

fn surroundings(root: &Path, s: &Session, sid: &str, st: &State) -> Surroundings {
    // Since the start of the /clear chain, not of this session: a session
    // cleared from one that committed on a branch still needs that branch.
    let since = if st.chain_start.is_empty() {
        s.turns.first().map(|t| t.ts.clone()).unwrap_or_default()
    } else {
        st.chain_start.clone()
    };
    let edited = uniq_recent(s.turns.iter().flat_map(|t| t.edits.clone()));
    // Everything the session touched, not just Edit/Write calls: edits made
    // through Bash or subagents never show up as edits, and a repo worked on
    // that way would otherwise vanish from the handoff.
    let touched = touched_paths(root, s);
    let touched_set: std::collections::HashSet<&str> = touched.iter().map(String::as_str).collect();

    // Group in-project paths by the git repo that owns them; only edits feed
    // the per-file status and diff.
    let root_top = PathBuf::from(git(root, &["rev-parse", "--show-toplevel"]).trim());
    let mut tops: std::collections::HashMap<PathBuf, PathBuf> = Default::default();
    let mut by_repo: std::collections::BTreeMap<PathBuf, Vec<String>> = Default::default();
    let edited_set: std::collections::HashSet<&str> = edited.iter().map(String::as_str).collect();
    for e in touched.iter().filter(|e| !e.starts_with('/')) {
        let abs = root.join(e);
        let dir = abs.parent().unwrap_or(root).to_path_buf();
        let top = tops
            .entry(dir.clone())
            .or_insert_with(|| {
                let mut d = dir.clone();
                // The dir may be gone (file deleted); walk up to one that exists.
                while !d.is_dir() && d.pop() {}
                let top = PathBuf::from(git(&d, &["rev-parse", "--show-toplevel"]).trim());
                top.canonicalize().unwrap_or(top)
            })
            .clone();
        if top.as_os_str().is_empty() {
            continue;
        }
        let rel = abs
            .strip_prefix(&top)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| e.clone());
        let files = by_repo.entry(top).or_default();
        if edited_set.contains(e.as_str()) {
            files.push(rel);
        }
    }
    if !root_top.as_os_str().is_empty() {
        by_repo
            .entry(root_top.canonicalize().unwrap_or(root_top))
            .or_default();
    }
    // Nested clones (a workspace wrapping several repos): any with commits
    // since the session started belongs in the handoff, touched or not.
    if let Ok(rd) = fs::read_dir(root) {
        for d in rd.flatten().map(|e| e.path()) {
            if d.join(".git").exists() {
                by_repo.entry(d.canonicalize().unwrap_or(d)).or_default();
            }
        }
    }
    // Repos earlier sessions in the chain worked in stay, active or not.
    for label in &st.repos {
        let p = if label == "." {
            root.to_path_buf()
        } else {
            root.join(label)
        };
        if p.is_dir() {
            by_repo.entry(p.canonicalize().unwrap_or(p)).or_default();
        }
    }
    let canon_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut repos: Vec<RepoState> = by_repo
        .iter()
        .map(|(top, files)| {
            let ct = top.canonicalize().unwrap_or_else(|_| top.clone());
            let label = match ct.strip_prefix(&canon_root) {
                Ok(p) if p.as_os_str().is_empty() => ".".to_string(),
                Ok(p) => p.display().to_string(),
                Err(_) => top.display().to_string(),
            };
            let base = st.baseline.get(&label).map(Vec::as_slice);
            repo_state(top, label, files, base, &since)
        })
        // A repo with nothing from this chain (the wrapper, usually) is noise.
        .filter(|r| {
            !r.status.trim().is_empty()
                || !r.commits.trim().is_empty()
                || !r.diff.trim().is_empty()
                || (r.label != "." && st.repos.contains(&r.label))
        })
        .collect();
    // Repos this session edited in first, the root last.
    repos.sort_by_key(|r| r.label == ".");

    // Tasks in flight: claimed, or created/updated during this session.
    let start = parse_ts(&since);
    let mut tasks = vec![];
    if let Ok(store) = crate::track::store::read_tasks(root) {
        use crate::track::types::TaskStatus;
        for t in store.tasks.iter() {
            let touched = start.zip(parse_ts(&t.updated)).is_some_and(|(a, b)| b >= a);
            if t.status != TaskStatus::Active && !touched {
                continue;
            }
            let status = t.status.label();
            let note = match (&t.status, t.close_reason.as_deref(), t.note.as_deref()) {
                (TaskStatus::Closed, Some(r), _) => format!("\n  closed: {}", one_line(r, 300)),
                (_, _, Some(n)) => format!("\n  note: {}", one_line(n, 400)),
                _ => String::new(),
            };
            tasks.push(format!(
                "- {} [p{}, {status}] {}{note}",
                t.id,
                t.priority,
                one_line(&t.title, 200)
            ));
        }
        // Open high-priority work the session didn't touch is still in flight
        // for whoever picks up next.
        let listed: Vec<String> = tasks.clone();
        let mut extra: Vec<&crate::track::types::Task> = store
            .tasks
            .iter()
            .filter(|t| {
                t.priority <= 1
                    && matches!(
                        t.status,
                        TaskStatus::Open | TaskStatus::Active | TaskStatus::Blocked
                    )
                    && !listed
                        .iter()
                        .any(|l| l.starts_with(&format!("- {} ", t.id)))
            })
            .collect();
        extra.sort_by_key(|t| t.priority);
        for t in extra.into_iter().take(6) {
            tasks.push(format!(
                "- {} [p{}, {}] {} (not touched this session)",
                t.id,
                t.priority,
                t.status.label(),
                one_line(&t.title, 160)
            ));
        }
    }
    // Open work first, closed last.
    tasks.sort_by_key(|t| t.contains(", closed]"));

    let mut corrections = vec![];
    let mut core_corrections = vec![];
    if let Ok(store) =
        crate::workspace::read_yaml::<super::types::CorrectionStore>(&super::corrections_path(root))
    {
        let fmt = |c: &super::types::Correction| {
            format!(
                "- {}: {} -> {}",
                c.file_path.as_deref().unwrap_or("general"),
                one_line(&c.mistake, 160),
                one_line(&c.correction, 240)
            )
        };
        // Repos this session is active in, by their path under the root.
        let active: Vec<String> = repos
            .iter()
            .filter(|r| r.label != ".")
            .map(|r| format!("{}/", r.label))
            .collect();
        let tier = |c: &super::types::Correction| -> u8 {
            match c.file_path.as_deref().filter(|f| *f != "general") {
                Some(f)
                    if touched_set.contains(f) || touched_set.iter().any(|e| e.starts_with(f)) =>
                {
                    0
                }
                Some(f) if active.iter().any(|a| f.starts_with(a.as_str())) => 1,
                None => 2,
                Some(_) => 3,
            }
        };
        let mut ranked: Vec<(u8, &super::types::Correction)> = store
            .corrections
            .iter()
            .rev()
            .map(|c| (tier(c), c))
            .collect();
        ranked.sort_by_key(|(t, _)| *t);
        let mut per = [0usize; 4];
        let caps = [8, 4, 3, 3];
        let mut seen = std::collections::HashSet::new();
        for (t, c) in ranked {
            let t = t as usize;
            // The same correction recorded twice is one correction.
            if !seen.insert(fmt(c)) {
                continue;
            }
            if per[t] < caps[t] {
                per[t] += 1;
                // Touched file, same repo, and general ones go in the core too.
                if t < 3 {
                    core_corrections.push(fmt(c));
                }
                corrections.push(fmt(c));
            }
        }
    }

    let mut learnings = vec![];
    if let Ok(store) =
        crate::workspace::read_yaml::<super::types::LearningStore>(&super::learnings_path(root))
    {
        learnings.extend(
            store
                .learnings
                .iter()
                .rev()
                .take(5)
                .map(|l| format!("- [{}] {}", l.category.label(), one_line(&l.text, 240))),
        );
    }
    let all_saves = super::save::read_saves(root);
    let saves = all_saves
        .iter()
        .filter(|v| v.session == sid || st.chain.contains(&v.session))
        .rev()
        .take(8)
        .map(|v| clip(&super::save::fmt_save(v), 600))
        .collect();
    let decisions = super::save::recent_decisions(&all_saves, 8)
        .into_iter()
        .map(|d| clip(&d, 240))
        .collect();

    // Background work: this session's, plus unfinished work carried from
    // earlier sessions in the chain.
    let mut bg: Vec<Bg> = st.bg.clone();
    for b in &s.bg {
        bg.retain(|x| !(x.kind == b.kind && x.id == b.id && x.command == b.command));
        bg.push(b.clone());
    }
    let mut logs: Vec<String> = st.logs.clone();
    logs.extend(s.logs.iter().cloned());
    let logs = uniq_recent(logs.into_iter());
    Surroundings {
        repos,
        tasks,
        corrections,
        core_corrections,
        learnings,
        saves,
        decisions,
        bg: bg_lines(&bg),
        logs: log_lines(&logs),
    }
}

fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(h)) => PathBuf::from(h).join(rest),
        _ => PathBuf::from(p),
    }
}

fn age_of(p: &Path) -> Option<u64> {
    fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
}

fn ago(secs: u64) -> String {
    match secs {
        s if s < 90 => format!("{s}s ago"),
        s if s < 5400 => format!("{}m ago", s / 60),
        s if s < 172_800 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}

/// The last non-empty line of a file, reading only its tail.
fn last_line(p: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = fs::File::open(p) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(4096)));
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf);
    text.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| one_line(l, 160))
        .unwrap_or_default()
}

/// Processes running `key`. A shell whose `-c` script merely mentions it
/// (an agent's `tail x.sh.log`, this hook's own caller) isn't running it.
fn pgrep(key: &str) -> Vec<String> {
    let pids: Vec<String> = Command::new("pgrep")
        .args(["-f", key])
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    pids.into_iter()
        .filter(|p| {
            Command::new("ps")
                .args(["-o", "args=", "-p", p])
                .output()
                .ok()
                .is_some_and(|o| {
                    let a = String::from_utf8_lossy(&o.stdout);
                    !a.trim().is_empty() && !a.contains(" -c ")
                })
        })
        .collect()
}

/// One line per background item, with its status checked now.
fn bg_lines(bg: &[Bg]) -> Vec<String> {
    let now = chrono::Utc::now();
    bg.iter()
        .rev()
        .take(8)
        .map(|b| {
            let started = parse_ts(&b.started);
            let status = match b.kind.as_str() {
                "monitor" => {
                    let expired = b.status == "expired"
                        || started.is_some_and(|t| {
                            b.timeout_ms > 0
                                && now.signed_duration_since(t).num_milliseconds() as u64
                                    > b.timeout_ms
                        });
                    if expired {
                        "expired; re-arm to keep watching".to_string()
                    } else if !b.status.is_empty() {
                        b.status.clone()
                    } else {
                        "watching".to_string()
                    }
                }
                "detached" => match detached_key(&b.command).map(|k| (pgrep(&k), k)) {
                    Some((pids, k)) if !pids.is_empty() => {
                        format!("running (`{k}`, pid {})", pids.join(","))
                    }
                    Some((_, k)) => format!("not running (no process matches `{k}`)"),
                    None => "unknown (can't tell what it launched)".to_string(),
                },
                _ if !b.status.is_empty() => b.status.clone(),
                _ => match age_of(Path::new(&b.output)) {
                    Some(a) => format!(
                        "no completion notice; output updated {}: {}",
                        ago(a),
                        last_line(Path::new(&b.output))
                    ),
                    None => "no completion notice; output file gone".to_string(),
                },
            };
            let mut line = format!("- [{}] {}: {status}", b.kind, b.what);
            if !b.last_event.is_empty() {
                let _ = write!(line, "\n  last event: {}", b.last_event);
            }
            if b.kind != "bash" || b.output.is_empty() {
                let _ = write!(line, "\n  command: `{}`", one_line(&b.command, 240));
            } else {
                let _ = write!(line, "\n  output: {}", b.output);
            }
            line
        })
        .collect()
}

/// Watched log/status files: how fresh, and their last line.
fn log_lines(logs: &[String]) -> Vec<String> {
    logs.iter()
        .take(5)
        .filter_map(|l| {
            let p = expand_home(l);
            let a = age_of(&p)?;
            Some(format!("- {l}: updated {}: {}", ago(a), last_line(&p)))
        })
        .collect()
}

/// Project-relative paths this session touched: edits, reads (range
/// stripped), and project paths named in Bash commands.
fn touched_paths(root: &Path, s: &Session) -> Vec<String> {
    let root_prefix = format!("{}/", root.display());
    let from_bash = s.turns.iter().flat_map(|t| t.bash.iter()).flat_map(|cmd| {
        cmd.split(|c: char| c.is_whitespace() || "\"'`;|&()=<>".contains(c))
            .filter_map(|tok| tok.strip_prefix(&root_prefix))
            .map(|p| p.trim_end_matches('/').to_string())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
    });
    let reads = s
        .turns
        .iter()
        .flat_map(|t| t.reads.iter())
        .map(|r| match r.rsplit_once(':') {
            Some((p, range)) if range.chars().all(|c| c.is_ascii_digit() || c == '-') => {
                p.to_string()
            }
            _ => r.clone(),
        });
    uniq_recent(
        s.turns
            .iter()
            .flat_map(|t| t.edits.clone())
            .chain(reads)
            .chain(from_bash)
            .filter(|p| !is_scratch(p))
            .collect::<Vec<_>>()
            .into_iter(),
    )
}

fn session_line(s: &Session, session_id: &str) -> String {
    format!(
        "Session {} · {} turns · last turn {} · context was ~{}k tokens\n",
        session_id,
        s.turns.len(),
        s.turns
            .last()
            .map(|t| local_ts(&t.ts))
            .unwrap_or_else(|| "?".into()),
        s.context_tokens / 1000
    )
}

fn render_repos(out: &mut String, env: &Surroundings, max_commits: usize) {
    if env.repos.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "### Repos (as of {})\n",
        chrono::Local::now().format("%H:%M")
    );
    for r in &env.repos {
        let up = if r.upstream.is_empty() {
            String::new()
        } else {
            format!(" ({})", r.upstream)
        };
        let _ = writeln!(out, "**{}** on `{}`{up}", r.label, r.branch);
        let commits: Vec<&str> = r.commits.lines().take(max_commits).collect();
        if !commits.is_empty() {
            let _ = writeln!(
                out,
                "commits since the session began:\n{}",
                commits.join("\n")
            );
        }
        if !r.status.trim().is_empty() {
            let _ = writeln!(
                out,
                "uncommitted (this session's files):\n{}",
                r.status.trim_end()
            );
        }
        if r.other_dirty > 0 {
            if r.baseline_known {
                let _ = writeln!(
                    out,
                    "(+{} files already uncommitted before the session began)",
                    r.other_dirty
                );
            } else {
                let _ = writeln!(
                    out,
                    "(+{} other uncommitted files: not edited in the main thread, but subagent or Bash edits may be among them)",
                    r.other_dirty
                );
            }
        }
        let _ = writeln!(out);
    }
}

/// The report's closing question, if it ended on one: "keep going" doesn't
/// answer it, so the next agent should see it's still open.
fn open_question(report: &str) -> Option<String> {
    let l = report
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("```"))?;
    let l = l.trim_matches(['*', '_', ' ']);
    l.ends_with('?').then(|| clip(l, 400))
}

fn render_bg(out: &mut String, env: &Surroundings) {
    if !env.bg.is_empty() || !env.logs.is_empty() {
        let _ = writeln!(
            out,
            "### Background work (checked {})\n",
            chrono::Local::now().format("%H:%M")
        );
        for l in env.bg.iter().chain(env.logs.iter()) {
            let _ = writeln!(out, "{l}");
        }
        let _ = writeln!(out);
    }
}

/// Local time for a transcript timestamp; the stores use local time too.
fn local_ts(ts: &str) -> String {
    parse_ts(ts)
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| ts.to_string())
}

/// The part injected on /clear. Held under the hook output limit (Claude Code
/// spills anything over ~10k chars to a file and shows a 2KB preview), so it
/// carries only what the next action depends on and points at the full file.
pub const CORE_MAX_BYTES: usize = 9_000;

pub fn render_core(s: &Session, env: &Surroundings, session_id: &str, full: &Path) -> String {
    let mut head = String::new();
    let _ = writeln!(head, "## kazam handoff: where the last session left off\n");
    let _ = writeln!(
        head,
        "Rebuilt from the previous agent's transcript, git, and .kazam/ (no model). The user's words are \
         verbatim; the report is that agent's final message (yours, if you just ran /clear). Treat it as \
         your memory.\n\n\
         **Full snapshot:** `{}`. Read it before acting if the next request continues this work; \
         \"Only in the full snapshot\" below lists what's there and not here. One turn in full: \
         `kazam ctx handoff show --session {} --turn N`.\n",
        full.display(),
        session_id
    );
    let _ = writeln!(head, "{}", session_line(s, session_id));
    let Some(last) = s.turns.last() else {
        return head;
    };
    let interrupted = if last.interrupted {
        ", interrupted before the agent finished"
    } else {
        ""
    };
    let _ = writeln!(
        head,
        "### Where things stand\n\nLast request (turn {}{interrupted}):\n{}\n",
        last.n,
        clip(&last.prompt, 2500)
    );
    if let Some(q) = open_question(&last.report) {
        let _ = writeln!(
            head,
            "**Waiting on the user:** the report ended on a question, still unanswered unless the next \
             message answers it:\n> {q}\n"
        );
    }

    let mut tail = String::new();
    render_bg(&mut tail, env);
    if !env.tasks.is_empty() {
        let _ = writeln!(
            tail,
            "### kazam tasks in flight\n{}\n",
            env.tasks.join("\n")
        );
    }
    render_repos(&mut tail, env, 5);
    if !env.saves.is_empty() {
        let _ = writeln!(
            tail,
            "### Saved in this session (kazam save)\n{}\n",
            env.saves.join("\n")
        );
    }
    if !env.decisions.is_empty() {
        let _ = writeln!(
            tail,
            "### Decisions (newest first)\n{}\n",
            env.decisions.join("\n")
        );
    }
    if !env.core_corrections.is_empty() {
        let _ = writeln!(
            tail,
            "### Standing corrections for this work (do not repeat)\n{}\n",
            env.core_corrections.join("\n")
        );
    }
    let tail = cut_marked(
        &tail,
        CORE_MAX_BYTES / 2,
        "\n[…more in the full snapshot]\n",
    );

    let mut out = head;
    if !last.report.is_empty() {
        let room = CORE_MAX_BYTES.saturating_sub(out.len() + tail.len() + 1400);
        let report = last.report.trim();
        let body = if report.len() > room {
            format!(
                "{}\n[…rest of this report in the full snapshot]",
                cut_bytes(report, room)
            )
        } else {
            report.to_string()
        };
        let _ = writeln!(out, "What you reported:\n{body}\n");
    } else {
        let _ = writeln!(
            out,
            "What you reported: nothing (the turn was interrupted or never finished).\n"
        );
    }
    out.push_str(&tail);

    // Pointers the core doesn't carry, so the agent knows the snapshot has
    // something for it (paths, URLs, logs named in earlier turns).
    let mut only: Vec<String> = vec![];
    for t in s.turns.iter().rev().skip(1) {
        let text = format!("{}\n{}", t.report, t.bash.join("\n"));
        for m in mentions(&text) {
            if !out.contains(&m) && !only.iter().any(|o| o.ends_with(&format!(": {m}"))) {
                only.push(format!("- turn {}: {m}", t.n));
            }
        }
        if only.len() >= 8 {
            break;
        }
    }
    only.truncate(8);
    if !only.is_empty() {
        let _ = writeln!(out, "### Only in the full snapshot\n{}\n", only.join("\n"));
    }
    cut_marked(
        &out,
        CORE_MAX_BYTES,
        "\n[core truncated; the full snapshot has the rest]\n",
    )
}

/// `cut_bytes` that says so: a silent cut reads as complete.
fn cut_marked(s: &str, max: usize, marker: &str) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let body = cut_bytes(s, max.saturating_sub(marker.len()));
    // End on a line boundary so the marker doesn't split a line.
    let body = body.rfind('\n').map_or(body, |i| &body[..i]);
    format!("{body}{marker}")
}

/// The full snapshot, written to disk and read on demand. Highest-value
/// sections first, each under its own cap, then the diff takes what's left.
pub fn render(s: &Session, env: &Surroundings, session_id: &str, budget_tokens: usize) -> String {
    let mut out = String::new();
    let last = s.turns.last();
    let _ = writeln!(out, "## kazam handoff: full snapshot\n");
    let _ = writeln!(
        out,
        "The previous session, rebuilt by kazam from its transcript, git, and .kazam/. The user's words \
         are verbatim; reports are the agent's own final messages. Any turn in full: \
         `kazam ctx handoff show --session {session_id} --turn N`.\n"
    );
    let _ = writeln!(out, "{}", session_line(s, session_id));

    if let Some(t) = last {
        let _ = writeln!(out, "### Where things stand\n");
        let interrupted = if t.interrupted {
            ", interrupted before the agent finished"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "Last request (turn {}{interrupted}):\n{}\n",
            t.n,
            clip(&t.prompt, 4000)
        );
        if let Some(q) = open_question(&t.report) {
            let _ = writeln!(out, "**Waiting on the user:**\n> {q}\n");
        }
        if !t.report.is_empty() {
            let _ = writeln!(out, "What you reported:\n{}\n", clip(&t.report, 10000));
        }
    }
    render_bg(&mut out, env);

    let _ = writeln!(out, "### Session so far\n");
    if let Some(sum) = &s.compact_summary {
        let _ = writeln!(
            out,
            "Before the last compaction (its summary):\n{}\n",
            clip(sum, 24000)
        );
    }
    // The summary already covers turns before the compaction.
    let n = s.turns.len();
    for t in s.turns.iter().filter(|t| t.n > s.compacted_through) {
        let recent = n - t.n < 12;
        let prompt = if recent {
            clip(&t.prompt, 2000)
        } else {
            one_line(&t.prompt, 300)
        };
        let _ = write!(out, "- turn {}: {}", t.n, prompt.replace('\n', "\n  "));
        let edits = uniq_recent(t.edits.iter().filter(|e| !is_scratch(e)).cloned());
        if !edits.is_empty() {
            let _ = write!(out, "\n  edited: {}", edits.join(", "));
        }
        if !t.agents.is_empty() {
            let _ = write!(out, "\n  subagents: {}", t.agents.join("; "));
        }
        let _ = writeln!(out);
    }
    let _ = writeln!(out);

    let earlier: Vec<&Turn> = s
        .turns
        .iter()
        .rev()
        .skip(1)
        .filter(|t| !t.report.is_empty())
        .take(6)
        .collect();
    if !earlier.is_empty() {
        let _ = writeln!(out, "### Earlier reports (newest first)\n");
        for t in earlier {
            let _ = writeln!(out, "Turn {}:\n{}\n", t.n, clip(&t.report, 3000));
        }
    }

    let edited = uniq_recent(
        s.turns
            .iter()
            .flat_map(|t| t.edits.clone())
            .filter(|e| !is_scratch(e)),
    );
    let read = uniq_recent(
        s.turns
            .iter()
            .flat_map(|t| t.reads.clone())
            .filter(|e| !is_scratch(e)),
    );
    if !edited.is_empty() || !read.is_empty() {
        let _ = writeln!(out, "### Files\n");
        if !edited.is_empty() {
            let _ = writeln!(out, "Edited (most recent first): {}", edited.join(", "));
        }
        if !read.is_empty() {
            let _ = writeln!(
                out,
                "Read (most recent first): {}",
                read[..read.len().min(30)].join(", ")
            );
        }
        let _ = writeln!(out);
    }

    if !env.saves.is_empty() {
        let _ = writeln!(
            out,
            "### Saved this session (kazam save)\n{}\n",
            env.saves.join("\n")
        );
    }
    if !env.decisions.is_empty() {
        let _ = writeln!(
            out,
            "### Decisions (newest first)\n{}\n",
            env.decisions.join("\n")
        );
    }
    if !env.tasks.is_empty() {
        let _ = writeln!(out, "### kazam tasks in flight\n{}\n", env.tasks.join("\n"));
    }
    if !env.corrections.is_empty() {
        let _ = writeln!(
            out,
            "### Standing corrections (files in flight first; do not repeat)\n{}\n",
            env.corrections.join("\n")
        );
    }
    if !env.learnings.is_empty() {
        let _ = writeln!(out, "### Recent learnings\n{}\n", env.learnings.join("\n"));
    }
    render_repos(&mut out, env, 25);

    // The diff gets whatever budget is left, and the whole thing is held to the
    // cap even if the transcript sections ran long.
    let diff: String = env
        .repos
        .iter()
        .filter(|r| !r.diff.trim().is_empty())
        .map(|r| format!("# {}\n{}", r.label, r.diff))
        .collect();
    // Room for the diff after its heading, fence, and truncation note, so a
    // cut diff keeps the note instead of losing it to the final cap.
    const DIFF_OVERHEAD: usize = 300;
    let left = budget_tokens
        .saturating_sub(est_tokens(&out))
        .saturating_mul(4)
        .saturating_sub(DIFF_OVERHEAD);
    if !diff.trim().is_empty() && left > 800 {
        let d = if diff.len() > left {
            format!(
                "{}\n[diff truncated; run git diff HEAD in the repo for the rest]",
                cut_bytes(&diff, left)
            )
        } else {
            diff
        };
        let _ = writeln!(
            out,
            "### Uncommitted changes to this session's files (untracked files are listed above, not diffed)\n```diff\n{}\n```",
            d.trim_end()
        );
    }
    let cap = budget_tokens * 4;
    if out.len() > cap {
        out = cut_bytes(&out, cap).to_string();
        out.push_str("\n[handoff truncated at budget]\n");
    }
    out
}

// ── Storage ──────────────────────────────────────

fn session_dir(root: &Path) -> PathBuf {
    crate::workspace::root(root).join("session")
}

/// Whether a process is Claude Code: the native binary is `claude`; an npm
/// install runs as `node` with claude in its arguments.
fn is_claude(pid: u32, comm: &str) -> bool {
    let name = Path::new(comm)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name == "claude" {
        return true;
    }
    if name != "node" {
        return false;
    }
    Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .ok()
        .is_some_and(|o| {
            let a = String::from_utf8_lossy(&o.stdout);
            a.contains("/claude") || a.contains("claude-code")
        })
}

/// The Claude Code process this hook runs under. Survives /clear, which only
/// changes the session id.
fn claude_pid() -> Option<u32> {
    let mut pid = std::process::id();
    for _ in 0..12 {
        let out = Command::new("ps")
            .args(["-o", "ppid=,comm=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let (ppid, comm) = line.split_once(char::is_whitespace)?;
        if pid != std::process::id() && is_claude(pid, comm.trim()) {
            return Some(pid);
        }
        pid = ppid.trim().parse().ok()?;
        if pid <= 1 {
            return None;
        }
    }
    None
}

/// Per-session state, written at SessionStart and after every turn. A
/// session cleared from another inherits its chain: where the chain began,
/// the repos it worked in, what was dirty before it started, and background
/// work still running, so nothing drops out after a second /clear.
#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(default)]
struct State {
    session_id: String,
    transcript: String,
    closed_tasks: usize,
    nudged_at_turn: usize,
    /// HEAD per repo label, for the task-boundary check.
    heads: std::collections::BTreeMap<String, String>,
    /// When the first session in this /clear chain began (RFC 3339).
    chain_start: String,
    /// Earlier session ids in the chain, oldest first.
    chain: Vec<String>,
    /// Repo labels the chain has worked in.
    repos: Vec<String>,
    /// Dirty paths per repo label when the chain began.
    baseline: std::collections::BTreeMap<String, Vec<String>>,
    bg: Vec<Bg>,
    logs: Vec<String>,
}

/// The project root's repo and any nested clones, by label.
fn repos_here(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = vec![];
    if !git(root, &["rev-parse", "--show-toplevel"])
        .trim()
        .is_empty()
    {
        out.push((".".to_string(), root.to_path_buf()));
    }
    if let Ok(rd) = fs::read_dir(root) {
        for d in rd.flatten().map(|e| e.path()) {
            if d.join(".git").exists() {
                if let Some(n) = d.file_name() {
                    out.push((n.to_string_lossy().into_owned(), d.clone()));
                }
            }
        }
    }
    out
}

fn closed_count(root: &Path) -> usize {
    crate::track::store::read_tasks(root)
        .map(|s| {
            s.tasks
                .iter()
                .filter(|t| t.status == crate::track::types::TaskStatus::Closed)
                .count()
        })
        .unwrap_or(0)
}

fn heads_now(root: &Path) -> std::collections::BTreeMap<String, String> {
    repos_here(root)
        .into_iter()
        .map(|(l, p)| (l, git(&p, &["rev-parse", "HEAD"]).trim().to_string()))
        .filter(|(_, h)| !h.is_empty())
        .collect()
}

impl State {
    /// A new chain, at SessionStart: records what was already dirty, so
    /// later changes by subagents or Bash are recognizably this session's.
    fn fresh(root: &Path, sid: &str, transcript: &str) -> State {
        State {
            session_id: sid.to_string(),
            transcript: transcript.to_string(),
            closed_tasks: closed_count(root),
            heads: heads_now(root),
            chain_start: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            baseline: repos_here(root)
                .into_iter()
                .map(|(l, p)| {
                    let st = git(&p, &["status", "--short", "--untracked-files=normal"]);
                    (l, status_paths(&st))
                })
                .collect(),
            ..Default::default()
        }
    }

    /// The next session in a /clear chain.
    fn inherit(prev: &State, root: &Path, sid: &str, transcript: &str) -> State {
        let mut chain = prev.chain.clone();
        chain.push(prev.session_id.clone());
        let skip = chain.len().saturating_sub(10);
        State {
            session_id: sid.to_string(),
            transcript: transcript.to_string(),
            closed_tasks: closed_count(root),
            heads: heads_now(root),
            chain_start: prev.chain_start.clone(),
            chain: chain.split_off(skip),
            repos: prev.repos.clone(),
            baseline: prev.baseline.clone(),
            // Finished work stays with the session that ran it.
            bg: prev
                .bg
                .iter()
                .filter(|b| b.status.is_empty())
                .cloned()
                .collect(),
            logs: prev.logs.clone(),
            nudged_at_turn: 0,
        }
    }
}

fn key() -> Option<String> {
    claude_pid().map(|p| format!("pid-{p}"))
}

/// A session id safe to use as a file name.
fn sid_key(sid: &str) -> String {
    sid.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

/// Snapshot files older than this are pruned by the Stop hook.
const KEEP_SECS: u64 = 14 * 24 * 3600;

fn prune(dir: &Path) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > KEEP_SECS);
        if old && e.file_name() != ".gitignore" {
            let _ = fs::remove_file(e.path());
        }
    }
}

fn ensure_dir(dir: &Path) {
    let _ = fs::create_dir_all(dir);
    let gi = dir.join(".gitignore");
    if !gi.exists() {
        let _ = fs::write(gi, "*\n");
    }
}

fn read_state(dir: &Path, key: &str) -> State {
    fs::read_to_string(dir.join(format!("{key}.json")))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Write a session's state, and point this process at it. Without a known
/// process there's no pointer: a shared one would let two sessions reload
/// each other's work without the fallback's "might not be yours" warning.
fn write_state(dir: &Path, key: Option<&str>, st: &State) {
    let Ok(j) = serde_json::to_string(st) else {
        return;
    };
    let _ = fs::write(dir.join(format!("{}.json", sid_key(&st.session_id))), &j);
    if let Some(k) = key {
        let _ = fs::write(dir.join(format!("{k}.json")), j);
    }
}

/// The newest session state in this repo (for when no process pointer
/// matches), if it's recent and isn't `except`.
fn newest_state(dir: &Path, except: &str, max_age: u64) -> Option<State> {
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.ends_with(".json")
                && !n.starts_with("pid-")
                && n != format!("{}.json", sid_key(except))
        })
        .filter_map(|e| Some((age_of(&e.path())?, e.path())))
        .filter(|(a, _)| *a < max_age)
        .min_by_key(|(a, _)| *a)
        .and_then(|(_, p)| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<State>(&s).ok())
        .filter(|s| !s.session_id.is_empty() && !s.transcript.is_empty())
}

/// This process's current session id, from its pointer file.
pub fn current_session(project: &Path) -> Option<String> {
    let k = key()?;
    Some(read_state(&session_dir(project), &k).session_id).filter(|s| !s.is_empty())
}

pub fn log_line(root: &Path, line: &str) {
    log(root, line)
}

fn log(root: &Path, line: &str) {
    use std::io::Write;
    let p = crate::workspace::root(root).join("ctx/handoff.log");
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(
            f,
            "{}\t{line}",
            chrono::Local::now().format("%Y-%m-%dT%H:%M:%S")
        );
    }
}

fn project_root(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    start
        .ancestors()
        .find(|a| a.join(crate::workspace::DIR).is_dir())
        .map(Path::to_path_buf)
}

fn read_stdin() -> serde_json::Value {
    use std::io::Read;
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    serde_json::from_str(&input).unwrap_or_default()
}

fn disabled() -> bool {
    std::env::var("KAZAM_HANDOFF").as_deref() == Ok("0")
}

fn cwd_of(v: &serde_json::Value) -> PathBuf {
    v["cwd"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

struct Built {
    session: Session,
    env: Surroundings,
    core: String,
    full: String,
}

/// Parse a session's transcript and write its snapshot and core. Git and
/// background status are read now, so a rebuild at load time is current
/// even when commits landed after the session's last turn.
fn build(root: &Path, st: &State, last_msg: Option<&str>) -> Option<Built> {
    let text = fs::read_to_string(&st.transcript).ok()?;
    let mut session = parse_transcript(&text, root);
    let last = session.turns.last_mut()?;
    // The transcript can lag the turn that just ended; the payload's copy of
    // the final message is authoritative.
    if let Some(m) = last_msg.filter(|m| !m.trim().is_empty()) {
        last.report = m.trim().to_string();
    }
    let sid = &st.session_id;
    let env = surroundings(root, &session, sid, st);
    let dir = session_dir(root);
    ensure_dir(&dir);
    let skey = sid_key(sid);
    let full_path = dir.join(format!("{skey}.md"));
    let full = render(&session, &env, sid, BUDGET_TOKENS);
    let core = render_core(&session, &env, sid, &full_path);
    let _ = fs::write(&full_path, &full);
    let _ = fs::write(dir.join(format!("{skey}.core.md")), &core);
    // latest.core.md is the keyless fallback.
    let _ = fs::write(dir.join("latest.core.md"), &core);
    Some(Built {
        session,
        env,
        core,
        full,
    })
}

/// Rebuild a session's handoff now, for `kazam load`: `(core, full)`.
pub fn rebuild(project: &Path, sid: &str) -> Option<(String, String)> {
    let project = project
        .canonicalize()
        .unwrap_or_else(|_| project.to_path_buf());
    let project = project.as_path();
    let st = read_state(&session_dir(project), &sid_key(sid));
    if st.session_id.is_empty() || st.transcript.is_empty() {
        return None;
    }
    build(project, &st, None).map(|b| (b.core, b.full))
}

/// Stop hook: rebuild this session's snapshot, and nudge toward /clear when
/// context is large and a task just wrapped. Never fails the hook.
pub fn stop_hook() {
    if disabled() {
        return;
    }
    let v = read_stdin();
    let Some(root) = project_root(&cwd_of(&v)) else {
        return;
    };
    let (Some(sid), Some(tp)) = (v["session_id"].as_str(), v["transcript_path"].as_str()) else {
        return;
    };
    let dir = session_dir(&root);
    ensure_dir(&dir);
    prune(&dir);
    let key = key();

    // This session's state: written at SessionStart, or (hooks installed
    // mid-session, or SessionStart didn't run) started here.
    let mut st = read_state(&dir, &sid_key(sid));
    if st.session_id != sid {
        let prev = key.as_deref().map(|k| read_state(&dir, k));
        st = match prev.filter(|p| !p.session_id.is_empty() && p.session_id != sid) {
            Some(p) => State::inherit(&p, &root, sid, tp),
            // No baseline: it would be taken after this turn's edits.
            None => State {
                session_id: sid.to_string(),
                heads: heads_now(&root),
                closed_tasks: closed_count(&root),
                ..Default::default()
            },
        };
    }
    st.transcript = tp.to_string();
    let Some(b) = build(&root, &st, v["last_assistant_message"].as_str()) else {
        return;
    };
    if st.chain_start.is_empty() {
        st.chain_start = b
            .session
            .turns
            .first()
            .map(|t| t.ts.clone())
            .unwrap_or_default();
    }
    for r in b.env.repos.iter().filter(|r| r.label != ".") {
        if !st.repos.contains(&r.label) {
            st.repos.push(r.label.clone());
        }
    }
    for x in &b.session.bg {
        st.bg
            .retain(|y| !(y.kind == x.kind && y.id == x.id && y.command == x.command));
        st.bg.push(x.clone());
    }
    let skip = st.bg.len().saturating_sub(20);
    st.bg.drain(..skip);
    st.logs.extend(b.session.logs.iter().cloned());
    let mut logs = uniq_recent(st.logs.drain(..));
    logs.truncate(10);
    logs.reverse();
    st.logs = logs;

    // Task boundary: a repo's HEAD moved or a kazam task closed since the
    // last turn. Only repos present both times count, so a repo joining or
    // leaving the set isn't mistaken for a commit.
    let heads = heads_now(&root);
    let moved = heads
        .iter()
        .find(|(l, h)| st.heads.get(*l).is_some_and(|p| p != *h));
    let closed = closed_count(&root);
    let boundary = moved.is_some() || closed > st.closed_tasks;
    let n = b.session.turns.len();
    let nudge = boundary && b.session.context_tokens >= NUDGE_TOKENS && st.nudged_at_turn != n;
    let what = if closed > st.closed_tasks {
        "a kazam task closed"
    } else {
        "a commit landed"
    };
    st.heads = heads;
    st.closed_tasks = closed;
    if nudge {
        st.nudged_at_turn = n;
    }
    write_state(&dir, key.as_deref(), &st);
    if nudge {
        log(
            &root,
            &format!("nudge\t{sid}\tctx={}\tturn={n}", b.session.context_tokens),
        );
        println!(
            "{}",
            serde_json::json!({
                "systemMessage": format!(
                    "kazam: context is ~{}k and {what}. /clear is safe: the next session reloads this one's handoff (~{}k core, ~{}k full on demand).",
                    b.session.context_tokens / 1000,
                    est_tokens(&b.core).div_ceil(1000),
                    est_tokens(&b.full) / 1000
                )
            })
        );
    }
}

/// SessionStart hook. On /clear, rebuild and print the handoff of the
/// session being cleared (stdout becomes context; the core points at the
/// full snapshot) and start the new session's state from it. On startup,
/// record the baseline and print a one-line resume notice per recent session.
pub fn load_hook() {
    if disabled() {
        return;
    }
    let v = read_stdin();
    let Some(root) = project_root(&cwd_of(&v)) else {
        return;
    };
    let dir = session_dir(&root);
    ensure_dir(&dir);
    let key = key();
    let new_sid = v["session_id"].as_str().unwrap_or_default().to_string();
    let tp = v["transcript_path"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    match v["source"].as_str() {
        // A fresh session gets a one-line-per-session teaser; the agent pulls
        // the handoff with `kazam load` only if the work continues.
        Some("startup") => {
            if !new_sid.is_empty() && read_state(&dir, &sid_key(&new_sid)).session_id != new_sid {
                write_state(&dir, key.as_deref(), &State::fresh(&root, &new_sid, &tp));
            }
            let _ = super::save::load(&root, None, false, true, false);
            return;
        }
        // Resuming an old session: point this process back at it.
        Some("resume") => {
            if !new_sid.is_empty() {
                let mut st = read_state(&dir, &sid_key(&new_sid));
                if st.session_id != new_sid {
                    st = State::fresh(&root, &new_sid, &tp);
                }
                write_state(&dir, key.as_deref(), &st);
            }
            return;
        }
        Some("clear") => {}
        _ => return,
    }

    // The process pointer still names the session being cleared from: the
    // new session hasn't run a turn yet.
    let prev = key
        .as_deref()
        .map(|k| read_state(&dir, k))
        .filter(|p| !p.session_id.is_empty() && p.session_id != new_sid);
    let mut built: Option<Built> = None;
    let (text, how, from, inherited) = match &prev {
        Some(p) => {
            built = build(&root, p, None);
            let text = built.as_ref().map(|b| b.core.clone()).or_else(|| {
                fs::read_to_string(dir.join(format!("{}.core.md", sid_key(&p.session_id)))).ok()
            });
            (text, "process", p.session_id.clone(), true)
        }
        None => {
            // No process match (ps failed, or a new process): only trust a
            // recent session, and say where it came from.
            let other = newest_state(&dir, &new_sid, FALLBACK_MAX_AGE_SECS);
            let text = other
                .as_ref()
                .and_then(|o| build(&root, o, None).map(|b| b.core));
            let from = other.map(|o| o.session_id).unwrap_or_default();
            (text, "latest", from, false)
        }
    };
    if !new_sid.is_empty() {
        let st = match (&prev, inherited) {
            (Some(p), true) => {
                let mut st = State::inherit(p, &root, &new_sid, &tp);
                // The state file lags the transcript (and older ones lack
                // chain fields): fill the chain from the session just rebuilt.
                if let Some(b) = &built {
                    if st.chain_start.is_empty() {
                        st.chain_start = b
                            .session
                            .turns
                            .first()
                            .map(|t| t.ts.clone())
                            .unwrap_or_default();
                    }
                    for r in b.env.repos.iter().filter(|r| r.label != ".") {
                        if !st.repos.contains(&r.label) {
                            st.repos.push(r.label.clone());
                        }
                    }
                    for x in b.session.bg.iter().filter(|x| x.status.is_empty()) {
                        if !st
                            .bg
                            .iter()
                            .any(|y| y.kind == x.kind && y.id == x.id && y.command == x.command)
                        {
                            st.bg.push(x.clone());
                        }
                    }
                    st.logs.extend(b.session.logs.iter().cloned());
                    let mut logs = uniq_recent(st.logs.drain(..));
                    logs.truncate(10);
                    logs.reverse();
                    st.logs = logs;
                }
                st
            }
            _ => State::fresh(&root, &new_sid, &tp),
        };
        write_state(&dir, key.as_deref(), &st);
    }
    let Some(text) = text else {
        return;
    };
    if how == "latest" {
        println!("(kazam: no snapshot for this process; reloading the most recent session in this repo. If it isn't yours, ignore it.)\n");
    }
    print!("{text}");
    log(
        &root,
        &format!(
            "load\t{}\t{how}\tfrom={from}\t~{} tokens core",
            if new_sid.is_empty() { "?" } else { &new_sid },
            est_tokens(&text)
        ),
    );
}

/// `kazam ctx handoff show`: the current snapshot, or one turn in full.
pub fn show(project: &Path, session: Option<&str>, turn: Option<usize>) -> anyhow::Result<()> {
    let dir = session_dir(project);
    let key = match session {
        Some(s) => sid_key(s),
        None => key().unwrap_or_default(),
    };
    let st = if key.is_empty() {
        State::default()
    } else {
        read_state(&dir, &key)
    };
    // No match: the newest session in this repo.
    let st = if st.transcript.is_empty() && session.is_none() {
        newest_state(&dir, "", u64::MAX).unwrap_or_default()
    } else {
        st
    };
    if let Some(n) = turn {
        anyhow::ensure!(!st.transcript.is_empty(), "no session recorded yet");
        let text = fs::read_to_string(&st.transcript)?;
        let s = parse_transcript(&text, project);
        let t = s
            .turns
            .iter()
            .find(|t| t.n == n)
            .ok_or_else(|| anyhow::anyhow!("turn {n} not found ({} turns)", s.turns.len()))?;
        println!(
            "## turn {} ({})\n\n### prompt\n{}\n",
            t.n,
            local_ts(&t.ts),
            t.prompt
        );
        for (label, xs) in [
            ("read", &t.reads),
            ("edited", &t.edits),
            ("ran", &t.bash),
            ("subagents", &t.agents),
        ] {
            if !xs.is_empty() {
                println!(
                    "### {label}\n{}\n",
                    xs.iter()
                        .map(|x| format!("- {x}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
            }
        }
        println!("### report\n{}", t.report);
        return Ok(());
    }
    let path = dir.join(format!("{}.md", sid_key(&st.session_id)));
    anyhow::ensure!(
        !st.session_id.is_empty() && path.is_file(),
        "no handoff snapshot yet"
    );
    print!("{}", fs::read_to_string(&path)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(v: serde_json::Value) -> String {
        v.to_string()
    }

    fn transcript() -> String {
        [
            line(serde_json::json!({"type":"user","timestamp":"2026-09-28T10:00:00Z","origin":{"kind":"human"},
                "message":{"content":"fix the deal review render <system-reminder>noise</system-reminder>"}})),
            line(serde_json::json!({"type":"assistant","message":{"usage":{"input_tokens":10,"cache_read_input_tokens":90000,"cache_creation_input_tokens":0},
                "content":[{"type":"tool_use","name":"Read","input":{"file_path":"/r/app/render.py","offset":10,"limit":20}},
                           {"type":"tool_use","name":"Bash","input":{"command":"pytest -q\nsecond line"}}]}})),
            line(serde_json::json!({"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}})),
            line(serde_json::json!({"type":"attachment","attachment":{"type":"queued_command","prompt":"keep it small","origin":{"kind":"human"}}})),
            line(serde_json::json!({"type":"assistant","message":{"usage":{"input_tokens":5,"cache_read_input_tokens":160000},
                "content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/r/app/render.py"}},
                           {"type":"text","text":"Fixed the render."}]}})),
            line(serde_json::json!({"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>x</task-notification>"}})),
            line(serde_json::json!({"type":"user","message":{"content":"<local-command-stdout>Compacted</local-command-stdout>"}})),
            line(serde_json::json!({"type":"user","isCompactSummary":true,"message":{"content":"Summary of before."}})),
            line(serde_json::json!({"type":"user","timestamp":"2026-09-28T10:05:00Z","message":{"content":"<command-name>/review</command-name><command-args>the diff</command-args>"}})),
        ]
        .join("\n")
    }

    #[test]
    fn parses_turns_and_skips_harness_messages() {
        let s = parse_transcript(&transcript(), Path::new("/r"));
        assert_eq!(s.turns.len(), 2);
        assert_eq!(
            s.turns[0].prompt,
            "fix the deal review render\n\n[sent mid-turn] keep it small"
        );
        assert_eq!(s.turns[0].reads, vec!["app/render.py:10-30"]);
        assert_eq!(s.turns[0].edits, vec!["app/render.py"]);
        assert_eq!(s.turns[0].bash, vec!["pytest -q"]);
        assert_eq!(s.turns[0].report, "Fixed the render.");
        assert_eq!(s.turns[1].prompt, "/review the diff");
        assert_eq!(s.compact_summary.as_deref(), Some("Summary of before."));
        assert_eq!(s.context_tokens, 160005);
    }

    #[test]
    fn render_puts_latest_first_and_holds_budget() {
        let s = parse_transcript(&transcript(), Path::new("/r"));
        let env = Surroundings {
            repos: vec![RepoState {
                label: ".".into(),
                diff: "+x\n".repeat(50_000),
                ..Default::default()
            }],
            corrections: vec!["- app/render.py: did X -> do Y".into()],
            ..Default::default()
        };
        let out = render(&s, &env, "sid", 3000);
        let stand = out.find("### Where things stand").unwrap();
        let so_far = out.find("### Session so far").unwrap();
        assert!(stand < so_far);
        assert!(out.contains("Last request (turn 2):\n/review the diff"));
        assert!(out.contains("Standing corrections"));
        assert!(out.len() <= 3000 * 4 + 64, "len {}", out.len());
        assert!(
            out.contains("diff truncated"),
            "the truncation note must survive the cap"
        );
        assert!(!out.contains("handoff truncated"));
    }

    #[test]
    fn core_fits_hook_limit_and_points_at_full() {
        let mut s = parse_transcript(&transcript(), Path::new("/r"));
        s.turns.last_mut().unwrap().report = "long report line\n".repeat(5_000);
        let env = Surroundings {
            repos: vec![RepoState {
                label: "kazam".into(),
                branch: "research-cochange".into(),
                commits: "1dcde8f feat: handoff\n".into(),
                status: " M src/ctx/handoff.rs\n".into(),
                other_dirty: 3,
                ..Default::default()
            }],
            tasks: vec!["- kz-1 [p1, open] dogfood".into()],
            corrections: vec![
                "- app/render.py: did X -> do Y".into(),
                "- other/x.rs: unrelated -> skip".into(),
            ],
            core_corrections: vec!["- app/render.py: did X -> do Y".into()],
            ..Default::default()
        };
        let core = render_core(&s, &env, "sid-1", Path::new("/r/.kazam/session/sid-1.md"));
        assert!(core.len() <= CORE_MAX_BYTES, "len {}", core.len());
        assert!(core.contains("`/r/.kazam/session/sid-1.md`"));
        assert!(core.contains("show --session sid-1 --turn N"));
        assert!(core.contains("Last request (turn 2):\n/review the diff"));
        assert!(core.contains("rest of this report in the full snapshot"));
        assert!(core.contains("**kazam** on `research-cochange`"));
        assert!(core.contains("(+3 other uncommitted files: not edited in the main thread"));
        assert!(core.contains("kz-1 [p1, open] dogfood"));
        assert!(core.contains("did X -> do Y"));
        assert!(!core.contains("unrelated"));
    }

    #[test]
    fn touched_paths_covers_reads_and_bash_not_just_edits() {
        let s = Session {
            turns: vec![Turn {
                reads: vec!["kazam/src/ctx/handoff.rs:1-240".into()],
                bash: vec![
                    "cd /r/kazam && cargo test".into(),
                    "sed -i '' 's/a/b/' \"/r/kazam/src/workspace.rs\"".into(),
                    "ls /elsewhere/x".into(),
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let t = touched_paths(Path::new("/r"), &s);
        assert!(t.contains(&"kazam/src/ctx/handoff.rs".to_string()), "{t:?}");
        assert!(t.contains(&"kazam".to_string()), "{t:?}");
        assert!(t.contains(&"kazam/src/workspace.rs".to_string()), "{t:?}");
        assert!(!t.iter().any(|p| p.contains("elsewhere")), "{t:?}");
    }

    #[test]
    fn sid_key_is_filename_safe() {
        assert_eq!(sid_key("a2a196c6-1944/../x"), "a2a196c6-1944x");
    }

    #[test]
    fn parse_ts_handles_both_task_formats() {
        let a = parse_ts("2026-09-28T19:00:00.000Z").unwrap();
        let b = parse_ts("2026-09-28 14:09:37.970445-05:00").unwrap();
        assert!(b > a);
    }

    #[test]
    fn parses_background_work_and_skips_harness_turns() {
        let t = [
            line(serde_json::json!({"type":"user","timestamp":"2026-09-28T10:00:00Z","message":{"content":"<command-name>/clear</command-name><command-args></command-args>"}})),
            line(serde_json::json!({"type":"user","timestamp":"2026-09-28T10:00:05Z","origin":{"kind":"human"},"message":{"content":"start the backfill"}})),
            line(serde_json::json!({"type":"assistant","timestamp":"2026-09-28T10:00:06Z","message":{"content":[
                {"type":"tool_use","id":"t1","name":"Bash","input":{"command":"kazam open doc.md","description":"Open doc","run_in_background":true}},
                {"type":"tool_use","id":"t2","name":"Monitor","input":{"command":"tail -F ~/.kazam/refresh-all.log","description":"watch backfill","timeout_ms":1800000}},
                {"type":"tool_use","id":"t3","name":"Bash","input":{"command":"cd ~ && BATCH=100 nohup ~/.kazam/bin/kazam-refresh-all.sh >/dev/null 2>&1 &"}}]}})),
            line(serde_json::json!({"type":"user","toolUseResult":{"backgroundTaskId":"b1"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Command running in background with ID: b1. Output is being written to: /tmp/x/b1.output. You will be notified."}]}})),
            line(serde_json::json!({"type":"user","toolUseResult":{"taskId":"m1"},"message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"Monitor started (task m1)"}]}})),
            line(serde_json::json!({"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>\n<task-id>b1</task-id>\n<status>completed</status>\n</task-notification>"}})),
            line(serde_json::json!({"type":"user","origin":{"kind":"task-notification"},"message":{"content":"<task-notification>\n<task-id>m1</task-id>\n<event>model server down</event>\n</task-notification>"}})),
            line(serde_json::json!({"type":"user","message":{"content":"<bash-input>gcloud auth login</bash-input>"}})),
            line(serde_json::json!({"type":"user","message":{"content":"<bash-stdout>ok</bash-stdout><bash-stderr></bash-stderr>"}})),
            line(serde_json::json!({"type":"user","message":{"content":[{"type":"text","text":"[Request interrupted by user]"}]}})),
        ]
        .join("\n");
        let s = parse_transcript(&t, Path::new("/r"));
        assert_eq!(s.turns.len(), 1, "{:?}", s.turns);
        assert_eq!(
            s.turns[0].prompt,
            "start the backfill\n\n[user ran `gcloud auth login`]"
        );
        assert!(s.turns[0].interrupted);
        assert_eq!(s.bg.len(), 3);
        assert_eq!((s.bg[0].kind.as_str(), s.bg[0].id.as_str()), ("bash", "b1"));
        assert_eq!(s.bg[0].output, "/tmp/x/b1.output");
        assert_eq!(s.bg[0].status, "completed");
        assert_eq!(
            (s.bg[1].kind.as_str(), s.bg[1].id.as_str()),
            ("monitor", "m1")
        );
        assert_eq!(s.bg[1].last_event, "model server down");
        assert_eq!(s.bg[1].timeout_ms, 1_800_000);
        assert_eq!(s.bg[2].kind, "detached");
        assert_eq!(
            detached_key(&s.bg[2].command).as_deref(),
            Some("kazam-refresh-all.sh")
        );
        assert_eq!(s.logs, vec!["~/.kazam/refresh-all.log"]);
    }

    #[test]
    fn detached_detection() {
        assert!(is_detached("python3 -m http.server 8000 &"));
        assert!(is_detached("nohup ./run.sh > out.log 2>&1"));
        assert!(!is_detached("cargo build && cargo test"));
        assert!(!is_detached("cmd 2>&1 | tail"));
        assert!(!is_detached(
            "python3 - <<'EOF'\nx = a & b\nrun() &\nEOF\necho done"
        ));
        assert!(is_detached("cat <<EOF > f\nbody\nEOF\n./serve.sh &"));
        assert_eq!(
            detached_key("python3 -m http.server 8000 &").as_deref(),
            Some("http.server")
        );
    }

    #[test]
    fn bg_lines_report_status() {
        let bg = vec![
            Bg {
                kind: "bash".into(),
                what: "Open doc".into(),
                status: "completed".into(),
                output: "/nope".into(),
                ..Default::default()
            },
            Bg {
                kind: "monitor".into(),
                what: "watch".into(),
                started: "2020-01-01T00:00:00Z".into(),
                timeout_ms: 1000,
                command: "tail -F x.log".into(),
                ..Default::default()
            },
            Bg {
                kind: "detached".into(),
                what: "backfill".into(),
                command: "nohup kazam-no-such-process-xyz.sh &".into(),
                ..Default::default()
            },
        ];
        let l = bg_lines(&bg).join("\n");
        assert!(l.contains("[bash] Open doc: completed"), "{l}");
        assert!(l.contains("[monitor] watch: expired; re-arm"), "{l}");
        assert!(l.contains("command: `tail -F x.log`"), "{l}");
        assert!(l.contains("[detached] backfill: not running (no process matches `kazam-no-such-process-xyz.sh`)"), "{l}");
    }

    #[test]
    fn core_flags_open_question_background_and_snapshot_only_items() {
        let mut s = parse_transcript(&transcript(), Path::new("/r"));
        s.turns[0].report =
            "Brief is at http://localhost:3002 and the log is ~/.kazam/refresh-all.log".into();
        s.turns.last_mut().unwrap().report = "Done.\n\nShould I log that as a miss?".into();
        let env = Surroundings {
            bg: vec!["- [detached] backfill: running".into()],
            ..Default::default()
        };
        let core = render_core(&s, &env, "sid-1", Path::new("/r/.kazam/session/sid-1.md"));
        assert!(core.contains("**Waiting on the user:**"), "{core}");
        assert!(core.contains("> Should I log that as a miss?"));
        assert!(core.contains("### Background work"));
        assert!(core.contains("- [detached] backfill: running"));
        assert!(core.contains("### Only in the full snapshot"));
        assert!(core.contains("turn 1: http://localhost:3002"));
        assert!(core.contains("turn 1: ~/.kazam/refresh-all.log"));
        assert_eq!(open_question("All done."), None);
        assert_eq!(
            open_question("**Go with #1-#4?**").as_deref(),
            Some("Go with #1-#4?")
        );
    }

    #[test]
    fn core_marks_a_truncated_tail() {
        let s = parse_transcript(&transcript(), Path::new("/r"));
        let env = Surroundings {
            decisions: (0..200)
                .map(|i| format!("- decision {i} with some words"))
                .collect(),
            ..Default::default()
        };
        let core = render_core(&s, &env, "sid-1", Path::new("/f.md"));
        assert!(core.len() <= CORE_MAX_BYTES);
        assert!(core.contains("[…more in the full snapshot]"), "{core}");
    }

    #[test]
    fn inherit_carries_the_chain_and_unfinished_work_only() {
        let root = std::env::temp_dir().join(format!("kazam-inherit-{}", std::process::id()));
        let _ = fs::create_dir_all(&root);
        let prev = State {
            session_id: "a".into(),
            chain: vec!["z".into()],
            chain_start: "2026-09-28T10:00:00Z".into(),
            repos: vec!["kazam".into()],
            baseline: [("kazam".to_string(), vec!["x.rs".to_string()])].into(),
            bg: vec![
                Bg {
                    kind: "bash".into(),
                    id: "b1".into(),
                    status: "completed".into(),
                    ..Default::default()
                },
                Bg {
                    kind: "detached".into(),
                    command: "nohup x.sh &".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let st = State::inherit(&prev, &root, "b", "/t.jsonl");
        assert_eq!(st.session_id, "b");
        assert_eq!(st.chain, vec!["z", "a"]);
        assert_eq!(st.chain_start, "2026-09-28T10:00:00Z");
        assert_eq!(st.repos, vec!["kazam"]);
        assert_eq!(st.baseline["kazam"], vec!["x.rs"]);
        assert_eq!(st.bg.len(), 1);
        assert_eq!(st.bg[0].kind, "detached");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn repo_state_counts_subagent_edits_as_the_sessions_own() {
        let top = std::env::temp_dir().join(format!("kazam-repostate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&top);
        fs::create_dir_all(&top).unwrap();
        let g = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&top)
                .args(args)
                .output()
                .unwrap();
        };
        g(&["init", "-q"]);
        g(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "init",
        ]);
        fs::write(top.join("before.rs"), "dirty before").unwrap();
        let baseline = vec!["before.rs".to_string()];
        // After the session began: one main-thread edit, one by a subagent.
        fs::write(top.join("main.rs"), "edited").unwrap();
        fs::write(top.join("sub.rs"), "subagent").unwrap();
        let r = repo_state(
            &top,
            "x".into(),
            &["main.rs".to_string()],
            Some(&baseline),
            "",
        );
        assert!(
            r.status.contains("main.rs") && r.status.contains("sub.rs"),
            "{}",
            r.status
        );
        assert!(!r.status.contains("before.rs"));
        assert_eq!(r.other_dirty, 1);
        assert!(r.baseline_known);
        assert_eq!(r.upstream, "no upstream");
        // Without a baseline only main-thread edits are claimed, and the rest
        // is labelled as possibly ours.
        let r = repo_state(&top, "x".into(), &["main.rs".to_string()], None, "");
        assert_eq!(r.other_dirty, 2);
        assert!(!r.baseline_known);
        let _ = fs::remove_dir_all(&top);
    }
}

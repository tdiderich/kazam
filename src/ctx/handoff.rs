//! Clear handoff: `/clear` as instant compaction.
//!
//! The Stop hook rebuilds a ready-to-inject snapshot of the session after every
//! turn (`handoff stop`), and the SessionStart hook prints it when the session
//! was cleared (`handoff load`). Clearing costs nothing at clear time because
//! the work already happened, and nothing on either path calls a model: the
//! snapshot is the user's prompts verbatim, the agent's own final reports,
//! files touched, commits, the uncommitted diff, and kazam's task and
//! correction stores.
//!
//! Snapshots are keyed by the Claude Code process, not the session id: /clear
//! starts a new session id in the same process, and keying by process keeps
//! two sessions open in one repo from reloading each other's state.
//!
//! Replay A/B (2026-09-28, maze-apps): /clear plus a ~1.1k-token version of
//! this matched full-context quality at 11.7% of the tokens. The budget here
//! is larger on purpose (quality over thrift; the startup prefix is ~75k).

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Approximate token cap for the whole snapshot.
pub const BUDGET_TOKENS: usize = 15_000;
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
}

#[derive(Default, Debug)]
pub struct Session {
    pub turns: Vec<Turn>,
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
    {
        return None;
    }
    let mut s = strip_tag(&raw, "system-reminder");
    // `/cmd args` arrives as tags; keep it readable.
    if s.contains("<command-name>") {
        let name = between(&s, "<command-name>", "</command-name>").unwrap_or_default();
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
    for line in text.lines() {
        let Ok(d) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match d["type"].as_str() {
            Some("user") => {
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
                            match (b["name"].as_str().unwrap_or_default(), fp) {
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

/// Everything outside the transcript: git state and kazam's stores.
#[derive(Default)]
pub struct Surroundings {
    pub commits: String,
    pub status: String,
    pub diff: String,
    pub tasks: Vec<String>,
    pub corrections: Vec<String>,
    pub learnings: Vec<String>,
}

fn surroundings(root: &Path, s: &Session) -> Surroundings {
    let since = s.turns.first().map(|t| t.ts.clone()).unwrap_or_default();
    let commits = if since.is_empty() {
        String::new()
    } else {
        git(
            root,
            &[
                "log",
                "--oneline",
                "--no-decorate",
                &format!("--since={since}"),
                "-25",
            ],
        )
    };
    let status = git(root, &["status", "--short", "--untracked-files=normal"]);
    // Diffs only for files this session edited: the rest of a dirty tree
    // isn't this session's work.
    let edited = uniq_recent(s.turns.iter().flat_map(|t| t.edits.clone()));
    let mut diff = String::new();
    if !edited.is_empty() {
        let mut args = vec!["diff", "HEAD", "--"];
        args.extend(edited.iter().map(String::as_str));
        diff = git(root, &args);
    }
    let edited_set: std::collections::HashSet<&str> = edited.iter().map(String::as_str).collect();

    let mut tasks = vec![];
    if let Ok(store) = crate::track::store::read_tasks(root) {
        for t in store
            .tasks
            .iter()
            .filter(|t| t.status == crate::track::types::TaskStatus::Active)
        {
            let note = t
                .note
                .as_deref()
                .map(|n| format!("\n  note: {}", one_line(n, 400)))
                .unwrap_or_default();
            tasks.push(format!(
                "- {} [p{}] {}{note}",
                t.id,
                t.priority,
                one_line(&t.title, 200)
            ));
        }
    }

    let mut corrections = vec![];
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
        let (on_files, rest): (Vec<_>, Vec<_>) = store.corrections.iter().partition(|c| {
            c.file_path.as_deref().is_some_and(|f| {
                edited_set.contains(f) || edited_set.iter().any(|e| e.starts_with(f))
            })
        });
        corrections.extend(on_files.iter().rev().take(8).map(|c| fmt(c)));
        corrections.extend(rest.iter().rev().take(5).map(|c| fmt(c)));
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
    Surroundings {
        commits,
        status,
        diff,
        tasks,
        corrections,
        learnings,
    }
}

/// Assemble the snapshot, highest-value sections first, each under its own cap,
/// then the diff takes whatever budget is left.
pub fn render(s: &Session, env: &Surroundings, session_id: &str, budget_tokens: usize) -> String {
    let mut out = String::new();
    let last = s.turns.last();
    let _ = writeln!(out, "## kazam session handoff (reloaded after /clear)\n");
    let _ = writeln!(
        out,
        "This is the session you were in before /clear, rebuilt by kazam from its transcript, git, and .kazam/. \
         The user's words are verbatim; reports are your own final messages. Treat it as your memory of the \
         session and carry on. Full detail of any turn: `kazam ctx handoff show --turn N`.\n"
    );
    let _ = writeln!(
        out,
        "Session {} · {} turns · last turn {} · context was ~{}k tokens\n",
        session_id,
        s.turns.len(),
        last.map(|t| t.ts.as_str()).unwrap_or("?"),
        s.context_tokens / 1000
    );

    if let Some(t) = last {
        let _ = writeln!(out, "### Where things stand\n");
        let _ = writeln!(
            out,
            "Last request (turn {}):\n{}\n",
            t.n,
            clip(&t.prompt, 3000)
        );
        if !t.report.is_empty() {
            let _ = writeln!(out, "What you reported:\n{}\n", clip(&t.report, 6000));
        }
    }

    let _ = writeln!(out, "### Session so far\n");
    if let Some(sum) = &s.compact_summary {
        let _ = writeln!(
            out,
            "Before the last compaction (its summary):\n{}\n",
            clip(sum, 8000)
        );
    }
    // The summary already covers turns before the compaction.
    let n = s.turns.len();
    for t in s.turns.iter().filter(|t| t.n > s.compacted_through) {
        let recent = n - t.n < 8;
        let prompt = if recent {
            clip(&t.prompt, 1200)
        } else {
            one_line(&t.prompt, 240)
        };
        let _ = write!(out, "- turn {}: {}", t.n, prompt.replace('\n', "\n  "));
        let edits = uniq_recent(t.edits.iter().cloned());
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
        .take(4)
        .collect();
    if !earlier.is_empty() {
        let _ = writeln!(out, "### Earlier reports (newest first)\n");
        for t in earlier {
            let _ = writeln!(out, "Turn {}:\n{}\n", t.n, clip(&t.report, 1800));
        }
    }

    let edited = uniq_recent(s.turns.iter().flat_map(|t| t.edits.clone()));
    let read = uniq_recent(s.turns.iter().flat_map(|t| t.reads.clone()));
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
    if !env.commits.trim().is_empty() {
        let _ = writeln!(
            out,
            "### Commits since the session started\n{}",
            env.commits.trim_end()
        );
        let _ = writeln!(out);
    }
    if !env.status.trim().is_empty() {
        let lines: Vec<&str> = env.status.lines().take(40).collect();
        let _ = writeln!(out, "### Working tree\n{}\n", lines.join("\n"));
    }

    // The diff gets whatever budget is left, and the whole thing is held to the
    // cap even if the transcript sections ran long.
    let left = budget_tokens
        .saturating_sub(est_tokens(&out))
        .saturating_mul(4);
    if !env.diff.trim().is_empty() && left > 800 {
        let d = if env.diff.len() > left {
            let mut cut = left;
            while !env.diff.is_char_boundary(cut) {
                cut -= 1;
            }
            format!(
                "{}\n[diff truncated; run git diff HEAD for the rest]",
                &env.diff[..cut]
            )
        } else {
            env.diff.clone()
        };
        let _ = writeln!(
            out,
            "### Uncommitted changes to files this session edited\n```diff\n{}\n```",
            d.trim_end()
        );
    }
    let cap = budget_tokens * 4;
    if out.len() > cap {
        let mut cut = cap;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out.push_str("\n[handoff truncated at budget]\n");
    }
    out
}

// ── Storage ──────────────────────────────────────

fn session_dir(root: &Path) -> PathBuf {
    crate::workspace::root(root).join("session")
}

/// The Claude Code process this hook runs under. Survives /clear, which only
/// changes the session id.
fn claude_pid() -> Option<u32> {
    let mut pid = std::process::id();
    for _ in 0..8 {
        let out = Command::new("ps")
            .args(["-o", "ppid=,comm=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let (ppid, comm) = line.split_once(char::is_whitespace)?;
        let comm = comm.trim();
        if pid != std::process::id() && Path::new(comm).file_name().is_some_and(|n| n == "claude") {
            return Some(pid);
        }
        pid = ppid.trim().parse().ok()?;
        if pid <= 1 {
            return None;
        }
    }
    None
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    session_id: String,
    transcript: String,
    head: String,
    closed_tasks: usize,
    nudged_at_turn: usize,
}

fn key() -> String {
    claude_pid()
        .map(|p| format!("pid-{p}"))
        .unwrap_or_else(|| "nopid".into())
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

/// Stop hook: rebuild this process's snapshot, and nudge toward /clear when
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
    let Ok(text) = fs::read_to_string(tp) else {
        return;
    };
    let mut session = parse_transcript(&text, &root);
    let Some(last) = session.turns.last_mut() else {
        return;
    };
    // The transcript can lag the turn that just ended; the payload's copy of
    // the final message is authoritative.
    if let Some(m) = v["last_assistant_message"]
        .as_str()
        .filter(|m| !m.trim().is_empty())
    {
        last.report = m.trim().to_string();
    }
    let env = surroundings(&root, &session);
    let snapshot = render(&session, &env, sid, BUDGET_TOKENS);

    let dir = session_dir(&root);
    ensure_dir(&dir);
    let key = key();
    let _ = fs::write(dir.join(format!("{key}.md")), &snapshot);
    let _ = fs::write(dir.join("latest.md"), &snapshot);

    // Task boundary: a commit landed or a kazam task closed since the last turn.
    let prev = read_state(&dir, &key);
    let head = git(&root, &["rev-parse", "HEAD"]).trim().to_string();
    let closed = crate::track::store::read_tasks(&root)
        .map(|s| {
            s.tasks
                .iter()
                .filter(|t| t.status == crate::track::types::TaskStatus::Closed)
                .count()
        })
        .unwrap_or(0);
    let same_session = prev.session_id == sid;
    let boundary = same_session && (head != prev.head || closed > prev.closed_tasks);
    let n = session.turns.len();
    let nudge = boundary && session.context_tokens >= NUDGE_TOKENS && prev.nudged_at_turn != n;
    let state = State {
        session_id: sid.to_string(),
        transcript: tp.to_string(),
        head,
        closed_tasks: closed,
        nudged_at_turn: if nudge {
            n
        } else if same_session {
            prev.nudged_at_turn
        } else {
            0
        },
    };
    if let Ok(j) = serde_json::to_string(&state) {
        let _ = fs::write(dir.join(format!("{key}.json")), j);
    }
    if nudge {
        let what = if closed > prev.closed_tasks {
            "a kazam task closed"
        } else {
            "a commit landed"
        };
        log(
            &root,
            &format!("nudge\t{sid}\tctx={}\tturn={n}", session.context_tokens),
        );
        println!(
            "{}",
            serde_json::json!({
                "systemMessage": format!(
                    "kazam: context is ~{}k and {what}. /clear is safe: the next session reloads this one's handoff (~{}k tokens).",
                    session.context_tokens / 1000,
                    est_tokens(&snapshot) / 1000
                )
            })
        );
    }
}

/// SessionStart hook: on /clear, print this process's snapshot. Stdout becomes
/// context. Silent for every other source.
pub fn load_hook() {
    if disabled() {
        return;
    }
    let v = read_stdin();
    if v["source"].as_str() != Some("clear") {
        return;
    }
    let Some(root) = project_root(&cwd_of(&v)) else {
        return;
    };
    let dir = session_dir(&root);
    let key = key();
    let keyed = dir.join(format!("{key}.md"));
    let (path, how) = if keyed.is_file() {
        (keyed, "process")
    } else {
        // No process match (ps failed or a new process): only trust a recent
        // latest.md, and say where it came from.
        let latest = dir.join("latest.md");
        let fresh = fs::metadata(&latest)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() < FALLBACK_MAX_AGE_SECS);
        if !fresh {
            return;
        }
        (latest, "latest")
    };
    let Ok(snapshot) = fs::read_to_string(&path) else {
        return;
    };
    if how == "latest" {
        println!("(kazam: no snapshot for this process; reloading the most recent session in this repo. If it isn't yours, ignore it.)\n");
    }
    print!("{snapshot}");
    log(
        &root,
        &format!(
            "load\t{}\t{how}\t~{} tokens",
            v["session_id"].as_str().unwrap_or("?"),
            est_tokens(&snapshot)
        ),
    );
}

/// `kazam ctx handoff show`: the current snapshot, or one turn in full.
pub fn show(project: &Path, turn: Option<usize>) -> anyhow::Result<()> {
    let dir = session_dir(project);
    let key = key();
    if let Some(n) = turn {
        let st = read_state(&dir, &key);
        let st = if st.transcript.is_empty() {
            // Fall back to whichever state file was written last.
            let newest = fs::read_dir(&dir)?
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
            newest
                .and_then(|e| fs::read_to_string(e.path()).ok())
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            st
        };
        anyhow::ensure!(!st.transcript.is_empty(), "no session recorded yet");
        let text = fs::read_to_string(&st.transcript)?;
        let s = parse_transcript(&text, project);
        let t = s
            .turns
            .iter()
            .find(|t| t.n == n)
            .ok_or_else(|| anyhow::anyhow!("turn {n} not found ({} turns)", s.turns.len()))?;
        println!("## turn {} ({})\n\n### prompt\n{}\n", t.n, t.ts, t.prompt);
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
    let keyed = dir.join(format!("{key}.md"));
    let path = if keyed.is_file() {
        keyed
    } else {
        dir.join("latest.md")
    };
    print!(
        "{}",
        fs::read_to_string(&path).map_err(|_| anyhow::anyhow!("no handoff snapshot yet"))?
    );
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
            diff: "+x\n".repeat(50_000),
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
        assert!(out.contains("diff truncated") || out.contains("handoff truncated"));
    }
}

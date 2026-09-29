//! `kazam save` / `kazam load`: the agent-agnostic side of the clear handoff.
//!
//! The Stop hook builds snapshots from Claude Code transcripts, which no other
//! agent produces. `save` lets any agent with a shell write what a transcript
//! can't carry reliably (intent, next step, decisions) into
//! `.kazam/ctx/saves.jsonl`, and `load` prints the newest session's context
//! for whichever agent opens next. `load --brief` is the session-start form:
//! one line per recent session, so an unrelated new session pays ~100 tokens,
//! not the whole handoff.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Sessions older than this don't show in `load --brief`.
const BRIEF_MAX_AGE_SECS: u64 = 24 * 3600;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Save {
    pub ts: String,
    pub agent: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub next: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<String>,
}

pub fn saves_path(project: &Path) -> PathBuf {
    crate::workspace::root(project).join("ctx/saves.jsonl")
}

pub fn read_saves(project: &Path) -> Vec<Save> {
    fs::read_to_string(saves_path(project))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Best guess at which agent is running this, from its environment.
fn detect_agent() -> String {
    let env = |k: &str| std::env::var_os(k).is_some();
    if env("CLAUDECODE") {
        "claude".into()
    } else if env("CURSOR_TRACE_ID") || env("CURSOR_AGENT") {
        "cursor".into()
    } else if env("CODEX_SANDBOX") || env("CODEX_HOME") {
        "codex".into()
    } else if env("GEMINI_CLI") {
        "gemini".into()
    } else {
        "agent".into()
    }
}

pub fn save(
    project: &Path,
    note: Option<String>,
    next: Option<String>,
    decisions: Vec<String>,
    agent: Option<String>,
    session: Option<String>,
) -> Result<()> {
    let note = note.unwrap_or_default();
    let next = next.unwrap_or_default();
    if note.trim().is_empty() && next.trim().is_empty() && decisions.is_empty() {
        bail!("nothing to save: pass a note, --next, or --decision");
    }
    let s = Save {
        ts: chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        agent: agent.unwrap_or_else(detect_agent),
        session: session
            .or_else(|| super::handoff::current_session(project))
            .unwrap_or_default(),
        note: note.trim().to_string(),
        next: next.trim().to_string(),
        decisions: decisions
            .into_iter()
            .map(|d| d.trim().to_string())
            .collect(),
    };
    let p = saves_path(project);
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
    use std::io::Write;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&p)?;
    writeln!(f, "{}", serde_json::to_string(&s)?)?;
    println!(
        "  ✓ saved ({}{}{} decision{})",
        s.agent,
        if s.session.is_empty() {
            ""
        } else {
            ", session "
        },
        short(&s.session),
        if s.decisions.len() == 1 {
            ": 1".to_string()
        } else {
            format!("s: {}", s.decisions.len())
        }
    );
    Ok(())
}

fn short(sid: &str) -> &str {
    &sid[..sid.len().min(8)]
}

pub fn fmt_save(s: &Save) -> String {
    let mut out = format!("- {} [{}]", s.ts.get(..16).unwrap_or(&s.ts), s.agent);
    if !s.note.is_empty() {
        let _ = write!(out, " {}", s.note);
    }
    if !s.next.is_empty() {
        let _ = write!(out, "\n  next: {}", s.next);
    }
    for d in &s.decisions {
        let _ = write!(out, "\n  decided: {d}");
    }
    out
}

/// Recent decisions across all sessions, newest first.
pub fn recent_decisions(saves: &[Save], n: usize) -> Vec<String> {
    saves
        .iter()
        .rev()
        .flat_map(|s| {
            s.decisions
                .iter()
                .rev()
                .map(move |d| format!("- {} ({})", d, s.ts.get(..10).unwrap_or(&s.ts)))
        })
        .take(n)
        .collect()
}

struct Snap {
    sid: String,
    core: PathBuf,
    full: PathBuf,
    age_secs: u64,
    last_request: String,
}

fn snapshots(project: &Path) -> Vec<Snap> {
    let dir = crate::workspace::root(project).join("session");
    let Ok(rd) = fs::read_dir(&dir) else {
        return vec![];
    };
    let mut out: Vec<Snap> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let sid = name.strip_suffix(".core.md")?.to_string();
            if sid == "latest" || sid.starts_with("pid-") {
                return None;
            }
            let age_secs = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs())
                .unwrap_or(u64::MAX);
            let core = e.path();
            let text = fs::read_to_string(&core).unwrap_or_default();
            let last_request = text
                .split_once("Last request (turn ")
                .and_then(|(_, rest)| rest.split_once('\n'))
                .and_then(|(_, rest)| rest.lines().next())
                .unwrap_or("")
                .to_string();
            Some(Snap {
                full: dir.join(format!("{sid}.md")),
                sid,
                core,
                age_secs,
                last_request,
            })
        })
        .collect();
    out.sort_by_key(|s| s.age_secs);
    out
}

fn age(secs: u64) -> String {
    match secs {
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

pub fn load(
    project: &Path,
    target: Option<&str>,
    list: bool,
    brief: bool,
    full: bool,
) -> Result<()> {
    let snaps = snapshots(project);
    let saves = read_saves(project);

    if list {
        if snaps.is_empty() && saves.is_empty() {
            println!("no sessions saved in this repo yet");
        }
        for s in &snaps {
            let n = saves.iter().filter(|v| v.session == s.sid).count();
            println!(
                "{}  {:>8}  {}{}",
                s.sid,
                age(s.age_secs),
                clip(&s.last_request, 80),
                if n > 0 {
                    format!("  ({n} saves)")
                } else {
                    String::new()
                }
            );
        }
        let orphan: Vec<&Save> = saves
            .iter()
            .rev()
            .filter(|v| v.session.is_empty() || !snaps.iter().any(|s| s.sid == v.session))
            .take(10)
            .collect();
        for v in orphan {
            println!(
                "{}  [{}]  {}",
                v.ts.get(..16).unwrap_or(&v.ts),
                v.agent,
                clip(&v.note, 80)
            );
        }
        return Ok(());
    }

    if brief {
        // Session-start form: a teaser, not the handoff.
        let recent: Vec<&Snap> = snaps
            .iter()
            .filter(|s| s.age_secs < BRIEF_MAX_AGE_SECS)
            .take(3)
            .collect();
        let last_save = saves.last();
        if recent.is_empty() && last_save.is_none() {
            return Ok(());
        }
        println!("kazam: earlier work in this repo can be resumed. If the user's first message continues it, run `kazam load` (or `kazam load <id>`) before acting; otherwise ignore this.");
        for s in recent {
            println!(
                "- {} ({}): {}",
                short(&s.sid),
                age(s.age_secs),
                clip(&s.last_request, 140)
            );
        }
        if let Some(v) = last_save {
            let what = if v.next.is_empty() { &v.note } else { &v.next };
            println!(
                "- last save [{}] {}: {}",
                v.agent,
                v.ts.get(..16).unwrap_or(&v.ts),
                clip(what, 140)
            );
        }
        return Ok(());
    }

    let snap = match target {
        Some(t) => {
            let hit: Vec<&Snap> = snaps.iter().filter(|s| s.sid.starts_with(t)).collect();
            match hit.as_slice() {
                [one] => Some(*one),
                [] => bail!("no session matching '{t}' (see `kazam load --list`)"),
                _ => bail!("'{t}' matches {} sessions; use more of the id", hit.len()),
            }
        }
        None => snaps.first(),
    };

    // The snapshot core already carries this session's saves and recent
    // decisions as of its last Stop; only print what it doesn't have.
    // Rebuilt now when the transcript is still there, so git state and
    // background work are current, not as of the session's last turn.
    let snap_text = snap
        .map(|s| match super::handoff::rebuild(project, &s.sid) {
            Some((core, whole)) => {
                if full {
                    whole
                } else {
                    core
                }
            }
            None => fs::read_to_string(if full { &s.full } else { &s.core }).unwrap_or_default(),
        })
        .unwrap_or_default();
    let unseen = |v: &&Save| !snap_text.contains(&fmt_save(v));
    let session_saves: Vec<&Save> = match snap {
        Some(s) => saves
            .iter()
            .filter(|v| v.session == s.sid)
            .filter(unseen)
            .collect(),
        None => vec![],
    };
    let others: Vec<&Save> = if target.is_none() {
        saves
            .iter()
            .rev()
            .filter(|v| snap.is_none_or(|s| v.session != s.sid))
            .filter(unseen)
            .take(5)
            .collect()
    } else {
        vec![]
    };

    let mut out = String::new();
    if !session_saves.is_empty() || !others.is_empty() {
        let _ = writeln!(out, "## kazam load: saved notes\n");
        for v in session_saves.iter().rev().take(8) {
            let _ = writeln!(out, "{}", fmt_save(v));
        }
        if !others.is_empty() {
            let _ = writeln!(out, "\nFrom other sessions and agents (newest first):");
            for v in others {
                let _ = writeln!(out, "{}", fmt_save(v));
            }
        }
        let _ = writeln!(out);
    }
    // Decisions not already printed above or in the snapshot.
    let decisions: Vec<String> = recent_decisions(&saves, 10)
        .into_iter()
        .filter(|d| {
            let text = d
                .trim_start_matches("- ")
                .rsplit_once(" (")
                .map_or(d.as_str(), |(t, _)| t);
            !out.contains(text) && !snap_text.contains(text)
        })
        .collect();
    if !decisions.is_empty() {
        let _ = writeln!(
            out,
            "### Decisions (newest first)\n{}\n",
            decisions.join("\n")
        );
    }
    if snap.is_none() && out.is_empty() {
        bail!("nothing to load in this repo yet");
    }
    out.push_str(&snap_text);
    print!("{out}");
    super::handoff::log_line(
        project,
        &format!(
            "load\t{}\tcli\t~{} tokens{}",
            snap.map(|s| s.sid.as_str()).unwrap_or("-"),
            out.len() / 4,
            if full { " full" } else { "" }
        ),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_decisions_newest_first() {
        let saves = vec![
            Save {
                ts: "2026-09-27T10:00:00".into(),
                decisions: vec!["a".into()],
                ..Default::default()
            },
            Save {
                ts: "2026-09-28T10:00:00".into(),
                decisions: vec!["b".into(), "c".into()],
                ..Default::default()
            },
        ];
        let d = recent_decisions(&saves, 2);
        assert_eq!(d, vec!["- c (2026-09-28)", "- b (2026-09-28)"]);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("kazam-save-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".kazam/ctx")).unwrap();
        save(
            &dir,
            Some("wired load".into()),
            Some("add AGENTS.md line".into()),
            vec!["load over hooks: every agent has a shell".into()],
            Some("cursor".into()),
            Some("s1".into()),
        )
        .unwrap();
        let saves = read_saves(&dir);
        assert_eq!(saves.len(), 1);
        assert_eq!(saves[0].agent, "cursor");
        assert!(fmt_save(&saves[0]).contains("decided: load over hooks"));
        assert!(save(&dir, None, None, vec![], None, None).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}

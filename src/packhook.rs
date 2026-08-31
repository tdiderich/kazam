//! `kazam pack-hook` - the trusted runner for declarative pack hooks.
//!
//! Packs never ship executable code. They ship declarative hook config (a
//! [`crate::types::PackHook`]) that this runner interprets. The runner reads a
//! stored config, reads the harness hook payload from stdin, and emits a
//! decision: allow, block, or inject text. It has no network or arbitrary
//! filesystem-write capability, so a hostile pack can at worst block the user's
//! own tool calls or inject visible text. That is the safety guarantee.

use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::Path;

use crate::types::{MatchMode, PackHook};

/// Where a pack's hook config lives, relative to the install dir. Used when
/// writing the config at install time (the dir is known and correct).
pub fn config_path(dir: &Path, slug: &str) -> std::path::PathBuf {
    dir.join(".kazam")
        .join("packs")
        .join(format!("{}.hooks.yaml", slug))
}

/// Locate a pack's hook config at hook-run time. The command registered in
/// settings.json carries no `--dir`, so `dir` defaults to the harness cwd,
/// which may be a subdirectory of the repo (or wherever the session started).
/// Walk up from `dir` to find the nearest ancestor holding
/// `.kazam/packs/<slug>.hooks.yaml`, the same way git finds `.git/`. Falls back
/// to the cwd-relative path so a genuine "not installed" error still points at
/// the expected location.
pub fn resolve_config_path(dir: &Path, slug: &str) -> std::path::PathBuf {
    let rel = Path::new(".kazam")
        .join("packs")
        .join(format!("{}.hooks.yaml", slug));
    let start = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    for ancestor in start.ancestors() {
        let candidate = ancestor.join(&rel);
        if candidate.is_file() {
            return candidate;
        }
    }
    config_path(dir, slug)
}

#[derive(Debug, PartialEq)]
enum Decision {
    Allow,
    Block(String),
    Inject(String),
}

/// Expand `\uXXXX` escapes so a pattern can encode characters (like the em
/// dash) that cannot be authored literally through content-guard write hooks.
fn unescape(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'u') {
            chars.next();
            let hex: String = (0..4).filter_map(|_| chars.next()).collect();
            if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                if let Some(ch) = char::from_u32(cp) {
                    out.push(ch);
                    continue;
                }
            }
            out.push('\\');
            out.push('u');
            out.push_str(&hex);
        } else {
            out.push(c);
        }
    }
    out
}

/// Text a block/require primitive scans. With `field`, scan just that
/// `tool_input` field (string value verbatim, non-string serialized) - lets an
/// MCP-tool hook target one arg like a Slack message body. Without `field`,
/// scan the whole serialized `tool_input`, falling back to the whole payload.
/// Mirrors how the existing shell guards grep the full hook input.
fn scan_text(payload: &serde_json::Value, field: Option<&str>) -> String {
    let tool_input = payload.get("tool_input");
    match (tool_input, field) {
        (Some(ti), Some(f)) => ti
            .get(f)
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string())
            })
            .unwrap_or_default(),
        (Some(ti), None) => ti.to_string(),
        (None, _) => payload.to_string(),
    }
}

/// A word char for word-boundary matching: alphanumeric or underscore, the
/// same class `\w` uses.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `pat` occurs in `text` with non-word chars (or string edges) on both
/// sides of at least one occurrence. Case-sensitive, matching substring mode.
fn word_match(text: &str, pat: &str) -> bool {
    if pat.is_empty() {
        return false;
    }
    let mut start = 0;
    while let Some(pos) = text[start..].find(pat) {
        let abs = start + pos;
        let end = abs + pat.len();
        let before_ok = text[..abs]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        let after_ok = text[end..].chars().next().is_none_or(|c| !is_word_char(c));
        if before_ok && after_ok {
            return true;
        }
        // `end` is a valid char boundary (end of a found substring); advance
        // past this occurrence and keep scanning for a boundary-aligned one.
        start = end;
    }
    false
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Apply a hook to a payload. Pure, so it is unit-testable without IO.
fn apply(hook: &PackHook, payload: &serde_json::Value) -> Result<Decision> {
    match hook {
        PackHook::BlockOnMatch {
            mode,
            field,
            patterns,
            message,
            ..
        } => {
            if *mode == MatchMode::Regex {
                bail!("regex match mode is not supported yet; use substring or word patterns");
            }
            let text = scan_text(payload, field.as_deref());
            for p in patterns {
                let pat = unescape(p);
                let hit = match mode {
                    MatchMode::Word => word_match(&text, &pat),
                    _ => text.contains(&pat),
                };
                if hit {
                    return Ok(Decision::Block(message.clone()));
                }
            }
            Ok(Decision::Allow)
        }
        PackHook::BlockUnlessMatch {
            require, message, ..
        } => {
            let text = scan_text(payload, None);
            if text.contains(&unescape(require)) {
                Ok(Decision::Allow)
            } else {
                Ok(Decision::Block(message.clone()))
            }
        }
        PackHook::Allowlist {
            field,
            allow,
            message,
            ..
        } => {
            let val = payload
                .get("tool_input")
                .and_then(|ti| ti.get(field))
                .and_then(|v| {
                    v.as_str()
                        .map(str::to_string)
                        .or_else(|| Some(v.to_string()))
                });
            match val {
                Some(v) if allow.contains(&v) => Ok(Decision::Allow),
                _ => Ok(Decision::Block(message.clone())),
            }
        }
        PackHook::Inject { text, .. } => Ok(Decision::Inject(text.replace("{{date}}", &today()))),
        PackHook::ReviewPrompt { prompt, .. } => {
            // review_prompt is normally registered as a settings prompt hook and
            // never reaches this runner. If it does, surface the prompt as an
            // injected note rather than doing nothing.
            Ok(Decision::Inject(prompt.clone()))
        }
    }
}

/// CLI entry: load the pack's hook config, apply hook `index` to the stdin
/// payload, map the decision to the harness hook protocol. `config` is the
/// absolute path installs since 1.8.0 register in settings.json; when absent
/// (a pre-1.8.0 install with no `--config` on the registered command), fall
/// back to `resolve_config_path`'s upward walk for the old `.kazam/packs/`
/// location.
pub fn run(pack: &str, index: usize, config: Option<std::path::PathBuf>, dir: &Path) -> Result<()> {
    let path = config.unwrap_or_else(|| resolve_config_path(dir, pack));
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("no hook config at {}", path.display()))?;
    let hooks: Vec<PackHook> = serde_yaml::from_str(&raw)
        .with_context(|| format!("invalid hook config {}", path.display()))?;
    let hook = hooks
        .get(index)
        .with_context(|| format!("pack '{}' has no hook at index {}", pack, index))?;

    let mut stdin = String::new();
    std::io::stdin().read_to_string(&mut stdin).ok();
    let payload: serde_json::Value =
        serde_json::from_str(&stdin).unwrap_or(serde_json::Value::Null);

    match apply(hook, &payload)? {
        Decision::Allow => Ok(()),
        Decision::Inject(text) => {
            // stdout on UserPromptSubmit/SessionStart is added to context.
            print!("{}", text);
            Ok(())
        }
        Decision::Block(message) => {
            // Exit 2 signals the harness to block the tool call; stderr is shown.
            eprintln!("{}", message);
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{HookMatch, InjectEvent};

    fn write_payload(content: &str) -> serde_json::Value {
        serde_json::json!({ "tool_input": { "content": content } })
    }

    fn block_hook(patterns: &[&str]) -> PackHook {
        PackHook::BlockOnMatch {
            on: HookMatch {
                tool: "Write".into(),
            },
            mode: MatchMode::Substring,
            field: None,
            patterns: patterns.iter().map(|s| s.to_string()).collect(),
            message: "blocked".into(),
        }
    }

    fn word_hook(patterns: &[&str]) -> PackHook {
        PackHook::BlockOnMatch {
            on: HookMatch {
                tool: "Write".into(),
            },
            mode: MatchMode::Word,
            field: None,
            patterns: patterns.iter().map(|s| s.to_string()).collect(),
            message: "blocked".into(),
        }
    }

    #[test]
    fn unescape_handles_unicode() {
        assert_eq!(unescape("\\u2014"), "\u{2014}");
        assert_eq!(unescape("plain"), "plain");
        assert_eq!(unescape("a\\u2014b"), "a\u{2014}b");
    }

    #[test]
    fn block_on_match_blocks_and_allows() {
        let h = block_hook(&["delve", "\\u2014"]);
        assert_eq!(
            apply(&h, &write_payload("let us delve in")).unwrap(),
            Decision::Block("blocked".into())
        );
        // em dash via escape
        assert_eq!(
            apply(&h, &write_payload("a \u{2014} b")).unwrap(),
            Decision::Block("blocked".into())
        );
        assert_eq!(
            apply(&h, &write_payload("clean prose")).unwrap(),
            Decision::Allow
        );
    }

    #[test]
    fn block_unless_match() {
        let h = PackHook::BlockUnlessMatch {
            on: HookMatch {
                tool: "Write".into(),
            },
            require: "SIGNED-OFF".into(),
            message: "needs sign-off".into(),
        };
        assert_eq!(
            apply(&h, &write_payload("no marker")).unwrap(),
            Decision::Block("needs sign-off".into())
        );
        assert_eq!(
            apply(&h, &write_payload("SIGNED-OFF here")).unwrap(),
            Decision::Allow
        );
    }

    #[test]
    fn allowlist_checks_field() {
        let h = PackHook::Allowlist {
            on: HookMatch { tool: "mcp".into() },
            field: "db_id".into(),
            allow: vec!["1".into(), "2".into()],
            message: "db not allowed".into(),
        };
        let ok = serde_json::json!({ "tool_input": { "db_id": "2" } });
        let bad = serde_json::json!({ "tool_input": { "db_id": "9" } });
        assert_eq!(apply(&h, &ok).unwrap(), Decision::Allow);
        assert_eq!(
            apply(&h, &bad).unwrap(),
            Decision::Block("db not allowed".into())
        );
    }

    #[test]
    fn inject_substitutes_date() {
        let h = PackHook::Inject {
            event: InjectEvent::UserPromptSubmit,
            text: "today is {{date}}".into(),
        };
        match apply(&h, &serde_json::Value::Null).unwrap() {
            Decision::Inject(t) => {
                assert!(t.starts_with("today is 20"));
                assert!(!t.contains("{{date}}"));
            }
            other => panic!("expected inject, got {:?}", other),
        }
    }

    #[test]
    fn word_mode_respects_boundaries() {
        let h = word_hook(&["foster"]);
        // whole word blocked
        assert_eq!(
            apply(&h, &write_payload("we foster growth")).unwrap(),
            Decision::Block("blocked".into())
        );
        // word at string edge (inside serialized tool_input) blocked
        assert_eq!(
            apply(&h, &write_payload("foster")).unwrap(),
            Decision::Block("blocked".into())
        );
        // substring inside a larger word is NOT blocked
        assert_eq!(
            apply(&h, &write_payload("fostering a culture")).unwrap(),
            Decision::Allow
        );
        assert_eq!(
            apply(&h, &write_payload("a defoster unit")).unwrap(),
            Decision::Allow
        );
    }

    #[test]
    fn word_mode_unescapes() {
        // em dash via escape, word_match on a non-word char pattern still hits
        let h = word_hook(&["\\u2014"]);
        assert_eq!(
            apply(&h, &write_payload("a \u{2014} b")).unwrap(),
            Decision::Block("blocked".into())
        );
    }

    #[test]
    fn field_scopes_the_scan() {
        // Scanning only tool_input.text: a match in `text` blocks, and a slop
        // word appearing only in another field (or a field name) does not.
        let h = PackHook::BlockOnMatch {
            on: HookMatch {
                tool: "mcp__claude_ai_Slack__slack_send_message".into(),
            },
            mode: MatchMode::Word,
            field: Some("text".into()),
            patterns: vec!["delve".into()],
            message: "blocked".into(),
        };
        let hit =
            serde_json::json!({ "tool_input": { "text": "let us delve in", "channel": "C1" } });
        let miss =
            serde_json::json!({ "tool_input": { "text": "clean copy", "channel": "delve" } });
        assert_eq!(apply(&h, &hit).unwrap(), Decision::Block("blocked".into()));
        assert_eq!(apply(&h, &miss).unwrap(), Decision::Allow);
    }

    #[test]
    fn regex_mode_errors() {
        let h = PackHook::BlockOnMatch {
            on: HookMatch {
                tool: "Write".into(),
            },
            mode: MatchMode::Regex,
            field: None,
            patterns: vec!["a.*b".into()],
            message: "x".into(),
        };
        assert!(apply(&h, &write_payload("aXb")).is_err());
    }
}

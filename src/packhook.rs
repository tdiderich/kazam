//! `kazam pack-hook` — the trusted runner for declarative pack hooks.
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

/// Where a pack's hook config lives, relative to the install dir.
pub fn config_path(dir: &Path, slug: &str) -> std::path::PathBuf {
    dir.join(".kazam")
        .join("packs")
        .join(format!("{}.hooks.yaml", slug))
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

/// Text a block/require primitive scans: the serialized `tool_input`, falling
/// back to the whole payload. Mirrors how the existing shell guards grep the
/// full hook input.
fn scan_text(payload: &serde_json::Value) -> String {
    match payload.get("tool_input") {
        Some(v) => v.to_string(),
        None => payload.to_string(),
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Apply a hook to a payload. Pure, so it is unit-testable without IO.
fn apply(hook: &PackHook, payload: &serde_json::Value) -> Result<Decision> {
    match hook {
        PackHook::BlockOnMatch {
            mode,
            patterns,
            message,
            ..
        } => {
            if *mode == MatchMode::Regex {
                bail!("regex match mode is not supported yet; use substring patterns");
            }
            let text = scan_text(payload);
            for p in patterns {
                if text.contains(&unescape(p)) {
                    return Ok(Decision::Block(message.clone()));
                }
            }
            Ok(Decision::Allow)
        }
        PackHook::BlockUnlessMatch {
            require, message, ..
        } => {
            let text = scan_text(payload);
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

/// CLI entry: load `.kazam/packs/<pack>.hooks.yaml`, apply hook `index` to the
/// stdin payload, map the decision to the harness hook protocol.
pub fn run(pack: &str, index: usize, dir: &Path) -> Result<()> {
    let path = config_path(dir, pack);
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
    fn regex_mode_errors() {
        let h = PackHook::BlockOnMatch {
            on: HookMatch {
                tool: "Write".into(),
            },
            mode: MatchMode::Regex,
            patterns: vec!["a.*b".into()],
            message: "x".into(),
        };
        assert!(apply(&h, &write_payload("aXb")).is_err());
    }
}

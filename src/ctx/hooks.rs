use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

const SESSION_START_SH: &str = r#"#!/bin/bash
# kazam workspace — session start hook
# Surfaces anatomy drift and ready tasks at the start of each agent session.
# Silent when nothing is actionable (no drift, no ready tasks).
if ! command -v kazam &>/dev/null; then
  echo '{"ok":false,"error":"kazam not installed — run: cargo install --git https://github.com/tdiderich/kazam"}'
  exit 0
fi
DRIFT=$(kazam ctx scan --check --json 2>/dev/null)
READY=$(kazam track ready --json 2>/dev/null)
HAS_DRIFT=$(echo "$DRIFT" | grep -c '"new_files":\[\|"deleted_files":\[\|"changed_files":\[' 2>/dev/null || true)
HAS_READY=$(echo "$READY" | grep -c '"data":\[{' 2>/dev/null || true)
if [ "$HAS_DRIFT" != "0" ] || [ "$HAS_READY" != "0" ]; then
  [ "$HAS_DRIFT" != "0" ] && echo "$DRIFT"
  [ "$HAS_READY" != "0" ] && echo "$READY"
fi
"#;

const POST_WRITE_SH: &str = r#"#!/bin/bash
# kazam workspace — post-write hook
# Logs file modifications to the activity feed.
FILE="$(echo "$KAZAM_TOOL_INPUT" | grep -o '"file_path":"[^"]*"' | head -1 | cut -d'"' -f4)"
if [ -n "$FILE" ]; then
  kazam track log add "Modified $FILE" --source "${KAZAM_AGENT:-agent}" --severity info 2>/dev/null
fi
"#;

const STOP_SH: &str = r#"#!/bin/bash
# kazam workspace — session stop hook
# Rescans anatomy, then summarizes session activity and suggests enrichment.
kazam ctx scan 2>/dev/null
DIFF=$(kazam ctx scan --check --json 2>/dev/null)
NEW=$(echo "$DIFF" | grep -o '"new_files":\[[^]]*\]' | grep -o '"[^"]*\..*"' | wc -l 2>/dev/null | tr -d ' ')
CHANGED=$(echo "$DIFF" | grep -o '"changed_files":\[[^]]*\]' | grep -o '"[^"]*\..*"' | wc -l 2>/dev/null | tr -d ' ')
if [ "${NEW:-0}" != "0" ] || [ "${CHANGED:-0}" != "0" ]; then
  echo "kazam: session touched ${CHANGED:-0} changed + ${NEW:-0} new files"
  echo "  → enrich descriptions: kazam ctx describe <path> \"what this file does\""
  echo "  → record learnings:    kazam ctx learn \"lesson\" --category correction"
  echo "  → record bugs:         kazam ctx bug \"symptom\" --file <path>"
fi
"#;

const WORKSPACE_RULES: &str = r#"# Kazam Workspace

This project uses **kazam** for task tracking and context intelligence.
Use kazam for ALL task tracking — do NOT use the built-in TaskCreate/TaskUpdate tools.
State lives in `.kazam/` as YAML files.

## Prerequisites
- kazam must be installed: `cargo install --git https://github.com/tdiderich/kazam`
- If `kazam` is not on PATH, install it before using any workspace commands.

## Navigating the codebase — MANDATORY
**Before you `grep`, `find`, `ls`, or spawn a subagent to explore, read the
anatomy index.** This is not optional. The index exists so you don't waste
tokens scanning the filesystem.

**Step 1 — Read the summary:**
`.kazam/ctx/anatomy.tsv` — compact index with root files and directory rollups
(file count, total tokens, description). ~68 lines even for huge repos.

**Step 2 — Drill into a directory:**
`.kazam/ctx/anatomy/<dir>.tsv` — individual files in that directory.
Nested paths use `--` as separator: `frontend/src/app` → `anatomy/frontend--src--app.tsv`.

**Step 3 — Read the source file you need.**

Summary → detail → source. Three reads, zero exploration.

**For multi-file exploration** (where is X, what calls Y, bug hunts across
directories), dispatch the `kazam-scout` agent instead of exploring in your
own context. It navigates anatomy-first and returns compact `file:line`
citations, keeping file dumps out of the main conversation.

**When delegating to subagents:** subagents don't see these rules, so you
must brief them. Include in every subagent prompt:
1. **Anatomy:** "Read `.kazam/ctx/anatomy.tsv` for project layout, then
   `.kazam/ctx/anatomy/<dir>.tsv` for the directory you need — don't
   grep or find for structure."
2. **Task context:** "You are working on task `<ID>`: <title>. When done,
   run `kazam track close <ID> --reason '<what you did>'`."
3. **Enrichment:** "After reading an unfamiliar file, run
   `kazam ctx describe <path> '<description>'`."

## On session start or context recovery
Run `kazam track ready --json` to orient — see unblocked tasks sorted by priority.
If resuming from a compacted context, this is how you re-establish what needs doing.

## Before starting work
- Claim a task: `kazam track claim <ID> --name <your-name>`.
- **MANDATORY: before fixing any error**, run `kazam ctx bugs --file <path>`
  to check if it was solved before. Do not skip this step.

## During work — close tasks as you go, don't batch
- **After each commit**, check if it completes an open task. If so, close it
  immediately: `kazam track close <ID> --reason "what you did"`.
- Tasks with `--owner human` are not yours to close. If one blocks your work,
  mark it blocked: `kazam track block <ID> --reason "why"`. When the user
  completes a human task, close it for them.
- After reading an unfamiliar file, enrich its description:
  `kazam ctx describe <path> "what this file actually does"`.
- Record non-obvious learnings: `kazam ctx learn "lesson" --category correction`.
- Record bugs you find: `kazam ctx bug "symptom" --file <path>`.
- When the user corrects your approach, record it immediately:
  `kazam ctx correction "what you did wrong" "what to do instead" --file <path>`.

## Quick reference
```
kazam track ready --json     # unblocked tasks by priority
kazam track close <ID> --reason "..."   # mark task done
kazam track block <ID> --reason "..."   # mark task blocked
kazam track list --json      # all tasks with status
kazam ctx describe <path> "description" # enrich file description
kazam ctx bugs --file <path> # known bugs on a file
kazam ctx learn "lesson" --category correction
kazam ctx bug "symptom" --file <path>
kazam ctx correction "mistake" "fix" --file <path>  # record a correction
kazam ctx corrections --json   # view past corrections
```

## Direct YAML editing
You may edit `.kazam/track/tasks.yaml` or `.kazam/ctx/*.yaml` directly.
The board (`kazam board`) auto-refreshes on any `.kazam/*.yaml` change.
"#;

const SCOUT_AGENT: &str = r#"---
name: kazam-scout
description: Read-only repository scout. Locates code fast and returns compact file:line citations instead of file dumps. Use for "where is X defined", "what calls Y", "which files handle Z" before making changes. Navigates via the kazam anatomy index instead of blind grep.
tools: Read, Glob, Grep, Bash
model: haiku
---

You are kazam-scout, a repository exploration subagent. Your job is to find
code and return citations — never to fix, refactor, or judge it.

## Protocol

1. Read `.kazam/ctx/anatomy.tsv` first — root files and directory rollups.
2. Drill into `.kazam/ctx/anatomy/<dir>.tsv` for the directories that matter.
   Nested paths use `--` as separator: `src/app/api` → `anatomy/src--app--api.tsv`.
3. Confirm with targeted Read/Grep on specific files. Issue independent
   searches in parallel, not one at a time.
4. Verify every citation by reading the actual lines before reporting.

## Output contract

Return ONLY this format:

FINDINGS
- path/to/file.rs:42-58 — router definition, handles the auth redirect
- path/to/other.ts:101-119 — the only caller

NOT FOUND (only if applicable)
- searched: <patterns and directories covered>

Rules:
- Max 10 citations, ranked by relevance.
- One line of "why it matters" per citation. No code blocks longer than 3 lines.
- Never propose fixes, improvements, or opinions on code quality.
- If anatomy lists a file that doesn't exist on disk, note it as stale and move on.

## Enrichment

After reading a file whose anatomy description is empty or generic, run:
`kazam ctx describe <path> "<one line on what it actually does>"`
"#;

pub fn install(project: &Path, agent: &str, skunkworks: bool) -> Result<()> {
    let hooks_dir = crate::workspace::root(project).join("hooks");
    fs::create_dir_all(&hooks_dir).context("create hooks dir")?;

    fs::write(hooks_dir.join("session-start.sh"), SESSION_START_SH)?;
    fs::write(hooks_dir.join("post-write.sh"), POST_WRITE_SH)?;
    fs::write(hooks_dir.join("stop.sh"), STOP_SH)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["session-start.sh", "post-write.sh", "stop.sh"] {
            let p = hooks_dir.join(name);
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755))?;
        }
    }

    if agent == "claude" || agent == "all" {
        install_claude_hooks(project, skunkworks)?;
    }

    // Write workspace rules (base + optional team override)
    let rules_dir = project.join(".claude").join("rules");
    fs::create_dir_all(&rules_dir).context("create .claude/rules")?;

    let override_path = crate::workspace::root(project).join("ctx/rules-override.md");
    let mut rules = WORKSPACE_RULES.to_string();
    if override_path.exists() {
        let custom = fs::read_to_string(&override_path).unwrap_or_default();
        if !custom.trim().is_empty() {
            rules.push_str("\n## Team overrides\n\n");
            rules.push_str(&custom);
            rules.push('\n');
        }
    }

    fs::write(rules_dir.join("kazam-workspace.md"), &rules).context("write workspace rules")?;

    // Write the scout agent definition (anatomy-first repository explorer)
    let agents_dir = project.join(".claude").join("agents");
    fs::create_dir_all(&agents_dir).context("create .claude/agents")?;
    fs::write(agents_dir.join("kazam-scout.md"), SCOUT_AGENT).context("write kazam-scout agent")?;

    let settings_name = if skunkworks {
        "settings.local.json"
    } else {
        "settings.json"
    };
    println!("  ✓ hooks installed to .kazam/hooks/");
    if agent == "claude" || agent == "all" {
        println!("  ✓ Claude Code hooks registered in .claude/{settings_name}");
    }
    println!("  ✓ workspace rules written to .claude/rules/kazam-workspace.md");
    println!("  ✓ scout agent written to .claude/agents/kazam-scout.md");
    if override_path.exists() {
        println!("  ✓ team overrides applied from .kazam/ctx/rules-override.md");
    }
    Ok(())
}

pub fn uninstall(project: &Path) -> Result<()> {
    let hooks_dir = crate::workspace::root(project).join("hooks");
    if hooks_dir.exists() {
        fs::remove_dir_all(&hooks_dir).context("remove hooks dir")?;
        fs::create_dir_all(&hooks_dir).context("recreate hooks dir")?;
    }

    let rules_file = project.join(".claude/rules/kazam-workspace.md");
    if rules_file.exists() {
        fs::remove_file(&rules_file).context("remove workspace rules")?;
    }

    let scout_file = project.join(".claude/agents/kazam-scout.md");
    if scout_file.exists() {
        fs::remove_file(&scout_file).context("remove kazam-scout agent")?;
    }

    // Remove only kazam entries from .claude/settings.json, preserve everything else
    let settings_path = project.join(".claude/settings.json");
    if settings_path.exists() {
        let text = fs::read_to_string(&settings_path)?;
        if let Ok(mut settings) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(obj) = settings.as_object_mut() {
                if let Some(hooks) = obj.get_mut("hooks") {
                    if let Some(hooks_obj) = hooks.as_object_mut() {
                        for event in ["SessionStart", "PostToolUse", "Stop"] {
                            if let Some(arr) =
                                hooks_obj.get_mut(event).and_then(|v| v.as_array_mut())
                            {
                                arr.retain(|item| {
                                    let nested = item
                                        .pointer("/hooks/0/description")
                                        .and_then(|d| d.as_str());
                                    let flat =
                                        item.pointer("/description").and_then(|d| d.as_str());
                                    !nested.is_some_and(|d| d.starts_with("kazam-workspace:"))
                                        && !flat.is_some_and(|d| d.starts_with("kazam-workspace:"))
                                });
                                if arr.is_empty() {
                                    hooks_obj.remove(event);
                                }
                            }
                        }
                    }
                }
            }
            let json = serde_json::to_string_pretty(&settings)?;
            fs::write(&settings_path, json)?;
        }
    }

    println!("  ✓ hooks uninstalled");
    Ok(())
}

pub fn status(project: &Path) -> Result<()> {
    let hooks_dir = crate::workspace::root(project).join("hooks");
    let scripts = ["session-start.sh", "post-write.sh", "stop.sh"];

    let mut installed = 0;
    for name in &scripts {
        if hooks_dir.join(name).exists() {
            installed += 1;
        }
    }

    let settings_path = project.join(".claude/settings.json");
    let claude_registered = if settings_path.exists() {
        let text = fs::read_to_string(&settings_path).unwrap_or_default();
        text.contains("kazam-workspace")
    } else {
        false
    };

    let rules_exist = project.join(".claude/rules/kazam-workspace.md").exists();

    println!("  hook scripts: {installed}/{} installed", scripts.len());
    println!(
        "  claude hooks: {}",
        if claude_registered {
            "registered"
        } else {
            "not registered"
        }
    );
    println!(
        "  workspace rules: {}",
        if rules_exist { "present" } else { "missing" }
    );
    Ok(())
}

fn install_claude_hooks(project: &Path, skunkworks: bool) -> Result<()> {
    let settings_file = if skunkworks {
        "settings.local.json"
    } else {
        "settings.json"
    };
    let settings_path = project.join(".claude").join(settings_file);
    fs::create_dir_all(project.join(".claude")).context("create .claude")?;

    let mut settings: serde_json::Value = if settings_path.exists() {
        let text = fs::read_to_string(&settings_path)?;
        serde_json::from_str(&text).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let hooks_dir = crate::workspace::root(project).join("hooks");
    let hooks_abs = hooks_dir
        .canonicalize()
        .unwrap_or(hooks_dir.clone())
        .to_string_lossy()
        .to_string();

    let obj = settings.as_object_mut().unwrap();
    let hooks = obj
        .entry("hooks")
        .or_insert(serde_json::json!({}))
        .as_object_mut()
        .unwrap();

    let kazam_hooks = [
        (
            "SessionStart",
            serde_json::json!({
                "matcher": "",
                "hooks": [{
                    "type": "command",
                    "command": format!("bash {hooks_abs}/session-start.sh"),
                    "description": "kazam-workspace: surface anatomy drift and ready tasks"
                }]
            }),
        ),
        (
            "PostToolUse",
            serde_json::json!({
                "matcher": "Write|Edit",
                "hooks": [{
                    "type": "command",
                    "command": format!("bash {hooks_abs}/post-write.sh"),
                    "description": "kazam-workspace: log file modifications"
                }]
            }),
        ),
        (
            "Stop",
            serde_json::json!({
                "matcher": "",
                "hooks": [{
                    "type": "command",
                    "command": format!("bash {hooks_abs}/stop.sh"),
                    "description": "kazam-workspace: rescan anatomy on session end"
                }]
            }),
        ),
    ];

    for (event, entry) in kazam_hooks {
        let arr = hooks
            .entry(event)
            .or_insert(serde_json::json!([]))
            .as_array_mut()
            .unwrap();

        // Remove any existing kazam entries (by description prefix) to avoid duplicates.
        // Check both nested format (/hooks/0/description) and legacy flat format (/description).
        arr.retain(|item| {
            let nested = item
                .pointer("/hooks/0/description")
                .and_then(|d| d.as_str());
            let flat = item.pointer("/description").and_then(|d| d.as_str());
            !nested.is_some_and(|d| d.starts_with("kazam-workspace:"))
                && !flat.is_some_and(|d| d.starts_with("kazam-workspace:"))
        });

        arr.push(entry);
    }

    let json = serde_json::to_string_pretty(&settings)?;
    fs::write(&settings_path, json).with_context(|| format!("write .claude/{settings_file}"))?;
    Ok(())
}

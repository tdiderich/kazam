mod graph;
mod store;
pub mod types;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use std::path::Path;

use types::*;

#[derive(Subcommand)]
pub enum Command {
    /// Initialize .kazam/track/ with empty stores
    Init {
        /// Gitignore .kazam/ for shared repos
        #[arg(long)]
        skunkworks: bool,
    },
    /// Add a new task
    Add {
        /// One-line task title
        title: String,
        /// 0 (highest) through 9 (lowest)
        #[arg(short, long, default_value_t = 2)]
        priority: u8,
        /// Freeform category, e.g. task, bug, epic
        #[arg(short = 't', long, default_value = "task")]
        r#type: String,
        /// Who owns closing this: agent or human
        #[arg(long, default_value = "agent")]
        owner: String,
        /// Parent task ID, for subtasks under an epic
        #[arg(long)]
        parent: Option<String>,
        /// Task IDs this one blocks, comma-separated
        #[arg(long, value_delimiter = ',')]
        blocks: Vec<String>,
        /// Assignee name, claims the task immediately
        #[arg(long)]
        assign: Option<String>,
        /// Freeform note attached to the task
        #[arg(long)]
        note: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Show tasks with no open blockers, sorted by priority
    Ready {
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Atomically claim a task (set assignee + active)
    Claim {
        /// Task ID
        id: String,
        /// Claimant name (alias: --as)
        #[arg(long, alias = "as")]
        name: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Close a completed task
    Close {
        /// Task ID
        id: String,
        /// What was done, recorded on the task
        #[arg(long)]
        reason: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Mark a task as blocked
    Block {
        /// Task ID
        id: String,
        /// Why it's blocked, recorded on the task
        #[arg(long)]
        reason: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// List tasks (optionally filtered)
    List {
        /// Filter by status: open, closed, blocked
        #[arg(long)]
        status: Option<String>,
        /// Filter by assignee name
        #[arg(long)]
        assignee: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Show the task tree
    Tree {
        /// all, open, or closed
        #[arg(long, default_value = "all")]
        filter: String,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Show full details for a task
    Show {
        /// Task ID
        id: String,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Import tasks from a markdown plan (## headings → epics, - bullets → tasks)
    Import {
        /// Path to a markdown file
        file: String,
        /// Preview without creating tasks
        #[arg(long)]
        dry_run: bool,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Manage dependencies
    Dep {
        #[command(subcommand)]
        action: DepAction,
    },
    /// Show or add to the activity log
    Log {
        #[command(subcommand)]
        action: Option<LogAction>,
        /// Max entries to show
        #[arg(long, default_value_t = 25)]
        limit: usize,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum DepAction {
    /// Add a dependency: BLOCKER blocks BLOCKED
    Add {
        /// Task ID that must close first
        blocker: String,
        /// Task ID that's blocked
        blocked: String,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Remove a dependency
    Rm {
        /// Task ID that must close first
        blocker: String,
        /// Task ID that's blocked
        blocked: String,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum LogAction {
    /// Add a manual log entry
    Add {
        /// One-line entry title
        title: String,
        /// Where this came from, freeform
        #[arg(long)]
        source: Option<String>,
        /// info, warning, or major
        #[arg(long, default_value = "info")]
        severity: String,
        /// Associate with an existing task ID
        #[arg(long)]
        task_id: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: Command, project: &Path) -> Result<()> {
    match cmd {
        Command::Init { skunkworks } => cmd_init(project, skunkworks),
        Command::Add {
            title,
            priority,
            r#type,
            owner,
            parent,
            blocks,
            assign,
            note,
            json,
        } => cmd_add(
            project, title, priority, &r#type, &owner, parent, blocks, assign, note, json,
        ),
        Command::Ready { json } => cmd_ready(project, json),
        Command::Claim { id, name, json } => cmd_claim(project, &id, name, json),
        Command::Close { id, reason, json } => cmd_close(project, &id, reason, json),
        Command::Block { id, reason, json } => cmd_block(project, &id, reason, json),
        Command::List {
            status,
            assignee,
            json,
        } => cmd_list(project, status, assignee, json),
        Command::Tree { filter, json } => cmd_tree(project, &filter, json),
        Command::Show { id, json } => cmd_show(project, &id, json),
        Command::Import {
            file,
            dry_run,
            json,
        } => cmd_import(project, &file, dry_run, json),
        Command::Dep { action } => match action {
            DepAction::Add {
                blocker,
                blocked,
                json,
            } => cmd_dep_add(project, &blocker, &blocked, json),
            DepAction::Rm {
                blocker,
                blocked,
                json,
            } => cmd_dep_rm(project, &blocker, &blocked, json),
        },
        Command::Log {
            action,
            limit,
            json,
        } => match action {
            Some(LogAction::Add {
                title,
                source,
                severity,
                task_id,
                json: j,
            }) => cmd_log_add(project, title, source, &severity, task_id, j),
            None => cmd_log_list(project, limit, json),
        },
    }
}

// ── Helpers ──────────────────────────────────────

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

fn json_ok<T: serde::Serialize>(data: &T) {
    println!("{}", serde_json::json!({ "ok": true, "data": data }));
}

fn json_err(msg: &str) {
    println!("{}", serde_json::json!({ "ok": false, "error": msg }));
}

fn parse_severity(s: &str) -> Result<LogSeverity> {
    match s {
        "major" => Ok(LogSeverity::Major),
        "minor" => Ok(LogSeverity::Minor),
        "info" => Ok(LogSeverity::Info),
        _ => bail!("unknown severity: {s} (expected major|minor|info)"),
    }
}

fn parse_task_type(s: &str) -> Result<TaskType> {
    match s {
        "task" => Ok(TaskType::Task),
        "bug" => Ok(TaskType::Bug),
        "feature" => Ok(TaskType::Feature),
        "epic" => Ok(TaskType::Epic),
        _ => bail!("unknown type: {s} (expected task|bug|feature|epic)"),
    }
}

// ── Commands ─────────────────────────────────────

fn cmd_init(project: &Path, skunkworks: bool) -> Result<()> {
    crate::workspace::ensure(project)?;
    if skunkworks {
        crate::workspace::set_skunkworks(project)?;
    }
    println!("  ✓ .kazam/track/ ready");
    if !crate::workspace::hooks_installed(project) {
        println!("  hint: run `kazam ctx hooks install` to wire up agent hooks");
        println!("        or `kazam workspace init` for the full setup");
    }
    Ok(())
}

fn cmd_import(project: &Path, file: &str, dry_run: bool, json: bool) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("read plan file: {file}"))?;

    let mut tasks: Vec<(String, Option<String>)> = Vec::new(); // (title, parent_title)
    let mut current_epic: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        if let Some(heading) = trimmed.strip_prefix("## ") {
            let title = heading.trim();
            if !title.is_empty() {
                current_epic = Some(title.to_string());
                tasks.push((title.to_string(), None));
            }
        } else if let Some(rest) = trimmed.strip_prefix("- ") {
            let title = rest
                .trim_start_matches("[ ] ")
                .trim_start_matches("[x] ")
                .trim_start_matches("**")
                .trim_end_matches("**")
                .trim();
            if !title.is_empty() && !title.starts_with('[') {
                tasks.push((title.to_string(), current_epic.clone()));
            }
        }
    }

    if tasks.is_empty() {
        if json {
            json_ok(&serde_json::json!({ "imported": 0 }));
        } else {
            println!("  no tasks found in {file}");
        }
        return Ok(());
    }

    if dry_run {
        if json {
            let items: Vec<serde_json::Value> = tasks
                .iter()
                .map(|(title, parent)| {
                    serde_json::json!({
                        "title": title,
                        "parent": parent,
                        "type": if parent.is_none() { "epic" } else { "task" }
                    })
                })
                .collect();
            json_ok(&serde_json::json!({ "dry_run": true, "tasks": items }));
        } else {
            println!("  dry run - {} tasks found in {file}:", tasks.len());
            for (title, parent) in &tasks {
                if parent.is_none() {
                    println!("    ★ {title} (epic)");
                } else {
                    println!("      ○ {title}");
                }
            }
        }
        return Ok(());
    }

    crate::workspace::ensure(project)?;
    let mut s = store::read_tasks(project)?;
    let ts = now();
    let mut epic_ids: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut created = 0;

    for (title, parent_title) in &tasks {
        let id = crate::id::generate();
        let is_epic = parent_title.is_none();
        let parent_id = parent_title
            .as_ref()
            .and_then(|pt| epic_ids.get(pt))
            .cloned();

        let task = Task {
            id: id.clone(),
            title: title.clone(),
            status: TaskStatus::Open,
            priority: if is_epic { 1 } else { 2 },
            task_type: if is_epic {
                TaskType::Epic
            } else {
                TaskType::Task
            },
            owner: types::TaskOwner::Agent,
            assignee: None,
            parent: parent_id,
            blocks: vec![],
            related: vec![],
            note: None,
            created: ts.clone(),
            updated: ts.clone(),
            closed: None,
            close_reason: None,
        };

        if is_epic {
            epic_ids.insert(title.clone(), id.clone());
        }

        s.tasks.push(task);
        created += 1;
    }

    store::write_tasks(project, &s)?;
    store::append_log(
        project,
        LogEntry {
            date: ts,
            title: format!("imported {created} tasks from {file}"),
            detail: None,
            severity: LogSeverity::Major,
            source: None,
            task_id: None,
        },
    )?;

    if json {
        json_ok(&serde_json::json!({ "imported": created }));
    } else {
        println!("  ✓ imported {created} tasks from {file}");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_add(
    project: &Path,
    title: String,
    priority: u8,
    typ: &str,
    owner_str: &str,
    parent: Option<String>,
    blocks: Vec<String>,
    assign: Option<String>,
    note: Option<String>,
    json: bool,
) -> Result<()> {
    crate::workspace::ensure(project)?;
    let task_type = parse_task_type(typ)?;
    let owner: types::TaskOwner = owner_str
        .parse()
        .map_err(|e: String| anyhow::anyhow!("{e}"))?;
    let id = crate::id::generate();
    let ts = now();
    let task = Task {
        id: id.clone(),
        title: title.clone(),
        status: TaskStatus::Open,
        priority,
        task_type,
        owner,
        assignee: assign,
        parent,
        blocks,
        related: vec![],
        note,
        created: ts.clone(),
        updated: ts.clone(),
        closed: None,
        close_reason: None,
    };

    let mut s = store::read_tasks(project)?;
    s.tasks.push(task.clone());
    store::write_tasks(project, &s)?;

    store::append_log(
        project,
        LogEntry {
            date: ts,
            title: format!("added {id}: {title}"),
            detail: None,
            severity: LogSeverity::Info,
            source: None,
            task_id: Some(id.clone()),
        },
    )?;

    if json {
        json_ok(&task);
    } else {
        println!("  ✓ {id} - {title}");
    }
    Ok(())
}

fn cmd_ready(project: &Path, json: bool) -> Result<()> {
    let s = store::read_tasks(project)?;
    let ready = graph::ready(&s.tasks);

    if json {
        json_ok(&ready);
    } else if ready.is_empty() {
        println!("  no ready tasks");
    } else {
        for r in &ready {
            let t = &r.task;
            let meta = format!(
                "P{} · {}{}",
                t.priority,
                t.task_type.label(),
                t.assignee
                    .as_deref()
                    .map(|a| format!(" · {a}"))
                    .unwrap_or_default()
            );
            println!("  {} {} ({})", t.id, t.title, meta);
            if let Some(ref note) = t.note {
                println!("       {note}");
            }
            if let Some(ref parent) = r.parent_title {
                println!("       parent: {parent}");
            }
        }
    }
    Ok(())
}

fn cmd_claim(project: &Path, id: &str, name: Option<String>, json: bool) -> Result<()> {
    let mut s = store::read_tasks(project)?;
    let task = s.tasks.iter_mut().find(|t| t.id == id);
    let Some(task) = task else {
        if json {
            json_err(&format!("task {id} not found"));
        } else {
            bail!("task {id} not found");
        }
        return Ok(());
    };

    if task.status == TaskStatus::Active && task.assignee.is_some() {
        let owner = task.assignee.as_deref().unwrap_or("unknown");
        if json {
            json_err(&format!("already claimed by {owner}"));
        } else {
            bail!("already claimed by {owner}");
        }
        return Ok(());
    }

    let assignee = name.unwrap_or_else(|| "agent".to_string());
    task.assignee = Some(assignee.clone());
    task.status = TaskStatus::Active;
    task.updated = now();
    let title = task.title.clone();
    let task_out = task.clone();

    store::write_tasks(project, &s)?;
    store::append_log(
        project,
        LogEntry {
            date: now(),
            title: format!("{assignee} claimed {id}: {title}"),
            detail: None,
            severity: LogSeverity::Major,
            source: Some(assignee),
            task_id: Some(id.to_string()),
        },
    )?;

    if json {
        json_ok(&task_out);
    } else {
        println!("  ✓ claimed {id}: {title}");
    }
    Ok(())
}

fn cmd_close(project: &Path, id: &str, reason: Option<String>, json: bool) -> Result<()> {
    let mut s = store::read_tasks(project)?;
    let task = s.tasks.iter_mut().find(|t| t.id == id);
    let Some(task) = task else {
        if json {
            json_err(&format!("task {id} not found"));
        } else {
            bail!("task {id} not found");
        }
        return Ok(());
    };

    let ts = now();
    task.status = TaskStatus::Closed;
    task.closed = Some(ts.clone());
    task.close_reason = reason.clone();
    task.updated = ts.clone();
    let title = task.title.clone();
    let task_out = task.clone();

    store::write_tasks(project, &s)?;
    store::append_log(
        project,
        LogEntry {
            date: ts,
            title: format!("closed {id}: {title}"),
            detail: reason,
            severity: LogSeverity::Major,
            source: task_out.assignee.clone(),
            task_id: Some(id.to_string()),
        },
    )?;

    if json {
        json_ok(&task_out);
    } else {
        println!("  ✓ closed {id}: {title}");
    }
    Ok(())
}

fn cmd_block(project: &Path, id: &str, reason: Option<String>, json: bool) -> Result<()> {
    let mut s = store::read_tasks(project)?;
    let task = s.tasks.iter_mut().find(|t| t.id == id);
    let Some(task) = task else {
        if json {
            json_err(&format!("task {id} not found"));
        } else {
            bail!("task {id} not found");
        }
        return Ok(());
    };

    task.status = TaskStatus::Blocked;
    task.note = reason.clone().or(task.note.take());
    task.updated = now();
    let title = task.title.clone();
    let task_out = task.clone();

    store::write_tasks(project, &s)?;
    store::append_log(
        project,
        LogEntry {
            date: now(),
            title: format!("blocked {id}: {title}"),
            detail: reason,
            severity: LogSeverity::Minor,
            source: None,
            task_id: Some(id.to_string()),
        },
    )?;

    if json {
        json_ok(&task_out);
    } else {
        println!("  ⚠ blocked {id}: {title}");
    }
    Ok(())
}

fn cmd_list(
    project: &Path,
    status: Option<String>,
    assignee: Option<String>,
    json: bool,
) -> Result<()> {
    let s = store::read_tasks(project)?;
    let mut tasks: Vec<&Task> = s.tasks.iter().collect();

    if let Some(ref st) = status {
        let parsed: TaskStatus = st.parse().map_err(|e: String| anyhow::anyhow!(e))?;
        tasks.retain(|t| t.status == parsed);
    }
    if let Some(ref a) = assignee {
        tasks.retain(|t| t.assignee.as_deref() == Some(a.as_str()));
    }

    if json {
        json_ok(&tasks);
    } else if tasks.is_empty() {
        println!("  no tasks");
    } else {
        for t in &tasks {
            let glyph = match t.status {
                TaskStatus::Open => "○",
                TaskStatus::Active => "▸",
                TaskStatus::Closed => "✓",
                TaskStatus::Blocked => "⚠",
                TaskStatus::Deferred => "·",
            };
            println!("  {glyph} {} {} [{}]", t.id, t.title, t.status);
        }
    }
    Ok(())
}

fn cmd_tree(project: &Path, _filter: &str, json: bool) -> Result<()> {
    let s = store::read_tasks(project)?;

    if json {
        json_ok(&s.tasks);
        return Ok(());
    }

    // Build parent → children map
    let mut children: std::collections::HashMap<Option<&str>, Vec<&Task>> =
        std::collections::HashMap::new();
    for t in &s.tasks {
        children.entry(t.parent.as_deref()).or_default().push(t);
    }

    fn print_level(
        parent: Option<&str>,
        children: &std::collections::HashMap<Option<&str>, Vec<&Task>>,
        depth: usize,
    ) {
        let Some(kids) = children.get(&parent) else {
            return;
        };
        for t in kids {
            let indent = "  ".repeat(depth + 1);
            let glyph = match t.status {
                TaskStatus::Open => "○",
                TaskStatus::Active => "▸",
                TaskStatus::Closed => "✓",
                TaskStatus::Blocked => "⚠",
                TaskStatus::Deferred => "·",
            };
            println!("{indent}{glyph} {} {}", t.id, t.title);
            if let Some(ref note) = t.note {
                println!("{indent}  {note}");
            }
            print_level(Some(&t.id), children, depth + 1);
        }
    }

    print_level(None, &children, 0);
    Ok(())
}

fn cmd_show(project: &Path, id: &str, json: bool) -> Result<()> {
    let s = store::read_tasks(project)?;
    let task = s.tasks.iter().find(|t| t.id == id);
    let Some(task) = task else {
        if json {
            json_err(&format!("task {id} not found"));
        } else {
            bail!("task {id} not found");
        }
        return Ok(());
    };

    if json {
        let parent_title = task
            .parent
            .as_deref()
            .and_then(|pid| s.tasks.iter().find(|t| t.id == pid))
            .map(|t| t.title.clone());
        let blocker_titles: Vec<String> = graph::blocked_by(id, &s.tasks)
            .iter()
            .filter_map(|bid| s.tasks.iter().find(|t| t.id == *bid))
            .map(|t| t.title.clone())
            .collect();
        let rt = ReadyTask {
            task: task.clone(),
            parent_title,
            blocker_titles,
        };
        json_ok(&rt);
    } else {
        println!("  {} - {}", task.id, task.title);
        println!(
            "  status: {}  priority: {}  type: {}",
            task.status, task.priority, task.task_type
        );
        if let Some(ref a) = task.assignee {
            println!("  assignee: {a}");
        }
        if let Some(ref p) = task.parent {
            let ptitle = s
                .tasks
                .iter()
                .find(|t| t.id == *p)
                .map(|t| t.title.as_str())
                .unwrap_or("?");
            println!("  parent: {p} ({ptitle})");
        }
        if !task.blocks.is_empty() {
            println!("  blocks: {}", task.blocks.join(", "));
        }
        let open_blockers = graph::blocked_by(id, &s.tasks);
        if !open_blockers.is_empty() {
            println!("  blocked by: {}", open_blockers.join(", "));
        }
        if let Some(ref note) = task.note {
            println!("  note: {note}");
        }
    }
    Ok(())
}

fn cmd_dep_add(project: &Path, blocker: &str, blocked: &str, json: bool) -> Result<()> {
    let mut s = store::read_tasks(project)?;
    let task = s.tasks.iter_mut().find(|t| t.id == blocker);
    let Some(task) = task else {
        if json {
            json_err(&format!("task {blocker} not found"));
        } else {
            bail!("task {blocker} not found");
        }
        return Ok(());
    };

    if !task.blocks.contains(&blocked.to_string()) {
        task.blocks.push(blocked.to_string());
        task.updated = now();
    }
    store::write_tasks(project, &s)?;

    if let Some(cycle) = graph::has_cycle(&s.tasks) {
        // Revert
        let mut s = store::read_tasks(project)?;
        if let Some(t) = s.tasks.iter_mut().find(|t| t.id == blocker) {
            t.blocks.retain(|b| b != blocked);
            store::write_tasks(project, &s)?;
        }
        if json {
            json_err(&format!("cycle detected involving {cycle}"));
        } else {
            bail!("cycle detected involving {cycle} - dependency not added");
        }
        return Ok(());
    }

    if json {
        json_ok(&serde_json::json!({ "blocker": blocker, "blocked": blocked }));
    } else {
        println!("  ✓ {blocker} now blocks {blocked}");
    }
    Ok(())
}

fn cmd_dep_rm(project: &Path, blocker: &str, blocked: &str, json: bool) -> Result<()> {
    let mut s = store::read_tasks(project)?;
    let task = s.tasks.iter_mut().find(|t| t.id == blocker);
    let Some(task) = task else {
        if json {
            json_err(&format!("task {blocker} not found"));
        } else {
            bail!("task {blocker} not found");
        }
        return Ok(());
    };

    task.blocks.retain(|b| b != blocked);
    task.updated = now();
    store::write_tasks(project, &s)?;

    if json {
        json_ok(&serde_json::json!({ "blocker": blocker, "blocked": blocked }));
    } else {
        println!("  ✓ removed: {blocker} no longer blocks {blocked}");
    }
    Ok(())
}

fn cmd_log_add(
    project: &Path,
    title: String,
    source: Option<String>,
    severity: &str,
    task_id: Option<String>,
    json: bool,
) -> Result<()> {
    crate::workspace::ensure(project)?;
    let sev = parse_severity(severity)?;
    let entry = LogEntry {
        date: now(),
        title: title.clone(),
        detail: None,
        severity: sev,
        source,
        task_id,
    };
    store::append_log(project, entry.clone())?;

    if json {
        json_ok(&entry);
    } else {
        println!("  ✓ logged: {title}");
    }
    Ok(())
}

fn cmd_log_list(project: &Path, limit: usize, json: bool) -> Result<()> {
    let s = store::read_log(project)?;
    let events: Vec<&LogEntry> = s.events.iter().rev().take(limit).collect();

    if json {
        json_ok(&events);
    } else if events.is_empty() {
        println!("  no activity");
    } else {
        for e in &events {
            let src = e
                .source
                .as_deref()
                .map(|s| format!(" [{s}]"))
                .unwrap_or_default();
            println!("  {} -{src} {}", e.date, e.title);
        }
    }
    Ok(())
}

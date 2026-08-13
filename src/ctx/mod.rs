pub mod hooks;
pub mod scan;
pub mod types;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use std::path::Path;

use types::*;

#[derive(Subcommand)]
pub enum Command {
    /// Initialize .kazam/ctx/ (optionally scan files)
    Init {
        /// Run an initial anatomy scan right away
        #[arg(long)]
        scan: bool,
        /// Gitignore .kazam/ for shared repos
        #[arg(long)]
        skunkworks: bool,
    },
    /// Scan project files and update anatomy
    Scan {
        /// Report drift without writing changes
        #[arg(long)]
        check: bool,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Show context status summary
    Status {
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Update a file's anatomy description (agent-enriched)
    Describe {
        /// Path to the file, relative to the project
        file: String,
        /// What this file actually does
        description: String,
    },
    /// Record a learning
    Learn {
        /// The lesson learned, in one or two sentences
        text: String,
        /// preference, correction, or gotcha
        #[arg(long, default_value = "preference")]
        category: String,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// List all learnings
    Learnings {
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Record a bug encounter
    Bug {
        /// What went wrong, in one or two sentences
        symptom: String,
        /// File path the bug is associated with
        #[arg(long)]
        file: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// List bugs (optionally filtered by file path)
    Bugs {
        /// Only show bugs associated with this file path
        #[arg(long)]
        file: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Resolve a bug with a fix description
    Resolve {
        /// Bug ID
        id: String,
        /// How it was fixed
        #[arg(long)]
        fix: String,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Record a correction (agent got something wrong)
    Correction {
        /// What the agent did wrong
        mistake: String,
        /// What to do instead
        correction: String,
        /// File path the correction is associated with
        #[arg(long)]
        file: Option<String>,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// List all corrections
    Corrections {
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Consolidate stale data (remove old resolved bugs, deduplicate learnings)
    Consolidate {
        /// Only consolidate entries older than this many days
        #[arg(long, default_value = "30")]
        days: u32,
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Manage agent hooks (install/uninstall/status)
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },
}

#[derive(Subcommand)]
pub enum HooksAction {
    /// Install hook scripts and register with agent
    Install {
        /// Which agent to register hooks for
        #[arg(long, default_value = "claude")]
        agent: String,
    },
    /// Remove all hooks
    Uninstall,
    /// Show hook installation status
    Status,
}

pub fn run(cmd: Command, project: &Path) -> Result<()> {
    match cmd {
        Command::Init { scan, skunkworks } => cmd_init(project, scan, skunkworks),
        Command::Scan { check, json } => cmd_scan(project, check, json),
        Command::Status { json } => cmd_status(project, json),
        Command::Describe { file, description } => cmd_describe(project, &file, &description),
        Command::Learn {
            text,
            category,
            json,
        } => cmd_learn(project, text, &category, json),
        Command::Learnings { json } => cmd_learnings(project, json),
        Command::Bug {
            symptom,
            file,
            json,
        } => cmd_bug(project, symptom, file, json),
        Command::Bugs { file, json } => cmd_bugs(project, file, json),
        Command::Resolve { id, fix, json } => cmd_resolve(project, &id, &fix, json),
        Command::Correction {
            mistake,
            correction,
            file,
            json,
        } => cmd_correction(project, mistake, correction, file, json),
        Command::Corrections { json } => cmd_corrections(project, json),
        Command::Consolidate { days, json } => cmd_consolidate(project, days, json),
        Command::Hooks { action } => match action {
            HooksAction::Install { agent } => {
                let skunkworks = crate::workspace::read_config(project)
                    .map(|c| c.skunkworks)
                    .unwrap_or(false);
                hooks::install(project, &agent, skunkworks)
            }
            HooksAction::Uninstall => hooks::uninstall(project),
            HooksAction::Status => hooks::status(project),
        },
    }
}

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

fn json_ok<T: serde::Serialize>(data: &T) {
    println!("{}", serde_json::json!({ "ok": true, "data": data }));
}

fn json_err(msg: &str) {
    println!("{}", serde_json::json!({ "ok": false, "error": msg }));
}

/// Internal flat anatomy path used by board, describe, and status commands.
fn anatomy_path(project: &Path) -> std::path::PathBuf {
    crate::workspace::root(project).join("ctx/anatomy.flat.yaml")
}

fn learnings_path(project: &Path) -> std::path::PathBuf {
    crate::workspace::root(project).join("ctx/learnings.yaml")
}

fn bugs_path(project: &Path) -> std::path::PathBuf {
    crate::workspace::root(project).join("ctx/bugs.yaml")
}

fn corrections_path(project: &Path) -> std::path::PathBuf {
    crate::workspace::root(project).join("ctx/corrections.yaml")
}

// ── Commands ─────────────────────────────────────

fn cmd_init(project: &Path, do_scan: bool, skunkworks: bool) -> Result<()> {
    crate::workspace::ensure(project)?;
    if skunkworks {
        crate::workspace::set_skunkworks(project)?;
    }
    if do_scan {
        let store = scan::scan(project)?;
        crate::workspace::write_yaml(&anatomy_path(project), &store)?;
        scan::write_layered(project, &store)?;
        println!(
            "  ✓ .kazam/ctx/ initialized - {} files indexed",
            store.files.len()
        );
    } else {
        println!("  ✓ .kazam/ctx/ initialized");
    }
    if !crate::workspace::hooks_installed(project) {
        println!("  hint: run `kazam ctx hooks install` to wire up agent hooks");
        println!("        or `kazam workspace init` for the full setup");
    }
    Ok(())
}

fn cmd_scan(project: &Path, check: bool, json: bool) -> Result<()> {
    if check {
        let diff = scan::check(project)?;
        if json {
            json_ok(&diff);
        } else if diff.is_empty() {
            println!("  ✓ anatomy up to date");
        } else {
            if !diff.new_files.is_empty() {
                println!("  + {} new files", diff.new_files.len());
                for f in &diff.new_files {
                    println!("    {f}");
                }
            }
            if !diff.deleted_files.is_empty() {
                println!("  - {} deleted files", diff.deleted_files.len());
                for f in &diff.deleted_files {
                    println!("    {f}");
                }
            }
            if !diff.changed_files.is_empty() {
                println!("  ~ {} changed files", diff.changed_files.len());
                for f in &diff.changed_files {
                    println!("    {f}");
                }
            }
        }
    } else {
        let store = scan::scan(project)?;
        // Write flat store (for board.rs + describe + status)
        crate::workspace::write_yaml(&anatomy_path(project), &store)?;
        // Write layered summary (anatomy.tsv) + per-directory files (anatomy/<dir>.tsv)
        scan::write_layered(project, &store)?;
        if json {
            json_ok(&serde_json::json!({ "files": store.files.len() }));
        } else {
            println!("  ✓ scanned {} files", store.files.len());
        }
    }
    Ok(())
}

fn cmd_status(project: &Path, json: bool) -> Result<()> {
    let anatomy: AnatomyStore =
        crate::workspace::read_yaml(&anatomy_path(project)).unwrap_or(AnatomyStore {
            scanned: String::new(),
            files: vec![],
        });
    let learnings: LearningStore = crate::workspace::read_yaml(&learnings_path(project))
        .unwrap_or(LearningStore { learnings: vec![] });
    let bugs: BugStore =
        crate::workspace::read_yaml(&bugs_path(project)).unwrap_or(BugStore { bugs: vec![] });
    let corrections: CorrectionStore = crate::workspace::read_yaml(&corrections_path(project))
        .unwrap_or(CorrectionStore {
            corrections: vec![],
        });

    let status = CtxStatus {
        total_files: anatomy.files.len(),
        total_tokens: anatomy.files.iter().map(|f| f.tokens).sum(),
        total_reads: anatomy.files.iter().map(|f| f.reads as u64).sum(),
        learnings_count: learnings.learnings.len(),
        bugs_open: bugs.bugs.iter().filter(|b| b.resolved.is_none()).count(),
        bugs_resolved: bugs.bugs.iter().filter(|b| b.resolved.is_some()).count(),
        corrections_count: corrections.corrections.len(),
        last_scan: anatomy.scanned,
    };

    if json {
        json_ok(&status);
    } else {
        println!(
            "  files: {}  tokens: ~{}k",
            status.total_files,
            status.total_tokens / 1000
        );
        println!(
            "  reads: {}  learnings: {}",
            status.total_reads, status.learnings_count
        );
        println!(
            "  bugs: {} open / {} resolved",
            status.bugs_open, status.bugs_resolved
        );
        println!("  corrections: {}", status.corrections_count);
        if !status.last_scan.is_empty() {
            println!("  last scan: {}", status.last_scan);
        }
    }
    Ok(())
}

fn cmd_describe(project: &Path, file: &str, description: &str) -> Result<()> {
    let path = anatomy_path(project);
    let mut store: AnatomyStore = crate::workspace::read_yaml(&path).unwrap_or(AnatomyStore {
        scanned: String::new(),
        files: vec![],
    });

    let entry = store.files.iter_mut().find(|f| f.path == file);
    if let Some(entry) = entry {
        entry.description = Some(description.to_string());
        crate::workspace::write_yaml(&path, &store)?;

        // Also update the per-directory anatomy TSV file if one exists
        if let Some(slash_pos) = file.rfind('/') {
            let dir = &file[..slash_pos];
            let filename = format!("{}.tsv", dir.replace('/', "--"));
            let dir_file_path = crate::workspace::root(project)
                .join("ctx/anatomy")
                .join(&filename);
            if dir_file_path.exists() {
                let content = std::fs::read_to_string(&dir_file_path)
                    .with_context(|| format!("read {}", dir_file_path.display()))?;
                let mut new_lines: Vec<String> = Vec::new();
                let mut found = false;
                for line in content.lines() {
                    // Skip comment lines and header row unchanged
                    if line.starts_with('#') || line.starts_with("path\t") {
                        new_lines.push(line.to_string());
                        continue;
                    }
                    let mut cols: Vec<&str> = line.splitn(4, '\t').collect();
                    if !cols.is_empty() && cols[0] == file {
                        // Ensure we have at least 4 columns
                        while cols.len() < 4 {
                            cols.push("");
                        }
                        let safe_desc = description.replace('\t', "  ");
                        new_lines.push(format!(
                            "{}\t{}\t{}\t{}",
                            cols[0], cols[1], cols[2], safe_desc
                        ));
                        found = true;
                    } else {
                        new_lines.push(line.to_string());
                    }
                }
                if found {
                    let mut out = new_lines.join("\n");
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    let tmp = dir_file_path.with_extension("tsv.tmp");
                    std::fs::write(&tmp, &out)
                        .with_context(|| format!("write {}", tmp.display()))?;
                    std::fs::rename(&tmp, &dir_file_path)
                        .with_context(|| format!("rename to {}", dir_file_path.display()))?;
                }
            }
        }

        println!("  ✓ updated description for {file}");
    } else {
        bail!("file {file} not found in anatomy - run `kazam ctx scan` first");
    }
    Ok(())
}

fn cmd_learn(project: &Path, text: String, category: &str, json: bool) -> Result<()> {
    crate::workspace::ensure(project)?;
    let cat: LearningCategory = category.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    let learning = Learning {
        id: crate::id::generate(),
        text: text.clone(),
        category: cat,
        created: now(),
    };

    let path = learnings_path(project);
    let mut store: LearningStore =
        crate::workspace::read_yaml(&path).unwrap_or(LearningStore { learnings: vec![] });
    store.learnings.push(learning.clone());
    crate::workspace::write_yaml(&path, &store)?;

    if json {
        json_ok(&learning);
    } else {
        println!("  ✓ learned [{}]: {text}", cat.label());
    }
    Ok(())
}

fn cmd_learnings(project: &Path, json: bool) -> Result<()> {
    let store: LearningStore = crate::workspace::read_yaml(&learnings_path(project))
        .unwrap_or(LearningStore { learnings: vec![] });

    if json {
        json_ok(&store.learnings);
    } else if store.learnings.is_empty() {
        println!("  no learnings recorded");
    } else {
        for l in &store.learnings {
            println!("  [{}] {} - {}", l.category.label(), l.id, l.text);
        }
    }
    Ok(())
}

fn cmd_bug(project: &Path, symptom: String, file: Option<String>, json: bool) -> Result<()> {
    crate::workspace::ensure(project)?;
    let bug = BugEntry {
        id: crate::id::generate(),
        symptom: symptom.clone(),
        file_path: file,
        resolution: None,
        created: now(),
        resolved: None,
    };

    let path = bugs_path(project);
    let mut store: BugStore =
        crate::workspace::read_yaml(&path).unwrap_or(BugStore { bugs: vec![] });
    store.bugs.push(bug.clone());
    crate::workspace::write_yaml(&path, &store)?;

    if json {
        json_ok(&bug);
    } else {
        println!("  ✓ bug {} recorded: {symptom}", bug.id);
    }
    Ok(())
}

fn cmd_bugs(project: &Path, file: Option<String>, json: bool) -> Result<()> {
    let store: BugStore =
        crate::workspace::read_yaml(&bugs_path(project)).unwrap_or(BugStore { bugs: vec![] });

    let bugs: Vec<&BugEntry> = if let Some(ref f) = file {
        store
            .bugs
            .iter()
            .filter(|b| b.file_path.as_deref() == Some(f.as_str()))
            .collect()
    } else {
        store.bugs.iter().collect()
    };

    if json {
        json_ok(&bugs);
    } else if bugs.is_empty() {
        println!("  no bugs recorded");
    } else {
        for b in &bugs {
            let status = if b.resolved.is_some() { "✓" } else { "○" };
            let file_str = b
                .file_path
                .as_deref()
                .map(|f| format!(" [{f}]"))
                .unwrap_or_default();
            println!("  {status} {}{file_str} - {}", b.id, b.symptom);
            if let Some(ref fix) = b.resolution {
                println!("    fix: {fix}");
            }
        }
    }
    Ok(())
}

fn cmd_correction(
    project: &Path,
    mistake: String,
    correction: String,
    file: Option<String>,
    json: bool,
) -> Result<()> {
    crate::workspace::ensure(project)?;
    let entry = Correction {
        id: crate::id::generate(),
        mistake: mistake.clone(),
        correction: correction.clone(),
        file_path: file,
        created: now(),
    };

    let path = corrections_path(project);
    let mut store: CorrectionStore =
        crate::workspace::read_yaml(&path).unwrap_or(CorrectionStore {
            corrections: vec![],
        });
    store.corrections.push(entry.clone());
    crate::workspace::write_yaml(&path, &store)?;

    if json {
        json_ok(&entry);
    } else {
        println!("  ✓ correction {} recorded: {mistake}", entry.id);
    }
    Ok(())
}

fn cmd_corrections(project: &Path, json: bool) -> Result<()> {
    let store: CorrectionStore =
        crate::workspace::read_yaml(&corrections_path(project)).unwrap_or(CorrectionStore {
            corrections: vec![],
        });

    if json {
        json_ok(&store.corrections);
    } else if store.corrections.is_empty() {
        println!("  no corrections recorded");
    } else {
        for c in &store.corrections {
            let file_str = c
                .file_path
                .as_deref()
                .map(|f| format!(" [{f}]"))
                .unwrap_or_default();
            println!("  {} {}{file_str}", c.id, c.mistake);
            println!("    → {}", c.correction);
        }
    }
    Ok(())
}

fn cmd_consolidate(project: &Path, days: u32, json: bool) -> Result<()> {
    let cutoff = chrono::Local::now() - chrono::Duration::days(days as i64);
    let cutoff_str = cutoff.to_rfc3339();

    // Clean resolved bugs older than cutoff
    let bugs_p = bugs_path(project);
    let mut bugs: BugStore =
        crate::workspace::read_yaml(&bugs_p).unwrap_or(BugStore { bugs: vec![] });
    let before_bugs = bugs.bugs.len();
    bugs.bugs.retain(|b| {
        b.resolved
            .as_ref()
            .is_none_or(|r| r.as_str() > cutoff_str.as_str())
    });
    let removed_bugs = before_bugs - bugs.bugs.len();
    crate::workspace::write_yaml(&bugs_p, &bugs)?;

    // Deduplicate learnings
    let learn_p = learnings_path(project);
    let mut learnings: LearningStore =
        crate::workspace::read_yaml(&learn_p).unwrap_or(LearningStore { learnings: vec![] });
    let before_learn = learnings.learnings.len();
    let mut seen = std::collections::HashSet::new();
    learnings.learnings.retain(|l| seen.insert(l.text.clone()));
    let deduped_learn = before_learn - learnings.learnings.len();
    crate::workspace::write_yaml(&learn_p, &learnings)?;

    if json {
        json_ok(&serde_json::json!({
            "removed_bugs": removed_bugs,
            "deduped_learnings": deduped_learn,
        }));
    } else {
        println!(
            "  ✓ consolidated: removed {removed_bugs} resolved bugs, deduped {deduped_learn} learnings"
        );
    }
    Ok(())
}

fn cmd_resolve(project: &Path, id: &str, fix: &str, json: bool) -> Result<()> {
    let path = bugs_path(project);
    let mut store: BugStore =
        crate::workspace::read_yaml(&path).unwrap_or(BugStore { bugs: vec![] });

    let bug = store.bugs.iter_mut().find(|b| b.id == id);
    let Some(bug) = bug else {
        if json {
            json_err(&format!("bug {id} not found"));
        } else {
            bail!("bug {id} not found");
        }
        return Ok(());
    };

    bug.resolution = Some(fix.to_string());
    bug.resolved = Some(now());
    let bug_out = bug.clone();
    crate::workspace::write_yaml(&path, &store)?;

    if json {
        json_ok(&bug_out);
    } else {
        println!("  ✓ resolved {id}: {fix}");
    }
    Ok(())
}

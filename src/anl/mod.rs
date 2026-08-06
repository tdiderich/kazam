//! Agent Native Language (`.anl`): a dense, declarative DSL that turns a
//! task into a static directed graph with mandatory invariants, so an LLM
//! agent runs inside deterministic execution boundaries instead of free-form
//! natural-language instructions.

pub mod ast;
pub mod compiler;
pub mod lexer;
pub mod parser;
pub mod validator;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum Command {
    /// Validate an .anl spec: parse it, then run the static graph analyzer
    /// (reachability, terminal completeness, branch integrity, invariants).
    Validate {
        /// Path to the .anl spec file
        path: PathBuf,
        /// Emit machine-readable JSON instead of the human-readable report
        #[arg(long)]
        json: bool,
    },
    /// Compile an .anl spec into a token-dense agent system-prompt block
    Export {
        /// Path to the .anl spec file
        path: PathBuf,
        /// Output format (currently only "prompt" is supported)
        #[arg(long, default_value = "prompt")]
        format: String,
        /// Write to this file instead of stdout
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Validate { path, json } => run_validate(&path, json),
        Command::Export { path, format, out } => run_export(&path, &format, out.as_deref()),
    }
}

fn read_source(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

fn run_validate(path: &Path, json: bool) -> Result<()> {
    let src = read_source(path)?;
    let parsed = match parser::parse(&src) {
        Ok(parsed) => parsed,
        Err(e) => {
            if json {
                let obj = serde_json::json!({
                    "valid": false,
                    "parse_error": { "message": e.message, "line": e.line, "col": e.col },
                });
                println!("{}", serde_json::to_string_pretty(&obj)?);
            } else {
                println!("{}: parse error", path.display());
                println!("  {e}");
            }
            std::process::exit(1);
        }
    };

    let diags = validator::validate(&parsed.spec, &parsed.state_lines);
    let has_errors = validator::has_errors(&diags);

    if json {
        let json_diags: Vec<_> = diags
            .iter()
            .map(|d| {
                let severity = match d.severity {
                    validator::Severity::Error => "error",
                    validator::Severity::Warning => "warning",
                };
                serde_json::json!({
                    "severity": severity,
                    "code": d.code,
                    "message": d.message,
                    "location": d.location,
                    "line": d.line,
                })
            })
            .collect();
        let obj = serde_json::json!({ "valid": !has_errors, "diagnostics": json_diags });
        println!("{}", serde_json::to_string_pretty(&obj)?);
    } else {
        println!("{}", path.display());
        println!("{}", validator::format_pretty(&diags));
    }

    if has_errors {
        std::process::exit(1);
    }
    Ok(())
}

fn run_export(path: &Path, format: &str, out: Option<&Path>) -> Result<()> {
    if format != "prompt" {
        bail!("unsupported export format '{format}' (only 'prompt' is supported)");
    }
    let src = read_source(path)?;
    let parsed = parser::parse(&src).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    let rendered = compiler::to_prompt(&parsed.spec);

    match out {
        Some(out_path) => {
            std::fs::write(out_path, &rendered)
                .with_context(|| format!("failed to write {}", out_path.display()))?;
        }
        None => print!("{rendered}"),
    }
    Ok(())
}

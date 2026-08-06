//! Agent Graph Language (`.agl`): a dense, declarative DSL that turns a
//! task into a static directed graph with mandatory invariants, so an LLM
//! agent runs inside deterministic execution boundaries instead of free-form
//! natural-language instructions.

pub mod ast;
pub mod compiler;
pub mod lexer;
pub mod parser;
pub mod resolver;
pub mod skill;
pub mod validator;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum Command {
    /// Validate an .agl spec: parse it, resolve its imports, then run the
    /// static graph analyzer (reachability, terminal completeness, branch
    /// integrity, and invariant soundness).
    Validate {
        /// Path to the .agl spec file, or a bare name resolved against
        /// ~/.kazam/agl/specs/<name>.agl
        path: PathBuf,
        /// Emit machine-readable JSON instead of the human-readable report
        #[arg(long)]
        json: bool,
        /// Optional flat JSON array of dotted `Server.method` tool names.
        /// When given, warns about any call()/map() function in the flow
        /// that isn't listed. This is a name-existence check only, not
        /// schema validation — the manifest is hand-maintained and has no
        /// notion of a server's actual tool/argument schema. Omit this
        /// flag for zero behavior change.
        #[arg(long)]
        tools: Option<PathBuf>,
    },
    /// Compile an .agl spec into a token-dense agent system-prompt block
    Export {
        /// Path to the .agl spec file, or a bare name resolved against
        /// ~/.kazam/agl/specs/<name>.agl
        path: PathBuf,
        /// Output format (currently only "prompt" is supported)
        #[arg(long, default_value = "prompt")]
        format: String,
        /// Write to this file instead of stdout
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Print a top-to-bottom ASCII rendering of a spec's flow — states,
    /// actions, and transitions, with branches fanned out underneath the
    /// state that owns them. A plan preview, not the graph's source syntax.
    Flow {
        /// Path to the .agl spec file, or a bare name resolved against
        /// ~/.kazam/agl/specs/<name>.agl
        path: PathBuf,
    },
    /// Compile a validated .agl spec (imports resolved) into a portable
    /// skill document for an LLM coding tool
    Skill {
        /// Path to the .agl spec file, or a bare name resolved against
        /// ~/.kazam/agl/specs/<name>.agl
        path: PathBuf,
        /// Which tool's skill format to render
        #[arg(long)]
        target: skill::Target,
        /// Write to this file (or into this directory, as <name>.md)
        /// instead of stdout
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Validate { path, json, tools } => run_validate(&path, json, tools.as_deref()),
        Command::Export { path, format, out } => run_export(&path, &format, out.as_deref()),
        Command::Flow { path } => run_flow(&path),
        Command::Skill { path, target, out } => run_skill(&path, target, out.as_deref()),
    }
}

fn read_source(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

fn home_dir() -> Result<PathBuf> {
    match std::env::var_os("HOME") {
        Some(h) => Ok(PathBuf::from(h)),
        None => bail!("HOME is not set — cannot resolve a ~/.kazam/agl spec name"),
    }
}

/// The `~/.kazam/agl` hub convention (documented in CONTRIBUTING.md):
/// `~/.kazam/agl/specs/*.agl` holds authored specs, `~/.kazam/agl/shared/*.agl`
/// holds importable fragments. A bare name with no `/` and no `.agl`
/// extension is a convenience shorthand for `~/.kazam/agl/specs/<name>.agl`;
/// anything else (an actual path) is used as-is.
fn resolve_spec_path(path: &Path) -> Result<PathBuf> {
    let raw = path.to_string_lossy();
    if !raw.is_empty() && !raw.contains('/') && !raw.ends_with(".agl") {
        return Ok(home_dir()?
            .join(".kazam")
            .join("agl")
            .join("specs")
            .join(format!("{raw}.agl")));
    }
    Ok(path.to_path_buf())
}

/// Parse the spec at `path` and resolve its `import` lines, extending
/// `spec.invariants` with everything they transitively pull in. Shared by
/// `export` and `skill` so an import's invariants are always in force on
/// every path that eventually renders a spec. `validate` inlines the same
/// two steps itself, since it needs to distinguish a parse error from a
/// resolution error in its `--json` output.
fn load_spec(path: &Path) -> Result<parser::Parsed> {
    let src = read_source(path)?;
    let mut parsed = parser::parse(&src).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    let extra = resolver::resolve_imports(path, &parsed.imports)
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    parsed.spec.invariants.extend(extra);
    Ok(parsed)
}

fn load_tool_manifest(path: &Path) -> Result<HashSet<String>> {
    let src = read_source(path)?;
    let names: Vec<String> = serde_json::from_str(&src)
        .with_context(|| format!("{}: expected a flat JSON array of strings", path.display()))?;
    Ok(names.into_iter().collect())
}

fn run_validate(path: &Path, json: bool, tools: Option<&Path>) -> Result<()> {
    let resolved_path = resolve_spec_path(path)?;
    let src = read_source(&resolved_path)?;
    let mut parsed = match parser::parse(&src) {
        Ok(parsed) => parsed,
        Err(e) => {
            if json {
                let obj = serde_json::json!({
                    "valid": false,
                    "parse_error": { "message": e.message, "line": e.line, "col": e.col },
                });
                println!("{}", serde_json::to_string_pretty(&obj)?);
            } else {
                println!("{}: parse error", resolved_path.display());
                println!("  {e}");
            }
            std::process::exit(1);
        }
    };

    match resolver::resolve_imports(&resolved_path, &parsed.imports) {
        Ok(extra) => parsed.spec.invariants.extend(extra),
        Err(e) => {
            if json {
                let obj = serde_json::json!({
                    "valid": false,
                    "resolution_error": e.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&obj)?);
            } else {
                println!("{}: import resolution error", resolved_path.display());
                println!("  {e}");
            }
            std::process::exit(1);
        }
    }

    let mut diags = validator::validate(&parsed.spec, &parsed.state_lines);
    if let Some(tools_path) = tools {
        let manifest = load_tool_manifest(tools_path)?;
        diags.extend(validator::check_tool_bindings(
            &parsed.spec,
            &manifest,
            &parsed.state_lines,
        ));
    }
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
        println!("{}", resolved_path.display());
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
    let resolved_path = resolve_spec_path(path)?;
    let parsed = load_spec(&resolved_path)?;
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

fn run_flow(path: &Path) -> Result<()> {
    let resolved_path = resolve_spec_path(path)?;
    let parsed = load_spec(&resolved_path)?;
    print!("{}", skill::render_ascii_flow(&parsed.spec));
    Ok(())
}

fn run_skill(path: &Path, target: skill::Target, out: Option<&Path>) -> Result<()> {
    let resolved_path = resolve_spec_path(path)?;
    let parsed = load_spec(&resolved_path)?;

    let diags = validator::validate(&parsed.spec, &parsed.state_lines);
    if validator::has_errors(&diags) {
        println!("{}", validator::format_pretty(&diags));
        bail!(
            "{} is not valid — fix the errors above before compiling a skill \
             (run `kazam agl validate {}` for the full report)",
            resolved_path.display(),
            path.display()
        );
    }

    let rendered = skill::render(&parsed.spec, target);

    match out {
        Some(out_path) => {
            let dest = if out_path.is_dir() {
                out_path.join(format!("{}.md", skill::kebab_case(&parsed.spec.name)))
            } else {
                out_path.to_path_buf()
            };
            std::fs::write(&dest, &rendered)
                .with_context(|| format!("failed to write {}", dest.display()))?;
        }
        None => print!("{rendered}"),
    }
    Ok(())
}

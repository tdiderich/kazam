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

/// Where `kazam agl load` installs skills when `--out` isn't given explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Scope {
    /// `~/.claude/skills/` (and `~/.claude/agents/` with --isolated) - every
    /// session on this machine sees them, regardless of which repo it's in.
    User,
    /// `.claude/skills/` under the current directory - only sessions started
    /// in this repo see them. Use this for skills meant to travel with the
    /// repo itself (checked in, shared with a team).
    Repo,
}

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
    /// Compile every spec in ~/.kazam/agl/specs/ into a Claude Code
    /// subagent + a thin dispatcher skill in the target project.
    /// Cursor/Codex aren't wired up here yet — use `kazam agl skill
    /// --target cursor|codex` one spec at a time until they are.
    Load {
        /// Install to the user's global ~/.claude, or the current repo's
        /// .claude. Ignored if --out is given explicitly.
        #[arg(long, value_enum, default_value = "user")]
        scope: Scope,
        /// Explicit project directory to write .claude/skills/ (and, with
        /// --isolated, .claude/agents/) into. Overrides --scope.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Compile a tool-scoped subagent + a thin dispatcher skill instead
        /// of the inline default. Use this when a graph genuinely needs
        /// isolation - a harder tool boundary than the invoking session
        /// has, a background/parallel run - not for anything that gates on
        /// approval from whoever's already in the conversation: a subagent
        /// can't verify that a relayed "approved" really came from a human,
        /// only the inline default can, because it runs as this
        /// conversation instead of a separate one.
        #[arg(long)]
        isolated: bool,
    },
    /// Bring an existing ~/.kazam/agl/cache/<name>.jsonl file's lines up to
    /// a cache block's current declared fields. Adds a type-appropriate
    /// default (empty string, 0, false, []) for any field a line is
    /// missing; never removes or otherwise touches fields already present.
    CacheMigrate {
        /// Path to the .agl spec (or fragment) declaring the cache block,
        /// or a bare name resolved against ~/.kazam/agl/specs/<name>.agl
        path: PathBuf,
        /// Which declared cache block to migrate, when the spec declares
        /// more than one. Required only in that case.
        #[arg(long)]
        name: Option<String>,
    },
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Validate { path, json, tools } => run_validate(&path, json, tools.as_deref()),
        Command::Export { path, format, out } => run_export(&path, &format, out.as_deref()),
        Command::Flow { path } => run_flow(&path),
        Command::Skill { path, target, out } => run_skill(&path, target, out.as_deref()),
        Command::Load {
            scope,
            out,
            isolated,
        } => run_load(scope, out.as_deref(), isolated),
        Command::CacheMigrate { path, name } => run_cache_migrate(&path, name.as_deref()),
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

/// Merge an import resolution's invariants and cache blocks into `parsed`.
/// A cache block pulled in via import that collides by name with one the
/// spec declared inline (or with another import) is the same conflict
/// `resolver::merge_cache_block` already checks - reused here so inline and
/// imported cache blocks go through identical conflict handling.
fn merge_resolved(parsed: &mut parser::Parsed, resolved: resolver::ResolvedImports) -> Result<()> {
    parsed.spec.invariants.extend(resolved.invariants);
    for block in resolved.cache {
        resolver::merge_cache_block(&mut parsed.spec.cache, block)?;
    }
    Ok(())
}

/// Parse the spec at `path` and resolve its `import` lines, extending
/// `spec.invariants`/`spec.cache` with everything they transitively pull
/// in. Shared by `export` and `skill` so an import's invariants and cache
/// blocks are always in force on every path that eventually renders a
/// spec. `validate` inlines the same two steps itself, since it needs to
/// distinguish a parse error from a resolution error in its `--json` output.
fn load_spec(path: &Path) -> Result<parser::Parsed> {
    let src = read_source(path)?;
    let mut parsed = parser::parse(&src).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    let resolved = resolver::resolve_imports(path, &parsed.imports)
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
    merge_resolved(&mut parsed, resolved)
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
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

    let resolution = resolver::resolve_imports(&resolved_path, &parsed.imports)
        .and_then(|resolved| merge_resolved(&mut parsed, resolved));
    if let Err(e) = resolution {
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

/// Every `~/.kazam/agl/templates/<name>.md` a state's `evaluate(...)` text
/// actually names, as `(name, file content)`. `skill::referenced_template_names`
/// returns every distinct word across all `evaluate(...)` expressions - a
/// superset, most of which aren't template names at all - so this is the
/// filesystem-check step that narrows it down to real files, keeping the
/// pure-string rendering in `skill.rs` free of filesystem I/O.
fn resolve_referenced_templates(spec: &ast::AglSpec) -> Result<Vec<(String, String)>> {
    let templates_dir = home_dir()?.join(".kazam").join("agl").join("templates");
    let mut found = Vec::new();
    for word in skill::referenced_template_names(spec) {
        let candidate = templates_dir.join(format!("{word}.md"));
        if candidate.is_file() {
            let content = std::fs::read_to_string(&candidate)
                .with_context(|| format!("failed to read {}", candidate.display()))?;
            found.push((word, content));
        }
    }
    Ok(found)
}

/// Every `spec_name` a `fan()` state names that has no corresponding
/// `~/.kazam/agl/specs/<kebab-name>.agl` file. Unlike a missing template
/// (silently fine, evaluate() just runs without boilerplate) a missing fan
/// target means the graph names a step that can't run at all - worth a
/// warning at `load` time, but not a hard failure: the spec might still be
/// mid-authoring, and `validate`/`skill` on this spec alone already caught
/// every error that actually blocks compiling it.
fn missing_fan_specs(spec: &ast::AglSpec, specs_dir: &Path) -> Vec<String> {
    skill::referenced_fan_specs(spec)
        .into_iter()
        .filter(|name| {
            let candidate = specs_dir.join(format!("{}.agl", skill::kebab_case(name)));
            !candidate.is_file()
        })
        .collect()
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

    let templates = resolve_referenced_templates(&parsed.spec)?;
    let rendered = skill::render(&parsed.spec, target, &templates);

    match out {
        Some(out_path) => {
            let dest = if out_path.is_dir() {
                let name = skill::resolved_skill_name(&parsed.spec);
                if matches!(target, skill::Target::Claude) {
                    // Claude Code only discovers skills shaped `<name>/SKILL.md`,
                    // never a flat `<name>.md` sitting in the skills root.
                    let skill_dir = out_path.join(&name);
                    std::fs::create_dir_all(&skill_dir)
                        .with_context(|| format!("failed to create {}", skill_dir.display()))?;
                    skill_dir.join("SKILL.md")
                } else {
                    // Cursor/Codex targets are a block meant to be appended into
                    // an existing `.cursorrules`/`AGENTS.md`, not a discovered
                    // skill directory - a flat file is the right shape here.
                    out_path.join(format!("{name}.md"))
                }
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

/// Compiles every `~/.kazam/agl/specs/*.agl` into `.claude/skills/<name>.md`
/// under `out` (default: the current directory). Default is inline: the
/// skill embeds the full graph (primer, preflight, flow, resolved source)
/// and runs directly in whatever session invokes it, so a `gate(...)`
/// checks approval against the human actually in that conversation.
///
/// `--isolated` compiles a tool-scoped `.claude/agents/<name>.md` subagent
/// plus a thin dispatcher skill instead - but refuses any spec with a
/// gate-protected write (`validator::has_gate_protected_writes`), since a
/// subagent has no way to verify a relayed "approved" came from a real
/// human rather than the orchestrating agent's own paraphrase. Read-only
/// specs (no writes an invariant cares about) compile to `--isolated` fine.
///
/// A spec that fails to parse, resolve, or validate is skipped with its
/// error printed rather than aborting the whole batch - one bad spec in
/// the hub shouldn't block loading the rest.
fn run_load(scope: Scope, out: Option<&Path>, isolated: bool) -> Result<()> {
    let project_root: PathBuf = match out {
        Some(explicit) => explicit.to_path_buf(),
        None => match scope {
            Scope::User => home_dir()?,
            Scope::Repo => PathBuf::from("."),
        },
    };
    let specs_dir = home_dir()?.join(".kazam").join("agl").join("specs");
    if !specs_dir.is_dir() {
        bail!(
            "no specs found at {} - nothing to load",
            specs_dir.display()
        );
    }

    let skills_dir = project_root.join(".claude").join("skills");
    std::fs::create_dir_all(&skills_dir)
        .with_context(|| format!("failed to create {}", skills_dir.display()))?;
    let agents_dir = project_root.join(".claude").join("agents");
    if isolated {
        std::fs::create_dir_all(&agents_dir)
            .with_context(|| format!("failed to create {}", agents_dir.display()))?;
    }

    let mut spec_paths: Vec<PathBuf> = std::fs::read_dir(&specs_dir)
        .with_context(|| format!("failed to read {}", specs_dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|ext| ext.to_str()) == Some("agl"))
        .collect();
    spec_paths.sort();

    let mut loaded: Vec<(String, Option<PathBuf>, PathBuf)> = Vec::new();
    let mut skipped: Vec<(PathBuf, String)> = Vec::new();
    let mut warnings: Vec<(PathBuf, Vec<String>)> = Vec::new();

    for spec_path in spec_paths {
        let outcome = load_spec(&spec_path).map(|parsed| {
            let diags = validator::validate(&parsed.spec, &parsed.state_lines);
            (parsed, diags)
        });
        match outcome {
            Ok((parsed, diags)) if !validator::has_errors(&diags) => {
                let name = skill::resolved_skill_name(&parsed.spec);
                // Claude Code only discovers skills shaped `<name>/SKILL.md`,
                // never a flat `<name>.md` sitting in the skills root.
                let skill_dir = skills_dir.join(&name);
                std::fs::create_dir_all(&skill_dir)
                    .with_context(|| format!("failed to create {}", skill_dir.display()))?;
                let skill_path = skill_dir.join("SKILL.md");
                let templates = resolve_referenced_templates(&parsed.spec)?;
                let missing_fans = missing_fan_specs(&parsed.spec, &specs_dir);
                if !missing_fans.is_empty() {
                    warnings.push((
                        spec_path.clone(),
                        missing_fans
                            .iter()
                            .map(|n| {
                                format!(
                                    "fan() names '{n}', no {}.agl found in {}",
                                    skill::kebab_case(n),
                                    specs_dir.display()
                                )
                            })
                            .collect(),
                    ));
                }

                if isolated {
                    if validator::has_gate_protected_writes(&parsed.spec) {
                        skipped.push((
                            spec_path,
                            "has at least one write protected by gate(human_approval) - \
                             refusing to compile to --isolated (subagent) mode, since a \
                             subagent can't verify a relayed approval came from a real \
                             human. Run `kazam agl load` without --isolated for this spec."
                                .to_string(),
                        ));
                        continue;
                    }
                    let agent_path = agents_dir.join(format!("{name}.md"));
                    std::fs::write(
                        &agent_path,
                        skill::render_agent_file(&parsed.spec, &templates),
                    )
                    .with_context(|| format!("failed to write {}", agent_path.display()))?;
                    std::fs::write(&skill_path, skill::render_skill_dispatcher(&parsed.spec))
                        .with_context(|| format!("failed to write {}", skill_path.display()))?;
                    loaded.push((name, Some(agent_path), skill_path));
                } else {
                    let rendered = skill::render(&parsed.spec, skill::Target::Claude, &templates);
                    std::fs::write(&skill_path, rendered)
                        .with_context(|| format!("failed to write {}", skill_path.display()))?;
                    loaded.push((name, None, skill_path));
                }
            }
            Ok((_, diags)) => skipped.push((spec_path, validator::format_pretty(&diags))),
            Err(e) => skipped.push((spec_path, e.to_string())),
        }
    }

    for (name, agent_path, skill_path) in &loaded {
        println!("loaded {name}:");
        if let Some(agent_path) = agent_path {
            println!("  {}", agent_path.display());
        }
        println!("  {}", skill_path.display());
    }
    if !warnings.is_empty() {
        println!("\nwarnings on {} spec(s):", warnings.len());
        for (path, lines) in &warnings {
            println!("  {}", path.display());
            for line in lines {
                println!("    {line}");
            }
        }
    }
    if !skipped.is_empty() {
        println!("\nskipped {} spec(s):", skipped.len());
        for (path, reason) in &skipped {
            println!("  {}", path.display());
            for line in reason.lines() {
                println!("    {line}");
            }
        }
    }
    if loaded.is_empty() {
        bail!("no valid specs loaded");
    }
    Ok(())
}

/// A type-appropriate placeholder for a cache field a JSONL line predates -
/// `null` for `Custom`, since there's no real spelling of "empty" for a
/// type this generic outside the schema author's own convention.
fn cache_default_for_type(dt: &ast::DataType) -> serde_json::Value {
    match dt {
        ast::DataType::String => serde_json::Value::String(String::new()),
        ast::DataType::Int => serde_json::Value::Number(0.into()),
        ast::DataType::Bool => serde_json::Value::Bool(false),
        ast::DataType::List(_) => serde_json::Value::Array(Vec::new()),
        ast::DataType::Custom(_) => serde_json::Value::Null,
    }
}

/// Brings `~/.kazam/agl/cache/<name>.jsonl` up to a cache block's current
/// declared fields: every existing line gets any field it's missing added
/// with a type-appropriate default. Fields already present, and the file's
/// line order, are never touched - this only ever adds, never removes or
/// reorders.
fn run_cache_migrate(path: &Path, name: Option<&str>) -> Result<()> {
    let resolved_path = resolve_spec_path(path)?;
    let parsed = load_spec(&resolved_path)?;

    let block = match (parsed.spec.cache.as_slice(), name) {
        ([], _) => bail!(
            "{} declares no cache - nothing to migrate",
            resolved_path.display()
        ),
        ([only], None) => only,
        (many, None) => bail!(
            "{} declares {} caches ({}) - pass --name to pick one",
            resolved_path.display(),
            many.len(),
            many.iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        (many, Some(want)) => many.iter().find(|b| b.name == want).with_context(|| {
            format!(
                "{} has no cache named '{want}' - declared: {}",
                resolved_path.display(),
                many.iter()
                    .map(|b| b.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?,
    };

    let cache_path = home_dir()?
        .join(".kazam")
        .join("agl")
        .join("cache")
        .join(format!("{}.jsonl", block.name));
    if !cache_path.is_file() {
        println!(
            "no cache file yet at {} - nothing to migrate",
            cache_path.display()
        );
        return Ok(());
    }

    let content = std::fs::read_to_string(&cache_path)
        .with_context(|| format!("failed to read {}", cache_path.display()))?;
    let mut migrated_lines = Vec::new();
    let mut changed = 0usize;
    let mut total = 0usize;
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        total += 1;
        let mut obj: serde_json::Map<String, serde_json::Value> = serde_json::from_str(line)
            .with_context(|| format!("{}: not a JSON object: {line:?}", cache_path.display()))?;
        let mut line_changed = false;
        for field in &block.fields {
            if !obj.contains_key(&field.name) {
                obj.insert(field.name.clone(), cache_default_for_type(&field.data_type));
                line_changed = true;
            }
        }
        if line_changed {
            changed += 1;
        }
        migrated_lines.push(serde_json::to_string(&obj)?);
    }

    if changed == 0 {
        println!(
            "{} already matches the declared fields for '{}' - nothing to migrate",
            cache_path.display(),
            block.name
        );
        return Ok(());
    }

    let mut out = migrated_lines.join("\n");
    out.push('\n');
    std::fs::write(&cache_path, out)
        .with_context(|| format!("failed to write {}", cache_path.display()))?;
    println!(
        "migrated {changed} of {total} line(s) in {}",
        cache_path.display()
    );
    Ok(())
}

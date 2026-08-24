//! `kazam connect` - reads `.map.yaml` connector files and executes them:
//! pull data from vendor APIs, transform records, aggregate, and output to
//! curata (or the terminal/a file). See `connectors/CONNECT_SPEC.md`.

pub mod aggregate;
pub mod config;
pub mod expr;
pub mod output;
pub mod pull;
pub mod transform;
pub mod types;

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use config::{ConnectorEnv, State};
use types::MappingFile;

#[derive(Subcommand)]
pub enum ConnectCommand {
    /// Execute a connector's mapping: pull, transform, aggregate, output
    Run {
        /// Connector/vendor name (matches connectors/<vendor>/)
        vendor: String,
        /// Pull + transform + aggregate and print results without writing
        #[arg(long)]
        dry_run: bool,
        /// Override the mapping file's output target (curata|terminal|both|file)
        #[arg(long)]
        target: Option<String>,
        /// Write even if content is unchanged since the last sync
        #[arg(long)]
        force: bool,
    },
    /// Show connector status: last sync, pull counts, page state
    Status {
        /// Show detailed state for a single connector
        vendor: Option<String>,
    },
}

pub fn run(command: ConnectCommand, dir: &Path) -> Result<()> {
    match command {
        ConnectCommand::Run {
            vendor,
            dry_run,
            target,
            force,
        } => run_vendor(dir, &vendor, dry_run, target, force),
        ConnectCommand::Status { vendor } => match vendor {
            Some(v) => status_one(dir, &v),
            None => status_all(dir),
        },
    }
}

fn connectors_root(dir: &Path) -> PathBuf {
    dir.join("connectors")
}

fn find_mapping_file(connector_dir: &Path) -> Result<PathBuf> {
    let entries = std::fs::read_dir(connector_dir)
        .with_context(|| format!("no such connector directory: {}", connector_dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_map_yaml = path
            .file_name()
            .and_then(|f| f.to_str())
            .map(|f| f.ends_with(".map.yaml"))
            .unwrap_or(false);
        if is_map_yaml {
            return Ok(path);
        }
    }
    bail!("no *.map.yaml mapping file found in {}", connector_dir.display());
}

fn load_mapping(path: &Path) -> Result<MappingFile> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read mapping file: {}", path.display()))?;
    serde_yaml::from_str(&content).with_context(|| format!("failed to parse mapping file: {}", path.display()))
}

fn run_vendor(dir: &Path, vendor: &str, dry_run: bool, target_override: Option<String>, force: bool) -> Result<()> {
    let connector_dir = connectors_root(dir).join(vendor);
    let mapping_path = find_mapping_file(&connector_dir)?;
    let mapping = load_mapping(&mapping_path)?;

    let host = config::load_host_config();
    let env = ConnectorEnv::load(&connector_dir);
    let state = State::load(&connector_dir);

    println!("kazam connect: running '{}' ({})", mapping.mapping, mapping_path.display());

    let resolved_base = env.resolve(&mapping.source.base_url, &host)?;
    let auth_desc = match &mapping.source.auth {
        types::Auth::Bearer { .. } => "bearer token",
        types::Auth::ApiKey { header, .. } => header.as_str(),
        types::Auth::Oauth2 { .. } => "oauth2 client credentials",
    };
    let prev_base = state.confirmed_base_url.as_deref();
    if prev_base != Some(resolved_base.as_str()) {
        eprintln!("  target: {}", resolved_base);
        eprintln!("  auth: {}", auth_desc);
        if !dry_run {
            eprint!("  first run or base_url changed. continue? [y/N] ");
            let mut answer = String::new();
            std::io::stdin().read_line(&mut answer)?;
            if !answer.trim().eq_ignore_ascii_case("y") {
                bail!("aborted by user");
            }
        }
    }

    let mut pull_results: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    let mut pull_counts: HashMap<String, usize> = HashMap::new();

    for (name, pull) in &mapping.pulls {
        println!("  pulling '{}' ...", name);
        match pull::execute_pull(name, pull, &mapping.source, &env, &host, &state) {
            Ok(outcome) => {
                println!("    {} record(s)", outcome.records.len());
                pull_counts.insert(name.clone(), outcome.records.len());
                pull_results.insert(name.clone(), outcome.records);
            }
            Err(e) => {
                eprintln!("    pull '{}' failed: {:#}", name, e);
                if !dry_run {
                    return Err(e).context(format!("pull '{}' failed", name));
                }
                pull_results.insert(name.clone(), Vec::new());
            }
        }
    }

    let mut shape_results = HashMap::new();
    for (name, shape) in &mapping.shapes {
        let Some(rows) = pull_results.get(&shape.pull) else {
            eprintln!("  shape '{}' references unknown pull '{}' - skipping", name, shape.pull);
            continue;
        };
        match aggregate::run_aggregate(rows.clone(), &shape.aggregate) {
            Ok(state) => {
                shape_results.insert(name.clone(), state);
            }
            Err(e) => eprintln!("  shape '{}' aggregation failed: {:#}", name, e),
        }
    }

    let target = target_override.unwrap_or_else(|| mapping.output.target.clone());
    output::render(&mapping, &shape_results, &target, dry_run, force, &connector_dir, &host)?;

    if !dry_run {
        let mut new_state = state;
        new_state.last_sync = Some(chrono::Utc::now().to_rfc3339());
        new_state.pull_counts = pull_counts;
        new_state.confirmed_base_url = Some(resolved_base);
        new_state.save(&connector_dir)?;
    }

    Ok(())
}

fn status_all(dir: &Path) -> Result<()> {
    let root = connectors_root(dir);
    let Ok(entries) = std::fs::read_dir(&root) else {
        println!("no connectors/ directory found at {}", root.display());
        return Ok(());
    };
    println!("{:<20} {:<28} {}", "CONNECTOR", "LAST SYNC", "STATE");
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let state = State::load(&entry.path());
        let last_sync = state.last_sync.clone().unwrap_or_else(|| "never".to_string());
        let flag = if find_mapping_file(&entry.path()).is_ok() {
            "ok"
        } else {
            "no mapping file"
        };
        println!("{:<20} {:<28} {}", name, last_sync, flag);
    }
    Ok(())
}

fn status_one(dir: &Path, vendor: &str) -> Result<()> {
    let connector_dir = connectors_root(dir).join(vendor);
    let mapping_path = find_mapping_file(&connector_dir)?;
    let mapping = load_mapping(&mapping_path)?;
    let state = State::load(&connector_dir);

    println!("connector: {}", vendor);
    println!("mapping: {} (v{})", mapping.mapping, mapping.version);
    println!("last_sync: {}", state.last_sync.as_deref().unwrap_or("never"));
    println!("page_created: {}", state.page_created);
    println!("content_hash: {}", state.content_hash.as_deref().unwrap_or("-"));
    if state.pull_counts.is_empty() {
        println!("pull_counts: (none yet)");
    } else {
        println!("pull_counts:");
        for (k, v) in &state.pull_counts {
            println!("  {}: {}", k, v);
        }
    }
    let mut pulls: Vec<&str> = mapping.pulls.keys().map(|s| s.as_str()).collect();
    pulls.sort();
    let mut shapes: Vec<&str> = mapping.shapes.keys().map(|s| s.as_str()).collect();
    shapes.sort();
    println!("pulls defined: {}", pulls.join(", "));
    println!("shapes defined: {}", shapes.join(", "));
    Ok(())
}

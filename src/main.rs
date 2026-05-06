use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod actions;
mod agents;
mod audit;
mod board;
mod build;
mod ctx;
mod dev;
mod freshness;
mod icons;
mod id;
mod init;
mod links;
mod llms;
mod manifest;
mod mcp;
mod minify;
mod prompts;
mod render;
mod search;
mod theme;
mod track;
mod types;
mod validate;
mod voice;
mod wish;
mod workspace;

#[derive(Parser)]
#[command(name = "kazam", about = "Beautiful sites from simple YAML", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a site from a directory of .yaml files
    Build {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(short, long, default_value = "_site")]
        out: PathBuf,
        /// Minify HTML, CSS, and JS in the output
        #[arg(short, long)]
        release: bool,
        /// Silence the orphan-page check (broken links still reported).
        /// Useful for draft pages you haven't wired into nav yet.
        #[arg(long)]
        allow_orphans: bool,
        /// Emit structured NDJSON instead of human-readable output
        #[arg(long)]
        json: bool,
        /// Skip emitting site.json manifest
        #[arg(long)]
        no_manifest: bool,
        /// Skip emitting search.json index
        #[arg(long)]
        no_search: bool,
        /// Skip emitting _health.html health dashboard
        #[arg(long)]
        no_health: bool,
    },
    /// Watch source, rebuild on change, serve at localhost:PORT
    Dev {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(short, long, default_value = "_site")]
        out: PathBuf,
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
    },
    /// Scaffold a new kazam site in <NAME>/
    Init { name: String },
    /// Print the LLM authoring guide (full AGENTS.md to stdout)
    Agents,
    /// Grant a wish — install a recipe for self-refreshing docs
    Wish {
        #[command(subcommand)]
        command: WishCommand,
    },
    /// Manage the work graph — tasks, dependencies, activity log.
    Track {
        #[command(subcommand)]
        command: track::Command,
        /// Project directory (default: current directory)
        #[arg(short, long, default_value = ".", global = true)]
        dir: PathBuf,
    },
    /// Manage context intelligence — file anatomy, learnings, bugs.
    Ctx {
        #[command(subcommand)]
        command: ctx::Command,
        /// Project directory (default: current directory)
        #[arg(short, long, default_value = ".", global = true)]
        dir: PathBuf,
    },
    /// Live dashboard — renders .kazam/ state as a visual board.
    Board {
        /// Project directory (default: current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(short, long, default_value_t = 3001)]
        port: u16,
    },
    /// Initialize the full agent workspace (track + ctx + hooks) in one shot.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
        /// Project directory (default: current directory)
        #[arg(short, long, default_value = ".", global = true)]
        dir: PathBuf,
    },
    /// Validate page YAML files against component schemas and structural rules.
    Validate {
        /// Directory of .yaml source files to validate (default: current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Human-readable output (default is JSON)
        #[arg(long)]
        pretty: bool,
    },
    /// Run an MCP server over stdio for AI client integration
    Mcp {
        /// Site directory to serve
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Allow write operations (write_page tool)
        #[arg(long)]
        allow_writes: bool,
        /// Transport: stdio (default) or http
        #[arg(long, default_value = "stdio")]
        transport: String,
        /// Port for HTTP transport
        #[arg(long, default_value = "8080")]
        port: u16,
    },
    /// Show freshness status for all pages in the site
    Freshness {
        #[command(subcommand)]
        command: Option<FreshnessCommand>,
        /// Site directory
        #[arg(default_value = ".", global = true)]
        dir: PathBuf,
    },
    /// Show or manage the site's voice configuration
    Voice {
        /// Site directory
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Manage prompt templates for agent workflows
    Prompt {
        #[command(subcommand)]
        command: prompts::Command,
        /// Project directory (default: current directory)
        #[arg(short, long, default_value = ".", global = true)]
        dir: PathBuf,
    },
    /// Manage GitHub Action workflow templates
    Actions {
        #[command(subcommand)]
        command: ActionsCommand,
        /// Project directory (default: current directory)
        #[arg(short, long, default_value = ".", global = true)]
        dir: PathBuf,
    },
    /// Audit site health — freshness, structural quality, and completeness
    Audit {
        /// Site directory
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Human-readable output (default is JSON)
        #[arg(long)]
        pretty: bool,
    },
}

#[derive(Subcommand)]
enum WishCommand {
    /// List available wishes (local + registry)
    List {
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Install a wish from the registry into local wishes/
    Init {
        /// Name of the wish to install
        name: String,
        /// Install to a specific directory instead of wishes/
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Overwrite existing local wish
        #[arg(long)]
        force: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Build {
            dir,
            out,
            release,
            allow_orphans,
            json,
            no_manifest,
            no_search,
            no_health,
        } => build::run(
            &dir,
            &out,
            release,
            allow_orphans,
            json,
            no_manifest,
            no_search,
            no_health,
        ),
        Command::Dev { dir, out, port } => dev::run(&dir, &out, port),
        Command::Init { name } => init::run(&name),
        Command::Agents => agents::run(),
        Command::Wish { command } => match command {
            WishCommand::List { json } => wish::list(json),
            WishCommand::Init { name, dir, force } => wish::init(&name, dir, force),
        },
        Command::Track { command, dir } => track::run(command, &dir),
        Command::Ctx { command, dir } => ctx::run(command, &dir),
        Command::Board { dir, port } => board::run(&dir, port),
        Command::Workspace { command, dir } => workspace::run_command(command, &dir),
        Command::Validate { dir, pretty } => {
            let errors = validate::validate_dir(&dir);
            if pretty {
                validate::print_pretty(&errors);
            } else {
                println!("{}", serde_json::to_string_pretty(&errors)?);
            }
            if !errors.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        Command::Mcp {
            dir,
            allow_writes,
            transport,
            port,
        } => match transport.as_str() {
            "http" => mcp::run_http(&dir, allow_writes, port),
            _ => mcp::run(&dir, allow_writes),
        },
        Command::Freshness { command, dir } => match command {
            None | Some(FreshnessCommand::Show { .. }) => {
                let (pretty, threshold) = match command {
                    Some(FreshnessCommand::Show { pretty, threshold }) => (pretty, threshold),
                    _ => (false, None),
                };
                freshness::run_command(&dir, pretty, threshold)
            }
            Some(FreshnessCommand::Review { json }) => freshness::run_review(&dir, json),
            Some(FreshnessCommand::Act { path, action }) => {
                freshness::run_act(&dir, &path, &action)
            }
            Some(FreshnessCommand::Notify { json }) => freshness::run_notify(&dir, json),
            Some(FreshnessCommand::Drift { pretty, repos }) => {
                freshness::run_drift(&dir, pretty, repos)
            }
        },
        Command::Voice { dir, json } => voice::run(&dir, json),
        Command::Prompt { command, dir } => prompts::run(command, &dir),
        Command::Actions { command, dir } => match command {
            ActionsCommand::List => actions::list(),
            ActionsCommand::Init { name } => actions::init(&name, &dir),
        },
        Command::Audit { dir, pretty } => audit::run(&dir, pretty),
    }
}

#[derive(Subcommand)]
pub enum FreshnessCommand {
    /// Show freshness status for all pages (default)
    Show {
        #[arg(long)]
        pretty: bool,
        #[arg(long)]
        threshold: Option<u64>,
    },
    /// List stale pages for review with recommended actions
    Review {
        /// Output as JSON (default is human-readable)
        #[arg(long)]
        json: bool,
    },
    /// Take action on a stale page: archive, refresh, or skip
    Act {
        /// Path to the page YAML file (relative to site dir)
        path: String,
        /// Action to take
        #[arg(value_enum)]
        action: FreshnessAction,
    },
    /// Generate a digest of stale pages grouped by owner (for Slack/email)
    Notify {
        /// Output as JSON instead of markdown
        #[arg(long)]
        json: bool,
    },
    /// Check if source-of-truth files have changed since pages were last updated
    Drift {
        /// Human-readable table output (default is JSON)
        #[arg(long)]
        pretty: bool,
        /// Additional repo mapping: PREFIX=LOCAL (can repeat)
        #[arg(long = "repo", value_name = "PREFIX=LOCAL")]
        repos: Vec<String>,
    },
}

#[derive(Clone, clap::ValueEnum)]
pub enum FreshnessAction {
    /// Set archived: true on the page
    Archive,
    /// Update freshness.updated to today's date
    Refresh,
}

#[derive(Subcommand)]
pub enum ActionsCommand {
    /// List available action templates
    List,
    /// Initialize an action template in .github/workflows/
    Init {
        /// Template name (validate, freshness, build)
        name: String,
    },
}

#[derive(Subcommand)]
pub enum WorkspaceCommand {
    /// Initialize track + ctx + scan + hooks in one shot
    Init {
        /// Agent to register hooks for
        #[arg(long, default_value = "claude")]
        agent: String,
        /// Gitignore .kazam/ for shared repos
        #[arg(long)]
        skunkworks: bool,
        /// Sass level for human blocker callouts (none, some, lots)
        #[arg(long, default_value = "some")]
        sass: String,
    },
    /// Show workspace status
    Status,
    /// Set the sass level for human blocker callouts
    Sass {
        /// none, some, or lots
        level: String,
    },
    /// Toggle skunkworks mode (gitignore .kazam/)
    Skunkworks {
        /// enable or disable
        action: String,
    },
}

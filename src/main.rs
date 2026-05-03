use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod agents;
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
mod minify;
mod render;
mod theme;
mod track;
mod types;
mod validate;
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
    /// Grant a wish — populated YAML from a workspace full of your context.
    Wish {
        /// Name of the wish (e.g., "deck", or "list" to see all)
        name: String,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long, value_enum)]
        agent: Option<wish::Agent>,
        #[arg(long)]
        stdout: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_name = "TOPIC", num_args = 0..=1, default_missing_value = "")]
        yolo: Option<String>,
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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Build {
            dir,
            out,
            release,
            allow_orphans,
            json,
        } => build::run(&dir, &out, release, allow_orphans, json),
        Command::Dev { dir, out, port } => dev::run(&dir, &out, port),
        Command::Init { name } => init::run(&name),
        Command::Agents => agents::run(),
        Command::Wish {
            name,
            out,
            agent,
            stdout,
            dry_run,
            yolo,
        } => wish::run(&name, out, agent, stdout, dry_run, yolo),
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
    }
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

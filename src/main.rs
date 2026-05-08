use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod actions;
mod agents;
mod annotations;
mod audit;
mod board;
mod build;
mod ctx;
mod dev;
mod freshness;
mod icons;
mod id;
mod ingest;
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
        /// Validate a single YAML file instead of the whole directory
        #[arg(long)]
        file: Option<PathBuf>,
        /// Human-readable output (default is JSON)
        #[arg(long)]
        pretty: bool,
    },
    /// Run an MCP server over stdio for AI client integration
    Mcp {
        /// Site directory to serve
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Allow write operations (write_page, annotate_page, update_annotation)
        #[arg(long)]
        allow_writes: bool,
        /// Transport: stdio (default) or http
        #[arg(long, default_value = "stdio")]
        transport: String,
        /// Port for HTTP transport
        #[arg(long, default_value = "8080")]
        port: u16,
        /// Bind to localhost only (default for http). Mutually exclusive with --remote.
        #[arg(long, conflicts_with = "remote")]
        local: bool,
        /// Bind to all interfaces (0.0.0.0) for remote access. Requires --token or KAZAM_MCP_TOKEN.
        #[arg(long, conflicts_with = "local")]
        remote: bool,
        /// Bearer token for remote HTTP auth. Also reads KAZAM_MCP_TOKEN env var.
        #[arg(long)]
        token: Option<String>,
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
    /// Ingest content from external platforms into kazam pages
    Ingest {
        #[command(subcommand)]
        command: IngestCommand,
    },
    /// Manage annotations on pages (sidecar files in .kazam/annotations/)
    Annotate {
        #[command(subcommand)]
        command: AnnotateCommand,
        /// Site directory
        #[arg(short, long, default_value = ".", global = true)]
        dir: PathBuf,
    },
    /// Output the kazam CSS theme for use in external apps
    Theme {
        #[command(subcommand)]
        command: ThemeCommand,
    },
}

#[derive(Subcommand)]
enum ThemeCommand {
    /// Print the full CSS stylesheet to stdout
    Css {
        /// Theme name (dark, light, red, orange, yellow, green, blue, indigo, violet)
        #[arg(long, default_value = "dark")]
        theme: String,
        /// Enable texture overlay (none, dots, grid, diagonal)
        #[arg(long, default_value = "none")]
        texture: String,
        /// Enable glow effect (none, accent, corner)
        #[arg(long, default_value = "none")]
        glow: String,
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
        Command::Validate { dir, file, pretty } => {
            let errors = if let Some(file_path) = file {
                let path = dir.join(&file_path);
                validate::validate_single_file(&path)
            } else {
                validate::validate_dir(&dir)
            };
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
            local,
            remote,
            token,
        } => match transport.as_str() {
            "http" => {
                let bind_addr = if remote { "0.0.0.0" } else { "127.0.0.1" };
                let resolved_token = token.or_else(|| std::env::var("KAZAM_MCP_TOKEN").ok());
                if remote && resolved_token.is_none() {
                    anyhow::bail!(
                        "--remote requires a bearer token: pass --token <TOKEN> or set KAZAM_MCP_TOKEN"
                    );
                }
                if local && resolved_token.is_some() {
                    eprintln!("kazam mcp: note: --token is ignored in local mode");
                }
                mcp::run_http(
                    &dir,
                    allow_writes,
                    port,
                    bind_addr,
                    resolved_token.as_deref(),
                )
            }
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
        Command::Annotate { command, dir } => match command {
            AnnotateCommand::Add {
                page,
                text,
                section,
                author,
                source,
            } => {
                let page_path = dir.join(&page);
                if !page_path.exists() {
                    anyhow::bail!("page not found: {}", page);
                }
                let source_enum: types::AnnotationSource = match source.as_str() {
                    "cli" => types::AnnotationSource::Cli,
                    "agent" => types::AnnotationSource::Agent,
                    "web" => types::AnnotationSource::Web,
                    other => {
                        anyhow::bail!("invalid source '{}': expected cli, agent, or web", other)
                    }
                };
                let today = freshness::today_iso();
                let id = annotations::generate_id(&today);
                let ann = types::Annotation {
                    id: id.clone(),
                    text,
                    author,
                    section,
                    added: today,
                    status: types::AnnotationStatus::Pending,
                    source: source_enum,
                };
                let slug = annotations::page_slug_from_path(&page);
                let path = annotations::save_annotation(&dir, &slug, &ann)?;
                println!("✓ annotation {} → {}", id, path.display());
                Ok(())
            }
            AnnotateCommand::List { page, json } => {
                let slug = annotations::page_slug_from_path(&page);
                let anns = annotations::load_annotations(&dir, &slug)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&anns)?);
                } else if anns.is_empty() {
                    println!("no annotations on {}", page);
                } else {
                    for ann in &anns {
                        let status = match ann.status {
                            types::AnnotationStatus::Pending => "pending",
                            types::AnnotationStatus::Incorporated => "incorporated",
                            types::AnnotationStatus::Ignored => "ignored",
                            types::AnnotationStatus::Stale => "stale",
                        };
                        let section = ann.section.as_deref().unwrap_or("-");
                        println!(
                            "  {} [{}] ({}, {}) {}",
                            ann.id, status, ann.author, section, ann.text
                        );
                    }
                    println!("\n{} annotation(s)", anns.len());
                }
                Ok(())
            }
            AnnotateCommand::Resolve { id } => {
                annotations::resolve_annotation(&dir, &id)?;
                println!("✓ {} → incorporated", id);
                Ok(())
            }
            AnnotateCommand::Clear { page } => {
                let slug = annotations::page_slug_from_path(&page);
                let count = annotations::clear_annotations(&dir, &slug)?;
                println!("✓ cleared {} annotation(s) from {}", count, page);
                Ok(())
            }
        },
        Command::Theme { command } => match command {
            ThemeCommand::Css {
                theme: theme_name,
                texture,
                glow,
            } => {
                let mode = if theme_name == "light" {
                    types::Mode::Light
                } else {
                    types::Mode::Dark
                };
                let t = theme::Theme::named(&theme_name, mode);
                let tex = match texture.as_str() {
                    "dots" => types::Texture::Dots,
                    "grid" => types::Texture::Grid,
                    "diagonal" => types::Texture::Diagonal,
                    _ => types::Texture::None,
                };
                let g = match glow.as_str() {
                    "accent" => types::Glow::Accent,
                    "corner" => types::Glow::Corner,
                    _ => types::Glow::None,
                };
                print!("{}", theme::render_css(&t, tex, g));
                Ok(())
            }
        },
        Command::Ingest { command } => match command {
            IngestCommand::Notion {
                database,
                page,
                token,
                out,
                dry_run,
                stats,
                all,
            } => {
                if stats {
                    ingest::notion_stats(&database, &page, &token, all)
                } else {
                    ingest::notion(&database, &page, &token, &out, dry_run, all)
                }
            }
        },
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
pub enum IngestCommand {
    /// Import pages from a Notion workspace.
    ///
    /// Setup (one-time):
    ///   1. Go to https://www.notion.so/profile/integrations/internal
    ///   2. Click "New integration", name it (e.g. "kazam"), submit
    ///   3. Copy the Internal Integration Secret (starts with ntn_)
    ///   4. Set NOTION_TOKEN in .env or export NOTION_TOKEN=ntn_...
    ///   5. Find your workspace ID: click workspace name (top-left) → Settings → General
    ///      (workspace ID is at the bottom of the page)
    ///   6. Set NOTION_WORKSPACE_ID in .env (optional, for --all discovery)
    ///   7. In Notion, open the page/database you want to import
    ///   8. Click ··· → Connections → add your integration (gives it access to content)
    ///      (child pages inherit access from parent)
    ///
    /// Finding IDs:
    ///   Page URL:  notion.so/My-Page-abc123def456 → --page abc123de-f456-...
    ///   DB URL:    notion.so/abc123?v=...         → --database abc123...
    ///   The 32-char hex string in the URL is the ID (add dashes for UUID format)
    Notion {
        /// Notion database ID — each row becomes a page
        #[arg(long)]
        database: Option<String>,
        /// Notion page ID — import a single page and its children
        #[arg(long)]
        page: Option<String>,
        /// Notion API token (default: .env NOTION_TOKEN or env var)
        #[arg(long)]
        token: Option<String>,
        /// Output directory for generated YAML files (default: current dir)
        #[arg(long, default_value = ".")]
        out: PathBuf,
        /// Preview what would be created without writing files
        #[arg(long)]
        dry_run: bool,
        /// Show staleness stats without ingesting (metadata only, fast)
        #[arg(long)]
        stats: bool,
        /// Discover and ingest all pages the integration can access
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand)]
pub enum AnnotateCommand {
    /// Add an annotation to a page
    Add {
        /// Page path relative to site root (e.g. 'deals/acme.yaml')
        page: String,
        /// Annotation text
        text: String,
        /// Section this annotation applies to (e.g. 'competitive', 'timeline')
        #[arg(long)]
        section: Option<String>,
        /// Author name
        #[arg(long, default_value = "anonymous")]
        author: String,
        /// Source: cli, agent, or web
        #[arg(long, default_value = "cli")]
        source: String,
    },
    /// List annotations on a page
    List {
        /// Page path relative to site root
        page: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Mark an annotation as incorporated
    Resolve {
        /// Annotation ID (e.g. 'ann-20260507-e708')
        id: String,
    },
    /// Remove all annotations for a page
    Clear {
        /// Page path relative to site root
        page: String,
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

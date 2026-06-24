use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{RecursiveMode, Watcher};
use tiny_http::{Header, Method, Response, Server};

use crate::ctx::types::{AnatomyStore, BugStore, LearningStore};
use crate::render;
use crate::track::types::{LogStore, TaskStatus, TaskStore};
use crate::types::*;
use crate::workspace;

pub fn run(project: &Path, port: u16) -> Result<()> {
    workspace::ensure(project)?;

    let config = board_site_config(project);
    let html = generate_html(project, &config)?;

    let version = Arc::new(AtomicU64::new(1));
    let html_arc = Arc::new(std::sync::RwLock::new(html));

    // Watcher thread
    let project_clone = project.to_path_buf();
    let config_clone = config.clone();
    let ver_clone = version.clone();
    let html_clone = html_arc.clone();
    thread::spawn(move || watch_loop(project_clone, config_clone, ver_clone, html_clone));

    // HTTP server
    let (server, actual_port) = bind_next_available(port)?;
    if actual_port != port {
        println!("\n  ⚠ port {port} is in use — serving on {actual_port} instead");
    }
    let url = format!("http://localhost:{actual_port}");
    println!("\n  ➜ {url}");
    println!("  watching .kazam/\n");
    open_browser(&url);

    for req in server.incoming_requests() {
        let ver = version.clone();
        let html = html_arc.clone();
        thread::spawn(move || {
            if let Err(e) = handle(req, &ver, &html) {
                eprintln!("  request error: {e}");
            }
        });
    }
    Ok(())
}

// ── Page generation ──────────────────────────────

fn generate_html(project: &Path, config: &SiteConfig) -> Result<String> {
    let page = generate_page(project, config)?;
    Ok(render::render_page(
        &page, config, "", "", "", false, "", None,
    ))
}

fn generate_page(project: &Path, config: &SiteConfig) -> Result<Page> {
    let root = workspace::root(project);

    let tasks: TaskStore =
        workspace::read_yaml(&root.join("track/tasks.yaml")).unwrap_or(TaskStore { tasks: vec![] });
    let log: LogStore =
        workspace::read_yaml(&root.join("track/log.yaml")).unwrap_or(LogStore { events: vec![] });
    // Prefer flat anatomy (written alongside layered summary); fall back to legacy path
    let flat_path = root.join("ctx/anatomy.flat.yaml");
    let anatomy_fallback = root.join("ctx/anatomy.yaml");
    let anatomy: AnatomyStore = if flat_path.exists() {
        workspace::read_yaml(&flat_path).unwrap_or(AnatomyStore {
            scanned: String::new(),
            files: vec![],
        })
    } else {
        workspace::read_yaml::<AnatomyStore>(&anatomy_fallback).unwrap_or(AnatomyStore {
            scanned: String::new(),
            files: vec![],
        })
    };
    let learnings: LearningStore = workspace::read_yaml(&root.join("ctx/learnings.yaml"))
        .unwrap_or(LearningStore { learnings: vec![] });
    let bugs: BugStore =
        workspace::read_yaml(&root.join("ctx/bugs.yaml")).unwrap_or(BugStore { bugs: vec![] });

    use crate::track::types::TaskOwner;

    // Stats
    let open = tasks
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Open)
        .count();
    let active = tasks
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Active)
        .count();
    let blocked = tasks
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Blocked)
        .count();
    let closed = tasks
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Closed)
        .count();

    let stat_grid = Component::StatGrid {
        stats: vec![
            Stat {
                label: "Open".into(),
                value: open.to_string(),
                detail: None,
                color: SemColor::Teal,
                trend: None,
                previous: None,
                history: None,
            },
            Stat {
                label: "Active".into(),
                value: active.to_string(),
                detail: None,
                color: SemColor::Green,
                trend: None,
                previous: None,
                history: None,
            },
            Stat {
                label: "Blocked".into(),
                value: blocked.to_string(),
                detail: None,
                color: if blocked > 0 {
                    SemColor::Red
                } else {
                    SemColor::Default
                },
                trend: None,
                previous: None,
                history: None,
            },
            Stat {
                label: "Closed".into(),
                value: closed.to_string(),
                detail: None,
                color: SemColor::Default,
                trend: None,
                previous: None,
                history: None,
            },
        ],
        columns: 4,
    };

    // Human blocker callout
    let sass = workspace::read_config(project)
        .map(|c| c.sass)
        .unwrap_or_default();
    let human_blockers: Vec<&crate::track::types::Task> = tasks
        .tasks
        .iter()
        .filter(|t| {
            t.owner == TaskOwner::Human && matches!(t.status, TaskStatus::Open | TaskStatus::Active)
        })
        .collect();
    let mut components = vec![stat_grid];
    if !human_blockers.is_empty() {
        let names: Vec<String> = human_blockers
            .iter()
            .map(|t| format!("{} — {}", t.id, t.title))
            .collect();
        let n = human_blockers.len();
        let title = match sass {
            workspace::SassLevel::None => format!(
                "{n} task{s} waiting on you",
                s = if n == 1 { "" } else { "s" }
            ),
            workspace::SassLevel::Some => format!(
                "Hey — {n} task{s} need{v} your attention",
                s = if n == 1 { "" } else { "s" },
                v = if n == 1 { "s" } else { "" }
            ),
            workspace::SassLevel::Lots => {
                if n == 1 {
                    "The agent is literally waiting on you. Just one thing. Come on.".into()
                } else {
                    format!(
                        "{n} tasks are blocked on you. The agent can't do your job for you. Yet."
                    )
                }
            }
        };
        components.push(Component::Callout {
            variant: CalloutVariant::Warn,
            title: Some(title),
            body: names.join("\n"),
            links: None,
        });
    }

    // ── Tab 1: Tasks (task tree) ──
    let tree_nodes = tasks_to_tree_nodes(&tasks.tasks);
    let task_tab = Tab {
        label: format!("Tasks ({})", open + active + blocked),
        components: vec![Component::Tree {
            nodes: tree_nodes,
            default_filter: TreeFilter::Incomplete,
            show_filter_toggle: true,
            default_collapsed: false,
            default_depth: None,
            show_counts: false,
            show_summary: false,
            default_view: TreeDefaultView::Tree,
        }],
    };

    // ── Tab 2: Anatomy (folder tree) ──
    let anatomy_tab = if !anatomy.files.is_empty() {
        let total_tokens: u64 = anatomy.files.iter().map(|f| f.tokens).sum();
        let anat_nodes = anatomy_to_tree_nodes(&anatomy.files);
        Tab {
            label: format!("Anatomy ({})", anatomy.files.len()),
            components: vec![
                Component::StatGrid {
                    stats: vec![
                        Stat {
                            label: "Files".into(),
                            value: anatomy.files.len().to_string(),
                            detail: None,
                            color: SemColor::Default,
                            trend: None,
                            previous: None,
                            history: None,
                        },
                        Stat {
                            label: "Total tokens".into(),
                            value: format!("~{}", format_token_count(total_tokens)),
                            detail: None,
                            color: SemColor::Default,
                            trend: None,
                            previous: None,
                            history: None,
                        },
                    ],
                    columns: 2,
                },
                Component::Tree {
                    nodes: anat_nodes,
                    default_filter: TreeFilter::All,
                    show_filter_toggle: false,
                    default_collapsed: true,
                    default_depth: None,
                    show_counts: false,
                    show_summary: false,
                    default_view: TreeDefaultView::Tree,
                },
            ],
        }
    } else {
        Tab {
            label: "Anatomy".into(),
            components: vec![Component::Callout {
                variant: CalloutVariant::Info,
                title: None,
                body: "Run kazam ctx scan to populate anatomy.".into(),
                links: None,
            }],
        }
    };

    // ── Tab 3: Activity (event log + learnings + bugs) ──
    let mut activity_components: Vec<Component> = Vec::new();
    let events = log_to_events(&log);
    if !events.is_empty() {
        activity_components.push(Component::EventTimeline {
            events,
            default_filter: EventFilter::All,
            show_filter_toggle: true,
            limit: Some(25),
            filter_by: vec![],
            group_by: None,
        });
    }
    if !learnings.learnings.is_empty() {
        let learn_rows: Vec<HashMap<String, serde_yaml::Value>> = learnings
            .learnings
            .iter()
            .map(|l| {
                let mut row = HashMap::new();
                row.insert(
                    "category".into(),
                    serde_yaml::Value::String(l.category.label().into()),
                );
                row.insert("learning".into(), serde_yaml::Value::String(l.text.clone()));
                row.insert("id".into(), serde_yaml::Value::String(l.id.clone()));
                row
            })
            .collect();
        activity_components.push(Component::Section {
            heading: Some("Learnings".into()),
            eyebrow: None,
            components: vec![Component::Table {
                columns: vec![
                    TableColumn {
                        key: "category".into(),
                        label: "Category".into(),
                        sortable: true,
                        align: Align::Left,
                        color_map: HashMap::new(),
                    },
                    TableColumn {
                        key: "learning".into(),
                        label: "Learning".into(),
                        sortable: false,
                        align: Align::Left,
                        color_map: HashMap::new(),
                    },
                    TableColumn {
                        key: "id".into(),
                        label: "ID".into(),
                        sortable: false,
                        align: Align::Left,
                        color_map: HashMap::new(),
                    },
                ],
                rows: learn_rows,
                filterable: false,
                summary: None,
            }],
            align: Align::Left,
            id: None,
        });
    }
    if !bugs.bugs.is_empty() {
        let bug_rows: Vec<HashMap<String, serde_yaml::Value>> = bugs
            .bugs
            .iter()
            .map(|b| {
                let mut row = HashMap::new();
                row.insert(
                    "status".into(),
                    serde_yaml::Value::String(
                        if b.resolved.is_some() {
                            "resolved"
                        } else {
                            "open"
                        }
                        .into(),
                    ),
                );
                row.insert(
                    "symptom".into(),
                    serde_yaml::Value::String(b.symptom.clone()),
                );
                row.insert(
                    "file".into(),
                    serde_yaml::Value::String(b.file_path.clone().unwrap_or_else(|| "—".into())),
                );
                row.insert(
                    "fix".into(),
                    serde_yaml::Value::String(b.resolution.clone().unwrap_or_else(|| "—".into())),
                );
                row
            })
            .collect();
        activity_components.push(Component::Section {
            heading: Some("Bugs".into()),
            eyebrow: None,
            components: vec![Component::Table {
                columns: vec![
                    TableColumn {
                        key: "status".into(),
                        label: "Status".into(),
                        sortable: true,
                        align: Align::Left,
                        color_map: HashMap::new(),
                    },
                    TableColumn {
                        key: "symptom".into(),
                        label: "Symptom".into(),
                        sortable: false,
                        align: Align::Left,
                        color_map: HashMap::new(),
                    },
                    TableColumn {
                        key: "file".into(),
                        label: "File".into(),
                        sortable: true,
                        align: Align::Left,
                        color_map: HashMap::new(),
                    },
                    TableColumn {
                        key: "fix".into(),
                        label: "Fix".into(),
                        sortable: false,
                        align: Align::Left,
                        color_map: HashMap::new(),
                    },
                ],
                rows: bug_rows,
                filterable: false,
                summary: None,
            }],
            align: Align::Left,
            id: None,
        });
    }
    if activity_components.is_empty() {
        activity_components.push(Component::Callout {
            variant: CalloutVariant::Info,
            title: None,
            body: "No activity yet.".into(),
            links: None,
        });
    }
    let activity_tab = Tab {
        label: "Activity".into(),
        components: activity_components,
    };

    components.push(Component::Tabs {
        tabs: vec![task_tab, anatomy_tab, activity_tab],
    });

    Ok(Page {
        title: format!("{} — Board", config.name),
        shell: Shell::Standard,
        eyebrow: Some("Agent Workspace".into()),
        subtitle: None,
        components: Some(components),
        slides: None,
        unlisted: true,
        texture: None,
        glow: None,
        print_flow: None,
        hub: None,
        freshness: None,
        search_terms: Vec::new(),
        owner: None,
        references: Vec::new(),
        personas: Vec::new(),
        archived: false,
        draft: false,
        nav_layout: None,
        nav: None,
    })
}

// ── Mapping ──────────────────────────────────────

fn tasks_to_tree_nodes(tasks: &[crate::track::types::Task]) -> Vec<TreeNode> {
    use crate::track::types::TaskOwner;

    fn task_to_node(
        t: &crate::track::types::Task,
        children_map: &HashMap<Option<&str>, Vec<&crate::track::types::Task>>,
    ) -> TreeNode {
        let status = match t.status {
            TaskStatus::Open => TreeStatus::Upcoming,
            TaskStatus::Active => TreeStatus::Active,
            TaskStatus::Closed => TreeStatus::Completed,
            TaskStatus::Blocked => TreeStatus::Blocked,
            TaskStatus::Deferred => TreeStatus::Default,
        };
        let mut meta_parts: Vec<String> = Vec::new();
        if let Some(ref a) = t.assignee {
            meta_parts.push(a.clone());
        }
        meta_parts.push(format!("P{}", t.priority));
        meta_parts.push(t.task_type.label().to_string());
        let meta = meta_parts.join(" · ");
        let note = match &t.note {
            Some(n) => Some(format!("{meta} — {n}")),
            None => Some(meta),
        };
        TreeNode {
            label: format!("{} {}", t.id, t.title),
            status,
            note,
            children: build_level(Some(&t.id), children_map),
            owner: None,
        }
    }

    fn build_level(
        parent: Option<&str>,
        children_map: &HashMap<Option<&str>, Vec<&crate::track::types::Task>>,
    ) -> Vec<TreeNode> {
        let Some(kids) = children_map.get(&parent) else {
            return vec![];
        };
        kids.iter().map(|t| task_to_node(t, children_map)).collect()
    }

    // Split: agent tasks get the normal parent→child hierarchy,
    // human tasks get their own top-level group node.
    let agent_tasks: Vec<&crate::track::types::Task> = tasks
        .iter()
        .filter(|t| t.owner != TaskOwner::Human)
        .collect();
    let human_tasks: Vec<&crate::track::types::Task> = tasks
        .iter()
        .filter(|t| t.owner == TaskOwner::Human)
        .collect();

    let mut children_map: HashMap<Option<&str>, Vec<&crate::track::types::Task>> = HashMap::new();
    for t in &agent_tasks {
        children_map.entry(t.parent.as_deref()).or_default().push(t);
    }

    let mut result = Vec::new();

    // Human group at top (only if there are incomplete human tasks)
    let incomplete_human: Vec<TreeNode> = human_tasks
        .iter()
        .filter(|t| !matches!(t.status, TaskStatus::Closed))
        .map(|t| {
            let status = match t.status {
                TaskStatus::Open => TreeStatus::Priority,
                TaskStatus::Active => TreeStatus::Active,
                TaskStatus::Blocked => TreeStatus::Blocked,
                _ => TreeStatus::Default,
            };
            TreeNode {
                label: format!("{} {}", t.id, t.title),
                status,
                note: t.note.clone(),
                children: vec![],
                owner: None,
            }
        })
        .collect();

    if !incomplete_human.is_empty() {
        result.push(TreeNode {
            label: "Waiting on you".into(),
            status: TreeStatus::Priority,
            note: Some(format!(
                "{} task{}",
                incomplete_human.len(),
                if incomplete_human.len() == 1 { "" } else { "s" }
            )),
            children: incomplete_human,
            owner: None,
        });
    }

    // Agent tree (normal hierarchy)
    result.extend(build_level(None, &children_map));

    result
}

fn anatomy_to_tree_nodes(files: &[crate::ctx::types::FileEntry]) -> Vec<TreeNode> {
    use std::collections::BTreeMap;

    struct DirNode {
        files: Vec<(String, u64, Option<String>)>,
        dirs: BTreeMap<String, DirNode>,
    }

    impl DirNode {
        fn new() -> Self {
            Self {
                files: vec![],
                dirs: BTreeMap::new(),
            }
        }

        fn total_tokens(&self) -> u64 {
            let file_sum: u64 = self.files.iter().map(|(_, t, _)| *t).sum();
            let dir_sum: u64 = self.dirs.values().map(|d| d.total_tokens()).sum();
            file_sum + dir_sum
        }

        fn file_count(&self) -> usize {
            self.files.len() + self.dirs.values().map(|d| d.file_count()).sum::<usize>()
        }
    }

    const MAX_DEPTH: usize = 3;

    let mut root = DirNode::new();
    for f in files {
        let parts: Vec<&str> = f.path.split('/').collect();
        let mut current = &mut root;
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                current
                    .files
                    .push((part.to_string(), f.tokens, f.description.clone()));
            } else if i < MAX_DEPTH - 1 {
                current = current
                    .dirs
                    .entry(part.to_string())
                    .or_insert_with(DirNode::new);
            } else {
                let remainder = parts[i..].join("/");
                current
                    .files
                    .push((remainder, f.tokens, f.description.clone()));
                break;
            }
        }
    }

    fn build(node: &DirNode) -> Vec<TreeNode> {
        let mut out = Vec::new();
        for (name, dir) in &node.dirs {
            out.push(TreeNode {
                label: format!("{name}/"),
                status: TreeStatus::Default,
                note: Some(format!(
                    "~{} tokens · {} files",
                    format_token_count(dir.total_tokens()),
                    dir.file_count()
                )),
                children: build(dir),
                owner: None,
            });
        }
        for (name, tokens, desc) in &node.files {
            let note = match desc {
                Some(d) if !d.is_empty() => {
                    format!("~{} tokens — {d}", format_token_count(*tokens))
                }
                _ => format!("~{} tokens", format_token_count(*tokens)),
            };
            out.push(TreeNode {
                label: name.clone(),
                status: TreeStatus::Default,
                note: Some(note),
                children: vec![],
                owner: None,
            });
        }
        out
    }

    build(&root)
}

fn format_token_count(t: u64) -> String {
    if t >= 1000 {
        format!("{:.1}k", t as f64 / 1000.0)
    } else {
        t.to_string()
    }
}

fn log_to_events(log: &LogStore) -> Vec<EventItem> {
    log.events
        .iter()
        .rev()
        .map(|e| {
            let severity = match e.severity {
                crate::track::types::LogSeverity::Major => EventSeverity::Major,
                crate::track::types::LogSeverity::Minor => EventSeverity::Minor,
                crate::track::types::LogSeverity::Info => EventSeverity::Info,
            };
            EventItem {
                date: e.date.clone(),
                title: e.title.clone(),
                summary: e.detail.clone(),
                severity,
                source: e.source.clone(),
                link: None,
                tags: vec![],
            }
        })
        .collect()
}

fn board_site_config(project: &Path) -> SiteConfig {
    let name = workspace::read_config(project)
        .map(|c| c.project_name)
        .unwrap_or_else(|_| "Project".to_string());

    SiteConfig {
        name,
        theme: Some("teal".to_string()),
        colors: HashMap::new(),
        nav: None,
        favicon: None,
        logo: None,
        view_source: Some(false),
        texture: Texture::Dots,
        glow: Glow::Corner,
        nav_layout: NavLayout::Top,
        mode: Mode::Dark,
        description: None,
        url: None,
        edit_url: None,
        og_image: None,
        voice: None,
        roles: Vec::new(),
        drift: None,
    }
}

// Clone for SiteConfig is needed for the watcher thread
impl Clone for SiteConfig {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            theme: self.theme.clone(),
            colors: self.colors.clone(),
            nav: None,
            favicon: None,
            logo: None,
            view_source: self.view_source,
            texture: self.texture,
            glow: self.glow,
            nav_layout: self.nav_layout,
            mode: self.mode,
            description: self.description.clone(),
            url: self.url.clone(),
            edit_url: self.edit_url.clone(),
            og_image: self.og_image.clone(),
            voice: self.voice.clone(),
            roles: self.roles.clone(),
            drift: self.drift.clone(),
        }
    }
}

// ── Server ───────────────────────────────────────

const PORT_FALLBACK_ATTEMPTS: u16 = 10;

fn bind_next_available(start: u16) -> Result<(Server, u16)> {
    use std::net::TcpListener;
    let mut last_err: Option<String> = None;
    for p in start..start.saturating_add(PORT_FALLBACK_ATTEMPTS) {
        let addr = format!("0.0.0.0:{p}");
        if TcpListener::bind(&addr).is_err() {
            last_err = Some(format!("port {p} already in use"));
            continue;
        }
        match Server::http(&addr) {
            Ok(s) => return Ok((s, p)),
            Err(e) => last_err = Some(e.to_string()),
        }
    }
    let tail = start
        .saturating_add(PORT_FALLBACK_ATTEMPTS)
        .saturating_sub(1);
    anyhow::bail!(
        "no free port in range {start}..={tail} — last error: {}",
        last_err.unwrap_or_else(|| "unknown".to_string())
    );
}

fn handle(
    req: tiny_http::Request,
    version: &AtomicU64,
    html: &std::sync::RwLock<String>,
) -> Result<()> {
    if req.method() != &Method::Get {
        return req
            .respond(Response::from_string("method not allowed").with_status_code(405))
            .context("respond");
    }

    let url = req.url().split('?').next().unwrap_or("/");

    if url == "/__kazam_version__" {
        let v = version.load(Ordering::SeqCst).to_string();
        let resp = Response::from_string(v)
            .with_header(hdr("Content-Type", "text/plain"))
            .with_header(hdr("Cache-Control", "no-store"));
        return req.respond(resp).context("respond");
    }

    if url == "/" || url == "/index.html" {
        let content = html.read().unwrap().clone();
        let resp = Response::from_string(content)
            .with_header(hdr("Content-Type", "text/html; charset=utf-8"))
            .with_header(hdr("Cache-Control", "no-store"));
        return req.respond(resp).context("respond");
    }

    req.respond(Response::from_string("404").with_status_code(404))
        .context("respond")
}

fn hdr(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    let _ = std::process::Command::new(cmd)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn watch_loop(
    project: PathBuf,
    config: SiteConfig,
    version: Arc<AtomicU64>,
    html: Arc<std::sync::RwLock<String>>,
) {
    let kazam_dir = workspace::root(&project);
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("watcher init failed: {e}");
            return;
        }
    };
    if let Err(e) = watcher.watch(&kazam_dir, RecursiveMode::Recursive) {
        eprintln!("watch failed: {e}");
        return;
    }

    let mut last_build = Instant::now() - Duration::from_secs(10);

    for event in rx {
        let Ok(event) = event else { continue };
        let relevant = event
            .paths
            .iter()
            .any(|p| p.extension().map(|e| e == "yaml").unwrap_or(false));
        if !relevant {
            continue;
        }
        if last_build.elapsed() < Duration::from_millis(200) {
            continue;
        }
        last_build = Instant::now();

        print!("  rebuild…");
        match generate_html(&project, &config) {
            Ok(new_html) => {
                *html.write().unwrap() = new_html;
                version.fetch_add(1, Ordering::SeqCst);
                println!(" ✓");
            }
            Err(e) => {
                println!(" ✗\n    {e:#}");
            }
        }
    }
}

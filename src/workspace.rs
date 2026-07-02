use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const DIR: &str = ".kazam";

fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SassLevel {
    None,
    #[default]
    Some,
    Lots,
}

fn is_default_sass(s: &SassLevel) -> bool {
    *s == SassLevel::Some
}

#[derive(Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub project_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_assignee: Option<String>,
    pub created: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub skunkworks: bool,
    #[serde(default, skip_serializing_if = "is_default_sass")]
    pub sass: SassLevel,
}

pub fn root(project: &Path) -> PathBuf {
    project.join(DIR)
}

pub fn ensure(project: &Path) -> Result<PathBuf> {
    let r = root(project);
    for sub in ["track", "ctx", "ctx/anatomy", "hooks", "annotations"] {
        fs::create_dir_all(r.join(sub)).with_context(|| format!("create .kazam/{sub}"))?;
    }

    let config_path = r.join("config.yaml");
    if !config_path.exists() {
        let canonical = fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
        let name = canonical
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();
        let config = WorkspaceConfig {
            project_name: name,
            default_assignee: None,
            created: chrono::Local::now().to_rfc3339(),
            skunkworks: false,
            sass: SassLevel::Some,
        };
        write_yaml(&config_path, &config)?;
    }

    let empty_files = [
        ("track/tasks.yaml", "tasks: []\n"),
        ("track/log.yaml", "events: []\n"),
        // anatomy.tsv is the agent-facing layered summary; anatomy.flat.yaml is the internal flat store
        (
            "ctx/anatomy.tsv",
            "# scanned:\n# root_files\npath\ttokens\treads\tdescription\n\n# directories\npath\tfiles\ttokens\tdescription\n",
        ),
        ("ctx/anatomy.flat.yaml", "scanned: \"\"\nfiles: []\n"),
        ("ctx/learnings.yaml", "learnings: []\n"),
        ("ctx/bugs.yaml", "bugs: []\n"),
        ("ctx/corrections.yaml", "corrections: []\n"),
        (
            "ctx/rules-override.md",
            "<!-- Team-specific workspace rules. Content here is appended to\n     .claude/rules/kazam-workspace.md on each `kazam workspace init`.\n     Add conventions, safety guards, or push policies your team needs. -->\n",
        ),
    ];
    for (rel, default) in empty_files {
        let p = r.join(rel);
        if !p.exists() {
            fs::write(&p, default).with_context(|| format!("write {rel}"))?;
        }
    }

    Ok(r)
}

pub fn apply_skunkworks(project: &Path) -> Result<()> {
    let gi = project.join(".gitignore");
    let content = fs::read_to_string(&gi).unwrap_or_default();
    let entries = [
        ".kazam/",
        ".claude/rules/kazam-workspace.md",
        ".claude/agents/kazam-scout.md",
    ];
    use std::io::Write;
    let needs: Vec<&str> = entries
        .iter()
        .filter(|e| !content.lines().any(|l| l.trim() == **e))
        .copied()
        .collect();
    if !needs.is_empty() {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&gi)
            .context("append to .gitignore")?;
        if !content.is_empty() && !content.ends_with('\n') {
            writeln!(f)?;
        }
        for entry in &needs {
            writeln!(f, "{entry}")?;
        }
    }
    Ok(())
}

pub fn set_skunkworks(project: &Path) -> Result<()> {
    let config_path = root(project).join("config.yaml");
    let mut config: WorkspaceConfig = read_yaml(&config_path)?;
    config.skunkworks = true;
    write_yaml(&config_path, &config)?;
    apply_skunkworks(project)?;
    println!("  ✓ skunkworks mode — .kazam/ added to .gitignore");
    Ok(())
}

pub fn disable_skunkworks(project: &Path) -> Result<()> {
    let config_path = root(project).join("config.yaml");
    let mut config: WorkspaceConfig = read_yaml(&config_path)?;
    config.skunkworks = false;
    write_yaml(&config_path, &config)?;

    let gi = project.join(".gitignore");
    if let Ok(content) = fs::read_to_string(&gi) {
        let remove = [
            ".kazam/",
            ".kazam",
            ".claude/rules/kazam-workspace.md",
            ".claude/agents/kazam-scout.md",
        ];
        let filtered: Vec<&str> = content
            .lines()
            .filter(|l| !remove.contains(&l.trim()))
            .collect();
        fs::write(&gi, filtered.join("\n") + "\n").context("rewrite .gitignore")?;
    }

    println!("  ✓ skunkworks disabled — .kazam/ removed from .gitignore");
    Ok(())
}

pub fn read_config(project: &Path) -> Result<WorkspaceConfig> {
    let path = root(project).join("config.yaml");
    let text = fs::read_to_string(&path).context("read .kazam/config.yaml")?;
    serde_yaml::from_str(&text).context("parse .kazam/config.yaml")
}

pub fn write_yaml<T: Serialize>(path: &Path, data: &T) -> Result<()> {
    let yaml = serde_yaml::to_string(data).context("serialize yaml")?;
    let tmp = path.with_extension("yaml.tmp");
    fs::write(&tmp, &yaml).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}

pub fn read_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_yaml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn parse_sass_level(s: &str) -> Result<SassLevel, String> {
    match s {
        "none" => Ok(SassLevel::None),
        "some" => Ok(SassLevel::Some),
        "lots" => Ok(SassLevel::Lots),
        _ => Err(format!(
            "unknown sass level: {s} (expected: none, some, lots)"
        )),
    }
}

pub fn set_sass(project: &Path, level: SassLevel) -> Result<()> {
    let config_path = root(project).join("config.yaml");
    let mut config: WorkspaceConfig = read_yaml(&config_path)?;
    config.sass = level;
    write_yaml(&config_path, &config)
}

pub fn hooks_installed(project: &Path) -> bool {
    let hooks_dir = root(project).join("hooks");
    hooks_dir.join("session-start.sh").exists()
}

pub fn run_command(cmd: crate::WorkspaceCommand, project: &Path) -> Result<()> {
    match cmd {
        crate::WorkspaceCommand::Init {
            agent,
            skunkworks,
            sass,
        } => {
            ensure(project)?;
            if skunkworks {
                set_skunkworks(project)?;
            }
            if let Ok(level) = parse_sass_level(&sass) {
                if level != SassLevel::Some {
                    set_sass(project, level)?;
                }
            }

            let store = crate::ctx::scan::scan(project)?;
            write_yaml(&root(project).join("ctx/anatomy.flat.yaml"), &store)?;
            crate::ctx::scan::write_layered(project, &store)?;
            println!(
                "  ✓ workspace initialized — {} files indexed",
                store.files.len()
            );

            crate::ctx::hooks::install(project, &agent, skunkworks)?;
            Ok(())
        }
        crate::WorkspaceCommand::Sass { level } => {
            let parsed = parse_sass_level(&level).map_err(|e| anyhow::anyhow!("{e}"))?;
            set_sass(project, parsed)?;
            println!("  ✓ sass level set to: {level}");
            Ok(())
        }
        crate::WorkspaceCommand::Skunkworks { action } => {
            ensure(project)?;
            match action.as_str() {
                "enable" => set_skunkworks(project),
                "disable" => disable_skunkworks(project),
                _ => anyhow::bail!("unknown action: {action} (expected: enable, disable)"),
            }
        }
        crate::WorkspaceCommand::Status => {
            let config = read_config(project);
            match config {
                Ok(c) => {
                    println!("  project: {}", c.project_name);
                    if c.skunkworks {
                        println!("  mode: skunkworks");
                    }
                }
                Err(_) => {
                    println!("  no workspace found — run `kazam workspace init`");
                    return Ok(());
                }
            }
            crate::ctx::run(crate::ctx::Command::Status { json: false }, project)?;
            crate::ctx::hooks::status(project)?;
            Ok(())
        }
    }
}

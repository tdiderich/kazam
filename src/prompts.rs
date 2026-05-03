use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

// ── Types ─────────────────────────────────────────────

#[derive(Deserialize, Serialize, Clone)]
pub struct Prompt {
    pub name: String,
    pub description: String,
    /// Target model (optional hint, e.g. "claude-sonnet-4-6")
    #[serde(default)]
    pub model: Option<String>,
    /// The system prompt text
    pub system_prompt: String,
    /// Optional list of tool names this prompt expects to have access to
    #[serde(default)]
    pub tools: Vec<String>,
    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Serialize)]
pub struct PromptListEntry {
    pub name: String,
    pub description: String,
    pub model: Option<String>,
    pub tags: Vec<String>,
    pub file: String,
}

// ── Subcommand ────────────────────────────────────────

#[derive(Subcommand)]
pub enum Command {
    /// List all prompts in the prompts/ directory
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show a specific prompt (default: raw system_prompt text; --json for full struct)
    Show {
        /// Prompt name (filename without .yaml extension)
        name: String,
        /// Output as JSON (default is the raw system_prompt text)
        #[arg(long)]
        json: bool,
    },
    /// Scaffold a new prompt file
    Init {
        /// Prompt name
        name: String,
    },
}

// ── Handlers ──────────────────────────────────────────

pub fn run(command: Command, dir: &Path) -> Result<()> {
    match command {
        Command::List { json } => list(dir, json),
        Command::Show { name, json } => show(dir, &name, json),
        Command::Init { name } => init(dir, &name),
    }
}

fn prompts_dir(dir: &Path) -> std::path::PathBuf {
    dir.join("prompts")
}

fn prompt_path(dir: &Path, name: &str) -> std::path::PathBuf {
    prompts_dir(dir).join(format!("{}.yaml", name))
}

fn list(dir: &Path, json: bool) -> Result<()> {
    let pd = prompts_dir(dir);
    if !pd.exists() {
        if json {
            println!("[]");
        } else {
            println!("No prompts/ directory found.");
        }
        return Ok(());
    }

    let mut entries: Vec<PromptListEntry> = Vec::new();

    let mut paths: Vec<_> = fs::read_dir(&pd)
        .with_context(|| format!("reading prompts dir {:?}", pd))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x == "yaml")
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();
    paths.sort();

    for path in paths {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("reading {:?}", path))?;
        let prompt: Prompt = serde_yaml::from_str(&content)
            .with_context(|| format!("parsing {:?}", path))?;
        let file = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        entries.push(PromptListEntry {
            name: prompt.name,
            description: prompt.description,
            model: prompt.model,
            tags: prompt.tags,
            file,
        });
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        if entries.is_empty() {
            println!("No prompts found in prompts/");
        } else {
            for e in &entries {
                let tags = if e.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", e.tags.join(", "))
                };
                println!("{}{}", e.name, tags);
                println!("  {}", e.description);
                if let Some(ref m) = e.model {
                    println!("  model: {}", m);
                }
            }
        }
    }
    Ok(())
}

fn show(dir: &Path, name: &str, json: bool) -> Result<()> {
    let path = prompt_path(dir, name);
    if !path.exists() {
        bail!("prompt not found: prompts/{}.yaml", name);
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("reading {:?}", path))?;
    let prompt: Prompt = serde_yaml::from_str(&content)
        .with_context(|| format!("parsing {:?}", path))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&prompt)?);
    } else {
        print!("{}", prompt.system_prompt);
    }
    Ok(())
}

fn init(dir: &Path, name: &str) -> Result<()> {
    let pd = prompts_dir(dir);
    fs::create_dir_all(&pd)
        .with_context(|| format!("creating prompts dir {:?}", pd))?;

    let path = prompt_path(dir, name);
    if path.exists() {
        bail!("prompt already exists: prompts/{}.yaml", name);
    }

    // Try to read the site name from kazam.yaml for the scaffold
    let site_name = load_site_name(dir).unwrap_or_else(|| name.to_string());

    let scaffold = format!(
        "name: {name}\ndescription: \"\"\nsystem_prompt: |\n  You are an agent working on the {site_name} knowledge base.\n\n  ## Voice\n  <voice config will be injected here if available>\n\n  ## Your task\n  <describe what this agent should do>\ntools: []\ntags: []\n"
    );

    fs::write(&path, &scaffold)
        .with_context(|| format!("writing {:?}", path))?;

    println!("created prompts/{}.yaml", name);
    Ok(())
}

fn load_site_name(dir: &Path) -> Option<String> {
    let config_path = dir.join("kazam.yaml");
    let content = fs::read_to_string(config_path).ok()?;
    let val: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;
    val.get("name")?.as_str().map(|s| s.to_string())
}

// ── Tests ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir(suffix: &str) -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let d = std::env::temp_dir()
            .join(format!("kazam-prompts-test-{}-{}", suffix, ts));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_prompt_yaml(name: &str, desc: &str, system: &str) -> String {
        format!(
            "name: {}\ndescription: \"{}\"\nsystem_prompt: |\n  {}\ntools: []\ntags: []\n",
            name, desc, system
        )
    }

    #[test]
    fn parse_valid_prompt_yaml() {
        let yaml = r#"
name: writer
description: "Writes pages"
system_prompt: |
  You are a writer.
tools:
  - read_page
  - write_page
tags:
  - authoring
"#;
        let prompt: Prompt = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(prompt.name, "writer");
        assert_eq!(prompt.description, "Writes pages");
        assert!(prompt.model.is_none());
        assert_eq!(prompt.tools, vec!["read_page", "write_page"]);
        assert_eq!(prompt.tags, vec!["authoring"]);
        assert!(prompt.system_prompt.contains("You are a writer"));
    }

    #[test]
    fn parse_prompt_with_model() {
        let yaml = r#"
name: reviewer
description: "Reviews pages"
model: claude-sonnet-4-6
system_prompt: |
  You are a reviewer.
tools: []
tags: []
"#;
        let prompt: Prompt = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(prompt.model, Some("claude-sonnet-4-6".to_string()));
    }

    #[test]
    fn list_prompts_from_temp_dir() {
        let d = tmp_dir("list");
        let pd = d.join("prompts");
        fs::create_dir_all(&pd).unwrap();

        fs::write(
            pd.join("alpha.yaml"),
            make_prompt_yaml("alpha", "Alpha prompt", "Do alpha things."),
        )
        .unwrap();
        fs::write(
            pd.join("beta.yaml"),
            make_prompt_yaml("beta", "Beta prompt", "Do beta things."),
        )
        .unwrap();

        // Collect entries directly
        let mut paths: Vec<_> = fs::read_dir(&pd)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|x| x == "yaml")
                    .unwrap_or(false)
            })
            .map(|e| e.path())
            .collect();
        paths.sort();

        assert_eq!(paths.len(), 2);
        let names: Vec<_> = paths
            .iter()
            .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn show_specific_prompt() {
        let d = tmp_dir("show");
        let pd = d.join("prompts");
        fs::create_dir_all(&pd).unwrap();

        let yaml = "name: writer\ndescription: \"Writes pages\"\nsystem_prompt: |\n  You are a writer.\ntools: []\ntags: []\n";
        fs::write(pd.join("writer.yaml"), yaml).unwrap();

        let path = prompt_path(&d, "writer");
        let content = fs::read_to_string(&path).unwrap();
        let prompt: Prompt = serde_yaml::from_str(&content).unwrap();
        assert_eq!(prompt.name, "writer");
        assert!(prompt.system_prompt.contains("You are a writer"));
    }

    #[test]
    fn init_creates_valid_scaffold() {
        let d = tmp_dir("init");

        init(&d, "my-agent").unwrap();

        let path = prompt_path(&d, "my-agent");
        assert!(path.exists());

        let content = fs::read_to_string(&path).unwrap();
        let prompt: Prompt = serde_yaml::from_str(&content).unwrap();
        assert_eq!(prompt.name, "my-agent");
        assert!(prompt.system_prompt.contains("knowledge base"));
        assert_eq!(prompt.tools, Vec::<String>::new());
        assert_eq!(prompt.tags, Vec::<String>::new());
    }

    #[test]
    fn init_fails_if_already_exists() {
        let d = tmp_dir("init-dup");
        init(&d, "dup").unwrap();
        let result = init(&d, "dup");
        assert!(result.is_err());
    }

    #[test]
    fn list_empty_returns_ok() {
        let d = tmp_dir("list-empty");
        // No prompts/ directory at all — should not error
        let result = list(&d, true);
        assert!(result.is_ok());
    }
}

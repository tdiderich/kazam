use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::agents;
use crate::build::load_config;
use crate::types::Shell;

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
        .filter(|e| e.path().extension().map(|x| x == "yaml").unwrap_or(false))
        .map(|e| e.path())
        .collect();
    paths.sort();

    for path in paths {
        let content = fs::read_to_string(&path).with_context(|| format!("reading {:?}", path))?;
        let prompt: Prompt =
            serde_yaml::from_str(&content).with_context(|| format!("parsing {:?}", path))?;
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
    let content = fs::read_to_string(&path).with_context(|| format!("reading {:?}", path))?;
    let prompt: Prompt =
        serde_yaml::from_str(&content).with_context(|| format!("parsing {:?}", path))?;

    if json {
        println!("{}", serde_json::to_string_pretty(&prompt)?);
    } else {
        let expanded = expand_template_vars(&prompt.system_prompt, dir);
        print!("{}", expanded);
    }
    Ok(())
}

fn expand_template_vars(system_prompt: &str, dir: &Path) -> String {
    let mut out = system_prompt.to_string();

    if out.contains("{{config}}") {
        let replacement = build_config_json(dir);
        out = out.replace("{{config}}", &replacement);
    }

    if out.contains("{{voice}}") {
        let replacement = build_voice_text(dir);
        out = out.replace("{{voice}}", &replacement);
    }

    if out.contains("{{page_list}}") {
        let replacement = build_page_list(dir);
        out = out.replace("{{page_list}}", &replacement);
    }

    if out.contains("{{kazam_agents}}") {
        out = out.replace("{{kazam_agents}}", agents::AGENTS_MD);
    }

    out
}

fn build_config_json(dir: &Path) -> String {
    let config = match load_config(dir) {
        Ok(c) => c,
        Err(_) => return "{}".to_string(),
    };

    let roles: Vec<serde_json::Value> = config
        .roles
        .iter()
        .map(|r| serde_json::json!({ "id": r.id, "label": r.label }))
        .collect();

    let obj = serde_json::json!({
        "name": config.name,
        "theme": config.theme,
        "nav_layout": match config.nav_layout {
            crate::types::NavLayout::Top => "top",
            crate::types::NavLayout::Sidebar => "sidebar",
        },
        "roles": roles,
        "edit_url": config.edit_url,
        "url": config.url,
    });

    serde_json::to_string_pretty(&obj).unwrap_or_else(|_| "{}".to_string())
}

fn build_voice_text(dir: &Path) -> String {
    let config = match load_config(dir) {
        Ok(c) => c,
        Err(_) => return "No voice configuration defined.".to_string(),
    };

    let voice = match &config.voice {
        Some(v) => v,
        None => return "No voice configuration defined.".to_string(),
    };

    let mut lines = Vec::new();
    lines.push(format!("Voice configuration for \"{}\":", config.name));
    if let Some(tone) = &voice.tone {
        lines.push(format!("  Tone: {}", tone));
    }
    if let Some(level) = &voice.reading_level {
        lines.push(format!("  Reading level: {}", level));
    }
    if let Some(term) = &voice.terminology {
        if !term.prefer.is_empty() {
            let mut prefer: Vec<(&String, &String)> = term.prefer.iter().collect();
            prefer.sort_by_key(|(k, _)| *k);
            for (avoid, use_instead) in prefer {
                lines.push(format!("  Prefer: \"{}\" over \"{}\"", use_instead, avoid));
            }
        }
        if !term.avoid.is_empty() {
            lines.push(format!("  Avoid: {}", term.avoid.join(", ")));
        }
    }
    lines.join("\n")
}

fn build_page_list(dir: &Path) -> String {
    struct PageInfo {
        path: String,
        title: String,
        shell: String,
    }

    let mut pages: Vec<PageInfo> = Vec::new();

    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() > 0 && e.file_type().is_dir() {
                let name = e.file_name();
                if name == "_site" || name == "prompts" {
                    return false;
                }
            }
            if e.depth() > 0 {
                if let Some(name) = e.file_name().to_str() {
                    if name.starts_with('.') {
                        return false;
                    }
                }
            }
            true
        })
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }
        let fname = path.file_name().unwrap_or_default();
        if fname == "kazam.yaml" {
            continue;
        }
        if !path.extension().map(|e| e == "yaml").unwrap_or(false) {
            continue;
        }
        let rel = match path.strip_prefix(dir) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        // Parse only enough to get title + shell
        #[derive(serde::Deserialize)]
        struct PageMini {
            title: String,
            #[serde(default)]
            shell: Option<Shell>,
        }
        let page: PageMini = match serde_yaml::from_str(&content) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let shell_str = match page.shell {
            Some(Shell::Document) => "document",
            Some(Shell::Deck) => "deck",
            _ => "standard",
        };
        pages.push(PageInfo {
            path: rel,
            title: page.title,
            shell: shell_str.to_string(),
        });
    }

    pages.sort_by(|a, b| a.path.cmp(&b.path));

    pages
        .iter()
        .map(|p| format!("{}\t{}\t{}", p.path, p.title, p.shell))
        .collect::<Vec<_>>()
        .join("\n")
}

fn init(dir: &Path, name: &str) -> Result<()> {
    let pd = prompts_dir(dir);
    fs::create_dir_all(&pd).with_context(|| format!("creating prompts dir {:?}", pd))?;

    let path = prompt_path(dir, name);
    if path.exists() {
        bail!("prompt already exists: prompts/{}.yaml", name);
    }

    // Try to read the site name from kazam.yaml for the scaffold
    let site_name = load_site_name(dir).unwrap_or_else(|| name.to_string());

    let scaffold = format!(
        "name: {name}\ndescription: \"\"\nsystem_prompt: |\n  You are an agent working on the {site_name} knowledge base.\n\n  ## Voice\n  <voice config will be injected here if available>\n\n  ## Your task\n  <describe what this agent should do>\ntools: []\ntags: []\n"
    );

    fs::write(&path, &scaffold).with_context(|| format!("writing {:?}", path))?;

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
        let d = std::env::temp_dir().join(format!("kazam-prompts-test-{}-{}", suffix, ts));
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
            .filter(|e| e.path().extension().map(|x| x == "yaml").unwrap_or(false))
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
        // No prompts/ directory at all - should not error
        let result = list(&d, true);
        assert!(result.is_ok());
    }

    #[test]
    fn expand_template_vars_no_vars_passthrough() {
        let d = tmp_dir("expand-noop");
        let input = "You are an agent. Do things.";
        let out = expand_template_vars(input, &d);
        assert_eq!(out, input);
    }

    #[test]
    fn expand_template_vars_voice_with_config() {
        let d = tmp_dir("expand-voice");
        fs::write(
            d.join("kazam.yaml"),
            "name: TestSite\nvoice:\n  tone: \"direct\"\n  reading_level: \"senior engineer\"\n",
        )
        .unwrap();
        let out = expand_template_vars("Voice: {{voice}}", &d);
        assert!(out.contains("direct"), "expected tone in voice output");
        assert!(out.contains("senior engineer"));
        assert!(!out.contains("{{voice}}"));
    }

    #[test]
    fn expand_template_vars_voice_no_config() {
        let d = tmp_dir("expand-voice-none");
        fs::write(d.join("kazam.yaml"), "name: NoVoice\n").unwrap();
        let out = expand_template_vars("{{voice}}", &d);
        assert_eq!(out, "No voice configuration defined.");
    }

    #[test]
    fn expand_template_vars_config_contains_site_name() {
        let d = tmp_dir("expand-config");
        fs::write(d.join("kazam.yaml"), "name: AcmeCorp\n").unwrap();
        let out = expand_template_vars("Config: {{config}}", &d);
        assert!(
            out.contains("AcmeCorp"),
            "expected site name in config JSON"
        );
        assert!(!out.contains("{{config}}"));
    }

    #[test]
    fn expand_template_vars_page_list() {
        let d = tmp_dir("expand-pages");
        fs::write(d.join("kazam.yaml"), "name: MySite\n").unwrap();
        fs::write(
            d.join("index.yaml"),
            "title: Home\nshell: standard\ncomponents: []\n",
        )
        .unwrap();
        fs::write(
            d.join("about.yaml"),
            "title: About Us\nshell: document\ncomponents: []\n",
        )
        .unwrap();
        let out = expand_template_vars("Pages:\n{{page_list}}", &d);
        assert!(out.contains("Home"), "expected Home page title");
        assert!(out.contains("About Us"), "expected About Us page title");
        assert!(!out.contains("{{page_list}}"));
    }

    #[test]
    fn expand_template_vars_kazam_agents() {
        let d = tmp_dir("expand-agents");
        let out = expand_template_vars("{{kazam_agents}}", &d);
        assert!(!out.contains("{{kazam_agents}}"));
        assert!(!out.is_empty());
        // AGENTS_MD has content - verify we got something from the bundle
        assert_eq!(out, agents::AGENTS_MD);
    }
}

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const GITHUB_REPO: &str = "tdiderich/kazam";
const GITHUB_BRANCH: &str = "main";

const REGISTRY_YAML: &str = r#"
- name: hubspot-icp
  description: "Data-driven ICP from HubSpot deals + Apollo enrichment"
  tags: [gtm, hubspot, apollo, icp]
  path: wishes/hubspot-icp
"#;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegistryEntry {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LocalWish {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub data_sources: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubContent {
    pub name: String,
    #[serde(rename = "type")]
    pub content_type: String,
    pub download_url: Option<String>,
}

fn load_registry() -> Result<Vec<RegistryEntry>> {
    serde_yaml::from_str(REGISTRY_YAML).context("failed to parse embedded registry")
}

fn load_local_wishes() -> Result<Vec<LocalWish>> {
    let wishes_dir = PathBuf::from("wishes");
    let mut wishes = Vec::new();

    if !wishes_dir.exists() {
        return Ok(wishes);
    }

    for entry in fs::read_dir(&wishes_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let wish_yaml = path.join("wish.yaml");
            if wish_yaml.exists() {
                let contents = fs::read_to_string(&wish_yaml)
                    .with_context(|| format!("failed to read {}", wish_yaml.display()))?;
                match serde_yaml::from_str::<LocalWish>(&contents) {
                    Ok(w) => wishes.push(w),
                    Err(e) => eprintln!("warning: skipping {}: {}", wish_yaml.display(), e),
                }
            }
        }
    }

    Ok(wishes)
}

pub fn list(json: bool) -> Result<()> {
    let local = load_local_wishes()?;
    let registry = load_registry()?;

    let local_names: std::collections::HashSet<&str> =
        local.iter().map(|w| w.name.as_str()).collect();

    let registry_new: Vec<&RegistryEntry> = registry
        .iter()
        .filter(|e| !local_names.contains(e.name.as_str()))
        .collect();

    if json {
        #[derive(Serialize)]
        struct Output<'a> {
            local: &'a Vec<LocalWish>,
            registry: Vec<&'a RegistryEntry>,
        }
        let out = Output {
            local: &local,
            registry: registry_new,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    if local.is_empty() && registry_new.is_empty() {
        println!("  No wishes available.");
        return Ok(());
    }

    if !local.is_empty() {
        println!("LOCAL");
        for w in &local {
            let tags = w.tags.join(", ");
            println!("  {} — {} [{}]", w.name, w.description, tags);
        }
    }

    if !registry_new.is_empty() {
        println!("REGISTRY");
        for e in registry_new {
            let tags = e.tags.join(", ");
            println!("  {} — {} [{}]", e.name, e.description, tags);
        }
    }

    Ok(())
}

pub fn init(name: &str, dir: Option<PathBuf>, force: bool) -> Result<()> {
    let registry = load_registry()?;

    let entry = registry
        .iter()
        .find(|e| e.name == name)
        .with_context(|| format!("'{}' not found in registry", name))?;

    let dest = match dir {
        Some(d) => d,
        None => PathBuf::from("wishes").join(name),
    };

    if dest.exists() && !force {
        bail!(
            "'{}' already exists at {}. Use --force to overwrite.",
            name,
            dest.display()
        );
    }

    let api_url = format!(
        "https://api.github.com/repos/{}/contents/{}?ref={}",
        GITHUB_REPO, entry.path, GITHUB_BRANCH
    );

    let response = ureq::get(&api_url)
        .set("User-Agent", "kazam")
        .call()
        .with_context(|| format!("failed to fetch registry contents for '{}'", name))?;

    let body = response
        .into_string()
        .context("failed to read GitHub API response body")?;

    let files: Vec<GitHubContent> =
        serde_json::from_str(&body).context("failed to parse GitHub API response")?;

    fs::create_dir_all(&dest)
        .with_context(|| format!("failed to create directory {}", dest.display()))?;

    for file in files {
        if file.content_type != "file" {
            continue;
        }
        let download_url = match file.download_url {
            Some(u) => u,
            None => continue,
        };

        let content = ureq::get(&download_url)
            .set("User-Agent", "kazam")
            .call()
            .with_context(|| format!("failed to download {}", file.name))?
            .into_string()
            .with_context(|| format!("failed to read body for {}", file.name))?;

        let out_path = dest.join(&file.name);
        fs::write(&out_path, &content)
            .with_context(|| format!("failed to write {}", out_path.display()))?;

        println!("  wrote {}", out_path.display());
    }

    println!(
        "\nInstalled '{}' to {}.\nSee {}/README.md to get started.",
        name,
        dest.display(),
        dest.display()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parses() {
        let entries = load_registry().unwrap();
        assert!(!entries.is_empty());
        assert!(entries.iter().any(|e| e.name == "hubspot-icp"));
    }

    #[test]
    fn registry_entries_have_required_fields() {
        for entry in load_registry().unwrap() {
            assert!(!entry.name.is_empty());
            assert!(!entry.description.is_empty());
            assert!(!entry.path.is_empty());
            assert!(entry.path.starts_with("wishes/"));
        }
    }

    #[test]
    fn init_rejects_unknown_wish() {
        let result = init("nonexistent-wish", None, false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not found in registry"));
    }
}

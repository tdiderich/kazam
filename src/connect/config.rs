//! Config resolution for `kazam connect`: host-level `~/.kazam/connect.yaml`,
//! per-connector `.env` secrets, shell environment fallback, and the
//! per-connector `.state.yaml` sync-state file.
//!
//! Resolution order for `{{VAR}}` template placeholders (highest priority
//! first): connector `.env` -> host config's arbitrary top-level keys ->
//! shell environment. See `connectors/CONNECT_SPEC.md`'s "Config Resolution".

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
pub struct HostConfig {
    #[serde(default)]
    pub curata_url: Option<String>,
    #[serde(default)]
    pub curata_token: Option<String>,
    #[serde(default)]
    pub default_target: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

fn host_config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".kazam").join("connect.yaml"))
}

pub fn load_host_config() -> HostConfig {
    let Some(path) = host_config_path() else {
        return HostConfig::default();
    };
    match fs::read_to_string(&path) {
        Ok(s) => serde_yaml::from_str(&s).unwrap_or_default(),
        Err(_) => HostConfig::default(),
    }
}

/// Parse a simple `KEY=VALUE` `.env` file. No escaping beyond stripping
/// matching surrounding quotes; blank lines and `#` comments are skipped.
pub fn load_env_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(content) = fs::read_to_string(path) else {
        return map;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim().to_string();
            let mut v = v.trim().to_string();
            if v.len() >= 2
                && ((v.starts_with('"') && v.ends_with('"'))
                    || (v.starts_with('\'') && v.ends_with('\'')))
            {
                v = v[1..v.len() - 1].to_string();
            }
            map.insert(k, v);
        }
    }
    map
}

pub struct ConnectorEnv {
    pub vars: HashMap<String, String>,
}

impl ConnectorEnv {
    pub fn load(connector_dir: &Path) -> Self {
        Self {
            vars: load_env_file(&connector_dir.join(".env")),
        }
    }

    /// Resolve every `{{VAR}}` placeholder in `template` through the
    /// connector `.env` -> host config -> shell env chain. Errors if any
    /// variable can't be resolved anywhere in the chain.
    pub fn resolve(&self, template: &str, host: &HostConfig) -> Result<String> {
        let mut out = template.to_string();
        while let Some(start) = out.find("{{") {
            let end = out[start..]
                .find("}}")
                .map(|i| start + i)
                .context("unterminated {{ in template")?;
            let name = out[start + 2..end].trim().to_string();
            let value = self
                .vars
                .get(&name)
                .cloned()
                .or_else(|| {
                    host.extra
                        .get(&name)
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                })
                .or_else(|| std::env::var(&name).ok())
                .with_context(|| {
                    format!(
                        "missing config value for template var '{{{{{}}}}}' - set it in \
                         connectors/<vendor>/.env, ~/.kazam/connect.yaml, or the shell environment",
                        name
                    )
                })?;
            out.replace_range(start..end + 2, &value);
        }
        Ok(out)
    }
}

/// Per-connector sync state, persisted at `connectors/<vendor>/.state.yaml`.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct State {
    #[serde(default)]
    pub last_sync: Option<String>,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default)]
    pub page_created: bool,
    #[serde(default)]
    pub pull_counts: HashMap<String, usize>,
    #[serde(default)]
    pub confirmed_base_url: Option<String>,
}

impl State {
    pub fn path(connector_dir: &Path) -> PathBuf {
        connector_dir.join(".state.yaml")
    }

    pub fn load(connector_dir: &Path) -> Self {
        let path = Self::path(connector_dir);
        match fs::read_to_string(&path) {
            Ok(s) => serde_yaml::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, connector_dir: &Path) -> Result<()> {
        if let Some(parent) = Self::path(connector_dir).parent() {
            fs::create_dir_all(parent).ok();
        }
        let yaml = serde_yaml::to_string(self)?;
        fs::write(Self::path(connector_dir), yaml)
            .with_context(|| format!("failed to write state for {}", connector_dir.display()))
    }
}

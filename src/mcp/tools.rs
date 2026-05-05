use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;
use walkdir::WalkDir;

use crate::types::{Page, RefreshMode, RefreshStep, RefreshValue, Shell, SiteConfig};

use super::protocol::ToolResult;

// ── Tool definitions (JSON Schema) ──────────────────────

pub fn tool_definitions() -> Vec<super::protocol::Tool> {
    vec![
        super::protocol::Tool {
            name: "read_page".into(),
            description: "Read a page's YAML content and metadata. Returns raw YAML plus parsed metadata (title, shell, freshness).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to the YAML file from the site root (e.g. 'customers/acme.yaml')"
                    }
                },
                "required": ["path"]
            }),
        },
        super::protocol::Tool {
            name: "list_pages".into(),
            description: "List all pages in the site or a subdirectory. Returns an array of objects with path, title, shell, and has_freshness.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "directory": {
                        "type": "string",
                        "description": "Optional subdirectory to list (e.g. 'customers'). Omit to list all pages."
                    }
                },
                "required": []
            }),
        },
        super::protocol::Tool {
            name: "get_config".into(),
            description: "Read the site configuration (kazam.yaml) as JSON.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        super::protocol::Tool {
            name: "search".into(),
            description: "Text search across page YAML content (case-insensitive, literal match). Returns matches with file path, title, and line context. If a query returns no results, try synonyms or broader terms (e.g. 'family leave' instead of 'maternity', 'PTO' instead of 'vacation'). Also try list_pages to browse by directory.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Text to search for (case-insensitive)"
                    }
                },
                "required": ["query"]
            }),
        },
        super::protocol::Tool {
            name: "write_page".into(),
            description: "Write or update a page YAML file. Validates the content parses as a valid Page before writing. Requires --allow-writes to be set.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative path to write (e.g. 'docs/auth.yaml')"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full YAML content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        },
    ]
}

// ── Helpers ───────────────────────────────────────────────

fn serialize_refresh(refresh: &Option<RefreshValue>) -> serde_json::Value {
    use serde_json::json;
    match refresh {
        None => serde_json::Value::Null,
        Some(RefreshValue::Prompt(s)) => json!(s),
        Some(RefreshValue::Full(config)) => {
            let mode = match config.mode {
                RefreshMode::Human => "human",
                RefreshMode::Auto => "auto",
                RefreshMode::Assisted => "assisted",
            };
            let steps: Vec<serde_json::Value> = config
                .steps
                .iter()
                .map(|s| match s {
                    RefreshStep::Run(v) => json!({"run": v}),
                    RefreshStep::Prompt(v) => json!({"prompt": v}),
                    RefreshStep::Review(v) => json!({"review": v}),
                })
                .collect();
            json!({
                "mode": mode,
                "steps": steps,
            })
        }
    }
}

// ── Tool implementations ─────────────────────────────────

pub fn read_page(dir: &Path, params: &Value) -> Result<ToolResult> {
    let path_str = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: path"))?;

    // Sanitize: reject paths that escape the site directory.
    if path_str.contains("..") {
        return Ok(ToolResult::error("path must not contain '..'"));
    }

    let full_path = dir.join(path_str);
    if !full_path.exists() {
        return Ok(ToolResult::error(format!("file not found: {}", path_str)));
    }

    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| anyhow::anyhow!("reading {}: {}", path_str, e))?;

    // Parse for metadata extraction; surface parse errors as tool errors.
    let page_result: Result<Page, _> = serde_yaml::from_str(&content);
    let metadata = match page_result {
        Ok(page) => json!({
            "title": page.title,
            "shell": shell_name(page.shell).to_string(),
            "has_freshness": page.freshness.is_some(),
            "unlisted": page.unlisted,
            "freshness": page.freshness.as_ref().and_then(|fv| fv.as_full()).map(|f| json!({
                "owner": f.owner,
                "updated": f.updated,
                "review_every": f.review_every,
                "refresh": serialize_refresh(&f.refresh),
            })),
        }),
        Err(e) => json!({ "parse_error": e.to_string() }),
    };

    let output = json!({
        "path": path_str,
        "raw_yaml": content,
        "metadata": metadata,
    });

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

pub fn list_pages(dir: &Path, params: &Value) -> Result<ToolResult> {
    let subdir = params
        .get("directory")
        .and_then(Value::as_str)
        .unwrap_or("");

    let search_root = if subdir.is_empty() {
        dir.to_path_buf()
    } else {
        if subdir.contains("..") {
            return Ok(ToolResult::error("directory must not contain '..'"));
        }
        dir.join(subdir)
    };

    if !search_root.exists() {
        return Ok(ToolResult::error(format!(
            "directory not found: {}",
            subdir
        )));
    }

    let mut pages = Vec::new();

    for entry in WalkDir::new(&search_root)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() > 0 {
                if let Some(name) = e.file_name().to_str() {
                    if name.starts_with('.') {
                        return false;
                    }
                }
            }
            true
        })
        .flatten()
    {
        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }

        let fname = path.file_name().unwrap_or_default();
        if fname == "kazam.yaml" || fname == "404.yaml" {
            continue;
        }

        if path.extension().map(|e| e == "yaml").unwrap_or(false) {
            let rel = path.strip_prefix(dir).unwrap_or(path);
            let rel_str = rel.to_string_lossy().replace('\\', "/");

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let (title, shell_str, has_freshness) = match serde_yaml::from_str::<Page>(&content) {
                Ok(page) => (
                    page.title.clone(),
                    shell_name(page.shell).to_string(),
                    page.freshness.is_some(),
                ),
                Err(_) => ("(unparseable)".into(), "unknown".into(), false),
            };

            pages.push(json!({
                "path": rel_str,
                "title": title,
                "shell": shell_str,
                "has_freshness": has_freshness,
            }));
        }
    }

    Ok(ToolResult::text(serde_json::to_string_pretty(&pages)?))
}

pub fn get_config(dir: &Path) -> Result<ToolResult> {
    let config_path = dir.join("kazam.yaml");
    if !config_path.exists() {
        // Return the default config as JSON
        let default = SiteConfig::default();
        let as_json = json!({
            "name": default.name,
            "theme": default.theme,
            "nav": null,
            "view_source": default.view_source,
            "_note": "No kazam.yaml found; showing defaults"
        });
        return Ok(ToolResult::text(serde_json::to_string_pretty(&as_json)?));
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| anyhow::anyhow!("reading kazam.yaml: {}", e))?;

    // Parse YAML then round-trip through serde_json Value for clean output.
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| anyhow::anyhow!("parsing kazam.yaml: {}", e))?;

    let json_value = yaml_to_json(yaml_value);
    Ok(ToolResult::text(serde_json::to_string_pretty(&json_value)?))
}

pub fn search(dir: &Path, params: &Value) -> Result<ToolResult> {
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: query"))?;

    if query.is_empty() {
        return Ok(ToolResult::error("query must not be empty"));
    }

    let query_lower = query.to_lowercase();
    let mut matches = Vec::new();

    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() > 0 {
                if let Some(name) = e.file_name().to_str() {
                    if name.starts_with('.') {
                        return false;
                    }
                }
            }
            true
        })
        .flatten()
    {
        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }

        let fname = path.file_name().unwrap_or_default();
        if fname == "kazam.yaml" || fname == "404.yaml" {
            continue;
        }

        if !path.extension().map(|e| e == "yaml").unwrap_or(false) {
            continue;
        }

        let rel = path.strip_prefix(dir).unwrap_or(path);
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let title = serde_yaml::from_str::<Page>(&content)
            .map(|p| p.title)
            .unwrap_or_else(|_| "(unparseable)".into());

        let mut matching_lines = Vec::new();
        for (line_num, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                matching_lines.push(json!({
                    "line": line_num + 1,
                    "text": line.trim(),
                }));
            }
        }

        if !matching_lines.is_empty() {
            matches.push(json!({
                "path": rel_str,
                "title": title,
                "matches": matching_lines,
            }));
        }
    }

    Ok(ToolResult::text(serde_json::to_string_pretty(&matches)?))
}

pub fn write_page(dir: &Path, params: &Value, allow_writes: bool) -> Result<ToolResult> {
    if !allow_writes {
        return Ok(ToolResult::error(
            "write_page is disabled; restart kazam mcp with --allow-writes to enable it",
        ));
    }

    let path_str = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: path"))?;

    let content = params
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: content"))?;

    if path_str.contains("..") {
        return Ok(ToolResult::error("path must not contain '..'"));
    }

    // Validate it parses as a Page before writing anything.
    if let Err(e) = serde_yaml::from_str::<Page>(content) {
        return Ok(ToolResult::error(format!("invalid Page YAML: {}", e)));
    }

    let full_path = dir.join(path_str);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("creating directories: {}", e))?;
    }

    std::fs::write(&full_path, content)
        .map_err(|e| anyhow::anyhow!("writing {}: {}", path_str, e))?;

    Ok(ToolResult::text(format!(
        "{{\"ok\": true, \"path\": \"{}\"}}",
        path_str
    )))
}

fn shell_name(shell: Shell) -> &'static str {
    match shell {
        Shell::Standard => "standard",
        Shell::Document => "document",
        Shell::Deck => "deck",
    }
}

// ── YAML → JSON conversion ───────────────────────────────

fn yaml_to_json(value: serde_yaml::Value) -> Value {
    match value {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i.into())
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        }
        serde_yaml::Value::String(s) => Value::String(s),
        serde_yaml::Value::Sequence(seq) => {
            Value::Array(seq.into_iter().map(yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                let key = match k {
                    serde_yaml::Value::String(s) => s,
                    other => format!("{:?}", other),
                };
                obj.insert(key, yaml_to_json(v));
            }
            Value::Object(obj)
        }
        serde_yaml::Value::Tagged(t) => yaml_to_json(t.value),
    }
}

// ── Tests ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_temp_site() -> TempDir {
        let dir = tempfile::tempdir().unwrap();

        fs::write(
            dir.path().join("kazam.yaml"),
            "name: Test Site\ntheme: dark\n",
        )
        .unwrap();

        fs::write(
            dir.path().join("index.yaml"),
            "title: Home\nshell: standard\ncomponents:\n  - type: header\n    title: Welcome\n",
        )
        .unwrap();

        let sub = dir.path().join("docs");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            sub.join("auth.yaml"),
            "title: Auth Guide\nshell: document\ncomponents:\n  - type: markdown\n    body: Authentication docs\n",
        )
        .unwrap();

        dir
    }

    #[test]
    fn read_page_returns_content() {
        let site = make_temp_site();
        let result = read_page(site.path(), &json!({"path": "index.yaml"})).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = &result.content[0].text;
        assert!(text.contains("Home"));
        assert!(text.contains("raw_yaml"));
    }

    #[test]
    fn read_page_rejects_path_traversal() {
        let site = make_temp_site();
        let result = read_page(site.path(), &json!({"path": "../etc/passwd"})).unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn read_page_missing_file() {
        let site = make_temp_site();
        let result = read_page(site.path(), &json!({"path": "missing.yaml"})).unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn list_pages_returns_all() {
        let site = make_temp_site();
        let result = list_pages(site.path(), &json!({})).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = &result.content[0].text;
        assert!(text.contains("index.yaml"));
        assert!(text.contains("docs/auth.yaml"));
    }

    #[test]
    fn list_pages_subdir() {
        let site = make_temp_site();
        let result = list_pages(site.path(), &json!({"directory": "docs"})).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = &result.content[0].text;
        assert!(text.contains("docs/auth.yaml"));
        assert!(!text.contains("index.yaml"));
    }

    #[test]
    fn list_pages_rejects_traversal() {
        let site = make_temp_site();
        let result = list_pages(site.path(), &json!({"directory": "../other"})).unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn get_config_returns_json() {
        let site = make_temp_site();
        let result = get_config(site.path()).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = &result.content[0].text;
        assert!(text.contains("Test Site"));
    }

    #[test]
    fn get_config_no_kazam_yaml_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let result = get_config(dir.path()).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = &result.content[0].text;
        assert!(text.contains("_note"));
    }

    #[test]
    fn search_finds_matches() {
        let site = make_temp_site();
        let result = search(site.path(), &json!({"query": "authentication"})).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = &result.content[0].text;
        assert!(text.contains("auth.yaml"));
    }

    #[test]
    fn search_case_insensitive() {
        let site = make_temp_site();
        let result = search(site.path(), &json!({"query": "AUTHENTICATION"})).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = &result.content[0].text;
        assert!(text.contains("auth.yaml"));
    }

    #[test]
    fn search_no_results() {
        let site = make_temp_site();
        let result = search(site.path(), &json!({"query": "xyzzy_not_found"})).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        assert!(result.content[0].text.contains("[]"));
    }

    #[test]
    fn write_page_blocked_without_allow_writes() {
        let site = make_temp_site();
        let result = write_page(
            site.path(),
            &json!({"path": "new.yaml", "content": "title: X\nshell: standard\n"}),
            false,
        )
        .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("--allow-writes"));
    }

    #[test]
    fn write_page_rejects_invalid_yaml() {
        let site = make_temp_site();
        let result = write_page(
            site.path(),
            &json!({"path": "new.yaml", "content": "not valid page yaml: [[["}),
            true,
        )
        .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("invalid Page YAML"));
    }

    #[test]
    fn write_page_rejects_traversal() {
        let site = make_temp_site();
        let result = write_page(
            site.path(),
            &json!({"path": "../outside.yaml", "content": "title: X\nshell: standard\n"}),
            true,
        )
        .unwrap();
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn write_page_creates_valid_page() {
        let site = make_temp_site();
        let content =
            "title: New Page\nshell: document\ncomponents:\n  - type: markdown\n    body: Hello\n";
        let result = write_page(
            site.path(),
            &json!({"path": "new.yaml", "content": content}),
            true,
        )
        .unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let written = fs::read_to_string(site.path().join("new.yaml")).unwrap();
        assert_eq!(written, content);
    }
}

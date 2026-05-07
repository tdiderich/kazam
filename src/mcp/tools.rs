use anyhow::Result;
use serde_json::{json, Value};
use std::path::Path;
use walkdir::WalkDir;

use crate::annotations;
use crate::types::{
    Annotation, AnnotationSource, AnnotationStatus, Page, RefreshMode, RefreshStep, RefreshValue,
    Shell, SiteConfig,
};

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
        super::protocol::Tool {
            name: "annotate_page".into(),
            description: "Add an annotation to a page. Creates a sidecar YAML file in .kazam/annotations/<page-slug>/. Requires --allow-writes.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "page": {
                        "type": "string",
                        "description": "Relative path to the page YAML file (e.g. 'deals/acme.yaml')"
                    },
                    "text": {
                        "type": "string",
                        "description": "Annotation text"
                    },
                    "author": {
                        "type": "string",
                        "description": "Who is writing this annotation"
                    },
                    "section": {
                        "type": "string",
                        "description": "Optional section this annotation applies to (e.g. 'competitive', 'timeline')"
                    },
                    "source": {
                        "type": "string",
                        "enum": ["cli", "agent", "web"],
                        "description": "Where this annotation came from (default: agent)"
                    }
                },
                "required": ["page", "text", "author"]
            }),
        },
        super::protocol::Tool {
            name: "update_annotation".into(),
            description: "Update an annotation's status (e.g. after a refresh incorporates or ignores it). Requires --allow-writes.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "page": {
                        "type": "string",
                        "description": "Relative path to the page YAML file (e.g. 'deals/acme.yaml')"
                    },
                    "annotation_id": {
                        "type": "string",
                        "description": "Annotation ID (e.g. 'ann-20260507-a3f2')"
                    },
                    "status": {
                        "type": "string",
                        "enum": ["pending", "incorporated", "ignored", "stale"],
                        "description": "New status for the annotation"
                    }
                },
                "required": ["page", "annotation_id", "status"]
            }),
        },
        super::protocol::Tool {
            name: "list_annotations".into(),
            description: "List all annotations for a page. Returns an array of annotation objects from .kazam/annotations/<page-slug>/.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "page": {
                        "type": "string",
                        "description": "Relative path to the page YAML file (e.g. 'deals/acme.yaml')"
                    }
                },
                "required": ["page"]
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

pub fn annotate_page(dir: &Path, params: &Value, allow_writes: bool) -> Result<ToolResult> {
    if !allow_writes {
        return Ok(ToolResult::error(
            "annotate_page is disabled; restart kazam mcp with --allow-writes to enable it",
        ));
    }

    let page = params
        .get("page")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: page"))?;
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: text"))?;
    let author = params
        .get("author")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: author"))?;

    if page.contains("..") {
        return Ok(ToolResult::error("page path must not contain '..'"));
    }

    let full_path = dir.join(page);
    if !full_path.exists() {
        return Ok(ToolResult::error(format!("page not found: {}", page)));
    }

    let section = params
        .get("section")
        .and_then(Value::as_str)
        .map(String::from);
    let source = match params.get("source").and_then(Value::as_str) {
        Some("cli") => AnnotationSource::Cli,
        Some("web") => AnnotationSource::Web,
        Some("agent") | None => AnnotationSource::Agent,
        Some(other) => {
            return Ok(ToolResult::error(format!(
                "invalid source '{}': expected cli, agent, or web",
                other
            )));
        }
    };

    let today = crate::freshness::today_iso();
    let id = annotations::generate_id(&today);
    let ann = Annotation {
        id: id.clone(),
        text: text.to_string(),
        author: author.to_string(),
        section,
        added: today,
        status: AnnotationStatus::Pending,
        source,
    };

    let slug = annotations::page_slug_from_path(page);
    let path = annotations::save_annotation(dir, &slug, &ann)?;

    let output = json!({
        "ok": true,
        "id": id,
        "path": path.strip_prefix(dir).unwrap_or(&path).to_string_lossy(),
    });
    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

pub fn update_annotation(dir: &Path, params: &Value, allow_writes: bool) -> Result<ToolResult> {
    if !allow_writes {
        return Ok(ToolResult::error(
            "update_annotation is disabled; restart kazam mcp with --allow-writes to enable it",
        ));
    }

    let page = params
        .get("page")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: page"))?;
    let annotation_id = params
        .get("annotation_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: annotation_id"))?;
    let status_str = params
        .get("status")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: status"))?;

    if page.contains("..") {
        return Ok(ToolResult::error("page path must not contain '..'"));
    }

    let new_status = match status_str {
        "pending" => AnnotationStatus::Pending,
        "incorporated" => AnnotationStatus::Incorporated,
        "ignored" => AnnotationStatus::Ignored,
        "stale" => AnnotationStatus::Stale,
        other => {
            return Ok(ToolResult::error(format!(
                "invalid status '{}': expected pending, incorporated, ignored, or stale",
                other
            )));
        }
    };

    let slug = annotations::page_slug_from_path(page);
    let ann_dir = annotations::annotations_dir(dir, &slug);
    let ann_path = ann_dir.join(format!("{}.yaml", annotation_id));

    if !ann_path.exists() {
        return Ok(ToolResult::error(format!(
            "annotation not found: {}",
            annotation_id
        )));
    }

    let content = std::fs::read_to_string(&ann_path)
        .map_err(|e| anyhow::anyhow!("reading annotation: {}", e))?;
    let mut ann: Annotation =
        serde_yaml::from_str(&content).map_err(|e| anyhow::anyhow!("parsing annotation: {}", e))?;

    ann.status = new_status;
    let yaml = serde_yaml::to_string(&ann).map_err(|e| anyhow::anyhow!("serializing: {}", e))?;
    std::fs::write(&ann_path, yaml).map_err(|e| anyhow::anyhow!("writing annotation: {}", e))?;

    let output = json!({
        "ok": true,
        "id": annotation_id,
        "status": status_str,
    });
    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

pub fn list_annotations(dir: &Path, params: &Value) -> Result<ToolResult> {
    let page = params
        .get("page")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: page"))?;

    if page.contains("..") {
        return Ok(ToolResult::error("page path must not contain '..'"));
    }

    let slug = annotations::page_slug_from_path(page);
    let anns = annotations::load_annotations(dir, &slug)?;

    let items: Vec<Value> = anns
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "text": a.text,
                "author": a.author,
                "section": a.section,
                "added": a.added,
                "status": format!("{:?}", a.status).to_lowercase(),
                "source": format!("{:?}", a.source).to_lowercase(),
            })
        })
        .collect();

    Ok(ToolResult::text(serde_json::to_string_pretty(&items)?))
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

    #[test]
    fn annotate_page_creates_sidecar_file() {
        let site = make_temp_site();
        let result = annotate_page(
            site.path(),
            &json!({
                "page": "index.yaml",
                "text": "Test annotation",
                "author": "tyler",
                "section": "competitive",
                "source": "agent"
            }),
            true,
        )
        .unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = &result.content[0].text;
        assert!(text.contains("\"ok\": true"));
        assert!(text.contains("ann-"));
    }

    #[test]
    fn annotate_page_blocked_without_allow_writes() {
        let site = make_temp_site();
        let result = annotate_page(
            site.path(),
            &json!({"page": "index.yaml", "text": "note", "author": "tyler"}),
            false,
        )
        .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("--allow-writes"));
    }

    #[test]
    fn annotate_page_missing_page() {
        let site = make_temp_site();
        let result = annotate_page(
            site.path(),
            &json!({"page": "nonexistent.yaml", "text": "note", "author": "tyler"}),
            true,
        )
        .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("not found"));
    }

    #[test]
    fn list_annotations_empty_page() {
        let site = make_temp_site();
        let result = list_annotations(site.path(), &json!({"page": "index.yaml"})).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        assert!(result.content[0].text.contains("[]"));
    }

    #[test]
    fn update_annotation_status() {
        let site = make_temp_site();
        // Create an annotation first
        let create_result = annotate_page(
            site.path(),
            &json!({
                "page": "index.yaml",
                "text": "Status update test",
                "author": "tyler",
            }),
            true,
        )
        .unwrap();
        let create_text = &create_result.content[0].text;
        let create_json: serde_json::Value = serde_json::from_str(create_text).unwrap();
        let ann_id = create_json["id"].as_str().unwrap();

        // Update status to incorporated
        let result = update_annotation(
            site.path(),
            &json!({
                "page": "index.yaml",
                "annotation_id": ann_id,
                "status": "incorporated",
            }),
            true,
        )
        .unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = &result.content[0].text;
        assert!(text.contains("incorporated"));

        // Verify via list
        let list_result = list_annotations(site.path(), &json!({"page": "index.yaml"})).unwrap();
        let list_text = &list_result.content[0].text;
        assert!(list_text.contains("incorporated"));
    }

    #[test]
    fn update_annotation_not_found() {
        let site = make_temp_site();
        let result = update_annotation(
            site.path(),
            &json!({
                "page": "index.yaml",
                "annotation_id": "ann-20260507-0000",
                "status": "incorporated",
            }),
            true,
        )
        .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("not found"));
    }

    #[test]
    fn annotate_then_list_round_trip() {
        let site = make_temp_site();
        annotate_page(
            site.path(),
            &json!({
                "page": "index.yaml",
                "text": "Round-trip test",
                "author": "tyler",
            }),
            true,
        )
        .unwrap();
        let result = list_annotations(site.path(), &json!({"page": "index.yaml"})).unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = &result.content[0].text;
        assert!(text.contains("Round-trip test"));
        assert!(text.contains("tyler"));
        assert!(text.contains("pending"));
    }
}

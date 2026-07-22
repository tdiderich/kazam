//! `kazam install` — fetch an AI tool pack page from a curata instance and
//! compile it into local AI tool config files (CLAUDE.md + .cursorrules).
//!
//! A pack is an ordinary curata page (usually created from the `ai-tool-pack`
//! template). Its markdown components, concatenated in order, become the rules
//! text. The text is written inside managed marker blocks so reinstalls are
//! idempotent and user content outside the block is never touched.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

const TARGETS: [&str; 2] = ["CLAUDE.md", ".cursorrules"];

#[derive(Deserialize)]
struct PackPage {
    title: String,
    #[serde(default)]
    pack: Option<InstallPackMeta>,
    #[serde(default)]
    components: Vec<PackComponent>,
}

#[derive(Deserialize)]
struct InstallPackMeta {
    #[serde(default)]
    targets: Vec<String>,
}

/// Permissive component view: we only care about markdown bodies and
/// containers that nest more components. Everything else is ignored so any
/// curata page (whose schema is a superset of kazam's) parses cleanly.
#[derive(Deserialize)]
struct PackComponent {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    components: Option<Vec<PackComponent>>,
}

/// Split a pack URL into (instance base URL, page slug).
///
/// Accepted forms (scheme optional, defaults to https):
///   https://host/pages/<slug>
///   https://host/<prefix...>/pages/<slug>
///   https://host/p/<org>/<slug>
///   https://host/<slug>
fn parse_pack_url(input: &str) -> Result<(String, String)> {
    let url = if input.starts_with("http://") || input.starts_with("https://") {
        input.to_string()
    } else {
        format!("https://{}", input)
    };

    let scheme_end = url.find("://").unwrap() + 3;
    let (host, path) = match url[scheme_end..].find('/') {
        Some(i) => (&url[..scheme_end + i], &url[scheme_end + i..]),
        None => (url.as_str(), ""),
    };

    // Strip query string / fragment, split into segments.
    let path = path.split(['?', '#']).next().unwrap_or("");
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if segs.is_empty() {
        bail!(
            "no page slug in '{}' — expected <instance>/pages/<slug> or <instance>/<slug>",
            input
        );
    }

    let slug = (*segs.last().unwrap()).to_string();

    // Base = host + any path prefix before the routing marker.
    let prefix_len = if let Some(i) = segs.iter().rposition(|s| *s == "pages") {
        i
    } else if segs.len() >= 3 && segs[segs.len() - 3] == "p" {
        segs.len() - 3
    } else {
        segs.len() - 1
    };

    let mut base = host.to_string();
    for seg in &segs[..prefix_len] {
        base.push('/');
        base.push_str(seg);
    }

    Ok((base, slug))
}

/// Depth-first collection of markdown component bodies, in document order.
fn collect_markdown(components: &[PackComponent], out: &mut Vec<String>) {
    for c in components {
        if c.kind == "markdown" {
            if let Some(body) = &c.body {
                let trimmed = body.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
            }
        }
        if let Some(children) = &c.components {
            collect_markdown(children, out);
        }
    }
}

fn start_marker(slug: &str) -> String {
    format!("<!-- kazam-pack:start {} -->", slug)
}

fn end_marker(slug: &str) -> String {
    format!("<!-- kazam-pack:end {} -->", slug)
}

/// Build the full managed block for a pack.
fn render_block(
    slug: &str,
    source: &str,
    hash: &str,
    date: &str,
    title: &str,
    rules: &str,
) -> String {
    format!(
        "{}\n<!-- source: {} | hash: {} | installed: {} -->\n\n# Pack: {}\n\n{}\n\n{}",
        start_marker(slug),
        source,
        hash,
        date,
        title,
        rules,
        end_marker(slug)
    )
}

/// Insert or replace this pack's managed block, leaving all other content
/// (including other packs' blocks) byte-identical.
fn upsert_block(existing: Option<&str>, slug: &str, block: &str) -> Result<String> {
    let start = start_marker(slug);
    let end = end_marker(slug);

    match existing {
        None => Ok(format!("{}\n", block)),
        Some(text) => {
            if let Some(s) = text.find(&start) {
                let e = text[s..].find(&end).with_context(|| {
                    format!(
                        "found '{}' but no matching end marker — file corrupt, fix by hand",
                        start
                    )
                })?;
                let after = s + e + end.len();
                Ok(format!("{}{}{}", &text[..s], block, &text[after..]))
            } else {
                let trimmed = text.trim_end();
                if trimmed.is_empty() {
                    Ok(format!("{}\n", block))
                } else {
                    Ok(format!("{}\n\n{}\n", trimmed, block))
                }
            }
        }
    }
}

const AUTH_HINT: &str = "pass --api-key or set KAZAM_CURATA_API_KEY. \
On tailscale-auth instances, use the https:// Tailscale-served URL.";

/// Pull yaml + contentHash out of a read_page result object.
fn extract_page(result: &serde_json::Value, slug: &str) -> Result<(String, String)> {
    let yaml = result
        .get("yaml")
        .and_then(|y| y.as_str())
        .with_context(|| format!("read_page result for '{}' missing 'yaml'", slug))?
        .to_string();
    let hash = result
        .get("contentHash")
        .and_then(|h| h.as_str())
        .unwrap_or("unknown")
        .to_string();
    Ok((yaml, hash))
}

/// Streamable-HTTP MCP responses may arrive as plain JSON or SSE frames
/// (`event: message` / `data: {...}`). Return the first JSON-RPC message.
fn parse_sse_or_json(text: &str) -> Result<serde_json::Value> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        return serde_json::from_str(trimmed).context("response was not valid JSON");
    }
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                return Ok(v);
            }
        }
    }
    bail!("response was neither JSON nor an SSE message stream")
}

fn build_request(endpoint: &str, api_key: Option<&str>) -> ureq::Request {
    let mut req = ureq::post(endpoint)
        .set("User-Agent", "kazam")
        .set("Content-Type", "application/json")
        .set("Accept", "application/json, text/event-stream");
    if let Some(key) = api_key {
        req = req.set("Authorization", &format!("Bearer {}", key));
    }
    req
}

/// Fetch via the plain REST shim: POST {base}/api/mcp {"tool","args"}.
/// Ok(None) means the endpoint isn't there (fall back to the MCP stream route).
fn fetch_rest(base: &str, slug: &str, api_key: Option<&str>) -> Result<Option<(String, String)>> {
    let endpoint = format!("{}/api/mcp", base);
    let body = serde_json::json!({ "tool": "read_page", "args": { "slug": slug } });

    let response = match build_request(&endpoint, api_key).send_string(&body.to_string()) {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(ureq::Error::Status(401, _)) => {
            bail!(
                "unauthorized fetching '{}' from {} — {}",
                slug,
                endpoint,
                AUTH_HINT
            )
        }
        Err(ureq::Error::Status(code, resp)) => {
            let detail = resp.into_string().unwrap_or_default();
            bail!("fetch failed ({}) from {}: {}", code, endpoint, detail)
        }
        Err(e) => return Err(e).with_context(|| format!("failed to reach {}", endpoint)),
    };

    let text = response
        .into_string()
        .context("failed to read response body")?;
    // Some deployments route unknown paths to an HTML page instead of a 404
    // status; treat non-JSON as "shim not available" rather than a hard error.
    let parsed: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    if let Some(err) = parsed.get("error").and_then(|e| e.as_str()) {
        bail!("curata returned an error for '{}': {}", slug, err);
    }
    let result = parsed
        .get("result")
        .context("response missing 'result' — is this a curata /api/mcp endpoint?")?;
    extract_page(result, slug).map(Some)
}

/// Fetch via the streamable-HTTP MCP route: POST {base}/api/mcp/stream with a
/// JSON-RPC tools/call. The route is stateless, so no initialize handshake.
fn fetch_stream(base: &str, slug: &str, api_key: Option<&str>) -> Result<(String, String)> {
    let endpoint = format!("{}/api/mcp/stream", base);
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "read_page", "arguments": { "slug": slug } }
    });

    let response = match build_request(&endpoint, api_key).send_string(&body.to_string()) {
        Ok(r) => r,
        Err(ureq::Error::Status(401, _)) => {
            bail!(
                "unauthorized fetching '{}' from {} — {}",
                slug,
                endpoint,
                AUTH_HINT
            )
        }
        Err(ureq::Error::Status(code, resp)) => {
            let detail = resp.into_string().unwrap_or_default();
            bail!("fetch failed ({}) from {}: {}", code, endpoint, detail)
        }
        Err(e) => return Err(e).with_context(|| format!("failed to reach {}", endpoint)),
    };

    let text = response
        .into_string()
        .context("failed to read response body")?;
    let message = parse_sse_or_json(&text)?;

    if let Some(err) = message.get("error") {
        bail!("MCP error fetching '{}': {}", slug, err);
    }
    let tool_result = message
        .get("result")
        .context("JSON-RPC response missing 'result'")?;
    let inner_text = tool_result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .context("tools/call result missing content[0].text")?;
    if tool_result
        .get("isError")
        .and_then(|e| e.as_bool())
        .unwrap_or(false)
    {
        bail!("curata returned an error for '{}': {}", slug, inner_text);
    }
    let result: serde_json::Value = serde_json::from_str(inner_text)
        .context("read_page payload inside tools/call result was not JSON")?;
    extract_page(&result, slug)
}

/// Fetch the pack page. Tries the REST shim first (self-hosted instances),
/// falls back to the MCP streamable-HTTP route (hosted instances where the
/// shim isn't reachable).
fn fetch_pack(base: &str, slug: &str, api_key: Option<&str>) -> Result<(String, String)> {
    if let Some(pair) = fetch_rest(base, slug, api_key)? {
        return Ok(pair);
    }
    fetch_stream(base, slug, api_key)
}

/// First `{{variable}}` placeholder left in the text, if any. Pages created
/// from templates can carry unfilled variables — installing those would ship
/// literal `{{rules_markdown}}` into someone's CLAUDE.md.
fn find_unfilled_var(text: &str) -> Option<&str> {
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        if let Some(end) = after.find("}}") {
            let name = &after[..end];
            if !name.is_empty()
                && name.len() <= 64
                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                return Some(name);
            }
            rest = &after[end + 2..];
        } else {
            return None;
        }
    }
    None
}

/// Map pack target names to config file names. Empty targets = all.
fn resolve_targets(targets: &[String]) -> Result<Vec<&'static str>> {
    if targets.is_empty() {
        return Ok(TARGETS.to_vec());
    }
    let mut files = Vec::new();
    for t in targets {
        match t.as_str() {
            "claude" => files.push("CLAUDE.md"),
            "cursor" => files.push(".cursorrules"),
            other => bail!(
                "pack declares unknown target \"{}\" — this kazam version supports: claude, cursor",
                other
            ),
        }
    }
    files.dedup();
    Ok(files)
}

pub fn run(url: &str, api_key: Option<String>, dir: &Path, force: bool) -> Result<()> {
    let (base, slug) = parse_pack_url(url)?;
    let api_key = api_key.or_else(|| std::env::var("KAZAM_CURATA_API_KEY").ok());

    println!("Fetching pack '{}' from {} ...", slug, base);
    let (yaml, hash) = fetch_pack(&base, &slug, api_key.as_deref())?;

    let page: PackPage = serde_yaml::from_str(&yaml)
        .with_context(|| format!("failed to parse page YAML for '{}'", slug))?;

    let targets = match &page.pack {
        Some(meta) => resolve_targets(&meta.targets)?,
        None if force => {
            println!(
                "  warning: '{}' has no pack: marker — installing anyway (--force)",
                slug
            );
            TARGETS.to_vec()
        }
        None => bail!(
            "'{}' is not a pack — the page has no top-level pack: block. \
             Add `pack:` (optionally with targets:) to the page, or rerun with --force \
             to install any page's markdown at your own risk.",
            slug
        ),
    };

    let mut bodies = Vec::new();
    collect_markdown(&page.components, &mut bodies);
    if bodies.is_empty() {
        bail!(
            "pack '{}' has no markdown components — nothing to install. \
             Packs are pages whose rules live in markdown components.",
            slug
        );
    }

    let rules = bodies.join("\n\n");
    if let Some(var) = find_unfilled_var(&rules) {
        bail!(
            "pack '{}' contains an unfilled template variable {{{{{}}}}} — \
             fill in the page content before installing",
            slug,
            var
        );
    }

    let source = format!("{}/{}", base, slug);
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let block = render_block(&slug, &source, &hash, &date, &page.title, &rules);

    for target in targets {
        let path = dir.join(target);
        let existing = if path.exists() {
            Some(
                fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?,
            )
        } else {
            None
        };
        let updated = upsert_block(existing.as_deref(), &slug, &block)?;
        let action = match &existing {
            Some(text) if text.contains(&start_marker(&slug)) => "updated block in",
            Some(_) => "added block to",
            None => "created",
        };
        fs::write(&path, updated).with_context(|| format!("failed to write {}", path.display()))?;
        println!("  {} {}", action, path.display());
    }

    println!(
        "\nInstalled '{}' ({} markdown section{}, hash {}).",
        page.title,
        bodies.len(),
        if bodies.len() == 1 { "" } else { "s" },
        hash
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pages_url() {
        let (base, slug) =
            parse_pack_url("https://curata.example.com/pages/python-security").unwrap();
        assert_eq!(base, "https://curata.example.com");
        assert_eq!(slug, "python-security");
    }

    #[test]
    fn parses_path_prefixed_instance() {
        let (base, slug) = parse_pack_url("https://apps.example.com/ts-hub/pages/my-pack").unwrap();
        assert_eq!(base, "https://apps.example.com/ts-hub");
        assert_eq!(slug, "my-pack");
    }

    #[test]
    fn parses_public_share_url() {
        let (base, slug) =
            parse_pack_url("https://curata.example.com/p/maze/company-standards").unwrap();
        assert_eq!(base, "https://curata.example.com");
        assert_eq!(slug, "company-standards");
    }

    #[test]
    fn parses_bare_host_slug_and_adds_scheme() {
        let (base, slug) = parse_pack_url("curata.example.com/django-rest").unwrap();
        assert_eq!(base, "https://curata.example.com");
        assert_eq!(slug, "django-rest");
    }

    #[test]
    fn strips_query_and_fragment() {
        let (base, slug) = parse_pack_url("https://h.co/pages/x?v=1#top").unwrap();
        assert_eq!(base, "https://h.co");
        assert_eq!(slug, "x");
    }

    #[test]
    fn rejects_bare_host() {
        assert!(parse_pack_url("https://curata.example.com").is_err());
        assert!(parse_pack_url("https://curata.example.com/").is_err());
    }

    #[test]
    fn collects_markdown_depth_first_including_sections() {
        let yaml = r#"
title: Test Pack
shell: document
components:
  - type: markdown
    body: "first"
  - type: section
    heading: Rules
    components:
      - type: card_grid
        cards: []
      - type: markdown
        body: "second"
  - type: markdown
    body: "  "
"#;
        let page: PackPage = serde_yaml::from_str(yaml).unwrap();
        let mut out = Vec::new();
        collect_markdown(&page.components, &mut out);
        assert_eq!(out, vec!["first".to_string(), "second".to_string()]);
    }

    #[test]
    fn upsert_creates_new_file_content() {
        let block = render_block("pk", "https://h/pk", "abc", "2026-07-21", "Pack", "rules");
        let out = upsert_block(None, "pk", &block).unwrap();
        assert!(out.starts_with("<!-- kazam-pack:start pk -->"));
        assert!(out.ends_with("<!-- kazam-pack:end pk -->\n"));
    }

    #[test]
    fn upsert_appends_after_user_content() {
        let block = render_block("pk", "https://h/pk", "abc", "2026-07-21", "Pack", "rules");
        let out = upsert_block(Some("# My rules\n\ndo the thing\n"), "pk", &block).unwrap();
        assert!(out.starts_with("# My rules\n\ndo the thing\n\n<!-- kazam-pack:start pk -->"));
    }

    #[test]
    fn upsert_replaces_existing_block_idempotently() {
        let block1 = render_block(
            "pk",
            "https://h/pk",
            "abc",
            "2026-07-21",
            "Pack",
            "old rules",
        );
        let file1 = upsert_block(Some("# Mine\n"), "pk", &block1).unwrap();

        let block2 = render_block(
            "pk",
            "https://h/pk",
            "def",
            "2026-07-21",
            "Pack",
            "new rules",
        );
        let file2 = upsert_block(Some(&file1), "pk", &block2).unwrap();

        assert!(file2.contains("new rules"));
        assert!(!file2.contains("old rules"));
        assert!(file2.starts_with("# Mine\n"));
        // Reinstalling the same block changes nothing.
        let file3 = upsert_block(Some(&file2), "pk", &block2).unwrap();
        assert_eq!(file2, file3);
    }

    #[test]
    fn upsert_leaves_other_pack_blocks_alone() {
        let a = render_block("aa", "https://h/aa", "h1", "2026-07-21", "A", "a rules");
        let b = render_block("bb", "https://h/bb", "h2", "2026-07-21", "B", "b rules");
        let file = upsert_block(Some(&upsert_block(None, "aa", &a).unwrap()), "bb", &b).unwrap();

        let a2 = render_block("aa", "https://h/aa", "h9", "2026-07-21", "A", "a rules v2");
        let updated = upsert_block(Some(&file), "aa", &a2).unwrap();
        assert!(updated.contains("a rules v2"));
        assert!(updated.contains("b rules"));
        assert!(!updated.contains("h1"));
        assert!(updated.contains("h2"));
    }

    #[test]
    fn finds_unfilled_template_var() {
        assert_eq!(
            find_unfilled_var("rules {{rules_markdown}} here"),
            Some("rules_markdown")
        );
        assert_eq!(find_unfilled_var("no vars here"), None);
        // JS/CSS braces that aren't template vars don't trip it.
        assert_eq!(find_unfilled_var("if (x) {{ y = {a: 1}; }}"), None);
        assert_eq!(find_unfilled_var("empty {{}} braces"), None);
    }

    #[test]
    fn resolves_targets_default_and_explicit() {
        assert_eq!(
            resolve_targets(&[]).unwrap(),
            vec!["CLAUDE.md", ".cursorrules"]
        );
        assert_eq!(
            resolve_targets(&["claude".to_string()]).unwrap(),
            vec!["CLAUDE.md"]
        );
        assert_eq!(
            resolve_targets(&["cursor".to_string()]).unwrap(),
            vec![".cursorrules"]
        );
        assert!(resolve_targets(&["windsurf".to_string()]).is_err());
    }

    #[test]
    fn pack_marker_parses_from_yaml() {
        let yaml = "title: T\nshell: standard\npack:\n  targets: [claude]\ncomponents:\n  - type: markdown\n    body: rules\n";
        let page: PackPage = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(page.pack.unwrap().targets, vec!["claude"]);

        let yaml_bare = "title: T\nshell: standard\npack: {}\ncomponents: []\n";
        let page: PackPage = serde_yaml::from_str(yaml_bare).unwrap();
        assert!(page.pack.unwrap().targets.is_empty());

        let yaml_none = "title: T\nshell: standard\ncomponents: []\n";
        let page: PackPage = serde_yaml::from_str(yaml_none).unwrap();
        assert!(page.pack.is_none());
    }

    #[test]
    fn parses_sse_framed_message() {
        let sse =
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n";
        let v = parse_sse_or_json(sse).unwrap();
        assert_eq!(v["result"]["ok"], true);
    }

    #[test]
    fn parses_plain_json_message() {
        let v = parse_sse_or_json("{\"result\":{\"yaml\":\"title: X\"}}").unwrap();
        assert_eq!(v["result"]["yaml"], "title: X");
    }

    #[test]
    fn rejects_non_json_non_sse() {
        assert!(parse_sse_or_json("<!DOCTYPE html><html></html>").is_err());
    }

    #[test]
    fn extracts_yaml_and_hash() {
        let v = serde_json::json!({"slug":"s","yaml":"title: T","contentHash":"abc123"});
        let (yaml, hash) = extract_page(&v, "s").unwrap();
        assert_eq!(yaml, "title: T");
        assert_eq!(hash, "abc123");
    }

    #[test]
    fn extract_errors_without_yaml() {
        let v = serde_json::json!({"slug":"s"});
        assert!(extract_page(&v, "s").is_err());
    }

    #[test]
    fn upsert_errors_on_missing_end_marker() {
        let block = render_block("pk", "https://h/pk", "abc", "2026-07-21", "Pack", "rules");
        let corrupt = "<!-- kazam-pack:start pk -->\nno end here\n";
        assert!(upsert_block(Some(corrupt), "pk", &block).is_err());
    }
}

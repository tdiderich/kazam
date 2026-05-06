//! Ingest content from external platforms into kazam page YAML files.
//!
//! Currently supports Notion — query a database or walk a page tree and emit
//! one `.yaml` file per page, using the same component model as hand-authored
//! kazam sites.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

// ── token resolution ──────────────────────────────────────────────────────────

fn resolve_token(cli_token: &Option<String>) -> anyhow::Result<String> {
    if let Some(t) = cli_token {
        return Ok(t.clone());
    }
    // Try .env file in current dir
    if let Ok(content) = std::fs::read_to_string(".env") {
        for line in content.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("NOTION_TOKEN=") {
                let val = val.trim().trim_matches('"').trim_matches('\'');
                if !val.is_empty() {
                    return Ok(val.to_string());
                }
            }
        }
    }
    std::env::var("NOTION_TOKEN").map_err(|_| {
        anyhow::anyhow!(
            "No Notion token found.\n\n\
             Setup:\n\
             1. Go to https://www.notion.so/profile/integrations/internal\n\
             2. Create a new integration and copy the secret (starts with ntn_)\n\
             3. Add to .env in your project root:\n\
                NOTION_TOKEN=ntn_...\n\
                NOTION_WORKSPACE_ID=...  (workspace name → Settings → General)\n\
             \n\
             Then share pages with the integration:\n\
             Open a page in Notion → ··· → Connections → add your integration"
        )
    })
}

// ── HTTP helpers ──────────────────────────────────────────────────────────────

fn notion_get(token: &str, url: &str) -> anyhow::Result<serde_json::Value> {
    let body = ureq::get(url)
        .set("Authorization", &format!("Bearer {}", token))
        .set("Notion-Version", "2022-06-28")
        .set("Content-Type", "application/json")
        .call()?
        .into_string()?;
    Ok(serde_json::from_str(&body)?)
}

fn notion_post(
    token: &str,
    url: &str,
    body: &serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let body_str = serde_json::to_string(body)?;
    let resp_body = ureq::post(url)
        .set("Authorization", &format!("Bearer {}", token))
        .set("Notion-Version", "2022-06-28")
        .set("Content-Type", "application/json")
        .send_string(&body_str)?
        .into_string()?;
    Ok(serde_json::from_str(&resp_body)?)
}

// ── pagination helpers ────────────────────────────────────────────────────────

fn fetch_all_blocks(token: &str, block_id: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut blocks = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut url = format!(
            "https://api.notion.com/v1/blocks/{}/children?page_size=100",
            block_id
        );
        if let Some(c) = &cursor {
            url.push_str(&format!("&start_cursor={}", c));
        }
        let resp = notion_get(token, &url)?;

        if let Some(results) = resp["results"].as_array() {
            blocks.extend(results.clone());
        }
        if resp["has_more"].as_bool().unwrap_or(false) {
            cursor = resp["next_cursor"].as_str().map(String::from);
        } else {
            break;
        }
    }
    Ok(blocks)
}

fn fetch_all_database_pages(
    token: &str,
    database_id: &str,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut pages = Vec::new();
    let mut cursor: Option<String> = None;
    let url = format!("https://api.notion.com/v1/databases/{}/query", database_id);
    loop {
        let body = match &cursor {
            Some(c) => serde_json::json!({ "start_cursor": c, "page_size": 100 }),
            None => serde_json::json!({ "page_size": 100 }),
        };
        let resp = notion_post(token, &url, &body)?;

        if let Some(results) = resp["results"].as_array() {
            pages.extend(results.clone());
        }
        if resp["has_more"].as_bool().unwrap_or(false) {
            cursor = resp["next_cursor"].as_str().map(String::from);
        } else {
            break;
        }
    }
    Ok(pages)
}

// ── rich text → markdown ──────────────────────────────────────────────────────

fn rich_text_to_markdown(rich_text: &[serde_json::Value]) -> String {
    let mut out = String::new();
    for rt in rich_text {
        let plain = rt["plain_text"].as_str().unwrap_or("");
        let annotations = &rt["annotations"];
        let href = rt["href"].as_str();

        let mut text = plain.to_string();

        if annotations["code"].as_bool().unwrap_or(false) {
            text = format!("`{}`", text);
        }
        if annotations["bold"].as_bool().unwrap_or(false) {
            text = format!("**{}**", text);
        }
        if annotations["italic"].as_bool().unwrap_or(false) {
            text = format!("*{}*", text);
        }
        if annotations["strikethrough"].as_bool().unwrap_or(false) {
            text = format!("~~{}~~", text);
        }

        if let Some(url) = href {
            text = format!("[{}]({})", text, url);
        }

        out.push_str(&text);
    }
    out
}

fn get_rich_text(block_data: &serde_json::Value, key: &str) -> String {
    if let Some(arr) = block_data[key].as_array() {
        rich_text_to_markdown(arr)
    } else {
        String::new()
    }
}

// ── slugging ──────────────────────────────────────────────────────────────────

fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// ── page title extraction ─────────────────────────────────────────────────────

fn extract_page_title(page: &serde_json::Value) -> String {
    // For database rows, look through all properties for a title-type
    if let Some(props) = page["properties"].as_object() {
        for (_key, prop) in props {
            if prop["type"].as_str() == Some("title") {
                if let Some(title_arr) = prop["title"].as_array() {
                    let t = rich_text_to_markdown(title_arr);
                    if !t.is_empty() {
                        return t;
                    }
                }
            }
        }
    }
    // Fallback: use the page's own title if set (child_page blocks carry this)
    if let Some(title) = page["child_page"]["title"].as_str() {
        return title.to_string();
    }
    "Untitled".to_string()
}

fn extract_page_updated(page: &serde_json::Value) -> String {
    page["last_edited_time"]
        .as_str()
        .and_then(|s| s.get(..10))
        .unwrap_or("2026-01-01")
        .to_string()
}

fn extract_page_owner(page: &serde_json::Value) -> String {
    let editor = &page["last_edited_by"];
    // person type has person.email
    if let Some(email) = editor["person"]["email"].as_str() {
        if !email.is_empty() {
            return email.to_string();
        }
    }
    // bot type has name
    if let Some(name) = editor["name"].as_str() {
        if !name.is_empty() {
            return name.to_string();
        }
    }
    String::new()
}

// ── image download ────────────────────────────────────────────────────────────

/// Download bytes from `url` and save to `dest_path`. Returns the path on success.
fn download_image(url: &str, dest_path: &Path) -> anyhow::Result<()> {
    let resp = ureq::get(url).call()?;
    let mut bytes: Vec<u8> = Vec::new();
    use std::io::Read;
    resp.into_reader().read_to_end(&mut bytes)?;
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest_path, &bytes)?;
    Ok(())
}

// ── block → components ────────────────────────────────────────────────────────

/// Accumulated state while converting a flat list of Notion blocks into
/// kazam component YAML strings.
struct BlockConverter<'a> {
    token: &'a str,
    out_dir: &'a Path,
    page_slug: &'a str,
    dry_run: bool,
    image_index: usize,
    images_downloaded: usize,
    child_databases_skipped: usize,
    /// Pending markdown lines not yet flushed into a component.
    md_buf: String,
    /// Finished component YAML fragments (each already indented at the top
    /// level with `- type: …`).
    components: Vec<String>,
}

impl<'a> BlockConverter<'a> {
    fn new(token: &'a str, out_dir: &'a Path, page_slug: &'a str, dry_run: bool) -> Self {
        Self {
            token,
            out_dir,
            page_slug,
            dry_run,
            image_index: 0,
            images_downloaded: 0,
            child_databases_skipped: 0,
            md_buf: String::new(),
            components: Vec::new(),
        }
    }

    fn flush_markdown(&mut self) {
        let trimmed = self.md_buf.trim_end().to_string();
        if !trimmed.is_empty() {
            self.components.push(format!(
                "- type: markdown\n  body: |\n{}",
                indent_body(&trimmed, 4)
            ));
        }
        self.md_buf.clear();
    }

    fn push_md_line(&mut self, line: &str) {
        self.md_buf.push_str(line);
        self.md_buf.push('\n');
    }

    fn push_component(&mut self, yaml: String) {
        self.flush_markdown();
        self.components.push(yaml);
    }

    /// Recursively convert a list of Notion blocks, accumulating into `self`.
    fn convert_blocks(&mut self, blocks: &[serde_json::Value]) -> anyhow::Result<()> {
        for block in blocks {
            self.convert_block(block)?;
        }
        Ok(())
    }

    fn convert_block(&mut self, block: &serde_json::Value) -> anyhow::Result<()> {
        let block_type = block["type"].as_str().unwrap_or("unknown");
        let has_children = block["has_children"].as_bool().unwrap_or(false);
        let block_id = block["id"].as_str().unwrap_or("");

        match block_type {
            "paragraph" => {
                let text = get_rich_text(&block["paragraph"], "rich_text");
                if !text.is_empty() {
                    self.push_md_line(&text);
                } else {
                    self.push_md_line("");
                }
            }

            "heading_1" => {
                let text = get_rich_text(&block["heading_1"], "rich_text");
                self.push_md_line(&format!("# {}", text));
            }

            "heading_2" => {
                let text = get_rich_text(&block["heading_2"], "rich_text");
                self.push_md_line(&format!("## {}", text));
            }

            "heading_3" => {
                let text = get_rich_text(&block["heading_3"], "rich_text");
                self.push_md_line(&format!("### {}", text));
            }

            "bulleted_list_item" => {
                let text = get_rich_text(&block["bulleted_list_item"], "rich_text");
                self.push_md_line(&format!("- {}", text));
                if has_children {
                    let children = fetch_all_blocks(self.token, block_id)?;
                    // Indent child list items by 2 spaces
                    let mut child_buf = String::new();
                    for child in &children {
                        let child_type = child["type"].as_str().unwrap_or("");
                        if child_type == "bulleted_list_item" {
                            let ct = get_rich_text(&child["bulleted_list_item"], "rich_text");
                            child_buf.push_str(&format!("  - {}\n", ct));
                        } else if child_type == "numbered_list_item" {
                            let ct = get_rich_text(&child["numbered_list_item"], "rich_text");
                            child_buf.push_str(&format!("  1. {}\n", ct));
                        }
                    }
                    if !child_buf.is_empty() {
                        self.md_buf.push_str(&child_buf);
                    }
                }
            }

            "numbered_list_item" => {
                let text = get_rich_text(&block["numbered_list_item"], "rich_text");
                self.push_md_line(&format!("1. {}", text));
                if has_children {
                    let children = fetch_all_blocks(self.token, block_id)?;
                    for child in &children {
                        let child_type = child["type"].as_str().unwrap_or("");
                        if child_type == "bulleted_list_item" {
                            let ct = get_rich_text(&child["bulleted_list_item"], "rich_text");
                            self.md_buf.push_str(&format!("   - {}\n", ct));
                        } else if child_type == "numbered_list_item" {
                            let ct = get_rich_text(&child["numbered_list_item"], "rich_text");
                            self.md_buf.push_str(&format!("   1. {}\n", ct));
                        }
                    }
                }
            }

            "to_do" => {
                let text = get_rich_text(&block["to_do"], "rich_text");
                let checked = block["to_do"]["checked"].as_bool().unwrap_or(false);
                let marker = if checked { "- [x]" } else { "- [ ]" };
                self.push_md_line(&format!("{} {}", marker, text));
            }

            "quote" => {
                let text = get_rich_text(&block["quote"], "rich_text");
                self.push_md_line(&format!("> {}", text));
            }

            "code" => {
                let text = get_rich_text(&block["code"], "rich_text");
                let lang = block["code"]["language"].as_str().unwrap_or("text");
                self.push_component(format!(
                    "- type: code\n  language: {}\n  code: |\n{}",
                    lang,
                    indent_body(&text, 4)
                ));
            }

            "callout" => {
                let text = get_rich_text(&block["callout"], "rich_text");
                let emoji = block["callout"]["icon"]["emoji"].as_str().unwrap_or("");
                let body = if !emoji.is_empty() {
                    format!("{} {}", emoji, text)
                } else {
                    text
                };
                self.push_component(format!(
                    "- type: callout\n  body: |\n{}",
                    indent_body(&body, 4)
                ));
            }

            "divider" => {
                self.push_component("- type: divider".to_string());
            }

            "image" => {
                let src = extract_image_src(block);
                let is_external = block["image"]["type"].as_str() == Some("external");
                let final_src = if is_external || src.is_empty() {
                    src.clone()
                } else {
                    // Notion-hosted — download to assets/
                    self.image_index += 1;
                    let filename = format!("{}-{}.png", self.page_slug, self.image_index);
                    let dest_rel = format!("assets/images/{}", filename);
                    let dest_abs = self.out_dir.join(&dest_rel);
                    if !self.dry_run {
                        match download_image(&src, &dest_abs) {
                            Ok(_) => {
                                self.images_downloaded += 1;
                                format!("/{}", dest_rel)
                            }
                            Err(e) => {
                                eprintln!("  warning: image download failed ({}): {}", src, e);
                                src.clone()
                            }
                        }
                    } else {
                        format!("/{}", dest_rel)
                    }
                };
                if !final_src.is_empty() {
                    self.push_component(format!("- type: image\n  src: {}", final_src));
                }
            }

            "video" => {
                let src = block["video"]["external"]["url"]
                    .as_str()
                    .or_else(|| block["video"]["file"]["url"].as_str())
                    .unwrap_or("")
                    .to_string();
                if !src.is_empty() {
                    self.push_component(format!("- type: embed\n  src: {}", src));
                }
            }

            "embed" => {
                let src = block["embed"]["url"].as_str().unwrap_or("").to_string();
                if !src.is_empty() {
                    self.push_component(format!("- type: embed\n  src: {}", src));
                }
            }

            "bookmark" => {
                let url = block["bookmark"]["url"].as_str().unwrap_or("");
                let caption = get_rich_text(&block["bookmark"], "caption");
                let label = if !caption.is_empty() {
                    caption
                } else {
                    url.to_string()
                };
                if !url.is_empty() {
                    self.push_md_line(&format!("[{}]({})", label, url));
                }
            }

            "toggle" => {
                let title = get_rich_text(&block["toggle"], "rich_text");
                let body = if has_children {
                    let children = fetch_all_blocks(self.token, block_id)?;
                    let mut inner = String::new();
                    for child in &children {
                        let ct = child["type"].as_str().unwrap_or("");
                        let text = match ct {
                            "paragraph" => get_rich_text(&child["paragraph"], "rich_text"),
                            "bulleted_list_item" => {
                                format!(
                                    "- {}",
                                    get_rich_text(&child["bulleted_list_item"], "rich_text")
                                )
                            }
                            "numbered_list_item" => {
                                format!(
                                    "1. {}",
                                    get_rich_text(&child["numbered_list_item"], "rich_text")
                                )
                            }
                            _ => String::new(),
                        };
                        if !text.is_empty() {
                            inner.push_str(&text);
                            inner.push('\n');
                        }
                    }
                    inner.trim_end().to_string()
                } else {
                    String::new()
                };
                if body.is_empty() {
                    self.push_component(format!(
                        "- type: accordion\n  title: {}",
                        yaml_scalar(&title)
                    ));
                } else {
                    self.push_component(format!(
                        "- type: accordion\n  title: {}\n  body: |\n{}",
                        yaml_scalar(&title),
                        indent_body(&body, 4)
                    ));
                }
            }

            "table" => {
                let has_column_header = block["table"]["has_column_header"]
                    .as_bool()
                    .unwrap_or(false);
                if has_children {
                    let rows = fetch_all_blocks(self.token, block_id)?;
                    let yaml = table_to_yaml(&rows, has_column_header);
                    self.push_component(yaml);
                }
            }

            "column_list" => {
                if has_children {
                    let columns = fetch_all_blocks(self.token, block_id)?;
                    let mut col_bodies: Vec<String> = Vec::new();
                    for col in &columns {
                        let col_id = col["id"].as_str().unwrap_or("");
                        if !col_id.is_empty() {
                            let col_blocks = fetch_all_blocks(self.token, col_id)?;
                            let mut sub = BlockConverter::new(
                                self.token,
                                self.out_dir,
                                self.page_slug,
                                self.dry_run,
                            );
                            sub.convert_blocks(&col_blocks)?;
                            sub.flush_markdown();
                            self.images_downloaded += sub.images_downloaded;
                            self.child_databases_skipped += sub.child_databases_skipped;
                            // Serialize column content as plain markdown for now
                            let mut col_md = String::new();
                            for comp in &sub.components {
                                col_md.push_str(comp);
                                col_md.push('\n');
                            }
                            col_bodies.push(col_md.trim_end().to_string());
                        }
                    }
                    // Emit as a columns component with markdown content per column
                    let mut yaml = "- type: columns\n  columns:".to_string();
                    for body in &col_bodies {
                        yaml.push_str("\n  - body: |\n");
                        yaml.push_str(&indent_body(body, 6));
                    }
                    self.push_component(yaml);
                }
            }

            "column" => {
                // Handled by column_list above; skip if encountered standalone
            }

            "child_page" => {
                // Caller handles recursion; skip here
            }

            "child_database" => {
                if !block_id.is_empty() {
                    match self.convert_child_database(block_id) {
                        Ok(()) => {}
                        Err(e) => {
                            self.child_databases_skipped += 1;
                            eprintln!("  warning: child_database {}: {}", block_id, e);
                        }
                    }
                }
            }

            other => {
                eprintln!("  warning: unknown block type '{}' — skipped", other);
            }
        }
        Ok(())
    }

    fn convert_child_database(&mut self, db_id: &str) -> anyhow::Result<()> {
        let db_url = format!("https://api.notion.com/v1/databases/{}", db_id);
        let db_meta = notion_get(self.token, &db_url)?;
        let db_title = if let Some(arr) = db_meta["title"].as_array() {
            rich_text_to_markdown(arr)
        } else {
            String::new()
        };

        let props = db_meta["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();

        // Stable column order: title column first, then alphabetical
        let mut col_names: Vec<String> = props.keys().cloned().collect();
        col_names.sort();
        let title_col = col_names
            .iter()
            .position(|k| props[k]["type"].as_str() == Some("title"));
        if let Some(pos) = title_col {
            let name = col_names.remove(pos);
            col_names.insert(0, name);
        }

        let pages = fetch_all_database_pages(self.token, db_id)?;

        let mut yaml = "- type: table".to_string();

        if !db_title.is_empty() {
            self.push_md_line(&format!("### {}", db_title));
            self.flush_markdown();
        }

        yaml.push_str("\n  columns:");
        for col in &col_names {
            yaml.push_str(&format!("\n  - {}", yaml_scalar(col)));
        }

        yaml.push_str("\n  rows:");
        for page in &pages {
            let page_props = match page["properties"].as_object() {
                Some(p) => p,
                None => continue,
            };
            yaml.push_str("\n  - cells:");
            for col in &col_names {
                let val = page_props.get(col.as_str());
                let cell_text = match val {
                    Some(v) => extract_property_value(v),
                    None => String::new(),
                };
                yaml.push_str(&format!("\n    - {}", yaml_scalar(&cell_text)));
            }
        }

        self.push_component(yaml);
        Ok(())
    }
}

fn extract_property_value(prop: &serde_json::Value) -> String {
    match prop["type"].as_str().unwrap_or("") {
        "title" => prop["title"]
            .as_array()
            .map(|arr| rich_text_to_markdown(arr))
            .unwrap_or_default(),
        "rich_text" => prop["rich_text"]
            .as_array()
            .map(|arr| rich_text_to_markdown(arr))
            .unwrap_or_default(),
        "number" => prop["number"]
            .as_f64()
            .map(|n| {
                if n.fract() == 0.0 {
                    format!("{}", n as i64)
                } else {
                    format!("{}", n)
                }
            })
            .unwrap_or_default(),
        "select" => prop["select"]["name"].as_str().unwrap_or("").to_string(),
        "multi_select" => prop["multi_select"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v["name"].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        "date" => prop["date"]["start"].as_str().unwrap_or("").to_string(),
        "checkbox" => {
            if prop["checkbox"].as_bool().unwrap_or(false) {
                "Yes".to_string()
            } else {
                "No".to_string()
            }
        }
        "url" => prop["url"].as_str().unwrap_or("").to_string(),
        "email" => prop["email"].as_str().unwrap_or("").to_string(),
        "status" => prop["status"]["name"].as_str().unwrap_or("").to_string(),
        "formula" => match prop["formula"]["type"].as_str().unwrap_or("") {
            "string" => prop["formula"]["string"].as_str().unwrap_or("").to_string(),
            "number" => prop["formula"]["number"]
                .as_f64()
                .map(|n| format!("{}", n))
                .unwrap_or_default(),
            "boolean" => prop["formula"]["boolean"]
                .as_bool()
                .map(|b| format!("{}", b))
                .unwrap_or_default(),
            _ => String::new(),
        },
        "relation" => prop["relation"]
            .as_array()
            .map(|arr| format!("{} linked", arr.len()))
            .unwrap_or_default(),
        "people" => prop["people"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v["name"].as_str().or(v["person"]["email"].as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        _ => String::new(),
    }
}

// ── table serialization ───────────────────────────────────────────────────────

fn table_to_yaml(rows: &[serde_json::Value], has_column_header: bool) -> String {
    let mut yaml = "- type: table".to_string();
    let mut data_rows = rows.iter();

    if has_column_header {
        if let Some(header_row) = data_rows.next() {
            let cells: Vec<String> = header_row["table_row"]["cells"]
                .as_array()
                .map(|cols| {
                    cols.iter()
                        .map(|cell| {
                            let text = cell
                                .as_array()
                                .map(|arr| rich_text_to_markdown(arr))
                                .unwrap_or_default();
                            yaml_scalar(&text)
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !cells.is_empty() {
                yaml.push_str("\n  columns:");
                for col in &cells {
                    yaml.push_str(&format!("\n  - {}", col));
                }
            }
        }
    }

    yaml.push_str("\n  rows:");
    for row in data_rows {
        let cells: Vec<String> = row["table_row"]["cells"]
            .as_array()
            .map(|cols| {
                cols.iter()
                    .map(|cell| {
                        let text = cell
                            .as_array()
                            .map(|arr| rich_text_to_markdown(arr))
                            .unwrap_or_default();
                        yaml_scalar(&text)
                    })
                    .collect()
            })
            .unwrap_or_default();
        yaml.push_str("\n  - cells:");
        for cell in &cells {
            yaml.push_str(&format!("\n    - {}", cell));
        }
    }

    yaml
}

// ── image src extraction ──────────────────────────────────────────────────────

fn extract_image_src(block: &serde_json::Value) -> String {
    if let Some(url) = block["image"]["external"]["url"].as_str() {
        return url.to_string();
    }
    if let Some(url) = block["image"]["file"]["url"].as_str() {
        return url.to_string();
    }
    String::new()
}

// ── YAML helpers ──────────────────────────────────────────────────────────────

/// Indent each line of `body` by `spaces` spaces.
fn indent_body(body: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    body.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("{}{}", pad, line)
            }
        })
        .map(|mut s| {
            s.push('\n');
            s
        })
        .collect()
}

/// Produce a safe YAML scalar. For simple strings with no special chars, emit
/// unquoted. Otherwise wrap in single quotes and escape internal single quotes.
fn yaml_scalar(s: &str) -> String {
    // Needs quoting if empty, contains YAML special chars at start, or contains ':'
    let needs_quoting = s.is_empty()
        || s.starts_with([
            ':', '&', '*', '?', '|', '-', '<', '>', '=', '!', '%', '@', '`', '{', '}', '[', ']',
            '#', '\'',
        ])
        || s.contains(": ")
        || s.contains(" #")
        || s.starts_with('"')
        || s.contains('\n');
    if needs_quoting {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

// ── single page conversion ────────────────────────────────────────────────────

struct ConvertResult {
    yaml: String,
    images_downloaded: usize,
    child_databases_skipped: usize,
    /// (page_id, out_path) pairs for child pages to recurse into
    child_pages: Vec<(String, PathBuf)>,
}

/// Convert one Notion page (given its blocks) into a kazam YAML string.
fn page_to_yaml(
    token: &str,
    page: &serde_json::Value,
    blocks: &[serde_json::Value],
    out_dir: &Path,
    dry_run: bool,
) -> anyhow::Result<ConvertResult> {
    let title = extract_page_title(page);
    let updated = extract_page_updated(page);
    let owner = extract_page_owner(page);
    let slug = slugify(&title);

    let mut conv = BlockConverter::new(token, out_dir, &slug, dry_run);

    // Collect child_page references before converting blocks
    let mut child_pages: Vec<(String, PathBuf)> = Vec::new();
    let mut non_child_blocks: Vec<serde_json::Value> = Vec::new();

    for block in blocks {
        if block["type"].as_str() == Some("child_page") {
            let child_id = block["id"].as_str().unwrap_or("").to_string();
            let child_title = block["child_page"]["title"]
                .as_str()
                .unwrap_or("untitled")
                .to_string();
            let child_slug = slugify(&child_title);
            let child_dir = out_dir.join(&slug);
            let child_path = child_dir.join(format!("{}.yaml", child_slug));
            if !child_id.is_empty() {
                child_pages.push((child_id, child_path));
            }
        } else {
            non_child_blocks.push(block.clone());
        }
    }

    conv.convert_blocks(&non_child_blocks)?;
    conv.flush_markdown();

    let owner_line = if owner.is_empty() {
        String::new()
    } else {
        format!("owner: {}\n", owner)
    };

    let freshness_owner = if owner.is_empty() {
        String::new()
    } else {
        format!("  owner: {}\n", owner)
    };

    let header_component = format!("- type: header\n  title: {}", yaml_scalar(&title));

    let mut all_components = vec![header_component];
    all_components.extend(conv.components);

    let components_yaml = all_components
        .iter()
        .map(|c| {
            c.lines()
                .map(|line| format!("  {}", line))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n");

    let yaml = format!(
        "title: {title}\nshell: standard\nfreshness:\n  updated: '{updated}'\n  review_every: quarterly\n{freshness_owner}{owner_line}search_terms: []\nreferences: []\ncomponents:\n{components}\n",
        title = yaml_scalar(&title),
        updated = updated,
        freshness_owner = freshness_owner,
        owner_line = owner_line,
        components = components_yaml,
    );

    Ok(ConvertResult {
        yaml,
        images_downloaded: conv.images_downloaded,
        child_databases_skipped: conv.child_databases_skipped,
        child_pages,
    })
}

// ── database mode ─────────────────────────────────────────────────────────────

fn run_database(token: &str, database_id: &str, out: &Path, dry_run: bool) -> anyhow::Result<()> {
    println!("\n  Notion → kazam");

    // Fetch database metadata for name
    let db_meta_url = format!("https://api.notion.com/v1/databases/{}", database_id);
    let db_meta = notion_get(token, &db_meta_url).unwrap_or(serde_json::Value::Null);
    let db_name = if let Some(title_arr) = db_meta["title"].as_array() {
        rich_text_to_markdown(title_arr)
    } else {
        database_id.to_string()
    };

    let pages = fetch_all_database_pages(token, database_id)?;
    println!("  database: {} ({} pages)\n", db_name, pages.len());

    let mut stats = RunStats::new();

    for (i, page) in pages.iter().enumerate() {
        let page_id = page["id"].as_str().unwrap_or("");
        if page_id.is_empty() {
            continue;
        }

        // Rate limiting: sleep after every 5 pages
        if i > 0 && i % 5 == 0 {
            thread::sleep(Duration::from_millis(350));
        }

        let blocks = fetch_all_blocks(token, page_id)?;
        let result = page_to_yaml(token, page, &blocks, out, dry_run)?;

        let title = extract_page_title(page);
        let slug = slugify(&title);
        let file_path = out.join(format!("{}.yaml", slug));
        let display_path = file_path.display().to_string();

        if dry_run {
            println!("  [dry-run] {}", display_path);
        } else {
            println!("  {}", display_path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&file_path, &result.yaml)?;
        }

        stats.accumulate(&result);
        stats.record_page(page);

        // Recurse into child pages
        for (child_id, child_path) in &result.child_pages {
            write_page_tree(token, child_id, child_path, out, dry_run, &mut stats)?;
        }
    }

    print_run_summary(&stats, out, dry_run);
    Ok(())
}

// ── page tree mode ────────────────────────────────────────────────────────────

struct PageMeta {
    title: String,
    last_edited: String,
    days_stale: i64,
    editor: String,
}

fn days_since(date_str: &str) -> i64 {
    let today = chrono::Utc::now().date_naive();
    if let Ok(d) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        (today - d).num_days()
    } else {
        -1
    }
}

struct RunStats {
    images_downloaded: usize,
    dbs_skipped: usize,
    files_written: usize,
    page_metas: Vec<PageMeta>,
}

impl RunStats {
    fn new() -> Self {
        Self {
            images_downloaded: 0,
            dbs_skipped: 0,
            files_written: 0,
            page_metas: Vec::new(),
        }
    }

    fn accumulate(&mut self, result: &ConvertResult) {
        self.images_downloaded += result.images_downloaded;
        self.dbs_skipped += result.child_databases_skipped;
        self.files_written += 1;
    }

    fn record_page(&mut self, page: &serde_json::Value) {
        let title = extract_page_title(page);
        let last_edited = extract_page_updated(page);
        let days_stale = days_since(&last_edited);
        let editor = extract_page_owner(page);
        self.page_metas.push(PageMeta {
            title,
            last_edited,
            days_stale,
            editor,
        });
    }
}

fn write_page_tree(
    token: &str,
    page_id: &str,
    file_path: &Path,
    out_dir: &Path,
    dry_run: bool,
    stats: &mut RunStats,
) -> anyhow::Result<()> {
    let page_url = format!("https://api.notion.com/v1/pages/{}", page_id);
    let page = notion_get(token, &page_url)?;
    let blocks = fetch_all_blocks(token, page_id)?;

    // Use file_path's parent as local out_dir for nested images
    let local_out = file_path.parent().unwrap_or(out_dir);
    let result = page_to_yaml(token, &page, &blocks, local_out, dry_run)?;

    let display_path = file_path.display().to_string();
    if dry_run {
        println!("  [dry-run] {}", display_path);
    } else {
        println!("  {}", display_path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(file_path, &result.yaml)?;
    }

    stats.accumulate(&result);
    stats.record_page(&page);

    // Recurse into children
    for (child_id, child_path) in &result.child_pages {
        thread::sleep(Duration::from_millis(350));
        write_page_tree(token, child_id, child_path, out_dir, dry_run, stats)?;
    }

    Ok(())
}

fn run_page(token: &str, page_id: &str, out: &Path, dry_run: bool) -> anyhow::Result<()> {
    println!("\n  Notion → kazam\n");

    let page_url = format!("https://api.notion.com/v1/pages/{}", page_id);
    let page = notion_get(token, &page_url)?;
    let blocks = fetch_all_blocks(token, page_id)?;

    let title = extract_page_title(&page);
    let slug = slugify(&title);
    let file_path = out.join(format!("{}.yaml", slug));

    let result = page_to_yaml(token, &page, &blocks, out, dry_run)?;

    let display_path = file_path.display().to_string();
    if dry_run {
        println!("  [dry-run] {}", display_path);
    } else {
        println!("  {}", display_path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, &result.yaml)?;
    }

    let mut stats = RunStats::new();
    stats.accumulate(&result);
    stats.record_page(&page);

    // Recurse child pages
    for (child_id, child_path) in &result.child_pages {
        thread::sleep(Duration::from_millis(350));
        write_page_tree(token, child_id, child_path, out, dry_run, &mut stats)?;
    }

    print_run_summary(&stats, out, dry_run);
    Ok(())
}

// ── search API (--all mode) ──────────────────────────────────────────────────

fn search_all_pages(token: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut pages = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut body = serde_json::json!({
            "filter": { "value": "page", "property": "object" },
            "page_size": 100
        });
        if let Some(c) = &cursor {
            body["start_cursor"] = serde_json::Value::String(c.clone());
        }
        let resp = notion_post(token, "https://api.notion.com/v1/search", &body)?;
        if let Some(results) = resp["results"].as_array() {
            pages.extend(results.clone());
        }
        if resp["has_more"].as_bool().unwrap_or(false) {
            cursor = resp["next_cursor"].as_str().map(String::from);
        } else {
            break;
        }
    }
    Ok(pages)
}

fn find_root_pages(pages: &[serde_json::Value]) -> Vec<&serde_json::Value> {
    let all_ids: std::collections::HashSet<String> = pages
        .iter()
        .filter_map(|p| p["id"].as_str().map(String::from))
        .collect();

    pages
        .iter()
        .filter(|p| {
            let parent = &p["parent"];
            match parent["type"].as_str() {
                Some("page_id") => {
                    let pid = parent["page_id"].as_str().unwrap_or("");
                    !all_ids.contains(pid)
                }
                Some("workspace") => true,
                _ => true,
            }
        })
        .collect()
}

fn run_all(token: &str, out: &Path, dry_run: bool) -> anyhow::Result<()> {
    println!("\n  Notion → kazam (discovering all accessible pages)\n");

    let all_pages = search_all_pages(token)?;
    println!("  found {} page(s) via search API", all_pages.len());

    let roots = find_root_pages(&all_pages);
    println!("  {} root page(s) to ingest\n", roots.len());

    let mut stats = RunStats::new();

    for (i, page) in roots.iter().enumerate() {
        let page_id = page["id"].as_str().unwrap_or("");
        if page_id.is_empty() {
            continue;
        }

        if i > 0 && i % 5 == 0 {
            thread::sleep(Duration::from_millis(350));
        }

        let blocks = fetch_all_blocks(token, page_id)?;
        let title = extract_page_title(page);
        let slug = slugify(&title);
        let file_path = out.join(format!("{}.yaml", slug));
        let display_path = file_path.display().to_string();

        let result = page_to_yaml(token, page, &blocks, out, dry_run)?;

        if dry_run {
            println!("  [dry-run] {}", display_path);
        } else {
            println!("  {}", display_path);
            if let Some(parent) = file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&file_path, &result.yaml)?;
        }

        stats.accumulate(&result);
        stats.record_page(page);

        for (child_id, child_path) in &result.child_pages {
            write_page_tree(token, child_id, child_path, out, dry_run, &mut stats)?;
        }
    }

    print_run_summary(&stats, out, dry_run);
    Ok(())
}

fn stats_all(token: &str) -> anyhow::Result<()> {
    let pages = search_all_pages(token)?;
    let metas = collect_page_meta(&pages);
    print_stats(&metas, "all accessible pages");
    Ok(())
}

fn print_run_summary(stats: &RunStats, out: &Path, dry_run: bool) {
    println!();
    if dry_run {
        println!(
            "  [dry-run] {} page(s) would be written to {}/",
            stats.files_written,
            out.display()
        );
    } else {
        println!("  ✓ {} page(s) → {}/", stats.files_written, out.display());
    }
    if stats.images_downloaded > 0 {
        println!(
            "    {} image(s) downloaded to {}/assets/images/",
            stats.images_downloaded,
            out.display()
        );
    }
    if stats.dbs_skipped > 0 {
        println!("    {} child database(s) skipped", stats.dbs_skipped);
    }

    if stats.page_metas.is_empty() {
        return;
    }

    // Staleness breakdown
    let total = stats.page_metas.len();
    let fresh = stats
        .page_metas
        .iter()
        .filter(|m| staleness_bucket(m.days_stale) == "fresh (≤90d)")
        .count();
    let stale = stats
        .page_metas
        .iter()
        .filter(|m| staleness_bucket(m.days_stale) == "stale (91–180d)")
        .count();
    let very_stale = stats
        .page_metas
        .iter()
        .filter(|m| staleness_bucket(m.days_stale) == "very stale (>180d)")
        .count();
    let freshness_pct = if total > 0 {
        (fresh as f64 / total as f64 * 100.0).round() as usize
    } else {
        0
    };

    println!(
        "\n  Staleness: {}% fresh — {} fresh, {} stale, {} very stale",
        freshness_pct, fresh, stale, very_stale
    );

    // Next steps
    println!("\n  Next steps:");
    println!("    1. Review the generated YAML files and build your site:");
    println!("         kazam build {}", out.display());
    println!("    2. Run an audit to see what needs attention:");
    println!("         kazam audit {}", out.display());
    if very_stale > 0 {
        println!(
            "    3. {} page(s) haven't been touched in 6+ months.",
            very_stale
        );
        println!("       Consider archiving or refreshing with an agent:");
        println!("         kazam audit {} --pretty", out.display());
    }
    if fresh < total {
        let step = if very_stale > 0 { 4 } else { 3 };
        println!(
            "    {}. Set owners on pages so freshness reminders reach the right people:",
            step
        );
        println!("         # Edit freshness.owner in each YAML file");
    }
    println!();
}

// ── public entry point ────────────────────────────────────────────────────────

pub fn notion(
    database: &Option<String>,
    page: &Option<String>,
    token: &Option<String>,
    out: &Path,
    dry_run: bool,
    all: bool,
) -> anyhow::Result<()> {
    if all {
        let tok = resolve_token(token)?;
        return run_all(&tok, out, dry_run);
    }
    match (database, page) {
        (None, None) => {
            anyhow::bail!(
                "Specify --database <id>, --page <id>, or --all.\n\n\
                 --all discovers every page the integration can access.\n\n\
                 Finding IDs:\n  \
                 Page: notion.so/My-Page-abc123def456 → --page abc123de-f456-...\n  \
                 DB:   notion.so/abc123?v=...         → --database abc123...\n  \
                 The 32-char hex at the end of the URL is the ID.\n\n\
                 Run `kazam ingest notion --help` for full setup instructions."
            );
        }
        (Some(db_id), _) => {
            let tok = resolve_token(token)?;
            run_database(&tok, db_id, out, dry_run)
        }
        (None, Some(page_id)) => {
            let tok = resolve_token(token)?;
            run_page(&tok, page_id, out, dry_run)
        }
    }
}

// ── stats mode ───────────────────────────────────────────────────────────────

fn staleness_bucket(days: i64) -> &'static str {
    if days < 0 {
        "unknown"
    } else if days <= 90 {
        "fresh (≤90d)"
    } else if days <= 180 {
        "stale (91–180d)"
    } else {
        "very stale (>180d)"
    }
}

fn collect_page_meta(pages: &[serde_json::Value]) -> Vec<PageMeta> {
    pages
        .iter()
        .map(|page| {
            let title = extract_page_title(page);
            let last_edited = extract_page_updated(page);
            let days_stale = days_since(&last_edited);
            let editor = extract_page_owner(page);
            PageMeta {
                title,
                last_edited,
                days_stale,
                editor,
            }
        })
        .collect()
}

fn print_stats(metas: &[PageMeta], source_label: &str) {
    println!("\n  Notion staleness report — {}\n", source_label);
    println!(
        "  {:<45} {:>12} {:>8}  Editor",
        "Page", "Last edited", "Days"
    );
    println!("  {}", "─".repeat(90));

    let mut sorted: Vec<&PageMeta> = metas.iter().collect();
    sorted.sort_by_key(|m| std::cmp::Reverse(m.days_stale));

    for m in &sorted {
        let truncated_title: String = if m.title.len() > 44 {
            format!("{}…", &m.title[..43])
        } else {
            m.title.clone()
        };
        let editor_short: String = if m.editor.len() > 25 {
            format!("{}…", &m.editor[..24])
        } else {
            m.editor.clone()
        };
        println!(
            "  {:<45} {:>12} {:>5}d  {}",
            truncated_title, m.last_edited, m.days_stale, editor_short
        );
    }

    // Bucket summary
    let mut fresh = 0usize;
    let mut stale = 0usize;
    let mut very_stale = 0usize;
    let mut unknown = 0usize;
    for m in metas {
        match staleness_bucket(m.days_stale) {
            "fresh (≤90d)" => fresh += 1,
            "stale (91–180d)" => stale += 1,
            "very stale (>180d)" => very_stale += 1,
            _ => unknown += 1,
        }
    }

    let total = metas.len();
    let freshness_pct = if total > 0 {
        (fresh as f64 / total as f64 * 100.0).round() as usize
    } else {
        0
    };

    println!("\n  Summary ({} pages):", total);
    println!(
        "    fresh (≤90d):      {:>4}  ({:.0}%)",
        fresh,
        if total > 0 {
            fresh as f64 / total as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "    stale (91–180d):   {:>4}  ({:.0}%)",
        stale,
        if total > 0 {
            stale as f64 / total as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "    very stale (>180d):{:>4}  ({:.0}%)",
        very_stale,
        if total > 0 {
            very_stale as f64 / total as f64 * 100.0
        } else {
            0.0
        }
    );
    if unknown > 0 {
        println!("    unknown:           {:>4}", unknown);
    }
    println!("    freshness score:   {:>3}%\n", freshness_pct);

    // Top editors by staleness
    let mut editor_map: std::collections::HashMap<String, Vec<&PageMeta>> =
        std::collections::HashMap::new();
    for m in metas {
        let key = if m.editor.is_empty() {
            "(unknown)".to_string()
        } else {
            m.editor.clone()
        };
        editor_map.entry(key).or_default().push(m);
    }
    let mut editors: Vec<_> = editor_map.into_iter().collect();
    editors.sort_by_key(|e| std::cmp::Reverse(e.1.len()));

    println!("  By editor:");
    println!(
        "  {:<35} {:>6} {:>6} {:>6} {:>6}",
        "Editor", "Total", "Fresh", "Stale", "V.Stale"
    );
    println!("  {}", "─".repeat(70));
    for (editor, pages) in &editors {
        let f = pages
            .iter()
            .filter(|p| staleness_bucket(p.days_stale) == "fresh (≤90d)")
            .count();
        let s = pages
            .iter()
            .filter(|p| staleness_bucket(p.days_stale) == "stale (91–180d)")
            .count();
        let vs = pages
            .iter()
            .filter(|p| staleness_bucket(p.days_stale) == "very stale (>180d)")
            .count();
        let editor_display: String = if editor.len() > 34 {
            format!("{}…", &editor[..33])
        } else {
            editor.clone()
        };
        println!(
            "  {:<35} {:>6} {:>6} {:>6} {:>6}",
            editor_display,
            pages.len(),
            f,
            s,
            vs
        );
    }
    println!();
}

fn stats_database(token: &str, database_id: &str) -> anyhow::Result<()> {
    let db_meta_url = format!("https://api.notion.com/v1/databases/{}", database_id);
    let db_meta = notion_get(token, &db_meta_url).unwrap_or(serde_json::Value::Null);
    let db_name = if let Some(title_arr) = db_meta["title"].as_array() {
        rich_text_to_markdown(title_arr)
    } else {
        database_id.to_string()
    };
    let pages = fetch_all_database_pages(token, database_id)?;
    let metas = collect_page_meta(&pages);
    print_stats(&metas, &format!("database: {}", db_name));
    Ok(())
}

fn stats_page_tree(token: &str, page_id: &str) -> anyhow::Result<()> {
    let page_url = format!("https://api.notion.com/v1/pages/{}", page_id);
    let root_page = notion_get(token, &page_url)?;
    let root_title = extract_page_title(&root_page);

    let mut all_metas: Vec<PageMeta> = Vec::new();
    collect_tree_metas(token, &root_page, &mut all_metas)?;

    print_stats(&all_metas, &format!("page tree: {}", root_title));
    Ok(())
}

fn collect_tree_metas(
    token: &str,
    page: &serde_json::Value,
    metas: &mut Vec<PageMeta>,
) -> anyhow::Result<()> {
    let title = extract_page_title(page);
    let last_edited = extract_page_updated(page);
    let days_stale = days_since(&last_edited);
    let editor = extract_page_owner(page);
    metas.push(PageMeta {
        title,
        last_edited,
        days_stale,
        editor,
    });

    let page_id = page["id"].as_str().unwrap_or("");
    if page_id.is_empty() {
        return Ok(());
    }

    let blocks = fetch_all_blocks(token, page_id)?;
    for block in &blocks {
        if block["type"].as_str() == Some("child_page") {
            let child_id = block["id"].as_str().unwrap_or("");
            if !child_id.is_empty() {
                if metas.len() > 4 && metas.len().is_multiple_of(5) {
                    thread::sleep(Duration::from_millis(350));
                }
                let child_url = format!("https://api.notion.com/v1/pages/{}", child_id);
                match notion_get(token, &child_url) {
                    Ok(child_page) => {
                        collect_tree_metas(token, &child_page, metas)?;
                    }
                    Err(e) => {
                        eprintln!("  warning: could not fetch child page {}: {}", child_id, e);
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn notion_stats(
    database: &Option<String>,
    page: &Option<String>,
    token: &Option<String>,
    all: bool,
) -> anyhow::Result<()> {
    let tok = resolve_token(token)?;
    if all {
        return stats_all(&tok);
    }
    match (database, page) {
        (None, None) => {
            anyhow::bail!(
                "Specify --database <id>, --page <id>, or --all.\n\n\
                 --all discovers every page the integration can access.\n\n\
                 Finding IDs:\n  \
                 Page: notion.so/My-Page-abc123def456 → --page abc123de-f456-...\n  \
                 DB:   notion.so/abc123?v=...         → --database abc123...\n  \
                 The 32-char hex at the end of the URL is the ID.\n\n\
                 Run `kazam ingest notion --help` for full setup instructions."
            );
        }
        (Some(db_id), _) => stats_database(&tok, db_id),
        (None, Some(page_id)) => stats_page_tree(&tok, page_id),
    }
}

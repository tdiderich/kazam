# Wishes as Recipes - Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the old static-page wish system with a recipe library backed by a registry + GitHub fetch model.

**Architecture:** Delete the entire old wish module (1407 LOC across 4 files). Replace with a single `src/wish.rs` that handles `list` (scan local + embedded registry) and `init` (fetch from GitHub API). Add `ureq` for HTTP. Ship the first wish (`hubspot-icp`) as a real directory in `wishes/`.

**Tech Stack:** Rust, clap (CLI), serde_yaml (wish.yaml parsing), ureq (GitHub API), serde_json (API responses)

---

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `src/wish.rs` | Create (replaces `src/wish/`) | New wish module: list, init, registry, GitHub fetch |
| `src/wish/mod.rs` | Delete | Old workspace/agent scaffolding |
| `src/wish/deck.rs` | Delete | Old deck wish templates |
| `src/wish/brief.rs` | Delete | Old brief wish templates |
| `src/wish/dashboard.rs` | Delete | Old dashboard wish templates |
| `src/main.rs` | Modify | Update `Wish` command enum + dispatch |
| `Cargo.toml` | Modify | Add `ureq` dependency |
| `src/wish/registry.yaml` | Create | Embedded wish index |
| `wishes/hubspot-icp/wish.yaml` | Create | Wish metadata |
| `wishes/hubspot-icp/script.py` | Create | Copy from maze-brain's proven script |
| `wishes/hubspot-icp/prompt.md` | Create | Analysis instructions |
| `wishes/hubspot-icp/page.yaml` | Create | Starter kazam page with refresh block |
| `wishes/hubspot-icp/README.md` | Create | Customization guide |

---

### Task 1: Delete old wish module and update main.rs CLI

**Files:**
- Delete: `src/wish/mod.rs`, `src/wish/deck.rs`, `src/wish/brief.rs`, `src/wish/dashboard.rs`
- Modify: `src/main.rs:28,79-93,209-216`

- [ ] **Step 1: Delete the old wish directory**

```bash
rm -rf src/wish/
```

- [ ] **Step 2: Create an empty placeholder `src/wish.rs`**

```rust
use anyhow::Result;

pub fn list(_json: bool) -> Result<()> {
    println!("  No wishes available yet.");
    Ok(())
}

pub fn init(_name: &str, _dir: Option<std::path::PathBuf>, _force: bool) -> Result<()> {
    anyhow::bail!("not yet implemented")
}
```

- [ ] **Step 3: Update main.rs - replace the old Wish command with new subcommands**

Change the `mod wish;` declaration (it already works - Rust will find `wish.rs` instead of `wish/mod.rs`).

Replace the `Wish` variant in the `Command` enum (lines 79-93) with:

```rust
    /// Grant a wish - install a recipe for self-refreshing docs
    Wish {
        #[command(subcommand)]
        command: WishCommand,
    },
```

Add the `WishCommand` enum after the `Command` enum:

```rust
#[derive(Subcommand)]
enum WishCommand {
    /// List available wishes (local + registry)
    List {
        /// Machine-readable JSON output
        #[arg(long)]
        json: bool,
    },
    /// Install a wish from the registry into local wishes/
    Init {
        /// Name of the wish to install
        name: String,
        /// Install to a specific directory instead of wishes/
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Overwrite existing local wish
        #[arg(long)]
        force: bool,
    },
}
```

Update the dispatch (lines 209-216) to:

```rust
        Command::Wish { command } => match command {
            WishCommand::List { json } => wish::list(json),
            WishCommand::Init { name, dir, force } => wish::init(&name, dir, force),
        },
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: `Finished`

- [ ] **Step 5: Verify the old wishes are gone and new subcommands appear**

Run: `cargo run -- wish list 2>&1`
Expected: `No wishes available yet.`

Run: `cargo run -- wish --help 2>&1`
Expected: Shows `list` and `init` subcommands, no `--yolo`, `--dry-run`, `--stdout`

- [ ] **Step 6: Commit**

```bash
git add -A src/wish* src/main.rs
git commit -m "refactor: replace old wish module with recipe-based subcommands

Delete the old workspace/agent-shell wish system (deck, brief, dashboard).
Replace with wish list + wish init subcommands that will back the new
recipe library. Placeholder implementations for now."
```

---

### Task 2: Add ureq dependency and implement registry loading

**Files:**
- Modify: `Cargo.toml`
- Create: `src/wish/registry.yaml` (but since we now use `wish.rs` not `wish/`, embed it differently - see below)

Note: since `src/wish.rs` is a single file (not a directory module), we'll embed the registry YAML as a const string in `wish.rs` rather than `include_str!` from a separate file. This keeps it simple. When the registry grows large, it can be extracted.

- [ ] **Step 1: Add ureq to Cargo.toml**

Add after the `chrono` line in `[dependencies]`:

```toml
ureq = "2"
```

- [ ] **Step 2: Define types and registry in `src/wish.rs`**

Replace the entire file with:

```rust
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const REGISTRY_YAML: &str = r#"
- name: hubspot-icp
  description: "Data-driven ICP from HubSpot deals + Apollo enrichment"
  tags: [gtm, hubspot, apollo, icp]
  path: wishes/hubspot-icp
"#;

const GITHUB_REPO: &str = "tdiderich/kazam";
const GITHUB_BRANCH: &str = "main";

#[derive(Deserialize, Clone)]
struct RegistryEntry {
    name: String,
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    path: String,
}

#[derive(Deserialize)]
struct LocalWish {
    name: String,
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    data_sources: Vec<String>,
}

fn load_registry() -> Vec<RegistryEntry> {
    serde_yaml::from_str(REGISTRY_YAML).unwrap_or_default()
}

fn scan_local(base: &Path) -> Vec<LocalWish> {
    let wishes_dir = base.join("wishes");
    if !wishes_dir.is_dir() {
        return vec![];
    }
    let mut found = vec![];
    if let Ok(entries) = fs::read_dir(&wishes_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let meta_path = path.join("wish.yaml");
            if let Ok(contents) = fs::read_to_string(&meta_path) {
                if let Ok(wish) = serde_yaml::from_str::<LocalWish>(&contents) {
                    found.push(wish);
                }
            }
        }
    }
    found
}

pub fn list(json: bool) -> Result<()> {
    let local = scan_local(Path::new("."));
    let registry = load_registry();

    let local_names: Vec<&str> = local.iter().map(|w| w.name.as_str()).collect();

    if json {
        let entries: Vec<serde_json::Value> = local
            .iter()
            .map(|w| {
                serde_json::json!({
                    "name": w.name,
                    "description": w.description,
                    "tags": w.tags,
                    "source": "local",
                })
            })
            .chain(registry.iter().filter(|r| !local_names.contains(&r.name.as_str())).map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "description": r.description,
                    "tags": r.tags,
                    "source": "registry",
                })
            }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    println!();
    println!("  Available wishes:");

    if !local.is_empty() {
        println!();
        println!("  LOCAL");
        for w in &local {
            println!("    {:<20} {}", w.name, w.description);
        }
    }

    let remote_only: Vec<&RegistryEntry> = registry
        .iter()
        .filter(|r| !local_names.contains(&r.name.as_str()))
        .collect();

    if !remote_only.is_empty() {
        println!();
        println!("  REGISTRY (kazam wish init <name> to install)");
        for r in &remote_only {
            println!("    {:<20} {}", r.name, r.description);
        }
    }

    if local.is_empty() && remote_only.is_empty() {
        println!();
        println!("  (none)");
    }

    println!();
    Ok(())
}

pub fn init(_name: &str, _dir: Option<PathBuf>, _force: bool) -> Result<()> {
    bail!("not yet implemented")
}
```

- [ ] **Step 3: Verify it compiles and list works**

Run: `cargo build 2>&1 | tail -3`
Expected: `Finished`

Run: `cargo run -- wish list 2>&1`
Expected: Shows `REGISTRY` section with `hubspot-icp`

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/wish.rs
git commit -m "feat: wish registry loading and list command

Embed a YAML registry of available wishes. kazam wish list scans
local wishes/ directory and merges with the embedded registry.
Supports --json for machine-readable output."
```

---

### Task 3: Implement `wish init` - GitHub fetch

**Files:**
- Modify: `src/wish.rs`

- [ ] **Step 1: Add the GitHub fetch and init implementation**

Replace the placeholder `init` function at the bottom of `src/wish.rs` with:

```rust
pub fn init(name: &str, dir: Option<PathBuf>, force: bool) -> Result<()> {
    let registry = load_registry();
    let entry = registry
        .iter()
        .find(|r| r.name == name)
        .ok_or_else(|| anyhow::anyhow!("unknown wish '{}'. Run `kazam wish list` to see available wishes.", name))?;

    let base = dir.unwrap_or_else(|| PathBuf::from("wishes"));
    let dest = base.join(name);

    if dest.exists() && !force {
        bail!(
            "'{}' already exists. Pass --force to overwrite.",
            dest.display()
        );
    }

    println!();
    println!("  Fetching wish: {}", name);
    println!();

    let files = fetch_directory(&entry.path)?;

    fs::create_dir_all(&dest)
        .with_context(|| format!("creating {}", dest.display()))?;

    for (filename, contents) in &files {
        let file_path = dest.join(filename);
        fs::write(&file_path, contents)
            .with_context(|| format!("writing {}", file_path.display()))?;
        println!("    {}", file_path.display());
    }

    println!();
    println!("  ✨ Installed wish: {}", name);
    println!();
    println!("  Next: have your agent read wishes/{}/README.md and adapt the script.", name);
    println!();

    Ok(())
}

#[derive(Deserialize)]
struct GitHubContent {
    name: String,
    download_url: Option<String>,
    #[serde(rename = "type")]
    content_type: String,
}

fn fetch_directory(repo_path: &str) -> Result<Vec<(String, String)>> {
    let url = format!(
        "https://api.github.com/repos/{}/contents/{}?ref={}",
        GITHUB_REPO, repo_path, GITHUB_BRANCH
    );

    let resp = ureq::get(&url)
        .set("User-Agent", "kazam")
        .set("Accept", "application/vnd.github.v3+json")
        .call()
        .context("GitHub API request failed - check your internet connection")?;

    let entries: Vec<GitHubContent> = resp.into_json().context("parsing GitHub API response")?;

    let mut files = vec![];
    for entry in entries {
        if entry.content_type != "file" {
            continue;
        }
        let download_url = entry
            .download_url
            .ok_or_else(|| anyhow::anyhow!("no download URL for {}", entry.name))?;

        let content = ureq::get(&download_url)
            .set("User-Agent", "kazam")
            .call()
            .with_context(|| format!("downloading {}", entry.name))?
            .into_string()
            .with_context(|| format!("reading {}", entry.name))?;

        files.push((entry.name, content));
    }

    if files.is_empty() {
        bail!("no files found at {}", repo_path);
    }

    Ok(files)
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build 2>&1 | tail -3`
Expected: `Finished`

- [ ] **Step 3: Test init with a nonexistent wish**

Run: `cargo run -- wish init fake-wish 2>&1`
Expected: Error message `unknown wish 'fake-wish'`

- [ ] **Step 4: Commit**

```bash
git add src/wish.rs
git commit -m "feat: wish init fetches recipe from GitHub

kazam wish init <name> fetches the wish directory from the kazam
GitHub repo via the Contents API. Downloads each file into local
wishes/<name>/. Supports --force to overwrite and --dir for custom
install location."
```

---

### Task 4: Create the hubspot-icp wish directory

**Files:**
- Create: `wishes/hubspot-icp/wish.yaml`
- Create: `wishes/hubspot-icp/script.py`
- Create: `wishes/hubspot-icp/prompt.md`
- Create: `wishes/hubspot-icp/page.yaml`
- Create: `wishes/hubspot-icp/README.md`

The script.py is adapted from `/Users/tyler/maze-repos/maze-brain/scripts/generate-icp-data.py` - the proven recipe. Remove maze-specific hardcoded values and add comments marking customization points.

- [ ] **Step 1: Create `wishes/hubspot-icp/wish.yaml`**

```yaml
name: hubspot-icp
description: "Data-driven ICP from HubSpot deals + Apollo enrichment"
tags: [gtm, hubspot, apollo, icp]
env:
  - HUBSPOT_API_TOKEN
  - APOLLO_API_KEY
data_sources: [hubspot, apollo]
```

- [ ] **Step 2: Create `wishes/hubspot-icp/script.py`**

Copy from `/Users/tyler/maze-repos/maze-brain/scripts/generate-icp-data.py` and generalize:
- Replace maze-specific `LATE_STAGE_PREFIXES` with a clearly marked customization block at the top
- Add a comment block at the top explaining what to change
- Keep all the working logic (HubSpot search, Apollo enrichment, CSV output)

The script should be functional out of the box for any HubSpot + Apollo user who updates the prefixes.

- [ ] **Step 3: Create `wishes/hubspot-icp/prompt.md`**

Copy from `/Users/tyler/maze-repos/maze-brain/scripts/icp-prompt.md` and generalize - remove the hardcoded counts (the script prints those), replace with template language the agent fills in after running.

```markdown
# ICP Analysis Prompt

You have a CSV at `scripts/icp-data.csv` with customers, late-stage deals, and closed-lost deals. Each row has company firmographics from HubSpot + Apollo enrichment.

## Your task

Analyze this data and update the kazam YAML page at the path specified in the refresh block with a data-driven ICP. Structure your analysis as:

### 1. Customer DNA (P0 - highest confidence)
From existing customers, identify:
- **Company size sweet spot**: employee count range and median
- **Industry clusters**: which industries appear most
- **Geography**: where customers are concentrated
- **Tech stack signals**: common technologies (from Apollo)
- **Revenue/funding profile**: typical company stage and size
- What makes these companies similar? What's the archetype?

### 2. Pipeline validation (P1 - late-stage deals)
From late-stage deals (Business Validation, Negotiation, Closed Won):
- Do they match the customer DNA or diverge?
- Any new segments emerging in the pipeline?
- Deal size patterns

### 3. Closed-lost patterns (P2 - lighter review)
From closed-lost deals:
- Common loss reasons
- Are there company profiles that consistently lose? (wrong size, wrong industry, wrong stage)
- Any "near misses" worth understanding?

### 4. Updated ICP definition
Synthesize into:
- **Tier 1**: Companies that look like our customers (define the profile)
- **Tier 2**: Companies that look like our late-stage pipeline
- **Tier 3**: Worth pursuing but lower confidence
- **Disqualifiers**: Patterns from closed-lost that signal bad fit

Keep the existing page structure (kazam YAML with components) and replace the content with data-backed definitions. Use `type: table` with `columns` (key/label) and `rows` (key-value maps) for data tables.
```

- [ ] **Step 4: Create `wishes/hubspot-icp/page.yaml`**

```yaml
title: Ideal Customer Profile (ICP)
shell: standard
eyebrow: GTM
personas:
  - gtm
freshness:
  updated: "2025-01-01"
  review_every: quarterly
  owner: changeme@company.com
  refresh:
    mode: assisted
    steps:
      - run: scripts/generate-icp-data.py
      - prompt: "Read scripts/icp-prompt.md and scripts/icp-data.csv. Analyze customer DNA, pipeline validation, and closed-lost patterns. Update this page with data-backed tier definitions."
      - review: owner
components:
  - type: section
    heading: "Metrics"
    components:
      - type: markdown
        body: |
          Run the data pipeline to populate this page. See the refresh block above for instructions.
```

- [ ] **Step 5: Create `wishes/hubspot-icp/README.md`**

```markdown
# HubSpot ICP Wish

Build a data-driven Ideal Customer Profile page from your HubSpot CRM deals and Apollo company enrichment.

## Prerequisites

- `HUBSPOT_API_TOKEN` - HubSpot private app token with CRM read access (deals, companies, pipelines)
- `APOLLO_API_KEY` - Apollo.io API key (uses the paid `/organizations/enrich` endpoint, ~1 credit per company)
- Python 3.8+ with `requests` (run via `uv run --with requests` if not installed)

## What to customize

### script.py

- **`LATE_STAGE_PREFIXES`** (line ~15): These prefixes match your HubSpot deal stage labels for late-stage deals. The script prints all stage labels on first run - check the output and update these to match your pipeline.
- **`CLOSED_LOST_PREFIXES`** (line ~16): Same idea for closed-lost stages. The default uses `hs_is_closed_won=false` which works across all pipelines, so you may not need to change this.
- **`COMPANY_PROPS` / `DEAL_PROPS`**: Add any custom HubSpot properties you want in the CSV output.
- **Apollo enrichment**: If you don't have an Apollo key, the script still works - you just get HubSpot data only (no tech stacks, keywords, or funding data).

### page.yaml

- **`freshness.owner`**: Set to the page owner's email.
- **`eyebrow` and `personas`**: Adjust for your site's navigation structure.
- **Component structure**: The starter page is minimal. After the first analysis run, the agent will populate it with sections for Customer DNA, Pipeline Validation, Closed-Lost Patterns, Tier definitions, and Disqualifiers.

## How to run

1. Copy `script.py` to your site's `scripts/` directory
2. Copy `page.yaml` to your site's `site/` directory (rename as needed)
3. Set `HUBSPOT_API_TOKEN` and optionally `APOLLO_API_KEY` in your `.env`
4. Run: `uv run --with requests python3 scripts/generate-icp-data.py`
5. Check the CSV output at `scripts/icp-data.csv`
6. Have an agent read `scripts/icp-prompt.md` + the CSV and update the page

## What the output looks like

The finished page has:
- **Metrics**: row counts and data source summary
- **Customer DNA**: table of customers with firmographics + funding, tech stack signals across the customer base
- **Pipeline Validation**: late-stage deals confirming or extending the customer profile, deal size patterns
- **Closed-Lost Patterns**: table of losses with reasons, analysis of what kills deals
- **Tier 1/2/3**: data-backed company profile definitions at each confidence level
- **Disqualifiers**: hard and soft signals that indicate bad fit
- **Timing Indicators**: when to engage vs. when to wait
```

- [ ] **Step 6: Commit**

```bash
git add wishes/hubspot-icp/
git commit -m "feat: add hubspot-icp wish - first recipe in the library

Proven recipe from maze-brain: Python script pulls HubSpot deals +
Apollo enrichment, dumps CSV, prompt tells agent how to synthesize
into a kazam ICP page. README documents all customization points."
```

---

### Task 5: Add tests

**Files:**
- Modify: `src/wish.rs` (add test module at bottom)

- [ ] **Step 1: Add unit tests to `src/wish.rs`**

Add at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parses() {
        let entries = load_registry();
        assert!(!entries.is_empty(), "registry should have at least one entry");
        assert!(
            entries.iter().any(|e| e.name == "hubspot-icp"),
            "registry should contain hubspot-icp"
        );
    }

    #[test]
    fn registry_entries_have_required_fields() {
        for entry in load_registry() {
            assert!(!entry.name.is_empty(), "name must not be empty");
            assert!(!entry.description.is_empty(), "description must not be empty");
            assert!(!entry.path.is_empty(), "path must not be empty");
            assert!(
                entry.path.starts_with("wishes/"),
                "path should start with wishes/"
            );
        }
    }

    #[test]
    fn scan_local_returns_empty_for_missing_dir() {
        let wishes = scan_local(Path::new("/tmp/nonexistent-kazam-test"));
        assert!(wishes.is_empty());
    }

    #[test]
    fn scan_local_finds_wish_in_temp_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let wish_dir = tmp.path().join("wishes").join("test-wish");
        fs::create_dir_all(&wish_dir).unwrap();
        fs::write(
            wish_dir.join("wish.yaml"),
            "name: test-wish\ndescription: A test wish\n",
        )
        .unwrap();
        let wishes = scan_local(tmp.path());
        assert_eq!(wishes.len(), 1);
        assert_eq!(wishes[0].name, "test-wish");
    }

    #[test]
    fn init_rejects_unknown_wish() {
        let result = init("nonexistent-wish", None, false);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("unknown wish"),
        );
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --lib wish::tests 2>&1`
Expected: 5 tests pass

- [ ] **Step 3: Run the full test suite**

Run: `cargo test 2>&1 | tail -5`
Expected: All tests pass (74 existing + 5 new)

- [ ] **Step 4: Commit**

```bash
git add src/wish.rs
git commit -m "test: add wish module tests

Registry parsing, local scanning, and init validation."
```

---

### Task 6: End-to-end verification and final cleanup

**Files:**
- None (verification only)

- [ ] **Step 1: Verify `kazam wish list` shows the registry**

Run: `cargo run -- wish list`
Expected output includes:
```
  REGISTRY (kazam wish init <name> to install)
    hubspot-icp          Data-driven ICP from HubSpot deals + Apollo enrichment
```

- [ ] **Step 2: Verify `kazam wish list --json` works**

Run: `cargo run -- wish list --json 2>&1`
Expected: valid JSON array with `hubspot-icp` entry

- [ ] **Step 3: Verify `kazam wish init hubspot-icp` fetches from GitHub**

Run in a temp directory:
```bash
cd /tmp && mkdir kazam-wish-test && cd kazam-wish-test && kazam wish init hubspot-icp
```
Expected: Downloads files to `wishes/hubspot-icp/`, prints success message.

Run: `ls wishes/hubspot-icp/`
Expected: `README.md  page.yaml  prompt.md  script.py  wish.yaml`

- [ ] **Step 4: Verify `kazam wish init hubspot-icp` rejects duplicate without --force**

Run: `kazam wish init hubspot-icp` (same directory)
Expected: Error about already existing

Run: `kazam wish init hubspot-icp --force`
Expected: Re-downloads successfully

- [ ] **Step 5: Verify local wish appears in list after init**

Run: `kazam wish list`
Expected: `hubspot-icp` now appears under `LOCAL`, not `REGISTRY`

- [ ] **Step 6: Clean up temp directory**

```bash
rm -rf /tmp/kazam-wish-test
```

- [ ] **Step 7: Final commit with any cleanup**

```bash
git add -A
git commit -m "chore: wishes-as-recipes implementation complete"
```

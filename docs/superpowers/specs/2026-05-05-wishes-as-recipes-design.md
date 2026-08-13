# Wishes as Recipes - Design Spec

**Date:** 2026-05-05
**Status:** Approved
**Branch:** `docs/1.5.0-positioning`

## Problem

The current `kazam wish` system scaffolds static pages - workspace, questions, agent shell-out, YAML output. In practice, nobody uses it. The real value is self-refreshing docs: a data pipeline (script → structured output → agent analysis → kazam page) backed by a `refresh` block that tells agents how to keep the page alive.

We proved this with the ICP page on maze-brain: a Python script pulls HubSpot deals + Apollo enrichment, dumps CSV, and a prompt tells the agent how to synthesize it into a kazam YAML page. The refresh block on the page captures the full loop (run script → prompt agent → human review).

## Solution

Replace the old wish system with a **recipe library**. Each wish is a directory containing a sample script, analysis prompt, starter page, and customization guide. Kazam manages the library; the agent does the adaptation and execution in its normal session.

## Wish directory structure

Each wish is a directory with a fixed layout:

```
wishes/<name>/
  wish.yaml     # metadata
  script.py     # sample data-gathering script
  prompt.md     # analysis instructions for the agent
  page.yaml     # starter kazam page with refresh block pre-wired
  README.md     # what to customize (human + agent readable)
```

### wish.yaml

```yaml
name: hubspot-icp
description: "Data-driven ICP from HubSpot deals + Apollo enrichment"
tags: [gtm, hubspot, apollo, icp]
env:
  - HUBSPOT_API_TOKEN
  - APOLLO_API_KEY
data_sources: [hubspot, apollo]
```

Fields:
- `name`: unique identifier, matches the directory name
- `description`: one-line summary shown in `kazam wish list`
- `tags`: freeform tags for agent discoverability
- `env`: required environment variables (script won't work without these)
- `data_sources`: which external systems the script talks to

### script.py

A real, runnable data-gathering script. Not a template with placeholders - a working example that an agent can read, understand, and adapt. The README explains what's org-specific.

Conventions:
- Loads env vars from `.env` in the site root
- Outputs structured data (CSV, JSON) to `scripts/`
- Outputs an analysis prompt to `scripts/`
- Prints progress to stderr, results to stdout
- Python preferred (most agents handle it well), but any language works

### prompt.md

The analysis instructions passed to the agent after the script runs. Tells the agent what to look for in the data and how to structure the kazam page output. This is the portable part - it works regardless of how the script was customized.

### page.yaml

A starter kazam page with the `refresh` block pre-wired. The `refresh.steps` point at the script and prompt:

```yaml
freshness:
  updated: "2026-01-01"
  review_every: quarterly
  owner: changeme@company.com
  refresh:
    mode: assisted
    steps:
      - run: scripts/generate-icp-data.py
      - prompt: "Read scripts/icp-prompt.md and scripts/icp-data.csv. Analyze..."
      - review: owner
```

### README.md

The customization guide. Written for both humans and agents. Structured as:

1. **What this wish does** - one paragraph
2. **Prerequisites** - API keys, access needed
3. **What to customize** - specific lines/values that are org-dependent, with explanations
4. **How to run** - step-by-step after customization
5. **What the output looks like** - description of the page structure

The "what to customize" section is the critical part. Example:

```markdown
## What to customize

- `LATE_STAGE_PREFIXES` in script.py (line 55): Change to match your
  HubSpot deal stage labels. Run `kazam wish init hubspot-icp` and check
  the stage map output to see your stages.
- `CLOSED_LOST_PREFIXES` in script.py (line 56): Same as above for your
  closed-lost stage labels.
- `freshness.owner` in page.yaml: Set to the page owner's email.
```

## CLI commands

### `kazam wish list`

Scans local `wishes/` directory and merges with the embedded registry. Output:

```
  Available wishes:

  LOCAL
    hubspot-icp        Data-driven ICP from HubSpot deals + Apollo enrichment

  REGISTRY (kazam wish init <name> to install)
    linear-ownership   Map people to what they own from Linear projects
    attention-calls    Call analysis trends from Attention
    ashby-pipeline     Hiring pipeline snapshot from Ashby
```

Local wishes appear first. Registry wishes that are already installed locally are shown under LOCAL, not duplicated.

### `kazam wish init <name>`

If `<name>` matches a registry entry, fetches the wish directory from GitHub into local `wishes/<name>/`. If it already exists locally, warns and skips (pass `--force` to overwrite).

Fetch mechanism: GitHub Contents API (`GET /repos/tdiderich/kazam/contents/wishes/<name>`) - lists files in the directory, then fetches each file's content. No auth needed for public repos. No git clone or submodules.

After fetching:

```
  ✨ Installed wish: hubspot-icp

  wishes/hubspot-icp/
    wish.yaml     metadata
    script.py     sample data-gathering script
    prompt.md     analysis prompt
    page.yaml     starter page with refresh block
    README.md     customization guide

  Next: have your agent read wishes/hubspot-icp/README.md and adapt the script.
```

### Flags

- `--dir <path>`: install to a specific directory instead of `wishes/`
- `--force`: overwrite existing local wish
- `--json`: machine-readable output for `list`

## Registry

The binary embeds a `registry.yaml` - a flat list of wish metadata:

```yaml
- name: hubspot-icp
  description: "Data-driven ICP from HubSpot deals + Apollo enrichment"
  tags: [gtm, hubspot, apollo, icp]
  path: wishes/hubspot-icp
- name: linear-ownership
  description: "Map people to what they own from Linear projects"
  tags: [engineering, linear, ownership]
  path: wishes/linear-ownership
```

The `path` field is relative to the kazam repo root. Used to construct the GitHub API URL for fetching.

The registry is updated with each kazam release. Users on older binaries see fewer wishes but can still manually copy wish directories from the repo.

## What gets deleted

The entire old wish module:
- `src/wish/mod.rs` - workspace scaffolding, agent detection/shelling, grant flow
- `src/wish/deck.rs` - deck wish templates
- `src/wish/brief.rs` - brief wish templates
- `src/wish/dashboard.rs` - dashboard wish templates

Removed concepts:
- Workspace scaffolding (`wish-deck/`, `questions.md`, `README.md`)
- Agent detection and shell-out (`Agent` enum, `run_agent()`, `detect_agent()`)
- `--yolo`, `--dry-run`, `--stdout` flags
- `MCP_GUIDANCE` constant
- `Wish` struct with embedded prompts

## What stays

- `RefreshValue` / `RefreshConfig` / `RefreshMode` / `RefreshStep` types in `types.rs` - the contract between recipes and agents
- `prompts` module - separate system for agent prompt templates
- `actions` module - GitHub Actions scaffolding

## New module structure

```
src/wish/
  mod.rs        # list, init, registry loading, GitHub fetch
  registry.yaml # embedded wish index (include_str!)
```

## Starter wishes to ship

1. **hubspot-icp** - the proven recipe from maze-brain (HubSpot deals + Apollo enrichment → ICP page)
2. **linear-ownership** - map Linear projects/issues to people → "who owns what" page (the next recipe Tyler wants to build)

More can be added via PR without a kazam release - just add the directory to `wishes/` and an entry to `registry.yaml`.

## Workflow end-to-end

1. User (or agent) runs `kazam wish list` → sees available wishes
2. Runs `kazam wish init hubspot-icp` → fetches to local `wishes/hubspot-icp/`
3. Agent reads `wishes/hubspot-icp/README.md` → understands what to customize
4. Agent adapts `script.py` for the user's HubSpot setup (stage labels, properties, etc.)
5. Agent copies `page.yaml` into `site/` with correct paths and owner
6. Agent copies adapted script into `scripts/`
7. Runs the script → CSV + prompt generated
8. Agent reads CSV + prompt → writes the kazam page
9. Page has `refresh` block → next time it's stale, an agent (or CI) can re-run the loop

## Future considerations (not in scope)

- `kazam wish run <name>` - execute the refresh steps directly from CLI
- Registry served from a URL instead of embedded (always-latest without binary updates)
- Community wish contributions via PR
- `kazam wish update <name>` - pull latest version from registry

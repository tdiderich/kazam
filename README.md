# kazam

**Local infrastructure for coding agents.**

Instant codebase context, live visibility into what your agent is doing, and durable execution, one Rust binary, no cloud. Structured YAML pages with freshness tracking, sidecar annotations, and an MCP server let agents create, review, annotate, and refresh content the same way your team does.

---

## Why

Every company has docs that were true when someone wrote them. Nobody owns them now. Nobody knows which ones are wrong. Your AI agent can't read your wiki without burning tokens on `find` and `grep`. Your doc toolchain has more dependencies than your product.

kazam fixes this with structured YAML pages, not wiki pages. Every page can declare an owner, review cadence, and sources of truth. The build flags what's overdue — yellow banners for due-soon, red for stale, a `stale.md` manifest your agent can act on, and a `_health.html` dashboard showing freshness across the whole site. An MCP server gives agents direct read/search/write/annotate access to your knowledge base. Sidecar annotations capture human context that data sources can't — corrections, competitive intel, meeting notes — and feed it back into the next refresh cycle. One Rust binary, static HTML out, host anywhere.

## Capabilities

- **YAML content** — 30+ components, three shells (standard, document, deck), zero runtime JS, theme-aware via CSS vars
- **Freshness tracking** — owner, review cadence, sources of truth, stale banners at build time
- **Sidecar annotations** — human context stored as YAML files in `.kazam/annotations/`, with 14-day decay, status tracking, and build-time rendering
- **MCP server** — 8 tools: agents read, search, write, annotate, and update content directly. stdio + HTTP transports with bearer token auth for remote access
- **Annotation-aware refresh** — prompt templates read annotations as highest-priority source; agents update annotation status after each cycle
- **AI tool packs**: `kazam install <curata-url>` compiles a curata pack page into managed CLAUDE.md / .cursorrules blocks; idempotent, drift-detectable via content hash
- **Wishes** — portable agent recipes: deal-360, debrief, audit-fix, freshness-notifier, and more
- **Notion ingest** — import databases, pages, and child pages with `--all` discovery mode
- **Audit** — structural quality checks: freshness compliance, component validation, annotation health
- **Agent workspace** — codebase anatomy index, task tracking, visual board
- **Validation** — structure checks, orphan detection, broken internal links

## Install

```bash
# Homebrew (macOS / Linux)
brew install tdiderich/tap/kazam

# Cargo (any platform with Rust)
cargo install kazam

# Bleeding edge
cargo install --git https://github.com/tdiderich/kazam
```

## Quickstart

```bash
kazam init my-docs && cd my-docs
kazam dev .                              # live reload at localhost:3000
kazam prompt show add-page | claude -p   # agent creates a page
kazam validate .                         # check structure
kazam build . --out dist --release       # publish
# later...
kazam prompt show refresh | claude -p    # agent refreshes stale pages
```

**[Docs + live examples](https://tdiderich.github.io/kazam/)** · **[Components](https://tdiderich.github.io/kazam/components/index.html)** · **[Themes](https://tdiderich.github.io/kazam/themes.html)** · **[Deploy recipes](https://tdiderich.github.io/kazam/deploy.html)**

## Annotations

Annotations are human context that data sources can't capture. They live as sidecar YAML files — not embedded in page content — so agents and humans can write them without collisions.

```bash
# Add an annotation from a call
kazam annotate customers/acme.yaml "evaluating Wiz alongside us" \
  --section competitive --author tyler

# List annotations on a page
kazam mcp   # then: list_annotations { "page": "customers/acme.yaml" }

# Annotations render inline at build time with age indicators and status badges
kazam build .
```

Annotations feed into the refresh cycle. When an agent refreshes a page, it reads pending annotations first, incorporates what's relevant, and marks them as `incorporated`. The 14-day decay window ensures stale annotations surface for review.

**MCP tools:** `annotate_page`, `list_annotations`, `update_annotation` — available over stdio and HTTP (`--local` / `--remote --token`).

## Agent Graph Language (`.agl`)

`.agl` turns a task into a static directed graph with mandatory approval gates, so an agent runs inside deterministic execution boundaries instead of free-form natural-language instructions.

```bash
# Parse + statically validate a spec: reachability, cycles, branch integrity,
# invariant soundness (a write reachable without its required gate)
kazam agl validate my-task.agl

# See the graph as a plain ASCII diagram - states, actions, transitions
kazam agl flow my-task.agl

# Cross-check requires: against every call()/map() in the flow, then compile
# to a portable skill (Claude/Cursor/Codex) with a preflight tool check and
# the same ASCII flow diagram baked in
kazam agl skill my-task.agl --target claude --out .claude/skills/

# Compile straight to a natural-language system-prompt block instead
kazam agl export my-task.agl
```

Specs and shared fragments live under `~/.kazam/agl/{specs,shared}/` by convention - see `CONTRIBUTING.md` for the grammar (`import`, `requires:`) and the hub layout.

## Agent workspace

The workspace engine makes agents fast when working inside a kazam project.

```bash
cd your-repo
kazam workspace init --agent claude
kazam board
```

### Anatomy — persistent codebase context

`kazam ctx scan` builds a two-tier index:

- **Summary** (`.kazam/ctx/anatomy.tsv`) — root files + directory rollups with file counts, token estimates, and descriptions
- **Detail** (`.kazam/ctx/anatomy/<dir>.tsv`) — individual files per directory

Agents read the summary, drill into what they need. No `find`. No `grep`. No wasted turns.

### Benchmarks

Tested with Sonnet 4.6, identical prompts, git worktrees — kazam-equipped vs vanilla Claude Code:

| Repo | Files | Task | Cost | Speed |
|---|---|---|---|---|
| Internal tools repo | 8,000+ | Add CLI flag + thread to SQL | **45% cheaper** | **41% faster** |
| Plugin repo | 126 | Add config field to skill | **44% cheaper** | **59% faster** |
| React/TS app | 89 | Add loading skeleton | **46% cheaper** | **47% faster** |
| Python service | 233 | Cross-cutting model change | **45% cheaper** | **44% faster** |

Input tokens per turn dropped 81-94% across the board.

### Task tracking

```bash
kazam track add "Fix the auth middleware" --priority 1
kazam track claim kz-a1b2 --name claude
kazam track close kz-a1b2 --reason "patched token validation"
kazam track ready --json    # unblocked tasks sorted by priority
```

Tasks live in `.kazam/track/tasks.yaml`. They survive session restarts, context compaction, and agent handoffs.

### Board

```bash
kazam board
```

Auto-refreshing local dashboard — task status, codebase anatomy, activity log. Built with kazam's own rendering engine.

### Reading files: `open` and `show`

Agents can open a browser tab. They can't reach into your editor and put the right file in front of you.

```bash
kazam open notes.md        # browser: rendered, live-reloading, editable
kazam show config.yaml     # terminal: pretty-printed with syntax colors
```

Both take one file, `.md`, `.yaml`, or `.json`. Anything else is rejected by name.

`kazam open` serves a small API next to the page, which is the point: you type notes in the browser, the agent reads them over `GET /api/content` without you saving anything. `GET /api/status` reports whether you have unsaved edits, whether the file still parses, and the parse error if it doesn't.

Hit Save or Cmd+S to write the file. Nothing autosaves, because writing on every keystroke would churn the file and fire your agent hooks over and over.

Unsaved edits are never discarded. If the file changes on disk while your buffer is dirty, the page shows a conflict bar with **Keep mine** and **Load from disk** instead of reloading over your work, and Save is refused until you pick one.

## MCP server

```bash
# Local — stdio transport for Claude Code, Cursor, etc.
kazam mcp

# Remote — HTTP transport with bearer token auth
kazam mcp --transport http --remote --token $KAZAM_MCP_TOKEN --port 8080
```

8 tools: `read_page`, `list_pages`, `search`, `get_config`, `write_page`, `annotate_page`, `list_annotations`, `update_annotation`.

The `--local` flag (default) binds to 127.0.0.1. The `--remote` flag binds to 0.0.0.0 and requires a bearer token via `--token` or `KAZAM_MCP_TOKEN` env var.

## AI tool packs

Install shared AI agent rules straight from a curata instance into your repo:

```bash
# Fetch a pack page and write CLAUDE.md + .cursorrules
kazam install curata.ai/pages/pack-maze-engineering-standards

# Private instances: pass a key, or set KAZAM_CURATA_API_KEY
kazam install my-curata.internal/pages/company-standards --api-key <key>

# Public pack, no key needed (the shareable /p/ URL)
kazam install curata.ai/p/maze/company-standards

# Pick which tools to write, regardless of what the pack declares
kazam install curata.ai/pages/company-standards --cli claude,agents

# Check installed packs for drift against their source pages
kazam check
```

A pack is an ordinary curata page (see the `ai-tool-pack` template) carrying a top-level `pack:` block:

```yaml
pack:
  targets: [claude, cursor]   # optional - omit for the default pair
```

Targets map to files: `claude` (CLAUDE.md), `cursor` (.cursorrules), `agents` (AGENTS.md, read by 30+ tools), plus `windsurf`, `copilot`, `gemini`, `aider`.

### Install scope: user (default) or repo

`kazam install` defaults to a **user** install (`~/.claude`), so a personal guardrail pack (a voice or de-AI ruleset) applies to every project you work in:

```bash
kazam install curata.ai/p/maze/voice-rules --user --allow-hooks   # explicit
kazam install curata.ai/p/maze/voice-rules --allow-hooks          # same (user is default)
```

Use `--repo` to pin the install to the current project instead (writes the project's `CLAUDE.md`, `.cursorrules`, etc.). An explicit `--dir` also implies repo scope. When you pass neither flag and the terminal is interactive, install asks which scope you want.

User scope writes the `claude` target (`~/.claude/CLAUDE.md`) and any hooks. Other rules targets have no shared user-level home, so under `--user` they warn and are skipped; install them with `--repo` if you need them.

### Declarative hooks (optional)

A pack can carry guardrail hooks. Packs never ship executable code: a hook is declarative config (block-on-match, allowlist, inject, review) that the trusted `kazam pack-hook` runner interprets. Install them with `--allow-hooks`, which writes the config to `.claude/kazam-packs/<slug>.hooks.yaml` (or `~/.claude/kazam-packs/` at user scope) and registers the runner in `settings.json` with an absolute path to that config, so the hook resolves no matter what directory the session runs from:

```yaml
pack:
  targets: [claude]
  hooks:
    - kind: block_on_match
      on: { tool: "Write|Edit" }
      mode: word                # substring (default) | word (word-boundary)
      patterns: ["delve", "foster"]
      message: "Blocked: AI-slop word. Use plain language."
```

`on.tool` is passed to the harness verbatim, so it matches any tool the harness knows, not only `Write|Edit`. An MCP tool by full name (`mcp__claude_ai_Slack__slack_send_message`) or prefix works too. On `block_on_match`, `mode: word` matches only on word boundaries (so `foster` won't trip on `fostering`), and an optional `field:` scopes the scan to one `tool_input` argument (like `field: text` for a Slack message body) instead of the whole tool input.

The runner has no network or arbitrary-write capability, so a hostile pack can at worst block your own tool calls or inject visible text, never exfiltrate. Hook-bearing packs stay off by default; `--allow-hooks` is the consent.

Pages without the marker are refused (`--force` overrides), and pages with unfilled `{{template}}` variables never install. `kazam validate` enforces pack structure server-side too: a `pack:` page must have at least one non-empty markdown component. The markdown components compile, in order, into a managed block inside `CLAUDE.md` and `.cursorrules`:

- Content outside the block is never touched, so your existing rules survive.
- Reinstalling replaces the block in place; installs are idempotent.
- Multiple packs coexist, one block per pack slug.
- The block header records source URL and content hash, so drift against the source page is detectable.

Because packs are pages, they inherit everything the platform already does: versioning, annotations, freshness tracking, search, and MCP access.

## curata - the hosted platform

**kazam** is the OSS engine. A Rust CLI that builds structured YAML pages into themed HTML, with freshness tracking, sidecar annotations, and an MCP server. Free forever, MIT licensed.

**curata** ([github.com/tdiderich/curata](https://github.com/tdiderich/curata)) is the OSS app. A Next.js dashboard for browsing, annotating, and managing kazam pages. Deploy with Docker Compose, expose an API for agents, and serve an MCP server so any AI client can read and write your knowledge base directly. Also MIT licensed.

**curata.ai** ([curata.ai](https://curata.ai)) is the hosted cloud: shared infrastructure for your whole organization's coding agents, not just one machine. Free to use, sign up, connect your agent via MCP, and start capturing AI outputs.

Connect via the built-in MCP server — add this to your editor's MCP config:

```json
{ "type": "url", "url": "https://curata.ai/api/mcp/stream" }
```

**The knowledge loop:** agents write structured pages → humans review and annotate in curata → agents read the annotations on the next cycle → the knowledge base compounds over time. Each annotation narrows what the agent needs to reconsider; each refresh closes the loop.

See [PRODUCT.md](PRODUCT.md) for the full product plan.

## CLI Reference

Generated from `--help` metadata, not hand-maintained. Regenerate with `kazam cli-reference --write`; CI fails if this drifts from the real CLI surface.

<!-- CLI_REFERENCE:START -->

### `kazam`

Local infrastructure for coding agents: context, visibility, durable execution

#### `kazam build`

Build a site from a directory of .yaml files

- `dir` - Directory of .yaml source files

| Flag | Default | Description |
|---|---|---|
| `--out, -o` | `_site` | Output directory for the built site |
| `--release, -r` |  | Minify HTML, CSS, and JS in the output |
| `--allow-orphans` |  | Silence the orphan-page check (broken links still reported). Useful for draft pages you haven't wired into nav yet |
| `--json` |  | Emit structured NDJSON instead of human-readable output |
| `--no-manifest` |  | Skip emitting site.json manifest |
| `--no-search` |  | Skip emitting search.json index |
| `--no-health` |  | Skip emitting _health.html health dashboard |

#### `kazam dev`

Watch source, rebuild on change, serve at localhost:PORT

- `dir` - Directory of .yaml source files

| Flag | Default | Description |
|---|---|---|
| `--out, -o` | `_site` | Output directory for the built site |
| `--port, -p` | `3000` | Port to serve the live-reloading site on |

#### `kazam init`

Scaffold a new kazam site in <NAME>/

- `name` - Directory to create, also used as the site name

#### `kazam agents`

Print the LLM authoring guide (full AGENTS.md to stdout)

#### `kazam install`

Install an AI tool pack from a curata instance (writes CLAUDE.md + .cursorrules)

- `url` - Pack URL: https://<instance>/pages/<slug>, /p/<org>/<slug>, or <instance>/<slug>

| Flag | Default | Description |
|---|---|---|
| `--api-key` |  | API key for the curata instance (falls back to KAZAM_CURATA_API_KEY) |
| `--dir, -d` | `.` | Directory to write config files into (implies --repo if not "."; repo scope) |
| `--force` |  | Install even if the page has no pack: marker |
| `--cli` |  | Override the pack's declared targets. Repeatable or comma-separated. Supported: claude, cursor, agents, windsurf, copilot, gemini, aider |
| `--allow-hooks` |  | Also install the pack's declarative hooks (writes hook config + registers the kazam runner in .claude/settings.json). Off by default |
| `--user` |  | Install for the current user (~/.claude), shared across every project. This is the default when no scope flag is given |
| `--repo` |  | Install into this repo only (writes at --dir). Mutually exclusive with --user |

#### `kazam check`

Check installed packs for drift against their curata source pages

| Flag | Default | Description |
|---|---|---|
| `--dir, -d` | `.` | Directory to scan for installed packs |
| `--api-key` |  | API key for the curata instance (falls back to KAZAM_CURATA_API_KEY) |

#### `kazam pack-hook`

Internal: run a declarative pack hook (registered in settings by install)

| Flag | Default | Description |
|---|---|---|
| `--pack` |  | Pack slug whose hook config to load |
| `--index` |  | Index of the hook within the pack's config |
| `--config` |  | Absolute path to the hook config, set automatically by install. When absent (pre-1.8.0 installs), falls back to walking up from `dir` for the old `.kazam/packs/` location |
| `--dir` | `.` | Project directory (default: current directory) |

#### `kazam wish`

Grant a wish — install a recipe for self-refreshing docs

##### `kazam wish list`

List available wishes (local + registry)

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Machine-readable JSON output |

##### `kazam wish init`

Install a wish from the registry into local wishes/

- `name` - Name of the wish to install

| Flag | Default | Description |
|---|---|---|
| `--dir` |  | Install to a specific directory instead of wishes/ |
| `--force` |  | Overwrite existing local wish |

#### `kazam track`

Manage the work graph — tasks, dependencies, activity log

| Flag | Default | Description |
|---|---|---|
| `--dir, -d` | `.` | Project directory (default: current directory) |

##### `kazam track init`

Initialize .kazam/track/ with empty stores

| Flag | Default | Description |
|---|---|---|
| `--skunkworks` |  | Gitignore .kazam/ for shared repos |

##### `kazam track add`

Add a new task

- `title` - One-line task title

| Flag | Default | Description |
|---|---|---|
| `--priority, -p` | `2` | 0 (highest) through 9 (lowest) |
| `--type, -t` | `task` | Freeform category, e.g. task, bug, epic |
| `--owner` | `agent` | Who owns closing this: agent or human |
| `--parent` |  | Parent task ID, for subtasks under an epic |
| `--blocks` |  | Task IDs this one blocks, comma-separated |
| `--assign` |  | Assignee name, claims the task immediately |
| `--note` |  | Freeform note attached to the task |
| `--json` |  | Machine-readable JSON output |

##### `kazam track ready`

Show tasks with no open blockers, sorted by priority

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Machine-readable JSON output |

##### `kazam track claim`

Atomically claim a task (set assignee + active)

- `id` - Task ID

| Flag | Default | Description |
|---|---|---|
| `--name` |  | Claimant name (alias: --as) |
| `--json` |  | Machine-readable JSON output |

##### `kazam track close`

Close a completed task

- `id` - Task ID

| Flag | Default | Description |
|---|---|---|
| `--reason` |  | What was done, recorded on the task |
| `--json` |  | Machine-readable JSON output |

##### `kazam track block`

Mark a task as blocked

- `id` - Task ID

| Flag | Default | Description |
|---|---|---|
| `--reason` |  | Why it's blocked, recorded on the task |
| `--json` |  | Machine-readable JSON output |

##### `kazam track list`

List tasks (optionally filtered)

| Flag | Default | Description |
|---|---|---|
| `--status` |  | Filter by status: open, closed, blocked |
| `--assignee` |  | Filter by assignee name |
| `--json` |  | Machine-readable JSON output |

##### `kazam track tree`

Show the task tree

| Flag | Default | Description |
|---|---|---|
| `--filter` | `all` | all, open, or closed |
| `--json` |  | Machine-readable JSON output |

##### `kazam track show`

Show full details for a task

- `id` - Task ID

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Machine-readable JSON output |

##### `kazam track import`

Import tasks from a markdown plan (## headings → epics, - bullets → tasks)

- `file` - Path to a markdown file

| Flag | Default | Description |
|---|---|---|
| `--dry-run` |  | Preview without creating tasks |
| `--json` |  | Machine-readable JSON output |

##### `kazam track dep`

Manage dependencies

###### `kazam track dep add`

Add a dependency: BLOCKER blocks BLOCKED

- `blocker` - Task ID that must close first
- `blocked` - Task ID that's blocked

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Machine-readable JSON output |

###### `kazam track dep rm`

Remove a dependency

- `blocker` - Task ID that must close first
- `blocked` - Task ID that's blocked

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Machine-readable JSON output |

##### `kazam track log`

Show or add to the activity log

| Flag | Default | Description |
|---|---|---|
| `--limit` | `25` | Max entries to show |
| `--json` |  | Machine-readable JSON output |

###### `kazam track log add`

Add a manual log entry

- `title` - One-line entry title

| Flag | Default | Description |
|---|---|---|
| `--source` |  | Where this came from, freeform |
| `--severity` | `info` | info, warning, or major |
| `--task-id` |  | Associate with an existing task ID |
| `--json` |  | Machine-readable JSON output |

#### `kazam ctx`

Manage context intelligence — file anatomy, learnings, bugs

| Flag | Default | Description |
|---|---|---|
| `--dir, -d` | `.` | Project directory (default: current directory) |

##### `kazam ctx init`

Initialize .kazam/ctx/ (optionally scan files)

| Flag | Default | Description |
|---|---|---|
| `--scan` |  | Run an initial anatomy scan right away |
| `--skunkworks` |  | Gitignore .kazam/ for shared repos |

##### `kazam ctx scan`

Scan project files and update anatomy

| Flag | Default | Description |
|---|---|---|
| `--check` |  | Report drift without writing changes |
| `--json` |  | Machine-readable JSON output |

##### `kazam ctx status`

Show context status summary

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Machine-readable JSON output |

##### `kazam ctx describe`

Update a file's anatomy description (agent-enriched)

- `file` - Path to the file, relative to the project
- `description` - What this file actually does

##### `kazam ctx learn`

Record a learning

- `text` - The lesson learned, in one or two sentences

| Flag | Default | Description |
|---|---|---|
| `--category` | `preference` | preference, correction, or gotcha |
| `--json` |  | Machine-readable JSON output |

##### `kazam ctx learnings`

List all learnings

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Machine-readable JSON output |

##### `kazam ctx bug`

Record a bug encounter

- `symptom` - What went wrong, in one or two sentences

| Flag | Default | Description |
|---|---|---|
| `--file` |  | File path the bug is associated with |
| `--json` |  | Machine-readable JSON output |

##### `kazam ctx bugs`

List bugs (optionally filtered by file path)

| Flag | Default | Description |
|---|---|---|
| `--file` |  | Only show bugs associated with this file path |
| `--json` |  | Machine-readable JSON output |

##### `kazam ctx resolve`

Resolve a bug with a fix description

- `id` - Bug ID

| Flag | Default | Description |
|---|---|---|
| `--fix` |  | How it was fixed |
| `--json` |  | Machine-readable JSON output |

##### `kazam ctx correction`

Record a correction (agent got something wrong)

- `mistake` - What the agent did wrong
- `correction` - What to do instead

| Flag | Default | Description |
|---|---|---|
| `--file` |  | File path the correction is associated with |
| `--json` |  | Machine-readable JSON output |

##### `kazam ctx corrections`

List all corrections

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Machine-readable JSON output |

##### `kazam ctx consolidate`

Consolidate stale data (remove old resolved bugs, deduplicate learnings)

| Flag | Default | Description |
|---|---|---|
| `--days` | `30` | Only consolidate entries older than this many days |
| `--json` |  | Machine-readable JSON output |

##### `kazam ctx hooks`

Manage agent hooks (install/uninstall/status)

###### `kazam ctx hooks install`

Install hook scripts and register with agent

| Flag | Default | Description |
|---|---|---|
| `--agent` | `claude` | Which agent to register hooks for |

###### `kazam ctx hooks uninstall`

Remove all hooks

###### `kazam ctx hooks status`

Show hook installation status

#### `kazam board`

Live dashboard — renders .kazam/ state as a visual board

- `dir` - Project directory (default: current directory)

| Flag | Default | Description |
|---|---|---|
| `--port, -p` | `3001` | Port to serve the board on |

#### `kazam open`

Open a file (.md, .yaml, .json) in the browser with live reload and inline editing

- `path` - Path to the file to open

| Flag | Default | Description |
|---|---|---|
| `--port, -p` | `3002` | Port to serve the file view on |

#### `kazam show`

Pretty-print a file (.md, .yaml, .json) in the terminal

- `path` - Path to the file to show

#### `kazam workspace`

Initialize the full agent workspace (track + ctx + hooks) in one shot

| Flag | Default | Description |
|---|---|---|
| `--dir, -d` | `.` | Project directory (default: current directory) |

##### `kazam workspace init`

Initialize track + ctx + scan + hooks in one shot

| Flag | Default | Description |
|---|---|---|
| `--agent` | `claude` | Agent to register hooks for |
| `--skunkworks` |  | Gitignore .kazam/ for shared repos |
| `--sass` | `some` | Sass level for human blocker callouts (none, some, lots) |

##### `kazam workspace status`

Show workspace status

##### `kazam workspace sass`

Set the sass level for human blocker callouts

- `level` - none, some, or lots

##### `kazam workspace skunkworks`

Toggle skunkworks mode (gitignore .kazam/)

- `action` - enable or disable

#### `kazam validate`

Validate page YAML files against component schemas and structural rules

- `dir` - Directory of .yaml source files to validate (default: current directory)

| Flag | Default | Description |
|---|---|---|
| `--file` |  | Validate a single YAML file instead of the whole directory |
| `--pretty` |  | Human-readable output (default is JSON) |

#### `kazam mcp`

Run an MCP server over stdio for AI client integration

- `dir` - Site directory to serve

| Flag | Default | Description |
|---|---|---|
| `--allow-writes` |  | Allow write operations (write_page, annotate_page, update_annotation) |
| `--transport` | `stdio` | Transport: stdio (default) or http |
| `--port` | `8080` | Port for HTTP transport |
| `--local` |  | Bind to localhost only (default for http). Mutually exclusive with --remote |
| `--remote` |  | Bind to all interfaces (0.0.0.0) for remote access. Requires --token or KAZAM_MCP_TOKEN |
| `--token` |  | Bearer token for remote HTTP auth. Also reads KAZAM_MCP_TOKEN env var |

#### `kazam freshness`

Show freshness status for all pages in the site

- `dir` - Site directory

##### `kazam freshness show`

Show freshness status for all pages (default)

| Flag | Default | Description |
|---|---|---|
| `--pretty` |  | Human-readable table output (default is JSON) |
| `--threshold` |  | Days since last update before a page counts as stale |

##### `kazam freshness review`

List stale pages for review with recommended actions

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Output as JSON (default is human-readable) |

##### `kazam freshness act`

Take action on a stale page: archive, refresh, or skip

- `path` - Path to the page YAML file (relative to site dir)
- `action` - Action to take

##### `kazam freshness notify`

Generate a digest of stale pages grouped by owner (for Slack/email)

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Output as JSON instead of markdown |

##### `kazam freshness drift`

Check if source-of-truth files have changed since pages were last updated

| Flag | Default | Description |
|---|---|---|
| `--pretty` |  | Human-readable table output (default is JSON) |
| `--repo` |  | Additional repo mapping: PREFIX=LOCAL (can repeat) |

#### `kazam voice`

Show or manage the site's voice configuration

- `dir` - Site directory

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Output as JSON |

#### `kazam prompt`

Manage prompt templates for agent workflows

| Flag | Default | Description |
|---|---|---|
| `--dir, -d` | `.` | Project directory (default: current directory) |

##### `kazam prompt list`

List all prompts in the prompts/ directory

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Output as JSON |

##### `kazam prompt show`

Show a specific prompt (default: raw system_prompt text; --json for full struct)

- `name` - Prompt name (filename without .yaml extension)

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Output as JSON (default is the raw system_prompt text) |

##### `kazam prompt init`

Scaffold a new prompt file

- `name` - Prompt name

#### `kazam actions`

Manage GitHub Action workflow templates

| Flag | Default | Description |
|---|---|---|
| `--dir, -d` | `.` | Project directory (default: current directory) |

##### `kazam actions list`

List available action templates

##### `kazam actions init`

Initialize an action template in .github/workflows/

- `name` - Template name (validate, freshness, build)

#### `kazam audit`

Audit site health — freshness, structural quality, and completeness

- `dir` - Site directory

| Flag | Default | Description |
|---|---|---|
| `--pretty` |  | Human-readable output (default is JSON) |

#### `kazam ingest`

Ingest content from external platforms into kazam pages

##### `kazam ingest notion`

Import pages from a Notion workspace

| Flag | Default | Description |
|---|---|---|
| `--database` |  | Notion database ID — each row becomes a page |
| `--page` |  | Notion page ID — import a single page and its children |
| `--token` |  | Notion API token (default: .env NOTION_TOKEN or env var) |
| `--out` | `.` | Output directory for generated YAML files (default: current dir) |
| `--dry-run` |  | Preview what would be created without writing files |
| `--stats` |  | Show staleness stats without ingesting (metadata only, fast) |
| `--all` |  | Discover and ingest all pages the integration can access |

#### `kazam annotate`

Manage annotations on pages (sidecar files in .kazam/annotations/)

| Flag | Default | Description |
|---|---|---|
| `--dir, -d` | `.` | Site directory |

##### `kazam annotate add`

Add an annotation to a page

- `page` - Page path relative to site root (e.g. 'deals/acme.yaml')
- `text` - Annotation text

| Flag | Default | Description |
|---|---|---|
| `--section` |  | Section this annotation applies to (e.g. 'competitive', 'timeline') |
| `--author` | `anonymous` | Author name |
| `--source` | `cli` | Source: cli, agent, or web |

##### `kazam annotate list`

List annotations on a page

- `page` - Page path relative to site root

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Output as JSON |

##### `kazam annotate resolve`

Mark an annotation as incorporated

- `id` - Annotation ID (e.g. 'ann-20260507-e708')

##### `kazam annotate clear`

Remove all annotations for a page

- `page` - Page path relative to site root

#### `kazam sdk`

Emit a TypeScript SDK from the page schema (types, enums, interfaces)

##### `kazam sdk emit`

Print TypeScript type definitions to stdout

##### `kazam sdk emit-react`

Print React component renderer to stdout (TSX)

##### `kazam sdk emit-schema`

Print JSON component schema to stdout (for agent tooling)

##### `kazam sdk emit-agents`

Print markdown component reference to stdout (for agent context)

#### `kazam theme`

Output the kazam CSS theme for use in external apps

##### `kazam theme css`

Print the full CSS stylesheet to stdout

| Flag | Default | Description |
|---|---|---|
| `--theme` | `dark` | Theme name (dark, light, red, orange, yellow, green, blue, indigo, violet) |
| `--mode` | `dark` | Base mode for rainbow themes (dark, light). Ignored for dark/light themes |
| `--texture` | `none` | Enable texture overlay (none, dots, grid, grain, topography, diagonal) |
| `--glow` | `none` | Enable glow effect (none, accent, corner) |
| `--switchable` |  | Emit all theme/mode/texture/glow variants as [data-*] CSS selectors for runtime switching. When set, --theme/--mode/--texture/--glow are ignored |

##### `kazam theme vars`

Print only the :root CSS custom properties block to stdout

| Flag | Default | Description |
|---|---|---|
| `--theme` | `dark` | Theme name (dark, light, red, orange, yellow, green, blue, indigo, violet) |
| `--mode` | `dark` | Base mode for rainbow themes (dark, light). Ignored for dark/light themes |

##### `kazam theme json`

Print theme tokens as JSON (for programmatic consumption)

| Flag | Default | Description |
|---|---|---|
| `--theme` | `dark` | Theme name (dark, light, red, orange, yellow, green, blue, indigo, violet) |
| `--mode` | `dark` | Base mode for rainbow themes (dark, light). Ignored for dark/light themes |

#### `kazam agl`

Parse, validate, and compile Agent Graph Language (.agl) specs

##### `kazam agl validate`

Validate an .agl spec: parse it, resolve its imports, then run the static graph analyzer (reachability, terminal completeness, branch integrity, and invariant soundness)

- `path` - Path to the .agl spec file, or a bare name resolved against ~/.kazam/agl/specs/<name>.agl

| Flag | Default | Description |
|---|---|---|
| `--json` |  | Emit machine-readable JSON instead of the human-readable report |
| `--tools` |  | Optional flat JSON array of dotted `Server.method` tool names. When given, warns about any call()/map() function in the flow that isn't listed. This is a name-existence check only, not schema validation — the manifest is hand-maintained and has no notion of a server's actual tool/argument schema. Omit this flag for zero behavior change |

##### `kazam agl export`

Compile an .agl spec into a token-dense agent system-prompt block

- `path` - Path to the .agl spec file, or a bare name resolved against ~/.kazam/agl/specs/<name>.agl

| Flag | Default | Description |
|---|---|---|
| `--format` | `prompt` | Output format (currently only "prompt" is supported) |
| `--out, -o` |  | Write to this file instead of stdout |

##### `kazam agl flow`

Print a top-to-bottom ASCII rendering of a spec's flow — states, actions, and transitions, with branches fanned out underneath the state that owns them. A plan preview, not the graph's source syntax

- `path` - Path to the .agl spec file, or a bare name resolved against ~/.kazam/agl/specs/<name>.agl

##### `kazam agl skill`

Compile a validated .agl spec (imports resolved) into a portable skill document for an LLM coding tool

- `path` - Path to the .agl spec file, or a bare name resolved against ~/.kazam/agl/specs/<name>.agl

| Flag | Default | Description |
|---|---|---|
| `--target` |  | Which tool's skill format to render |
| `--out, -o` |  | Write to this file (or into this directory, as <name>.md) instead of stdout |

##### `kazam agl load`

Compile every spec in ~/.kazam/agl/specs/ into a Claude Code subagent + a thin dispatcher skill in the target project. Cursor/Codex aren't wired up here yet — use `kazam agl skill --target cursor|codex` one spec at a time until they are

| Flag | Default | Description |
|---|---|---|
| `--scope` | `user` | Install to the user's global ~/.claude, or the current repo's .claude. Ignored if --out is given explicitly |
| `--out, -o` |  | Explicit project directory to write .claude/skills/ (and, with --isolated, .claude/agents/) into. Overrides --scope |
| `--isolated` |  | Compile a tool-scoped subagent + a thin dispatcher skill instead of the inline default. Use this when a graph genuinely needs isolation - a harder tool boundary than the invoking session has, a background/parallel run - not for anything that gates on approval from whoever's already in the conversation: a subagent can't verify that a relayed "approved" really came from a human, only the inline default can, because it runs as this conversation instead of a separate one |

##### `kazam agl cache-migrate`

Bring an existing ~/.kazam/agl/cache/<name>.jsonl file's lines up to a cache block's current declared fields. Adds a type-appropriate default (empty string, 0, false, []) for any field a line is missing; never removes or otherwise touches fields already present

- `path` - Path to the .agl spec (or fragment) declaring the cache block, or a bare name resolved against ~/.kazam/agl/specs/<name>.agl

| Flag | Default | Description |
|---|---|---|
| `--name` |  | Which declared cache block to migrate, when the spec declares more than one. Required only in that case |

#### `kazam cli-reference`

Generate the CLI command reference from --help metadata

| Flag | Default | Description |
|---|---|---|
| `--write` |  | Write the generated reference into README.md between the markers |
| `--check` |  | Exit 1 if README.md's block doesn't match freshly generated output (for CI) |


<!-- CLI_REFERENCE:END -->

## Security

~10 direct Rust crates, `Cargo.lock` committed, `cargo-audit` in CI, protected main, signed release tags. Remote MCP transport requires bearer token auth when binding to non-localhost interfaces. Full scope: [`SECURITY.md`](SECURITY.md). Report vulnerabilities privately via the [GitHub advisory form](https://github.com/tdiderich/kazam/security/advisories/new).

## Contributing

PRs welcome — agent-assisted contributions explicitly encouraged. See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

MIT — see [`LICENSE`](LICENSE).

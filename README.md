# kazam

**The AI-native knowledge base.**

Structured YAML pages with freshness tracking, sidecar annotations, and an MCP server — so your agents can create, review, annotate, and refresh content the same way your team does.

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
```

A pack is an ordinary curata page (see the `ai-tool-pack` template) carrying a top-level `pack:` block:

```yaml
pack:
  targets: [claude, cursor]   # optional - omit to write all targets
```

Pages without the marker are refused (`--force` overrides), and pages with unfilled `{{template}}` variables never install. `kazam validate` enforces pack structure server-side too: a `pack:` page must have at least one non-empty markdown component. The markdown components compile, in order, into a managed block inside `CLAUDE.md` and `.cursorrules`:

- Content outside the block is never touched, so your existing rules survive.
- Reinstalling replaces the block in place; installs are idempotent.
- Multiple packs coexist, one block per pack slug.
- The block header records source URL and content hash, so drift against the source page is detectable.

Because packs are pages, they inherit everything the platform already does: versioning, annotations, freshness tracking, search, and MCP access.

## curata - the hosted platform

**kazam** is the OSS engine. A Rust CLI that builds structured YAML pages into themed HTML, with freshness tracking, sidecar annotations, and an MCP server. Free forever, MIT licensed.

**curata** ([github.com/tdiderich/curata](https://github.com/tdiderich/curata)) is the OSS app. A Next.js dashboard for browsing, annotating, and managing kazam pages. Deploy with Docker Compose, expose an API for agents, and serve an MCP server so any AI client can read and write your knowledge base directly. Also MIT licensed.

**curata.ai** ([curata.ai](https://curata.ai)) is the hosted cloud. Free to use — sign up, connect your agent via MCP, and start capturing AI outputs. No infrastructure to manage.

Connect via the built-in MCP server — add this to your editor's MCP config:

```json
{ "type": "url", "url": "https://curata.ai/api/mcp/stream" }
```

**The knowledge loop:** agents write structured pages → humans review and annotate in curata → agents read the annotations on the next cycle → the knowledge base compounds over time. Each annotation narrows what the agent needs to reconsider; each refresh closes the loop.

See [PRODUCT.md](PRODUCT.md) for the full product plan.

## Security

~10 direct Rust crates, `Cargo.lock` committed, `cargo-audit` in CI, protected main, signed release tags. Remote MCP transport requires bearer token auth when binding to non-localhost interfaces. Full scope: [`SECURITY.md`](SECURITY.md). Report vulnerabilities privately via the [GitHub advisory form](https://github.com/tdiderich/kazam/security/advisories/new).

## Contributing

PRs welcome — agent-assisted contributions explicitly encouraged. See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

MIT — see [`LICENSE`](LICENSE).

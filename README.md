# kazam

**The AI-native knowledge base.**

YAML pages with freshness tracking, prompt templates, and an MCP server — so your agents can create, review, and refresh content the same way your team does.

---

## Why

Docs go stale. Nobody knows which ones. Refreshing them is manual work that doesn't happen.

kazam treats content as structured data: YAML pages with explicit owners, review cadence, and sources of truth. Freshness metadata travels with the content. Build-time banners surface stale pages. Prompt templates give agents a standardized way to create, review, and refresh — so "keep docs current" becomes a workflow, not a wish.

## Capabilities

- **YAML content** — 30+ components, three shells (standard, document, deck), zero runtime JS, theme-aware via CSS vars
- **Freshness tracking** — owner, review cadence, sources of truth, stale banners at build time
- **Prompt templates** — standardized agent workflows: `migrate`, `add-page`, `refresh`, `audit`, `review`
- **MCP server** — agents read and write content directly without shell roundtrips
- **Agent workspace** — codebase anatomy index, task tracking, visual board (see below)
- **Wishes** — agent-generated content scaffolding (decks, briefs, pages)
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

Input tokens per turn dropped 81–94% across the board.

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

### Corrections

```bash
kazam ctx correction "assumed Express middleware" "it's custom Koa" --file src/auth.rs
kazam ctx corrections --json
```

Record when an agent gets something wrong. Corrections surface in workspace rules so future sessions don't repeat the mistake.

### Hooks

`kazam workspace init --agent claude` installs three Claude Code hooks: session start (surfaces anatomy drift and ready tasks), post-write (logs file modifications), session stop (rescans anatomy). They fire silently and only surface output when something is actionable.

## Security

~10 direct Rust crates, `Cargo.lock` committed, `cargo-audit` in CI, protected main, signed release tags. Full scope: [`SECURITY.md`](SECURITY.md). Report vulnerabilities privately via the [GitHub advisory form](https://github.com/tdiderich/kazam/security/advisories/new).

## Contributing

PRs welcome — agent-assisted contributions explicitly encouraged. See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

MIT — see [`LICENSE`](LICENSE).

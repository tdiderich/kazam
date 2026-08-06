# Changelog

All notable changes to kazam are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.17.0] - 2026-08-06

### Added
- **`~/.kazam/agl/templates/<name>.md`**: real markdown, not new grammar.
  `<!--spec-->` marks the boilerplate shape, `<!--samples-->` marks known
  good examples, no marker at all means the whole file is the shape. A
  state's existing `evaluate(...)` free text just names one directly (like
  `evaluate(activity_summary_draft vs activity-summary)`); `kazam agl
  skill`/`load` resolve every distinct word across a spec's `evaluate(...)`
  expressions against real files in that directory
  (`skill::referenced_template_names`), and embed each match's content,
  split on the `<!--samples-->` marker, into a `## Templates` section,
  same treatment as `## Preflight`/`## Cache`.

## [1.16.0] - 2026-08-06

### Added
- **Named `cache { field: type, ... }` blocks**: a spec can declare zero or
  more, inline for its own use or pulled in via `import` from a shared
  fragment (so every spec importing that fragment shares the same cache).
  A block's name is its file identity: `~/.kazam/agl/cache/<name>.jsonl`,
  deliberately outside the compiled skill entirely, so `kazam agl load`
  regenerating the skill can never lose cached data. Two blocks landing on
  the same name with different fields is a hard error at resolution time.
  The compiled skill gets a `## Cache` section per block: file path,
  schema, and the check-before-resolve / append-after-resolve convention.
- **`kazam agl cache-migrate <path> [--name <block>]`**: brings an existing
  cache file's lines up to a block's current declared fields. Adds a
  type-appropriate default for any field a line predates (empty string,
  `0`, `false`, `[]`); never touches fields already present or reorders
  lines. `--name` is required only when a spec declares more than one
  cache block.

## [1.15.0] - 2026-08-06

### Changed
- **`kazam agl load` is inline by default, not subagent-dispatch**: found
  live, running a converted workflow for real: a subagent has no way to
  verify a relayed "approved" came from an actual human rather than the
  orchestrating agent's own paraphrase, since it's isolated from the
  conversation the human is actually in. The compiled `.claude/skills/<name>.md`
  now embeds the whole graph (primer, preflight, flow, resolved source) and
  runs directly in whatever session invokes it, so a `gate(...)` checks
  approval against the real human already there. `--isolated` compiles the
  old tool-scoped subagent + thin dispatcher pair instead, for specs that
  genuinely want isolation (a harder tool boundary, background/parallel
  runs) - and refuses any spec with a write an invariant protects with a
  gate (new `validator::has_gate_protected_writes`), rather than silently
  compiling something whose approval check can't mean anything once
  isolated.

## [1.14.0] - 2026-08-06

### Added
- **`.agl` string-literal `call()`/`map()` args**: an argument can now be a
  quoted string literal (`call(Bash, customer, "https://...")`), not only a
  bare-ident variable reference. Comments are lexer trivia and never reach
  the AST, so config data like a real endpoint had nowhere else to live -
  literal args round-trip correctly through `render_agl_source`, unlike a
  comment.
- **`skill: <name>`**: optional spec-level field naming the compiled
  skill/subagent, defaulting to the kebab-cased graph name when absent.
- **Tool-scoped subagents**: `kazam agl skill` (and `load`, below) can now
  compile a `.claude/agents/<skill>.md` subagent whose `tools:` is exactly
  `requires:`, verbatim - the harness enforces the boundary the Preflight
  section describes, instead of relying on the model to self-police it.
  A matching thin `.claude/skills/<skill>.md` only dispatches to that
  subagent via the Agent tool, so the graph never runs inline wherever the
  skill got invoked with some other, broader toolset.
- **`kazam agl load [--out <dir>]`**: batch-compiles every spec under
  `~/.kazam/agl/specs/` into both files for a project. Claude Code only
  for now - Cursor/Codex need a shared-file merge scheme this doesn't
  build. A spec that fails to parse/resolve/validate is skipped with its
  error printed, not aborting the whole batch.

### Fixed
- **AGL lexer rejected hyphens in idents**: real MCP tool identifiers
  (`mcp__technical-success-hub__write_page`) are kebab-cased server names,
  but `is_ident_char` only allowed alphanumeric/`_`/`.`. Fixed without
  breaking a no-space `next->TERMINATE(...)` arrow, which would otherwise
  have its `-` absorbed by a naive fix.
- **Invariant-soundness check missed real write verbs**: `WRITE_SYNONYMS`
  only had `write/update/set/create/delete/remove/modify`. Linear's real
  tools are `save_issue`/`save_customer_need`, HubSpot's MCP tool is the
  generic `manage_crm_objects` - neither matched, so the core "write
  reachable without gate" guarantee silently never fired for either.
  Added `save/manage/sync/publish/insert/upsert/patch/put/post`.

## [1.13.0] - 2026-08-06

### Added
- **`.agl` shared invariant fragments (`import`)**: a spec can declare
  `import "path.agl"` lines before the `spec` keyword to pull in an
  `invariant { ... }` block from a fragment file, resolved relative to the
  importing file first, then against `~/.kazam/agl/shared/<path>`. Fragments
  can nest imports; a cycle is a hard parse-time error. This kills the copy
  paste drift where the same rule (like "any write to a CRM needs human
  approval") had to be hand duplicated into every spec that needed it.
- **`.agl` `requires:` declaration and tool-dependency checks**: a spec can
  declare `requires: Server.method, ...` after `out:`. `kazam agl validate`
  cross checks it against every `call()`/`map()` in the flow, warning on a
  flow action not covered (`undeclared-tool-dependency`) or a declared tool
  never called (`unused-tool-dependency`).
- **`kazam agl skill <path> --target <claude|cursor|codex>`**: compiles a
  validated, import resolved spec into a portable skill document, one of
  a static primer teaching an LLM how to read AGL cold, a preflight section
  generated from `requires:` (confirm every tool is available, abort before
  executing any state if one is missing, instead of failing mid-graph),
  a run-order note, an ASCII flow diagram, and the resolved spec in native
  `.agl` syntax. Claude gets `SKILL.md`-shaped YAML frontmatter; Cursor and
  Codex get the same body unwrapped or under a heading.
- **`kazam agl flow <path>`**: prints the same ASCII flow diagram on its
  own, a plain "what does this actually do" read of a spec's states,
  actions, and transitions, with branches fanned out under the state that
  owns them.
- **`kazam agl validate --tools <manifest.json>`**: opt-in, name-existence
  only check of every `call()`/`map()` function against a hand-maintained
  flat list of dotted tool names. Not schema validation, deliberately thin.
- **`~/.kazam/agl/{specs,shared}` hub convention**: a bare spec name with no
  `/` and no `.agl` extension resolves against `~/.kazam/agl/specs/<name>.agl`
  in `validate`, `export`, `flow`, and `skill`.

## [1.12.0] - 2026-08-06

### Added
- **`kazam agl` — Agent Graph Language parser, validator, and prompt compiler**:
  a new dense DSL (`.agl`) that turns a task into a static directed graph with
  mandatory invariants, so an LLM agent runs inside deterministic execution
  boundaries instead of free-form natural-language instructions. `kazam agl
  validate <file>` parses the spec and runs a static graph analyzer —
  unreachable states, dangling/undefined transitions, non-terminating cycles,
  branch integrity, and invariant soundness (e.g. a `write` action reachable
  without first passing its required `gate`) — with human-readable or `--json`
  output. `kazam agl export <file>` compiles a validated spec into a
  token-dense `<agent_spec>` system-prompt block ready for injection into an
  agent runtime.

## [1.11.0] - 2026-08-03

### Added
- **priority_queue Done section**: items with `status: completed` are separated
  into a collapsible "Done" group at the bottom of the queue, collapsed by
  default. Active items stay in their normal urgency/horizon/owner groups without
  completed items mixed in.

### Fixed
- **`kazam open` frontmatter handling**: markdown files with YAML frontmatter
  no longer render the frontmatter as body text. The frontmatter block is shown
  as muted inline metadata above the content, separated by a thin rule.

## [1.10.0] - 2026-07-28

### Added
- **`kazam open` can write to disk**: a Save button and Cmd+S (Ctrl+S) commit the
  edit buffer to the file, and `POST /api/save` does the same for agents. Saving
  is explicit on purpose, since autosaving every keystroke would churn the file
  and fire agent hooks repeatedly. Writes go through a temp file and a rename so
  a crash cannot leave the file truncated, and a save is refused while a conflict
  is unresolved rather than clobbering whatever changed on disk.

### Changed
- **The watcher ignores kazam's own writes**: after a save the new text is adopted
  as the disk state, so the resulting filesystem event no longer bumps the version
  and reloads the page for no reason.

## [1.9.0] - 2026-07-28

### Added
- **`kazam open <path>`**: opens a single `.md`, `.yaml`, `.yml`, or `.json` file
  in the browser with live reload. Rendered markdown or line-numbered syntax
  highlighting, a View/Edit/Copy toolbar, and selection auto-copy. Edits are held
  in memory and exposed over `GET /api/content`, so an agent can read what you
  typed without the file being saved first. `POST /api/content` writes the buffer,
  `GET /api/rendered` returns the rendered HTML, and `GET /api/status` reports
  `dirty`, `conflict`, `valid`, and `error`. Unsupported extensions are rejected
  with the list of supported ones; invalid YAML or JSON is reported, not blocked,
  since text is transiently invalid while you type. Defaults to port 3002.
- **`kazam show <path>`**: pretty-prints the same three formats in the terminal
  with ANSI colors. Headings and lists for markdown, line numbers and per-token
  coloring for YAML and JSON. JSON is reformatted when it parses.

### Changed
- **Port selection**: `kazam board` and `kazam open` try the requested port, then
  fall back to an OS-assigned free port. Previously `board` walked the next ten
  ports in sequence, which could land on a port another tool had reserved.
- **Shutdown**: Ctrl+C on `kazam board` and `kazam open` now exits cleanly and
  releases the port instead of leaving the socket bound.

### Fixed
- **JSON syntax highlighting dropped delimiters**: `take_while` consumed the
  character that ended a `true`, `false`, or `null` token, so every trailing
  comma after those values was swallowed. The keyword arms are now peek-driven.
- **Board server boilerplate**: port binding, browser launch, response helpers,
  and the shutdown handler moved to a shared `server` module instead of living
  in `board.rs`.
- **`kazam show` dimmed YAML list scalars**: list items were routed through the
  map-value colorizer, which expects a `": "` prefix, so they fell through to the
  catch-all branch. `- 42` is now yellow like `count: 42` already was.
- **`docs/workspace.yaml` named the wrong board port**: it said `localhost:3000`
  while the default is 3001.

## [1.8.0] - 2026-07-22

### Added
- **User-level install scope**: `kazam install` now defaults to a user-level
  install (`~/.claude`), so a personal guardrail pack applies across every
  project. `--repo` pins the install to the current project instead, and an
  explicit `--dir` still implies repo scope. When neither flag is given and the
  session is interactive, install prompts for the scope. User scope writes the
  `claude` target (`~/.claude/CLAUDE.md`) and its hooks; other rules targets
  have no shared user home, so they warn and are skipped under `--user`.
- **Pack hook `mode: word`**: `block_on_match` gains word-boundary matching
  alongside the default `substring`. A `word` pattern only matches when the
  characters on either side are non-word, so `foster` blocks the standalone
  word but not `fostering`. (regex mode is still rejected at validate time.)
- **Pack hook `field:`**: `block_on_match` can scope its scan to a single
  `tool_input` field instead of the whole serialized input. Makes MCP-tool
  guards precise, like `field: text` to scan only a Slack message body.

### Changed
- **Hook config moved out of `.kazam/` to `.claude/kazam-packs/`** (repo scope)
  or `~/.claude/kazam-packs/` (user scope), and the runner command registered in
  `settings.json` now carries an absolute `--config` path. Hooks resolve their
  config no matter what directory the harness runs the command from. Fixes a bug
  where a hook installed at a repo root failed when the session ran from a
  subdirectory. Pre-existing installs keep working via an upward-walk fallback
  and re-point on the next install.

### Docs
- Clarified that a hook's `on.tool` is a verbatim harness matcher: MCP tool
  names and prefixes (`mcp__...`) work, not only `Write` or `Edit`.

### Docs
- Clarified that a hook's `on.tool` is a verbatim harness matcher: MCP tool
  names and prefixes (`mcp__...`) work, not only `Write` or `Edit`.

## [1.7.0] - 2026-07-22

The packs buildout. `kazam install` grows from a rules-only writer into a full
config installer with drift detection, cross-tool output, and safe declarative
hooks, plus anonymous install of public packs.

### Added
- **`kazam check`**: drift detection. Scans installed packs across all target
  files, re-fetches each source, and reports which have drifted from their
  curata page. The compared hash is computed locally (sha256 of the fetched
  YAML), so a compromised instance cannot report a false "fresh".
- **Cross-tool targets**: `agents` (AGENTS.md, the 30+-tool standard) plus
  `windsurf`, `copilot`, `gemini`, `aider`, on top of `claude`/`cursor`. New
  `--cli` flag overrides a pack's declared targets at install time.
- **Declarative hooks**: packs can carry deny/inject/review guardrail config
  (never executable code). `install --allow-hooks` writes the config and
  registers the trusted `kazam pack-hook` runner in `.claude/settings.json`.
  The runner has no network or arbitrary-write capability, so a hostile pack
  can at worst block the user's own tool calls or inject visible text.
- **Public anonymous install**: `kazam install <instance>/p/<org>/<slug>`
  fetches a public pack over the unauthenticated raw route, no key needed.
- Prompt-injection heuristic warns when compiled pack rules contain suspicious
  phrases before they land in CLAUDE.md.

### Security
- The URL-derived pack slug is now validated (`^[A-Za-z0-9_-]+$`) before it
  flows into file paths or the settings.json hook command, closing a command
  injection. Plaintext-http pack URLs to non-local hosts are refused so the API
  key never travels in cleartext.

## [1.6.0] - 2026-07-22

The packs release. kazam gains `kazam install` and the `pack:` page marker, so a
curata page can be compiled straight into a repo's AI tool config files.

### Added
- **AI tool packs**: `kazam install <instance-url>/pages/<slug>` fetches a pack
  page from a curata instance and compiles its markdown components into managed
  blocks in `CLAUDE.md` and `.cursorrules`. Fetches via the curata REST shim
  (`POST /api/mcp`) with a streamable-HTTP fallback (`/api/mcp/stream`); API key
  via `--api-key` or `KAZAM_CURATA_API_KEY`.
- **`pack:` page marker**: a new top-level Page field (`pack.targets: [claude, cursor]`)
  marks a page as an installable pack. `kazam validate` now checks pack pages
  have at least one non-empty markdown component and only known target values.
- Install safety: pages without a `pack:` marker are refused (`--force` overrides),
  and pages with unfilled `{{variables}}` never install. Managed blocks are
  idempotent, carry a source + content-hash header, and coexist one-per-pack.

## [1.5.0] — 2026-05-07

The annotation release. kazam gains a sidecar annotation system, annotation-aware
agent refresh, hardened HTTP MCP transport, Notion ingestion, a full audit command,
and wishes-as-recipes. This is the bridge release for curata.

### Added
- **Sidecar annotations** — human context stored as individual YAML files in
  `.kazam/annotations/<page-slug>/`. CLI: `kazam annotate <page> "text"`.
  MCP tools: `annotate_page`, `list_annotations`, `update_annotation`.
  14-day decay tracking with status lifecycle (pending → incorporated/ignored/stale).
  Build renders annotations inline with age indicators and status badges.
- **Annotation-aware refresh** — the deal-360 wish prompt reads annotations as
  highest-priority source. Conflict resolution: annotations override CRM/call data.
  Agent updates annotation status after each refresh cycle.
- **HTTP MCP hardening** — `--local` (127.0.0.1, default) / `--remote` (0.0.0.0)
  bind modes. `--remote` requires bearer token via `--token` or `KAZAM_MCP_TOKEN`
  env var. CORS includes Authorization header for remote agent access.
- **`kazam audit`** — site health audit covering freshness compliance, component
  validation, and annotation health. JSON output by default, `--pretty` for
  human-readable.
- **Notion ingest** — `kazam ingest notion` imports databases, pages, and child
  pages. `--all` discovers everything the integration can access. `--stats` for
  metadata-only staleness check. `--dry-run` preview.
- **Wishes-as-recipes** — wishes are now portable agent recipes in `wishes/`.
  Each has `wish.yaml`, `prompt.md`, optional `page.yaml` template and `script.py`.
  New wishes: `deal-360`, `debrief`, `audit-fix`, `freshness-notifier`, `hubspot-icp`,
  `linear-team-map`, `sources-map`, `notion-ingest`.
- **Freshness drift** — `kazam freshness drift` checks if source-of-truth files
  have changed since pages were last updated. `--repo PREFIX=LOCAL` for multi-repo.
- **Freshness notify** — `kazam freshness notify` generates a digest of stale
  pages grouped by owner for Slack/email distribution.
- **`role_map` component** — renders `roles:` from kazam.yaml as clickable
  jump-point cards. Roles gain an optional `href` field for navigation.
- **Health dashboard** — `kazam build` now generates `_health.html` with
  freshness stats (StatGrid, ProgressBar), overdue/due-soon tables, and
  ownership summary. Opt out with `--no-health`.
- **Template variables in prompts** — `kazam prompt show` expands `{{config}}`
  and other variables before output.

### Changed
- **MCP server** — 5 → 8 tools. Added `annotate_page`, `list_annotations`,
  `update_annotation` alongside existing `read_page`, `list_pages`, `search`,
  `get_config`, `write_page`.
- **Wishes architecture** — old monolithic wish modules (`wish/brief.rs`,
  `wish/deck.rs`, `wish/dashboard.rs`) replaced by portable recipe directory
  format in `wishes/`. `kazam wish list` discovers local + registry wishes.
  `kazam wish init <name>` scaffolds from registry.
- **Search scoring overhaul** — word-boundary detection, tiered field bonuses
  (title +10, search_terms +8, headings +5, description +3), match context
  snippets shown in results instead of default description.
- **Freshness-aware search ranking** — overdue pages penalized (-3), expired
  pages penalized (-5). New `freshness_status` field in search.json.
- **`kazam init`** — now creates `.kazam/annotations/` alongside track, ctx,
  and hooks directories.

### Fixed
- **Tree collapse** — toggle JS now loads for all trees with children, not
  only those with filter toggles. Fixes non-functional chevrons on plain
  trees like org charts.

## [1.3.1] — 2026-05-01

### Changed
- **Anatomy format: YAML → TSV** — agent-facing anatomy files
  (`anatomy.tsv`, `anatomy/<dir>.tsv`) now use tab-separated values
  instead of YAML. ~60–80% fewer tokens per read. The internal flat
  store (`anatomy.flat.yaml`) remains YAML for the board and tooling.
  Descriptions are sanitized (tabs replaced with double spaces).
  Old YAML anatomy files are cleaned up automatically on scan.
- **Dropped `last_scanned` from anatomy output** — agents never used it;
  removing it cuts per-entry size further.
- **Removed dead layered anatomy structs** — `AnatomySummary`,
  `DirEntry`, `DirAnatomy` types deleted since TSV replaced YAML
  serialization for the agent-facing layer.
- **Updated benchmarks** — re-ran with Sonnet 4.6, identical prompts,
  git worktrees. Results: 44–46% cheaper, 41–59% faster, 81–94% fewer
  input tokens per turn across 4 real codebases.

### Added
- **Benchmark harness** — `benchmarks/run.sh` and `benchmarks/run-all.sh`
  automate A/B comparisons (kazam vs vanilla) using `claude -p` with
  JSON output. Test definitions in `benchmarks/tests/`.

### Fixed
- **Hook format matching** — `retain` logic now matches both nested and
  legacy flat hook formats (from 1.3.0 fix on this branch).

### Docs
- **Nav restructured** — Home → Get Started → Workspace → Sites (Site
  Guide, Themes, Deploy) → Recipes → Components. Workspace promoted to
  top-level nav item.
- **Landing page rewritten** — workspace-first hero, three pillars
  (tokens, tracking, visibility), updated benchmark stats, condensed
  site-gen section, dual quickstart callouts.
- **Get Started rewritten** — dual-path tabs (workspace vs sites) with
  next-up callouts to deep-dive pages.
- **Site Guide** — `about.yaml` retitled from "Full Tour", workspace
  callout removed (workspace has its own page).
- **Workspace docs polished** — added corrections, consolidation, and
  rules-override sections. Anatomy examples updated to TSV format.
- **README updated** — tighter site-gen section, new workspace features
  (corrections, consolidation, rules-override), updated benchmarks.

## [1.3.0] — 2026-04-30

kazam is no longer just a static site generator. This release adds a
full agent workspace — codebase indexing, task tracking, a visual board,
and invisible hooks that wire it all into Claude Code. The positioning
shifts: kazam is the tool your coding agent didn't know it needed.

### Added
- **`kazam workspace init`** — one command to set up an agent workspace
  in any repo. Scans the codebase, writes a two-tier anatomy index,
  installs agent hooks, and writes workspace rules. `--agent claude`
  registers Claude Code hooks in `.claude/settings.json`.
  `--skunkworks` auto-creates tasks from TODOs and known patterns.
- **Two-tier anatomy** — `kazam ctx scan` produces a compact summary
  (`anatomy.yaml` — root files + top-level directory rollups) and
  per-directory detail files (`anatomy/<dir>.yaml`). Even 5,800-file
  repos compress to a ~68-line summary even with thousands of files. Agents read the summary first,
  drill into the directory they need — no `find`, no `grep`, no wasted
  turns. Path-aware descriptions infer file roles from directory
  conventions (routes/, models/, lib/, etc.).
- **Task tracking** — `kazam track add|claim|close|block|ready|list`.
  Tasks live in `.kazam/track/tasks.yaml`, survive session restarts and
  context compaction. `ready --json` returns unblocked tasks sorted by
  priority — the entry point for any session start or context recovery.
- **`kazam board`** — themed, auto-refreshing local dashboard showing
  task status, codebase anatomy, and activity log. Built with kazam's
  own rendering engine. Auto-refreshes on any `.kazam/*.yaml` change.
- **Agent hooks** — three Claude Code hooks installed by
  `workspace init`: session-start (surfaces drift + ready tasks),
  post-write (logs file modifications), session-stop (rescans anatomy).
  Silent when nothing is actionable.
- **Workspace rules** — `.claude/rules/kazam-workspace.md` teaches the
  agent to use anatomy-first navigation, structured task tracking, and
  commit-triggered task closing. Suppresses built-in TaskCreate/TaskUpdate
  in favor of kazam's tracking.
- **Settings merge** — `workspace init` appends kazam hook entries to
  existing `.claude/settings.json` arrays instead of replacing them.
  Deduplicates by description prefix on re-init.
- **Context enrichment** — `kazam ctx describe`, `kazam ctx learn`,
  `kazam ctx bug` for agents to record what they discover during work.

### Changed
- README rewritten — workspace-first positioning, benchmark results,
  dual quickstart (workspace + static sites).
- `Cargo.toml` description and keywords reflect the dual identity.

## [1.2.2] — 2026-04-28

Three new components plus a small set of polish fixes that surfaced
during a real-data review against a live customer page.

### Added
- **`event_timeline`** — vertical event history with optional Major/All
  filter toggle. Per-event date, severity (`major | minor | info`),
  optional source chip, and external link. When a `summary` is provided
  the event collapses behind a native `<details>` toggle (no JS for
  expand/collapse). Filter toggle is a tiny class-swap script.
- **`tree`** — recursive nested status tree. Each node has a label,
  optional inline note, and per-node status (`default | completed |
  active | blocked | upcoming`). Status drives glyph + color. Optional
  filter toggle with three modes:
  - `all` — everything visible
  - `incomplete` — hides completed nodes (a completed branch correctly
    hides its descendants)
  - `blocked` — shows only blocked nodes plus their ancestor chain;
    server walks the tree and marks ancestors with
    `data-has-blocked-descendant` so the path-to-root keeps context.
- **`venn`** — two- or three-set venn diagram, native inline SVG. Per-set
  color flows through the `SemColor` enum; optional `overlaps[].sets`
  (length 2 or 3) place intersection labels. For pairwise overlaps in a
  3-set venn the label is pushed away from the un-included set's center
  so it lands in the actual lune, not piled up at the centroid.

### Fixed
- **Callout body now inherits markdown styling.** Bullets inside a
  `callout` were rendering flush left, mashed against the colored
  border. The body wrapper is now dual-classed `c-callout-body
  c-markdown` so list padding, code styles, and paragraph margins
  propagate from the existing `.c-markdown` rules.
- **`divider` had `margin: 0`.** Sat flush against neighboring section
  headers with zero breathing room. Bumped to `margin: 32px 0` for both
  labeled and unlabeled variants.

## [1.2.1] — 2026-04-27

A patch release that exists almost entirely so the v1.2 launch carousel
could be built with kazam itself. The punchline writes itself.

### Added
- **`print_flow: square` for `shell: deck` pages** — one slide per
  8.5in × 8.5in page, content vertically centered, no letterbox. Built
  for LinkedIn document carousels and other near-square viewports where
  the existing 4:3 landscape mode shrinks each slide into wasted space.
  Set it in the deck's frontmatter, print to PDF, drag the file into a
  LinkedIn "Add a document" post — no other tweaks required.

### Fixed
- **Deck slides no longer top-anchor when printed.** The deck shell's
  fit-to-screen JS sets `transform: scale(k)` with `transform-origin:
  top center` to keep oversized content from overflowing on screen.
  That transform persisted into print mode, leaving content stuck in
  the upper third of every printed page. Print CSS now resets the
  transform with `!important` so flex centering inside `.deck-inner`
  actually works against the print page. Affects all print modes
  (`slides`, `continuous`, and the new `square`).

## [1.2.0] — 2026-04-25

Second wish drop in the 8-week series — `kazam wish brief` — plus a
shared MCP-aware yolo posture across every wish, and an href-resolution
fix that aligns the renderer with HTML/Markdown semantics.

### Added
- `kazam wish brief` — generates a short, print-optimized `shell: document`
  artifact for a meeting, incident, vendor sync, 1:1, or exec readout.
  Same three-mode shape as `wish deck`: guided (scaffold `wish-brief/`
  + `questions.md` + `reference/`, drop context, rerun to grant),
  `--yolo [topic]` (skip the workspace; agent grounds the brief in MCP
  data and writes the YAML), portable (`--stdout` / `--dry-run`). The
  brief shape is meta block → one-line goal → context → agenda or
  timeline → talking points → optional risks → action items.
- MCP guidance shared across every wish's `--yolo` prompt. When the
  topic is the user's own world (a real meeting, a recent incident, a
  deal, a teammate), agents with MCP access (Google Calendar, Gmail,
  Slack, Linear, Granola, HubSpot, Attention, etc.) are invited to
  gather real context. Public/external topics ("the history of TLS",
  "a deck about coffee") never trigger MCP. Wired into both `wish deck`
  and `wish brief`.
- **MCP-first rule for `wish brief --yolo`** — for any topic that names a
  person, company, meeting, deal, ticket, channel, or incident, the
  agent's first actions are MCP lookups (HubSpot → Calendar → Granola →
  Linear → Slack → Attention). Every concrete claim in the brief —
  attendee names, dates, deal amounts, prior-call counts — must trace
  to a tool result. When a tool returns nothing, the brief writes
  `TBD — confirm before sending` instead of fabricating. Briefs are
  artifacts the user walks into real meetings carrying; invented
  specifics are a hard failure, not a creative liberty.
- `docs/examples/brief.yaml` — worked partner-renewal-sync brief, used
  as the in-workspace `reference/example-brief.yaml` and as a use-case
  example linked from the docs site.

### Changed
- **Href resolution** now follows standard HTML/Markdown semantics. Bare
  names (`content.html`, `assets/og.svg`) are page-relative and pass
  through to the browser; leading-`/` paths (`/index.html`,
  `/components/grids.html`) are site-root and the renderer prepends the
  depth base for subpath-deployment portability. Previously bare names
  were silently rewritten as site-root, which broke sibling links from
  any nested directory (e.g. the components index card's "Open →"
  buttons routed to `/kazam/content.html` instead of
  `/kazam/components/content.html`). The link analyzer already used the
  HTML/Markdown convention; the renderer now matches it.
- `docs/wishes.yaml` — `kazam wish brief` flipped from `planned` to
  `shipped` and now links to its rendered example.
- `docs/index.yaml` — the meeting-agendas use-case card surfaces both
  the agenda and brief examples.

### Fixed
- Docs `Content components` page no longer advertises a `kbd` section in
  its subtitle — `kbd` lives on the Indicators page. The component
  count badge on the index card is now `7`.
- Docs `kazam.yaml` nav, favicon, and og_image switched to `/`-prefixed
  site-root paths so they remain portable from any page depth.

## [1.0.1] — 2026-04-22

Patch release — three bug fixes reported post-launch.

### Added
- Table cells linkify `[text](url)` syntax. Scheme-allowlisted
  (`http://`, `https://`, `mailto:`, and relative paths — `/`, `#`,
  `./`, `../`); anything else (`javascript:` etc.) passes through as
  literal escaped text. Intentionally narrow — cells grow links only,
  not bold/italic/code.

### Fixed
- `kazam build --release` no longer injects the `/__kazam_version__`
  hot-reload poller. Static hosts (S3/CloudFront, Firebase, Tailscale
  Serve, `python3 -m http.server`, etc.) no longer see a 404 flood on
  every open tab. `kazam dev` still injects the poller as before.
- `shell: standard` PDF exports now print edge-to-edge on dark themes.
  The white outer frame Chromium painted into the page margin is gone:
  a new `@page standard-page { margin: 0 }` lets the theme background
  reach the sheet, with `.main-content { padding: 0.5in }` inside
  `@media print` restoring reader margins inside the page. `shell: deck`
  and `shell: document` print paths unchanged.

## [1.0.0] — 2026-04-21

The launch release. Earlier `0.x` versions were pre-launch iteration;
`1.0.0` is the first line we commit to. Everything shipped in the `0.x`
series is carried forward; the notes below cover only the delta since
`0.4.0`.

### Added
- Anchor `id:` on `section` and `header` components — auto-slugs from
  `heading` / `title` by default (lowercase, hyphens, punctuation +
  emoji stripped) so `/guide.html#outcomes` links work out of the box.
  Explicit `id:` overrides the slug for stable anchors that survive
  copy edits. Collisions within a page dedupe with `-2`, `-3`, etc.
  Scroll-offset CSS clears the sticky site bar so deep-links don't
  land with the heading hidden behind it.
- Build-time link report — every `kazam build` now walks the page graph
  and surfaces **orphan pages** (built but unreachable from `index.html`
  or the `nav:`) and **broken internal links** (`.html` hrefs that don't
  match any built page). Silent on clean builds. When anything surfaces,
  the build prints a grouped summary and writes `_site/links.md` so an
  agent can consume the list directly. `kazam dev` and
  `kazam build --allow-orphans` silence the orphan check (useful for
  draft pages); broken links always surface. `unlisted: true` on a page
  excludes it from the orphan check.
- `freshness:` page metadata — declare last-updated date, review cadence,
  owner, and sources-of-truth pointers per page. kazam computes status
  at build time (zero runtime JS) and injects a banner at the top of
  stale pages: **yellow** when the review comes due within 7 days,
  **red** when it's already overdue. Every build also prints a grouped
  summary of every stale page (silent when everything is fresh), sorted
  most-urgent-first. Use `KAZAM_TODAY=YYYY-MM-DD` for deterministic
  builds. Full docs at `/freshness`. Example:
  ```yaml
  freshness:
    updated: 2026-01-15
    review_every: 90d
    owner: owner@example.com
    sources_of_truth:
      - https://notion.so/abc123
      - label: "#ts-hub"
        href: https://company.slack.com/archives/C012345
  ```
- `logo:` field on `kazam.yaml` site config — replaces the text `name:`
  treatment in the site bar with an `<img>`. Accepts both shorthand
  (`logo: assets/logo.svg`) and expanded object form
  (`logo: { src, height, alt }`). Rendered height is capped at the
  site-bar content height so a tall logo never pushes the bar taller;
  width flows from aspect ratio and caps at 240px so a wide wordmark
  doesn't crush the nav. `src` routes through the depth-aware path
  rewriter, so absolute `/…` paths pass through verbatim and relative
  paths resolve from any subfolder page. Absent → falls back cleanly
  to the text-name treatment (no layout regression).
- `AGENTS.md` bug-filing + feature-request protocols. When an agent
  reproduces a bug or has a kazam-shaped feature idea, the guide now
  tells it to check `gh auth`, dedup against existing issues/PRs
  (including closed ones — a closed bug may mean the fix shipped in a
  newer version), then file with a consistent template. Feature
  requests also include a scope-check step ("does this fit kazam?")
  before filing, so wontfix noise stays down.

### Fixed
- Every component that emits an `href` now routes through the canonical
  `resolve_href` helper, honoring the verbatim-prefix rule documented
  in `AGENTS.md` (`/`, `http://`, `https://`, `#`, `mailto:`, `tel:`
  pass through untouched). Previously only the site nav followed this;
  `button_group`, `card_grid` (card href + links), `breadcrumb`,
  `empty_state`, `callout` links, and markdown link destinations all
  stripped leading `/` and emitted relative paths that 404'd from
  pages at depth ≥ 1.
- `kazam dev` now walks forward to the next free port when the
  requested one is in use (matches Vite / Next.js / Parcel UX) instead
  of failing to bind. Prints a one-line warning when it falls back:
  `⚠ port 3000 is in use — serving on 3001 instead`.
- `kazam dev` no longer rebuilds itself in an infinite loop when `out`
  is relative. The watcher canonicalizes `out` up front and also
  ignores any nested `_site` in the watched tree.
- `kazam build` skips nested `_site` directories. Running from a
  parent dir that contains previously-built sub-sites no longer
  recursively ingests those outputs as source.
- `kazam wish` auto-creates a minimal `kazam.yaml` in the current
  directory if one is missing, so the flow works in any fresh empty
  directory without forcing the user to hand-write site config first.

## [0.4.0] — 2026-04-20

### Added
- `kazam wish <name>` — scaffolds a `wish-<name>/` workspace with structured
  prompts (`questions.md`), usage hints (`README.md`), and a version-matched
  schema + worked example (`reference/`). Fill in what you know, drop real
  context (docs, notes, transcripts, PDFs) into the workspace, then run the
  same command again to grant: kazam shells out to the first agent it finds
  on `$PATH` (Claude, Gemini, Codex, OpenCode) with the workspace as CWD.
  The agent reads everything with its own file tools and writes a populated
  YAML. kazam itself does no file parsing. First wish: `kazam wish deck` —
  a ~7-slide deck for any topic (QBR, launch review, pitch, retrospective,
  etc.). Flags: `--agent` (force a specific CLI), `--yolo [topic]` (skip
  the workspace, let the agent invent everything), `--dry-run` (print the
  grant prompt), `--stdout` (portable wish markdown spec), `--out`
  (override output path).
- `/wishes` docs page with the scaffold→grant flow, agent-applications
  panel, and 8-week roadmap.
- Deck shell typography + layout pass — non-cover slides vertically
  center their content, inner width widened 900 → 1100px, every content
  primitive steps one type tier up on `shell: deck` so slides read as
  slides, not doc pages. Mobile scales down proportionally.
- Mobile responsiveness pass across the whole theme: stat grids, callout
  columns, before/after, tab buttons, tables, code blocks, and the deck
  shell all adapt to phone (≤640px) and tablet (≤768px) viewports.
- Social/SEO meta: `<meta name="description">`, full Open Graph and
  Twitter-card tags, and `<link rel="canonical">` on every page.
- Automatic `sitemap.xml` and `robots.txt` generation when a site's
  `url:` is configured.
- New `description:`, `url:`, and `og_image:` fields on site config.
- Site-wide Open Graph image (`docs/assets/og.svg`).
- `API reference` example page (`docs/examples/api.html`), demonstrating
  a Scalar-style endpoint doc composed entirely from existing components.
- Dedicated `About` and `How it works` pages; landing slimmed to
  manifesto + 30-second demo + three link cards.

### Fixed
- `before_after` component now renders inline markdown (`**bold**`,
  `` `code` ``) in its `before`/`after` fields instead of escaping them
  as literal characters.
- Build walker skips hidden entries (`.git`, `.DS_Store`) at any depth.
- Deck PDF export: cover slides now vertically center on landscape pages
  instead of hugging the top. New `print_flow: continuous` page option
  flows slides as one portrait document with thin separators between them,
  for sharing as a readable artifact rather than a presentation.
- Chart component renders inline SVG for pie, bar (vertical / horizontal /
  stacked), and timeseries (single + multi-series) — themed, zero runtime
  JS, stackable inside decks/grids/callouts.

## [0.3.0] — 2026-04-18

Renamed from `finro` to `kazam`. No functional changes. Existing
`cargo install --git` users pick up the rename via GitHub's repository
redirect; binary name is now `kazam` (was `finro`).

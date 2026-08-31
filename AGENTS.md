# AGENTS.md - developing kazam

For the YAML authoring guide (what `kazam agents` prints), see `AGENTS.md.template`.

## Commands

```bash
cargo test --release --all-targets   # CI gate (always --release)
cargo fmt --all --check              # CI gate
cargo clippy --release --all-targets -- -D warnings  # CI gate; warnings = error
cargo build --release                # full build
./target/release/kazam dev docs --port 3002  # live-edit the docs site
```

CI runs these four jobs in parallel: test, fmt, clippy, cargo-audit.

## Architecture

Single Rust binary. No workspace, no features, no build script. ~7 direct crates.

```
src/
  main.rs              CLI entry (clap derive): build, dev, init, agents, wish, track, ctx, board, workspace
  types.rs             All YAML-facing serde structs (SiteConfig, Page, Component, …)
  theme.rs             Theme tokens + STATIC_CSS (giant const string)
  render/
    mod.rs             Page render orchestration
    components.rs      Per-component render fns + dispatch match at top
    shells.rs          Shell chrome (standard, document, deck)
    scripts.rs         Bundled inline JS (deck nav, tabs, accordion, auto-fit)
    charts.rs          SVG chart rendering (pie, bar, timeseries)
    slug.rs            Anchor slug generation (used by section + header)
  build.rs             Batch build (yaml → html, copy assets)
  dev.rs               Watch + serve (notify + tiny_http)
  init.rs              kazam init scaffolding (KAZAM_YAML / INDEX_YAML / AGENTS_MD consts)
  agents.rs            kazam agents subcommand
  freshness.rs         Staleness computation (freshness: → banner + stale.md)
  links.rs             Link graph (orphan pages + broken internal hrefs)
  llms.rs              llms.txt emission
  minify.rs            Release-mode HTML/CSS/JS minification
  icons.rs             Bundled lucide icon SVGs
  wish/
    mod.rs             kazam wish dispatch (list, scaffold, grant, yolo)
    deck.rs            Deck wish (workspace + prompt generation)
    brief.rs           Brief wish
  workspace.rs         .kazam/ directory init, config read/write, atomic YAML persistence
  id.rs                Hash-based ID gen (kz-XXXX, no rand dep)
  board.rs             kazam board - live dashboard rendering .kazam/ state
  server.rs            Shared HTTP helpers: port binding, browser launch, responses, shutdown
  open.rs              kazam open, single-file browser viewer/editor for md/yaml/json
  show.rs              kazam show, single-file terminal pretty-printer for md/yaml/json
  track/
    mod.rs             kazam track CLI dispatch (11 subcommands)
    types.rs           Task, TaskStatus, TaskType, LogEntry, LogSeverity
    store.rs           YAML read/write for tasks + log
    graph.rs           Ready algorithm, dependency validation, cycle detection
  ctx/
    mod.rs             kazam ctx CLI dispatch (10 subcommands)
    types.rs           FileEntry, AnatomyStore, Learning, BugEntry
    scan.rs            Project file walker, token estimation, anatomy diff
    hooks.rs           Agent hook install/uninstall, shell script templates
docs/                  The hosted docs site (itself a kazam site)
examples/kb/           Example site used by integration tests
tests/integration.rs   E2e: invokes the binary, builds sites, asserts on HTML output
AGENTS.md.template     Authoring guide bundled via include_str! → kazam agents / kazam init
```

## Adding a new component

1. Add the struct + `Component` enum variant in `src/types.rs` (with `#[serde(default)]` where sensible)
2. Add the render fn in `src/render/components.rs` + wire it into the `Component` dispatch `match` at the top of the file
3. Add styles in `src/theme.rs` `STATIC_CSS` using theme tokens - never hardcoded rgba
4. Add an example in `docs/components/*.yaml` (pick the right category page)
5. Update `AGENTS.md.template` so LLM authors know about it
6. Optionally add a test in `tests/integration.rs`

## Tripwires

- **Hardcoded colors** - `rgba(60, 206, 206, X)` is teal and must be `rgba(var(--accent-rgb), X)`. Same for `rgba(9, 13, 24, X)` → `rgba(var(--bg-rgb), X)` and `rgba(255, 255, 255, X)` → `rgba(var(--text-rgb), X)`. If you find one, fix it.
- **Cargo.lock is committed** - don't gitignore it. Supply-chain posture per SECURITY.md.
- **`kazam init` raw strings** - templates contain `#` in hex colors (e.g. `"#3CCECE"`). Must use `r##"..."##` delimiters because `"#` would terminate a single-hash raw string.
- **Version** - keep `Cargo.toml` version authoritative. `src/main.rs` clap uses `version` (no literal) so `kazam --version` auto-matches.
- **Release builds** strip the dev hot-reload poller (`__kazam_version__`). Test `build_release_minifies` guards this.
- **Build skips `_site/` dirs** - nested previously-built output must not be re-ingested as source. Test `build_skips_nested_site_directories` guards this.
- **404 page** - `build.rs` skips `404.yaml` in the normal walk and renders it separately with a special base (`/` or the site URL) so all links are absolute. The default 404 page (no `404.yaml`) uses an EmptyState component. `dev.rs` serves `404.html` for missing pages instead of plain text.

## Code style

- Small well-named functions over macros or generics.
- Components are self-contained: one render fn in `components.rs`, matching styles in `theme.rs`, type in `types.rs`.
- No new runtime dependencies without strong reason - the value proposition includes shipping as one tiny binary.
- Inline scripts for interactivity go in `src/render/scripts.rs`. No JS build system.
- Commit messages: imperative mood ("add X", not "added X").

## Testing

All tests are e2e in `tests/integration.rs` - they invoke the compiled binary, build sites from scratch YAML, and assert on the rendered HTML. There are no unit tests in `src/`. Use `CARGO_BIN_EXE_kazam` env var (set by cargo test runner) to locate the binary. `KAZAM_TODAY=YYYY-MM-DD` env var simulates "today" for freshness tests.

## Agent workspace (`track`, `ctx`, `board`)

The `workspace.rs`, `track/`, `ctx/`, and `board.rs` modules implement agent
task tracking and context intelligence. State lives in `.kazam/` as YAML.

Key patterns:

- **CLI dispatch**: `track/mod.rs` and `ctx/mod.rs` follow the same pattern -
  `Command` enum via clap derive, `run()` match, private `cmd_*` functions.
  Every command supports `--json` using the `json_ok`/`json_err` envelope.
- **Atomic writes**: all YAML writes go through `workspace::write_yaml` (tmp +
  rename). Never write `.kazam/*.yaml` directly with `fs::write`.
- **ID generation**: `id::generate()` returns `kz-XXXX` (4 hex chars). Uses
  `SystemTime` + `process::id()` + atomic counter through `DefaultHasher` - no
  `rand` dependency.
- **Board**: `board.rs` constructs `Page`/`Component` structs in memory (same
  types from `types.rs`) and renders via `render::render_page()`. It never
  writes YAML - read-only from `.kazam/`.
- **Hook scripts**: embedded as `const &str` in `ctx/hooks.rs`. The `install`
  fn writes them to `.kazam/hooks/` and patches `.claude/settings.json`.

Adding a new `track` or `ctx` subcommand:
1. Add the variant to the `Command` enum in the module's `mod.rs`
2. Add the match arm in `run()`
3. Write a `cmd_*` function following the `--json` envelope pattern
4. If it mutates state, go through `store::*` helpers (track) or
   `workspace::write_yaml` (ctx)

## Docs site

`docs/` is a kazam site itself. Changes to component rendering should be visually verified by running `./target/release/kazam dev docs --port 3002`.

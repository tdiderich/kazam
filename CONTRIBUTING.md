# Contributing to kazam

Thanks for the interest. kazam is small on purpose — Rust CLI, YAML in, HTML out — and the goal is to keep it that way. This guide covers how to propose a change, what we value in a PR, and how contributors can safely use coding agents alongside their own work.

## What kind of contributions are valued

- **Bug fixes** with a reproducer in the description.
- **New components** that compose with existing ones. A component earns its place by unlocking a real page type; a one-off CSS trick usually doesn't.
- **Theme tokens / print / accessibility improvements** to existing components.
- **Docs** — the hosted site lives in `docs/`, authored in kazam itself. Adding examples is always welcome.
- **CI / dev ergonomics.**

If you're unsure whether a change fits, open a small issue first and describe the use case.

## Getting set up

```bash
git clone https://github.com/YOUR-FORK/kazam
cd kazam
git config core.hooksPath .githooks   # enable pre-commit checks
cargo build --release
cargo test --release
./target/release/kazam dev docs --port 3002   # live-edit the docs site
```

The `core.hooksPath` line activates the repo's pre-commit hook (`.githooks/pre-commit`), which runs `cargo fmt --check` and `cargo clippy` before every commit. Use the latest stable Rust toolchain via [rustup](https://rustup.rs). The repo pins nothing — if stable works, we support it.

## Fork + PR flow

1. Fork on GitHub and clone your fork.
2. Branch off `main`: `git checkout -b feature/short-name`.
3. Make changes. Keep the diff focused — one concern per PR is easiest to review.
4. Run the local checks before pushing:
   ```bash
   cargo test --release
   cargo fmt --check
   cargo clippy --release -- -D warnings
   ```
5. Push your branch and open a PR against `tdiderich/kazam:main`.
6. A maintainer reviews. Expect small turnaround and occasional pushback on scope.

## PR checklist

- [ ] `cargo test --release` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --release -- -D warnings` passes (or justify any exception in the PR)
- [ ] If you changed the output HTML or CSS, eyeball the docs site (`kazam dev docs --port 3002`) to confirm nothing regressed.
- [ ] If you added a component or config field, update `AGENTS.md.template` and the relevant page under `docs/components/`.
- [ ] Commit messages are in the imperative mood ("add X", not "added X").

## Code style

- Prefer small, well-named functions over macros or generics.
- Keep components self-contained: one render fn in `src/render/components.rs`, matching styles in `src/theme.rs`, type in `src/types.rs`.
- Don't introduce a new runtime dependency without a good reason. kazam's value is partly that it ships as one tiny binary.
- Inline scripts for interactivity go in `src/render/scripts.rs`. We don't ship a JS build system and don't intend to.
- Theme-aware colors use CSS custom props (`var(--accent-rgb)`, `var(--text-rgb)`, etc.), not hardcoded rgba literals. If you find a hardcoded color, fix it.

## Using coding agents

kazam is fine territory for LLM-assisted contributions. A couple of expectations:

- **Point the agent at `AGENTS.md`** when it's writing kazam YAML (pages, examples, docs content). `kazam agents` dumps the same guide.
- **Review the agent's diff yourself before opening a PR.** Autonomous-mode commits that the contributor hasn't read are the usual source of low-quality PRs we'll close.
- **Don't paste secrets or third-party code** into agent prompts or into the repo. See `SECURITY.md` for more.
- **Disclose agent usage in the PR body** if a meaningful portion was agent-authored. We're not against it; we just want the review to focus on the right things.

## Architecture — where things live

```
src/
  main.rs                 CLI entry + subcommands
  build.rs / dev.rs       Batch build and watch-serve
  render/
    mod.rs                Page render orchestration
    shells.rs             Shell chrome (standard, document, deck)
    components.rs         Each component's render fn
    scripts.rs            Bundled inline JS (deck nav, tabs, etc.)
  theme.rs                Theme tokens + STATIC_CSS
  types.rs                All YAML-facing types (serde structs)
  annotations.rs          Sidecar annotation system (load, save, decay)
  audit.rs                Site health auditing
  freshness.rs            Freshness tracking + drift + notify
  ingest.rs               Notion (and future) content ingestion
  mcp/
    mod.rs                MCP server dispatch (stdio + HTTP transports)
    protocol.rs           JSON-RPC types
    tools.rs              MCP tool implementations (8 tools)
  workspace.rs            Agent workspace init + status
  track.rs / ctx.rs       Task tracking + context intelligence
  wish.rs                 Wish registry (list, init from recipes)
  llms.rs                 llms.txt emission
  init.rs / agents.rs     `kazam init` / `kazam agents` scaffolding
  minify.rs               Release-mode HTML/CSS/JS minification
wishes/                   Portable agent recipes (deal-360, debrief, etc.)
docs/                     The hosted docs site (itself a kazam site)
AGENTS.md.template        Authoring guide bundled into the binary
tests/                    Integration tests
```

Most component additions touch: `types.rs` (struct), `render/components.rs` (HTML), `theme.rs` (CSS), plus a `docs/components/*.yaml` example.

## Agent Graph Language (`.agl`), the `~/.kazam/agl` hub

`.agl` specs and their importable fragments live in one place on a machine,
by convention rather than any hub-management subcommand:

```
~/.kazam/agl/
  specs/    authored .agl specs, one per task
  shared/   importable fragments (invariant { ... } blocks only)
```

A bare name with no `/` and no `.agl` extension, passed to `validate`,
`export`, `flow`, or `skill`, is shorthand for `~/.kazam/agl/specs/<name>.agl`.
An `import "some/path.agl"` line inside a spec resolves relative to the
importing file first, then falls back to `~/.kazam/agl/shared/<path>`.

Grammar beyond what's in `PRODUCT.md`/`README.md`:

- `import "path.agl"`, zero or more, before the `spec` keyword. Pulls a
  fragment's `invariant { ... }` rules into the importing spec. Fragments
  can nest imports; cycles are a hard error.
- `requires: Server.method, Server.other_method`, optional, inside the
  spec block after `out:`. Declares the dotted tool names the flow depends
  on. `kazam agl validate` cross-checks this against every `call()`/`map()`
  in the flow (`undeclared-tool-dependency` / `unused-tool-dependency`
  warnings) so the list stays trustworthy. `kazam agl skill` renders it as
  a preflight instruction: confirm every tool is available before executing
  any state, abort immediately if one is missing, rather than discovering
  the gap mid-graph after other states already ran.

`kazam agl flow <spec>` prints a plain ASCII rendering of the graph, states,
actions, transitions, branch fan-out, meant as a quick "what does this
actually do" read for a human, separate from `kazam agl skill`'s per-target
compiled output (which embeds the same diagram after its preflight section,
before the resolved source).

## License

By submitting a PR you agree that your contribution is licensed under the repo's MIT license.

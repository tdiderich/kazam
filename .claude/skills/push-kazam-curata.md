---
name: push-kazam-curata
description: |
  Full release pipeline: kazam → curata OSS → curata-app → maze-apps.
  Each phase waits for the previous to fully complete before proceeding.
  Use when asked to "push", "release", "deploy", or "ship" changes across
  kazam and curata repos.
triggers:
  - push kazam curata
  - release kazam curata
  - deploy curata
  - ship it
---

# Kazam / Curata Release Pipeline

Read the full workflow from technical-success-hub before executing:

```
mcp__technical-success-hub__read_page slug: workflow-kazam-curata-release
```

Follow the workflow phases in strict order. Summary below — the workflow page
is the source of truth.

## Phase 1 — kazam: push + wait for build

1. `cargo fmt` in kazam repo
2. Commit all kazam changes, push to main
3. **WAIT**: `gh run watch` — Release workflow must fully complete

## Phase 2 — curata OSS: push + wait for build

1. Copy built binary: `cp kazam/target/release/kazam curata/.bin/kazam`
2. In curata repo: `pnpm generate` to regenerate files
3. Commit generated files + any curata-specific changes, push to main
4. If no curata changes beyond generated files and those are already current, use empty commit:
   `git commit --allow-empty -m "chore: redeploy for kazam <sha>"`
5. **WAIT**: pre-push hook runs build + tests. Must all pass.

## Phase 3 — Wait gate

Verify curata OSS commit is on main before proceeding.

## Phase 4 — curata-app: empty commit to rebuild

1. In curata-app repo:
   `git commit --allow-empty -m "chore: redeploy for curata OSS <sha>"`
   `git push`
2. **WAIT**: CI passes, Railway deploys

## Phase 5 — maze-apps: sync + merge

1. `gh workflow run sync-curata.yml --repo AtlasSecurityInc/maze-apps`
2. Wait for PR, review, merge
3. Verify deployment on TS Hub + KB

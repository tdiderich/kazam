# kazam + curata — Product Plan

## Model

**kazam** is the free, open-source engine. **curata** (curata.ai) is the paid BYOA platform built on top. ProjectDiscovery/Nuclei model: the OSS tool is the top of the funnel, the platform is the business.

### kazam (OSS CLI) — free forever

The full Rust binary. Build sites, track freshness, run audits, ingest from Notion, scaffold wishes, MCP server with annotation tools, agent workspace — everything in this repo.

Users install via `cargo install` or Homebrew, run locally or in CI, own everything. Every curata customer starts here.

### curata (BYOA Platform) — paid

Bring Your Own Agent. Customers bring their own AI agents with company-specific skills and MCP servers. curata provides:

- **Web annotation view** — rich UI for commenting on rendered pages (text selection, section-level notes, status tracking)
- **Hosted site** — CDN-served `_site/` output with auth (SSO/magic link)
- **Queued updates feed** — activity feed showing agent changes with rendered before/after diffs
- **Merge pipeline** — linear queue with push/approval/queued modes per page
- **Notifications** — Slack/email/Teams nudges for stale pages, pending approvals, audit summaries

Customers provide their own AI tool (Claude Code, Cursor, Gemini CLI) and their own API keys for integrations. curata never runs LLMs on the customer's behalf in the base product.

**BYOA is the whole product, not a tier.** "Managed" (curata runs agents for you) is speculative/future — only pursue if demand proves out.

### Why this works

- Near-zero marginal cost: static site CDN + lightweight approval service + notification webhooks
- No LLM costs (customer's keys)
- No data pipeline costs (customer runs locally)
- The annotation bridge: annotations are written via kazam CLI/MCP (free) but the rich web annotation UI is curata (paid). Clean OSS/paid boundary.

---

## Architecture — the annotation bridge

Annotations are the key primitive that connects kazam and curata. They're human context that data sources can't capture: "customer is evaluating Wiz," "timeline moved to Q3," "this section is wrong."

**In kazam (OSS):**
- Sidecar YAML files in `.kazam/annotations/<page-slug>/`
- CLI: `kazam annotate <page> "text" --section competitive --author tyler`
- MCP tools: `annotate_page`, `list_annotations`, `update_annotation`
- Build renders annotations inline with age indicators and status badges
- 14-day decay tracking; annotation-aware refresh prompts

**In curata (paid):**
- Rich web annotation view (TypeScript/React, separate repo)
- Text selection, comment anchoring, interactive status updates
- Talks to kazam's HTTP MCP API (`--remote --token`)

---

## Phases

### Done (kazam 1.5.0)

| Phase | What | Status |
|-------|------|--------|
| 0 | Manual validation — comment blocks on 3-5 deals | Done |
| 1 | Sidecar annotation schema + CLI + MCP + build rendering | Done |
| 2 | Annotation-aware refresh — deal-360 prompt reads annotations | Done |
| 3 | HTTP MCP hardening — `--local`/`--remote`, bearer token auth | Done |

### Next (curata repo)

| Phase | What | Status |
|-------|------|--------|
| 4 | Web annotation view — rich UI in curata repo (TypeScript/React) | Next |
| 5 | Deploy on 5 Maze deals + extend annotations to debrief/call-prep | Open |
| 6 | curata platform — auth/RBAC, hosted CDN, BYOA API, diff review UI | Open |

---

## Merge Pipeline (curata)

Every agent change goes through one of three modes. The page owner (or site default) picks which.

### Mode 1: Push (default)
Changes land immediately. Conflicts block — the agent must resolve and retry.

### Mode 2: Request Approval
Changes queue for assigned reviewers before going live.

```yaml
freshness:
  merge: approval
  reviewers:
    - legal@company.com
```

### Mode 3: Queue for Release
Changes stage and deploy on a schedule or manual trigger.

```yaml
freshness:
  merge: queued
  release: weekly
```

---

## Target Customer

AI-native startups (10-200 people) who:
- Already use an AI coding tool (Claude Code, Cursor, Gemini CLI)
- Have docs scattered across Notion/Confluence/Google Docs
- Know their docs are stale but don't have a process to fix it
- Would pay to never think about doc freshness again

GTM teams are the entry point — they feel the pain most acutely (deal pages, competitive docs, customer briefs) and the annotation pattern maps directly to their workflow.

## Key Insight

BYOA means curata is a convergence layer, not an AI product. Customers' agents already know their business; curata provides the structured surface where those agents' outputs meet. The value isn't the AI — it's the meeting point.

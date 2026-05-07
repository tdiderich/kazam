# kazam Product Plan

## Tiers

### Free — Self-Hosted (OSS CLI)
**Price:** Free forever
**What they get:** The full kazam binary. Build sites, track freshness, run audits, ingest from Notion/Confluence, scaffold wishes, MCP server — everything in the repo today.
**How they use it:** `cargo install`, run locally or in CI. They own everything.
**Moat:** The CLI is the top of the funnel. Every user starts here.

### Self-Managed — Hosted Site + Queued Updates
**Price:** ~$100/month, unlimited users/pages (up to a KB size limit)
**What they get:**
- Hosted site — we build and serve their `_site/` on a CDN with auth (SSO/magic link)
- Queued updates feed — activity feed showing every agent change with rendered before/after diffs; optional hold-for-review gate on sensitive pages
- Notifications — Slack/email/Teams nudges for stale pages, pending approvals, audit summaries
- Freshness dashboard — live _health.html, not just build-time static

**What they provide:**
- Their own AI tool (Claude Code, Gemini CLI, Cursor, etc.) for agent work
- Their own API keys for MCP integrations (Notion, Slack, HubSpot, etc.)
- They run `kazam audit`, `kazam ingest`, wishes, etc. locally or in CI
- Agent pushes changes → queued updates feed shows what landed and what's pending review

**Why $100/mo works at ~100% margin:**
- Infra = static site CDN + lightweight approval service + notification webhooks
- No LLM costs (customer's keys)
- No data pipeline costs (customer runs locally)
- Scale concern: very large KBs (1000+ pages) might need build compute limits → tier by page count or build minutes

**Pricing risk:** KB size. Options:
- Flat $100/mo up to 500 pages, $200/mo up to 2000, custom above
- Or flat unlimited — most startups won't hit 500 pages for years

### Self-Driving — We Run the Agents
**Price:** $300-1000/month depending on tier
**What they get:**
- Everything in Self-Managed
- We run the agent loops (freshness refresh, audit-fix, debrief digests)
- Token budget included per tier
- Observer hooks — auto-capture from Slack/GitHub (decisions, PRs → page updates)
- Inline annotation UX — cmd+K on any section to flag corrections, digest pass updates pages
- Scheduled refresh runs (weekly/daily depending on tier)

**Tiers by token budget:**
- Starter ($300/mo): up to 200 pages, weekly refresh, 500k tokens/mo
- Growth ($500/mo): up to 1000 pages, daily refresh, 2M tokens/mo  
- Enterprise ($1000/mo): unlimited pages, continuous refresh, 10M tokens/mo, custom integrations

**Why this tier exists:** Some companies want the value but don't want to set up Claude Code / manage agent workflows. They want to point kazam at their Notion, connect Slack, and have it just work.

---

## What Needs Building — By Tier

### Free (OSS CLI) — ship now, mostly done
| Feature | Status | Notes |
|---------|--------|-------|
| kazam build/dev/init | ✅ Done | |
| Freshness tracking + audit | ✅ Done | |
| MCP server | ✅ Done | |
| Notion ingest | ✅ Done | |
| Wishes (audit-fix, debrief, deal-360, etc.) | ✅ Done | |
| GitHub Action: freshness CI gate | 🔲 TODO | Viral hook — fails build if overdue pages exceed threshold |
| GitHub Action: PR staleness bot | 🔲 TODO | Comments on PRs that touch stale page sources |
| Confluence ingest | 🔲 TODO | Lower priority, messier API |
| `kazam init --from-notion` one-liner | 🔲 TODO | Chains ingest → build → dev for 5-min demo |

### Self-Managed ($100/mo) — the product
| Feature | Status | Notes |
|---------|--------|-------|
| Hosted build + CDN | 🔲 TODO | Accept git push, run kazam build, deploy to CDN |
| Auth (SSO / magic link) | 🔲 TODO | Who can view the site |
| Queued updates feed | 🔲 TODO | Activity feed of all changes; optional hold-for-review on sensitive pages |
| Merge pipeline | 🔲 TODO | Linear queue: one merge at a time, reject-on-conflict |
| Notification service | 🔲 TODO | Slack/email webhooks for stale pages + pending approvals |
| Live health dashboard | 🔲 TODO | _health.html but dynamic, not build-time |
| Billing / subscription | 🔲 TODO | Stripe integration |
| Onboarding flow | 🔲 TODO | Connect repo → first build → invite team |

### Self-Driving ($300-1000/mo) — the vision
| Feature | Status | Notes |
|---------|--------|-------|
| Managed agent loops | 🔲 TODO | Run refresh/audit/debrief on schedule with our keys |
| Token metering + limits | 🔲 TODO | Track usage per customer, enforce tier limits |
| Observer hooks | 🔲 TODO | GitHub webhook + Slack listener for auto-capture |
| Inline annotation UX | 🔲 TODO | cmd+K on rendered pages, digest pass |
| Custom integration setup | 🔲 TODO | Enterprise onboarding for bespoke data sources |

---

## Go-To-Market Sequence

1. **Now:** Ship the two GitHub Actions. These are the viral hooks that get kazam into repos and make freshness visible in PR workflows.
2. **Next:** Build the hosted site + queued updates feed (self-managed tier). This is the revenue unlock. Target: AI-native startups who already use Claude Code / Cursor and have docs in Notion.
3. **Then:** Add the self-driving tier for companies who want turnkey. This is where the real margin is but also the most complex to operate.

## Target Customer
AI-native startups (10-200 people) who:
- Already use an AI coding tool (Claude Code, Cursor, Gemini CLI)
- Have docs scattered across Notion/Confluence/Google Docs
- Know their docs are stale but don't have a process to fix it
- Would pay $100/mo to never think about doc freshness again

## Merge Pipeline (Self-Managed + Self-Driving)

Every agent change goes through one of three modes. The page owner (or site default) picks which.

### Mode 1: Push (default)
Changes land immediately. Conflicts block — the agent must resolve locally and retry.

1. Agent pushes to main
2. System runs `kazam build` + `kazam validate`
3. Build passes → deploy, update appears in the activity feed
4. Build fails → reject, notify agent/owner with error
5. Conflict → reject, agent re-runs against latest main

This is the right default. Most agent changes — freshness bumps, metric refreshes, ingest runs — don't need a human in the loop. The activity feed gives visibility after the fact; owners can revert from the feed if something looks wrong.

### Mode 2: Request Approval
Changes queue for assigned reviewers before going live. For pages where auto-push isn't appropriate.

```yaml
freshness:
  merge: approval       # changes queue for review before going live
  reviewers:            # who can approve
    - legal@company.com
    - alice@company.com
```

1. Agent pushes changes → they appear in the feed as **pending**
2. Feed shows a **rendered before/after** (not raw YAML diffs)
3. Assigned reviewer approves → changes go live
4. Reviewer rejects → changes discarded, agent notified with reason
5. Pending changes unreviewed for 7d → reminder notification

Good for: policies, compliance docs, external-facing content, new pages.

### Mode 3: Queue for Release
Changes are staged and deploy on a schedule or manual trigger. For teams that want batched, predictable updates.

```yaml
freshness:
  merge: queued         # changes stage until released
  release: weekly       # or: manual, daily, "2026-06-01"
```

1. Agent pushes changes → they stage in the release queue
2. Queue shows all pending changes with rendered diffs
3. On release trigger (schedule or manual button) → all staged changes deploy at once
4. Conflicts within the queue are resolved at release time, not push time

Good for: teams that want a "documentation release" cadence, regulated environments, sites where changes should batch (e.g., weekly knowledge base refresh).

### Site-level default + per-page override

```yaml
# site.yaml — site-wide default
merge:
  default: push         # push | approval | queued
  release: weekly       # only applies when default is queued

# page.yaml — per-page override
freshness:
  merge: approval
  reviewers: [legal@company.com]
```

### Merge mechanics
- Linear queue, one merge at a time — simple, no locking
- Reject on conflict (push/approval modes) — agent re-runs, it's cheap
- Build validation on every merge — broken pages never deploy

### What this replaces
- GitHub PR review (too technical for non-eng page owners)
- Slack "hey can you approve this" threads (no audit trail, easy to miss)
- Manual git merges (customers shouldn't think about git)

### Future
- Batch approvals ("approve all 8 pending changes at once")
- Confidence scoring (high-confidence auto-pushes even on approval pages)
- One-click revert from the activity feed
- Release notes auto-generated from staged queue

## Key Insight
The self-managed tier is the sweet spot. Customer brings their own AI, we just host the output and provide the queued updates feed. Near-zero marginal cost, high perceived value ("my docs update themselves"), natural upsell to self-driving when they want to stop running agents manually.

# MCP Content Mapping - Wish/Recipe Opportunities

> Generated 2026-05-05 from live surveys of all connected MCPs.
> Review this to decide which recipes to build next.

## Summary

6 data sources surveyed. **28 recipe candidates** identified, **15 rated high-feasibility**.

| Source | Connected | Recipes Found | Top Pick |
|--------|-----------|---------------|----------|
| HubSpot | yes | 5 | Customer Health Scorecard |
| Linear | yes | 4 | Sprint Velocity Report |
| Attention | yes | 5 | Competitive Intelligence |
| Granola | yes | 7 | Action Item Tracker |
| Slack | yes | 7 | #insights Call Intelligence Digest |
| Gmail | yes | 4 | CI Health Dashboard |

**Already built:** hubspot-icp, linear-team-map, freshness-notifier

---

## HubSpot (CRM)

**Data available:** 616 deals across 4 pipelines, 19k companies, 42k contacts. Rich custom properties: `loss_reason` enum on 333/401 closed-lost deals, `health_summary` JSON blob on all 7 customers (refreshed daily with Red/Yellow/Green per-dimension scores, MAU/WAU, stickiness).

### Recipes

| # | Recipe | Feasibility | Data Quality | Notes |
|---|--------|------------|--------------|-------|
| 1 | **Customer Health Scorecard** | HIGH | Excellent | `health_summary` JSON on all 7 customers - per-dimension scores, utilization %, noise reduction %, connector coverage. Daily refresh. Nearly zero transformation needed. |
| 2 | **Win/Loss Analysis** | HIGH | Good | 333 closed-lost with structured `loss_reason` + free-text `closed_lost_reason` + `last_stage_before_closed_lost`. Only 22 wins - really a loss analysis page. |
| 3 | **Pipeline Funnel Health** | HIGH | Good | Clean stage counts (105→42→11→7→1→4→7 won, 401 lost). Drop-off rates computable. Caveat: `amount` is $50k placeholder until S4+. |
| 4 | **Sales Rep Performance** | MEDIUM | Partial | 6 active reps with deal ownership. Issue: no revenue attribution until late stage. |
| 5 | **Sourcing/Attribution** | LOW | Sparse | `outreach_first_touch_type` on 13% of deals. Directional only. |

---

## Linear (Project Management)

**Data available:** 14 teams, 6 running weekly cycles (92 issues in current sprint). 22 active initiatives with owners and health signals. 20+ active projects. Label taxonomy: Bug/Feature/Improvement + scheduling labels.

### Recipes

| # | Recipe | Feasibility | Data Quality | Notes |
|---|--------|------------|--------------|-------|
| 1 | **Sprint Velocity Report** | HIGH | Clean | 6 teams with `issueCountHistory` arrays per cycle. Current sprint load: Maze(25), Data(24), Analysis(21), Infra(10), Response(8), ML(4). |
| 2 | **Initiative Tracker** | HIGH | Good | 22 active initiatives, pillar naming consistent (`Data -`, `Analysis -`, `Infra -`, etc.), owners present, 2 explicitly `atRisk`. |
| 3 | **Bug Backlog Health** | HIGH | Good | `Bug` label used consistently. `Customer Reported Bugs` initiative exists. Filter label=Bug, state!=Done per team. |
| 4 | **Project Status Dashboard** | MEDIUM | Thin | Quality gated on status update frequency - only 1 project (Remediation for EC2) writes rich updates. Falls back to issue state distribution. |

**Note:** `list_projects` API returns 400 - projects discoverable via issues and initiatives only.

---

## Attention (Call Intelligence)

**Data available:** 33 users, 2 teams (Sales, Engineering). Corpus spans Dec 2025–present. Dozens of calls/week. Per-call AI extraction: Need, Fit, Budget, Timeline, Authority, Next Steps. Demo scorecard with 6 behavioral dimensions.

### Recipes

| # | Recipe | Feasibility | Data Quality | Notes |
|---|--------|------------|--------------|-------|
| 1 | **Competitive Intelligence** | HIGH | Excellent | Wiz in 68 calls (30d), CrowdStrike 13, Qualys multiple. Verbatim prospect comparisons + rep responses. Richest single recipe available across all MCPs. |
| 2 | **Objection Handling Guide** | HIGH | Good | Structured Insights already extract objections with speaker attribution. Real quotes, not sanitized marketing copy. |
| 3 | **Customer Voice / Feature Requests** | HIGH | Good | `Need` and Fit Signals fields extract verbatim pain statements. `Product Feedback: Yes` label for filtering. |
| 4 | **Sales Playbook (Rep Patterns)** | MEDIUM | Good | Demo scorecard has behavioral scores per call with timestamps. Requires cross-rep comparison logic. |
| 5 | **Deal Risk Signals** | MEDIUM | Good | Timeline/Authority/Budget fields per call + recency. Would flag stalls, missing authority, unaddressed Wiz overlap. |

**Architecture note:** Reliable path is `transcript_search` → get call IDs → `ask_attention` (up to 25 calls). General `search_calls` without filters errors.

---

## Granola (Meeting Notes)

**Data available:** 53 meetings in 30 days. No folders (flat list). High-quality AI summaries with explicit decisions, named action items, and deal-level specificity. Recurring: GTM Daily, Weekly Wrap, Deal Review, Feature Triage, customer cadence syncs.

### Recipes

| # | Recipe | Feasibility | Data Quality | Notes |
|---|--------|------------|--------------|-------|
| 1 | **Action Item Tracker** | HIGH | Excellent | Every summary surfaces owners + specific commitments. Highest signal-to-noise recipe from Granola. |
| 2 | **Weekly Digest** | HIGH | Excellent | Weekly Wrap is purpose-built for this - already reads like a digest page. Near-zero transformation. |
| 3 | **Feature Request Log** | HIGH | Good | Feature Triage runs biweekly with status taxonomy (Parked/To Do/In Progress/Done) + customer attribution. |
| 4 | **Decision Log** | HIGH | Good | Dense in Deal Review (per-rep strategy calls) and Feature Triage (status transitions). |
| 5 | **Customer Relationship Map** | MEDIUM | Partial | Participant roles inconsistent - needs enrichment pass. |
| 6 | **Meeting Cadence Report** | MEDIUM | Partial | Duplicate entries (same call, two note-takers) need dedup. |
| 7 | **Bug/Blocker Tracker** | MEDIUM | Partial | Recurring blockers surface across meetings but need keyword extraction. |

**Constraint:** No folders - all queries must filter by meeting title pattern or attendee domain.

---

## Slack

**Data available:** ~10 substantive public channels + 15+ auto-generated `#int-deal-*` channels (HubSpot Breeze). Key channels: #announcements, #tech, #product, #product-launches, #insights, #support. 3 canvases (Customer Chronicle monthly report, AWS onboarding runbook, Fleet enrollment runbook).

### Recipes

| # | Recipe | Feasibility | Data Quality | Notes |
|---|--------|------------|--------------|-------|
| 1 | **Call Intelligence Digest** | HIGH | Excellent | #insights receives structured Attention bot summaries per deal call (TLDR, Context, Pitch+Reaction, Product Gaps, Deal Dynamics, Next Steps). Already machine-parseable. |
| 2 | **Support Ticket Status** | HIGH | Excellent | #support gets Plain.com structured notifications: T-number, AI summary, priority, assignee, status. |
| 3 | **Product Launch Tracker** | HIGH | Good | #product-launches has weekly structured posts (Going out this week / Next week / Coming Later). |
| 4 | **Customer Chronicle** | HIGH | Good | Monthly canvas (F0AQLPD5KL3) with health table, utilization %, sentiment, initiatives. Already repeating format. |
| 5 | **Active Deal Pipeline** | MEDIUM | Good | #tech POV roster + #int-deal-* channel list + HubSpot URLs from topic fields. |
| 6 | **Channel Directory** | MEDIUM | Easy | Auto-generated from `slack_search_channels` - #int-deal-* naming pattern is machine-readable. |
| 7 | **New Hire Digest** | MEDIUM | Good | #announcements new hire posts follow a template (name + role + location). |

**Notable absences:** No #incidents or #decisions channel. Incident content scattered in #tech.

---

## Gmail

**Data available:** ~50 threads/week. Dominated by GitHub automation (CI failures, PR merges, deploy approvals) and calendar accept/decline noise. 6 user labels exist but aren't consistently applied. Low substantive customer content in email.

### Recipes

| # | Recipe | Feasibility | Data Quality | Notes |
|---|--------|------------|--------------|-------|
| 1 | **CI Health Dashboard** | HIGH | Clean | `from:notifications@github.com subject:"Run failed"` is high-volume, same structure every email. Group by repo, count failures per workflow. |
| 2 | **Vendor Directory** | MEDIUM | Partial | External domains extractable from calendar threads + `label:external`. Needs 90-day window for coverage. |
| 3 | **Meeting Notes Digest** | LOW | Thin | Only 1 `gemini-notes@google.com` email in 7 days. Potential if this becomes standard practice. |
| 4 | **Open Follow-ups** | LOW | Unreliable | `label:Follow up` exists but requires manual application. |

**Verdict:** Gmail is the weakest source for auto-generated docs. Most valuable signal (customer comms, decisions) lives in Slack and Granola instead.

---

## Cross-Source Recipe Opportunities

These recipes combine multiple MCPs for richer pages:

| Recipe | Sources | Value |
|--------|---------|-------|
| **Deal 360 Page** | HubSpot (stage, amount) + Attention (call insights) + Slack (#int-deal-*) + Granola (meeting notes) | Per-deal dossier with CRM data, call intelligence, Slack activity, and meeting commitments |
| **Customer Health + Voice** | HubSpot (health_summary) + Attention (call sentiment) + Granola (sync notes) | Customer health scores enriched with actual voice-of-customer quotes and meeting outcomes |
| **Weekly Leadership Digest** | Granola (Weekly Wrap) + Linear (sprint velocity) + HubSpot (pipeline movement) + Slack (#announcements) | One-page weekly summary combining eng velocity, deal movement, and key announcements |
| **Competitive Battlecard** | Attention (call mentions) + Slack (#insights structured summaries) + Granola (Feature Triage for competitive features) | Living battlecard per competitor with real prospect quotes, win/loss context, and feature gaps |

---

## Recommended Build Order

**Tier 1 - High value, low effort (build these first):**
1. Customer Health Scorecard (HubSpot) - 7 records, daily-refreshed JSON, near-zero transformation
2. Competitive Intelligence (Attention) - 68 Wiz mentions alone, structured extraction exists
3. Action Item Tracker (Granola) - summaries already surface owners + commitments
4. Sprint Velocity Report (Linear) - cycle history arrays are clean, 6 teams

**Tier 2 - High value, moderate effort:**
5. Win/Loss Analysis (HubSpot) - 333 structured loss records
6. Weekly Digest (Granola + Linear + Slack) - cross-source but each piece is clean
7. Initiative Tracker (Linear) - 22 initiatives, owners, health signals
8. Call Intelligence Digest (Slack #insights) - already machine-parseable

**Tier 3 - Worth building but needs more work:**
9. Objection Handling Guide (Attention) - needs prompt engineering for extraction quality
10. Feature Request Log (Granola) - biweekly cadence, good structure
11. Support Ticket Status (Slack #support) - Plain.com schema is clean
12. Pipeline Funnel Health (HubSpot) - amount field unreliable pre-S4

**Defer:**
- Gmail-based recipes (weak signal)
- Customer Relationship Map (needs role enrichment)
- Meeting Cadence Report (needs dedup logic)
- Deal 360 and other cross-source recipes (build single-source first)

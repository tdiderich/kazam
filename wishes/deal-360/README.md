# Deal 360 Wish

Per-deal dossiers combining CRM data, call intelligence, and Slack activity into one page. The first cross-source kazam recipe.

## Data sources

| Source | What it provides | How it's accessed |
|--------|-----------------|-------------------|
| HubSpot | Deal stage, amount, owner, company, contacts, health | REST API via script (HUBSPOT_API_TOKEN) |
| Attention | Pain points, competition, objections, next steps, risk signals | MCP - agent queries per deal |
| Slack | Channel activity, blockers, scoping notes, POV progress | MCP - agent checks #int-deal-* channels |

## Prerequisites

- `HUBSPOT_API_TOKEN` - HubSpot private app token with CRM read access
- Attention MCP connected in your AI coding tool
- Slack MCP connected in your AI coding tool
- Python 3.8+ with `requests`

## What to customize

### script.py

- **`PIPELINE_NAME`**: Filter to a specific pipeline (default: all pipelines)
- **`STAGES_TO_INCLUDE`**: Stage label prefixes to include (default: all open stages). Set to `["S3", "S4", "S5", "S6"]` to focus on qualified+ deals.
- **`SLACK_CHANNEL_PREFIX`**: Your org's deal channel naming convention (default: `int-deal-`)
- **`MAX_DEALS`**: Cap on deals to fetch (default: 200)

### page.yaml

- **`freshness.owner`**: Set to the page owner's email
- **`personas`**: Adjust for your site's audience

## How to run

1. Copy `script.py` to `scripts/fetch-deal-360.py`
2. Copy `page.yaml` to your site's pages directory
3. Set `HUBSPOT_API_TOKEN` in `.env`
4. Run: `uv run --with requests python3 scripts/fetch-deal-360.py`
5. In a Claude Code session with Attention + Slack MCPs connected:
   "Read scripts/deal-360-prompt.md and scripts/deal-360-data.json. Build the Deal 360 page."

## What the output looks like

The finished page has:
- **Pipeline summary**: Deals per stage, total pipeline value (S4+ only, where amounts are reliable)
- **Per-deal sections** grouped by stage (latest stage first):
  - Deal table: owner, amount, close date, company info, champion, health score
  - Call intelligence callout: pain points, competitive landscape, objections, next steps, risk signals
  - Slack activity: recent channel messages, blockers, technical scoping, POV progress

## Architecture

This is the first cross-source recipe. The pattern:
1. **Script** handles the data source with a REST API (HubSpot) - fetches structured data, writes JSON
2. **Prompt** instructs the agent to enrich via MCPs (Attention, Slack) - these don't have REST APIs accessible from a script, but are available as MCPs in the agent's environment
3. **Agent** combines all three into the final page

This pattern works for any recipe that mixes API-accessible and MCP-only sources.

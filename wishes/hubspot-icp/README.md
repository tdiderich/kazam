# HubSpot ICP Wish

Build a data-driven Ideal Customer Profile page from your HubSpot CRM deals and Apollo company enrichment.

## Prerequisites

- `HUBSPOT_API_TOKEN` — HubSpot private app token with CRM read access (deals, companies, pipelines)
- `APOLLO_API_KEY` — Apollo.io API key (uses the paid `/organizations/enrich` endpoint, ~1 credit per company)
- Python 3.8+ with `requests` (run via `uv run --with requests` if not installed)

## What to customize

### script.py

- **`LATE_STAGE_PREFIXES`** (line ~15): These prefixes match your HubSpot deal stage labels for late-stage deals. The script prints all stage labels on first run — check the output and update these to match your pipeline.
- **`CLOSED_LOST_PREFIXES`** (line ~16): Same idea for closed-lost stages. The default uses `hs_is_closed_won=false` which works across all pipelines, so you may not need to change this.
- **`COMPANY_PROPS` / `DEAL_PROPS`**: Add any custom HubSpot properties you want in the CSV output.
- **Apollo enrichment**: If you don't have an Apollo key, the script still works — you just get HubSpot data only (no tech stacks, keywords, or funding data).

### page.yaml

- **`freshness.owner`**: Set to the page owner's email.
- **`eyebrow` and `personas`**: Adjust for your site's navigation structure.
- **Component structure**: The starter page is minimal. After the first analysis run, the agent will populate it with sections for Customer DNA, Pipeline Validation, Closed-Lost Patterns, Tier definitions, and Disqualifiers.

## How to run

1. Copy `script.py` to your site's `scripts/generate-icp-data.py` (the refresh step in page.yaml expects this name)
2. Copy `page.yaml` to your site's `site/` directory (rename as needed)
3. Set `HUBSPOT_API_TOKEN` and optionally `APOLLO_API_KEY` in your `.env`
4. Run: `uv run --with requests python3 scripts/generate-icp-data.py`
5. Check the CSV output at `scripts/icp-data.csv`
6. Have an agent read `scripts/icp-prompt.md` + the CSV and update the page

## What the output looks like

The finished page has:
- **Metrics**: row counts and data source summary
- **Customer DNA**: table of customers with firmographics + funding, tech stack signals across the customer base
- **Pipeline Validation**: late-stage deals confirming or extending the customer profile, deal size patterns
- **Closed-Lost Patterns**: table of losses with reasons, analysis of what kills deals
- **Tier 1/2/3**: data-backed company profile definitions at each confidence level
- **Disqualifiers**: hard and soft signals that indicate bad fit
- **Timing Indicators**: when to engage vs. when to wait

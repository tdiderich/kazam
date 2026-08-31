# Linear Team Map Wish

Build a "Who Owns What" page from your Linear workspace - maps people to teams, projects, and ownership based on actual issue activity.

## Prerequisites

- `LINEAR_API_KEY` - Linear API key with read access (Settings → API → Personal API keys)
- Python 3.8+ with `requests` (run via `uv run --with requests` if not installed)

## What to customize

### script.py

- **`LOOKBACK_DAYS`** (line ~20): How many days of issue history to scan. Default is 30. Increase for orgs with less frequent activity.
- **`EXCLUDED_TEAMS`** (line ~23): Team names to skip (test teams, archived teams).
- **`EXCLUDED_USERS`** (line ~26): Email addresses to exclude (bots, service accounts).

### page.yaml

- **`freshness.owner`**: Set to the page owner's email.
- **`eyebrow` and `personas`**: Adjust for your site's navigation structure.
- **Component structure**: The starter page is minimal. After the first analysis run, the agent populates it with per-team sections, cross-team people, and key observations.

## How to run

1. Copy `script.py` to your site's `scripts/fetch-team-map.py` (the refresh step in page.yaml expects this name)
2. Copy `page.yaml` to your site's pages directory (rename as needed)
3. Set `LINEAR_API_KEY` in your `.env`
4. Run: `uv run --with requests python3 scripts/fetch-team-map.py`
5. Check the output at `scripts/team-map-data.json` and `scripts/team-map-data.csv`
6. Have an agent read `scripts/team-map-prompt.md` + the JSON and update the page

## What the output looks like

The finished page has:
- **Per-team sections**: Each Linear team with members, issue counts, and active projects
- **Cross-functional people**: Anyone appearing on 2+ teams
- **Key observations**: Single points of failure (1-person teams), inactive users, overloaded people (3+ teams)
- **Active projects**: Project names, leads, progress, and target dates

## How team membership is inferred

Linear doesn't expose team membership directly. This script infers it from issue assignments - if someone is assigned issues on a team in the last N days, they're counted as a member. Issue count serves as a proxy for activity level and focus area.

# GitHub Actions for kazam

Copy these workflows to `.github/workflows/` in your kazam site repo.

## Workflows

- **build-and-check.yaml** — Build site, detect stale pages and broken links. Runs on push, PR, and weekday schedule.
- **scheduled-refresh.yaml** — Refresh stale pages using the `refresh` prompt template. Opens a PR with changes. Runs weekly.
- **audit-on-release.yaml** — Run voice/structure audit on YAML changes. Runs on push to main.
- **product-sync.yaml** — Example: sync content from Linear on a schedule. Adapt for any product integration.

## Required secrets

- `ANTHROPIC_API_KEY` — for Claude Code agent invocations
- Product-specific keys as needed (e.g., Linear API key in MCP config)

## Customization

- Adjust cron schedules to your team's cadence
- Change the `claude -p` prompts to match your site structure
- Add product-specific env vars for BYOK integrations

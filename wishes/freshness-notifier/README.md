# Freshness Notifier Wish

Notify content owners about stale docs via Slack DMs. Works two ways:

1. **MCP path** (recommended): Run `kazam freshness notify --json`, then use Claude Code with Slack MCP to send DMs automatically
2. **Script path**: Use the included Python script with a Slack bot token

## Prerequisites

**MCP path**: Slack MCP connected in Claude Code — no additional setup needed.

**Script path**: `SLACK_BOT_TOKEN` — Slack bot token with `chat:write`, `users:read.email`, and `im:write` scopes. Python 3.8+ with `requests`.

## MCP path (recommended)

1. Run `kazam freshness notify --json > scripts/freshness-digest.json`
2. In a Claude Code session with Slack MCP: "Read scripts/freshness-digest.json and scripts/freshness-notifier-prompt.md. Send each owner a Slack DM about their stale pages."

The agent looks up Slack users by email and sends formatted DMs.

## Script path

1. Copy `script.py` to your site's `scripts/freshness-notifier.py`
2. Set `SLACK_BOT_TOKEN` in your `.env`
3. Run: `kazam freshness notify --json | python3 scripts/freshness-notifier.py`

### Script options

- `--dry-run`: Preview messages without sending
- `--file <path>`: Read JSON from file instead of stdin
- `--channel <id>`: Post to a channel instead of individual DMs

### What to customize in script.py

- **`SITE_BASE_URL`**: Your deployed site URL for clickable page links
- **`OWNER_EMAIL_DOMAIN`**: Append domain if owners use bare usernames
- **`MESSAGE_HEADER`**: Greeting text at the top of each DM

## What owners receive

Each owner gets a DM listing their stale pages with status emoji, days overdue, title, and path. Includes instructions for `kazam freshness act` to refresh or archive.

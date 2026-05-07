# Freshness Notifier Wish

Notify content owners about stale docs via Slack, Teams, or stdout. Works two ways:

1. **MCP path** (recommended): Run `kazam freshness notify --json`, then use an agent with Slack or Teams MCP to send DMs automatically
2. **Script path**: Use the included Python script with a Slack bot token (Slack-only)

## MCP path (recommended)

1. Run `kazam freshness notify --json > scripts/freshness-digest.json`
2. In an agent session with messaging MCP: "Read scripts/freshness-digest.json and scripts/freshness-notifier-prompt.md. Send each owner a DM about their stale pages."

The agent detects which messaging platform is available and sends via that.

## Script path (Slack-only)

1. Copy `script.py` to your site's `scripts/freshness-notifier.py`
2. Set `SLACK_BOT_TOKEN` in your `.env`
3. Run: `kazam freshness notify --json | python3 scripts/freshness-notifier.py`

Options: `--dry-run`, `--file <path>`, `--channel <id>`

Customize in script.py: `SITE_BASE_URL`, `OWNER_EMAIL_DOMAIN`, `MESSAGE_HEADER`

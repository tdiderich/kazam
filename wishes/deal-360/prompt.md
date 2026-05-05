# Deal 360 Analysis Prompt

You have HubSpot deal data at `scripts/deal-360-data.json` with open deals across the pipeline.

## Your task

Build a Deal 360 page by combining three data sources per deal:

### 1. HubSpot data (already in JSON)
Each deal has: stage, amount, owner, close date, company (industry, size, health), contacts (names, titles), and a guessed Slack channel name.

### 2. Attention call intelligence (query via MCP)
For each deal, search Attention for calls mentioning the company name:
- Use `search_calls` with the company name as transcript search
- For deals with calls, use `ask_attention` (up to 25 call IDs) asking: "Summarize pain points, competitive landscape, objections raised, next steps, and risk signals for this deal"
- If no calls found, note "No call data"

### 3. Slack channel activity (query via MCP)
For each deal, check if the guessed Slack channel exists:
- Search for the channel name from `slack_channel_guess`
- If found, read recent messages for blockers, scoping notes, and POV progress
- If no channel found, note "No Slack channel"

### Output format

One section per deal, ordered by stage (latest first). Each deal gets a table (owner, amount, close date, company, champion, health) plus a call intelligence callout and Slack activity summary. Group by stage with stage headers. Add a pipeline summary at the top.

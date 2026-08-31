# Deal 360 Analysis Prompt

You have HubSpot deal data at `scripts/deal-360-data.json` with open deals across the pipeline.

## Your task

Build a Deal 360 page by combining four data sources per deal:

### 1. Owner annotations (highest priority - read via MCP)
For each deal, use the `list_annotations` MCP tool with the deal's page path to read sidecar annotations from `.kazam/annotations/`. These are notes from the deal owner - things they know from calls, relationships, and conversations that don't appear in any data source.

**Conflict resolution rule:** When an annotation directly contradicts a data source (ex. HubSpot says close date is June, an annotation says "push to Q3"), prefer the annotation. The deal owner has context the CRM doesn't capture.

After incorporating an annotation, note it in the output: "[Incorporated: annotation from {author}, {date}]" so the deal owner can see which notes were used.

After building the page, update each annotation's status using the `annotate_page` MCP tool:
- `incorporated` - the annotation was used in the page content
- `ignored` - the annotation was not relevant to this refresh
- Leave `pending` - if unsure whether the annotation is still relevant

### 2. HubSpot data (already in JSON)
Each deal has: stage, amount, owner, close date, company (industry, size, health), contacts (names, titles), and a guessed Slack channel name.

### 3. Attention call intelligence (query via MCP)
For each deal, search Attention for calls mentioning the company name:
- Use `search_calls` with the company name as transcript search
- For deals with calls, use `ask_attention` (up to 25 call IDs) asking: "Summarize pain points, competitive landscape, objections raised, next steps, and risk signals for this deal"
- If no calls found, note "No call data"

### 4. Messaging channel activity (query via MCP)
For each deal, check if the guessed channel exists in the available messaging platform (Slack or Teams):
- Search for the channel name from `slack_channel_guess`
- If found, read recent messages for blockers, scoping notes, and POV progress
- If no channel found, note "No deal channel"

### Output format

One section per deal, ordered by stage (latest first). Each deal gets:
- A table (owner, amount, close date, company, champion, health)
- An **Owner Context** callout showing which annotations were incorporated and any flagged as potentially stale (>14 days old)
- A call intelligence callout
- A channel activity summary

Group by stage with stage headers. Add a pipeline summary at the top.

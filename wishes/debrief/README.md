# Debrief Wish

Paste meeting notes or a transcript, agent updates the relevant report page(s) with new data.

## Quickstart

```bash
# In an agent session with your kazam site:
# "Read wishes/debrief/prompt.md. Here are my notes from the pipeline review: [paste]"
# Agent finds the Deal 360 page and updates deal stages, amounts, etc. in place.
```

## Input options

- **Paste** — raw meeting notes or transcript text
- **File** — path to a transcript file (`.txt`, `.md`)
- **Granola** — meeting ID if Granola MCP is connected

## What it does

- Extracts data points, decisions, corrections, and action items from meeting content
- Finds the right page(s) in your kazam site to update
- Updates content in place — reports stay current, not changelog-style
- Bumps `freshness.updated` to today
- Shows a diff for review before committing

## Best with

- **Deal 360** — pipeline reviews update deal stages, amounts, and signals
- **Project status pages** — sprint reviews update timelines and shipped features
- **Org charts / Who Owns What** — hiring syncs update headcount and roles
- **Any report page** — if the meeting produced data that belongs on a page, debrief handles it

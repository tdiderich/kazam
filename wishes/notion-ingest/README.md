# Notion Ingest Wish

Migrate a Notion workspace to kazam - ingest pages, audit staleness, triage content.

## Setup

1. Create a Notion integration at https://www.notion.so/profile/integrations/internal
2. Copy the integration secret (`ntn_...`) to `.env` as `NOTION_TOKEN`
3. Find your workspace ID: click workspace name (top-left) → Settings → General (bottom of page)
4. Add `NOTION_WORKSPACE_ID` to `.env`
5. In Notion, share the pages/databases with your integration (··· → Connections)
   - The integration needs access to content before it can read it
   - Child pages inherit access from parent, so share the root

## Quickstart

```bash
# Preview staleness before migrating (all accessible pages)
kazam ingest notion --all --stats

# Or target a specific page tree
kazam ingest notion --page <id> --stats

# Dry run - see what files would be created
kazam ingest notion --all --dry-run

# Import everything
kazam ingest notion --all --out docs/

# Audit the result
kazam audit docs/ --pretty
```

## Finding IDs

- **Page ID:** last 32 hex chars in the Notion URL (`notion.so/My-Page-abc123def456`)
- **Database ID:** hex string before `?v=` in database URLs
- **Workspace ID:** workspace name (top-left) → Settings → General (bottom of page)

## What it does

- Walks Notion page trees or database rows
- Converts blocks to kazam components (markdown, tables, code, callouts, etc.)
- Downloads Notion-hosted images to `assets/images/`
- Scaffolds freshness metadata from Notion timestamps
- Prints an aftermath report with staleness breakdown and next steps

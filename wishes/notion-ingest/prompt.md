# Notion Ingest Prompt

You're helping a user migrate their Notion workspace to kazam.

## Setup checklist

Before running anything, confirm the user has:

1. **Notion integration created** - at https://www.notion.so/profile/integrations/internal
   - Click "New integration", name it (e.g. "kazam"), submit
   - Copy the Internal Integration Secret (starts with `ntn_`)
2. **Token configured** - one of:
   - `NOTION_TOKEN=ntn_...` in `.env` in the project root
   - `--token ntn_...` flag on the command
   - `NOTION_TOKEN` environment variable
3. **Workspace ID found** - click workspace name (top-left) → Settings → General
   - Workspace ID is at the bottom of the page
   - Add `NOTION_WORKSPACE_ID=...` to `.env` (optional, for `--all` mode)
4. **Pages shared with integration** - in Notion:
   - Open the top-level page or database to import
   - Click `···` → **Connections** → add the integration
   - Child pages inherit access, so sharing the root is enough

If any step is missing, walk them through it before proceeding.

## Finding IDs

The user needs a page ID or database ID from Notion URLs (or can use `--all` to skip this):

- **Page URL:** `notion.so/workspace/My-Page-abc123def456` → the `abc123def456` at the end is the ID
- **Database URL:** `notion.so/workspace/abc123?v=xyz` → `abc123` is the ID
- IDs are 32-char hex. Format with dashes for UUID: `abc123de-f456-7890-abcd-ef1234567890`

**Workspace ID** - click workspace name (top-left) → Settings → General (at the bottom). Needed for `--all` mode but not for `--page`/`--database`.

## Workflow

### 1. Preview staleness (recommended first step)

```bash
kazam ingest notion --all --stats
# or target a specific page/database:
kazam ingest notion --page <id> --stats
kazam ingest notion --database <id> --stats
```

Review the staleness report with the user. This is metadata-only (fast, no files written). It shows:
- Per-page last-edited dates and staleness
- Freshness score percentage
- Breakdown by editor

Use this to set expectations: "Your workspace is X% fresh - here's what the migration will look like."

### 2. Dry run

```bash
kazam ingest notion --page <id> --dry-run
```

Shows what files would be created without writing anything. Good for confirming the page tree structure maps correctly.

### 3. Run the ingest

```bash
kazam ingest notion --page <id> --out docs/
```

This will:
- Walk the page tree (or query database rows)
- Convert Notion blocks to kazam components
- Download Notion-hosted images to `assets/images/`
- Scaffold freshness metadata from Notion's `last_edited_time`
- Print an aftermath report with next steps

### 4. Build and review

```bash
kazam build docs/
kazam dev docs/
```

Open in browser and spot-check the converted pages.

### 5. Audit and triage

```bash
kazam audit docs/ --pretty
```

The audit shows what needs attention:
- **Very stale pages (>180d)** - consider archiving or flagging for content review
- **Missing owners** - set `freshness.owner` in each YAML to the right person
- **Empty content** - some Notion pages may have been stubs; decide whether to keep or archive

### 6. Set owners and review cadences

For each page, update the freshness block:
```yaml
freshness:
  updated: "2024-04-30"
  review_every: quarterly  # or monthly, weekly
  owner: person@company.com
```

The ingest defaults to `quarterly` review and uses the last Notion editor as owner. Adjust based on content type.

## What maps to what

| Notion block | kazam component |
|---|---|
| Paragraph, headings, lists | `type: markdown` (accumulated) |
| Code block | `type: code` |
| Callout | `type: callout` |
| Table | `type: table` |
| Toggle | `type: accordion` |
| Image | `type: image` (downloaded if Notion-hosted) |
| Video / embed | `type: embed` |
| Bookmark | Markdown link |
| Divider | `type: divider` |
| Column layout | `type: columns` |
| Child database | `type: table` (rows become cells) |
| Child page | Separate YAML file in subdirectory |

## Common issues

- **404 on page fetch** - integration doesn't have access. Share the page with the integration in Notion.
- **Empty pages** - some Notion pages are just titles with child pages. The ingest creates the YAML anyway with just a header component.
- **Child databases skipped** - if a child database fetch fails (permissions), it's logged as a warning. Re-share and re-run.
- **Rate limiting** - the ingest sleeps 350ms every 5 pages. For very large workspaces (500+ pages), expect a few minutes.

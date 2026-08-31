# Debrief Prompt

You're updating a kazam report page with new information from a meeting.

## Input

The user will provide one of:
- **Pasted transcript/notes** - raw text from a meeting (Granola, Otter, manual notes)
- **Granola meeting ID** - fetch via Granola MCP if available
- **A file path** - to a transcript or notes file

And a **target page** - the kazam YAML page to update. If not specified, ask which page to update. Use `kazam search` or check the site's page list to find the right one.

## What to extract

From the meeting content, identify:

1. **Updated data points** - numbers, metrics, dates, statuses that supersede what's currently on the page
2. **Decisions made** - choices that should be reflected in the page content
3. **New information** - facts, context, or details not yet on the page
4. **Action items** - commitments that affect the page's accuracy or completeness
5. **Corrections** - things the page says that the meeting contradicts

## How to update

1. Read the target page YAML
2. For each piece of extracted information:
   - Find the component where it belongs (match by section heading, table, or context)
   - Update the content in place - don't append a "meeting notes" dump
   - If the info doesn't fit any existing section, note it for the user
3. Bump `freshness.updated` to today's date
4. Show the user a diff of what changed and why

## Rules

- **Update in place, don't append.** The page should read as a current report, not a changelog. If a metric was 45 and the meeting says it's now 52, change 45 to 52.
- **Preserve structure.** Don't reorganize the page - just update the content within existing components.
- **Flag ambiguity.** If a meeting quote could mean multiple things, ask rather than guess.
- **Skip chatter.** Only extract information that's relevant to the target page. Meeting small talk, scheduling, and off-topic discussion should be ignored.
- **Attribute sparingly.** Don't add "per [name] in the 5/6 meeting" - that's what git blame is for. Only attribute if the source matters (ex. "CEO confirmed" vs. generic update).

## Example

**Meeting notes:**
> Pipeline review - Tyler mentioned Acme deal moved to S5, amount revised to $180k (was $150k). Lost the Initech deal, they went with competitor. New deal: Globex Corp, $95k, S2, Sarah owns it.

**Target page:** deal-360.yaml

**Updates:**
- Acme deal table row: stage → S5, amount → $180k
- Initech deal: move to lost section or mark as closed-lost
- Add Globex Corp row: $95k, S2, Sarah, new entry
- Bump freshness.updated

## Multi-page debrief

Some meetings touch multiple reports. If the user says "debrief from the weekly sync" and the content spans multiple pages (ex. pipeline + hiring + product), update each page separately. Confirm the page list with the user before making changes.

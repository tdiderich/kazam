# Team Map Analysis Prompt

You have structured data from Linear at `scripts/team-map-data.json` and a flat CSV at `scripts/team-map-data.csv` mapping people to teams via issue assignments.

## Your task

Analyze the data and update the kazam YAML page at the path specified in the refresh block. Structure as:

### Per-team sections
For each team with active members, create a section with:
- Team description (from Linear or inferred from project names)
- **Team lead** if identifiable (highest issue count or project lead role)
- Table of members: Person | Focus area (inferred from projects and issue volume)
- Active projects listed

### Cross-functional people
List people who appear on 2+ teams with their team list.

### Key observations
Flag:
- **Single points of failure**: Teams with only 1 active member
- **Inactive users**: Users with no Linear activity in the lookback window (may use other tools or have non-engineering roles)
- **Overloaded people**: Anyone on 3+ teams

Keep the page structure as kazam YAML with `type: section` and `type: markdown` components. Use markdown tables for team rosters.

# Audit Fix Wish

Run `kazam audit`, fix issues automatically, and open PRs grouped by your preferred strategy.

## Quickstart

```bash
kazam audit > scripts/audit-results.json
# Then in an agent session with GitHub + messaging MCP:
# "Read scripts/audit-results.json and wishes/audit-fix/prompt.md. Fix issues and open PRs."
```

## PR strategies

- **single** - one PR with all fixes
- **per-owner** - one PR per page owner (each owner reviews their own)
- **per-page** - one PR per page (granular)

## What gets auto-fixed

- Overdue/due-soon pages: freshness date bumped (if content matches sources)
- Missing owners: inferred from git blame
- Missing sources_of_truth: mapped from repo scans

## What gets flagged for humans

- Expired pages (need archive decision)
- Pages where sources have drifted (need content review, not just a date bump)
- Empty pages (need content, not metadata)

## Notifications

Agent DMs owners via Slack or Teams (whichever MCP is available) and/or requests PR review on GitHub.

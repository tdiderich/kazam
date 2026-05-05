# Freshness Notification Prompt

You have the output of `kazam freshness notify --json` which groups stale pages by owner email.

## Your task

For each owner in the JSON:

1. Look up the owner's Slack user by their email address
2. Send them a DM with their stale pages, formatted as:
   - A greeting line
   - Each page with its status (EXPIRED/OVERDUE/DUE SOON), days count, title, and file path
   - Instructions: `kazam freshness act <path> refresh` to mark as reviewed, `kazam freshness act <path> archive` to archive

3. For "(unowned)" pages, post a summary to a team channel instead of DM

Skip owners you can't resolve to a Slack user and report them at the end.

## Example DM format

```
Hey! Some docs you own need attention:

  :warning: *[OVERDUE 45d]* Deployment Guide (`engineering/deployment-guide.yaml`)
  :hourglass_flowing_sand: *[due in 3d]* API Reference (`product/api-reference.yaml`)

To mark a page as reviewed: `kazam freshness act <path> refresh`
To archive: `kazam freshness act <path> archive`
```

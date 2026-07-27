#!/bin/bash
# kazam workspace — pre-compact hook
#
# Fires immediately before /compact (manual) and before auto-compaction.
# PreCompact stdout is not reliably injected into the compacted context, so
# this hook deliberately prints nothing. Its job is to persist state into
# kazam's stores, where session-start.sh replays it after the transcript has
# been summarized away:
#
#   1. kazam ctx scan       -> anatomy reflects reality, not session start
#   2. kazam track log add  -> durable "compact boundary" marker in log.yaml
#
# The boundary marker is what lets the post-compact hook replay only the
# activity belonging to the transcript that was discarded. No jq dependency.

set -uo pipefail

command -v kazam >/dev/null 2>&1 || exit 0
cd "$(dirname "$0")/../.." || exit 0

INPUT=$(cat 2>/dev/null || echo '{}')
TRIGGER=$(printf '%s' "$INPUT" | sed -n 's/.*"trigger"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
[ -z "$TRIGGER" ] && TRIGGER="unknown"

kazam ctx scan >/dev/null 2>&1

ACTIVE=$(kazam track list --status active --json 2>/dev/null \
  | grep -o '"id":"[^"]*"' | cut -d'"' -f4 | paste -sd, - | tr -d '[:space:]')
[ -z "$ACTIVE" ] && ACTIVE="none"

TOUCHED=$(kazam ctx scan --check --json 2>/dev/null \
  | grep -oE '"(changed|new)_files":\[[^]]*\]' | grep -o '"[^"]*\.[^"]*"' | wc -l | tr -d ' ')

kazam track log add \
  "compact boundary ($TRIGGER) | active: $ACTIVE | touched: ${TOUCHED:-0} files" \
  --source compact --severity info >/dev/null 2>&1

exit 0

#!/bin/bash
# kazam workspace — post-tool hook
#
# Runs after Read / Write / Edit. Claude Code delivers the tool payload as JSON
# on STDIN. An earlier version of this hook read a $KAZAM_TOOL_INPUT env var
# that nothing ever sets, so it was a silent no-op in every workspace.
#
#   Write | Edit -> log the modification to the activity feed
#   Read         -> append to ctx/reads.log, folded into anatomy on next scan
#
# Reads are appended rather than counted in place: appends are cheap on the hot
# path and survive parallel subagents, where a read-modify-write of
# anatomy.flat.yaml would silently lose updates.

set -uo pipefail

command -v kazam >/dev/null 2>&1 || exit 0
cd "$(dirname "$0")/../.." || exit 0

INPUT=$(cat 2>/dev/null || echo '{}')

if command -v jq >/dev/null 2>&1; then
  TOOL=$(printf '%s' "$INPUT" | jq -r '.tool_name // ""' 2>/dev/null)
  FILE=$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // ""' 2>/dev/null)
else
  TOOL=$(printf '%s' "$INPUT" | sed -n 's/.*"tool_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
  FILE=$(printf '%s' "$INPUT" | grep -o '"file_path":"[^"]*"' | head -1 | cut -d'"' -f4)
fi

[ -n "$FILE" ] || exit 0

# Normalize to a project-relative path so it matches anatomy entries.
ROOT=$(pwd -P)
case "$FILE" in "$ROOT"/*) FILE="${FILE#"$ROOT"/}" ;; esac
# Still absolute means the file lives outside the project — not our business.
case "$FILE" in /*) exit 0 ;; esac

case "$TOOL" in
  Read)
    printf '%s\t%s\n' "$FILE" "$(date +%Y-%m-%dT%H:%M:%S)" >> .kazam/ctx/reads.log
    ;;
  Write|Edit|NotebookEdit)
    kazam track log add "Modified $FILE" \
      --source "${KAZAM_AGENT:-claude-code}" --severity info >/dev/null 2>&1
    ;;
esac

exit 0

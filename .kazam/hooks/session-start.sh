#!/bin/bash
# kazam workspace — session start hook
#
# Fires on startup / resume / clear / compact. Stdout IS injected into the
# fresh context, which makes this the one reliable place to restore state that
# compaction dropped.
#
#   startup | resume | clear -> quiet orientation: anatomy drift + ready tasks
#   compact                  -> full recovery payload replayed from .kazam/
#
# The compact branch reads only from kazam's existing stores (track/log.yaml,
# track/tasks.yaml, ctx/*). No separate session-state file, so nothing can
# drift out of sync with the work graph.

set -uo pipefail

if ! command -v kazam >/dev/null 2>&1; then
  echo '{"ok":false,"error":"kazam not installed — run: cargo install --git https://github.com/tdiderich/kazam"}'
  exit 0
fi
cd "$(dirname "$0")/../.." || exit 0

INPUT=$(cat 2>/dev/null || echo '{}')
DRIFT=$(kazam ctx scan --check --json 2>/dev/null)
READY=$(kazam track ready --json 2>/dev/null)

# jq drives the compact-recovery payload. Without it, degrade to plain
# orientation rather than failing the hook.
if ! command -v jq >/dev/null 2>&1; then
  HAS_DRIFT=$(echo "$DRIFT" | grep -c '"new_files":\[\|"deleted_files":\[\|"changed_files":\[' 2>/dev/null || true)
  HAS_READY=$(echo "$READY" | grep -c '"data":\[{' 2>/dev/null || true)
  [ "$HAS_DRIFT" != "0" ] && echo "$DRIFT"
  [ "$HAS_READY" != "0" ] && echo "$READY"
  exit 0
fi

SOURCE=$(printf '%s' "$INPUT" | jq -r '.source // ""' 2>/dev/null || echo "")
N_DRIFT=$(printf '%s' "$DRIFT" | jq -r \
  '[(.data.changed_files // []), (.data.new_files // []), (.data.deleted_files // [])] | add | length' 2>/dev/null)
N_READY=$(printf '%s' "$READY" | jq -r '(.data // []) | length' 2>/dev/null)

# ---------------------------------------------------------------- normal start
if [ "$SOURCE" != "compact" ]; then
  [ "${N_DRIFT:-0}" != "0" ] && echo "$DRIFT"
  [ "${N_READY:-0}" != "0" ] && echo "$READY"
  exit 0
fi

# ----------------------------------------------------- post-compaction recovery
echo "## kazam state (post-compaction)"
echo
echo "Replayed from .kazam/ stores. Where this disagrees with the summary above,"
echo "this is correct — the summary is lossy, these files are not."
echo

ACTIVE=$(kazam track list --status active --json 2>/dev/null \
  | jq -r '(.data // [])[] | "- \(.id) [p\(.priority)] \(.title)\(if .note then "\n    note: " + (.note | .[0:220]) else "" end)"' 2>/dev/null)
if [ -n "$ACTIVE" ]; then
  echo "### Claimed / in flight"
  echo "$ACTIVE"
  echo
  echo "Resume these before starting anything new. Close with:"
  echo '  kazam track close <ID> --reason "what you did"'
else
  echo "### Claimed / in flight"
  echo "- none claimed. Claim before working: kazam track claim <ID> --name <your-name>"
fi
echo

if [ "${N_READY:-0}" != "0" ]; then
  echo "### Ready (top 5 by priority)"
  printf '%s' "$READY" | jq -r '(.data // [])[0:5][] | "- \(.id) [p\(.priority)] \(.title | .[0:150])"' 2>/dev/null
  echo
fi

# Activity belonging to the transcript that was just summarized: everything
# logged between the newest compact boundary and the one before it. File
# modifications are split out and deduped so they cannot drown the task events.
SLICE=$(kazam track log --limit 300 --json 2>/dev/null | jq -c '
  (.data // []) as $e
  | [ $e | to_entries[] | select(.value.title | startswith("compact boundary")) | .key ] as $b
  | (if ($b | length) >= 2 then $e[($b[0] + 1):$b[1]]
     elif ($b | length) == 1 then $e[($b[0] + 1):($b[0] + 61)]
     else $e[0:60] end)
' 2>/dev/null)

ACTIVITY=$(printf '%s' "$SLICE" | jq -r '
  .[]
  | select(.title | startswith("Modified ") | not)
  | "- [\(.severity)] \(.title | .[0:160])\(if .detail then "\n    " + (.detail | .[0:240]) else "" end)"
' 2>/dev/null | head -30)
if [ -n "$ACTIVITY" ]; then
  echo "### Work logged in the compacted stretch"
  echo "$ACTIVITY"
  echo
fi

TOUCHED=$(printf '%s' "$SLICE" | jq -r '
  [ .[] | select(.title | startswith("Modified ")) | .title[9:] ] | unique | .[]
' 2>/dev/null)
if [ -n "$TOUCHED" ]; then
  N_TOUCHED=$(printf '%s\n' "$TOUCHED" | wc -l | tr -d ' ')
  echo "### Files edited in the compacted stretch (${N_TOUCHED})"
  printf '%s\n' "$TOUCHED" | head -30 | sed 's/^/- /'
  echo
fi

if [ "${N_DRIFT:-0}" != "0" ]; then
  echo "### Uncommitted file drift (${N_DRIFT} files)"
  printf '%s' "$DRIFT" | jq -r '
    [(.data.new_files // [] | map("new     " + .)),
     (.data.changed_files // [] | map("changed " + .)),
     (.data.deleted_files // [] | map("deleted " + .))] | add | .[0:25][] | "- " + .' 2>/dev/null
  echo
fi

CORR=$(kazam ctx corrections --json 2>/dev/null \
  | jq -r '(.data // [])[0:5][] | "- \(.file_path // "general"): \(.mistake | .[0:130]) -> \(.correction | .[0:200])"' 2>/dev/null)
if [ -n "$CORR" ]; then
  echo "### Standing corrections (do not repeat these)"
  echo "$CORR"
  echo
fi

LEARN=$(kazam ctx learnings --json 2>/dev/null \
  | jq -r '(.data // [])[0:5][] | "- [\(.category // "note")] \(.lesson // .text // .title // "" | .[0:200])"' 2>/dev/null \
  | grep -v '^- \[.*\] $' || true)
if [ -n "$LEARN" ]; then
  echo "### Recent learnings"
  echo "$LEARN"
  echo
fi

echo "### Reminders"
echo "- Navigate via .kazam/ctx/anatomy.tsv then .kazam/ctx/anatomy/<dir>.tsv. Do not grep for structure."
echo "- Dispatch kazam-scout for multi-file exploration so file dumps stay out of this context."
echo "- Before fixing any error: kazam ctx bugs --file <path>"
echo "- Close tasks per commit, do not batch."

exit 0

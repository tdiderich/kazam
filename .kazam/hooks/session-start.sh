#!/bin/bash
# kazam workspace — session start hook
# Surfaces anatomy drift and ready tasks at the start of each agent session.
# Silent when nothing is actionable (no drift, no ready tasks).
if ! command -v kazam &>/dev/null; then
  echo '{"ok":false,"error":"kazam not installed — run: cargo install --git https://github.com/tdiderich/kazam"}'
  exit 0
fi
DRIFT=$(kazam ctx scan --check --json 2>/dev/null)
READY=$(kazam track ready --json 2>/dev/null)
HAS_DRIFT=$(echo "$DRIFT" | grep -c '"new_files":\[\|"deleted_files":\[\|"changed_files":\[' 2>/dev/null || true)
HAS_READY=$(echo "$READY" | grep -c '"data":\[{' 2>/dev/null || true)
if [ "$HAS_DRIFT" != "0" ] || [ "$HAS_READY" != "0" ]; then
  [ "$HAS_DRIFT" != "0" ] && echo "$DRIFT"
  [ "$HAS_READY" != "0" ] && echo "$READY"
fi

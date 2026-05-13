#!/bin/bash
# kazam workspace — session stop hook
# Rescans anatomy, then summarizes session activity and suggests enrichment.
kazam ctx scan 2>/dev/null
DIFF=$(kazam ctx scan --check --json 2>/dev/null)
NEW=$(echo "$DIFF" | grep -o '"new_files":\[[^]]*\]' | grep -o '"[^"]*\..*"' | wc -l 2>/dev/null | tr -d ' ')
CHANGED=$(echo "$DIFF" | grep -o '"changed_files":\[[^]]*\]' | grep -o '"[^"]*\..*"' | wc -l 2>/dev/null | tr -d ' ')
if [ "${NEW:-0}" != "0" ] || [ "${CHANGED:-0}" != "0" ]; then
  echo "kazam: session touched ${CHANGED:-0} changed + ${NEW:-0} new files"
  echo "  → enrich descriptions: kazam ctx describe <path> \"what this file does\""
  echo "  → record learnings:    kazam ctx learn \"lesson\" --category correction"
  echo "  → record bugs:         kazam ctx bug \"symptom\" --file <path>"
fi

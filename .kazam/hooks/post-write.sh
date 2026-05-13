#!/bin/bash
# kazam workspace — post-write hook
# Logs file modifications to the activity feed.
FILE="$(echo "$KAZAM_TOOL_INPUT" | grep -o '"file_path":"[^"]*"' | head -1 | cut -d'"' -f4)"
if [ -n "$FILE" ]; then
  kazam track log add "Modified $FILE" --source "${KAZAM_AGENT:-agent}" --severity info 2>/dev/null
fi

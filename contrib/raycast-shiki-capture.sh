#!/bin/bash

# Required parameters:
# @raycast.schemaVersion 1
# @raycast.title Capture to Shiki
# @raycast.mode compact
# @raycast.packageName Shiki
# @raycast.argument1 { "type": "text", "placeholder": "What to capture? Use work: prefix for notebook" }

# Optional parameters:
# @raycast.icon 📝
# @raycast.description Instant capture (<100ms via daemon if TUI is open)

text="$1"
if [[ -z "$text" ]]; then
  echo "Empty"
  exit 0
fi
# --source raycast lets daemon log provenance
result=$(printf '%s' "$text" | /opt/homebrew/bin/shiki capture --source raycast --json 2>&1)
path=$(echo "$result" | /opt/homebrew/bin/jq -r .path 2>/dev/null || echo "$result")
echo "Captured: $path"

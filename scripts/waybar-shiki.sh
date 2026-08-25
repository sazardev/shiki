#!/usr/bin/env bash
# waybar-shiki.sh — Waybar custom module for shiki
# Shows daemon status + overdue task count + click to capture
#
# Waybar config (JSONC):
#   "custom/shiki": {
#     "exec": "/path/to/shiki/scripts/waybar-shiki.sh",
#     "interval": 30,
#     "return-type": "json",
#     "on-click": "/path/to/shiki/scripts/rofi-capture.sh",
#     "on-click-right": "shiki tasks --overdue",
#     "tooltip": true
#   }
#   // then add "custom/shiki" to "modules-right"
#
# Polybar example (ini):
#   [module/shiki]
#   type = custom/script
#   exec = /path/to/shiki/scripts/waybar-shiki.sh --polybar
#   interval = 30
#   click-left = /path/to/shiki/scripts/rofi-capture.sh &
#
# Deps: shiki, jq (optional but recommended)
set -euo pipefail

POLYBAR=false
if [[ "${1:-}" == "--polybar" ]]; then POLYBAR=true; fi

DAEMON_REACHABLE=false
DAEMON_ENABLED=false
if shiki capture --check --json 2>/dev/null | grep -q '"reachable": true'; then
  DAEMON_REACHABLE=true
  if shiki capture --check --json 2>/dev/null | grep -q '"enabled": true'; then
    DAEMON_ENABLED=true
  fi
fi

# Overdue tasks (fast, local)
OVERDUE=$(shiki tasks --overdue --count 2>/dev/null || echo 0)
TODAY=$(shiki tasks --today --count 2>/dev/null || echo 0)
# Trim whitespace
OVERDUE=$(echo "$OVERDUE" | tr -d ' \n')
TODAY=$(echo "$TODAY" | tr -d ' \n')

# Icons: daemon on ●, off ○, unreachable ◌
if [[ "$DAEMON_REACHABLE" == true && "$DAEMON_ENABLED" == true ]]; then
  ICON="●"
  CLASS="daemon-on"
elif [[ "$DAEMON_REACHABLE" == true ]]; then
  ICON="◐"
  CLASS="daemon-disabled"
else
  ICON="◌"
  CLASS="daemon-off"
fi

TEXT="$ICON $OVERDUE"
if [[ "$OVERDUE" != "0" ]]; then
  CLASS="$CLASS overdue"
fi

if [[ "$POLYBAR" == true ]]; then
  # Polybar has no json return-type; use text + color via %{F}
  if [[ "$OVERDUE" != "0" ]]; then
    echo "%{F#ff5555}$ICON $OVERDUE%{F-} $TODAY today"
  else
    echo "$ICON $TODAY today"
  fi
else
  # Waybar JSON
  TOOLTIP="daemon: $DAEMON_REACHABLE/$DAEMON_ENABLED\\noverdue: $OVERDUE\\ntoday: $TODAY\\nclick: capture • right-click: tasks"
  # jq avoids manual escaping; fallback to printf if missing
  if command -v jq >/dev/null 2>&1; then
    jq -n --arg text "$TEXT" --arg tooltip "$TOOLTIP" --arg class "$CLASS" \
      '{text:$text, tooltip:$tooltip, class:$class}'
  else
    printf '{"text":"%s","tooltip":"%s","class":"%s"}\n' "$TEXT" "$TOOLTIP" "$CLASS"
  fi
fi

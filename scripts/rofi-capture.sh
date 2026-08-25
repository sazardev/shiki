#!/usr/bin/env bash
# rofi-capture.sh — instant capture for shiki via rofi/wofi/dmenu
# <100ms path when daemon is reachable (try_daemon 300ms timeout)
# Falls back to direct disk write if TUI is closed.
#
# Usage:
#   ./scripts/rofi-capture.sh               # prompt for text, captures to default notebook
#   ./scripts/rofi-capture.sh --clip        # captures clipboard instead of prompting
#   ./scripts/rofi-capture.sh --daily       # append to daily note
# Dependencies: rofi or wofi or dmenu, shiki, notify-send (optional), jq (optional)
#
# Hyprland example:
#   bind = SUPER, C, exec, /path/to/shiki/scripts/rofi-capture.sh
# Sway example:
#   bindsym $mod+c exec /path/to/shiki/scripts/rofi-capture.sh
# GNOME custom shortcut: point to this script

set -euo pipefail

MODE="prompt"
DAILY=""
NOTIFY=true

while [[ $# -gt 0 ]]; do
  case "$1" in
    --clip) MODE="clip"; shift ;;
    --daily) DAILY="--daily"; shift ;;
    --no-notify) NOTIFY=false; shift ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

# Prefer rofi -> wofi -> dmenu
PICKER=""
if command -v rofi >/dev/null 2>&1; then
  PICKER="rofi -dmenu -p capture> -theme-str window{width:50%} -i"
elif command -v wofi >/dev/null 2>&1; then
  PICKER="wofi --dmenu -p capture>"
elif command -v dmenu >/dev/null 2>&1; then
  PICKER="dmenu -p capture>"
fi

get_text() {
  if [[ "$MODE" == "clip" ]]; then
    # Use shiki's own --clip so we get the same OS clipboard logic (arboard -> OSC52 fallback)
    # We just need to confirm we have something to capture; shiki capture --clip will read it.
    # Return a sentinel so main does shiki capture --clip directly.
    echo "__CLIP__"
  else
    if [[ -z "$PICKER" ]]; then
      echo "no picker found (rofi/wofi/dmenu) and --clip not given" >&2
      exit 1
    fi
    # shellcheck disable=SC2086
    TEXT=$(echo "" | eval $PICKER || true)
    echo "$TEXT"
  fi
}

TEXT=$(get_text)

# Empty (Esc) -> cancel silently
if [[ "$TEXT" == "" ]]; then exit 0; fi

# shiki capture routing: "work: hello" -> notebook work when no -n
# We always add --source rofi/clip for logs
if [[ "$TEXT" == "__CLIP__" ]]; then
  RESULT=$(shiki capture --clip --source rofi $DAILY --json 2>&1 || true)
else
  # Single source of truth for source header is daemon; we tag as rofi
  RESULT=$(printf '%s' "$TEXT" | shiki capture --source rofi $DAILY --json 2>&1 || true)
fi

# Try to parse JSON for nice notification
if command -v jq >/dev/null 2>&1 && echo "$RESULT" | jq -e . >/dev/null 2>&1; then
  PATH_OUT=$(echo "$RESULT" | jq -r .path // empty)
  DAEMON=$(echo "$RESULT" | jq -r .daemon // empty)
  if [[ -n "$PATH_OUT" && "$NOTIFY" == true ]] && command -v notify-send >/dev/null 2>&1; then
    if [[ "$DAEMON" == "true" ]]; then
      notify-send "Shiki" "captured (daemon): $(basename "$PATH_OUT")" -t 1500
    else
      notify-send "Shiki" "captured: $(basename "$PATH_OUT")" -t 1500
    fi
  else
    echo "$RESULT"
  fi
else
  # Plain text fallback (non-json error like "locked: ...")
  if [[ "$NOTIFY" == true ]] && command -v notify-send >/dev/null 2>&1; then
    notify-send "Shiki" "$RESULT" -t 2000
  fi
  echo "$RESULT" >&2
  # Propagate non-zero for lock errors so callers can react
  if echo "$RESULT" | grep -q "^locked:"; then exit 1; fi
fi

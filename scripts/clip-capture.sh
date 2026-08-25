#!/usr/bin/env bash
# clip-capture.sh — capture clipboard or browser URL -> shiki
# For keybinds that dump clipboard, not just rofi prompt
#   Super+Shift+C -> capture clipboard as note
#   Super+Shift+V -> capture selection + URL (when invoked from browser extension helper)
set -euo pipefail

URL="${SHIKI_URL:-}"
TITLE="${SHIKI_TITLE:-}"
TAGS="${SHIKI_TAGS:-}"
NOTEBOOK="${SHIKI_NOTEBOOK:-}"
TEMPLATE="${SHIKI_TEMPLATE:-}"
DAILY="${1:-}"

ARGS=()
if [[ -n "$NOTEBOOK" ]]; then ARGS+=(-n "$NOTEBOOK"); fi
if [[ -n "$TAGS" ]]; then ARGS+=(--tags "$TAGS"); fi
if [[ -n "$TEMPLATE" ]]; then ARGS+=(--template "$TEMPLATE"); fi
if [[ -n "$URL" ]]; then ARGS+=(--url "$URL"); fi
if [[ -n "$TITLE" ]]; then ARGS+=(--title "$TITLE"); fi
if [[ "$DAILY" == "--daily" ]]; then ARGS+=(--daily); fi
ARGS+=(--source clip)

# Uses shiki's arboard-backed --clip, not xclip/xsel, so Wayland/X11/SSH all work
shiki capture --clip "${ARGS[@]}" --json | tee /tmp/shiki-last-capture.json
if command -v notify-send >/dev/null 2>&1; then
  PATH_OUT=$(jq -r .path /tmp/shiki-last-capture.json 2>/dev/null || echo "")
  if [[ -n "$PATH_OUT" ]]; then notify-send "Shiki" "captured: $(basename "$PATH_OUT")" -t 1500; fi
fi

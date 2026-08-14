#!/usr/bin/env bash
# Seeds a throwaway "demo" notebook with sample notes that exercise shiki's
# editor + PREVIEW features, so you can try them by hand instead of reading
# docs or trusting an automated tour:
#
#   1. Ctrl+D timestamp insert         (key_handlers.rs::insert_timestamp)
#   2. Outline modal live filter       (panel_outline.rs::filtered_headings)
#   3. Spell-check via hunspell Ctrl+E (shiki-core/src/spell.rs)
#   4. Images in PREVIEW via chafa     (shiki-tui/src/term_image.rs)
#
# It uses an isolated XDG environment — your real notebooks, config, and git
# repos are never touched. After seeding it prints step-by-step instructions
# and offers to launch the TUI so you can go through them yourself.
#
# Usage:
#   scripts/new-features-demo.sh [--cleanup]
#
#   --cleanup  kills nothing (no tmux involved) — just removes the seeded
#              demo data from /tmp/shiki-feature-demo.
#
# Optional tools (the demo still works without them, see the instructions):
#   - hunspell + a dictionary  → real spell-check suggestions
#   - chafa                    → real terminal-art images
#   Install chafa e.g. `sudo pacman -S chafa`, `sudo apt install chafa`, or
#   (without touching your distro's package state) `nix-env -iA nixpkgs.chafa`.
#   Both are reported by `shiki doctor` too.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/shiki"

DEMO_DIR="/tmp/shiki-feature-demo"
DEMO_CFG="$DEMO_DIR/config"
DEMO_DATA="$DEMO_DIR/data"
export XDG_CONFIG_HOME="$DEMO_CFG"
export XDG_DATA_HOME="$DEMO_DATA"

say()  { printf '\n\033[1;36m%s\033[0m\n' "$*"; }
info() { printf '   \033[0;2m%s\033[0m\n' "$*"; }

if [[ "${1:-}" == "--cleanup" ]]; then
  rm -rf "$DEMO_DIR"
  echo "removed demo data from $DEMO_DIR"
  exit 0
fi

# ---------------------------------------------------------------------------
# Build (needed for the config keys + seeding commands to exist)
# ---------------------------------------------------------------------------
say "Building shiki (debug)…"
cargo build -p shiki-cli --manifest-path "$ROOT/Cargo.toml"

# ---------------------------------------------------------------------------
# Seed the demo notebook + notes + a generated image
# ---------------------------------------------------------------------------
say "Seeding a throwaway 'demo' notebook in an isolated XDG environment"
rm -rf "$DEMO_DIR"
mkdir -p "$DEMO_DIR"

"$BIN" notebook create demo
NOTES="$XDG_DATA_HOME/shiki/demo"

# 00 — lots of headings: material for the outline filter (Ctrl+O).
cat >"$NOTES/00-outline.md" <<'EOF'
---
title: Outline demo
date: 2026-08-14
tags: [demo]
notebook: demo
---
# Welcome to shiki

## Installation
Follow the README.

## API reference
### REST endpoints
### Authentication
### Rate limiting
### Webhooks

## Configuration
### Keybindings
### Themes

## Troubleshooting
### Common errors
### Logs
EOF

# 01 — deliberate typos: material for the spell-check pass (Ctrl+E).
cat >"$NOTES/01-spellcheck.md" <<'EOF'
---
title: Spellcheck demo
date: 2026-08-14
tags: [demo]
notebook: demo
---
# Spell check demo

This note has a few misspelled words on purpose: wrrds, teh and seconnd.

Run the check with Ctrl+E — the popup lists them with hunspell suggestions,
and Enter replaces the selected one with its top suggestion.
EOF

# 02 — an empty log bullet where Ctrl+D stamps a timestamp.
cat >"$NOTES/02-log.md" <<'EOF'
---
title: Daily log
date: 2026-08-14
tags: [demo]
notebook: demo
---
# Daily log
- 
EOF

# 03 — an image on its own line: material for the PREVIEW art demo.
cat >"$NOTES/03-gallery.md" <<'EOF'
---
title: Gallery
date: 2026-08-14
tags: [demo]
notebook: demo
---
# Image gallery

Here is a generated test image:

![gradient](cat.png)

It renders as terminal art via chafa, or as icon+alt text without it.
EOF

# A small gradient PNG (pure Python stdlib, no PIL).
python3 - "$NOTES/cat.png" <<'PYEOF'
import struct, sys, zlib
w, h = 48, 32
rows = []
for y in range(h):
    row = bytearray([0])
    for x in range(w):
        row += bytes([int(255 * x / w), int(255 * y / h), 128])
    rows.append(bytes(row))
raw = b"".join(rows)
def chunk(tag, data):
    block = tag + data
    return struct.pack(">I", len(data)) + block + struct.pack(">I", zlib.crc32(block) & 0xFFFFFFFF)
png = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(raw))
       + chunk(b"IEND", b""))
with open(sys.argv[1], "wb") as f:
    f.write(png)
PYEOF

# Tune the seeded config so the demo keys behave the way the instructions say:
# timestamp with time, spell-check on, default notebook = demo, en_US dict if
# hunspell can use it.
CFG="$XDG_CONFIG_HOME/shiki/config.toml"
sed -i 's/^default_notebook = "personal"/default_notebook = "demo"/' "$CFG"
sed -i 's/^timestamp_with_time = false/timestamp_with_time = true/' "$CFG"
sed -i 's/^spellcheck = false/spellcheck = true/' "$CFG"
if hunspell -d en_US -l </dev/null >/dev/null 2>&1; then
  sed -i 's/^spellcheck_lang = ""/spellcheck_lang = "en_US"/' "$CFG"
fi

# ---------------------------------------------------------------------------
# Report + instructions
# ---------------------------------------------------------------------------
say "Demo ready"
info "Notebook 'demo' seeded with 4 notes + a test image in:"
info "   $NOTES"
info "Your real config/data were not touched (XDG overridden)."

say "Try it"
info "1. Ctrl+D timestamp — open 'Daily log', press i (edit), Down, End, Ctrl+D."
info "    A date+time is stamped into the empty bullet (timestamp_with_time is on)."
info ""
info "2. Outline live filter — open 'Outline demo', press i, then Ctrl+O."
info "    Type 'api': the heading list narrows to '## API reference'. Enter jumps there."
info ""
info "3. Spell-check — open 'Spellcheck demo', press i, then Ctrl+E."
if command -v hunspell >/dev/null; then
  info "    Misspelled words are listed with a ▸ cursor; Enter opens the suggestions"
  info "    submenu to pick the replacement, and the fixed word flashes in success green."
else
  info "    hunspell is NOT installed, so you'll see the 'not found' message."
  info "    Install it (e.g. sudo pacman -S hunspell hunspell-en_us) to see suggestions."
fi
info ""
info "4. Images in PREVIEW — open 'Gallery'."
if command -v chafa >/dev/null; then
  info "    The image renders as terminal art; resize the window and it re-renders at the new width."
else
  info "    chafa is NOT installed, so you'll see the icon+alt fallback."
  info "    Install it (e.g. sudo pacman -S chafa) to see the real art."
fi
info ""
info "Tip: run \`shiki doctor\` first — it now reports chafa/hunspell availability."

say "Launch"
info "TUI command (run it yourself, or let this script do it below):"
info "   XDG_CONFIG_HOME=$XDG_CONFIG_HOME XDG_DATA_HOME=$XDG_DATA_HOME $BIN"

printf '\nLaunch the TUI now? [y/N] '
read -r -p "" ans
if [[ "$ans" =~ ^[yY] ]]; then
  exec "$BIN"
fi
echo "not launching — data stays seeded in $DEMO_DIR (remove with: scripts/new-features-demo.sh --cleanup)"

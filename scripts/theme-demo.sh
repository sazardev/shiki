#!/usr/bin/env bash
# Seeds a throwaway, deliberately varied notebook set and enables
# `theme.name = "auto"`, so you can watch shiki re-theme itself live when the
# terminal/OS light-dark scheme changes.
#
# The TUI re-reads the terminal background over OSC 11 while idle (about
# every 1.5 s, never while you're typing), then resolves `auto_dark` /
# `auto_light` from `[theme]`. Terminals that don't answer OSC 11 simply stay
# on the dark half.
#
# Usage:
#   scripts/theme-demo.sh [--launch] [--reset] [--debug] [--cleanup]
#
#   --launch    seed (if needed) and immediately launch the TUI, no prompt
#   --reset     wipe and re-seed the demo data even if it already exists
#   --debug     build/use target/debug/shiki instead of the release binary
#   --cleanup   removes the seeded data from $SHIKI_THEME_DEMO_DIR
#
# One-command demo (what you normally want):
#   scripts/theme-demo.sh --launch
#
# The XDG dirs are isolated, so your real notebooks/config/repos are never
# touched. Override the location with SHIKI_THEME_DEMO_DIR=...
#
# Test it:
#   1. Launch it with `--launch`.
#   2. Switch the terminal between a light and a dark theme. On Omarchy:
#        omarchy theme list
#        omarchy theme set <some-light-theme>   # then a dark one
#      Or use your terminal's own theme picker (Ghostty/Kitty/Alacritty/...).
#   3. shiki should switch between gruvbox-dark and gruvbox-light within
#      ~1-2 seconds, with a "theme: ... (auto · light|dark)" status line.
#   4. Press `space c` for the theme picker: the "auto" row is the live mode;
#      pick any concrete theme to pin it again.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEMO_DIR="${SHIKI_THEME_DEMO_DIR:-/tmp/shiki-theme-demo}"
PROFILE="release"
LAUNCH=0
RESET=0
for arg in "$@"; do
  case "$arg" in
    --cleanup)
      rm -rf "$DEMO_DIR"
      echo "removed demo data from $DEMO_DIR"
      exit 0
      ;;
    --launch) LAUNCH=1 ;;
    --reset) RESET=1 ;;
    --debug) PROFILE="debug" ;;
    --release) PROFILE="release" ;;
    --help | -h)
      sed -n '11,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "usage: $0 [--launch] [--reset] [--debug] [--cleanup]" >&2
      exit 2
      ;;
  esac
done
BIN="$ROOT/target/$PROFILE/shiki"

say()  { printf '\n\033[1;36m%s\033[0m\n' "$*"; }
info() { printf '   \033[0;2m%s\033[0m\n' "$*"; }

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
say "Building shiki ($PROFILE)…"
if [[ "$PROFILE" == "release" ]]; then
  cargo build -p shiki-cli --release --manifest-path "$ROOT/Cargo.toml"
else
  cargo build -p shiki-cli --manifest-path "$ROOT/Cargo.toml"
fi

# ---------------------------------------------------------------------------
# Seed isolated data (reused across launches unless --reset is given, so
# notes you write while testing survive a re-run)
# ---------------------------------------------------------------------------
export XDG_CONFIG_HOME="$DEMO_DIR/config"
export XDG_DATA_HOME="$DEMO_DIR/data"
CFG="$XDG_CONFIG_HOME/shiki/config.toml"

if [[ "$RESET" -eq 1 || ! -f "$CFG" ]]; then
say "Seeding varied fake notebooks in an isolated XDG environment"
rm -rf "$DEMO_DIR"
mkdir -p "$DEMO_DIR"

for nb in personal work ideas; do
  "$BIN" notebook create "$nb" >/dev/null
done

PERSONAL="$XDG_DATA_HOME/shiki/personal"
WORK="$XDG_DATA_HOME/shiki/work"
IDEAS="$XDG_DATA_HOME/shiki/ideas"
mkdir -p "$PERSONAL/journal" "$PERSONAL/recipes" "$PERSONAL/ideas" "$WORK/meetings" "$WORK/projects" "$WORK/archive" "$IDEAS/reading"

cat >"$PERSONAL/welcome.md" <<'EOF'
---
title: Welcome to shiki
date: 2026-11-07
tags: [personal, getting-started]
notebook: personal
---
# Welcome

This notebook was seeded for the **live system-theme demo**. Everything here
is fake, disposable data — your real vault was not touched.

## What to look at

- The theme should follow the terminal's light/dark scheme.
- Code fences, tags, task checkboxes, tables and wikilinks all get themed.
- `space c` opens the theme picker; `auto` is the row that follows the system.

```rust
fn main() {
    println!("hello from a themed code fence");
}
```

> Tip: switch the terminal from a dark theme to a light one and keep your
> eyes on the borders, selection band and status bar.

EOF

cat >"$PERSONAL/journal/2026-11-07.md" <<'EOF'
---
title: Saturday log
date: 2026-11-07
tags: [journal, personal]
notebook: personal
---
# Saturday log

- [ ] Try the light theme, then the dark one again @due(today) @repeat(day)
- [x] Seed the fake notebooks
- [ ] Tune `auto_dark` / `auto_light` in `config.toml`
- [ ] Write a note about [[Theme engine]]

## Notes

The garden needs water. Also: replace the kitchen bulb.

EOF

cat >"$PERSONAL/journal/2026-11-06.md" <<'EOF'
---
title: Friday log
date: 2026-11-06
tags: [journal, personal]
notebook: personal
---
# Friday log

- [x] Groceries
- [x] Call the dentist
- [ ] Plan the weekend
- [ ] Finish [[Ramen]] research

Mood: focused, slightly caffeinated.

EOF

cat >"$PERSONAL/reading-list.md" <<'EOF'
---
title: Reading list
date: 2026-11-04
tags: [reading, personal]
notebook: personal
status: active
priority: low
---
# Reading list

| Book | Author | Status |
|------|--------|--------|
| The Design of Everyday Things | Don Norman | reading |
| A Philosophy of Software Design | John Ousterhout | queued |
| The Art of Doing Science and Engineering | Richard Hamming | queued |

## Next up

- [ ] Finish chapter 4 @due(+3d)
- [ ] Take notes in [[Garden ideas]]

EOF

cat >"$PERSONAL/recipes/ramen.md" <<'EOF'
---
title: Ramen
date: 2026-11-03
tags: [recipe, food]
notebook: personal
servings: 2
---
# Weeknight ramen

## Ingredients

- 2 portions fresh noodles
- 1 litre chicken stock
- 2 eggs
- spring onion, soy, miso

## Method

1. Soft-boil the eggs (6:30), chill and peel.
2. Warm the stock, whisk in miso.
3. Cook the noodles, assemble, top with the egg.

```sh
# the only command this recipe needs
echo "eat while hot"
```

EOF

cat >"$PERSONAL/ideas/garden.md" <<'EOF'
---
title: Garden ideas
date: 2026-10-29
tags: [garden, projects]
notebook: personal
aliases: [Balcony garden]
---
# Garden ideas

- [ ] Herbs on the balcony: basil, mint, thyme @due(+2w)
- [ ] Build a small trellis @due(+1m)
- [ ] Read about [[Reading list]] while the soil settles

Linked from [[Saturday log]] and [[Theme engine]].
EOF

cat >"$WORK/roadmap-q4.md" <<'EOF'
---
title: Q4 roadmap
date: 2026-11-01
tags: [work, planning, roadmap]
notebook: work
status: in-progress
priority: high
project: atlas
---
# Q4 roadmap

## Goals

1. Ship the migration
2. Cut flaky tests by half
3. Document the public API

| Workstream | Owner | Confidence |
|------------|-------|------------|
| API migration | me | medium |
| Theme engine | me | high |
| Docs | team | low |

## Tasks

- [ ] Draft the migration checklist @due(+2d)
- [ ] Review [[2026-11-05 standup]] @due(tomorrow)
- [x] Book the planning room
- [ ] Update the [[API migration]] page

EOF

cat >"$WORK/meetings/2026-11-05-standup.md" <<'EOF'
---
title: 2026-11-05 standup
date: 2026-11-05
tags: [work, meeting]
notebook: work
attendees: [ana, luis, me]
---
# Standup — 5 Nov

## Yesterday

- Shipped the theme auto-resolution prototype
- Fixed the CI cache

## Today

- [ ] Wire the live terminal-background watch @due(today)
- [ ] Review [[Q4 roadmap]]
- [ ] Pair on the migration

## Blockers

None right now.

EOF

cat >"$WORK/projects/shiki-themes.md" <<'EOF'
---
title: Theme engine
date: 2026-10-30
tags: [work, project, shiki]
notebook: work
status: active
priority: high
project: shiki
---
# Theme engine

Goal: follow the terminal's light/dark scheme without a restart.

## Design

- Query OSC 11 for the background colour
- Resolve `auto_dark` / `auto_light`
- Re-query only while idle, so keystrokes are never stolen

```toml
[theme]
name = "auto"
auto_dark = "gruvbox-dark"
auto_light = "gruvbox-light"
```

## Open questions

- [ ] Should `auto` also follow the ANSI palette itself? @due(+1w)
- [x] Add a picker row
- [ ] Document it in the manual

EOF

cat >"$WORK/projects/api-migration.md" <<'EOF'
---
title: API migration
date: 2026-10-24
tags: [work, project, api]
notebook: work
status: blocked
priority: critical
project: atlas
---
# API migration

## Current state

The old client still calls `/v1`. The new gateway speaks `/v2` only.

```python
def migrate(payload):
    payload["version"] = 2
    return payload
```

## Checklist

- [x] Inventory call sites
- [ ] Add the compatibility shim @due(today)
- [ ] Remove `/v1` after the freeze @due(+3w)
- [ ] Update [[Q4 roadmap]]

EOF

cat >"$WORK/archive/2026-10-retro.md" <<'EOF'
---
title: October retro
date: 2026-10-31
tags: [work, retro, archive]
notebook: work
---
# October retro

## Went well

- [x] Theme work stayed on schedule
- [x] Fewer production alerts

## To improve

- [ ] Smaller pull requests
- [ ] Write decisions down sooner

EOF

cat >"$IDEAS/theme-engine.md" <<'EOF'
---
title: Theme ideas
date: 2026-10-28
tags: [ideas, design]
notebook: ideas
status: exploring
---
# Theme ideas

## Follow the system

`name = "auto"` is the switch. The interesting part is doing it *live*:

```rust
// sketch
if let Some((_, bg)) = term_colors::query_fg_bg_with_timeout(35.ms()) {
    app.set_terminal_colors(Some((fg, bg)));
}
```

## Wild ideas

- [ ] Per-notebook light/dark pairs
- [ ] Blend the terminal's ANSI palette into a generated theme @due(+2m)
- [ ] Preview both halves side by side
- [ ] A `theme doctor` that prints the detected background

See also [[Garden ideas]] and [[Q4 roadmap]].

EOF

cat >"$IDEAS/reading/books.md" <<'EOF'
---
title: Books to read
date: 2026-10-20
tags: [ideas, reading]
notebook: ideas
---
# Books to read

| Title | Why |
|-------|-----|
| Thinking in Systems | mental models |
| The Timeless Way of Building | design language |
| Crafting Interpreters | fun |

- [ ] Pick one for December @due(dec 1)
- [ ] Add notes to [[Theme ideas]]

EOF

cat >"$IDEAS/someday.md" <<'EOF'
---
title: Someday
date: 2026-10-18
tags: [ideas, backlog]
notebook: ideas
---
# Someday / maybe

- [ ] Build a tiny terminal dashboard @due(+6m)
- [ ] Write a `shiki` plugin for the browser extension
- [ ] Try the `default` theme in a light terminal
- [ ] Revisit [[Theme ideas]]

EOF

# A small gradient PNG for the PREVIEW image/attachment path.
python3 - "$PERSONAL/recipes/gradient.png" <<'PYEOF'
import struct, sys, zlib
w, h = 64, 40
rows = []
for y in range(h):
    row = bytearray([0])
    for x in range(w):
        row += bytes([int(255 * x / w), int(255 * y / h), 160])
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
cat >>"$PERSONAL/recipes/ramen.md" <<'EOF'

![gradient](gradient.png)
EOF

# Commit part of the work notebook so the git colours have both clean and
# dirty files to show; the rest stays untracked on purpose.
if command -v git >/dev/null 2>&1; then
  for nb in personal work ideas; do
    dir="$XDG_DATA_HOME/shiki/$nb"
    git -C "$dir" config user.email "demo@shiki.dev" >/dev/null 2>&1 || true
    git -C "$dir" config user.name "shiki demo" >/dev/null 2>&1 || true
  done
  git -C "$WORK" add roadmap-q4.md archive/2026-10-retro.md >/dev/null 2>&1 || true
  git -C "$WORK" commit -q -m "seed roadmap and retro" >/dev/null 2>&1 || true
  git -C "$PERSONAL" add welcome.md >/dev/null 2>&1 || true
  git -C "$PERSONAL" commit -q -m "seed welcome note" >/dev/null 2>&1 || true
fi

# ---------------------------------------------------------------------------
# Point the seeded config at the auto theme + a comfortable demo setup
# ---------------------------------------------------------------------------
sed -i 's/^default_notebook = "personal"/default_notebook = "personal"/' "$CFG"
sed -i 's/^show_dates = false/show_dates = true/' "$CFG"
sed -i 's/^show_borders = true/show_borders = true/' "$CFG"
sed -i 's/^status_message_timeout_secs = [0-9]*/status_message_timeout_secs = 8/' "$CFG"
# `name = "gruvbox-dark"` is the generated default line; replace it with the
# live auto mode and pin its two halves (the generated file already carries
# those two keys at their defaults, so replace rather than append).
sed -i 's/^name = "gruvbox-dark"$/name = "auto"/' "$CFG"
sed -i 's/^auto_dark = .*/auto_dark = "gruvbox-dark"/' "$CFG"
sed -i 's/^auto_light = .*/auto_light = "gruvbox-light"/' "$CFG"
else
say "Reusing existing demo data"
info "   $DEMO_DIR  (pass --reset to wipe and re-seed)"
fi

# ---------------------------------------------------------------------------
# Report + instructions
# ---------------------------------------------------------------------------
say "Demo ready"
info "Notebooks: personal (6 notes), work (5), ideas (3) — folders, tags,"
info "tasks with due/repeat, tables, code fences, wikilinks, custom metadata,"
info "and a generated image. Data lives in:"
info "   $XDG_DATA_HOME/shiki"
info "Config (theme.name = \"auto\"):"
info "   $CFG"
info "Your real config/data were not touched (XDG overridden)."

if [[ "$LAUNCH" -eq 1 ]]; then
  say "Launching the live-theme demo"
  info "Cambia el tema claro/oscuro de la terminal: shiki se adapta en 1-2 s."
  if command -v omarchy >/dev/null 2>&1; then
    info "   omarchy theme set \"Catppuccin Latte\"   # claro"
    info "   omarchy theme set Aether               # oscuro"
    info "   (en otra terminal/split, para verlo solo:)"
    info "   for t in 'Catppuccin Latte' Aether; do omarchy theme set \"\$t\"; sleep 5; done"
  fi
  info "space c = theme picker (fila auto) · q = salir"
  exec "$BIN"
fi

say "Try the live system theme"
info "1. Launch the TUI:"
info "   XDG_CONFIG_HOME=$XDG_CONFIG_HOME XDG_DATA_HOME=$XDG_DATA_HOME $BIN"
info ""
info "2. Change the terminal's light/dark scheme while shiki is open."
if command -v omarchy >/dev/null 2>&1; then
  info "   On Omarchy:  omarchy theme list"
  info "                omarchy theme set <light-theme>"
  info "                omarchy theme set <dark-theme>"
else
  info "   Use your terminal's own theme picker (Ghostty, Kitty, Alacritty…)."
fi
info ""
info "   shiki re-reads OSC 11 while idle (~1.5 s) and switches between"
info "   gruvbox-dark and gruvbox-light. A status line confirms the switch,"
info "   e.g. 'theme: gruvbox-light (auto · light)'."
info ""
info "3. Press 'space c' to open the theme picker. The System group starts"
info "   with 'auto' (live), followed by 'default' (terminal-coloured)."
info "   Picking a concrete theme pins it; picking 'auto' unpins it again."
info ""
info "4. 'shiki theme list' / 'shiki theme set auto' work too."

say "Launch"
printf '\nLaunch the TUI now? [y/N] '
read -r -p "" ans
if [[ "$ans" =~ ^[yY] ]]; then
  exec "$BIN"
fi
echo "not launching — data stays seeded in $DEMO_DIR"
echo "remove it with: SHIKI_THEME_DEMO_DIR=$DEMO_DIR $0 --cleanup"
echo "or just launch next time with: $0 --launch"

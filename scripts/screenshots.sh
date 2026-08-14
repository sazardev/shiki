#!/usr/bin/env bash
# Generates marketing screenshots of the shiki TUI: every built-in theme, at
# every responsive layout tier (columned/stacked/single), covering the main
# screens and modals — real terminal rendering (xterm under Xvfb), not a
# mockup.
#
# Usage: scripts/screenshots.sh [output-dir]
#   Defaults to screenshots/ at the repo root. Wipes and regenerates it.
#
# Requires (local dev machine only — this is not part of any CI/release
# pipeline, deliberately): xterm, imagemagick (for import/identify), xdotool,
# Xvfb, and a Nerd Font (for the UI's icons — this script reuses whichever
# one is already installed via `fc-list`, falling back to plain "monospace"
# if none is found, which will render icons as boxes).
#
#   Arch/CachyOS:   sudo pacman -S xterm imagemagick xdotool xorg-server-xvfb
#   Debian/Ubuntu:  sudo apt install xterm imagemagick xdotool xvfb
#
# Runs its own virtual display (Xvfb) rather than reusing whatever $DISPLAY
# is already set — real desktop/WSLg compositors can leave window content
# unreadable via XGetImage (hit this exact issue building the script:
# BadMatch on X_GetImage against a WSLg-hosted window, immune under a plain
# Xvfb framebuffer since there's no RDP-forwarding layer in the way) — so
# this doesn't depend on, or interfere with, any real display you have.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/screenshots}"
WORK="$(mktemp -d)"
XVFB_DISPLAY=":97"

cleanup() {
  pkill -9 -f "Xvfb $XVFB_DISPLAY" 2>/dev/null || true
  pkill -9 -f "shiki-ss-" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

for tool in xterm import xdotool Xvfb; do
  command -v "$tool" >/dev/null || {
    echo "error: $tool not found on \$PATH — see the header of this script for what's needed." >&2
    exit 1
  }
done

# A Nerd Font is required for the UI's icons to render as glyphs instead of
# boxes/mojibake — reuse whatever's already installed rather than assuming
# a specific one. `fc-list` is captured into a variable *before* grep ever
# runs, rather than piped straight into it — a previous version of this
# line (`fc-list | grep -im1 ... | head -1`) sent `head` closing the pipe
# early a SIGPIPE under `set -o pipefail`, which was "fixed" by switching to
# `grep -m1` (it stops reading after its own first match instead of relying
# on `head` to do it) — except `grep -m1` closing the pipe early is exactly
# as capable of SIGPIPE'ing `fc-list` itself once there are enough matching
# lines, which is a real, reproducible failure on a machine with a large
# installed Nerd Font family (confirmed: `fc-list | grep -im1 ...` alone
# exits 141 here). Capturing to a variable first means `fc-list` always
# runs to completion on its own terms, with nothing downstream that can
# close its pipe early — the actual fix, not just a different flavor of
# the same race.
FC_LIST_OUTPUT="$(fc-list)"
NERD_FONT="$(grep -im1 "nerd font mono" <<< "$FC_LIST_OUTPUT" | cut -d: -f2 | sed 's/^ *//' | cut -d, -f1)"
NERD_FONT="${NERD_FONT:-monospace}"

BIN="$ROOT/target/release/shiki"
# Always run through cargo rather than just checking `-x "$BIN"` — a stale
# release binary from an earlier version would otherwise be reused as-is
# (cargo's own up-to-date check makes this a fast no-op when nothing changed).
echo "Building release binary..."
cargo build --release -p shiki-cli --manifest-path "$ROOT/Cargo.toml"

echo "Starting Xvfb on $XVFB_DISPLAY..."
Xvfb "$XVFB_DISPLAY" -screen 0 1920x1080x24 >"$WORK/xvfb.log" 2>&1 &
sleep 1
export DISPLAY="$XVFB_DISPLAY"

THEMES=(
  catppuccin-mocha catppuccin-macchiato catppuccin-frappe catppuccin-latte
  tokyo-night-storm tokyo-night tokyo-night-moon
  gruvbox-dark gruvbox-light
  nord
  solarized-dark solarized-light
  dracula one-dark monokai
  "LoL (Jinx)" "LoL (Teemo)" "LoL (Ahri)"
  "Pokémon (Pikachu)" "Pokémon (Charizard)" "Pokémon (Gengar)"
  Zelda Portal "Super Mario" "Super Mario (Luigi)" Overwatch Halo "Stardew Valley"
  default
)

# xterm reserves a small `internalBorder` margin around the text grid that
# the running program never draws into — it's filled with xterm's own `-bg`
# resource, which defaults to white. Without setting `-bg` explicitly, every
# capture showed a thin white border around an otherwise fully-themed
# screenshot, no matter what theme shiki itself was rendering. Extracted
# straight from each theme's own Rust source rather than hardcoded here
# twice, so it can't drift if a theme's palette changes. "default" (the
# terminal-inheriting theme) has no fixed hex — falls back to plain black,
# a reasonable generic terminal background for that one capture.
theme_bg() {
  local theme="$1" hex
  # `index()` is a literal substring search rather than a regex match: theme
  # names like "LoL (Jinx)" contain regex-special characters (parens) that a
  # `~` pattern would mangle, and this file's capture loop already uses the
  # same space/paren-bearing names for directory names and xterm titles.
  hex="$(awk -v name="\"$theme\"" '
    index($0, "name: " name) { found=1; next }
    found { if (match($0, /#[0-9a-fA-F]{6}/)) print substr($0, RSTART, RLENGTH); exit }
  ' "$ROOT"/shiki-config/src/themes/*.rs 2>/dev/null)"
  echo "${hex:-#000000}"
}

rm -rf "$OUT"
mkdir -p "$OUT"

# --- Sample data: a couple of notebooks with real-looking notes, committed
# so the footer shows a clean synced state instead of a distracting dirty
# marker. Shared across every theme/size run below (the app doesn't mutate
# it materially aside from one daily-note capture near the end of the full
# depth pass, which is fine to repeat/overwrite per theme).
DATA="$WORK/data/shiki"
mkdir -p "$DATA/personal" "$DATA/work"

write_note() {
  local nb="$1" file="$2" title="$3" date="$4" tags="$5" body="$6"
  cat >"$DATA/$nb/$file" <<EOF
---
title: $title
date: $date
tags: $tags
notebook: $nb
links: []
template: null
---

$body
EOF
}

setup_sample_data() {
  for nb in personal work; do
    git -C "$DATA/$nb" init -q
    git -C "$DATA/$nb" config user.email "demo@shiki.dev"
    git -C "$DATA/$nb" config user.name "shiki demo"
  done

  # Computed relative to the actual capture date, not hardcoded, so the
  # global tasks panel screenshot (below) always shows one real overdue,
  # one due-today, and one future task — the three urgency colors — no
  # matter when this script is actually run.
  local due_overdue due_today due_future
  due_overdue="$(date -d '-4 days' +%F 2>/dev/null || date -v-4d +%F)"
  due_today="$(date +%F)"
  due_future="$(date -d '+5 days' +%F 2>/dev/null || date -v+5d +%F)"

  write_note personal "recipe-homemade-pasta.md" "Recipe: homemade pasta" "2026-07-18" "[cooking, recipes]" \
"## Ingredients

- 400g 00 flour
- 4 large eggs
- pinch of salt
- olive oil

## Method

1. Mound the flour on a clean surface, make a well in the center.
2. Crack the eggs into the well, add salt and a splash of olive oil.
3. Whisk the eggs gradually incorporating flour from the inner rim.
4. Knead for 10 minutes until smooth and elastic.
5. Rest wrapped in plastic for 30 minutes before rolling.

Best paired with a simple brown butter and sage sauce. See also [[Book recommendations]] for
something to read while the dough rests."

  write_note personal "book-recommendations.md" "Book recommendations" "2026-07-15" "[reading, books]" \
"## Currently reading

- *The Pragmatic Programmer* — Hunt & Thomas
- *Project Hail Mary* — Andy Weir

## Queue

- *A Philosophy of Software Design* — John Ousterhout
- *The Left Hand of Darkness* — Ursula K. Le Guin

Recommended by a friend: anything by Ted Chiang, starting with *Exhalation*.

Want to bring one along on the Weekend hiking trip — something short enough to finish by the
campfire."

  write_note personal "weekend-hiking-trip.md" "Weekend hiking trip" "2026-07-10" "[outdoors, planning]" \
"Planning a two-day trip along the ridge trail.

- Trailhead parking fills up early, arrive before 7am
- Pack layers, weather can flip fast above 2000m
- Water refill point at the halfway hut
- Reserve the shelter two weeks in advance"

  write_note work "meeting-notes-q3-planning.md" "Meeting notes: Q3 planning" "2026-07-20" "[work, planning]" \
"## Attendees

Product, Engineering, Design leads.

## Decisions

- Ship the notebook tree view before the Q3 review
- Push the mobile companion app to Q4
- Design to finalize the onboarding flow by end of month

## Action items

- [ ] Circulate the roadmap doc for async feedback @due($due_overdue)
- [ ] Schedule design review for the onboarding flow @due($due_today)
- [ ] Follow up with infra team on the migration timeline @due($due_future)"

  write_note work "onboarding-checklist.md" "Onboarding checklist" "2026-07-12" "[work, hr]" \
"## First week

1. Laptop + accounts provisioned
2. Repo access granted, clone the monorepo
3. Pair with a buddy on a starter ticket
4. Read the architecture overview doc

## First month

- Ship a small, well-scoped fix end to end
- Meet 1:1 with your manager and skip-level
- Present at a team demo"

  # Deliberately empty body — the /-menu screenshot (below) needs a note
  # with nothing in it, so the very first keystroke lands at column 0 of an
  # empty line and reliably triggers the menu, instead of scripting cursor
  # movement into the middle of a real note's content.
  write_note personal "quick-capture.md" "Quick capture" "2026-07-20" "[]" ""

  # Three lines all starting with the exact same word, deliberately — the
  # multi-cursor screenshots (below) drive this with repeated Ctrl+D
  # presses (select word under cursor, then keep adding the next
  # occurrence), which only needs the cursor to start at (0, 0) — the
  # default position on entering edit mode — and never needs pixel-precise
  # mouse coordinates or counted arrow-key navigation the way an Alt+Click
  # or Ctrl+Alt+Down demo would.
  write_note personal "errands-this-week.md" "Errands this week" "2026-07-21" "[personal, todo]" \
"TODO: buy milk
TODO: call the dentist
TODO: finish the quarterly report"

  write_note personal "retry-helper-snippet.md" "Retry helper snippet" "2026-07-19" "[rust, ideas]" \
"A small generic retry wrapper I keep copy-pasting between projects — should probably become its
own crate at some point.

\`\`\`rust
pub fn retry<T, E>(attempts: u32, mut f: impl FnMut() -> Result<T, E>) -> Result<T, E> {
    let mut last_err = None;
    for attempt in 0..attempts {
        match f() {
            Ok(value) => return Ok(value),
            Err(err) => {
                last_err = Some(err);
                std::thread::sleep(std::time::Duration::from_millis(200 * (attempt + 1) as u64));
            }
        }
    }
    Err(last_err.unwrap())
}
\`\`\`

Linear backoff is fine for now — exponential would be better under real load."

  # The 0.9.1 PREVIEW-rendering showcase: a note carrying every construct the
  # renderer gained recently, so shot 25 shows them all at once — a code fence
  # with a `file:` header token + line-number gutter, a collapsible
  # <details>/<summary> block, a prettified $$math$$ block, and a mermaid
  # flowchart. Escaping: `\$` and backticks stay literal inside the heredoc
  # (same as the retry note above).
  write_note personal "release-day-notes.md" "Release day notes" "2026-07-22" "[rust, meta]" \
"## Feature rollup

\`\`\`tsx file:App.tsx
const App = () => {
  return <h1>{title}</h1>;
};
\`\`\`

<details>
<summary>Why TypeScript now highlights</summary>

\`two-face\` bundles TSX (plus ~150 more languages) on top of syntect's defaults,
so this fence is colored instead of flat dimmed text.
</details>

## Math

\$\$\int_0^\infty e^{-x^2} dx = \\frac{\\sqrt{\\pi}}{2}\$\$

## Diagram

\`\`\`mermaid
graph TD
    A[Ideas] --> B{Enough?}
    B -->|yes| C[Write note]
    B -->|no| D[Keep reading]
\`\`\`"

  for nb in personal work; do
    git -C "$DATA/$nb" add -A
    git -C "$DATA/$nb" commit -q -m "shiki: initial notes"
  done
}

setup_sample_data

CONFIG_BASE="$WORK/config-base/shiki"
mkdir -p "$CONFIG_BASE"
XDG_CONFIG_HOME="$WORK/config-base" XDG_DATA_HOME="$DATA/.." timeout 2 "$BIN" config >/dev/null 2>&1 || true

# `line_numbers`/`multi_cursor` default off (see `[editor]` in
# shiki-config) — flipped on here, once, in the base config every theme's
# own copy is `sed`'d from below, so the editor screenshots show these
# features active rather than screenshotting the (invisible-by-design)
# off state. `mouse_selection`/`find_replace` already default to `true`,
# so nothing to flip for those.
sed -i 's/^line_numbers = false/line_numbers = true/' "$CONFIG_BASE/config.toml"
sed -i 's/^multi_cursor = false/multi_cursor = true/' "$CONFIG_BASE/config.toml"

# --- One xterm session per (theme, size). xdotool sends keys/text directly
# to the window by id (a plain windowfocus first, since Xvfb has no window
# manager to mediate `_NET_ACTIVE_WINDOW`-based activation).
capture() {
  local theme="$1" size_dir="$2" cols="$3" rows="$4"
  local cfg_dir="$WORK/config-$theme"
  mkdir -p "$cfg_dir/shiki"
  sed "s/^name = .*/name = \"$theme\"/" "$CONFIG_BASE/config.toml" >"$cfg_dir/shiki/config.toml"

  # Unique per capture, not a shared literal — a leftover window from a
  # previous capture (still closing down) matching a shared title could
  # otherwise be picked up instead of this run's actual window.
  local title="shiki-ss-$theme-$size_dir"
  local bg
  bg="$(theme_bg "$theme")"
  # `-xrm` zeroes out xterm's own internal/outer border entirely (rather
  # than just recoloring it to match) — the cleanest fix, since it removes
  # the artifact instead of masking it; `-bg` is still set as a safety net
  # for any edge pixel the border removal doesn't reach.
  LANG=C.utf8 LC_ALL=C.utf8 xterm -u8 -fa "$NERD_FONT" -fs 13 -bg "$bg" \
    -xrm "XTerm*internalBorder: 0" -xrm "XTerm*borderWidth: 0" \
    -geometry "${cols}x${rows}" -title "$title" \
    -e env XDG_CONFIG_HOME="$cfg_dir" XDG_DATA_HOME="$DATA/.." LANG=C.utf8 LC_ALL=C.utf8 "$BIN" \
    >/dev/null 2>&1 &
  local xterm_pid=$!
  sleep 1.2

  local win=""
  for _ in $(seq 1 30); do
    win="$(xdotool search --name "$title" 2>/dev/null | head -1)"
    [ -n "$win" ] && break
    sleep 0.2
  done
  if [ -z "$win" ]; then
    echo "warning: xterm window never appeared for $theme/$size_dir, skipping" >&2
    kill -9 "$xterm_pid" 2>/dev/null || true
    return
  fi
  xdotool windowfocus "$win"
  sleep 0.2

  send_text() { xdotool windowfocus "$win"; xdotool type --window "$win" --clearmodifiers -- "$1"; }
  send_key() { xdotool windowfocus "$win"; xdotool key --window "$win" --clearmodifiers "$1"; }
  shot() {
    mkdir -p "$OUT/$theme"
    sleep 0.35
    import -window "$win" "$OUT/$theme/$size_dir-$1.png"
  }

  if [ "$size_dir" = "wide" ]; then
    shot "01-notebooks"
    send_text "l"
    shot "02-notes"
    send_text "l"
    shot "03-preview"
    send_text "?"
    shot "04-which-key"
    send_key "Escape"
    send_text " c"
    shot "05-theme-picker"
    send_key "Escape"
    send_text " g"
    send_text "recipe"
    shot "06-global-search"
    send_key "Escape"
    send_text " T"
    shot "07-tags-panel"
    send_key "Escape"
    send_text "hD"
    shot "08-toggle-dates"
    send_text " l"
    shot "09-logs"
    send_key "Escape"
    send_text "T"
    shot "10-tree-view"
    send_key "Escape"
    send_text "lH"
    shot "11-history"
    send_key "Escape"
    send_text " U"
    sleep 1.5
    shot "12-check-update"
    send_key "Escape"
    send_text " b"
    shot "13-drawer"
    send_key "Escape"
    send_text " s"
    shot "14-settings"
    send_key "Escape"
    # /-menu: jump to the dedicated empty note (see setup_sample_data) by
    # title rather than by list position, so this doesn't depend on
    # exactly where it happens to sort among the other notes. `hhh` first
    # resets focus all the way back to NOTEBOOKS regardless of whatever
    # panel the steps above left focus on, since backward() is a no-op
    # once already there — cheap insurance against relying on incidental
    # leftover focus state.
    send_text "hhhl"
    send_text "/"
    send_text "quick"
    send_key "Return"
    send_text "i"
    send_text "/"
    shot "15-slash-menu"
    # Removes the "/" first, then does the same for `[[` (wikilink
    # autocomplete) on the exact same empty note — cheaper than jumping
    # elsewhere, and the note still needs to come back out empty afterward
    # either way.
    send_key "BackSpace"
    send_text "[["
    shot "21-wikilink-autocomplete"
    # Esc closes the menu but leaves the typed "[[" in the buffer (same
    # convention as the slash-menu above) — 2 BackSpaces restore the note
    # to exactly as empty as it went in, since setup_sample_data only runs
    # once and this same file is reused by every theme/tier capture that
    # follows.
    send_key "Escape"
    send_key "BackSpace"
    send_key "BackSpace"
    send_key "Escape"
    send_key "Escape"

    # EDITOR settings tab — GENERAL is the section a fresh leader+s always
    # opens on, so `Right` x3 (GENERAL -> THEME -> GIT -> EDITOR) reaches it
    # the same way a real user would, rather than assuming its index.
    send_text " s"
    send_key "Right"
    send_key "Right"
    send_key "Right"
    shot "16-settings-editor"
    send_key "Escape"

    # Multi-cursor + find/replace, on the dedicated errands-this-week note
    # (see setup_sample_data) — jumped to by title, same pattern as the
    # slash-menu note above.
    send_text "hhhl"
    send_text "/"
    send_text "errands"
    send_key "Return"
    send_text "i"
    shot "17-editor-line-numbers"
    # Ctrl+D 3x: 1st press selects the word under the cursor (no new
    # cursor yet), each press after that adds the next occurrence as its
    # own cursor — three identical "TODO"s means three presses select all
    # of them, with no pixel-coordinate mouse math or counted arrow-key
    # navigation needed at all.
    send_key "ctrl+d"
    send_key "ctrl+d"
    send_key "ctrl+d"
    shot "18-editor-multicursor-select"
    send_text "DONE"
    shot "19-editor-multicursor-typed"
    # `Escape` first collapses the secondary cursors (this app's own
    # convention) rather than leaving their now-stale positions/selections
    # around for the find/replace shot below. 4 Ctrl+U (one per typed
    # character, each grouped atomically across all 3 cursors — see
    # `editor_undo`'s own doc comment) restores "TODO" exactly, since this
    # note is shared by every theme/tier capture that follows.
    send_key "Escape"
    send_key "ctrl+u"
    send_key "ctrl+u"
    send_key "ctrl+u"
    send_key "ctrl+u"
    send_key "ctrl+f"
    send_text "milk"
    shot "20-editor-find-replace"
    send_key "Escape"
    send_key "Escape"

    # Global tasks view (leader+t) — works from any focus, no navigation
    # needed first; the overdue/today/future @due tags on
    # meeting-notes-q3-planning.md's action items (see setup_sample_data)
    # show all three urgency colors at once.
    send_text " t"
    shot "22-tasks-panel"
    send_key "Escape"

    # Links modal on "Weekend hiking trip" — book-recommendations.md
    # mentions it in plain text without a real [[wikilink]] (see
    # setup_sample_data), so its Mentions (unlinked) section is non-empty.
    # `hhhl` resets focus to NOTEBOOKS then into NOTES (personal is the
    # default-selected notebook), `/` jumps to the note by title.
    send_text "hhhl"
    send_text "/"
    send_text "hiking"
    send_key "Return"
    send_text " B"
    shot "23-links-mentions"
    send_key "Escape"

    # Syntax-highlighted fenced code block — retry-helper-snippet.md's
    # ```rust block (see setup_sample_data), viewed in PREVIEW.
    send_text "hhhl"
    send_text "/"
    send_text "retry"
    send_key "Return"
    send_text "l"
    shot "24-syntax-highlighting"
    # The 0.9.1 PREVIEW-rendering showcase — release-day-notes.md (see
    # setup_sample_data) carries every construct the renderer gained recently:
    # a code fence with a `file:` header token + line-number gutter, a
    # collapsible <details>/<summary> block, prettified $$math$$, and a
    # mermaid flowchart, all on one note. Same title-jump pattern as shot 24.
    send_text "hhhl"
    send_text "/"
    send_text "release"
    send_key "Return"
    send_text "l"
    shot "25-preview-rendering"
  else
    shot "overview"
  fi

  kill -9 "$xterm_pid" 2>/dev/null || true
  wait "$xterm_pid" 2>/dev/null || true
}

for theme in "${THEMES[@]}"; do
  echo "== $theme =="
  capture "$theme" wide 140 40
  capture "$theme" stacked 60 40
  capture "$theme" single 40 12
done

# --- CLI-only terminal captures (no TUI, plain stdout) for `shiki graph` and
# `shiki export` — these render fixed ANSI escapes (`graph.rs::render_canvas`),
# not shiki's own themed colors, so one representative capture is enough
# rather than repeating it per theme. Uses gruvbox-dark's background, same
# as the marketing hero's own screenshot, for visual consistency with it.
echo "== cli (graph, export) =="
CLI_CFG="$WORK/config-cli/shiki"
mkdir -p "$CLI_CFG"
sed "s/^name = .*/name = \"gruvbox-dark\"/" "$CONFIG_BASE/config.toml" >"$CLI_CFG/config.toml"

# `display_cmd` is what's echoed as a fake shell prompt line (the plain
# "shiki ..." a real user would type); `real_cmd` is what actually runs
# ($BIN's full temp-dir path — not something worth showing on screen).
# `rows` is sized per-command rather than shared: `graph`'s canvas height
# is `width / 3` (clamped 16..60) — at the terminal's own detected width
# (crossterm::terminal::size(), i.e. `cols` here) that's taller than the
# fixed-height `export` capture needs, and a too-short window silently
# clips the bottom of the canvas/orphans list instead of erroring.
capture_cli() {
  local shotname="$1" display_cmd="$2" real_cmd="$3" cols="$4" rows="$5"
  local title="shiki-ss-cli-$shotname"
  local bg
  bg="$(theme_bg "gruvbox-dark")"
  LANG=C.utf8 LC_ALL=C.utf8 xterm -u8 -fa "$NERD_FONT" -fs 13 -bg "$bg" \
    -xrm "XTerm*internalBorder: 0" -xrm "XTerm*borderWidth: 0" \
    -geometry "${cols}x${rows}" -title "$title" \
    -e env XDG_CONFIG_HOME="$WORK/config-cli" XDG_DATA_HOME="$DATA/.." LANG=C.utf8 LC_ALL=C.utf8 \
    sh -c "printf '\$ %s\n' '$display_cmd'; $real_cmd; sleep 8" \
    >/dev/null 2>&1 &
  local xterm_pid=$!
  sleep 1.5

  local win=""
  for _ in $(seq 1 30); do
    win="$(xdotool search --name "$title" 2>/dev/null | head -1)"
    [ -n "$win" ] && break
    sleep 0.2
  done
  if [ -z "$win" ]; then
    echo "warning: xterm window never appeared for cli/$shotname, skipping" >&2
    kill -9 "$xterm_pid" 2>/dev/null || true
    return
  fi
  sleep 0.5
  mkdir -p "$OUT/cli"
  import -window "$win" "$OUT/cli/$shotname.png"
  kill -9 "$xterm_pid" 2>/dev/null || true
  wait "$xterm_pid" 2>/dev/null || true
}

capture_cli "graph" "shiki graph -n personal" "$BIN graph -n personal" 130 58
capture_cli "export" "shiki export -n personal --out notes.html" \
  "$BIN export -n personal --out /tmp/shiki-ss-export.html && ls -la /tmp/shiki-ss-export.html" 100 14

count=$(find "$OUT" -name '*.png' | wc -l)
echo "Done — $count screenshots in $OUT"

#!/usr/bin/env bash
# Generates marketing assets for query mode specifically — the "★ saved
# queries + generated suggestions" work — not a general app tour like
# scripts/demo-gif.sh (which is already crowded with 19 other phases and
# would bury this feature in the middle of it). Produces one GIF plus a
# handful of PNG stills at the exact frames worth using standalone in an
# announcement post/README section, all from a single scripted recording
# (VHS's `Screenshot` command grabs a still mid-tape without stopping the
# recording), so the GIF and the stills can never show a different-looking
# feature than each other the way two independently-hand-taken sets could.
#
# Seeds a single, deliberately realistic "work" notebook (12 notes across 3
# projects, real people as assignees, a spread of status/priority/due
# values) — richer than scripts/query-demo.sh's toy 8-note example
# notebook (that one exists for a developer to *try* query mode locally;
# this one exists to make query mode *look* powerful on camera), so the
# suggestions list actually has dozens of real entries to show off instead
# of a couple of thin ones.
#
# Usage: scripts/query-feature-demo.sh [gif-path] [screenshots-dir]
#   Defaults to docs/assets/query-feature-demo.gif and
#   docs/assets/query-feature/.
#
# Requires (same as scripts/demo-gif.sh): vhs, ttyd, ffmpeg, a Nerd Font.
#   Arch/CachyOS:   sudo pacman -S vhs
#   Debian/Ubuntu:  see https://github.com/charmbracelet/vhs#installation
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GIF_OUT="${1:-$ROOT/docs/assets/query-feature-demo.gif}"
SHOTS_DIR="${2:-$ROOT/docs/assets/query-feature}"
WORK="$(mktemp -d)"

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

for tool in vhs ttyd ffmpeg; do
  command -v "$tool" >/dev/null || {
    echo "error: $tool not found on \$PATH — see the header of this script for what's needed." >&2
    exit 1
  }
done

# Same fc-list-into-a-variable-first fix scripts/demo-gif.sh/screenshots.sh
# both already needed — piping straight into grep can SIGPIPE fc-list on a
# machine with a large installed Nerd Font family.
FC_LIST_OUTPUT="$(fc-list)"
NERD_FONT="$(grep -im1 "nerd font mono" <<< "$FC_LIST_OUTPUT" | cut -d: -f2 | sed 's/^ *//' | cut -d, -f1)"
NERD_FONT="${NERD_FONT:-monospace}"

BIN="$ROOT/target/release/shiki"
echo "Building release binary..."
cargo build --release -p shiki-cli --manifest-path "$ROOT/Cargo.toml"

# Relative to the actual recording date, not hardcoded, so "overdue"/
# "today"/"this week" always mean what they say regardless of when this
# script runs.
rel() { date -d "$1 days" +%F 2>/dev/null || date -v"$1"d +%F; }

DATA="$WORK/data/shiki"
CFG="$WORK/config/shiki"
mkdir -p "$DATA/work" "$CFG"
git -C "$DATA/work" init -q
git -C "$DATA/work" config user.email "demo@shiki.dev"
git -C "$DATA/work" config user.name "shiki demo"

write_note() {
  local relpath="$1" title="$2" date="$3" tags="$4" status="$5" priority="$6"
  local project="$7" assignee="$8" due="$9" body="${10}"
  cat >"$DATA/work/$relpath" <<EOF
---
title: $title
date: $date
tags: $tags
notebook: work
links: []
template: null
status: $status
priority: $priority
project: $project
assignee: $assignee
due: $due
---

$body
EOF
}

# Three projects, three people, a real spread of status/priority/due — the
# whole point is that leader+q's suggestions list ends up with dozens of
# genuinely distinct entries (every status × every project × every
# assignee value, both sort directions, several due-date variants) rather
# than the two or three a minimal example would generate.
write_note "redesign-checkout-flow.md" "Redesign checkout flow" "2026-07-20" "[frontend]" \
  "in-progress" "high" "Website-Relaunch" "Maria" "$(rel -3)" \
  "New checkout is down to two steps instead of four. Still need to wire up
the saved-address autocomplete before this can ship."

write_note "fix-payment-gateway-bug.md" "Fix payment gateway bug" "2026-07-22" "[backend]" \
  "todo" "high" "Website-Relaunch" "Diego" "$(rel 0)" \
  "Intermittent 502 from the payment provider's sandbox under load. Waiting
on their support ticket before we can reproduce it reliably."

write_note "migrate-database-to-postgres.md" "Migrate database to Postgres" "2026-07-18" "[backend]" \
  "blocked" "high" "Website-Relaunch" "Diego" "$(rel -6)" \
  "Blocked on the infra team provisioning the new instance. Migration
script itself is written and tested against a local copy."

write_note "refactor-auth-module.md" "Refactor auth module" "2026-07-21" "[backend]" \
  "in-progress" "medium" "Website-Relaunch" "Diego" "$(rel 5)" \
  "Splitting session handling out of the monolithic auth service into its
own module ahead of the SSO work next quarter."

write_note "write-api-documentation.md" "Write API documentation" "2026-07-19" "[docs]" \
  "todo" "medium" "Mobile-App" "Maria" "$(rel 3)" \
  "Covers the three new endpoints from last sprint. OpenAPI spec is close,
just needs real example payloads."

write_note "set-up-ci-pipeline.md" "Set up CI pipeline" "2026-07-10" "[infra]" \
  "done" "medium" "Mobile-App" "Diego" "$(rel -9)" \
  "Fastlane + GitHub Actions, builds both platforms on every PR. Average
build time is under six minutes."

write_note "design-onboarding-screens.md" "Design onboarding screens" "2026-07-23" "[design]" \
  "in-progress" "high" "Mobile-App" "Ana" "$(rel 2)" \
  "Three-screen flow instead of the original five — cut the permissions
priming screen after the last round of user testing."

write_note "accessibility-audit.md" "Accessibility audit" "2026-07-15" "[design]" \
  "todo" "low" "Mobile-App" "Maria" "$(rel 10)" \
  "VoiceOver pass on the new onboarding flow once it's out of design
review — screen reader labels are still placeholders in a few spots."

write_note "user-interview-synthesis.md" "User interview synthesis" "2026-07-05" "[research]" \
  "done" "medium" "Mobile-App" "Maria" "$(rel -14)" \
  "Eight interviews, clear signal that the search feature is undiscoverable
— it's the top request across every single session."

write_note "q3-budget-review.md" "Q3 budget review" "2026-07-24" "[planning]" \
  "todo" "low" "Q3-Planning" "Ana" "$(rel 14)" \
  "Waiting on actuals from finance before this can really start. Draft
headcount numbers are in the shared sheet."

write_note "prepare-investor-update.md" "Prepare investor update" "2026-07-24" "[planning]" \
  "todo" "high" "Q3-Planning" "Ana" "$(rel 4)" \
  "Needs the real Q2 retention numbers once they're finalized — everything
else in the deck is ready."

write_note "retro-notes-sprint-14.md" "Retro notes — sprint 14" "2026-07-17" "[planning]" \
  "done" "low" "Q3-Planning" "Ana" "$(rel -8)" \
  "Main takeaway: estimates keep slipping on anything touching the payment
gateway specifically — worth its own retro at some point."

git -C "$DATA/work" add -A
git -C "$DATA/work" commit -q -m "seed demo data"

cat >"$CFG/config.toml" <<EOF
[general]
default_notebook = "work"

[theme]
name = "gruvbox-dark"

[git]
auto_commit = false
auto_push = false
EOF

mkdir -p "$(dirname "$GIF_OUT")" "$SHOTS_DIR"
rm -f "$SHOTS_DIR"/*.png

TAPE="$WORK/query-demo.tape"
cat >"$TAPE" <<TAPEEOF
Output "$GIF_OUT"
Set Shell "bash"
Set FontFamily "$NERD_FONT"
Set FontSize 16
Set Width 1200
Set Height 750
Set Padding 0
Set TypingSpeed 30ms
Set WaitTimeout 5s

Hide
# \`shiki\` resolves via \$PATH here — this script invokes \`vhs\` itself with
# XDG_CONFIG_HOME/XDG_DATA_HOME/PATH all pointing at this recording's own
# throwaway data and release binary (see the \`vhs "\$TAPE"\` invocation
# below), so every command in this whole tape — this launch and the
# closing CLI segment alike — shares one consistent environment without
# repeating it inline each time.
Type "shiki"
Enter
Sleep 1200ms
Show

# --- Phase 1: land in the notebook (single notebook, so Enter goes
# straight in), glance at the note list to establish there's a real,
# varied set of work here before jumping to the feature itself.
Sleep 400ms
Enter
Sleep 500ms
Down@100ms 4
Sleep 500ms
Up@100ms 4
Sleep 300ms

# --- Phase 2: open query mode with an empty box — every generated
# suggestion (every status/priority/project/assignee value, both sort
# directions, relative-date ranges) shows immediately, no typing needed.
Space
Type "q"
Sleep 900ms
Screenshot "$SHOTS_DIR/1-suggestions.png"
Down@120ms 6
Sleep 700ms

# --- Phase 3: typing filters the suggestions live — "priority" narrows
# straight down to only the priority-related ones.
Up@120ms 6
Sleep 300ms
Type "priority"
Sleep 900ms
Screenshot "$SHOTS_DIR/2-filtered-suggestions.png"

# --- Phase 4: Enter fills the box with the highlighted suggestion and
# runs it immediately — real matching notes, not a mockup table.
Enter
Sleep 900ms
Screenshot "$SHOTS_DIR/3-results.png"

# --- Phase 5: clear the box and hand-write a real, multi-condition query
# — proves this isn't just clicking through canned suggestions.
Escape
Sleep 400ms
Space
Type "q"
Sleep 500ms
Type "where status != done and priority = high sort due asc"
Sleep 1100ms
Screenshot "$SHOTS_DIR/4-custom-query.png"

# --- Phase 6: save it — Ctrl+S prompts for a name, writes straight to
# config.toml's [queries] table.
Ctrl+S
Sleep 600ms
Type "urgent-open-work"
Sleep 400ms
Screenshot "$SHOTS_DIR/5-save-query.png"
Enter
Sleep 700ms

# --- Phase 7: clear the box again — the saved query now leads the
# suggestions list with a star, ahead of every generated example.
Escape
Sleep 300ms
Space
Type "q"
Sleep 700ms
Screenshot "$SHOTS_DIR/6-saved-query-suggestion.png"
Enter
Sleep 900ms
Escape
Sleep 300ms

# --- Phase 8: the same query DSL, reachable a second way — global search
# doubling as query mode the instant the box starts with "!". Typing a
# project name jumps straight into a real matching note.
Space
Type "g"
Sleep 400ms
Type "!where project = Mobile-App"
Sleep 1000ms
Screenshot "$SHOTS_DIR/7-global-search-query-mode.png"
Enter
Sleep 900ms

# Quit off-screen, back to a bare shell — the closing beat runs the same
# saved query from the command line, proving it round-trips: a query
# saved from inside the TUI a few seconds ago is already usable by
# \`shiki query --saved\`, no restart or re-save needed.
Hide
Type "q"
Sleep 300ms
Show
Sleep 300ms
Type "shiki query --saved urgent-open-work"
Sleep 300ms
Enter
Sleep 900ms
Screenshot "$SHOTS_DIR/8-cli-saved-query.png"
Sleep 500ms
TAPEEOF

echo "Recording with VHS..."
XDG_CONFIG_HOME="$WORK/config" XDG_DATA_HOME="$WORK/data" PATH="$(dirname "$BIN"):$PATH" \
  vhs "$TAPE"

echo "Wrote $GIF_OUT"
echo "Wrote stills to $SHOTS_DIR:"
ls -1 "$SHOTS_DIR"

#!/usr/bin/env bash
# scripts/query-demo.sh — a disposable "shiki-test" playground for trying
# query mode without touching any real notebook: leader+`q`'s dedicated
# modal, and leader+`g`'s global search doubling as query mode when the box
# starts with `!` (see shiki-tui/src/key_handlers.rs::global_search_is_query).
#
# Seeds a "demo" notebook with 8 notes carrying varied custom frontmatter
# (status/priority/project/due) — plain `shiki new` has no flag for custom
# frontmatter fields, so each note is created normally and then has its
# extra fields spliced in right after the always-present `template: null`
# line (every note's frontmatter ends with that line; see note.rs's
# Frontmatter field order) — plus a "Query examples" note, inside the demo
# notebook itself, listing ready-to-try DSL strings so they're readable
# from PREVIEW too, not just this script's own stdout.
#
# Uses the exact XDG-override convention CLAUDE.md documents for exercising
# the CLI without touching real user config/data. The directories are wiped
# on every run, so re-running this script always starts from the same known
# state — nothing here is meant to be kept between runs.
#
# Usage:
#   scripts/query-demo.sh          seeds data, launches the TUI
#   scripts/query-demo.sh --cli    seeds data, runs one `shiki query` example
#                                   and prints the rest instead of launching
#                                   the TUI

set -euo pipefail
cd "$(dirname "$0")/.."

export XDG_CONFIG_HOME=/tmp/shiki-test-config
export XDG_DATA_HOME=/tmp/shiki-test-data
rm -rf "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"

echo "==> building shiki-cli (debug)"
cargo build -p shiki-cli --quiet
BIN=./target/debug/shiki

echo "==> seeding the 'demo' notebook"
"$BIN" notebook create demo >/dev/null

# Inserts one "key: value" frontmatter line into an already-created note,
# right after the `template: null` line every note's frontmatter has.
add_field() {
    sed -i "/^template: null\$/a $2" "$1"
}

# $1 = title, $2 = body, then any number of "key: value" extra-field pairs.
new_note() {
    local title="$1" body="$2" path
    shift 2
    path=$("$BIN" new "$title" -n demo --body "$body" | sed -n 's/^created: //p')
    for kv in "$@"; do
        add_field "$path" "$kv"
    done
}

in_days() { date -I -d "+$1 days"; }
ago_days() { date -I -d "-$1 days"; }
today=$(date -I)

new_note "Fix login bug" \
    "Users can't log in with SSO after the last deploy." \
    "status: pending" "priority: high" "project: alpha" "due: $(ago_days 3)"
new_note "Write onboarding docs" \
    "Draft the getting-started guide for new hires." \
    "status: pending" "priority: medium" "project: alpha" "due: $(in_days 2)"
new_note "Refactor auth module" \
    "Split the auth module into smaller services." \
    "status: in-progress" "priority: high" "project: alpha" "due: $(in_days 7)"
new_note "Update dependencies" \
    "Bump every outdated crate/package." \
    "status: done" "priority: low" "project: beta" "due: $(ago_days 10)"
new_note "Design new landing page" \
    "Mockups for the redesigned homepage." \
    "status: pending" "priority: medium" "project: beta" "due: $today"
new_note "Research competitor pricing" \
    "Compare pricing tiers across five competitors." \
    "status: pending" "priority: low" "project: gamma"
new_note "Client meeting notes" \
    "Notes from the kickoff call." \
    "status: done" "priority: high" "project: gamma" "due: $(ago_days 5)"
new_note "Plan Q3 roadmap" \
    "Draft goals and milestones for Q3." \
    "status: pending" "priority: high" "project: alpha" "due: $(in_days 14)"

examples=$(cat <<'EOF'
Try these in the query modal (leader+q) or global search's query mode
(leader+g, then type ! first):

  where status = pending
  where status = pending and priority = high
  where project = alpha sort due asc
  where priority = high or priority = medium
  where due < today
  where status != done sort due asc
  where project ~ "al"
  where contains "landing"

Or from a shell, with `shiki query` (scriptable, --json/--count):

  shiki query 'where status = pending sort due asc'
  shiki query 'where priority = high' --json
  shiki query 'where project = alpha' --count
EOF
)
new_note "Query examples" "$examples"

echo
echo "==> demo notebook ready at \$XDG_DATA_HOME/shiki/demo (8 notes + this cheat sheet)"
echo
echo "Example queries to try (leader+q, or leader+g then '!'):"
echo "  where status = pending"
echo "  where status = pending and priority = high"
echo "  where project = alpha sort due asc"
echo "  where due < today"
echo "  where status != done sort due asc"
echo

if [[ "${1:-}" == "--cli" ]]; then
    echo "==> shiki query 'where status = pending sort due asc'"
    "$BIN" query 'where status = pending sort due asc'
    exit 0
fi

echo "==> launching the TUI (leader is Space by default) — Esc to close a modal, q to quit"
exec "$BIN"

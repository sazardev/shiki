#!/usr/bin/env bash
# Generates docs/assets/demo.gif (plus an mp4 transcode of it, which the
# marketing site's hero actually serves as a `<video>`): a fast-paced,
# scripted tour of shiki
# against a deliberately rich dataset (3 notebooks, 30 notes, nested
# folders 2 levels deep, long note bodies, varied tags) — meant to show
# the app handling real volume, not a 3-note toy example. Beyond browsing/
# reading, the tour also exercises real editing actions: global search
# with an actual cross-notebook jump, tags, multi-select (batch delete),
# folder create + folder move, writing a full multi-section note using the
# inline editor's `/`-menu (v0.8.3+) for headings/bullets/checklist items
# instead of hand-typed markdown, a git commit, a full tour of the now
# fully-interactive Settings screen (v0.8.5 — every tab, including creating
# a brand-new `/`-menu snippet from inside it), the native editor's mouse/
# keyboard UX overhaul (multi-cursor via repeated Ctrl+D, live find/
# replace), and live-cycling through the theme picker's alphabetical list
# (Cyberpunk 2077 / LoL (Jinx) — the newest catalog flagships) — not just
# read-only navigation. Runs
# automatically on every release now
# (release.yml's update-screenshots job calls this right alongside
# scripts/screenshots.sh, committing docs/assets/demo.gif back to `main`
# the same way it already did for the per-theme screenshots), so the demo
# can't go stale from here on — before this it was a manual "re-run after
# any workflow change" step, which is exactly how the fixed Phase 10 bug
# below happened once already: the `a`/NewNote flow grew a template-picker
# step in v0.8.1 and this script kept assuming the old direct-to-editor
# behavior for several releases after, because nobody remembered to re-run
# it by hand.
#
# Uses VHS (https://github.com/charmbracelet/vhs): a `.tape` file is a
# literal, deterministic keystroke script, so the same recording comes out
# every run — no manual screen-recording/editing step, and it's cheap to
# re-run after every release against that release's own binary.
#
# Usage: scripts/demo-gif.sh [output-path]
#   Defaults to docs/assets/demo.gif.
#
# Requires (local dev machine, or CI — see release.yml's update-screenshots
# job for the exact install commands): vhs, ttyd, ffmpeg (vhs's own runtime
# deps), and a Nerd Font (same requirement as scripts/screenshots.sh, same
# fc-list auto-detection).
#
#   Arch/CachyOS:   sudo pacman -S vhs
#   Debian/Ubuntu:  see https://github.com/charmbracelet/vhs#installation
#     (vhs isn't in Ubuntu's default apt repos — install via the .deb
#     release asset or `go install`)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/docs/assets/demo.gif}"
WORK="$(mktemp -d)"
# Built-in theme the recording is shot in (used in the config heredoc
# below) — defaults to gruvbox-dark; the docs site records one video per
# theme so the on-site theme switcher can swap the hero to match.
THEME="${THEME:-gruvbox-dark}"

cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

for tool in vhs ttyd ffmpeg; do
  command -v "$tool" >/dev/null || {
    echo "error: $tool not found on \$PATH — see the header of this script for what's needed." >&2
    exit 1
  }
done

# fc-list is captured to a variable before grep ever runs, not piped
# straight into it — see scripts/screenshots.sh's identical line for why:
# a live `fc-list | grep -m1 ...` pipe can still SIGPIPE fc-list itself
# once there are enough matching lines (reproduced directly on a machine
# with a large installed Nerd Font family), the same failure a previous,
# `head -1`-based version of this line had, just from a different command
# closing the pipe early.
FC_LIST_OUTPUT="$(fc-list)"
NERD_FONT="$(grep -im1 "nerd font mono" <<< "$FC_LIST_OUTPUT" | cut -d: -f2 | sed 's/^ *//' | cut -d, -f1)"
NERD_FONT="${NERD_FONT:-monospace}"

BIN="$ROOT/target/release/shiki"
echo "Building release binary..."
cargo build --release -p shiki-cli --manifest-path "$ROOT/Cargo.toml"

# Computed relative to the actual recording date, not hardcoded, so the
# global tasks phase always shows one real overdue, one due-today, and one
# future task regardless of when this script runs.
DUE_OVERDUE="$(date -d '-4 days' +%F 2>/dev/null || date -v-4d +%F)"
DUE_TODAY="$(date +%F)"
DUE_FUTURE="$(date -d '+5 days' +%F 2>/dev/null || date -v+5d +%F)"

# --- Rich sample data: 3 notebooks, 30 notes, folders 2 levels deep, long
# bodies, varied tags — deliberately more than scripts/screenshots.sh's
# minimal set, since this recording exists specifically to demonstrate
# shiki staying fast and legible at real volume, not just its layout.
DATA="$WORK/data/shiki"
CFG="$WORK/config/shiki"
mkdir -p "$DATA" "$CFG"

write_note() {
  local nb="$1" relpath="$2" title="$3" date="$4" tags="$5" body="$6"
  mkdir -p "$DATA/$nb/$(dirname "$relpath")"
  cat >"$DATA/$nb/$relpath" <<EOF
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

for nb in personal work research; do
  mkdir -p "$DATA/$nb"
  git -C "$DATA/$nb" init -q
  git -C "$DATA/$nb" config user.email "demo@shiki.dev"
  git -C "$DATA/$nb" config user.name "shiki demo"
done

# === personal/ — 8 root notes + journal/ (4) + projects/{shiki-app,website}/ (2 each) ===

write_note personal "book-recommendations.md" "Book recommendations" "2026-07-15" "[reading, books]" \
"## Currently reading

- *The Pragmatic Programmer* — Hunt & Thomas
- *Project Hail Mary* — Andy Weir

## Queue

- *A Philosophy of Software Design* — John Ousterhout
- *The Left Hand of Darkness* — Ursula K. Le Guin
- *Exhalation* — Ted Chiang
- *The Design of Everyday Things* — Don Norman

## Finished this year

1. *Klara and the Sun* — Kazuo Ishiguro — beautiful, quietly devastating.
2. *Debt: The First 5000 Years* — David Graeber — dense but worth it.
3. *The Phoenix Project* — Gene Kim — read this one for work, actually enjoyed it.

Recommended by a friend: anything by Ted Chiang, starting with *Exhalation*.
Also want to revisit *Gödel, Escher, Bach* — tried it in college, bounced off it,
might land better now.

Want to bring something short along on the Weekend hiking trip — maybe the Ted Chiang."

write_note personal "gift-ideas.md" "Gift ideas" "2026-07-14" "[shopping, gifts]" \
"## Mom's birthday (August)

- That ceramic pour-over set she mentioned twice
- Framed print from the trip to the coast
- Gift card as backup if nothing lands in time

## Dad — Father's Day was already covered, but for Christmas

- Replacement head for the electric razor (he never buys this himself)
- A decent multitool — his current one is rusted through

## Housewarming (the Chens, new apartment)

- Something for the kitchen, they mentioned they don't own a real chef's knife
- A plant that's hard to kill — pothos or a snake plant

## Running list of \"good gift, no occasion yet\"

- Nice fountain pen
- A framed star chart for their anniversary
- That board game everyone keeps recommending"

write_note personal "morning-routine.md" "Morning routine" "2026-07-10" "[habits, productivity]" \
"## Current routine (roughly 6:15–7:30)

1. Wake up, no snooze — phone charges across the room specifically to force this.
2. Glass of water before coffee.
3. 20 minutes of reading (paper book, not a screen) with the first coffee.
4. Quick stretch/mobility routine — nothing fancy, maybe 10 minutes.
5. Shower, get dressed, out the door.

## What's actually working

- Reading before screens has been the single biggest change — sets a completely
  different tone for the day than opening a phone first thing.
- Water before coffee sounds trivial but noticeably helps with the mid-morning
  energy dip.

## What isn't

- The mobility routine gets skipped more often than not once things get busy.
  Might need to shrink it to something so short there's no excuse (5 minutes?).
- Still checking messages on the walk to the kitchen sometimes. Working on it.

## Experiment for next month

Try moving the workout to mornings instead of evenings, see if it sticks better
before the day has a chance to get away from me."

write_note personal "movie-watchlist.md" "Movie watchlist" "2026-07-08" "[entertainment]" \
"## To watch

- *Perfect Days* — heard it's meditative, in the mood for that
- *The Zone of Interest*
- *Past Lives*
- Rewatch *Paprika* — been too long

## Watched recently

- *Poor Things* — visually stunning, mixed on the pacing in the back half
- *Oppenheimer* — the sound design alone was worth the ticket
- *The Holdovers* — exactly the comfort-watch it looked like it'd be

## Someone's recommendation queue

Friend keeps insisting on the whole *Before* trilogy back to back. Need to
actually clear an evening for that instead of half-watching it in pieces."

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
6. Roll thin, cut to whatever shape — tagliatelle is the easiest to cut by hand.
7. Cook 2–3 minutes in heavily salted boiling water, fresh pasta cooks fast.

## Notes from last attempt

Dough was slightly too wet — next time start with 380g flour and add more only
if it won't come together, easier to add flour than take it away.

Best paired with a simple brown butter and sage sauce, or a light tomato and
basil if it's summer and the tomatoes are actually good."

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

write_note personal "travel-bucket-list.md" "Travel bucket list" "2026-07-05" "[travel, dreams]" \
"## Definitely happening (booked or budgeted)

- Portugal, October — Lisbon then a few days in Porto
- Long weekend in the mountains for the fall colors

## Someday list

- Japan in cherry blossom season — apparently you have to book almost a year out
- New Zealand, South Island specifically, for the hiking
- Slow train across a country instead of flying, just to see what that's like
- A proper long-haul backpacking trip before settling down feels harder to do

## Notes to self

Stop overplanning these and just book something. The Portugal trip almost
didn't happen because of exactly that — glad it did anyway."

write_note personal "weekend-hiking-trip.md" "Weekend hiking trip" "2026-07-10" "[outdoors, planning]" \
"Planning a two-day trip along the ridge trail.

## Logistics

- Trailhead parking fills up early, arrive before 7am
- Pack layers, weather can flip fast above 2000m
- Water refill point at the halfway hut
- Reserve the shelter two weeks in advance

## Gear checklist

- [ ] Tent + stakes @due($DUE_OVERDUE)
- [ ] Sleeping bag rated for the actual overnight low, not the daytime high @due($DUE_TODAY)
- [ ] Water filter, not just tablets @due($DUE_FUTURE)
- [ ] Headlamp + spare batteries
- [ ] First aid kit, actually check what's in it before leaving

## Route notes from last time

The east approach is prettier but adds about two hours — worth it if the
weather holds, skip it if there's any chance of afternoon storms."

write_note personal "year-2026-goals.md" "2026 goals" "2026-01-02" "[goals, planning]" \
"## Health

- Run a half marathon — currently at 10k comfortably, need to build up
- Actually use the gym membership more than twice a month

## Work / craft

- Ship the side project instead of letting it rot in a private repo forever
- Get comfortable with a second language beyond what work requires

## Personal

- Read 24 books (2 a month) — tracking in book-recommendations.md
- Take the Portugal trip, don't let it slip like last year

## Mid-year check-in (July)

Running is going well, roughly on pace. Side project is... still not shipped.
Portugal is booked though, so that one's actually happening this time."

write_note personal "projects/shiki-app/roadmap.md" "shiki roadmap" "2026-07-01" "[rust, tui, project-management]" \
"## Shipped

- Three-pane Yazi-style layout, responsive to terminal size
- Notebooks as independent git repos
- Real per-note version history via git log
- 37 built-in themes, live picker
- In-TUI self-update

## In progress

- Marketing site + full documentation reference
- CI: automated screenshots + demo GIF per release

## Considering

- Shell completions + man page (clap_complete)
- Encrypted notes at rest — bigger scope, needs real design first
- Plugin system — probably not, keep the surface area small

## Explicitly not doing

- A GUI version — the whole point is staying in the terminal
- A hosted/cloud sync service — git remotes already solve this"

write_note personal "projects/shiki-app/bug-tracker.md" "shiki bug tracker" "2026-07-12" "[rust, tui, bugs]" \
"## Open

- Long lines in PREVIEW don't always wrap at a sensible point on very narrow
  terminals — check the wrapping logic against a 46-column window.
- Theme picker live-preview can lag by one frame on a very large notebook.

## Fixed recently

- White border in generated screenshots (xterm's internalBorder default) —
  fixed by setting -bg per theme and -xrm to zero the border out entirely.
- Theme screenshot not hiding behind the CSS fallback mockup — a CSS rule
  with higher specificity than the browser's default [hidden] rule.

## Won't fix

- Icons render as boxes without a Nerd Font installed — this is inherent to
  using Nerd Font glyphs at all, documented in the README prerequisites."

write_note personal "projects/website/todo.md" "Website TODO" "2026-07-20" "[web, project-management]" \
"## Before next release

- [ ] Regenerate screenshots against the new version
- [ ] Update the changelog section fetch to confirm it still parses cleanly
- [ ] Re-check the OG image still renders correctly after any copy changes

## Nice to have, not blocking

- [ ] Dark/light auto-detect based on the visitor's OS preference on first load
- [ ] A proper 404 page instead of GitHub Pages' default
- [ ] Analytics — privacy-respecting only, still deciding if it's worth it at all"

write_note personal "projects/website/design-notes.md" "Website design notes" "2026-07-19" "[web, design]" \
"## Why the theme switcher matters

Most landing pages for terminal tools show one static screenshot and call it
done. Letting a visitor actually click through the real palettes — with real
screenshots, not a CSS approximation — is a much stronger \"this is a real,
polished piece of software\" signal than any amount of copywriting.

## Layout decisions

- Themes section right after the hero, not buried after Features — the
  live-recolor effect should be visible within one scroll.
- Hero screenshot sized noticeably larger than the text column, since the
  screenshot itself is doing most of the persuading.

## Open questions

- Does the demo GIF belong in the hero too, replacing the static screenshot,
  or lower down as a supplement? Leaning toward: static screenshot in the
  hero (loads instantly), GIF further down where a visitor has already
  decided they're interested enough to keep scrolling."

write_note personal "journal/2026-07-20.md" "2026-07-20" "2026-07-20" "[journal]" \
"Spent most of today heads-down on the release checklist. Longer than expected
because of the screenshot border bug — small thing, but the fix took a while
to actually track down to the right root cause instead of just papering over
the symptom.

Good focus day overall. Ended with a short walk, needed it after staring at a
terminal for that long."

write_note personal "journal/2026-07-21.md" "2026-07-21" "2026-07-21" "[journal]" \
"Slower day. Spent the morning on something that turned out to be a dead end —
tried optimizing a code path that wasn't actually the bottleneck. Should have
measured first instead of assuming.

Lesson, again: profile before optimizing, no matter how obvious the \"obvious\"
bottleneck seems."

write_note personal "journal/2026-07-22.md" "2026-07-22" "2026-07-22" "[journal]" \
"Good conversation about the roadmap today. Landed on: ship what's already
built and polished before starting anything new, rather than letting three
half-finished features pile up at once.

Also finally fixed the thing that's been bugging me about the theme picker's
live preview lag. Turned out to be a caching issue, of course it was."

write_note personal "journal/2026-07-23.md" "2026-07-23" "2026-07-23" "[journal]" \
"Big day — the whole open-source prep landed: contributing guide, code of
conduct, security policy, issue templates, branch protection, the works.
Also the marketing site went live.

Satisfying to see it all actually deployed instead of sitting as local
commits. Tomorrow: keep an eye on whether any of the automation breaks on
the next real release."

# === work/ — 5 root notes + clients/ (2) + docs/ (2) ===

write_note work "meeting-notes-q3-planning.md" "Meeting notes: Q3 planning" "2026-07-20" "[work, meetings, planning]" \
"## Attendees

Product, Engineering, Design leads.

## Decisions

- Ship the notebook tree view before the Q3 review
- Push the mobile companion app to Q4 — not enough bandwidth to do it well
- Design to finalize the onboarding flow by end of month

## Action items

- [ ] Engineering: scope the tree view work, rough estimate by Friday
- [ ] Design: onboarding flow mockups
- [ ] Product: update the roadmap doc to reflect the Q4 push

## Open questions carried to next meeting

Do we need a dedicated mobile design pass before Q4, or can engineering start
from the desktop flows and adapt as they go?"

write_note work "architecture-decisions.md" "Architecture decisions" "2026-07-05" "[work, architecture]" \
"## ADR-014: Background sync via a dedicated thread + channel

Chose a plain std::thread + mpsc::channel over pulling in an async runtime,
since the rest of the render loop is already a synchronous ~100ms poll loop.
Adding async for one feature would be inconsistent with everything else and
buy nothing.

## ADR-015: Per-notebook sync policy overrides

Global git settings weren't enough — a private work repo should auto-push,
a scratch notebook with no remote shouldn't try to sync at all. Solved with
an optional override table per notebook, falling back to global defaults
for anything unset.

## ADR-016: Real screenshot generation over hand-drawn mockups

For any UI documentation/marketing asset, prefer capturing the actual
running app over a hand-drawn approximation — mockups drift from reality,
real screenshots can't."

write_note work "sprint-review.md" "Sprint review" "2026-07-18" "[work, retro]" \
"## Completed

- Folder move/copy/delete, generalized addressing scheme
- Visual mode multi-select, wired up after sitting dead in the enum for a while
- Auto-tag workflow, closes the gap where a release could be built but never
  actually get tagged/published

## Carried over

- Marketing site polish — bigger than expected once the theme switcher and
  live changelog were added on top of the original plan

## Retro notes

Estimation was off on \"add a marketing site\" — treated it like a small task,
it grew into: theme switcher, screenshot automation, SEO, a full documentation
page, and a CI deploy pipeline. Should have scoped it as its own sprint from
the start instead of squeezing it in."

write_note work "onboarding-checklist.md" "Onboarding checklist" "2026-06-01" "[work, onboarding]" \
"## Day one

- [ ] Repo access, CI secrets explained (what's provisioned, what's pending)
- [ ] Walk through CLAUDE.md — it's the actual source of truth for
  non-obvious decisions, not just a formality
- [ ] Get the dev environment building: cargo check --workspace should be
  green before anything else

## First week

- [ ] Pick a small, well-scoped issue to get a full PR through the pipeline
  once, branch protection and all
- [ ] Read through the four-crate architecture split — shiki-core has no
  ratatui dependency on purpose, don't reach for it there

## Common first-week mistakes to flag early

- Adding a TUI-only concept into shiki-core (it should stay pure domain logic)
- Reaching for git commands directly instead of the existing git.rs helpers"

write_note work "team-retro-notes.md" "Team retro notes" "2026-07-01" "[work, retro]" \
"## What went well

- Shipping in small, reviewable increments instead of one giant PR
- Documenting the *why* behind non-obvious decisions as we go, not after
  the fact when the reasoning's already been forgotten

## What didn't

- A couple of \"quick fixes\" turned out to have a root cause worth actually
  investigating instead of patching the symptom — cost more time overall
  than just investigating properly the first time would have

## Action for next sprint

When something feels like it should be quick and isn't, that's the signal
to stop and actually understand it rather than pushing through."

write_note work "clients/acme-corp.md" "Acme Corp" "2026-06-15" "[work, clients]" \
"## Contacts

- Primary: their eng lead, prefers async updates over calls
- Escalation: their PM, only loop in for anything actually blocking

## Current engagement

Quarterly infrastructure review, mostly advisory. Nothing time-sensitive
right now, next check-in scheduled for end of quarter.

## History

Been a client for two years, generally low-friction. One rough patch early
on around unclear scope — fixed by writing everything into a shared doc
before starting future engagements."

write_note work "clients/globex-industries.md" "Globex Industries" "2026-05-20" "[work, clients]" \
"## Contacts

- Primary: their CTO, very hands-on, expect direct technical questions
- They prefer everything in writing — document decisions, don't rely on
  verbal agreements from calls

## Current engagement

Migration project, roughly 60% complete. On schedule as of the last check-in.

## Watch items

- Their legacy system's undocumented edge cases keep surfacing later than
  ideal — worth over-communicating discovery of these as they're found
  rather than batching them into a single end-of-phase report."

write_note work "docs/api-reference.md" "API reference" "2026-07-10" "[work, docs]" \
"## Authentication

All requests require a bearer token in the Authorization header.
Tokens are scoped per-client, rotate every 90 days.

## Endpoints

- GET /v1/status — health check, no auth required
- GET /v1/resources — list resources, paginated
- POST /v1/resources — create, idempotent via a client-supplied key
- DELETE /v1/resources/{id} — soft-delete, recoverable for 30 days

## Rate limits

1000 requests/hour per token, 429 with a Retry-After header when exceeded."

write_note work "docs/deployment-guide.md" "Deployment guide" "2026-07-08" "[work, docs, deployment]" \
"## Pre-deploy checklist

- [ ] All tests green on the target branch
- [ ] Changelog entry added
- [ ] Database migrations reviewed if any are included

## Deploy steps

1. Tag the release
2. CI builds and runs the full verification suite
3. Manual approval gate for production (deliberately not fully automatic)
4. Rollout is gradual, monitored at each stage before proceeding

## Rollback

Previous version stays deployable for 48 hours after any release
specifically so a rollback is always a known-good, already-tested target."

# === research/ — 6 root notes, no folders (a flat notebook for variety) ===

# Three lines starting with the exact same word, deliberately — Phase 13
# below drives this with repeated Ctrl+D presses (select word under
# cursor, then keep adding the next occurrence), which only needs the
# cursor to start at (0, 0) — the default position on entering edit mode —
# and never needs pixel-precise mouse coordinates the way an Alt+Click
# multi-cursor demo would. Placed in research/, not personal/, on purpose:
# a real bug caught live building this phase — an earlier version added it
# to personal/'s root, and since notes sort alphabetically, "Errands this
# week" landed *before* "Gift ideas" there, shifting every later note's
# index by one. Phase 7 (much earlier in this same recording, written long
# before this note existed) batch-deletes by a hardcoded index that
# assumed "Gift ideas" specifically, so it silently deleted this note
# instead once the index shifted — by Phase 13 the note was simply gone
# ("no match" from the notes-scope search that first replaced global
# search here, before the actual root cause was found). research/ is
# never touched by index-dependent navigation anywhere in this recording,
# so a new note there can't collide with any earlier phase's assumptions
# the way personal/'s root can.
write_note research "errands-this-week.md" "Errands this week" "2026-07-21" "[personal, todo]" \
"TODO: buy milk
TODO: call the dentist
TODO: finish the quarterly report"

write_note research "rust-async-patterns.md" "Rust async patterns" "2026-07-02" "[rust, research]" \
"## When NOT to reach for async

If the rest of a codebase is already synchronous (a plain render loop, a
CLI tool with no concurrent I/O to overlap), pulling in an async runtime for
one feature is usually the wrong call — a plain thread + channel handles
\"do this in the background, tell me when it's done\" just fine without the
added complexity of a runtime nobody else in the codebase uses.

## When it's clearly worth it

- Genuinely concurrent I/O — many sockets/requests in flight at once
- A framework that's already async-first (most web servers)

## Patterns worth remembering

- mpsc channels for background-thread-to-main-thread communication cover a
  surprising amount of what people reach for async for
- Capturing values *before* a long-running operation that might invalidate
  them (e.g. current_exe() before self-replacing the running binary) is a
  general pattern, not async-specific, but bites people in async code a lot"

write_note research "tui-design-inspiration.md" "TUI design inspiration" "2026-06-20" "[tui, research, design]" \
"## Yazi

The Miller-columns file manager layout (each level collapses the previous
one down to a thin strip) translates surprisingly well to a notes app —
notebooks/notes/preview maps naturally onto the same pattern file managers
already use for directories/files/preview.

## Helix

Modal editing done right — the which-key-style discoverability (press a
key, see what it could lead to) lowers the learning curve a lot compared to
expecting users to memorize a keybinding cheat sheet up front.

## Common thread across the good ones

Consistent, hardcoded navigation (movement keys behave the same everywhere)
combined with fully custom action keybindings per context. Trying to make
navigation itself configurable seems to be a trap — it stops feeling
predictable the moment two similar apps bind movement differently."

write_note research "note-taking-apps-comparison.md" "Note-taking apps comparison" "2026-06-25" "[research, notes-apps]" \
"## Obsidian

Excellent linking/graph view, but Electron-based — heavier than it needs to
be for what's fundamentally text editing, and the vault format, while
plain-text, has enough proprietary conventions layered on top that it
doesn't feel fully portable.

## Notion

Powerful, but genuinely proprietary storage — no meaningful offline story,
and exporting cleanly is harder than it should be for something calling
itself a notes app.

## Plain directories of Markdown + a tool on top (nb, this project)

The actual notes are just files, always portable, always readable with
nothing but a text editor even if the tool itself disappeared. The tradeoff
is building (or choosing) a tool that makes that plain-file approach
pleasant to use day to day, instead of getting linking/search/organization
for free from a database-backed app."

write_note research "git-internals.md" "Git internals" "2026-06-10" "[git, research]" \
"## Objects

Everything is a blob, tree, commit, or tag — content-addressed by SHA. A
commit is really just a pointer to a tree plus some metadata and a pointer
to its parent(s).

## Why per-file history \"for free\" isn't actually free

git has no built-in \"log for one path\" primitive at the object level —
\`git log -- path\` (and shiki's own file_history) has to walk the full
commit graph and diff each commit's tree against its parent's, checking
whether the specific path's blob changed. It's not indexed by path anywhere
in the object model itself.

## Fast-forward vs. real merges

A fast-forward is just moving a branch pointer forward when the current
branch's history is a strict subset of the target — no new commit is
created. This is why \`pull\`'s fast-forward-or-fail behavior can't just
always succeed: if local history has diverged, git has no fast-forward
path available at all, only a real merge or a rebase."

write_note research "terminal-ui-libraries.md" "Terminal UI libraries" "2026-06-05" "[rust, tui, research]" \
"## ratatui

Immediate-mode-ish rendering — you rebuild the widget tree every frame from
current state, and the library diffs against the previous frame's buffer to
compute the minimal set of terminal writes. No persistent widget objects to
manage state for.

## crossterm

The terminal backend underneath — raw mode, alternate screen, mouse
capture, cross-platform (including real Windows console support, not just
ANSI-passthrough), which is why ratatui pairs with it instead of a
Unix-only backend.

## Why not a retained-mode GUI toolkit's terminal equivalent

Retained-mode makes sense when widget state is expensive to rebuild every
frame. For a note-taking TUI redrawing at ~10Hz against small in-memory
lists, immediate-mode's simplicity (state lives in one place, rendering is
a pure function of it) outweighs any performance argument for retained
widgets."

# The 0.9.1 PREVIEW-rendering showcase — a note carrying every construct the
# renderer gained recently, so Phase 20 can show them all at once: a code
# fence with a `file:` header token + line-number gutter, a collapsible
# <details>/<summary> block, a prettified $$math$$ block, and a mermaid
# flowchart. Deliberately in research/ (flat, never touched by index-
# dependent navigation anywhere in this recording) — same rule as the
# errands-this-week.md note above — so adding it can't shift any earlier
# phase's hardcoded indices in personal/. `\$`/backticks stay literal inside
# the heredoc.
write_note research "release-day-notes.md" "Release day notes" "2026-07-22" "[rust, meta]" \
"## Feature rollup

\`\`\`tsx file:App.tsx
const App = () => {
  return <h1>{title}</h1>;
};
\`\`\`

<details>
<summary>Why TypeScript now highlights</summary>

\`two-face\` bundles TSX (plus ~150 more languages) on top of syntect's
defaults, so this fence is colored instead of flat dimmed text.
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

echo "Sample data written under $DATA"

# --- Config: a real theme active, matching the version currently being
# recorded, plus enough of a keybindings/git setup that the demo doesn't
# hit any first-run prompts. `THEME` (default `gruvbox-dark`) picks which
# built-in theme the whole recording is shot in — the docs site uses this
# to produce one hero demo video per theme, so the on-site theme switcher
# can swap the video to match your selection.
mkdir -p "$CFG"
cat >"$CFG/config.toml" <<EOF
[general]
default_notebook = "personal"

[theme]
name = "$THEME"

[git]
auto_commit = false
auto_push = false

# line_numbers/multi_cursor default off (see [editor] in shiki-config) —
# flipped on here so Phase 13's editor screenshots show these features
# active rather than recording the (invisible-by-design) off state.
# mouse_selection/find_replace already default to true.
[editor]
line_numbers = true
multi_cursor = true
EOF

# --- Commit everything so the footer shows a clean synced state instead of
# a distracting dirty marker throughout the recording.
for nb in personal work research; do
  git -C "$DATA/$nb" add -A
  git -C "$DATA/$nb" commit -q -m "seed demo data"
done

mkdir -p "$(dirname "$OUT")"

TAPE="$WORK/demo.tape"
# The tape heredoc is *quoted* (`<<'TAPEEOF'`) so nothing inside is expanded
# by bash — the tape's explanatory comments are full of backticks (`code`)
# and `$VAR` references, and in an unquoted heredoc bash runs those as
# command substitutions (a pre-existing bug that spewed "command not found"/
# "unbound variable" errors into every recording run and could even corrupt
# the tape after a syntax error). Only four real values need the shell's
# expansion, so they're written as `__PLACEHOLDER__` and substituted by the
# `sed` right after the heredoc.
cat >"$TAPE" <<'TAPEEOF'
Output "__OUT__"
Set Shell "bash"
Set FontFamily "__NERD_FONT__"
Set FontSize 15
Set Width 1200
Set Height 750
Set Padding 0
Set TypingSpeed 30ms
Set WaitTimeout 5s

Hide
# `directories::ProjectDirs::from("", "", "shiki")` (shiki-config) appends
# its own "shiki" segment onto whatever `$XDG_CONFIG_HOME`/`$XDG_DATA_HOME`
# is given — so these must be the *parent* of `$CFG`/`$DATA` (which
# already end in "/shiki" themselves), not `$CFG`/`$DATA` directly. A real
# bug caught live while adding Phase 13 below: an earlier version of this
# line passed `$CFG` as-is, so the app was actually reading/writing
# `$WORK/config/shiki/shiki/config.toml` — one level too deep, and thus
# never the hand-written config this script generates at all. It went
# unnoticed for a while because `Config::default()`'s own values
# (gruvbox-dark theme, "personal" as the default notebook, auto_push
# false) happen to coincidentally match what the hand-written config also
# specifies — until `[editor] line_numbers`/`multi_cursor = true` was
# added and *didn't* match its own (false) default, which is what actually
# exposed this. `XDG_DATA_HOME` already did this correctly (`$WORK/data`,
# not `$DATA`); only `$XDG_CONFIG_HOME` had the bug.
Type "XDG_CONFIG_HOME='__WORK__/config' XDG_DATA_HOME='__WORK__/data' '__BIN__'"
Enter
Sleep 1200ms
Show

# --- Phase 1: notebooks tour (NOTEBOOKS focus, cursor starts on
# alphabetically-first "personal"; NotebookStore::list sorts by name, same
# as list_dir does for folders/notes within one — confirmed against
# shiki-core/src/notebook.rs before writing this, not assumed).
Sleep 500ms
Down@150ms 2
Sleep 300ms
Up@150ms 2
Sleep 400ms
Enter

# --- Phase 2: browse personal's root list fast, open a note with real
# content, scroll it, then explicitly return to NOTES (Left, not Escape —
# Escape only closes popups/cancels leader, it does NOT move focus out of
# PREVIEW; only h/Left does).
Sleep 400ms
Down@100ms 4
Sleep 300ms
Enter
Sleep 500ms
Down@100ms 6
Sleep 600ms
Left

# --- Phase 3: descend two folder levels deep, open a note there, then
# ascend all the way back out (Left ascends one folder level at a time
# before finally falling back to switching focus, once at notebook root).
# Cursor is at index 4 ("Morning routine") after Phase 2's Left — VHS has
# no dedicated Home/End key command, so this moves back to index 0
# (journal/) the same way End-of-Phase-2's Down got there, just reversed.
Up@150ms 4
Sleep 300ms
Down@150ms 1
Sleep 300ms
Enter
Sleep 500ms
Enter
Sleep 500ms
Down@150ms 1
Sleep 300ms
Enter
Sleep 900ms
Left
Sleep 300ms
Left
Sleep 300ms
Left
Sleep 400ms

# --- Phase 4: fuzzy jump within the notebook. `PendingInput::Search`'s
# confirm handler (app.rs) sets `self.focus = Focus::Notes` directly —
# unlike the tags/global-search jumps, it does NOT land in PREVIEW — so
# no extra `Left` is needed (or wanted) here. An earlier version of this
# script added one anyway on the wrong assumption it behaved like those
# other jumps; since we're already back at notebook root at this point
# (the jumped-to note has no parent folder), that stray `Left` — with
# focus already on NOTES and notes_path already empty — fell through to
# `navigate_backward`'s panel-switch fallback and silently flipped focus
# to NOTEBOOKS for the rest of the recording (invisible for several
# phases since leader-bound modals work regardless of focus, until
# Phase 7's Down@150ms 4 exposed it as a wraparound notebook jump).
Type "/"
Sleep 300ms
Type "hiking"
Sleep 700ms
Enter
Sleep 900ms

# --- Phase 5: global fuzzy search across every notebook, followed by an
# actual jump (Enter, not just Escape) to prove it really crosses
# notebooks instead of merely opening a box that closes again. "Acme" is
# a one-off string in this dataset (only "work > clients > Acme Corp"
# contains it — grepped before relying on it), so it's a deterministic
# top hit regardless of nucleo's exact scoring internals. After landing
# on it (focus flips to PREVIEW, notebook flips to "work"), three Lefts
# walk back out: PREVIEW->NOTES, pop the clients/ folder to work's root,
# then NOTES-at-root->NOTEBOOKS (navigate_backward's panel-switch
# fallback, same mechanic Phase 4's old bug abused by accident) — then
# Up@150ms 2 hovers back to "personal" (work=idx2 -> research=idx1 ->
# personal=idx0, each hover live-reloading notes/preview same as Phase 1),
# and Enter re-enters it so every later phase starts from the same known
# state Phase 6 already assumed.
Space
Type "g"
Sleep 400ms
Type "Acme"
Sleep 700ms
Enter
Sleep 900ms
Left
Sleep 300ms
Left
Sleep 300ms
Left
Sleep 300ms
Up@150ms 2
Sleep 300ms
Enter
Sleep 500ms

# --- Phase 6: tags panel — two levels deep (tag list, then notes carrying
# it); one Escape only backs out of level 2, a second is needed to fully
# close the modal. Extra settle time (600ms, not 300ms) after each Escape
# here — this transition is where a flaky run once let a stray keystroke
# get misdelivered to the wrong modal (a still-active overlay eating a key
# meant for the next phase), so every modal close in this script now gets
# a full render-loop cycle (shiki polls at ~100ms) plus margin to actually
# settle before the next keystroke fires.
Space
Type "T"
Sleep 500ms
Down@150ms 3
Sleep 500ms
Enter
Sleep 700ms
Escape
Sleep 600ms
Escape
Sleep 600ms

# --- Phase 7: real multi-select — anchor Mode::Visual on "Gift ideas"
# (index 3 in the combined folders++notes list: journal/, projects/, Book
# recommendations, Gift ideas, ...), extend one row down to "Morning
# routine", then batch-delete both with `d` + confirm (`y`). Unlike the
# old version of this phase (select, then just Escape with nothing to
# show for it), this actually exercises the delete path — apply_batch_
# delete resets Mode to Normal and reloads notes itself, so the very next
# phase can rely on a clean Mode::Normal/NOTES state with no keys wasted
# undoing anything.
Down@150ms 3
Sleep 300ms
Type "v"
Sleep 400ms
Down@150ms 1
Sleep 500ms
Type "d"
Sleep 500ms
Type "y"
Sleep 700ms

# --- Phase 8: create a folder ("f", NewFolder) — folders could only be
# navigated, never created, until this action existed. confirm_input's
# NewFolder arm auto-selects the freshly created folder by re-deriving
# its position in the (re-sorted) folder list, so the very next phase can
# assume it's already selected with no extra navigation.
Type "f"
Sleep 400ms
Type "archive"
Sleep 400ms
Enter
Sleep 700ms

# --- Phase 9: move that folder into personal/projects/ ("m" — MoveNote,
# broadened to also handle a selected folder). The prompt prefills with
# "{notebook}/{breadcrumb}" — just "personal" at root — and the input box
# only ever appends/backspaces (no cursor movement), so typing
# "/projects" after the prefill targets the existing projects/ folder as
# the new parent; copy_folder_to/move_folder_to append the folder's own
# name to that destination, so this doesn't need to be spelled out.
Type "m"
Sleep 400ms
Type "/projects"
Sleep 400ms
Enter
Sleep 700ms

# --- Phase 10: create a real note and write a full, multi-section body,
# using the inline editor's `/`-menu (v0.8.3+) for headings/bullets/a
# checklist item instead of hand-typing that markdown. Two real behaviors
# to get right here, both verified against the current source before
# scripting them (an earlier version of this exact phase silently broke
# because it assumed the pre-v0.8.1 flow — see the file header):
#   1. "a" (NewNote) no longer drops straight into Mode::Edit — confirming
#      the title now opens a template picker first (`open_template_picker`,
#      added in v0.8.1) with "blank" pre-selected at index 0. A second
#      `Enter` is required to actually confirm "blank" and land in the
#      inline editor; without it, every subsequent keystroke here would be
#      silently swallowed by the picker's own limited key handling instead
#      of typed into the note.
#   2. `/` only opens the quick-block menu when it's the very first
#      character of the *current line* (`at_line_start` checks the cursor
#      is at column 1 right after inserting it) — each `/xyz` below is
#      therefore typed at the start of a fresh line, never mid-sentence.
#      Typing the trigger narrows `slash_menu_filtered()` same as the
#      which-key/global-search filters already do; `Enter` applies the
#      single remaining match and replaces the typed `/xyz` with the
#      command's real body, cursor included.
# TypingSpeed is cranked way up for just this block — at the demo's normal
# 30ms/char this much text would take the better part of a minute, working
# against the "shiki is fast" point the whole recording is trying to make.
Set TypingSpeed 6ms
Type "a"
Sleep 400ms
Type "Release day retrospective"
Sleep 400ms
Enter
Sleep 500ms
Enter
Sleep 600ms
Type "/h1"
Sleep 500ms
Enter
Type "What shipped today"
Enter
Enter
Type "Spent the day wiring up the expanded demo recording -- way more surface"
Enter
Type "area than the first pass: settings, snippets, new themes, folder moves,"
Enter
Type "batch deletes, a freshly written note (this one), and a git commit."
Enter
Enter
Type "/h2"
Sleep 400ms
Enter
Type "What went well"
Enter
Enter
Type "/bullet"
Sleep 400ms
Enter
Type "Multi-select finally gets used for something real instead of just"
Enter
Type "  toggling on and off with nothing to show for it."
Enter
Type "/bullet"
Sleep 300ms
Enter
Type "Moving a folder into a different parent worked first try once the"
Enter
Type "  destination path was right."
Enter
Enter
Type "## What was fiddly"
Enter
Enter
Type "- Getting the exact keystroke count right for deep navigation without a"
Enter
Type "  search to fall back on -- folders aren't reachable via jump-search,"
Enter
Type "  only notes."
Enter
Enter
Type "## Tomorrow"
Enter
Enter
Type "/check"
Sleep 400ms
Enter
Type "Wire this whole thing into CI so it regenerates automatically per"
Enter
Type "  release, same as the screenshots already do."
Sleep 900ms
Set TypingSpeed 30ms
Escape
Sleep 700ms

# --- Phase 11: git sync ("s", notebook-scoped — resolves regardless of
# focus). Commits every pending change from phases 7-10 (batch delete,
# folder create+move, new note) in one go and reports it in the footer's
# dirty count. No remote is configured for this demo's notebooks, so this
# only commits — `auto_push` is false in the generated config, matching
# the resolved-policy behavior manual `s` always respects (unlike `u`,
# which force-pushes regardless).
Type "s"
Sleep 1800ms

# --- Phase 12: Settings screen (leader+`s`, v0.8.5) — every tab is
# genuinely interactive now, not read-only like the first cut in v0.8.4.
# `Left`/`Right` always switches tabs regardless of how deep a drill-down
# is (checked before the drill-state branch in `handle_settings_key`),
# demonstrated below by jumping straight from NOTEBOOKS level 2 to SNIPPETS
# without backing out first. Every toggle exercised here is flipped back to
# its starting value before moving on (use_favorite_editor, auto_push, the
# notebook's own auto_push override, EDITOR's mouse_selection) so this
# phase leaves no side effect behind except the one it's actually meant
# to: a real new `/`-menu snippet, created from scratch entirely from
# within Settings. Tab order is GENERAL -> THEME -> GIT -> EDITOR ->
# NOTEBOOKS -> SNIPPETS — EDITOR sits between GIT and NOTEBOOKS, so every
# tab-switch count below accounts for it.
Space
Type "s"
Sleep 700ms

# GENERAL: down to use_favorite_editor (row 4 of 4) and flip it on, then
# off again — booleans toggle in place on `Enter`, no confirmation needed.
Down@200ms 3
Sleep 500ms
Enter
Sleep 600ms
Enter
Sleep 600ms

# THEME: `name` opens the exact same theme picker leader+`c` does (not a
# duplicate) — browse a couple of entries live, then `Esc` cancels back to
# the committed theme and reopens Settings automatically on THEME
# (`reopen_settings_after_theme_picker`).
Right
Sleep 600ms
Enter
Sleep 600ms
Down@200ms 2
Sleep 600ms
Escape
Sleep 600ms

# GIT: toggle the global auto_push default on/off in place, then open (and
# cancel) commit_prefix to show a text field's prompt-prefilled-with-the-
# current-value behavior. Every tab switch below gets a full 600ms+ settle
# before the next keystroke, not just a quick 300-400ms — this phase
# follows right on the heels of Phase 11's background git-sync thread
# finishing up, and on one run a `Down` racing that transition landed the
# following `Enter` on row 0 (auto_commit) instead of row 1 (auto_push),
# silently flipping the wrong boolean; same "give a transition a full
# settle cycle" fix already applied to the tags-modal close in Phase 6,
# just needed more headroom here since a background thread is involved
# too, not only modal-open/close rendering.
Right
Sleep 600ms
Down@200ms 1
Sleep 500ms
Enter
Sleep 600ms
Enter
Sleep 600ms
Down@200ms 1
Sleep 500ms
Enter
Sleep 600ms
Escape
Sleep 600ms

# EDITOR (new tab, sits between GIT and NOTEBOOKS — every tab count below
# that used to jump straight from GIT to NOTEBOOKS with a single `Right`
# has to know about this or it silently lands one section short: caught
# for real by running this exact recording once before finalizing it —
# the original single-`Right` GIT->NOTEBOOKS jump landed on EDITOR
# instead, and the NOTEBOOKS-drill keystrokes that followed then
# misfired straight into EDITOR's own flat toggle list, in one run
# actually typing "PR review checklist" into personal's git remote text
# prompt). mouse_selection toggles off then back on in place, same
# boolean-flips-immediately-on-Enter behavior every other tab's booleans
# already have.
Right
Sleep 600ms
Enter
Sleep 600ms
Enter
Sleep 600ms

# NOTEBOOKS: level 1 lists every real notebook with its actual git remote
# (redacted) — drill into "personal" (alphabetically first), cycle its
# auto_push override through the full inherit -> true -> false -> inherit
# 3-state sequence (3 Enters, landing back at unset), then jump straight to
# SNIPPETS with `Right` instead of backing out of the drill first.
Right
Sleep 600ms
Enter
Sleep 600ms
Down@200ms 1
Sleep 500ms
Enter
Sleep 500ms
Enter
Sleep 500ms
Enter
Sleep 600ms

# SNIPPETS: create a brand-new `/`-menu command from scratch — "a" prompts
# for a trigger and drills straight into the new (empty) snippet; "enter"
# on label opens a text prompt; "enter" on body opens the very same inline
# editor a note's own body uses (`Mode::Edit`, tracked via
# `App.editing_snippet` instead of a note path). Once saved, this snippet
# is a real, usable `/review` entry in every note's `/`-menu from now on.
Right
Sleep 600ms
Type "a"
Sleep 500ms
Type "review"
Sleep 500ms
Enter
Sleep 600ms
Enter
Sleep 500ms
Type "PR review checklist"
Sleep 500ms
Enter
Sleep 600ms
Down@200ms 1
Sleep 500ms
Enter
Sleep 700ms
Type "- [ ] Approach makes sense"
Enter
Type "- [ ] Tests added or updated"
Enter
Type "- [ ] Docs / CHANGELOG updated"
Sleep 700ms
Escape
Sleep 700ms
Escape
Sleep 600ms

# --- Phase 13: native editor mouse/keyboard UX overhaul (mouse selection,
# find/replace, real OS clipboard, full multi-cursor, all opt-in via
# [editor], flipped on above in the generated config). Jumped to by global
# search (leader+`g`, the same cross-notebook mechanism Phase 5 already
# uses for "Acme") rather than local navigation, so this phase doesn't
# depend on tracking exactly which notebook/folder everything before it
# left selected.
#
# A real bug caught live building this phase: Phase 12's own trailing two
# `Escape`s (above) only back out of the SNIPPETS drill-down to its level-1
# list — they don't close the Settings modal itself, which was still open
# on SNIPPETS the whole time. `Space` and the first few letters of
# "errands" landed on that still-open modal instead of the leader key:
# `Space`/`g`/`e`/`r`/`r` are all no-ops at SNIPPETS level 1 (not bound to
# anything there) *except* `a`, which is level 1's own "new snippet"
# binding — it fired, opened the trigger prompt, and swallowed the
# remaining letters of "errands" ("nds") as the typed trigger name,
# creating a real (garbage) `/nds` snippet. One more `Escape` here — for
# real this time at level 1, where `Esc`/`q` does close the whole modal —
# fixes it.
#
# A second, sneakier bug caught the same way, unrelated to Settings: the
# demo note originally lived at personal/'s root, sorted alphabetically —
# which put it *before* "Gift ideas" there, shifting every later note's
# index by one. Phase 7 (written long before this note existed)
# batch-deletes by a hardcoded index that assumed "Gift ideas" specifically
# lived at that position, so once the index shifted, it silently deleted
# this note instead. By the time this phase ran, the note was simply gone
# — global search's top-ranked result was some unrelated note purely by
# coincidence, which looked at first like a *ranking* bug rather than a
# *the-note-doesn't-exist-anymore* bug. Moved the note to research/
# instead (see setup_sample_data) — never touched by index-dependent
# navigation anywhere in this recording, so it can't collide with an
# earlier phase's assumptions the way personal/'s root can.
Escape
Sleep 500ms
Space
Type "g"
Sleep 400ms
Type "errands"
Sleep 700ms
Enter
Sleep 700ms
Type "i"
Sleep 500ms
# Ctrl+D: 1st press selects the word under the cursor (no new cursor
# yet), each press after that adds the next occurrence as its own cursor
# — three identical "TODO"s means three presses select all of them, no
# pixel-coordinate mouse math or counted arrow-key navigation needed at
# all. Typing then replaces all three selections at once. Verified this
# exact sequence against a real recording before scripting it here (not
# assumed): it does replace all three, live, in one motion.
Ctrl+D
Sleep 500ms
Ctrl+D
Sleep 500ms
Ctrl+D
Sleep 600ms
Type "DONE"
Sleep 900ms
# `Escape` first collapses the secondary cursors (this app's own
# convention — verified directly: a second, distinct keystroke afterward
# only ever lands at the primary's own position, proving the secondaries
# were cleared rather than the editor having exited) before the find/
# replace demo below, so it isn't left with stale cursor markers from the
# multi-cursor edit. This edit is left in place rather than undone
# afterward — a real, lasting change, the same "show it actually doing
# something" preference every other phase in this recording already
# follows (see Phase 7's own comment).
Escape
Sleep 500ms
# Find/replace: Ctrl+F opens the bar, typing live-jumps to the match —
# genuinely live, not scripted around. Replacing via Ctrl+Enter/
# Ctrl+Alt+Enter is deliberately not demonstrated here: verified directly
# that this terminal stack (ttyd, the same one VHS itself records
# through) can't distinguish Ctrl+Enter from plain Enter without an
# extended keyboard protocol neither ttyd nor most real terminals
# implement, so scripting a "replace" demo around it would be recording
# a keystroke that doesn't reliably do the same thing on a real viewer's
# own terminal either.
Ctrl+F
Sleep 400ms
Type "dentist"
Sleep 900ms
Escape
Sleep 500ms
Escape
Sleep 600ms

# --- Phase 13b: wikilink autocomplete (v0.8.10) -- jump to Phase 10's own
# "Release day retrospective" note via global search (same mechanism Phase
# 5/13 already use, so this doesn't depend on tracking exactly which
# notebook/folder Phase 13 left selected). Cursor lands at (0, 0) on
# entering edit mode (same assumption Phase 13's own multi-cursor demo
# already relies on for "errands"), so typing directly there prefixes the
# note's first heading line rather than needing a dedicated "jump to end"
# navigation step. Typing the second bracket opens the picker mid-line --
# a wikilink is meaningful anywhere in a line, not just at its start,
# unlike the slash-command menu. "hik" fuzzy-filters live down to "Weekend
# hiking trip" (the same nucleo engine the slash jump and global search
# already use) before Enter inserts the resolved link; a second Enter then
# splits it onto its own line ahead of the original heading text.
Space
Type "g"
Sleep 400ms
Type "retrospective"
Sleep 700ms
Enter
Sleep 700ms
Type "i"
Sleep 500ms
Type "See [["
Sleep 500ms
Type "hik"
Sleep 700ms
Enter
Sleep 900ms
Enter
Sleep 400ms
Escape
Sleep 600ms

# --- Phase 14: theme picker, standalone (leader+`c`) — live-cycle through
# two of the new catalog's flagships (Cyberpunk 2077, then LoL (Jinx))
# before cancelling back to the notebook's actual gruvbox-dark. The list is
# alphabetical now, straight from `shiki-config/src/themes/mod.rs`'s
# `all()`: gruvbox-dark sits at index 8, Cyberpunk 2077 at index 3 (5 Ups
# up), and LoL (Jinx) at index 16 (13 Downs back down).
Space
Type "c"
Sleep 500ms
Up@200ms 5
Sleep 500ms
Down@200ms 13
Sleep 700ms
Escape
Sleep 500ms

# --- Phase 15: which-key / command palette, quick glimpse.
Type "?"
Sleep 900ms
Escape
Sleep 900ms

# --- Phase 16: global tasks view (leader+t, works from any focus) -- the
# gear checklist on Weekend hiking trip now carries overdue/today/future
# due tags (see the DUE_OVERDUE/DUE_TODAY/DUE_FUTURE setup above), so the
# urgency-sorted list shows all three colors at once without any extra
# setup. Enter toggles the highlighted task done in place.
Space
Type "t"
Sleep 800ms
Down@200ms 2
Sleep 500ms
Enter
Sleep 700ms
Escape
Sleep 500ms

# --- Phase 17: links modal mention repair -- the notes-scope "/" jump
# (title-only fuzzy match, same mechanism Phase 4 already uses) finds
# Weekend hiking trip unambiguously by title; global search (leader+g)
# would also match Book recommendations' own plain-text mention of it by
# body text, and -- once Phase 18 below creates today's daily note, whose
# injected agenda links back to this same note by title -- would start
# matching that too, so this deliberately stays a title-only jump instead.
# Left x3 resets focus to NOTEBOOKS, Right moves into NOTES (same reset
# pattern the earlier phases already rely on). leader+B opens the links
# modal -- this note already has a real Backlink from Phase 13b's own
# "Release day retrospective" wikilink, so the mention row isn't the
# first one selected; one Down reaches "Book recommendations" under
# "Mentions (unlinked)" before "c" repairs it into a real backlink.
Left
Left
Left
Sleep 400ms
Right
Sleep 400ms
Type "/"
Sleep 400ms
Type "hiking"
Sleep 700ms
Enter
Sleep 700ms
Space
Type "B"
Sleep 700ms
Down
Sleep 500ms
Type "c"
Sleep 800ms
Escape
Sleep 500ms

# --- Phase 18: daily note agenda -- Left x3 resets focus to NOTEBOOKS,
# Right moves into NOTES, then "t" (notes-scope daily note) creates
# today's daily. It opens with a "Due today" section already injected,
# listing every overdue/due-today task across every notebook -- Right
# moves focus into PREVIEW to read it, Down scrolls a little further into
# that section.
Left
Left
Left
Sleep 400ms
Right
Sleep 400ms
Type "t"
Sleep 700ms
Right
Sleep 700ms
Down@150ms 4
Sleep 900ms

# --- Phase 19: real syntax highlighting -- global search to the Retry
# helper snippet note, landing straight in PREVIEW with its fenced Rust
# code block rendered in real per-token color instead of flat dimmed text.
Space
Type "g"
Sleep 400ms
Type "retry helper"
Sleep 700ms
Enter
Sleep 900ms

# --- Phase 20: the 0.9.1 PREVIEW-rendering showcase -- global search to
# research/'s "Release day notes" (see setup_sample_data), landing straight
# in PREVIEW the same way Phase 19 does, so the code-fence header row (▌ tsx
# App.tsx) with line-number gutter, the collapsible <details>/<summary>
# block, the prettified $$∫₀^∞ e⁻ˣ² dx = √π/2$$ math, and the mermaid
# flowchart all appear on one note. "rollup" is the deliberate query, NOT
# "release day" — Phase 10 of this very recording creates a note titled
# "Release day retrospective" in personal/, which fuzzy-matches "release
# day" at least as well as this note does, making the top hit non-
# deterministic. "rollup" (from the note's "## Feature rollup" heading)
# only ever matches this one note in the whole dataset, so it's a
# deterministic first hit. Clicking the summary row to fold/unfold is mouse-
# driven and deliberately not scripted here (VHS mouse coordinates would
# need per-terminal calibration) — the collapsed state is what the note
# opens in anyway.
Space
Type "g"
Sleep 400ms
Type "rollup"
Sleep 700ms
Enter
Sleep 1200ms

# Quit off-screen — ending on the app itself, not a bare shell prompt with
# the XDG_CONFIG_HOME/XDG_DATA_HOME launch command sitting there.
Hide
Type "q"
Sleep 300ms
TAPEEOF
# Substitute the four placeholders back to their real values (see the
# comment above the heredoc for why the heredoc itself is quoted).
sed -i "s|__OUT__|$OUT|g; s|__NERD_FONT__|$NERD_FONT|g; s|__WORK__|$WORK|g; s|__BIN__|$BIN|g" "$TAPE"

echo "Recording with VHS..."
vhs "$TAPE"

echo "Wrote $OUT"

# The docs site's hero uses an mp4 `<video>` (autoplay/reduced-size), not the
# raw GIF — transcode it here so a release lands both artifacts at once. Only
# the frame timing of the GIF is carried over; a fast-start mp4 with
# yuv420p plays on every browser. `ffmpeg` is a vhs runtime dep already.
MP4="${OUT%.gif}.mp4"
ffmpeg -y -loglevel error -i "$OUT" -movflags +faststart -pix_fmt yuv420p \
  -vf "scale=trunc(iw/2)*2:trunc(ih/2)*2" "$MP4"
echo "Wrote $MP4"

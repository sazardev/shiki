# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**shiki** (私記) is a TUI note-taking app written in Rust, inspired by Yazi's three-pane
layout and modal navigation. Notes are plain Markdown files with YAML frontmatter,
organized into "notebooks" (directories, each its own git repo). Full design spec,
motivation, keybindings, config format, and included themes are documented in `IDEA.md` —
read it before making architectural changes, since it's the source of truth for intended
behavior (layout, CLI commands, config schema, etc).

## Commands

```sh
cargo build --workspace              # build everything
cargo check --workspace              # fast type-check (use this while iterating)
cargo clippy --workspace --all-targets   # lint; keep this clean before considering work done
cargo fmt --all                      # format (run after editing, before checking clippy)
cargo run -p shiki-cli -- <args>     # run the binary, e.g. `-- new "titulo"`, `-- daily`, no args launches the TUI
```

There are ~293 `#[test]`s: 134 in `shiki-core`, 17 in `shiki-config`, 129 in `shiki-tui`, 13 in
`shiki-cli` — `cargo test --workspace` is green. They're all inline `#[cfg(test)]` modules inside
the source files (no `tests/` dirs, no `#[ignore]`, no fixture setup), so the pattern set by
`panel_drawer::tests` (`shiki-tui/src/panel_drawer.rs`) — covering `drawer_hit_at`'s mouse
coordinate math as a plain function of numbers, not `&App` — is the norm. When adding tests,
put them as `#[cfg(test)]` modules in the relevant file; `shiki-core`/`shiki-config` have no
TUI/terminal dependency, so they're the easiest to unit test. For `shiki-tui`, prefer designing
the function to not need `&App` in the first place (as `drawer_hit_at` does) over constructing a
full `App` in a test.

To exercise the CLI without touching the real user config/data, override XDG dirs (used via
`directories::ProjectDirs::from("", "", "shiki")` in `shiki-config`):

```sh
XDG_CONFIG_HOME=/tmp/shiki-test-config XDG_DATA_HOME=/tmp/shiki-test-data \
  cargo run -p shiki-cli -- notebook create personal
```

## Versioning

`[workspace.package] version` in the root `Cargo.toml` is the single version number for the whole
app — every crate inherits it via `version.workspace = true`, so there's exactly one place to bump.
The TUI status bar shows it (right-aligned in the footer, paired with the `? help` hint) via
`env!("CARGO_PKG_VERSION")` in `shiki-tui/src/status_bar.rs`, which reads shiki-tui's own
(inherited) manifest version at compile time. Cutting a release is two steps: bump the workspace
version, add a `CHANGELOG.md` entry (follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)).

**Tagging the release is automatic, not a manual `git tag && git push` step anymore.**
`.github/workflows/auto-tag.yml` watches every push to `main`; if the triggering commit's message
contains `[PUBLISH]`, it reads the version straight out of the root `Cargo.toml`, checks
`git ls-remote --tags origin` to confirm that tag doesn't already exist, and pushes it itself. This
exists because v0.7.0 was tagged locally but the `git push origin v0.7.0` step was never actually
run — the tag sat on nobody's remote, `release.yml` (which only triggers on a pushed `v*` tag)
never ran, and crates.io silently stayed on 0.5.0 for three version bumps before anyone noticed.
Pushing the tag with the default `GITHUB_TOKEN` would silently not work either: GitHub suppresses
workflow triggers caused by a token a workflow already used, specifically to prevent infinite
trigger loops, so a tag pushed that way would never fire `release.yml`. `auto-tag.yml` instead
pushes using a `RELEASE_TAG_PAT` secret (a personal access token with repo write access) so the tag
push looks like it came from a real user. **Both `RELEASE_TAG_PAT` and `AUR_SSH_PRIVATE_KEY` are
configured as of v0.9.1** (verified live: auto-tag pushed the tag, AUR got both packages) — the
"doesn't exist yet" note below predates that provisioning. If a future run still fails on a missing
secret, `auto-tag.yml` fails loudly with a clear `::error::` annotation rather than silently
no-op'ing, so a missing secret can't reproduce the exact "nobody noticed" failure mode it exists to
prevent. To cut a release: bump the version, add the changelog entry, and include `[PUBLISH]`
anywhere in the commit message that lands on `main` — everything downstream (tag → build → GitHub
Release → crates.io → packaging manifests) follows automatically.

**The full, verified runbook for cutting a release — including the manual fallbacks that are part
of the real flow — lives in the `deploy` skill (`.opencode/skill/deploy/SKILL.md`), exposed as the
`/deploy` command.** Read that before cutting a release; the three non-obvious gotchas it encodes,
all confirmed during the v0.9.1 cut:

- **`release.yml` sometimes never fires on the tag push.** auto-tag pushes the tag via
  `RELEASE_TAG_PAT` (which is the point — a real-user push bypasses the GITHUB_TOKEN trigger
  suppression), but the tag push *itself* has been observed not to start the workflow (v0.8.4 and
  v0.9.1 both). The documented manual fallback is `gh workflow run release.yml --ref main --field
  tag=v0.9.1` — dispatch it if no Release run appears ~1–2 min after auto-tag finishes. The
  `resolve-tag` job exists precisely to make this dispatch path equivalent to a real tag push.
- **`update-packaging-manifests` fails at "Push Homebrew tap" if `RELEASE_TAG_PAT` can't write to
  `sazardev/homebrew-shiki`** (a 403 on the clone — confirmed on v0.9.1). Everything else in that
  job (Scoop manifests, both PKGBUILDs, the main-repo commit) already succeeded by then; only the
  tap formula push needs doing manually: copy `packaging/homebrew/shiki.rb` into the tap repo's
  `Formula/`, commit "Update to vX.Y.Z", push.
- **`update-screenshots` and `update-release-pages` are `needs:`-gated on
  `update-packaging-manifests`** — when that job fails at the Homebrew step, both get *skipped*,
  leaving the site's screenshots/demo-gif/release-page/OG-card/feed stale. Run them locally instead
  (they need the local toolchain — xterm/ImageMagick/xdotool/Xvfb/vhs/ttyd/ffmpeg/chromium): execute
  `scripts/screenshots.sh` + `scripts/demo-gif.sh`, copy into `docs/assets/` exactly as
  `release.yml` does, run `scripts/generate_release_pages.py --version X.Y.Z --chromium …`, then
  commit. The deploy skill spells out these exact steps.

**The status bar paints no background** (`status_bar.rs` — no `.bg(...)` anywhere, spans just use
themed fg colors on the terminal's own background) and only shows what's contextually useful: the
mode label is omitted entirely in `Mode::Normal` (only INSERT/EDIT/VISUAL are worth flagging),
there's no theme-name display, and the metadata block is focus-dependent — character count of the
selected note while reading/editing one (`Focus::Notes`/`Focus::Preview` with a note selected), or
the note count in view otherwise (e.g. still browsing NOTEBOOKS). Git info comes from
`App::git_status: shiki_core::git::GitStatus` — `git::status(path, remote)` resolves `HEAD`'s
branch name and runs `repo.graph_ahead_behind` against `refs/remotes/{remote}/{branch}` (from the
last fetch — it doesn't talk to the network itself). All three counts render together when
nonzero rather than only ever showing one: `+{dirty_count}` (uncommitted files), `{UPLOAD
icon}{ahead}` (local commits not yet pushed), `{DOWNLOAD icon}{behind}` (remote commits not yet
pulled in) — a diverged branch after a `pull` can genuinely show both ahead *and* behind at once,
verified by pushing a divergent commit from a second clone and pulling: `main +0 ↑1 ↓1`. Don't
collapse this back to a single dirty/clean marker; the counts were added specifically because a
bare marker didn't say how much was pending.

**The notebook drawer (`leader+b`, `Action::ToggleDrawer` → `App::toggle_drawer`) is a true
toggle, unlike `open_logs`/`open_tree`/most other modals.** Those are one-directional (opened by
their action, only closed via `Esc` inside their own key handler); the drawer's leader binding
opens *and* closes it, since it was asked for as something to "abrir o descolapsar" with the same
key. It's also the first overlay in this codebase that's **left-anchored** rather than
center-flexed: every other popup (`centered_rect`, used by the theme picker/logs/tree/history/
tags/confirm) sits in the middle of the screen, but `App::drawer_area` builds a fixed-width
(`DRAWER_WIDTH`) `Rect` pinned to `x: 0` instead, so it reads as a persistent sidebar rather than a
dialog — deliberately not using `centered_rect` for this one. **While the drawer is open, the
panels are *pushed* right by `drawer_width` rather than covered: `layout::split` takes the drawer's
width as an extra argument (`App::drawer_offset`, 0 when closed) and shifts `main` right by it,
because a pure overlay otherwise hid the left edge of whatever panel sat underneath — the first
~28 columns of every PREVIEW line (e.g. the first half of a long math formula) disappeared behind
the drawer.** `App::drawer_statuses: Vec<(String, GitStatus)>` (every notebook, not just the
selected one — contrast with `git_status`, which is
just the selected notebook) is only populated while `show_drawer` is true (`refresh_drawer_statuses`,
called from `toggle_drawer` and again from `refresh_git_status`/`apply_git_op_result` whenever it's
open), since computing it for every notebook on every draw tick would be pure waste when nothing's
showing it — same "compute once, not per draw" discipline as `history_count_cache`/
`note_preview_cache`/`folder_preview_cache`.

**`render::git_status_color`/`git_status_suffix` are shared by the footer and the drawer** — pulled
out of `status_bar.rs` specifically so the same notebook can't show a different color/count in the
footer than in the drawer because one of the two was hand-edited and the other wasn't. The color
priority (dirty outranks ahead/behind, which outranks clean) was already the footer's rule before
this was extracted; the drawer just reuses it per-row instead of only for the selected notebook.

**NOTES rows are colored by `App.note_statuses: HashMap<PathBuf, shiki_core::git::FileGitStatus>`**
(new→`theme.success`, modified/renamed→`theme.warning`, deleted→`theme.error`, absent from the map
at all→plain `theme.fg`) — `shiki_core::git::file_statuses(path)` is the same `repo.statuses(None)`
walk `diff_summary`/`git_status_color` already do, just bucketed per path instead of into one
summary string or one aggregate count. Refreshed in lockstep with `git_status` (same
`refresh_git_status` call sites — `reload_notes`/`refresh_notes_preserve_selection`), so it needed
no new invalidation logic of its own.

**Mouse hit-testing functions take plain coordinates/counts, not `&App`, specifically so they're
unit-testable without constructing a full `App`.** `panel_drawer::drawer_hit_at(notebook_count:
usize, area: Rect, column: u16, row: u16)` is the pattern to copy for any future clickable
panel — contrast with the pre-existing `App::global_search_hit_at`, which *does* take `&self` and
is consequently untested. `panel_drawer::tests` (in the same file) caught a real off-by-one in the
button row's coordinate math this way — the button text is the *first* row of the reserved
`Length(BUTTON_ROWS)` area (a `Paragraph` top-aligns, it doesn't center), not the row after it —
before the test existed, the hit-test math and the actual rendered position disagreed by one row.
`App::on_mouse` dispatches a click the same way the equivalent keypress would (`jump_to_drawer_notebook`
for a `DrawerHit::Notebook`, the same `PendingInput::NewNotebook` prompt for either button) rather
than duplicating logic — click and key are just two ways to reach the same handler.

**The footer's spinner (`SPINNER_FRAMES`, `status_bar.rs`) replaces the git-status segment
entirely while `App.sync_in_flight` is `Some`, rather than sitting next to it.** The point is
making it obvious *something's* happening during a slow network call, not showing two
git-status-shaped things at once that are about to disagree the moment the real result comes back.
`App.spinner_frame` advances once per `run()` iteration only while something's in flight (`if
app.sync_in_flight.is_some() { ... }`, right next to `poll_sync_channel`) — it's never reset to 0
when idle, since nobody's watching for an exact starting frame, just that it's moving.

**Notebook-scoped git actions (`Action::SyncNotebook`/`PullNotebook`/`PullAllNotebooks`/
`SetRemote`/`PushNotebook`) resolve regardless of which panel has focus, not only when NOTEBOOKS
does** (`handle_normal_key`'s fallback: if `resolve_scoped(self.focus, code)` misses, it also tries
`resolve_scoped(Focus::Notebooks, code)` and accepts the result only if `is_notebook_git_action`
allows it). These act on "the selected notebook" as a concept, not on "the NOTEBOOKS panel" — `u`
(push) while reading a note in PREVIEW previously did nothing at all, silently, since `u` isn't
bound in the `notes`/`preview` scope maps and the old code never fell back to `notebooks`.
Deliberately NOT extended to `NewNotebook`/`RenameNotebook`/`DeleteNotebook`: those share letters
with Notes-scope actions (`a`/`r`/`d`), so falling back to them from PREVIEW (which has no `d`
binding of its own) would silently delete the whole notebook instead of doing nothing — keep that
whitelist narrow if adding new notebook actions.

**`git::push` takes no branch argument — it pushes whatever `HEAD` actually points at.** It used to
take `branch: &str` and always push `refs/heads/{branch}:refs/heads/{branch}` using
`config.git.branch` (default `"main"`). But `pull`'s branch-fallback (above) means the local repo's
real branch can legitimately be something else, e.g. `master` — pushing the fixed configured name
then failed with "src refspec 'refs/heads/main' does not match any existing object" the moment it
didn't exist locally. `push` now resolves the branch itself via `repo.head()?.shorthand()`, so it's
never out of sync with what `pull` actually created. Don't reintroduce a `branch` parameter here;
if a caller needs to know which branch got pushed, read it off `GitStatus.branch` instead.

## Marketing site (`/docs`, GitHub Pages)

**`/docs` is a plain static HTML/CSS/JS site (no build step, no framework) — deliberately not a
`gh-pages` branch or a generator like Astro/11ty**, since a single marketing site doesn't need
componentization and a build step would be one more thing to keep working. `docs/css/styles.css`'s
theme variables (`[data-theme="..."]` blocks) are copied verbatim from
`shiki-config/src/themes/*.rs` — if a theme's hex values change there, or a new theme is added to
`themes/mod.rs::all()`, update both `styles.css` and the `THEMES` array in `docs/js/main.js` (which
drives the swatch buttons) so the site doesn't silently drift out of sync with the app it's
advertising.

**Deployment is `.github/workflows/pages.yml`, using the GitHub Actions Pages source (not the
classic "deploy from a branch" setting) — chosen specifically so a deploy is a real, observable
workflow run, not an opaque background process with no logs.** It triggers on any push to `main`
that touches `docs/**`, plus `workflow_dispatch` for a manual redeploy. `actions/deploy-pages@v4`
needs `contents: read`, `pages: write`, `id-token: write` (an OIDC token to authenticate the
deployment) — all three, not just `contents: read`, or the deploy step fails with a permissions
error. `concurrency: { group: pages, cancel-in-progress: true }` means a newer push wins over an
in-flight older deploy rather than queuing behind it, since an in-progress deploy of already-stale
content isn't worth finishing. This also means every release's `update-screenshots` commit (which
touches `docs/assets/screenshots/**`) triggers its own Pages deploy automatically — a new
release's refreshed screenshots go live with no separate manual "redeploy the site" step.

**All 12 themes have a real captured screenshot** (`docs/assets/screenshots/*.png`, via
`scripts/screenshots.sh`'s `THEMES` array — originally only 5 were captured; extended to all 12
after Omar pointed out that showing a CSS-only mockup for the other 7 wasn't what "capture every
theme" meant). `#term-fallback`, a small CSS-only mockup of the three-pane layout, still exists as
a genuine fallback (not dead code) for any theme `shiki-config` adds in the future before a
screenshot's been captured for it — `docs/js/main.js`'s per-theme `screenshot` flag controls this,
it's not assumed `true` for every entry. Note `/screenshots` at the repo root is gitignored
(regenerable marketing output, not source) but `docs/assets/screenshots/` is a different,
non-ignored path — the `.gitignore` rule is root-anchored (`/screenshots`), so copies living under
`docs/` are unaffected.

**A real, hard-to-spot bug lived in `docs/css/styles.css` for a while: the theme screenshot never
actually hid.** `.term-window img { display: block; ... }` is *more specific* than the browser's
own default `[hidden] { display: none }` rule, so `main.js` setting `img.hidden = true` did nothing
visually — whatever real screenshot was shown last stayed on screen, stacked above
`#term-fallback`, regardless of which theme was actually selected next. This is exactly why a
theme without a captured screenshot used to appear to "not change anything" when clicked: the
image was still the *previous* theme's, just silently wrong rather than absent. Fixed with an
explicit `.term-window img[hidden] { display: none; }` override. If you add another conditionally-
hidden element anywhere on this site, don't assume the bare `hidden` attribute alone is enough
once *any* CSS rule targets that element with equal-or-higher specificity — verify it actually
disappears, the way this one silently didn't.

**The changelog section fetches `CHANGELOG.md` live from `raw.githubusercontent.com/.../main/...`
at page load rather than duplicating its content into the HTML by hand** — verified live serving
`/docs` locally: it rendered the actual current `CHANGELOG.md` from GitHub, including the
in-progress `## [Unreleased]` section, with no manual sync step. `docs/js/main.js` includes a
small hand-rolled parser for exactly the subset of Keep a Changelog's format this file actually
uses (`##`/`###` headers, `-` bullets, `` `code` ``, `**bold**`, `[text](url)`) rather than pulling
in a general markdown library for a single file with known formatting. A real bug hit and fixed
while building this: CHANGELOG.md hand-wraps long bullets at ~100 columns, sometimes splitting a
single `` `code span` `` across two physical lines — parsing each line independently and
concatenating already-rendered HTML (the first version of this) let an unpaired backtick on one
line pair with the *next* bullet's own backtick, wrapping unrelated plain text in a spurious code
box. Fixed by accumulating a bullet's full raw text across all its continuation lines first and
running it through the inline-formatting regexes only once, at the end.

**The site's screenshots and version number both update automatically on every release, through
two different mechanisms with two different reasons.** The "latest version" shown next to the
download button and in the nav pill is fetched client-side from the GitHub Releases API
(`docs/js/main.js::loadLatestVersion`) on every page load — there's nothing to regenerate or
commit for that one, it's live by construction, same as the changelog fetch above. The
*screenshots* can't work that way (they're real captured images, not text), so
`.github/workflows/release.yml`'s `update-screenshots` job (`needs: release`, so it only runs
after a tag actually produces a real GitHub Release) installs `xterm`/`imagemagick`/`xdotool`/
`xvfb` plus a JetBrainsMono Nerd Font on a fresh `ubuntu-latest` runner, runs
`scripts/screenshots.sh` against that release's own code, copies all 12 themes'
`wide-01-notebooks.png` into `docs/assets/screenshots/`, and commits straight to `main` if
anything changed.

**`README.md`'s own "Screenshots" section reuses these exact same files — it doesn't have its own
screenshot-refresh step, deliberately.** It references `docs/assets/screenshots/{gruvbox-dark,
catppuccin-mocha,tokyo-night-storm,catppuccin-latte}.png` by relative path (renders fine on
GitHub, which resolves image paths relative to the file), so whatever `update-screenshots` already
keeps current for the website is automatically current in the README too, with zero additional
automation. Don't rename or move any of those four files without checking `README.md`'s
`<img>` tags — nothing enforces that link at commit time.

**Both this job and the pre-existing `update-packaging-manifests` job push to `main` using
`secrets.RELEASE_TAG_PAT`, not the default `GITHUB_TOKEN`** — `update-packaging-manifests` used to
push with the default token and that worked fine, until branch protection on `main` (added this
same session — "Require a pull request before merging") started requiring one for every push.
`GITHUB_TOKEN`/`github-actions[bot]` isn't a repo admin, so `enforce_admins: false`'s bypass
doesn't extend to it — only a push authenticated as an actual admin (which `RELEASE_TAG_PAT`,
Omar's own personal access token, is) legitimately bypasses that requirement, the same way a
human admin pushing directly would. Both jobs' `actions/checkout` fall back to `github.token` when
the secret isn't set (so checkout itself never fails), and both fail loudly with an explicit
`::error::` right at the push step instead of failing confusingly deep inside `git push` — the
same "fail loud, not silent" convention `auto-tag.yml` already established for this exact
not-yet-provisioned-secret situation. Don't revert either job to pushing with the default token;
it will appear to work today (a clean run rebuilds the same content and finds nothing to commit)
and only break the next time there's an actual diff to push.

**The Themes section comes right after the hero, not after Features** — Omar's own request, so a
visitor sees the live theme-switching effect within one scroll instead of after reading through
the whole feature list first. The hero's own screenshot got proportionally bigger at the same
time (`.hero-grid`'s columns went from `1.1fr 1fr` to `0.85fr 1.35fr`, plus a wider `.hero
.container` — 1280px instead of the site-wide 1100px), since a bigger, more legible screenshot is
the thing that actually sells "this is a real, detailed terminal app" fastest.

**The hero's download button is a smart, direct-download link, not a static link to the
`/releases/latest` page.** `docs/js/main.js::loadLatestRelease` does one fetch to the GitHub
Releases API (shared with the version-pill logic — one call feeds both, see below), detects the
visitor's OS from `navigator.userAgent`/`navigator.platform`, and rewrites the button's `href`
straight to that platform's actual release asset (`browser_download_url`) — clicking it downloads
the file immediately instead of landing on a page where the visitor still has to pick the right
one by hand. Genuinely detecting Apple Silicon vs. Intel from the main thread isn't reliable
(Safari/Chrome running under Rosetta both still report "Intel" in `navigator.userAgent`), so every
Mac visitor defaults to the `aarch64-apple-darwin` asset — the actual default for any Mac sold
since late 2020; an Intel Mac visitor still has the explicit x86_64 pick in the Install section
below. A failed fetch/detection leaves the button on its static HTML fallback (the
`/releases/latest` page) rather than breaking it — verified live: the button correctly read
"Download for Linux" and its `href` resolved to the real
`.../releases/download/v0.8.0/shiki-v0.8.0-x86_64-unknown-linux-gnu.tar.gz` asset URL when tested
in a Linux-reporting browser.

**Every install command has a one-click Copy button** (`.copy-btn` in `docs/js/main.js`) — reads
the adjacent `<pre>` (Install section cards, via a shared `.code-block` wrapper) or the preceding
`<code>` (the hero's inline `cargo install shiki-cli` hint) and writes it to the clipboard,
flipping the button to "Copied!" for 1.5s. A failed `navigator.clipboard.writeText` (insecure
context, denied permission) fails silently — the command text itself is still fully visible and
selectable by hand either way, so nothing is actually broken by a Clipboard API failure.

**`docs/documentation.html` is a real reference manual, not another marketing page** — every
keybinding table, the CLI command list, the full `config.toml` example, note format, and
filesystem layout, copied verbatim from `IDEA.md` (not paraphrased/summarized) so this page and
the repo can't quietly drift into saying different things about the same behavior. The homepage's
own Documentation section (`#docs` in `index.html`) used to link out to README/IDEA.md/CONTRIBUTING
on GitHub directly; those three now belong to the open-source/contributor side of the project (
`CONTRIBUTING.md` links contributors there already) and were removed from the homepage entirely —
`#docs` is now a short teaser with one button to `documentation.html`, the single dedicated place
for "how do I actually use this." The nav's "Docs" link points straight at
`documentation.html`, not `#docs`, since the real content lives there now.

**`docs/js/main.js` had to become genuinely reusable across pages, not written for `index.html`
alone, once `documentation.html` started including it too** — `applyTheme`/`buildSwatches`/
`loadChangelog` originally called `document.getElementById(...)` and used the result unconditionally
(a real `TypeError: Cannot set properties of null` was hit live building this, since
`documentation.html` has no `#theme-screenshot`/`#term-fallback`/`#changelog-content`). Each of
those now guards on the element actually existing before touching it, so the same `main.js` can be
dropped into any future page and only the parts relevant to that page's own DOM actually run — no
per-page script variants to keep in sync.

**SEO/social-preview metadata (`index.html`/`documentation.html` `<head>`) and the favicon are
real, generated assets, not placeholders.** `docs/assets/favicon.svg` renders the 私記 kanji (the
old favicon was a generic `>_` prompt glyph) — verified by rasterizing it with `rsvg-convert`
(this sandbox's Chromium renderer had already cached its font list before a CJK font was installed
mid-session, so it kept showing tofu boxes in-browser even after the font was available —
`rsvg-convert` picks up newly-installed fontconfig entries immediately, which is how the favicon
and the Open Graph image were actually verified to render the kanji correctly). `favicon-32.png`/
`apple-touch-icon.png`/`favicon-512.png` are pre-rasterized alongside the SVG for browsers/contexts
that don't support SVG favicons.

**`docs/assets/og-image.png` (1200×630, the banner shown when this link is shared on
Discord/Slack/X/iMessage/etc.) was built by rendering a dedicated HTML/CSS card
(kanji mark + "shiki" wordmark + tagline + tag pills + the real gruvbox-dark screenshot) through
headless `chromium --headless --screenshot` at the exact target resolution, not hand-drawn or
scraped from an existing screenshot.** This sandbox's own browser-automation viewport is capped at
~1061px regardless of requested window size (confirmed via `window.innerWidth` — resizing the
automation-controlled tab has no effect on the actual render width), so it couldn't be used
directly for a pixel-exact 1200×630 capture; local headless Chromium has no such cap and was used
instead. If this card ever needs to be regenerated (new tagline, new screenshot), redo it the same
way rather than trying to force the sandboxed browser tool to an exact size.

**The JSON-LD `SoftwareApplication` block in `index.html`'s `<head>` has a hardcoded
`softwareVersion`** (unlike the nav's version pill and the download button, which are fetched live
from the GitHub Releases API in `main.js`) — structured data is meant to be read by crawlers
parsing the static HTML, not necessarily post-JS-execution DOM state, so it can't reuse that same
live-fetch trick. Bump it by hand alongside the workspace version the same way `CHANGELOG.md`/
`Cargo.toml` already require a manual touch — it's not wired into any automated version-bump step.

**`docs/robots.txt`, `docs/sitemap.xml`, and `docs/llms.txt` round out discoverability** —
`robots.txt` allows every crawler and points at the sitemap; `sitemap.xml` lists both real pages
(`index.html`, `documentation.html`); `llms.txt` follows the emerging llms.txt convention (a plain
markdown file at the site root — H1, one-paragraph blockquote summary, then linked sections) aimed
at AI assistants/crawlers that check for it directly, the same way search engines check for
`robots.txt` at a well-known path. If a new real page is ever added to `/docs`, add it to
`sitemap.xml` too — nothing regenerates that list automatically.

## Architecture

Cargo workspace, four crates with a strict one-way dependency chain:

```
shiki-core   (pure domain logic, no TUI, no config crate dependency)
shiki-config (TOML config + themes, no ratatui dependency)
shiki-tui    (ratatui UI, depends on shiki-core + shiki-config)
shiki-cli    (clap entrypoint, depends on all three; binary name is `shiki`)
```

**shiki-config is deliberately decoupled from ratatui.** `Theme` (`shiki-config/src/theme.rs`)
stores every color slot as a string — `#rrggbb` hex, a terminal-native ANSI name
(`"blue"`, `"darkgray"`, …), or `"reset"` to inherit the terminal's own default — not
`ratatui::style::Color`. Don't add a ratatui dependency to shiki-config to "simplify" this — the
string→`Color` conversion lives in `shiki-tui/src/render.rs::hex_to_color`, keeping the config
crate reusable outside a TUI context. The `"default"` built-in theme (`Theme::terminal_default`)
uses `"reset"`/ANSI names throughout specifically so it doesn't impose a fixed palette.
Included theme palettes live one-per-file under `shiki-config/src/themes/` (catppuccin, tokyo_night,
gruvbox, nord, solarized, the League of Legends champion palettes in `lol.rs` — `LoL (Jinx)`,
`LoL (Teemo)`, `LoL (Ahri)` — the video-game palettes in `games.rs` — Pokémon, Zelda, Portal,
Mario, Overwatch, Halo, Stardew Valley — and the hacker/cyberpunk palettes in `hacker.rs` —
Matrix, Cyberpunk 2077, Arasaka, Synthwave, Tron, Fallout, Blade Runner, Doom, GitS, Mr. Robot),
registered in `themes/mod.rs::all()`/`by_name()`. Every palette's hex values are taken directly
from each project's own official spec (verified against
morhetz/gruvbox, tokyonight.nvim, nordtheme.com, solarized, and catppuccin — not approximated).

**`theme.selection` used to be defined by every palette but rendered nowhere.** Every `List` widget
across the TUI (Notebooks, Notes, tree, logs, global search, theme picker, which-key) set
`.highlight_style(Style::default().fg(accent).add_modifier(BOLD))` with no `.bg(...)` at all, so the
selected row only ever got bold accent-colored text — no actual highlighted background band, which
is why every theme looked "flat"/less faithful than the same palette elsewhere (editors, prompts):
none of them were using their own `selection` color for the one thing it's for. Fixed by adding
`.bg(hex_to_color(&theme.selection))` to every one of those `.highlight_style(...)` calls (verified
by inspecting live ANSI output via `tmux capture-pane -e`, not just visually — e.g. gruvbox-dark's
selected row now carries `48;2;60;56;54`, exactly `#3c3836`).

**Gruvbox's `accent`/`link` are its iconic yellow (`#fabd2f` dark, `#b57614` light), not blue —
every other included theme already sets `accent == link`, gruvbox was the one outlier still
pointing both at `#458588` (a blue).** `render::panel_block`'s unfocused border switched from
`BorderType::Rounded` to `BorderType::Plain` at the same time, for a fully square look across every
panel/modal — don't reintroduce `Rounded` there. `render::panel_block_reading` is a new sibling to
`panel_block`, not a parameter added to it: it always renders `BorderType::Plain` (never `Thick`)
in the theme's accent color, used only by PREVIEW while actually displaying a note's body — a
full-width `Thick` border around a wall of text reads as louder than the same emphasis on a narrow
list panel (Notebooks/Notes), which still gets the regular focused/unfocused `Thick`/`Plain` split
from `panel_block` unchanged. `docs/css/styles.css`/`docs/js/main.js`'s gruvbox theme-variable
copies were updated to match, per this file's own "keep the marketing site's copies in sync"
convention (see the Marketing site section below).

**The tags modal (leader+`T`, `panel_tags.rs` + `App::toggle_tags`/`handle_tags_key`) is two
levels deep, same shape as the tree view/global search's "browse then jump" pattern — it used to
be read-only (`ListState::select` was never called, so its `.highlight_style` was unreachable).**
Level 1 lists distinct tags (`TagIndex::build(&app.notes)`, current directory only — same scope
the tags modal always had); `Enter`/`l` on one sets `App.tags_viewing = Some(tag)`, switching to
level 2: every note in `self.notes` carrying that tag (`App::notes_with_tag`). `Enter`/`l` there
jumps straight to it (select + `Focus::Preview` + close both levels) — no `reload_notes`/
`notes_path` juggling needed, unlike the tree view's/global search's cross-folder jumps, since
every match is already sitting in the currently-loaded `self.notes` by construction (`notes_with_tag`
filters the same list `current_tags` was built from). `h`/`Esc`/`Backspace` at level 2 goes back
to level 1; `Esc`/`q` at level 1 closes the modal outright. `App::toggle_tags` resets both level
and selection to the top every time it opens, so it never reopens mid-drill-down from last time.

**The Settings screen (leader+`s`, `panel_settings.rs` + `App::toggle_settings`/`handle_settings_key`)
is paged by tab, not one long scroll, and every tab is genuinely interactive — this replaced an
earlier version where only NOTEBOOKS was, and before that, a version where the whole screen was a
read-only summary with `i`/`E` as the only way to change anything (hand-editing raw TOML).**
`App.settings_section: panel_settings::SettingsSection` picks the active tab; `←`/`→`
(`App::switch_settings_section`) cycles GENERAL → THEME → GIT → EDITOR → EXPORT → NOTEBOOKS →
SNIPPETS → back to GENERAL, always resetting `settings_selected`/`settings_notebook_drill`/
`settings_snippet_drill`/
`settings_field_selected` back to the top — switching tabs mid-drill-down doesn't leave a stale
drill state behind in whichever tab you land on. Each tab's field order is its own small `enum`
with an `ALL` const (`GeneralField`/`ThemeField`/`GitField`/`NotebookField`/`SnippetField`) —
`ALL`'s order must match that tab's row-builder function exactly, since `settings_selected`
indexes straight into `ALL` to know which field `Enter` acts on. GENERAL/GIT are single-level:
`Enter` on a boolean (`use_favorite_editor`; GIT's `auto_commit`/`auto_push`/`sign_commits`/
`auto_sync`) toggles it in place and `Config::save`s immediately, no confirmation, since it's one
reversible flip; `Enter` on every other field opens the existing text-input prompt machinery
(`PendingInput::SettingsGeneralText`/`SettingsGitText`) prefilled with the current value, resolved
back to a concrete field via `GeneralField::ALL[settings_selected]`/`GitField::ALL[...]` once
confirmed — not a payload on `PendingInput` itself, which stays a plain unit-variant enum
throughout, same as every other kind.

NOTEBOOKS and SNIPPETS are two levels deep instead, the same list-then-drill-in shape the tags
modal above already established (and reuse its exact `h`/`Esc`/`Backspace`-back,
`Esc`/`q`-close-at-level-1 convention) — `App.settings_notebook_drill`/`settings_snippet_drill`
are each `Some(name)` while drilled in, `None` while browsing that tab's level-1 list; the two
share one `settings_field_selected` for the level-2 row index, safely, since only one section is
ever active at a time. NOTEBOOKS level 1 lists every real notebook
(`panel_settings::sorted_notebook_names`, not just ones with a `[notebooks.<name>]` override — a
notebook's git remote is worth showing even with no override at all) alongside its git remote,
redacted via `shiki_core::git::redact_credentials` the same way logged URLs already are; level 2
shows that remote plus the three `NotebookGitOverride` fields (`auto_push`/`auto_sync`/
`auto_sync_every`), each falling back to `"inherit ({global value})"` when unset rather than a bare
`false`/`0`, so it's visible whether a value is this notebook's own or just the `[git]` default
showing through — `auto_push`/`auto_sync` cycle unset → `true` → `false` → unset again
(`App::cycle_notebook_bool_override`, a 3-state cycle unlike GIT's plain 2-state toggle, since
unset here means something real — "inherit" — not just "false"); `App::prune_empty_notebook_override`
removes the notebook's `[notebooks.<name>]` table entirely once all three cycle back to unset, so
toggling everything back to "inherit" doesn't leave a pointless empty table in `config.toml`.
`remote`/`auto_sync_every` open the same `PendingInput` prompt pattern GENERAL/GIT's text fields
use, reusing `shiki_core::git::set_remote`/`remote_url` — the same primitives the notebooks-panel
`R` binding (`start_set_remote`) already uses, rather than a second remote-editing code path.
A sixth `NotebookField::Hidden` row exists only for restoring a notebook that was untracked
(`hidden = true`, set by answering the notebook-delete dialog with "just remove the reference") —
`Enter` on it clears the flag, `reload_notebooks`, and prunes the now-empty `[notebooks.<name>]`
table, so a hidden notebook is never stranded: `sorted_notebook_names` deliberately includes
config-hidden names (they're filtered out of `App.notebooks` itself) so Settings can always reach
one. It's a one-way action, not a 3-state cycle like `auto_push`/`auto_sync` — a hidden notebook is
already invisible to NOTEBOOKS, so there's no "hide from here" state to cycle into.

SNIPPETS level 1 additionally supports `a` (new snippet: prompts for a trigger via
`PendingInput::SettingsSnippetTrigger`, creates it with an empty label/body, and drills straight
into level 2 so it can be filled in) and `d` (delete: `App.pending_delete_snippet` mirrors
`pending_delete`'s shape — a bare trigger string instead of a `(DeleteTarget, PathBuf)` pair, since
a snippet has no filesystem path — routed through the exact same confirm-dialog gate every other
delete in this app uses). Level 2 shows `label` (a `PendingInput::SettingsSnippetLabel` text
prompt) and `body` — `body` is the one field in all of Settings that *doesn't* use a `PendingInput`
prompt, since a snippet body is arbitrary multi-line text: `Enter` on it instead opens the same
`Mode::Edit`/`InlineEditor` machinery a note's own body uses, tracked via `App.editing_snippet:
Option<String>` (checked in `save_and_exit_edit` right alongside `editing_config`, a three-way
dispatch: config file / snippet body / note body). THEME is the odd one out structurally: `name`
doesn't edit anything inline at all, it opens the *existing* theme picker (leader+`c`) via
`open_theme_picker` rather than duplicating its live-preview/commit logic; `overrides` stays
purely informational (pointed at leader+`c`/`shiki theme create --from` in a status message), since
19 individual color slots don't fit a single-row edit.

Every one of these prompts/sub-modals (`PendingInput` prompts, the theme picker, `Mode::Edit`) has
to explicitly hide `show_settings` first and restore it after, and this is *not* optional
boilerplate — it's required both for input dispatch and for rendering. `on_key` checks
`show_settings` *before* `self.mode`, so an `Insert`/`Edit`-mode prompt underneath a still-`true`
`show_settings` would never receive its keystrokes at all (they'd keep going to
`handle_settings_key` instead); separately, in `draw()`, `panel_settings::render`'s call comes
*after* the theme picker's but *before* `pending_input`'s and `confirm`'s, so leaving
`show_settings` true while the theme picker is open would paint Settings right over it (the same
"render order has to independently agree with input-dispatch order" bug already hit once for the
history-modal/confirm nesting, and hit again for real while building this — the theme-picker case
specifically, confirmed by testing with `show_settings` left `true` and watching the picker vanish
under the settings popup). `App.reopen_settings_after_theme_picker` exists only to distinguish "the
picker was opened from Settings" from "opened directly via leader+`c`" — the latter must *not*
reopen Settings when it closes. `PendingInput` prompts and `Mode::Edit` don't need an equivalent
flag: their close paths (`confirm_input`, `Esc` in `handle_insert_key`, `save_and_exit_edit`) know
directly (from which `PendingInput`/`editing_snippet` was set) whether they were Settings-launched,
so they can set `show_settings = true` unconditionally in their own branch.

**Each crate has its own error type** — there is no shared error enum:
- `shiki_core::Error`/`Result` (thiserror, in `shiki-core/src/lib.rs`)
- `shiki_config::config::Error`/`Result` (thiserror, in `shiki-config/src/config.rs`)
- `shiki-cli` uses `anyhow` at the command layer to unify both.

**Re-export asymmetry to be aware of:** `shiki_config::Config` and `shiki_config::Theme` are
re-exported at the crate root, but the nested types (`Keybindings`, `GitConfig`, `ThemeConfig`)
are not — reach them via `shiki_config::config::Keybindings` etc. `shiki_core` re-exports `Note`,
`Frontmatter`, `Notebook`, `NotebookStore`, `SearchEngine`, `TagIndex`, `Template` at the root, but
functions like `shiki_core::daily::create_or_open` and `shiki_core::git::commit_all` are only
reachable through their module path.

**Note file format** (`shiki-core/src/note.rs`): a `.md` file starting with `---\n`, YAML
frontmatter, a closing `\n---`, then the Markdown body. This is parsed/serialized manually
(`Note::split` / `Note::to_file_contents`) rather than via a frontmatter crate — keep both in
sync if the delimiter format changes.

**Frontmatter is optional on read, not just on write.** `Note::from_file` never fails on content —
`try_parse_frontmatter` only succeeds on a well-formed `---` block with valid YAML; anything else
(a plain markdown file from `nb`, an existing repo, a manual export, or even a `---` block with
broken YAML) falls through to `synthesize_frontmatter`, which derives a title from the first `#
heading` or the filename and a date from the file's mtime, treating the whole file as the body.
This is deliberate: a notebook built by pointing `git.set_remote` + pull at someone else's repo
will have files shiki never wrote, and those must still show up as notes rather than silently
vanishing (the old behavior — `list_notes` used to propagate the parse error via `?`, so a single
non-conforming file blanked out the *entire* notebook's listing). The only remaining failure mode
of `from_file` is a genuine I/O error. Nothing is rewritten on disk until the note is actually
saved (rename/edit) through shiki — reading one doesn't touch it.

**A notebook can nest folders arbitrarily deep, like `nb`** — `Notebook::list_dir(relative)`
returns `(Vec<String> folder names, Vec<Note>)` for one level; `list_notes()` is just
`list_dir(Path::new(""))` (root only, for CLI/daily-note callers that don't care about depth), and
`all_notes_recursive()` walks every level (used by `NotebookStore::all_notes` for global search).
Note CRUD takes the actual `Path` rather than reconstructing one from a root-relative slug
(`create_note_in`, `delete_note_at`, `rename_note_at`) specifically so it keeps working at any
depth — don't reintroduce slug-based root-only variants as the "normal" path. In `shiki-tui`,
`App.notes_path: Vec<String>` is the breadcrumb and `App.selected_note` indexes into the
*combined* `folders ++ notes` list, not `notes` alone — use `selected_note()`/`selected_folder()`
rather than indexing either `Vec` with it directly. `l`/`→`/`enter` on a folder descends
(`navigate_forward`); `h`/`←` ascends one folder level before falling back to `Focus::backward()`
(`navigate_backward`).

**Folders could only be navigated, not created, until notes-scope `f`
(`Action::NewFolder` → `Notebook::create_folder_in`) was added.** Notes could already end up at any
depth because `create_note_in` calls `create_dir_all` as a side effect of writing a file, but there
was no way to make an *empty* folder up front — the only folders that ever showed up were ones that
already existed on disk (an imported repo, or one created outside shiki entirely). `create_folder_in`
mirrors `create_note_in`'s depth handling (`self.path.join(relative).join(name)`, `create_dir_all`)
but reuses notebook-name validation (`validate_name`) since the folder name becomes a path component
the same way a notebook name does. `PendingInput::NewFolder`'s submit handler cancels on an empty
name rather than substituting a default (unlike `NewNote`'s timestamp fallback) — a folder named
after a timestamp is confusing in a way a note isn't. Deliberately does *not* call `note_changed`
after creating one: git doesn't track empty directories at all, so the folder isn't a real change
from sync's perspective until a note actually gets created inside it.

**Folders can now be deleted and moved/copied — `d`/`m`/`y` all broadened from "notes only" to
"whichever's selected, note or folder."** `Notebook::delete_folder_at` (`shiki-core/src/notebook.rs`)
is a plain `remove_dir_all`; it didn't exist at all before, so selecting a folder and pressing `d`
used to silently no-op (`App::start_delete_note`'s guard was `selected_note().is_some()`, which is
`None` for a folder). `DeleteTarget` gained a `Folder` variant reusing the exact same
`pending_delete`/`confirm` mechanism note/notebook delete already had — no new confirmation
plumbing needed.

**`m` (move) is no longer "move a note to a different notebook's root" — it's "move a note or
folder to `notebook/path/within/it`, any depth, any notebook."** The prompt is always prefilled
with `"{current_notebook}/{breadcrumb}"` (`App::current_address`) — editing the trailing segments
targets a different folder in the *same* notebook (auto-created via `create_dir_all`, same as
`create_note_in`/`create_folder_in` already do); replacing the first segment targets a different
notebook entirely. The first segment must already exist — `App::parse_move_target` errors clearly
rather than creating a notebook from a typo, since a notebook is a new git repo, not just a
directory. `Notebook::copy_note_to`/`move_note_to`/`copy_folder_to`/`move_folder_to`
(`shiki-core/src/notebook.rs`) are the actual primitives: copying rewrites `frontmatter.notebook`
only when the destination is genuinely a different notebook (a plain filesystem copy would
otherwise leave a stale `notebook:` field in the copy's own YAML), and `copy_folder_to` recurses
via `list_dir` so every note at *any* depth inside gets that same rewrite, not just top-level
ones — empty subfolders are preserved too, since it walks `list_dir`'s own folder list rather than
inferring structure from where notes happen to be. All four error rather than silently overwriting
if something already exists at the destination (`Error::DestinationExists`). `move_*` is just
`copy_*` followed by removing the source — copy is the one primitive doing real work.

**`Mode::Visual` existed as a dead enum variant for a while before this — it was already routed to
`handle_normal_key` and the footer's `mode_label` already had a `"VISUAL"` arm, but nothing ever
set `self.mode = Mode::Visual`.** It's real now: `v` (`Action::ToggleVisual`) anchors
`App.visual_anchor` at whatever's currently selected; `j`/`k`/`PageUp`/`PageDown`/`Home`/`End` keep
moving `selected_note` exactly as they already did, with no special-casing needed there, since the
selected range is always computed as `min(visual_anchor, selected_note)..=max(...)`
(`App::visual_range`) over the same combined folders++notes list `selected_note()`/
`selected_folder()` already index into. `l`/`h`/`Enter`/`Tab` (navigate into a folder, switch
panels) are explicitly disabled while `Mode::Visual` is active (guarded in `handle_normal_key`'s
match arms) — entering/leaving a folder reloads the underlying list out from under
`visual_anchor`, stranding it at an index that means something completely different afterward;
better to just not allow it than to handle the fallout. `d`/`m` (already-bound actions) check
`self.mode == Mode::Visual` first and, if so, act on every item in `visual_range` instead of just
the one under the cursor; `y` (`Action::CopyEntries`) is Visual-mode-only, since there was no
single-item duplicate being asked for, only the batch one. Every batch entry is captured as an
absolute path *up front*, in `App.pending_batch`/`pending_batch_delete`
(`enum SelectedEntry { Note(PathBuf), Folder(PathBuf) }`) the moment `d`/`m`/`y` is pressed, rather
than re-deriving the selection from indices at confirm time — a background sync's `reload_notes`
completing while the confirm dialog or move/copy prompt is still open (now genuinely possible
since sync runs on a background thread, see below) can't shift the list out from under an
in-flight batch action this way. The single-item case (`start_move_or_copy` with `Mode::Normal`)
populates the exact same `pending_batch` shape with one entry, so `apply_pending_batch` has exactly
one code path regardless of count — there's no separate single-vs-batch move implementation.

**Selecting a folder (not a note) in NOTES previews its contents in PREVIEW, not a static hint.**
`panel_preview.rs::folder_preview_lines` calls `nb.list_dir(notes_relative_path().join(folder))`
fresh on every render (a single non-recursive `read_dir`, cheap enough to redo every frame — no
cached state, same as the rest of this render function being a pure function of `App`) and lists
its subfolders then notes, or "Empty folder." if there's nothing there. This replaced a static
"press enter to open this folder" message — the point is to show what's actually inside before
descending, the same way selecting a note already shows its content. `App::notes_relative_path` is
`pub` specifically so `panel_preview.rs` (a different module) can build that sub-path.

**The note-preview title is a multi-`Span` `Line`, not a plain string, so the date can be a
different color than the title.** `panel_block`'s `title_style` (bold, `theme.panel_title`) only
applies to spans that don't set their own style — a plain `Span::raw` for the title inherits it,
while the trailing `Span::styled(date, fg(muted))` overrides just the color, landing on a visibly
de-emphasized date next to a normal-looking title without needing two separate widgets. This
replaced a `"[j/k scroll]"` hint in the title, which stopped earning its space once scrolling
(and now `PageUp`/`PageDown`/`Home`/`End`) had been the obvious way to move around for a while.
`App.show_dates` (notes-scope `D`, off by default) does the equivalent in the NOTES list itself —
same append-a-muted-span approach, per note item this time.

**Collapsed (out-of-focus) panels are 1 column wide (`layout::COLLAPSED`), not 3.** At 3 columns a
collapsed panel showed a sliver of unreadable truncated content alongside its border; at 1 column
it's just the border line, which is all a collapsed panel needs to communicate — the user already
knows it's there and can `tab`/`h` back into it.

**`layout::split` has three size tiers, not one fixed 3-column layout** — `columned` (the original
side-by-side layout, `main.width >= STACK_WIDTH == 70`), `stacked` (same focused/collapsed
proportions from `tier_constraints`, just split vertically instead of horizontally, so a narrow-
but-tall window — portrait, a many-way tmux split, a square-ish terminal — still gives the visible
panel(s) the full width instead of a sliver), and `single` (`main.width < SINGLE_WIDTH == 46` or
`main.height < SINGLE_HEIGHT == 14`: only the focused panel renders at all, full screen, no
collapsed siblings — even a 1-column border doesn't fit alongside a usable panel at that size).
`single` gives the two non-focused panels a zero-sized `Rect` rather than omitting them — `draw()`
always renders all three panels unconditionally regardless of tier, and rendering into a
zero-sized `Rect` is a safe no-op in ratatui, so nothing outside `layout.rs` needed to change.
Navigation (`hjkl`/`tab`/`enter`, all driving `App.focus`) is completely tier-agnostic — moving
between "screens" in `single` tier is the exact same `navigate_forward`/`navigate_backward` calls
as switching panels in `columned`, just reflected differently by whichever tier `split` picked.
Verified by resizing an actual tmux pane through all three tiers (200×50 down to 20×8) and
confirming no panic and no broken rendering at any size, including navigating between panels while
in `single` tier.

**Notebook = directory + independent git repo** (`shiki-core/src/notebook.rs`,
`shiki-core/src/git.rs`, via `git2`). `NotebookStore::create` calls `git::init_repo` immediately,
so every notebook is git-managed from creation, not lazily. Notebook names come straight from user
input (the "new notebook" prompt) and are used as a path component — `validate_name` in
`notebook.rs` rejects empty/`.`/`..`/`/`/`\` before any `create`/`rename`/`get`/`delete` joins the
name onto `root`; don't bypass it by constructing paths manually elsewhere.

**Search** uses `nucleo-matcher` (not the full `nucleo`/`nucleo-picker` crates) — see
`shiki-core/src/search.rs::SearchEngine`. `search()` matches note titles only (used by the
notebook-local jump, `/`, via `all_notes_recursive` so it still finds nested notes); `search_text()`
is the generic version over arbitrary haystacks, used by global search (leader+`g`) with
`"{notebook} {title} {body}"` per note so it matches on content too, not just titles.

**Keybindings are scoped, not one flat map** (`shiki-config/src/config.rs::Keybindings`,
`shiki-tui/src/keybindings.rs::KeyMaps`). There are four independent `HashMap<KeyCode, Action>`s —
`global` (needs the leader key first), `notebooks`, `notes`, `preview` — each populated from its
own `[keybindings.*]` TOML table. The same key can resolve to a different `Action` depending on
which map is consulted, e.g. `a` is `NewNotebook` in the `notebooks` map and `NewNote` in the
`notes` map. `App::handle_normal_key` picks the map by `self.focus` via
`KeyMaps::resolve_scoped`, except when `leader_pending` is set (one key after the leader), in
which case it resolves against `KeyMaps::resolve_global` instead — see `leader_pending` handling
at the top of `handle_normal_key`. Navigation (`hjkl`, arrows, `tab`, `enter`, `?`) and `quit` are
**not** in any scope map; they're hardcoded in `handle_normal_key` since they behave identically
everywhere (`quit` is matched via `KeyMaps::is_quit`, a plain `KeyCode` comparison, not an
`Action` variant).

**`App::on_key` dispatches on `Mode`** (`shiki-tui/src/app.rs`) — `Insert` routes to
`handle_insert_key` (drives `InputBox` for new note/notebook, rename, jump-search, set-remote, and
move-to-notebook), `Edit` routes to `handle_edit_key` (forwards keys into the `tui-textarea`-backed
`InlineEditor`, `Esc` saves and exits), `Normal`/`Visual` route to `handle_normal_key`. A delete
(note/notebook, depending on `Focus`) goes through a separate `confirm: Option<ConfirmDialog>` gate
checked before mode dispatch. External editing (`E`, or `i` when
`general.use_favorite_editor` is on) sets `want_external_edit: Option<(PathBuf, String)>` — the
editor command travels with the path since it's resolved per-invocation (static configured editor
for `E`, the cached `App.favorite_editor` for `i` — see below) — `run()` picks this up between draw
calls to disable raw mode / leave the alternate screen, spawn the editor via
`shiki_core::editor::command_for` (splits multi-word commands like `"code --wait"`), and restore
the terminal. The theme picker (leader+`c`) live-previews by mutating `self.theme` as you move the
cursor and only persists to `config.toml` on `Enter`; `Esc` reverts to
`available_themes[theme_index]`. `shiki-tui/src/command.rs`'s `CommandPalette` is still unused
dead code — the notes-scope search (`/`) and global search (leader+`g`) were both built directly
in `App` instead.

The scratchpad (`leader+p`) is intentionally an in-memory `InlineEditor`, not a temporary file:
notebooks are independent git repositories and any file placed inside one can be picked up by
auto-sync. `Esc` discards the buffer; `Ctrl+S` stages it for the existing new-note title/template
flow, which is the only path that gives it a filesystem location.

**`[[wikilink]]` autocomplete (typing `[[` in the inline editor) reuses the exact fuzzy-matching
`SearchEngine` the notes-scope `/` jump and global search already use, rather than a separate
substring filter.** `App.wikilink_candidates: Vec<Note>` is snapshotted once, the instant the menu
opens (`open_wikilink_menu`, checked live off the buffer the same way `show_slash_menu`'s doc
comment already reasons about its own trigger) — every note in the current notebook, any folder
depth, excluding the note being edited (linking to yourself isn't useful); `wikilink_results:
Vec<SearchHit>` is what actually gets re-scored on every keystroke (`refresh_wikilink_menu`), the
same "expensive walk once, cheap re-score" split `global_search_pool`/`global_search` already
established. `App::wikilink_query` reads the filter live off the buffer — the text between the
closest preceding `[[` and the cursor — rather than tracking it as separate state, and returns
`None` (closing the menu) the moment that text contains a stray `[`/`]`, covering both "backspaced
past the opening `[[`" and "closed the pair by hand instead of picking a candidate." Each candidate
row shows its folder breadcrumb alongside the title (`relative_folder` against the selected
notebook's path), since — unlike the NOTES panel, always scoped to one folder — two notes named
the same thing in different folders are otherwise indistinguishable in this list. Selecting one
inserts `[[Title]]`, replacing the just-typed `[[query` in place.

**Ctrl+Click on a rendered `[[wikilink]]` in PREVIEW (`App::try_follow_preview_wikilink`) jumps
straight to the note it resolves to via `shiki_core::wikilinks::resolve_one` (with a
cross-notebook fallback through `wikilinks::resolve_one_global`, see below); a plain click still
enters edit mode everywhere, including on top of a wikilink** — Ctrl is deliberately the opt-in
gate here, the same way Alt+Click already gates adding a multi-cursor, so that clicking to edit a
note that happens to contain links doesn't become impossible. `App::jump_to_note(path, title)` is
the shared deep-link tail (breadcrumb + `reload_notes` + select + focus PREVIEW) factored out of
the links modal's (`leader+L`) existing jump handler specifically so this feature didn't duplicate
it — an unresolved link (no matching note) reports that in the status message rather than jumping
anywhere or erroring.

**The links modal has a third section, "Mentions (unlinked)" (`wikilinks::unlinked_mentions` +
`LinkRow::Mention`), and is reachable globally via leader+`B`, not only PREVIEW's `L`.** A mention
is a note whose body contains the current note's title as plain text (case-insensitive substring)
without actually `[[linking]]` to it — notes that already link are excluded even if they *also*
mention the title elsewhere, since they're backlinks, not missed links. The global binding is the
same `Action::ShowLinks` bound in both the `global` and `preview` scope maps (two entries in
which-key, one handler) — `open_links` already guards on "select a note first", so resolving it
from any focus was free. Mentions render muted with a search icon, deliberately weaker-looking
than a real backlink, because they're candidates, not facts.

**The global tasks view (leader+`t`, `shiki-core/src/tasks.rs` + `shiki-tui/src/panel_tasks.rs` +
`App::open_tasks`/`handle_tasks_key`) toggles checkboxes in the source file by *content address*,
never by line index.** `tasks::extract(body)` is a pure function (unit-tested without I/O) finding
`- [ ]`/`- [x]` lines plus an optional `@due(YYYY-MM-DD)` tag (malformed dates are just text, not
errors); each `Task` carries its exact `raw_line` and its `occurrence` index among *identical*
lines, and `tasks::toggle(path, raw_line, occurrence)` re-reads the file fresh and matches on
those — a stored line number would be invalidated by any edit between building the list and
toggling (the modal sits open indefinitely while background syncs/external edits can rewrite the
file; a vanished line is a clear `Error::TaskNotFound`, not a flip of whatever line ended up at
that index). Lines inside the leading `---` frontmatter block are excluded from occurrence
counting on both sides, so YAML that happens to look like a checkbox can't shift the count.
`toggle` returns `Toggled { done, raw_line, occurrence }` — the flipped line's *new* address, which
the TUI writes back into the row so a second toggle of the same row matches the file's new content
(the returned occurrence matters in the edge where the flip makes the line identical to a
pre-existing twin). Toggling updates the row **in place**, deliberately not a rebuild: with the
default pending-only filter a rebuild would make a just-checked task vanish out from under the
cursor with no way to immediately un-toggle it. Each toggle is a real note edit and is routed
through `refresh_notes_preserve_selection()` + `note_changed(notebook)` so panels/caches refresh
and auto_sync counts it like any other save. `panel_tasks::TaskRow` is a **flat list, not the
header-interspersed shape links/tree use** — it started as the `LinkRow`-style
headers-plus-`selected_row`-mapping design, and was flattened after Omar asked for each task to
say where it lives: a per-note header scrolls out of view while its tasks are still on screen, so
every row carries its own muted `location` (`"{notebook}/{folders…}/{note title}"`, human title
not file slug, matching the NOTES panel) instead. Rows are sorted by urgency —
`sort_by_key(|r| (due.is_none(), due))`, dated-ascending first then undated in walk order (the
`is_none` flip matters: `Option`'s derived ordering would otherwise float `None` *above* overdue) —
built from the same `NotebookStore::all_notes` walk global search uses; the cross-notebook jump
(`l`/`o`, `jump_to_task_note`) mirrors `jump_to_global_hit`. Due dates color by urgency (overdue →
`theme.error`, today → `theme.warning`, else muted; a *done* task's date is always muted — no
longer actionable). Note the default keys don't collide despite both being `t`: leader+`t` (tasks)
resolves against the `global` map, NOTES-scope `t` (daily note) against the `notes` map. But a
*user's own* config can collide — Omar's real config had `theme_picker = "t"` (a personal
customization; the shipped default was always `"c"`), which silently lost to `tasks_panel`'s new
default since `KeyMaps::from_config` binds into a plain `HashMap` where the last insert wins.
**`shiki doctor`'s keybinding-collision check is a hardcoded field list
(`doctor.rs`'s `scopes` array), not derived from the config struct — any new keybinding field must
be added there by hand or collisions on it are invisible to doctor**, which is exactly how the
`theme_picker`/`tasks_panel` clash went undetected until it was hit live (fixed: both `links` and
`tasks_panel` are in the list now, and doctor flags that config with "are all bound to the same
key — only one of them actually works").

**`shiki tasks`/`shiki graph` (CLI) and the TUI tasks modal share their formatting through
shiki-core, not by parallel implementations.** `tasks::location_of` is the single source of the
`"{notebook}/{folders…}/{title}"` string both show (same extraction as `git_status_color` being
shared by footer+drawer — two surfaces describing the same thing must go through one function).
`shiki tasks --count` exists specifically for status bars (waybar/polybar/tmux) and prints *only*
a number; `--json` includes a precomputed `overdue` bool per task so a bar script doesn't need to
re-derive date math. `shiki graph` (`shiki-cli/src/commands/graph.rs`) renders
`wikilinks::edges`/`orphans` via a **deterministic** Fruchterman–Reingold layout — seeded by a
golden-angle spiral, no RNG, so the same notebook draws the same picture every run (a graph that
reshuffles per invocation reads as noise). Y-distances are weighted 2× during force calculation to
compensate for terminal cells being ~2:1 tall:wide, else clusters render vertically squashed.
Edges draw first and only into `Cell::Empty`; node markers and labels always win. Graphs past
`MAX_NODES` (60) keep only the most-connected notes and say so. Links never resolve across
notebooks (title resolution is per-notebook by construction), so the all-notebooks graph is
naturally disjoint clusters — don't "fix" that by resolving titles globally; two notebooks can
legitimately have different notes with the same title.

**Relative `@due` specs are pinned at save time, never at read time** —
`tasks::parse_relative_due`/`normalize_due_tags` (`tomorrow`/`tom`, `+N`/`+Nd`/`+Nw`, weekday
names meaning *next* such weekday, never today). A relative spec is relative to the day it was
written, so read-time interpretation would silently shift the date every day the note stays open.
Both save paths normalize: `save_and_exit_edit`'s note branch (body only) and the external-edit
finalize in `run()` (re-reads the file, rewrites only if something actually normalized). `extract`
itself stays ISO-only on purpose — a file edited outside shiki keeps its relative spec as plain
text (due=None) until some shiki save path touches it, rather than shiki guessing a base date.
ISO dates and unrecognized junk are left byte-for-byte untouched: normalization only resolves,
never invents.

**The daily note's "## Due today" agenda (`tasks::agenda_section`) is injected only on creation,
and deliberately as plain bullets + `[[wikilinks]]`, not checkbox copies** — a `- [ ]` copy would
be extracted as a second, independent task (double-counted in the tasks view, and checking the
copy wouldn't complete the original). `daily::create_or_open` takes `agenda: Option<&str>` and
ignores it entirely when the daily already exists, so reopening later in the day can't duplicate
the section or clobber edits. Both callers (TUI `create_daily_note`, CLI `commands/daily.rs`)
build it from `store.all_notes()` — the agenda spans every notebook, not just the daily's own.
Those cross-notebook `[[links]]` now *resolve* when clicked or listed: `wikilinks::resolve_one_global`
falls back from the current notebook's notes to every other notebook's (local always wins, so two
notebooks with same-titled notes still resolve to the current one), and both Ctrl+Click
(`try_follow_preview_wikilink`) and the links modal's outgoing rows use it; `jump_to_note` was
made cross-notebook aware (it finds the owning notebook by path prefix and switches
`selected_notebook` before reloading) so the deep-link tail needed no per-caller branch. The bullet
itself is still plain text — only the jump path resolves.

**`c` in the links modal (`link_selected_mention` → `wikilinks::link_mention`) repairs a mention
into a real link, editing the *mentioning* note's file in place.** `link_mention` wraps the first
plain-text occurrence outside existing `[[...]]` spans (skipping frontmatter), preserving the
mention's own casing — the resolver is case-insensitive so `[[proyecto shiki]]` still resolves to
"Proyecto shiki". Byte offsets found on a lowercased copy are re-verified against the original
before use (`find_unlinked`'s doc comment explains the exotic-codepoint shift risk). After a
successful link the modal rebuilds via `open_links`, so the row visibly migrates from Mentions to
Backlinks; `Ok(false)` ("nothing linkable left") is a status message, not an error — it means the
file changed since the mention was detected.

**The theme picker's `Enter` (and `shiki theme set`) only reset `config.theme.overrides` when the
base theme name is actually *changing*, not on every confirm/set.** Both used to zero out
`ThemeOverrides` unconditionally — even re-confirming the theme that was already active with no
real change silently wiped any hand-written custom colors. The fix compares against
`config.theme.name` (the last *committed* value) before overwriting it — in `app.rs`, that's
deliberately not `self.theme.name`, which is the live-preview value while browsing the picker and
would give the wrong answer (it changes on every `j`/`k`, long before anything's actually
confirmed). **`ThemeOverrides` (`shiki-config/src/config.rs`) covers all 19 of `Theme`'s color
slots now, not just 5** (`bg`/`fg`/`accent`/`selection`/`border` — the other 14, e.g. `error`/
`warning`/`success`/`tag`/`link`/`cursor`, had no override path at all before). Every field is a
plain `Option<String>` with no `#[serde(default)]` needed — serde's derive already treats a
missing `Option<T>` field as `None`, which is exactly why the original 5-field partial-override
behavior already worked and why an existing `config.toml` with only some of the 19 keys (or none)
keeps parsing fine. `ThemeConfig::resolve()` applies all 19 via plain repeated `if let Some(v) = …`
blocks, not a macro — this codebase has no `macro_rules!` anywhere, and introducing the first one
just to save repeating a 3-line pattern 14 more times isn't worth the new precedent.

**`shiki theme create [--from <name>]`** (`shiki-cli/src/commands/theme.rs`) is the actual way to
get a full custom theme instead of hand-typing hex codes with no example to copy from — it
resolves the base theme (`--from`, or whichever is currently active), calls
`ThemeOverrides::from_theme(&base)` (sets *every* one of the 19 fields to that theme's real
values, not blanks), and — deliberately — also sets `config.theme.name` to that same base name,
not just the overrides: every slot is about to be explicitly overridden with `base`'s colors
anyway, so leaving `theme.name` pointing at whatever was active before would make the command's
own printed "removing a key falls back to `<base>`" guidance wrong the moment someone acts on it.

**The default theme on a fresh install is `gruvbox-dark`, not `catppuccin-mocha`** —
`ThemeConfig::default()` (`shiki-config/src/config.rs`) is the single place this is set; the
`[theme] name = "..."` example in `IDEA.md` was updated alongside it so the docs don't show a
config value nobody actually gets by default. This only affects a brand-new `config.toml` (or one
with no `[theme]` table) — anyone who already has `name = "catppuccin-mocha"` written down,
explicitly or via a prior default, keeps that theme; nothing migrates existing configs.

**The footer's Buy Me a Coffee segment (`status_bar.rs`) is mouse-clickable, not an OSC 8 terminal
hyperlink.** Embedding a real OSC 8 escape sequence (`\x1b]8;;URL\x1b\\`) directly in a `Span`'s
text was considered and rejected: ratatui/unicode-width has no concept of ANSI escape sequences,
so it would count every literal character of the escape sequence (`]`, `8`, `;`, the URL itself)
as display-width columns, corrupting the footer's layout rather than rendering invisibly the way
a real terminal-level OSC 8 sequence does. Instead, `status_bar::right_text()` builds the footer's
right-aligned string *and* returns the char-range the coffee segment occupies within it — a single
source of truth `render()` and `coffee_hit_at()` both call, so they can't disagree about where the
clickable area actually is (same reasoning as `git_status_color`/`git_status_suffix` being shared
between the footer and the drawer). `App::on_mouse` calls `status_bar::coffee_hit_at` the same way
it already hit-tests the drawer, and on a hit spawns `shiki_core::browser::open_url` (a new
`shiki-core` module — `xdg-open`/`open`/`cmd /C start` per platform, mirroring how
`shiki_core::editor::command_for` already handles editor spawns). It's fire-and-forget: a failed
launch (headless SSH, no GUI) just becomes a status message, never a panic. The status message's
`RESERVED_RIGHT` budget (footer message truncation) was changed from a hardcoded guess to
`right_text().chars().count()` while this was being touched — it's strictly more correct and can't
drift out of sync the way a second hardcoded number could.

**`App.favorite_editor: Option<String>` is resolved once at startup, not per-render.**
`shiki_core::editor::detect_favorite_editor()` can shell out to `xdg-mime` on Linux when
`$VISUAL`/`$EDITOR` aren't set — cheap once, but `draw()` runs roughly every ~100ms even while
idle (the `run()` loop's poll timeout), so calling it there would mean re-spawning that subprocess
several times a second for no reason. Cached at startup and reused by both the footer's editor-mode
indicator (`App::editor_status_label`) and `Action::EditInline`'s dispatch, so the two can never
disagree about which editor is actually in effect. leader+`e` (`Action::ToggleFavoriteEditor` →
`App::toggle_favorite_editor`) flips `general.use_favorite_editor` and persists it immediately
(same `Config::save` pattern as the theme picker), so switching between the built-in editor and the
OS favorite doesn't need hand-editing config.toml — the footer always shows which one is active:
the resolved editor's bare binary name (e.g. `nvim`) when on, `native` when off. Every note CRUD
operation (create via `a`, edit via `i`, delete via `d`, rename via `r`) already works identically
regardless of which mode is active — `native` isn't a reduced-functionality fallback.

**Note version history (PREVIEW-scope `H`) is real git history, not a separate versioning
system.** `shiki_core::git::file_history(repo_path, file_relative)` walks the revwalk from `HEAD`
and, for each commit, compares the file's blob at that path against its first parent's — a commit
is included only when the blob actually changed (or the file didn't exist in the parent yet), the
same idea as `git log -- <path>` implemented by hand since git2 has no built-in "log for one path"
helper. `show_file_at`/`revert_file_to` read/write the file's *entire* content at a given commit
(frontmatter included, since that's genuinely what the blob contains) — a revert is a full-file
replace, not a body-only patch. `revert_file_to` deliberately doesn't commit by itself: the
reverted file just becomes a normal pending change, so it flows through the exact same `s`/`u`/
`auto_sync` path as any other edit, rather than needing a special "revert commit" code path.
`App::pending_revert: Option<(PathBuf, String)>` mirrors `pending_delete`'s pattern (both drive the
same shared `confirm: Option<ConfirmDialog>`/`handle_confirm_key`) rather than inventing a second
confirmation mechanism. **The confirm dialog is rendered last in `draw()`, after every modal, not
before them** — a revert confirmation is opened *from inside* the history modal, so if `confirm`
rendered earlier (as it originally did, right after `pending_input`), the history modal painted
over it and the dialog was invisible despite `App.confirm` being genuinely `Some` (hit this exact
bug while building this: `on_key`'s `confirm.is_some()` check already came first for input
handling, but `draw()`'s *render* order didn't match, since it's a separate ordering that has to be
kept in sync by hand — there's no shared single source of truth for both). If you add another
modal-launched-from-a-modal confirmation, don't move `confirm`'s render block back up.

**The footer's "{n} changes" (PREVIEW only) is cached per note path, not recomputed every
draw.** `App::refresh_history_cache`, called once per `run()` loop iteration right before
`terminal.draw`, only actually re-walks the note's history when the selected note's path differs
from the last one it checked — a full revwalk on every ~100ms idle redraw would be wasteful.
The cache is explicitly invalidated (`history_count_cache = None`) after any commit
(`run_sync`'s `Ok(true)` branch) and after a revert, since either can change the count for the
*same* path without the path itself changing — path-based cache invalidation alone would miss
both of those and show a stale number.

**PREVIEW's note view and folder-peek follow the exact same cache-per-draw-tick pattern as
`history_count_cache` above — `App::note_preview_cache`/`folder_preview_cache`, refreshed by
`refresh_note_preview_cache`/`refresh_folder_preview_cache` right next to `refresh_history_cache`
in `run()`.** Before this, `panel_preview.rs` called `markdown_to_lines`/re-`list_dir`'d and
re-`format!`'d every row unconditionally on every single draw — cheap for a normal note or folder,
but a real, *measured* cost that scales with content size at ~10Hz: caught live via
`scripts/benchmark.sh`'s aggressive scenarios, a 300,000-line note cost double-digit idle CPU
purely from being reformatted every ~100ms regardless of whether anything had changed, and a
100,000-note folder selected-but-not-entered cost the same from re-listing and re-parsing every
note's frontmatter every redraw. Both caches key on `(path, [fg, accent, muted, link])` rather than
path alone — the theme picker live-previews by mutating `self.theme` while browsing, and a
color-blind cache would keep showing the old theme's colors until the note/folder selection
happened to change too. `refresh_notes_preserve_selection`/`reload_notes` clear both caches
unconditionally (same as `history_count_cache` does after a revert) since either can cover a case
where the *same* selected path's content changed underneath it (external edit, inline edit save,
revert) without the path itself changing.

**`<details>`/`<summary>` blocks are genuinely collapsible in PREVIEW, with session-only fold state
keyed by note path (`App.details_folded: HashMap<PathBuf, HashSet<usize>>`).** `markdown_to_lines_indexed`
takes a `folded: &HashSet<usize>` of block ids (document order) and a nested stack + `suppressed`
stack of folded ids to decide which lines to omit; `details_content_counts` pre-computes each block's
content line count (counting nested content toward every open ancestor) so a folded summary can say
`▸ More (12 hidden)`. A click on a rendered summary row toggles its block via `App::toggle_details_block`
— which needs `summary_blocks: HashMap<usize, usize>` (summary source-line -> block id) baked into
the `NotePreviewCache` tuple — instead of entering edit mode there. The `folded` set rides in the
cache key (like `[fg, accent, muted, link]` and `width`), so toggling a fold rebuilds the cache; it's
deliberately NOT persisted to `config.toml` (same "view state, not config" rule as zen mode). The
fold state is keyed by note path, not by the `App.notes` index, so switching notes and back keeps a
note's folds.

**`$$...$$` math renders its content through `mathfmt::latex_to_unicode`, a lightweight
hand-rolled LaTeX→Unicode converter (`shiki-tui/src/mathfmt.rs`), applied in `render.rs` at three
paths: the single-line block (`$$E = mc^2$$`), the multi-line block (`in_math_block`), and —
via `inline_spans` taking a `math` style parameter — inline `$$...$$` mid-line (a line like
"so $$a^2 + b^2 = c^2$$ and more" splits into plain prose + a math-styled span).** A terminal
can't typeset LaTeX, but it can beat echoing the markup verbatim: `\frac{a}{b}` → `a/b`,
`\sqrt{x}` → `√x`, `^2` → `²`, `_0` → `₀`, `\pi` → `π`, `\int` → `∫`, Greek letters and common
operators — so the math-blocks note's formula reads as `∫₀^∞ e⁻ˣ² dx = √π/2` instead of
`\int_0^\infty e^{-x^2} dx = \frac{\sqrt{\pi}}{2}`. Deliberately NOT a TeX engine: no dependency,
and anything unrecognized passes through unchanged (a note with an exotic macro still shows
*something*, just not prettified). It's implemented as a small recursive-descent scanner (commands,
`{}`-grouping, `^`/`_` scripts with Unicode super/subscript maps, script-glyph pass-through so
nested exponents like `e^{-x^2}` → `⁻ˣ²` work) rather than a parser-combinator dependency,
consistent with `tasks.rs`/`query.rs`'s hand-rolled-parser convention. Pure function of a string —
unit-testable without a TUI, same as `details_content_counts`. The inline case deliberately
requires `$$` (not a single `$`), so a price or shell prompt isn't swallowed as math.

**` ```mermaid ` fences are buffered whole and rendered as a real diagram through
`mermaid::render` (`shiki-tui/src/mermaid.rs`), the same hand-rolled-parser convention as
`mathfmt`, instead of the flat accent-colored text they used to get.** A diagram can't be laid out
line by line, so `markdown_to_lines_indexed` accumulates the fence's source in `mermaid_buf`
(`Vec<String>`) while open and, on the closing fence, hands it all to `mermaid::render` —
which returns `None` for anything it can't parse, falling back to the old per-line flat styling
(rather than failing). Flowcharts (`graph TD`/`flowchart LR`, …) parse node definitions
(`A[Label]`/`A(Label)`/`A{Label}`/`A((Label))`/`A[[Label]]`/`A>Label]`), edge operators
(`-->`, `---`, `-.->`, `==>`, `~~~`, `-- label -->`, `-->|label|`), and `subgraph` blocks, then
lay the graph out as an indented tree — roots are nodes with no incoming edge, children recurse
under `└─`/`├─` connectors, and a node reached twice (a cycle or a second parent) is drawn as a
muted `… [Label]` reference line instead of recursing forever. Sequence diagrams parse
`participant X`/`actor X` and messages (`->>`, `-->>`, `->`, `-->`, `--x`, `-)`, with
`A->>B: text` syntax), rendering each participant as a fixed-width column with pipes under every
participant and `─`/`·` fill + a `▶`/`◀` arrowhead between source and target. Diagram nodes are
accent/bold for labels, muted for the shape brackets and connectors — same color vocabulary as
`math`/`render`. `kind_of` only treats a fence as a diagram when its first content line is a real
`graph`/`flowchart`/`sequenceDiagram` header, so arbitrary text under a ` ```mermaid ` fence
falls back instead of being mangled.

**Every non-mermaid code fence gets a right-aligned line-number gutter**: `code_line_no` (reset to
0 when a fence opens, incremented per source line inside it) is rendered as a leading muted span
`format!("{n:>3} │ ")` prepended to each code row — before the syntax-highlighted spans for a
recognized language, before the plain `dim` span otherwise. The gutter is `muted` without the
italic `dim` uses, so it reads as a separate axis from the code. It's part of the same `Line`, so
the existing wrap/`note_preview_source_line` machinery treats it as ordinary text — no new
offset arithmetic anywhere. Line numbers reference the *source* lines of the fence, matching how
click-to-edit maps a PREVIEW row back to the raw Markdown line.

**Block-level `![alt](path)` images render as terminal art through `shiki-tui/src/term_image.rs`,
a shell-out to `chafa`** — the same external-binary pattern as `pretty-pdf`/`hunspell`, so no
bundled image decoding or terminal-graphics-protocol dependency (kitty/sixel images anchor to
screen coordinates and break PREVIEW scrolling; chafa's ANSI-colored half-block text drops into
the existing `Vec<Line>` model and scrolls naturally). Only a *whole* line given over to an image
is eligible (`whole_line_image_path` — an inline `![alt](path)` mid-line keeps the single-span
icon+alt form); the path resolves against the note's folder → notebook root → `data_dir`, local
files only. chafa's 24-bit SGR output is parsed by a hand-rolled `ansi_to_line` (fg + bg applied,
so half-block art paints both halves; any non-SGR escape is dropped), and the rows land through
the same `row_indices` mechanism mermaid/table blocks use, so click-to-edit still maps them to the
image's source line. The `ImageCtx` is threaded through `markdown_to_lines_indexed` as plain data
(`enabled`/`chafa`/`cols`/`base_dirs`), keeping the renderer a pure-ish function; `cols` is
precomputed by `App::refresh_note_preview_cache` as `preview_image_scale × panel width` and the
preview cache is width-keyed, so a resize re-runs chafa at the new width automatically. When
`chafa` is missing or the file doesn't exist, the renderer falls through to the icon+alt form —
never a broken frame. `shiki doctor` checks `chafa`/`hunspell` availability.

**`Ctrl+D` in the inline editor inserts a timestamp (`insert_timestamp`, on by default) — and it
deliberately yields precedence to `multi_cursor`'s own Ctrl+D ("add next occurrence").** The two
arms are ordered so the `multi_cursor` guard runs first: with `multi_cursor` on, Ctrl+D keeps its
documented multi-cursor meaning (an accepted opt-in collision, same category as its Emacs
delete-next-char clash), and the timestamp feature simply isn't reachable by key until `multi_cursor`
is off — the `/date` snippet remains available regardless. The stamp is a single `insert_str` (one
undo step); `timestamp_with_time` appends `HH:MM`.

**The outline modal gained a live filter (`App.outline_query`, `panel_outline::filtered_headings`),
and its key handling now matches the which-key modal's "all letters filter, arrows navigate"**
convention rather than the old "j/k navigate" — with a filter box, `j`/`k` had to either stop
being typeable (a real limitation for headings containing them) or keep navigating (making the
filter unpredictable), so they were dropped in favor of `↑`/`↓`. The popup's height budget grew by
the 3-row filter box (`rows + 5`), which a stale `rows + 2` left the list at height 1 and rendered
blank items — caught and fixed in the same pass. `Enter` acts on the *filtered* list, and `Esc`
resets the query.

**Spell-check (`shiki-core/src/spell.rs` + `shiki-tui/src/spell_report.rs`) is a discrete
`Ctrl+E` pass shelling out to `hunspell`, gated by `[editor] spellcheck` (off by default).** `-l`
lists misspelled words (mapped back to byte ranges by first-unclaimed-occurrence, so a repeated
word that's spelled right somewhere still flags each real misspelling); `-a` pipe mode yields
suggestions. The byte ranges are converted once, at check time, to `(row, char-column)` space
(`spell_report::build_report`) — the same space the editor cursor works in — and the editor
renderer underlines only rows that still match the checked snapshot (`checked_lines`), so rows
edited after a pass stop being underlined instead of repainting stale ranges. The popup's selected
row is marked with a plain `▸` cursor (`panel_spell.rs` uses a literal `▸`, not `icons::ARROW`'s
Nerd Font glyph, so the cursor is visible in every terminal font — the earlier arrow glyph silently
disappeared in terminals without a patched font). `Enter` on a word opens a *suggestions submenu*
(`show_spell_suggestions`/`spell_suggestion_selected` + `panel_spell::render_suggestions`, a nested
centered popup), so the user picks which correction to apply instead of always silently taking the
first — the shared `apply_spell_suggestion` path then replaces via the same select-`cut`+
`insert_str` primitives as paste/format and sets `App.spell_flash`
(`spell_report::SpellFlash`) so the editor highlights the just-replaced word in the theme's
`success` color for ~1.5s (`App::expire_spell_flash`, the same elapsed-time pattern as
`expire_status_message`) — the visible confirmation of what changed, since a silent footer message
alone was too easy to miss. `flash_range` recolors the `[start, start+len)` char range of a
rendered segment line, the mirror-image helper to `underline_missed_ranges`. Notably the
spell-check popup reuses `PendingInput::SettingsGeneralText` for its `spellcheck_lang` field but
resolves it by *section* (Editor), not through `GeneralField::ALL` — the settings selection index
points at the EDITOR tab's list, so reading `GeneralField::ALL[idx]` would panic.

**List items, blockquotes, and indented code all render at any nesting depth, via five small
prefix helpers (`done_task_item_prefix`/`open_task_item_prefix`/`bullet_item_prefix`/
`ordered_item_prefix`/`blockquote_prefix`) that `markdown_to_lines_indexed`'s render chain uses
instead of the old flat `strip_prefix("- ")`/`strip_prefix("> ")` checks.** A line's nesting level
is `indent_level` (leading spaces ÷ 2, CommonMark's 2-space rule); bullets step the glyph
`•` → `◦` → `▪` by `level % 3` so depth is readable even if the leading spaces scroll past,
ordered items keep their `N.` marker, tasks (`- [ ]`/`- [x]`) at any depth keep their checkbox,
and blockquotes repeat the `▏ ` gutter once per `>` (`>> quote` → `▏ ▏ quote`). Lines with 4+
leading spaces that aren't a list/quote fall through to a dim "indented code block" row (like a
language-less fence). `inline_spans` gained `~~strikethrough~~` (crossed-out) alongside the
existing bold/italic/code/link cases. The order in the render chain matters: done-task, then
open-task, then bullet, then ordered, then blockquote, then indented-code — each prefix helper
trims leading whitespace first so an indented list item is still recognized as a list, not
mistaken for indented code.

**Both caches store the already-*formatted* `Vec<Line<'static>>`, not just the raw listing/body —
an earlier version of `folder_preview_cache` only cached `Notebook::list_dir`'s raw output and
still rebuilt every row's `Line`/`Span` (with a `format!` call each) on every draw, which was
enough to show up as ~16% idle CPU at the 100,000-note benchmark scenario despite the "expensive"
I/O already being cached.** `panel_preview.rs`'s `render` then hands the cached lines to
`Paragraph::new` via `render::borrow_lines`, which rebuilds the small `Line`/`Span` *wrapper*
structs but points each span's text at the cache's own `Cow::Borrowed` bytes instead of cloning
them — a plain `.to_vec()` would deep-clone every `String` in the cache on every redraw, which is
exactly the residual cost `borrow_lines` was added to avoid once the formatting itself was already
cached. What's left after both fixes (rebuilding one small `Vec<Span>` per line, still real at
six-figure line/entry counts) is inherent to `ratatui::widgets::Paragraph` needing an owned
`Vec<Line<'a>>` each render — verified acceptable at the benchmark's most extreme, deliberately
unrealistic sizes (100,000 notes in one folder: ~4.7% idle CPU, sub-1s first frame; a single
300,000-line/~26MB note: ~9.9% idle CPU) with zero measured RSS drift, i.e. no leak, in either case.

**`PageUp`/`PageDown`/`Home`/`End` are handled explicitly everywhere they're needed — they are not
free from ratatui/tui-textarea.** The one place they *do* come for free is the inline editor
(`handle_edit_key` forwards the raw `KeyEvent` straight to `tui-textarea`'s `TextArea::input`,
which maps `Key::PageUp/PageDown` to its own `Scrolling::PageUp/PageDown` and `Key::Home/End` to
"start/end of the current line" — verified against the vendored crate source, not assumed). Every
other list/scroll in shiki-tui needed its own handling: `App::move_selection`'s `Focus::Preview`
arm used to always scroll by exactly 1 regardless of the `delta` passed in (a latent bug — nothing
called it with anything but ±1 until `PageUp`/`PageDown` needed ±`PAGE_STEP`); it now scrolls by
`delta.unsigned_abs()`. `jump_to_start`/`jump_to_end` (bound to `Home`/`End` in `handle_normal_key`)
reuse `move_selection`'s existing shift+reload logic for NOTEBOOKS/NOTES by computing the exact
delta to land on index 0 or `len - 1`, rather than duplicating that logic. The PREVIEW `End` case is
the one approximation: it can't know the note's true *rendered* (wrapped) line count from outside
`draw()`, so it clamps the raw source line count against the preview panel's actual height (via
`layout::split(self.last_frame_area, self.focus).preview.height` — reusing the exact layout
`draw()` itself uses) so it lands at the last screenful instead of overshooting into blank space
(an actual bug hit and fixed while building this — the first version scrolled straight to the raw
line count with no ceiling at all). The same four keys were also added to every scrollable modal
(logs, global search, tree view) using each one's own existing selected-index field — no new
pattern, just filling in a gap that existed since those modals were first built.

**The which-key modal (`?`, `shiki-tui/src/which.rs` + `App::open_which_key`/
`handle_which_key_key`/`which_key_filtered_entries`) is a near-full-screen searchable list, not the
old small centered `Paragraph` popup.** It used to size itself to content height clamped against
the terminal (`entries.len() + scopes + 3` capped at `area.height - 2`) with *no* scroll state at
all — anything that didn't fit was silently clipped, and `on_key` treated which-key as modal-that-
closes-on-any-key (`if self.show_which_key { self.show_which_key = false; return; }`), so it
couldn't be typed into or scrolled either. It's now a `List`/`ListState` (same widget-based pattern
as logs/tree/global-search) filling most of the frame (`which.rs::render`'s `margin_x`/`margin_y` is
`area.{width,height} / 10`), with `App.which_key_input: InputBox` filtering `KeyMaps::entries()` by
key/action-label/scope substring (case-insensitive, `App::which_key_filtered_entries`) and
`App.which_key_selected` indexing into the *filtered* list. It doubles as a command palette:
`Enter` looks up the highlighted filtered entry's `Action` and calls `handle_action` on it directly,
closing the modal first — same "search-then-execute" pattern as global search's jump-to-hit.
Typed characters go to the filter (not `j`/`k` navigation, since `j`/`k` are themselves things a
user might type to search for "jump"/"key") — navigation is arrows + PageUp/PageDown/Home/End only,
matching the convention global search already established for the same reason. Don't revert this to
`on_key`'s blanket "any key closes it," and don't add `j`/`k` as navigation shortcuts here.

**Tree view (notes-scope `T`, `shiki-tui/src/tree.rs` + `App::open_tree`/`handle_tree_key`) is a
read-only modal, not a persistent alternate mode for the Notes panel.** `tree::build(nb)` walks the
whole notebook recursively (depth-first, folders — and everything under them — before the notes at
that level, same per-level ordering the Notes panel itself uses, just applied at every depth) into
a flat `Vec<TreeRow>` computed fresh each time the modal opens; it isn't kept in sync with
`folders`/`notes` afterward; because it's just a display list. Folder rows are display-only —
`tree_selected` indexes only the `Note` rows (`App::tree_note_count`/`tree_selected_row` — the
latter maps that note-only index back to its row position for `ListState::select`, since folder
headers are interspersed). `Enter`/`l` on a note reuses the same deep-link pattern as global search
(`relative_folder` to point `notes_path` at the note's folder, `reload_notes`, select by path,
focus `Preview`) — don't reintroduce a separate persistent tree-mode toggle for the main panel;
the modal is deliberately the simpler design, consistent with how the logs/theme-picker/global-
search modals already work.

**New notebook (`a`) detects a pasted git URL and clones instead of creating a plain notebook**
(`App::confirm_input`'s `PendingInput::NewNotebook` arm → `looks_like_git_url` →
`App::create_notebook_from_url`). If the typed value starts with `http(s)://`, `git@`, `ssh://`, or
`git://`, the notebook name is derived from the URL's last path segment (minus a trailing `.git`,
via `notebook_name_from_git_url` — handles both `.../owner/repo` and `git@host:owner/repo.git`
since splitting on either `/` or `:` and taking the last piece lands on the repo name either way),
then it creates the notebook, `git::set_remote`s it, and `git::pull`s immediately — so importing an
existing repo is `a` + paste URL + Enter instead of four separate steps (new notebook, name, `R`,
pull). A plain name still takes the normal empty-input-fallback path.

**A plain (non-URL) name still gets a remote auto-configured if `git.remote_template` is set** —
`{notebook}` is substituted with the new name (e.g. `"git@git.example.com:notes/{notebook}.git"`),
then `git::set_remote`, same as the pasted-URL path just without the `pull` (there's nothing to
pull — the remote still has to already exist on that server; this is a naming convention, not repo
creation via a hosting provider's API). Deliberately doesn't push right after either: a
just-created notebook has no commits yet, so an immediate push would only fail with a confusing
"reference not found" — the existing `auto_push`/`auto_sync` machinery picks the remote up
naturally the first time there's actually something to sync. Empty string (the default) is a
no-op, so existing configs/behavior are unaffected until someone opts in.

**Git remote support** (`shiki-core/src/git.rs::set_remote`/`remote_url`, plus the pre-existing
`pull`/`push`/`commit_all`) lets a notebook's `origin` be a normal git URL or a local path — git2
treats both the same for fetch. `Action::PullAllNotebooks` loops every notebook and reports
`{ok} ok, {failed} failed`; notebooks with no remote configured are an expected failure there, not
a bug. `pull` handles two cases explicitly: if `refs/heads/{branch}` already exists locally it only
fast-forwards (never discards local commits); if it doesn't exist yet (a brand-new/empty notebook
being pointed at an existing remote — the "import an existing repo" flow: `a` new notebook, `R` set
remote, `p` pull) there's nothing to fast-forward against, so it creates the branch straight from
what was fetched instead, same as `git clone`'s initial checkout. Don't remove that branch: without
it, pulling into a fresh notebook fails with "reference 'refs/heads/main' not found".

**Authentication (`git.rs::build_callbacks`)** is shared by `push`/`pull` and tries, per libgit2's
`allowed: CredentialType` for that attempt: SSH agent (only if `SSH_KEY` is offered — irrelevant for
`https://` remotes), then `Cred::credential_helper` (the system's own git credential store — reuses
whatever `git`/`gh` already have cached, e.g. macOS Keychain, libsecret, Git Credential Manager),
then anonymous `Cred::default()` for public repos. An attempt counter caps retries at 5 so a server
that keeps rejecting every credential type can't loop forever. Don't go back to a bare
`Cred::ssh_key_from_agent`-only closure — that's what previously made any HTTPS remote fail with a
generic "authentication required but no callback" error.

**`pull` fetches every branch on the remote (empty refspec list, so `repo.remote()`'s default
`+refs/heads/*:refs/remotes/{remote}/*` applies) and never reads `FETCH_HEAD`.** FETCH_HEAD's
on-disk format has extra "branch '...' of '...'" annotation text after the commit id (and gains
extra lines when tags get auto-followed), which git2's loose-reference parser doesn't expect —
`repo.find_reference("FETCH_HEAD")` can fail with "corrupted loose reference file: FETCH_HEAD" even
when the fetch itself succeeded fine. Reading the commit id back via `repo.refname_to_id(&tracking_
ref)` instead sidesteps the format entirely (it's just a plain ref), and also makes `pull` immune to
an already-corrupted FETCH_HEAD left over from before this fix (verified: pre-corrupting
`.git/FETCH_HEAD` with garbage and pulling again still succeeds, since it's never read).
`opts.download_tags(AutotagOption::None)` is also set, purely to reduce noise/multi-line risk in
FETCH_HEAD for anyone still relying on it elsewhere. Don't reintroduce
`repo.find_reference("FETCH_HEAD")` as the way to learn what was fetched.

**`pull` returns the branch it actually pulled (`Result<String>`, not `Result<()>`), because it
isn't always `config.git.branch`.** After fetching, it prefers `branch` if that ref exists among
`refs/remotes/{remote}/*`; if not and there's exactly one branch on the remote, it falls back to
that one instead of failing outright — a repo whose default branch is `master` (or anything else)
shouldn't be un-pullable just because `config.git.branch` defaults to `"main"`. Ambiguous (multiple
branches, none matching `branch`) is a real error listing what's available, not a guess.
`App::pull_notebook` compares the returned branch against `config.git.branch` and reports the
mismatch in the status message so it's visible instead of silently pulling a different branch than
configured. `pull_notebook` also checks `git::remote_url(&nb.path).is_none()` *before* calling
`pull`, since `p` always acts on whichever notebook is currently selected — after switching
notebooks or relaunching that might not be the one a remote was set on, and letting it fail inside
git2 produced an opaque "remote 'origin' does not exist" with no indication of which notebook.

**Sync policy (`auto_push`/`auto_sync`/`auto_sync_every`) is resolved per notebook, not read
straight off the global `[git]` config.** `Config::sync_for(notebook_name) -> ResolvedSync`
(`shiki-config/src/config.rs`) layers an optional `[notebooks.<name>]` override
(`NotebookGitOverride`, every field `Option<_>`) on top of the global `[git]` defaults — a
notebook connected to a private work repo can have `auto_push = true` while a scratch notebook
with no remote stays untouched, without a global setting forcing one policy on every notebook.
Every sync/push call site resolves `sync.auto_push` on the main thread and passes the plain `bool`
into `App::run_sync_blocking` (see below); don't reintroduce a direct `self.config.git.auto_push`
read in notebook-specific code.

**`sync`/`push`/`pull`/`pull all` all run on a background thread now, not synchronously on the
render loop** — `App::spawn_git_op(label, op)` (mirrors the self-updater's existing
`std::thread`+`mpsc` pattern, see `open_update_check`/`poll_update_channel` below) spawns `op` and
sends its `GitOpResult` back over `sync_rx`, polled once per `run()` iteration
(`poll_sync_channel`) exactly where `refresh_history_cache`/`poll_update_channel` already are.
Only one operation runs at a time, globally (`App.sync_in_flight: Option<String>`, the label shown
by the footer's spinner) — a second request while one's in flight is reported ("a sync is already
running…") and dropped rather than queued, same simplicity level the self-updater already has.
`GitOpKind` (`Sync`/`Pull`/`PullAll`) tells `apply_git_op_result` what to refresh once the result
arrives: `Sync`/`Pull` only touch `git_status`/`reload_notes` if the notebook they were about is
*still* the selected one by the time the background thread finishes — a real correctness need this
introduced, not just carried over, since the selection can change while the operation is in
flight, which was never possible in the old synchronous version. The drawer's `drawer_statuses`
(if open) is refreshed unconditionally regardless of what's selected, since it shows every
notebook. Known, accepted tradeoff: a note create/edit/delete can race a few hundred ms against a
background commit's working-tree scan; worst case it's picked up by the *next* sync instead of
this one — no data loss, consistent with the "nothing lost, just retried" philosophy the failed-push
handling below already has. Not worth a blocking guard around note edits during an in-flight sync.

**`App::run_sync_blocking`'s commit message names the actual files, not a bare count.**
`shiki_core::git::diff_summary(path)` buckets `repo.statuses(None)` into added/updated/renamed/
deleted, and for each bucket names the files directly when there are ≤3 (`"added (First note.md)"`),
falling back to a count only once a bucket gets big (`"12 updated"`) so the message doesn't become
unreadable; the caller prefixes the joined result with `config.git.commit_prefix`.
`run_sync_blocking` takes only owned data (`&Path`/`&str`), no `&self`/`&App` — it runs inside the
closure passed to `spawn_git_op`, on a background thread, so it can't borrow from the `App` that
spawned it. It also takes a `force_push: bool` and reports every step explicitly (`"committed: …"`
or `"no changes to commit"`, then `"pushed and confirmed by remote"` or the specific error) joined
with `"; "` — a terse "pushed" previously gave no way to tell whether anything had actually
changed or whether the push was real.

**`u` (`Action::PushNotebook` → `App::push_notebook`) commits (same as `s`) and always pushes,
regardless of the resolved `auto_push` policy** — `run_sync_blocking(force_push: true)`, the
explicit "sync right now" override, not a push-only action. It used to skip the commit step
entirely, which meant repeatedly pressing `u` on a notebook with uncommitted notes kept reporting
"pushed" while the dirty count in the footer never moved, since nothing had actually been committed
— confusing, since "pushed" sounds like something happened. Manual `s` and the automatic
every-N-changes trigger (`App::note_changed`) both use `force_push: false` instead, so they still
respect the resolved policy.

**`App::note_changed(notebook_name)` drives `auto_sync`'s every-N-changes trigger** — called after
every note create/edit (both `save_and_exit_edit` and the external-edit finalize in `run()`)/
rename/delete/move (bumps both the source and destination notebook for a move). It's a no-op
unless `Config::sync_for` says `auto_sync` is on for that notebook; when the per-notebook
`pending_changes: HashMap<String, u32>` counter (not persisted — resets to empty on relaunch)
reaches `auto_sync_every`, it resets the counter and calls `spawn_git_op` the same as manual `s`.
A failed push (no internet, auth, rejected by the remote, etc.) is reported in the status/log like
any other error and never panics or blocks — the commit already succeeded locally regardless, so
the next sync attempt (manual `s`/`u`, or the next automatic trigger) just retries the push with
nothing lost.

**`git::push` actually verifies the push landed instead of trusting `Remote::push`'s `Ok(())` at
face value.** libgit2 reports the network round-trip succeeding even when the *server* rejected the
specific ref update (e.g. a rejecting pre-receive hook) — that only surfaces through the
`push_update_reference` callback, registered in `push` and turned into a real `Err` if the server
sent back a rejection status. (The far more common non-fast-forward rejection — diverged
history — is already caught earlier, directly as an `Err` from `Remote::push` itself; verified by
pushing a divergent commit from a second clone and confirming `push` reports the conflict instead
of silently claiming success.) Note: this callback path can't be exercised against a `file://`/local
path remote in tests — libgit2's local transport writes directly to the target repo's refs and
bypasses server-side hooks entirely, unlike `git push` CLI to a local path; it only matters for the
real HTTP/SSH transports pushes normally use.

**Status messages funnel through `App::set_status`, not direct `self.status_message = Some(...)`
assignment** — every call site was migrated to `set_status` specifically so each message also gets
appended to `log_history: Vec<LogEntry>` (capped at 500), since `status_message` itself only ever
holds the latest one and errors were getting silently overwritten before a user could read them.
leader+`l` opens the logs modal (`Action::ShowLogs` → `App::open_logs`/`handle_logs_key`) showing
that scrollback; `y`/`c` inside it copies the whole thing to the clipboard via
`shiki-tui/src/clipboard.rs` (OSC 52 escape sequence written straight to stdout — works through
ratatui's alternate screen in terminals that support it, e.g. kitty/iTerm2/Alacritty/WezTerm, and
over SSH/tmux with clipboard passthrough enabled — no native clipboard crate needed). If you add a
new status/error message anywhere, call `self.set_status(...)` — a raw `self.status_message =
Some(...)` assignment would skip the log history.

**`log_history` is now actually persistent, not just "survives until the next status message
overwrites the footer."** `set_status` calls `App::persist_log_entry`, which appends one line
(`RFC3339 timestamp` + tab + message, `format_log_line`/`parse_log_line`) to
`Config::default_log_path()` — the *config* dir (`~/.config/shiki/shiki.log`), deliberately not
the data dir: the data dir's top level *is* the set of notebooks, each one a plain user-named
directory, so a fixed log filename there could collide with a notebook someone names the same
thing; the config dir has no such risk. `App::new` reads that file back into `log_history` at
startup (`load_log_history`, same 500-entry cap as the in-memory side, though the *file* itself
is never capped — malformed individual lines are skipped rather than failing the whole read, same
spirit as `Notebook::list_notes` tolerating one bad note file). A write failure sets `log_path =
None` (persistence silently disabled for the rest of the session, not retried every keystroke) but
is still reported *once*, by pushing directly into `log_history`/`status_message` rather than
going back through `set_status` — routing it through `set_status` would call
`persist_log_entry` again and recurse.

**leader+`l` then `x` clears all logs — both the in-memory `log_history` and the on-disk
file — behind the same confirm-dialog pattern as delete note/delete notebook**
(`App.pending_clear_logs`, mirrors `pending_delete`/`pending_revert`; wired into
`handle_confirm_key`'s existing `y`/`n` chain, not a new confirmation mechanism). `App::clear_logs`
truncates the file then calls `set_status("logs cleared")` — that message becomes the new file's
first line, so *when* it was last cleared is itself part of the record instead of leaving an
empty file with no context.

**Git URLs are redacted before they ever reach a status message, since those are what
`log_history`/the persisted file actually store.** `shiki_core::git::redact_credentials(url)`
strips the userinfo (`user[:password]@` in `scheme://…`, or the whole `user@` in scp-style
`user@host:path`) — GitHub/GitLab personal-access-token URLs commonly embed the token as bare
userinfo with no `:password` at all (`https://ghp_xxx@github.com/…`), so *any* userinfo gets
redacted, not just ones containing a `:`. Every call site that used to interpolate a raw URL into
`set_status` (`create_notebook_from_url`, the `remote_template` auto-configure path, manual
`SetRemote`) now redacts it first — `set_remote`/`pull` themselves still get the real, unredacted
URL, since they actually need the credentials to work; only the *displayed/logged* string is
touched.

**The footer's status message clears itself after `STATUS_MESSAGE_TIMEOUT` (2s), independent of
`log_history`.** `set_status` also stamps `status_message_set_at: Instant`; `App::expire_status_
message`, called once per `run()` loop iteration (alongside `refresh_history_cache`), clears
`status_message` once that long has elapsed. This only shortens how long a message sits in the
footer pushing other segments around — `log_history` (and thus the logs modal) is untouched, since
`set_status` still records the full message there regardless of how quickly the footer clears it.
Before this, a message stayed until the *next* action happened to overwrite it, which could be
indefinitely if nothing else triggered a status update. **The status message is also truncated to
whatever footer width is actually left** (`status_bar.rs::truncate_to`, using a `RESERVED_RIGHT`
budget for the right-aligned help/version text) rather than being allowed to overflow — the
underlying stored string (in both `status_message` and `log_history`) is never touched, only the
rendered `Span`, so the full text is still there in the logs modal even when the footer only shows
a `…`-truncated prefix of it.

**`Notebook::list_notes` skips `.md` files that don't parse as a shiki note** (no `---` frontmatter)
instead of failing the whole listing — an imported/pre-existing repo commonly has plain markdown
files alongside shiki-formatted ones, and one bad file used to blank out the entire notebook's note
list with no error (the caller's `.ok().unwrap_or_default()` swallowed the failure). Skipped files
stay on disk and under git untouched; they just won't appear as notes until they have frontmatter.

**`KeyMaps` matches on `KeyCode` only, not the full `KeyEvent`.** Don't change this back to keying
on `KeyEvent`/comparing `KeyModifiers` — Shift-based bindings (`A`, `E`, `T`, `P`, `R` by default)
are configured as plain uppercase chars with no modifier syntax in `config.toml`, so matching must
stay modifier-agnostic or those bindings silently stop firing.

**Config/data locations**: resolved via `directories::ProjectDirs::from("", "", "shiki")`
(`Config::default_path`, `Config::default_data_dir`, `Config::default_templates_dir` in
`shiki-config/src/config.rs`), so they respect `$XDG_CONFIG_HOME`/`$XDG_DATA_HOME` — see the
Commands section above for testing in isolation.

`Cargo.lock` is committed intentionally — `shiki-cli` produces a binary, not a library, so the
lockfile should be checked in for reproducible builds (don't add it to `.gitignore`).

**`shiki doctor` (`shiki-cli/src/commands/doctor.rs`) is handled *before* `Context::load()` in
`main()`, not through the normal command dispatch.** Every other subcommand goes through
`Context::load()?` first (parses `config.toml`, propagating a raw TOML error and exiting if it's
malformed); `doctor` exists specifically to still work — and say what's wrong — in exactly that
broken-config situation, so it can't depend on `Context::load()` succeeding. It does its own
defensive checks instead: `Config::parse` (a `load_or_init` split-out, see below) wrapped in a
match rather than propagated via `?`, `on_path()` (a plain `$PATH` scan — deliberately not
executing the found binary, since probing a user's arbitrary `general.editor` command with
`--version` could hang or have side effects), and falls back to `Config::default()` for the
checks that need *some* config (editor, notebooks) when parsing failed. Reports config/data-dir/
templates-dir/git/gh/terminal-truecolor/editor/notebooks as pass/warn/fail with a summary count,
and exits non-zero only if something actually failed (warnings don't fail the exit code). If you
add a new subcommand that similarly shouldn't require a working config, follow the same
before-`Context::load()` pattern in `main()`, not a special-case inside the dispatch `match`.

**Every crate inherits `repository`/`keywords`/`categories` via `.workspace = true` in its own
`Cargo.toml` — don't assume setting them once under `[workspace.package]` is enough.** Workspace
inheritance requires each field to be explicitly opted into per-crate; only `version`/`edition`/
`authors`/`license` had this before, so every crate's manifest had no `repository` at all (`cargo
package` warned "manifest has no documentation, homepage or repository") until this was added
everywhere. `readme = "../README.md"` is a plain (non-inherited) path per crate, since `readme`
can't sensibly inherit a single value across crates that live in different directories.

**`git2`'s workspace dependency carries `ssh`/`https`/`vendored-libgit2`/`vendored-openssl`, not
the bare crate.** `vendored-libgit2`/`vendored-openssl`: without these, `git2` links against
whatever libgit2/OpenSSL happen to be installed on the *build* system — fine on a dev machine with
them present, but that's not guaranteed on a `windows-latest` GitHub Actions runner (no system
libgit2 at all) or on an end user's machine installing via `cargo install`. These features compile
libgit2/OpenSSL from source and statically link them instead, so the resulting binary has no
external git/TLS library dependency on any platform. Don't remove them to "simplify" the build —
that's what makes the release workflow's Windows target actually link. `ssh`/`https`: as of git2
0.21 (bumped from 0.19 to close 3 RUSTSEC "unsound" advisories, see below), `default-features`
became empty — git2 0.19 used to default to `["ssh", "https", "ssh_key_from_memory"]`, so losing
these silently would have dropped SSH remote support and `Cred::credential_helper` (now gated
behind git2's own `cred` feature, which `ssh`/`https` each pull in) entirely, breaking
`build_callbacks`'s credential-helper fallback with no compile error pointing at why. Don't drop
`ssh`/`https` thinking `vendored-libgit2`/`vendored-openssl` alone are enough post-0.21.

**git2 was bumped 0.19 → 0.21 specifically to close `cargo audit` advisories, not for new
features.** `cargo audit` flagged 3 "unsound" (potential UB) advisories against git2 0.19
(`Remote::list()`, `Signature` from a buffer-created `BlameHunk`, dereferencing `Buf`) — none of
which shiki's code actually calls, but staying on 0.19 meant they'd show up in every future audit
regardless. The bump is a breaking change: `Commit::summary()` went from `Option<&str>` to
`Result<Option<&str>, Error>` (`git.rs`'s `file_history` now does
`commit.summary().ok().flatten().unwrap_or("")`), and `Reference::shorthand()`/`::name()` and
`Remote::url()` went from `Option<&str>` to `Result<&str, Error>` (every call site converts via
`.ok()` where the existing code already treated absence as "silently skip/default", and via
`.map_err(...)` in `push()` where the old code turned `None` into a real `Error::Git` — same
observable behavior, just adapted to the new `Result` shape). Verified live: a full push → real
remote commit → pull-into-a-fresh-notebook → note-history round trip against a local bare repo,
confirming branch resolution, remote URL detection, and commit-summary display all still work
correctly post-bump. `cargo audit` after the bump: down to 4 warnings, all transitive (via
`syntect`/`ratatui`, not fixable from shiki's own `Cargo.toml`).

**Workspace path-dependencies (`shiki-core`/`shiki-config`/`shiki-tui` under
`[workspace.dependencies]` in the root `Cargo.toml`) carry an explicit `version = "0.3.0"`
alongside `path = "..."`.** `cargo package`/`cargo publish` refuse to package a crate whose
path-dependencies have no version requirement at all (confirmed via `cargo package -p shiki-cli
--allow-dirty --no-verify`, which failed with "dependency 'shiki-config' does not specify a
version" before this was added). Because it's a bare version string (no `=` pin), Cargo treats it
as a caret requirement (`^0.3.0`) — a patch bump (0.3.0 → 0.3.1) doesn't require touching these
three lines, only a minor/major bump does, same as any other dependency in this file.

**Release automation lives in `.github/workflows/ci.yml` (fmt/clippy/build on every push/PR,
matrixed across `ubuntu-latest`/`windows-latest`/`macos-latest` — this is what actually proves the
Windows/macOS build works, since neither can be verified locally in every dev environment) and
`.github/workflows/release.yml` (triggered by pushing a `v*` tag).** The release workflow: builds
`shiki-cli` release binaries for `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`,
`x86_64-apple-darwin`, and `aarch64-apple-darwin`; packages each as `.tar.gz` (Unix) or `.zip`
(Windows) alongside `README.md`/`LICENSE`/`CHANGELOG.md`; uploads them plus a `SHA256SUMS.txt` to
a GitHub Release (`softprops/action-gh-release`); then updates and commits
`packaging/scoop/shiki.json` and `packaging/aur/PKGBUILD`/`.SRCINFO` back to `main` with the new
version and checksums, so those manifest files always reflect the latest published release even
between manual reviews. `.SRCINFO` is regenerated via `makepkg --printsrcinfo` inside a throwaway
`archlinux:base-devel` container (no Arch tooling available on `ubuntu-latest` directly).

**`bucket/shiki.json` (repo root) is a second, byte-identical copy of `packaging/scoop/shiki.json`
— not a mistake, both are load-bearing.** The site's own install instructions
(`docs/index.html`'s Scoop card) document `scoop bucket add sazardev
https://github.com/sazardev/shiki` followed by `scoop install shiki`, which only works if Scoop can
actually find a manifest in the repo once it's added as a bucket — and Scoop only ever looks in a
bucket's `bucket/` folder or its repo root, never an arbitrary nested path like
`packaging/scoop/`. Before `bucket/shiki.json` existed, that already-documented two-line install
command would have failed outright with "couldn't find manifest for 'shiki'" — this was caught by
actually trying to reason through why a user reported Scoop "wasn't working," not by any test
(there's no CI check that would have caught a bucket layout mismatch like this). `packaging/scoop/
shiki.json` still exists and still matters on its own: it's what the *other*, bucket-free README
install method (`scoop install https://raw.githubusercontent.com/.../packaging/scoop/shiki.json`,
a direct-manifest-URL install) points at, and that method doesn't care where the file lives at all.
`release.yml`'s `update-packaging-manifests` job writes both paths from the same loop, so they
can't drift out of sync with each other the way two independently-maintained copies could.

**crates.io publishing is live as of v0.4.1** — `CARGO_REGISTRY_TOKEN` is configured as a repo
secret, so `publish-crates` actually runs on every tag now rather than no-op'ing. The very first
attempt (on `v0.4.1`) failed with "A verified email address is required to publish crates to
crates.io" — a crates.io *account* requirement, not a workflow bug — fixed by Omar verifying his
email at crates.io/settings/profile, then re-run via `gh run rerun <run-id> --failed` (no new tag
needed, since the failure happened before any crate actually uploaded — `set -euo pipefail` in the
publish loop means a failed `cargo publish` for one crate stops the whole step, so there's never a
partially-published state to clean up). Verified end-to-end: all four crates show up via the
crates.io API (`max_version` = `0.4.1`, `yanked: false`), and `cargo install shiki-cli` with no
`--git`/`--path` genuinely resolves and installs from the registry.

**Publishing to the real AUR is live as of v0.8.1 — `AUR_SSH_PRIVATE_KEY` is configured, and
`release.yml`'s "Push to AUR" step actually runs now instead of no-op'ing.** Getting there needed a
real aur.archlinux.org account (registered by Omar himself — account creation isn't something an
agent should ever do) with a dedicated SSH deploy key, plus a one-time manual `git clone
ssh://aur@aur.archlinux.org/<pkgname>.git` → add `PKGBUILD`/`.SRCINFO` → commit → push for *each*
AUR package to create its page (AUR rejects an empty repo's first push unless the branch is
literally named `master` — pushing `main` fails with "hook declined", a real error hit while
bootstrapping this). **There are two AUR packages, not one**, mirroring the common Arch convention
of shipping both a fast prebuilt binary and a from-source build side by side:
- `packaging/aur/` → `shiki-bin` (downloads this repo's own prebuilt release asset, `provides`/
  `conflicts` `shiki` so pacman treats them as mutually exclusive alternatives).
- `packaging/aur-src/` → `shiki` (builds from the plain GitHub source tarball for the tag via
  `cargo build --release`; `conflicts=('shiki-bin')` since both install the same `/usr/bin/shiki`).
  `shiki-bin` exists specifically because `-bin` is the AUR-convention suffix for "prebuilt binary,
  doesn't compile anything" — asking to rename it to plain `shiki` would get flagged by AUR
  moderators, since that name is reserved by convention for something that actually builds from
  source. Getting a *second*, real package was the only clean way to make `paru -S shiki` (no
  suffix) work at all.
  **A real, non-hypothetical build failure was hit and fixed while writing `packaging/aur-src/
  PKGBUILD`:** `makepkg.conf`'s hardening `CFLAGS`/`LDFLAGS` (`-march=native`, `--as-needed`,
  `-z,now`, …) are meant for C/C++ packages — `rustc` itself ignores them, but they still leak into
  any C build script a dependency runs, and `aws-lc-sys` (pulled in transitively via `self_update`'s
  reqwest+rustls) failed to link against them with pages of `undefined symbol: aws_lc_*` errors.
  Fixed by `unset CFLAGS CXXFLAGS LDFLAGS LTOFLAGS` right before the `cargo build` line in
  `build()` — verified by actually running `makepkg` end-to-end (not just reasoning about it) both
  before (failed) and after (built, packaged, and the resulting binary ran and printed the right
  version) the fix, in a directory *outside* `/tmp` specifically — this sandbox's own `/tmp` is an
  8G tmpfs that was already 86% full from unrelated leftover files, which briefly produced a
  *different*, red-herring "No space left on device" failure before the real root cause was found.
  `release.yml`'s "Push to AUR" step now loops over both package dirs against their respective AUR
  git repos, rather than duplicating the clone/commit/push block per package.

`publish-crates` publishes `shiki-core`/`shiki-config`/`shiki-tui`/`shiki-cli` in that exact
dependency order with a 30s pause between each (crates.io's index needs a moment to make a
just-published crate resolvable before the next one in the chain can depend on it). See
`bucket/shiki.json` above for the analogous "path Scoop actually needed didn't match what was
already documented" bug on the Windows side.

**In-TUI self-update (leader+`U`, `shiki-core/src/update.rs` + `App::open_update_check`/
`start_update_install`/`poll_update_channel`/`handle_update_key`) is built on the `self_update`
crate against GitHub Releases, not a hand-rolled downloader.** `self_update` handles the
cross-platform parts that are easy to get subtly wrong by hand: archive extraction (tar.gz *and*
zip, since Linux/macOS and Windows releases use different formats), the Windows-safe "replace a
running executable's file" dance (`self-replace` crate internally), and checksum verification.
`check_latest`/`install_latest` are configured with `.verify_release_digest(true)` — GitHub computes
and serves a real sha256 digest per uploaded release asset (confirmed by querying
`api.github.com/repos/sazardev/shiki/releases/latest` directly and seeing a `digest` field on every
asset), so this verifies the download against that without shiki needing to fetch/parse
`SHA256SUMS.txt` itself. `.bin_path_in_archive("shiki-v{{ version }}-{{ target }}/{{ bin }}")` matches
`release.yml`'s exact archive layout — `{{ version }}`/`{{ target }}`/`{{ bin }}` are `self_update`'s
own template placeholders, not something shiki implements. `self_update`'s default `reqwest`+`rustls`
features pull in `aws-lc-rs` (rustls 0.23+'s modern default crypto provider) rather than the older
`ring` backend — reqwest 0.13 no longer exposes a plain `ring` feature at all, so this wasn't a
choice; verified via real Windows CI that `aws-lc-sys` still builds fine there (`windows-latest` has
the cmake it needs).

**`self_update`'s HTTP calls are synchronous/blocking, so both phases run on a plain
`std::thread::spawn` + `std::sync::mpsc` channel, not async** — `tokio` is a workspace dependency
but is not actually used anywhere in the codebase (checked: no `tokio::` reference exists outside
`Cargo.toml`), and `App::run`'s render loop is a synchronous ~100ms poll loop throughout, so pulling
in an async runtime for just this one feature would be inconsistent with everything else. `App`
holds `update_rx: Option<mpsc::Receiver<UpdateMsg>>`; `poll_update_channel` (called once per
`run()` iteration, same spot as `refresh_history_cache`/`expire_status_message`) does a
non-blocking `try_recv()` and updates `update_state` accordingly — the render loop never blocks on
the network call, verified by watching the "Installing…" modal keep redrawing during a real
download rather than freezing.

**A real bug, hit and fixed while building this: `std::env::current_exe()` must be captured
*before* `install_latest` runs, not after.** `self_replace` (used internally by `self_update`)
replaces the running binary's file via unlink-then-recreate rather than an atomic rename-over —
querying `current_exe()` again *after* the replace resolves to the old, now-deleted inode
(`".../shiki (deleted)"` on Linux) instead of the fresh binary sitting at that same path, so
spawning it failed with a plain "No such file or directory" that only made sense once the deleted-
suffix in the path was actually read closely. Fixed by capturing the path once in
`start_update_install` (`App.relaunch_exe_path`, before the background thread even starts) and
reusing that same captured value in `relaunch_into_updated_binary` instead of re-querying — the
path *string* stays valid throughout (only the file's content changes), so capturing it early
sidesteps the stale-fd issue entirely. Verified live: a full check → confirm → download → verify →
install → auto-relaunch round trip against the real repo (temporarily faking the installed
version down to `0.3.0` so the real latest release, `0.4.1`, showed up as "available"), landing on
the new process actually reporting the new version in the footer, not just the install succeeding.

**The relaunch is genuinely automatic, not "tell the user to restart manually"** — once
`install_latest` succeeds, `poll_update_channel` sets `App.want_relaunch = true` immediately (no
keypress required), `run()` renders one "Installed vX.Y.Z — restarting…" frame, then
`relaunch_into_updated_binary` leaves the alternate screen (same teardown half `suspend_and_edit`
uses, deliberately without the restore half — this process is exiting, not resuming) and spawns
the freshly-installed binary as a child process before `should_quit = true` unwinds the loop. The
child inherits the parent's now-restored-to-normal terminal and does its own completely ordinary
`enable_raw_mode`/`EnterAlternateScreen` startup — from the user's perspective it just looks like
shiki restarted itself onto the new version.

**`shiki capture "text"` (`shiki-cli/src/commands/capture.rs`) plus the optional in-TUI capture
daemon (`shiki-tui/src/capture.rs`, `general.enable_capture_daemon`, off by default) exist so
external launchers — rofi/waybar/polybar, Raycast/Alfred, AutoHotkey, a bare OS hotkey — have one
command to shell out to for near-instant note capture, with or without a TUI open.** `shiki
capture` itself never depends on the daemon: it always falls all the way back to a direct
`Notebook::create_note` write (the exact same path `shiki new`/`daily.rs` already use, auto-
creating the target notebook if missing) when nothing answers. The daemon only exists to make an
*already-running* TUI find out about a capture immediately instead of only picking it up on the
next manual reload.

**Transport is a TCP loopback socket (`127.0.0.1`, OS-assigned ephemeral port), not a Unix domain
socket** — the deciding factor was that shiki genuinely ships and CI-tests Windows binaries
(`release.yml`/`ci.yml`), and a Unix-socket-only daemon would silently have no equivalent on that
platform. `std::net::TcpListener`/`TcpStream` alone cover Linux/macOS/Windows identically, no new
dependency. The bound port is written to `Config::default_capture_port_path()`
(`~/.config/shiki/capture.port`, config dir — same collision reasoning as `shiki.log`/`trash/`/
`session.toml`: the data dir's top level is user-named notebooks) every time the daemon starts, so
a crashed process never leaves a stale port number for the next `shiki capture` to hang against —
confirmed live: `kill -9`ing the TUI and immediately running `shiki capture` falls back to a direct
write in single-digit milliseconds, since connecting to a now-unbound port fails instantly rather
than timing out.

**The listener thread, once spawned, is never torn down — toggling `enable_capture_daemon` off
just flips a shared `Arc<AtomicBool>` (`CaptureDaemonHandle::enabled`) that the thread checks per
connection, replying `ERR daemon disabled` without ever touching the main-thread channel.**
Toggling back on flips the same flag rather than rebinding a new port. This mirrors the "no
shutdown/rejoin mechanism anywhere in this codebase" reality `spawn_git_op`/self-update already
live with — inventing one here for a boolean would be new complexity with no precedent.
`App::ensure_capture_daemon_spawned` (called from `App::new` when the config already says on, and
from `App::set_capture_daemon_enabled` the first time the Settings toggle turns it on) is the only
place the thread actually gets created; a bind/IO failure reverts `enable_capture_daemon` to
`false` and reports why via `set_status`, rather than pretending the daemon is running.

**The listener thread is a dumb pipe — it never touches `Notebook`/`NotebookStore`/`App` itself.**
Per connection it reads the request to EOF (not line-delimited; the capture text may itself contain
newlines), bundles a fresh one-shot `mpsc::Sender<CaptureReply>` into a `CaptureRequest` sent over
the long-lived `App::capture_tx` → `capture_rx` channel, and blocks on that one-shot receiver
(`recv_timeout(5s)` as a safety net, not the normal path — the main loop's `poll_capture_channel`,
called once per `run()` iteration right next to `poll_update_channel`/`poll_sync_channel`, drains
every pending request well within that window). All real work — resolving/creating the target
notebook, checking encryption, `Notebook::create_note`, `refresh_notes_preserve_selection`,
`note_changed` — runs in `capture::perform_capture` on the *main* thread, reusing the exact same
note-creation path every other capture route already uses. This is what makes panel refresh and
auto-sync counting "just work" for a capture that arrived from an external process, with no
duplicated business logic on the background thread.

**A capture into an encrypted notebook never blocks on a passphrase from the daemon path.**
`perform_capture` calls the existing `App::resolved_notebook_crypto` — if the target notebook is
configured as encrypted but nothing's been typed in this session (`None`), it replies with an error
prefixed `locked: ...` rather than hanging (a background thread has no way to pop up a prompt) or
silently writing plaintext. `shiki capture`'s client side treats this prefix as authoritative and
does *not* fall back to a direct write in that case — falling back there could otherwise duplicate
the note or confuse the user about which passphrase prompt they're about to hit — but treats every
other daemon error (`daemon disabled`, a timeout, connection refused) as "nothing to connect to"
and falls through to the direct-write path, where an encrypted notebook prompts interactively via
the existing `unlock_if_encrypted`, same as `shiki new` always has.

**The wire protocol grew a second request kind (`PING`) and a small text header block on `CAPTURE`,
once `--daily`/`--tags` and a `--check` health-check flag were added — still no JSON, on purpose.**
`shiki-tui/src/capture.rs::parse_request` is the single pure/testable parser both request shapes go
through: `PING\n\n` (no body) and `CAPTURE\n<zero-or-more key=value header lines>\n\n<raw body>` —
headers end at the *first* blank line, so the body (the capture text itself) is free to contain its
own blank lines/newlines without confusing the parser, since everything after that first `\n\n` is
taken verbatim regardless of what it contains. `daily=1`/`tags=a,b,c` are the only two header keys
today; an absent header just means "false"/"no tags," so the common case (a plain capture, no
flags) is still just a two-line request. **`PING` is answered directly inside the accept-loop
thread, without ever touching the `capture_tx`/`App` channel at all** — a health check only needs
to read the `Arc<AtomicBool>` the thread already holds its own clone of, so routing it through the
main thread (and its 5s timeout safety net) would just be unnecessary latency for something that's
supposed to answer "are you there" as fast as the TCP handshake itself. Reply is always `OK
enabled`/`OK disabled`, never `ERR` — being asked for status isn't itself a failure state, unlike an
actual capture attempt.

**`--daily` reuses `shiki_core::daily::create_or_open` on both sides of the daemon boundary
(`shiki-tui/src/capture.rs::capture_into_daily` and `shiki-cli/src/commands/capture.rs::
capture_into_daily`, two call sites, same underlying primitive)** — same template/agenda-on-
creation behavior the `t` keybinding and `shiki daily` already have, since a daily note capture is
just "open or create today's daily the normal way, then append one more line." The agenda section
is still only injected on first creation, never on an append to an already-existing daily, per
`create_or_open`'s own existing contract — appending a bullet is not a "recreate the daily" event.
`--tags` is silently ignored when `--daily` is also given, rather than erroring: an appended bullet
line has no frontmatter of its own to carry tags on, and rejecting the combination outright would
be friction for no real benefit (a script that always passes both flags shouldn't need a special
case for whichever target it happens to be capturing into that day).

**`--check` (`shiki capture --check`) exits `0` the moment *anything* answers the `PING`, regardless
of whether it says enabled or disabled** — "reachable" means a TUI process exists at all, which is
the fact a status-bar module actually wants to gate on; "enabled" is separately reported (in the
`--json` shape or the plain-text parenthetical) for a module that wants to distinguish "no TUI
running" from "TUI running, daemon toggled off" as two different icons/colors. `--check` never
reads stdin and never calls `try_daemon` with a `CAPTURE` request — it's a dead-simple three-branch
function (`run_check`) that exists specifically so composing it in a script (`shiki capture --check
&& ...`) can't accidentally block waiting on a terminal that isn't there.

**Every daemon-handled capture calls `App::set_status`, not just on success — `perform_capture`'s
error branches call it too**, so a locked-notebook refusal or a notebook-creation failure that
happened while nobody was looking at the screen still shows up in the logs modal (`leader` then
`l`) afterward, not just as a `capture failed: ...` line in the CLI's own stderr that only the
script/launcher process ever saw. The standalone (no-daemon) fallback path has no equivalent — there
is no running `App`/log history to write into in that case, which is an accepted, unavoidable gap
rather than a missed case: a capture with no TUI running has nothing to log into.

**`shiki capture --undo`/`--folder`/content-prefix routing landed together, since undo and folder
targeting both needed the wire protocol's `CAPTURE` header block extended anyway, and routing
shares its notebook-resolution call site with both.** `RequestKind` (`shiki-tui/src/capture.rs`)
replaced the old flat `CaptureRequest{text}` shape with an enum (`Capture{text, daily, tags,
notebook, folder}` / `Undo`) — `notebook`/`folder` are `Option<String>`, only populated from a
`notebook=`/`folder=` header line when the client actually sent one, same "absent header means
default behavior" convention `daily=`/`tags=` already established. A third wire command, `UNDO\n\n`
(no body), joins `PING`/`CAPTURE`; like `CAPTURE` (and unlike `PING`) it's routed through
`capture_tx`/`App` rather than answered inline, since undoing needs real `App` state (`trash_root`,
`resolved_notebook_crypto`, `note_changed`, live refresh) that the listener thread doesn't hold.

**Content-prefix routing (`shiki_core::notebook::route_by_prefix`) lives in `shiki-core`, not
duplicated per crate, because both `shiki-tui/src/capture.rs::perform_capture` and
`shiki-cli/src/commands/capture.rs::run`'s standalone fallback need the identical decision — "does
`text` start with `<real notebook>: `?" — and `shiki-core` is the one crate both already depend on.
It only ever runs when the caller has no explicit override (`-n`, or the wire's `notebook=` header)
at all; an explicit target is never second-guessed by trying to parse the text it's about to save.
`--folder`'s path validation (`shiki_core::notebook::validate_relative_path`) rejects the whole
path on the first bad segment (`..`, empty, embedded separator) — same per-component rule
`validate_name` already enforces for a single notebook/folder name, just applied across every `/`-
separated segment of a multi-level path, so `--folder ../../etc` fails loudly instead of writing
somewhere outside the notebook.

**`LastCapture` (`shiki-config/src/last_capture.rs`) is a one-slot record, not a stack — undoing
only ever reverses the single most recent capture, matching the TUI's own `leader+u` (undo delete),
which is also one level deep.** It's a plain TOML file (`Config::default_last_capture_path()`,
same collision reasoning as every other fixed file in the config dir), written by *both*
`perform_capture` (daemon) and the CLI's standalone fallback to the exact same path — this is what
makes `--undo` work identically regardless of which one performed the original capture, without
either side needing to know which path the other took. Two variants: `Note { notebook, path }` (a
whole new note — undo moves it to trash via the existing `shiki_core::trash::move_to_trash`, so
it's still recoverable by hand afterward, same safety net every other delete in this app already
has) and `DailyAppend { notebook, path, appended }` (undo re-reads the daily note, verifies its
body still ends with `appended` *verbatim*, and only then truncates it off — an edit made to the
daily note in between capture and undo means undo refuses with a clear error rather than silently
destroying content the user actually meant to keep, since a `--daily` capture is an append into a
file that could easily have other, unrelated edits after it). The record is cleared
(`LastCapture::clear`) immediately after a successful undo, so a second `--undo` reports "nothing
to undo" instead of either silently no-op'ing (`Note`, file already gone) or re-attempting a body
suffix strip that would now legitimately fail (`DailyAppend`) with a confusing error instead of a
clean one.

**`shiki capture --undo` tries the daemon first, for the same live-refresh reason `CAPTURE` does,
but falls back to reversing it directly through the identical `LastCapture` file if nothing
answers** (`shiki-cli/src/commands/capture.rs::run_undo` → `run_undo_standalone`) — since the
record is shared, the standalone path reports the exact same outcome a live daemon would have, just
without refreshing a TUI that isn't running anyway. Only a `locked: ...` daemon response skips the
fallback outright (same reasoning as `CAPTURE`'s own locked-notebook handling) — every other daemon
answer (unreachable, disabled, "nothing to undo") falls through, since re-reading the same shared
file standalone can't produce a worse or different answer than the daemon already gave.

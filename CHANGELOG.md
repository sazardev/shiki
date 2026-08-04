# Changelog

All notable changes to shiki are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project doesn't follow strict
semver yet (pre-1.0), but version bumps are still meaningful and tracked here.

## [Unreleased]

### Added

- `shiki tasks` — the tasks view as a scriptable CLI command: pending tasks across every notebook,
  urgency-sorted, with `--overdue`/`--today`/`--all`/`--notebook` filters, `--json` for scripting,
  and `--count` (just the number) made for waybar/polybar/tmux status modules — e.g.
  `shiki tasks --overdue --count` as a "2 overdue" bar widget.
- `shiki graph` — the `[[wikilink]]` connection graph drawn right in the terminal: a deterministic
  force-directed layout (Fruchterman–Reingold on a char canvas) where hubs (`◉`) pull their linked
  notes around them, edges render as `╱╲─│` lines, and orphans (`○`, notes with no links in or
  out) drift free and are listed below. `--notebook` scopes it, `--width` overrides the canvas,
  `--json` emits nodes/edges/orphans for graphviz/d3/gephi, and graphs past 60 notes show the
  most-connected ones rather than an unreadable hairball.
- Daily notes now open with today's agenda: on first creation each day, a "## Due today" section
  is appended after the template listing every pending task due today or overdue across every
  notebook — plain bullets with a `[[wikilink]]` back to each task's source note (deliberately not
  checkbox copies, which would double-count in the tasks view). Reopening an existing daily never
  re-injects or duplicates it. Works in both the TUI (`t`) and `shiki daily`.
- Relative due dates: `@due(tomorrow)`, `@due(+3d)`, `@due(+2w)`, `@due(fri)` (next such weekday)
  are pinned to their resolved `@due(YYYY-MM-DD)` form the moment the note is saved — inline or
  external editor — since a relative spec is relative to the day it was written. ISO dates and
  unrecognized specs are left byte-for-byte untouched.
- `c` in the links modal on a "Mentions (unlinked)" row repairs the missed link: the mentioning
  note's plain-text mention is wrapped into a real `[[wikilink]]` in place (preserving its casing,
  skipping text already inside links), and the row visibly migrates to Backlinks.

- Global tasks view (leader+`t`): every `- [ ]` checkbox task across every notebook in one modal,
  sorted by urgency (overdue first, then due today, then future, then undated), each row showing
  its location (`notebook/folders…/note title`) muted alongside the task so it's never lost while
  scrolling. `Enter`/`space` toggles a task directly in its source file (the edit flows through
  the same git/auto-sync machinery as any other note change), `l`/`o` jumps to the note it lives
  in, `a` also shows already-completed tasks. Tasks support an optional `@due(YYYY-MM-DD)` tag —
  overdue dates render in the theme's error color, today's in warning, future ones muted.
- `shiki doctor` now includes the new `links`/`tasks_panel` global keybindings in its collision
  check — a config that customized `theme_picker = "t"` (colliding with the new tasks default)
  gets flagged instead of one of the two actions silently not working.
- Unlinked mentions in the links modal: notes that mention the current note's title in plain text
  without actually `[[linking]]` to it get their own "Mentions (unlinked)" section under
  Backlinks — candidate links you probably meant to make, jumpable like any backlink.
- The links modal is now reachable globally via leader+`B` from any panel, not only through
  PREVIEW's own `L` binding.
- `shiki notebook delete <name> --yes` — the CLI had no way to delete a notebook at all, despite
  the TUI supporting it; `--yes` is required (mirrors the TUI's own confirm dialog) since this
  permanently removes the notebook's directory and every note in it.
- `shiki doctor` now also checks: unrecognized keys anywhere in `config.toml` (a generic diff
  against `Config::default()`'s own shape, so it can't drift out of sync with the struct fields the
  way a hand-maintained list of "known keys" would); `general.default_notebook` actually matching an
  existing notebook; `data_dir` being a real directory, not just existing; two notebooks resolving to
  the same path on disk; `git.remote_template` containing its `{notebook}` placeholder; and
  `git.sign_commits` having an actual signing key configured.
- `shiki-core`/`shiki-config` test coverage: `note.rs`, `tags.rs`, `daily.rs`, and `session.rs` had
  zero tests despite being pure, easily-testable logic — added coverage for frontmatter parsing
  (including the CRLF and YAML-block-scalar fixes below), slugify, tag indexing, daily-note
  creation, and session-path sanitization.
- CI: a `cargo audit` job (with a `.cargo/audit.toml` ignore-list mechanism for a known,
  not-yet-fixable transitive advisory) and a real `cargo test --workspace` job, both missing before.
- Real syntax highlighting for fenced code blocks in the PREVIEW panel (`shiki-tui/src/syntax.rs`),
  via `syntect` — a workspace dependency that, until now, wasn't actually used anywhere. Every code
  fence used to render as flat dimmed text regardless of its language tag; ` ```lang ` fences with a
  language `syntect`'s bundled syntax defs recognize now get real per-token coloring, picked to match
  the active theme's light/dark bias (`render::is_dark_color`) so a light theme like
  catppuccin-latte doesn't get a dark-on-dark syntect theme. ` ```mermaid ` fences render in a
  distinct accent color (a terminal can't render an actual diagram) instead of being visually
  indistinguishable from a blockquote.
- `shiki list`/`shiki search`/`shiki show`/`shiki notebook list` all gained a `--json` flag, emitting
  structured output instead of plain text — none of the CLI's read commands had a machine-readable
  format before, which made them unusable from scripts/other programs without fragile text parsing.
- `shiki new` gained `--body <text>`/`--stdin` (reads the note body from stdin) and `--tags`, so a
  note can be created fully non-interactively — every content-producing CLI command used to
  unconditionally spawn `$EDITOR` and block on it, with no way to script note creation at all.
- `shiki export --notebook <name> --out <path> --format html|md` — a new command bundling every note
  in a notebook into a single file. `--format html` (the default) is real Markdown-to-HTML via
  `pulldown-cmark` (another previously-declared-but-unused workspace dependency) with a small
  embedded stylesheet (light/dark aware, no external assets); `--format md` is a plain concatenated
  Markdown bundle. There was previously no export path of any kind.
- Notebooks now tolerate `.txt` and `.mdx` files alongside `.md` when listing/reading notes
  (`Notebook::list_dir`'s `NOTE_EXTENSIONS`) — a notebook pointed at an existing Obsidian vault
  commonly has both, and they used to be silently invisible to shiki (not deleted, just never
  listed). New notes are still always created as `.md`; renaming a `.txt`/`.mdx` note now preserves
  its original extension instead of silently converting it to `.md`.

### Changed

- `shiki sync` (CLI) now resolves `auto_push`/`auto_commit` per-notebook (`Config::sync_for`)
  instead of always reading the global `[git]` defaults, matching the TUI's `s`/`u` behavior; it also
  now writes the same file-named commit message the TUI does (`git::diff_summary`) instead of a bare
  `"{prefix}sync"`, and gives a clear error instead of a raw libgit2 one when no remote is configured.
- `shiki search`/`shiki list`/`shiki notebook list`'s note counts now walk every folder
  (`all_notes_recursive`), not just the notebook root — a note nested in a subfolder used to be
  invisible to these commands while still being editable by name via `shiki edit`.
- `shiki new`/`shiki edit`/`shiki daily` now respect `general.use_favorite_editor` the same way the
  TUI's `i` binding does, instead of always using the plain configured `general.editor`.
- The self-updater (leader+`U`) now installs the *exact* version its confirm dialog showed, rather
  than re-resolving "latest" a second time — a new release landing in the gap between check and
  confirmation could previously install a different version than what was displayed.
- CI's `fmt-and-clippy` job is now matrixed across all three OSes (fmt itself still only runs once,
  on Linux) so clippy actually lints the `#[cfg(target_os = "macos"/"windows")]` blocks in
  `shiki-core/src/editor.rs`; the old separate `build` job was folded into `test` (it was paying for
  close to the same compilation a second time on every push).
- Every workflow job across all 5 `.github/workflows/*.yml` files now has a `timeout-minutes`, and
  `release.yml`'s `contents: write` permission is scoped to just the `release` job instead of the
  whole workflow.
- The marketing site: a working hamburger menu below 900px (the nav used to just disappear with no
  replacement), with a real focus trap and no lingering stale popover state when it closes; `.ref-table`
  now scrolls horizontally in its own container instead of the whole page; TOC/search jumps in
  `documentation.html` no longer land headings underneath the sticky top bars; the 12 per-theme
  gallery screenshots no longer double-download (wrong theme, then the right one) on load; the hero
  GIF respects `prefers-reduced-motion`; `loadLatestRelease`'s fetch has a timeout.

### Fixed

- On macOS, `NotebookStore::list()` could pick up the `templates` directory as if it were a real
  notebook. `directories::ProjectDirs` resolves `config_dir()` and `data_dir()` to the exact same
  path on macOS (unlike Linux/Windows), so `default_templates_dir()`'s plain, non-git `templates/`
  folder ended up sitting directly inside the data dir alongside real notebooks. Auto-discovered
  subdirectories of the data dir now require a `.git` directory to count as a notebook — a real
  distinction, not a heuristic, since every notebook is git-initialized immediately on creation.
- The empty NOTEBOOKS panel's hint said `press \`A\` to create one`, but the actual default
  keybinding for it is lowercase `a` (found by actually using the freshly-built app, not just
  reading the diff).
- A note's frontmatter parser used to truncate at the *first* line reading exactly `---` anywhere in
  the file — a YAML block scalar that legitimately contained such a line lost every field after it.
  It now requires that line to be the real closing delimiter.
- Frontmatter written with CRLF line endings (round-tripped through an external editor on Windows)
  used to fail to parse entirely, falling back to a synthesized title as if the note had no
  frontmatter at all.
- `create_note_in`/`rename_note_at` could silently overwrite another note whose title slugified to
  the same filename (e.g. "Q3 Report" vs. "Q3, Report!"); both now dedupe with a `-2`/`-3` suffix, and
  a symbol/emoji-only title falls back to a timestamp-based slug instead of an empty one.
- `Notebook::collect_notes`'s recursive walk had no guard against a self-referential symlink inside a
  notebook, which could stack-overflow global search.
- `Template::render` (used by note templates and the `/`-menu) did naive sequential
  find-and-replace per placeholder, so a variable's value containing another placeholder's literal
  text (e.g. a title like "Meeting {{date}} notes") got that text substituted again on the next pass.
  It now does a single left-to-right scan.
- `editor::command_for` split an editor command on plain whitespace with no quoting support, breaking
  any configured/detected editor whose path contains a space (common on Windows).
- `git::redact_credentials` treated the *first* `@` anywhere in a URL as credentials to redact — a
  self-hosted remote with a legitimate `@` in its path (e.g. `.../notes@backup.git`) got silently
  mangled in logs.
- `general.daily_template` was a fully documented, Settings-editable config field that
  `daily::create_or_open` never actually read — it always hardcoded `"daily"` regardless. Both the
  CLI (`shiki daily`) and the TUI now pass the configured value through.
- `copy_folder_to`/`move_folder_to` had no guard against a destination that is the source itself, or
  nested inside it — reachable through the `m` (move) prompt's prefilled address — which used to
  recurse forever creating nested copies of the folder inside itself.
- `git::status()` reported a notebook as clean (`dirty_count = 0`) even when the underlying
  `repo.statuses(None)` call itself failed (locked index, permissions, a corrupted repo); the footer
  and drawer now show a distinct "status?" indicator instead.
- Applying a slash-command or a `[[wikilink]]` selection while multiple cursors were active only
  ever edited the primary cursor's text, leaving every secondary cursor with its own unconsumed
  literal text and a desynced position for the rest of the editing session. Both now collapse to a
  single cursor before applying.
- `p` (pull one notebook) reset the NOTES selection and preview scroll back to the top whenever the
  pulled notebook was still selected, even if the user had since navigated to a different note/folder
  within it — same bug already fixed for `P` (pull all), now fixed for the single-notebook case too.
- A hand-edited or corrupted `session.toml` with `notes_path` containing `".."` components could
  make the NOTES panel navigate outside the notebook (and the data directory) on restore; path
  components are now sanitized on load.
- `find_note` (used by `edit`/`show`/`daily`/`new`) could silently resolve to the wrong note when two
  notes in different folders shared a title/slug, a possible ambiguity introduced when note lookup
  became recursive; it now errors with both folders listed instead of guessing.
- `TagIndex::build` was rebuilt on every draw tick while the tags modal was open, and even after
  being cached, `App::tag_index()` still cloned the whole cached index on every draw call — both are
  now a real cache hit with no per-frame rebuild or clone.
- `wikilinks::backlinks` re-scanned every note (with a fresh `slugify` call) once per link per note,
  making it cost O(notes × total links in the notebook); it now builds a title/slug index once.
- `search_text` (global search) allocated a fresh scratch buffer per note on every keystroke instead
  of reusing one across the scan.

## [0.8.10] - 2026-07-31

### Added

- `[[wikilink]]` autocomplete in the inline editor: typing `[[` opens an Obsidian-style fuzzy note
  picker (reusing the existing `SearchEngine`, same fuzzy match `/` notebook-jump and global search
  already use), showing each candidate's folder breadcrumb so notes with duplicate titles in
  different folders stay distinguishable. Candidates are snapshotted once when the menu opens
  (excluding the note being edited) and re-scored per keystroke, the same "expensive walk once,
  cheap re-score" shape the `/`-command menu and global search already use. In PREVIEW, Ctrl+Click
  on a rendered `[[wikilink]]` jumps straight to the linked note; a plain click still enters edit
  mode everywhere, including on a wikilink, so this is purely additive.

### Changed

- Deleting a notebook (notebooks-scope `d`) now asks a real question instead of always destroying
  the directory: `[d]` deletes the notebook's files for real, `[r]` only stops tracking it as a
  notebook and leaves the directory completely untouched on disk, `[Esc]` cancels. Previously this
  confirm dialog was a plain "delete notebook and all its notes? (y/n)" that unconditionally called
  `remove_dir_all` on `y` — dangerous for a notebook adopted from an external directory (an
  Obsidian vault, an existing repo) where the folder was never shiki's to destroy in the first
  place. "Just untrack" sets a new `[notebooks.<name>] hidden = true` in `config.toml`; there's no
  in-app way to undo that yet, so reversing it means clearing the flag by hand and relaunching.
- Gruvbox's `accent`/`link` now use the palette's iconic yellow (`#fabd2f` dark, `#b57614` light)
  instead of blue, matching every other included theme's own convention of `accent == link`. All
  panel/modal borders switched from Rounded to Plain when unfocused for a fully square look, and
  PREVIEW gets a new thinner Plain (not Thick) accent-colored border specifically while reading a
  note's body — a full-width Thick border around a wall of text read as visually louder than the
  same emphasis on a narrow list panel. The marketing site's `docs/css`/`docs/js` theme copies were
  updated to match, per this project's own "keep both in sync" convention.

### Fixed

- Creating a note (`a` in Notes) now switches focus to Preview instead of leaving it on Notes: the
  fresh note's editor used to only get the collapsed 1-column sliver on narrow/short terminals
  (`single` layout tier renders only the focused panel), so it looked like nothing happened until
  the terminal was resized wider.
- Saving the scratchpad (leader+`p`, Ctrl+S) as a note no longer opens the template picker: since
  the scratchpad body always won over a rendered template anyway, picking a real template there
  previously created a note whose body was just the raw scratchpad text but whose `template:`
  frontmatter field named a template that was never actually applied. It now skips straight to a
  blank note using the scratchpad's own text, and `create_note_with_template` no longer stamps
  `frontmatter.template` in any path unless that template's render genuinely became the note's body.

## [0.8.9] - 2026-07-30

### Added

- `general.remember_last_session` (on by default): quitting the TUI now saves exactly where you
  were — the selected notebook, the folder inside it, the selected note or folder, and which panel
  (NOTEBOOKS/NOTES/PREVIEW) had focus — and the next launch restores it verbatim instead of always
  starting at the first notebook's root. Toggleable from the GENERAL tab in Settings (leader+`s`)
  or by hand-editing `config.toml`. A renamed/deleted notebook or a moved note is silently ignored
  rather than erroring — the app just falls back to its normal default startup state.
- Homebrew tap support: `brew tap sazardev/shiki && brew install shiki` installs a prebuilt binary
  on macOS (Intel and Apple Silicon). `packaging/homebrew/shiki.rb` is regenerated from each
  release's checksums and pushed to the `sazardev/homebrew-shiki` tap automatically on every
  tagged release, the same automation shape `release.yml` already used for the AUR/Scoop
  manifests. Documented on the marketing site and in `README.md`.

### Changed

- 5 dependencies bumped to their latest compatible versions: `comrak`, `toml`, `thiserror`,
  `directories`, `base64` — no behavior change.

### Security

- `ratatui` bumped 0.29 → 0.30 and `tui-textarea` swapped for `ratatui-textarea` 0.9 (its
  maintained successor under the ratatui org), closing the Dependabot alert on `lru`
  (GHSA-rhfx-m35p-ff5j, an `IterMut` soundness issue) pulled in transitively via `ratatui` —
  `tui-textarea` 0.7 never moved off the vulnerable `ratatui`/`lru` line, so the fix required
  moving the whole editor widget, not just bumping a version number. `crossterm` bumped 0.28 →
  0.29 alongside it to stay unified with what `ratatui` 0.30 itself pulls in. Fallout from both
  bumps is fixed throughout: `TextArea::cursor()` now returns a `DataCursor` struct instead of a
  plain tuple, `List::highlight_symbol` no longer accepts `&String`, and ratatui's `Backend` trait
  gained an associated `Error` type.

## [0.8.8] - 2026-07-29

### Added

- In-memory scratchpad (`leader+p`) for temporary writing. `Ctrl+S` saves its contents as a real
  note through the existing new-note flow; closing it with `Esc` discards the buffer.

- New `[editor]` config section (EDITOR tab in Settings, leader+`s`) — six independently toggleable
  behaviors for the native note editor's mouse/keyboard UX: `mouse_selection`, `find_replace`,
  `os_clipboard`, `select_all_ctrl_a`, `line_numbers`, `multi_cursor`.
- `editor.mouse_selection` (on by default): click to position the cursor inside the note editor,
  click-and-drag to select, double-click to select a word, triple-click to select a line — the
  editor had no mouse support at all before this.
- `editor.line_numbers` (off by default): a line-number gutter in the editor.
- `editor.find_replace` (on by default): Ctrl+F opens a find/replace bar inside the editor —
  live search-as-you-type, Enter/Shift+Enter for next/previous match, Ctrl+Enter to replace the
  current match and advance, Ctrl+Alt+Enter to replace every occurrence.
- `editor.os_clipboard` (off by default): Ctrl+C/X/V in the editor use the real OS clipboard
  (via `arboard`), falling back automatically to the existing OSC 52 mechanism when there's no
  display server to reach (e.g. headless SSH). Bracketed-paste is now enabled unconditionally so
  a terminal paste anywhere in the app lands as one atomic insert instead of a burst of keystrokes.
- `editor.select_all_ctrl_a` (off by default): Ctrl+A selects the whole buffer instead of
  tui-textarea's default Emacs-style "move to start of line".
- `editor.multi_cursor` (off by default): full multi-cursor editing. Alt+Click adds a cursor
  (dedups against existing ones); Ctrl+Alt+Up/Down adds one at the same column one row up/down
  from whichever cursor sits furthest in that direction (VS Code's own "Add Cursor Above/Below",
  a keyboard-only alternative added after Alt+Click alone felt uncomfortable in practice); Ctrl+D
  selects the word under the primary cursor and, on each further press, adds the next occurrence
  as its own independently-selecting cursor, wrapping around the buffer and reporting "no more
  occurrences" once every match already has a cursor. Every keystroke (typing, Backspace/Delete,
  Enter, navigation) replays across the primary and every secondary cursor via a new
  `multicursor` module built specifically because tui-textarea itself has no multi-cursor concept
  at all — see the Fixed section below for the real bugs this uncovered along the way. Ctrl+U/
  Ctrl+R undo/redo a multi-cursor edit as one action (however many cursors it actually mutated),
  not one step per cursor. Esc collapses back to a single cursor first (VS Code's own
  convention); a second Esc then saves and exits as usual. Secondary cursors render as a solid
  accent-colored block, distinct from the primary's plain reverse-video.
- A plain (non-dragged) mouse click on a PREVIEW row now jumps straight into `Mode::Edit` with the
  cursor on that clicked line — a mouse-only alternative to `i`/vim motions for anyone reading a
  note who wants to start editing it. Click-and-drag still selects text and copies it to the
  clipboard on release, same as before; only the plain-click case changed, gated behind the same
  existing `mouse_drag_selection` toggle. Contributed by @elsieej (#23).

### Fixed

- Mouse wheel scroll was never handled anywhere at all — reported live: scrolling over PREVIEW or
  the editor did nothing. Now scrolls the editor's cursor (`Mode::Edit`, which in turn scrolls the
  view — see the next entry) or reuses `move_selection`'s existing delta logic (`Mode::Normal`/
  `Visual`, covers NOTEBOOKS/NOTES/PREVIEW the same way `j`/`k` already do), gated behind the same
  "no modal is open" guard mouse clicks already use.
- `PageUp`/`PageDown` did nothing at all inside the note editor (`Mode::Edit`) — reported live
  alongside the mouse-scroll gap. Root cause: forwarding them to `tui-textarea` reaches
  `Scrolling::PageUp/PageDown`, which scrolls its *internal* `Viewport` — but that viewport is
  only ever populated by `tui-textarea`'s own `Widget` impl, and `InlineEditor::render`
  deliberately bypasses that entirely (word-wrap support, see the struct's own doc comment), so
  the viewport being scrolled is permanently zero-sized and the cursor never actually moves.
  Fixed by moving the cursor directly instead, which `InlineEditor`'s own scroll-follow (driven
  purely by cursor position) then scrolls the view to match.
- Added Ctrl+Home/Ctrl+End to jump to the very start/end of the note — plain Home/End already
  worked correctly (`tui-textarea`'s own start/end-of-*current-line*, no viewport dependency
  involved), but there was no way to jump to the start/end of the whole document at all.
- Multi-cursor's own replay algorithm initially processed cursors bottom-to-top on the assumption
  that this alone avoids invalidating other cursors' positions — caught by a failing unit test
  before it ever reached a person: an edit that changes row count (Enter, or backspace-merging two
  rows) *does* still invalidate an already-processed cursor's recorded result once a later
  (higher, "more top") edit shifts rows below it. Replaced with the standard top-to-bottom
  algorithm that tracks a running row/column delta instead, derived from `lines().len()`/cursor
  position before and after each per-cursor edit rather than reimplementing tui-textarea's own
  insert/delete arithmetic by hand.
- A single `tui_textarea::TextArea::input()` call can silently push *two* separate undo-history
  entries, not always one — verified directly against the vendored crate: typing a character over
  an active selection is "delete the selection, then insert the character" (2 entries), while
  Backspace over that same selection is just "delete" (1 entry). Assuming a flat count per
  keystroke (as an earlier version of this work did) left Ctrl+U undoing only half of a
  multi-cursor edit that replaced a selection, e.g. after Ctrl+D-selecting three occurrences and
  typing a replacement. Fixed by measuring the real count directly — undo repeatedly until the
  pre-edit content reappears, then redo back to the post-edit state — instead of guessing per key.
- Secondary cursors reused `cursor_style()` (tui-textarea's default `Modifier::REVERSED`, the same
  style the primary cursor uses) and had no fallback for sitting past the last character of a
  line — reported live: "solo hay 1 visualmente, no parpadean varios cursores." A terminal can
  only ever blink one real caret, so secondary cursors were never going to blink like the primary
  regardless of styling, but reusing the *exact same* subtle reverse-video look made them easy to
  miss entirely, and a cursor landing at end-of-line (the common case right after typing) had
  nothing to render at all. Fixed with a distinct solid accent-colored block style plus the
  missing end-of-line fallback.

- `arboard`'s clipboard handle is now kept alive for the process's lifetime instead of being
  constructed and dropped on every copy/paste — the previous per-call pattern hit arboard's own
  X11 "clipboard was dropped very quickly" guard, which `eprintln!`s a warning straight to the
  real terminal underneath ratatui's alternate screen, visibly corrupting the TUI on every
  Ctrl+C/Ctrl+V. Caught live by checking the real system clipboard (`wl-copy`/`wl-paste`) around
  a Ctrl+C, not just by reasoning about the code.

## [0.8.7] - 2026-07-28

### Added

- New-notebook (`a`) now accepts a filesystem path (`/abs/path`, `~/docs`, `./relative`) as a
  third fast path alongside a plain name (create empty) and a git URL (clone): it adopts that
  existing directory as a notebook instead, deriving the name from the last path segment. If the
  directory has no `.git` yet, it asks for confirmation before initializing one, keeping the
  existing rule that every notebook in shiki is git-managed; if it's already a repo, it's adopted
  immediately. Registered via the same `[notebooks.<name>] path` config field the existing
  "point at an Obsidian vault subfolder" feature already used, so it's picked up on every future
  launch too. Existing notes already inside the adopted folder show up right away.
- `general.show_hints` (on by default, toggleable from Settings' GENERAL tab) shows a small muted
  hint line under the input box for prompts whose behavior isn't obvious from the title alone —
  currently just new-notebook, explaining both the git-URL-clones and path-adopts fast paths.

## [0.8.6] - 2026-07-28

### Added

- Configurable notebooks root directory and per-notebook custom paths, so shiki can point at an
  existing collection of markdown notes (e.g. an Obsidian vault) instead of only ever living
  under its own data directory:
  - `[general] data_dir` overrides the default notebooks root — every notebook without its own
    `path` lives under this directory instead of the platform default.
  - `[notebooks.<name>] path` points an individual notebook at any absolute directory on disk,
    independent of `data_dir` — multiple notebooks can each link to different external directory
    trees (e.g. separate Obsidian vault subfolders) without moving or symlinking a single file.
  - `shiki doctor` warns when a configured `path` isn't absolute, since a relative one would
    otherwise resolve against whatever directory the process happened to be launched from
    (terminal vs. desktop entry vs. cron) rather than a stable location.
- Mouse drag-to-select and copy in PREVIEW: click-and-drag over a note's rendered body highlights
  the dragged rows (using the theme's `selection` color) and copies them to the clipboard
  (OSC 52) the instant the mouse button is released — no extra keypress needed, same mechanism
  the logs modal's `y`/`c` already uses. Toggle via the new `general.mouse_drag_selection` config
  key (`true` by default), also editable from Settings' GENERAL tab.

### Fixed

- Renaming a notebook with a custom `path` used to silently move its directory into shiki's own
  data directory instead of keeping it where it actually lived (e.g. inside an Obsidian vault) —
  it now stays in place unless the new name has its own configured `path`.

## [0.8.5] - 2026-07-27

### Changed

- Settings screen (leader+`s`) is now fully interactive instead of read-only — every tab lets you
  edit values in place with no need to drop into `config.toml` by hand:
  - Paged by tab (`←`/`→` switches GENERAL/THEME/GIT/NOTEBOOKS/SNIPPETS) instead of one long scroll.
  - GENERAL/GIT: booleans (`use_favorite_editor`, `auto_commit`/`auto_push`/`sign_commits`/
    `auto_sync`) toggle in place on `Enter`; every other field opens a prompt prefilled with its
    current value.
  - THEME: `name` opens the existing theme picker (live preview, same as leader+`c`); `overrides`
    stays informational, pointing at leader+`c`/`shiki theme create --from` instead.
  - NOTEBOOKS: the list now shows every notebook's actual git remote (redacted), not just ones with
    a config override — previously there was no way to see, let alone change, which repo a notebook
    was synced to from Settings at all. `Enter` drills into a notebook to edit its remote and its
    `auto_push`/`auto_sync`/`auto_sync_every` overrides (booleans cycle inherit → true → false →
    inherit; the notebook's `[notebooks.<name>]` table is removed automatically once every override
    is back to inherit).
  - SNIPPETS: `a` creates a new snippet (prompts for a trigger), `d` deletes one (with
    confirmation); drilling into one edits its `label` and its full multi-line `body` through the
    same inline editor a note's own body uses.
  - `i`/`E` still jump straight to editing `config.toml` itself for anything not covered above.

## [0.8.4] - 2026-07-27

### Added

- Settings screen (leader+`s`) — a read-only, near-full-screen summary of the current config
  (general/theme/git/per-notebook overrides/snippets), grouped by section, scrollable with
  `j`/`k`/`PageUp`/`PageDown`/`Home`/`End`. `i`/`E` jump straight to editing `config.toml` itself
  (inline or externally, same convention as editing a note); on save the config is re-parsed,
  applied immediately (theme, keybindings, favorite editor — no restart), and an invalid edit is
  reported without being written or applied, keeping the previous config running.
- A fresh install's `config.toml` is now fully commented, section by section — generated from the
  real `Config::default()` values (never hand-duplicated, so a comment can go stale but a default
  value never can) with prose explaining each table.
- 7 more built-in `/`-menu commands, so more of it is useful out of the box with zero config:
  `bullet`/`numbered` list items, `link`, `image`, `note`/`warning` callouts, and a collapsible
  `details` section — alongside the existing headers/code/math/table/checklist/quote/divider/
  date/tags/frontmatter, for 19 built-ins total.
- `shiki doctor` now flags configuration mistakes that used to fail silently: two keybindings in
  the same scope bound to the same key (only one ever actually works), a keybinding that collides
  with `leader` or `quit` (always loses to them, so it can never trigger), a keybinding string that
  doesn't parse to any real key, and two `/`-menu snippet triggers that collide case-insensitively
  (e.g. `[snippets.H1]` next to `[snippets.h1]`).
- 3 more built-in themes: **Dracula**, **One Dark**, and **Monokai** — 15 total now. Palettes taken
  from each project's own official/canonical values, same standard as every other included theme.

### Fixed

- `config.toml` tables missing an individual key (e.g. `[general]` with only `default_notebook`
  set) used to fail parsing the *entire* config instead of falling back to that one field's
  default — now every field across every table has its own default, so a partial hand-edit (more
  likely now that Settings invites editing this file directly) never takes down the whole app.
- Two `/`-menu snippet triggers colliding case-insensitively (`[snippets.H1]` and `[snippets.h1]`
  in the same config) used to resolve to whichever one `HashMap` iteration happened to visit last
  — different, unpredictably, between runs of the exact same config. Now sorted deterministically,
  so the outcome (while still worth fixing — `shiki doctor` reports it) is at least reproducible.
- PREVIEW's Markdown renderer never actually rendered `**bold**`/`*italic*`/`` `code` `` (the
  asterisks/backticks showed up literally), tables, `$$math$$` blocks, `<details>`/`<summary>`,
  ordered lists, horizontal rules, or `[text](url)`/`![alt](url)` links/images — so several of the
  `/`-menu's own blocks (table, math, note/warning callouts, details, link, image) looked broken
  the moment you left edit mode, since a callout's `> **Warning:** ...` showed the bold markers
  raw instead of bolding the text. All of the above are now recognized and styled; `<details>` is
  shown fully expanded (a static preview pane has no fold state to toggle against) rather than
  actually collapsing.

## [0.8.3] - 2026-07-27

### Added

- `/`-menu in the inline editor: typing `/` as the first character of a line opens a small
  searchable menu (filters as you keep typing, `↑`/`↓` navigates, `Enter` inserts, `Esc` closes the
  menu without leaving edit mode) with ready-to-insert blocks — `h1`/`h2`/`h3`, a code fence, a math
  block, a table, a checklist item, a quote, a divider, today's date, a tags line, and a YAML
  frontmatter block. `/` anywhere else on the line (a URL, a fraction) is still a plain character.
- `/`-menu commands are fully customizable via `[snippets.<trigger>]` in `config.toml` — each entry
  can add a new block or redefine an existing one (same trigger, case-insensitive). Supports
  `{{title}}`/`{{date}}` (same as note templates) and a `{{cursor}}` marker for where the cursor
  lands after insertion.
- `@` dropdown in the new-note title prompt (`a`): typing `@` after the title (or alone, with no
  title) opens a dropdown with `today`/`yesterday`/`tomorrow` (a computed date, no template) plus
  every available template — filters as you type, `Enter` creates the note and jumps straight to
  editing, skipping the normal "title → Enter → pick a template" flow.
- 9 new templates alongside the existing 3 (`default`/`daily`/`meeting`): `bug`, `spec`, `review`,
  `postmortem` (dev), `standup`, `retro`, `1on1`, `weekly` (productivity/meetings), and `brainstorm`
  (general) — generated automatically in `~/.config/shiki/templates/` on next launch, without
  touching any template you've already customized.
- The inline editor shows a placeholder ("Type '/' for quick blocks...") when the note is empty, so
  the `/`-menu is discoverable without reading the docs.

### Fixed

- The inline editor now wraps long lines to the panel's width (same as PREVIEW) instead of
  scrolling them off-screen horizontally — `tui-textarea` has no wrap support in any published
  version, so the editor's rendering is now computed by hand (the same wrap math is reused both to
  draw the text and to place the cursor, so the two can never disagree), while all real editing
  (insert/delete/undo/selection) still goes through `tui-textarea` unchanged.

## [0.8.2] - 2026-07-24

### Fixed

- The CLI's `find_note` now searches subfolders recursively (`all_notes_recursive`), not just the
  notebook's root — `shiki edit`/`shiki show` now find nested notes.
- `synthesize_frontmatter` now assigns the correct notebook name for notes with no YAML frontmatter
  that live several levels deep inside the notebook (it used to use the intermediate folder).
- `apply_pending_batch` reports the actual error message instead of a generic "already exists
  there?" when a move/copy fails.
- A potential `unwrap()` in `start_move_or_copy` replaced with `let Some(nb) = ... else`.
- `render_global_search` checks bounds before indexing `global_search_pool`, avoiding a panic if a
  reload happens between the search and the render.

### Changed

- 4 dead dependencies removed from `shiki-core`: `notify`, `pulldown-cmark`, `anyhow`, `uuid` (none
  were used anywhere in the source).
- `app.rs` split into 4 modules (`draw.rs`, `sync.rs`, `key_handlers.rs`, `app.rs`): went from 4057
  to 1508 lines. No behavior change, reorganization only.

## [0.8.1] - 2026-07-23

### Added

- Footer now shows a clickable "☕ Support" link (`buymeacoffee.com/sazarcode`) — opens in the
  default browser cross-platform via a new `shiki_core::browser::open_url`.
- Scripted, reproducible demo GIF (`scripts/demo-gif.sh`, recorded with VHS) covering global and
  in-notebook fuzzy search (with a real cross-notebook jump), tags, real multi-select with a batch
  delete, creating and moving folders, writing a full note from scratch in the inline editor, a
  git commit, and live theme switching — featured in the hero of the marketing site (playing on
  page load) and in a dedicated Demo section, plus `README.md`.
- Links modal (`L` in PREVIEW) — the selected note's outgoing `[[wikilinks]]` (resolved against
  every note in the notebook, any folder depth — not just its own directory) plus every other note
  that links back to it, with `Enter` jumping straight to either. `shiki_core::wikilinks` already
  had `extract`/`resolve` written but nothing called them; `resolve` is also fixed to search the
  whole notebook recursively instead of only a note's own directory.
- Deleting a note or folder now moves it to a trash directory (`~/.config/shiki/trash/`) instead
  of removing it permanently; leader+`u` restores the most recently deleted note/folder (or whole
  batch, from a Visual-mode delete) — one level of undo, not a full history.
- `a` (new note) now opens a template picker after the title — every `.md` file in
  `~/.config/shiki/templates/` plus a "blank" option — instead of always starting from an empty
  body. The chosen template's name is recorded in the note's own `template` frontmatter field.
- Footer's character count (NOTES/PREVIEW, a note selected) now also shows word count and an
  estimated reading time (200wpm).

### Changed

- Fresh installs now default to the `gruvbox-dark` theme instead of `catppuccin-mocha`.
- Delete confirmation prompts no longer say "this can't be undone" — it can now, via leader+`u`.

## [0.8.0] - 2026-07-23

### Added

- Notes-scope `v` enters real multi-select (`Mode::Visual` — declared long ago but never wired
  up): `j`/`k` extend a selection range, shown highlighted in the list and as `VISUAL (n
  selected)` in the footer. `d`/`m` then act on every selected item at once (delete, move);
  `y` copies the whole selection to a prompted target, leaving the originals in place.
- `d` (delete) and `m` (move) now work on folders, not just notes — a folder delete removes
  everything inside it (with confirmation); previously selecting a folder and pressing either key
  silently did nothing.
- `m`'s prompt is now `notebook/path/within/it`, prefilled with the current location — edit the
  trailing segments to move within the same notebook (missing folders are created automatically),
  or replace the first segment to move to a different notebook entirely. The target notebook must
  already exist (errors clearly otherwise — a notebook is a new git repo, so one is never silently
  created from a typo).
- 4 new `shiki-core` primitives backing all of the above: `copy_note_to`/`move_note_to` (rewriting
  a note's `frontmatter.notebook` when it actually crosses notebooks) and
  `copy_folder_to`/`move_folder_to` (recursive, preserving nested structure and empty subfolders),
  plus `delete_folder_at`. All error rather than silently overwriting if the destination already
  has something there. 7 new unit tests.

## [0.7.0] - 2026-07-23

### Added

- Tags modal (leader+`T`) is now real, two-level navigation instead of a read-only list: `j`/`k`
  browse tags, `Enter`/`l` drills into the notes carrying one, `Enter`/`l` there jumps straight to
  it, `h`/`Esc`/`Backspace` goes back a level.
- `git.remote_template` config option: auto-configures a notebook's remote on creation (plain
  name, not a pasted URL) from a template like `"git@git.example.com:notes/{notebook}.git"` —
  the remote still has to already exist on that server; this doesn't create one via any hosting
  provider's API. Doesn't push immediately (nothing to push yet on an empty notebook); the
  existing `auto_push`/`auto_sync` machinery picks it up naturally.
- Persistent, on-disk log history (`~/.config/shiki/shiki.log`) — the logs modal (leader+`l`) now
  survives restarts instead of resetting every session, and a new `x` (behind a confirmation)
  clears both the in-memory and on-disk history.
- `shiki theme create [--from <name>]`: scaffolds all 19 theme color slots into config.toml's
  `[theme.overrides]` at once, copied from a real palette, instead of hand-typing hex codes with
  no example to copy from.
- First unit tests in `shiki-core` (`git::tests`) and `shiki-config` (`config::tests`).

### Changed

- All 19 of a theme's color slots are now overridable in `[theme.overrides]`, not just 5
  (`bg`/`fg`/`accent`/`selection`/`border`) — `error`/`warning`/`success`/`tag`/`link`/`cursor`
  and 8 others had no override path at all before.
- Git remote URLs are redacted (`user:token@` → `***@`) before they ever reach a status message —
  closes a real exposure: a URL with embedded credentials (common for GitHub/GitLab personal
  access tokens) used to land in plaintext in the logs modal and clipboard, and would now also
  have been persisted to disk.

### Fixed

- The theme picker's `Enter` and `shiki theme set` no longer wipe custom color overrides when
  re-confirming/re-setting the theme that was already active with no actual change — previously
  they reset `[theme.overrides]` unconditionally on every confirm, silently discarding any
  hand-written custom colors even when nothing was actually being switched.

## [0.6.0] - 2026-07-23

### Added

- Notebook drawer (`leader+b`): a collapsible left-side sidebar listing every notebook's git
  status in color (dirty count, ahead/behind), separate from the always-visible NOTEBOOKS panel —
  `j`/`k`/`Enter` or a mouse click jumps to a notebook, `n`/`i` (or clicking the minimal "New"/
  "Import" buttons at the bottom) open the same new-notebook prompt that already detects a pasted
  git URL and clones instead of creating a plain notebook.
- Per-note coloring in NOTES: each note's title is tinted by its actual git status (new → green,
  modified/renamed → the warning color, deleted → the error color) instead of only being visible
  as an aggregate count in the footer — `shiki_core::git::file_statuses`.
- A spinner (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`) in the footer while a sync/push/pull is running in the background,
  replacing the git-status segment for the duration — visible feedback that something's actually
  happening on a slow network call instead of the UI just looking idle.
- First unit tests in `shiki-tui` (`panel_drawer::tests`), covering the drawer's mouse
  hit-testing math — caught a real off-by-one in the button row's coordinates before it shipped.

### Changed

- `sync`/`push`/`pull`/`pull all` now run on a background thread (the same `std::thread` + `mpsc`
  pattern already used by the in-TUI self-updater) instead of blocking the render loop for the
  duration of the git/network call. Only one operation runs at a time; a second request while one
  is in flight is reported and dropped rather than queued. Verified live against a real local
  bare-repo remote (successful push) and an unreachable one (commit succeeds, push fails cleanly,
  state refreshes correctly) — the UI never freezes in either case.

## [0.5.1] - 2026-07-23

### Added

- `scripts/benchmark.sh`: an automated, transparent CPU/RAM/responsiveness benchmark that drives
  the real release binary headlessly via tmux across seven scenarios, from an empty notebook up to
  100,000 notes in one folder, 200 levels of nested folders, and a single 300,000-line note —
  samples real `/proc/<pid>/stat`/`status` numbers (CPU ticks, VmRSS, wall-clock time to first
  rendered frame) rather than estimating anything, so it doubles as a freeze/hang check.

### Changed

- PREVIEW's note view and folder-peek now cache their formatted output (`App::note_preview_cache`/
  `folder_preview_cache`) instead of re-formatting on every ~100ms draw tick, and borrow rather than
  clone that cached text into the `Paragraph` (`render::borrow_lines`). Previously, a selected
  note's entire body was reformatted via `markdown_to_lines` on every redraw regardless of size,
  and a selected-but-not-entered folder re-listed the directory and re-parsed every note's
  frontmatter on every redraw regardless of folder size — both scaled with content size at ~10Hz
  whether or not anything had actually changed. Verified via `scripts/benchmark.sh`'s aggressive
  scenarios: a 100,000-note folder now costs ~4.7% idle CPU (found via the same benchmark to be in
  the double digits before formatted-output caching was added) and a 300,000-line note ~9.9%, both
  with a sub-second first frame and zero measured RSS drift.

## [0.5.0] - 2026-07-23

### Added

- Notes-scope `f` creates a new (empty) subfolder at the current breadcrumb depth — previously
  folders could only be navigated, never created from the TUI; the only way to get one was to
  already have it on disk (an imported repo, or made outside shiki entirely).

## [0.4.2] - 2026-07-23

### Added

- In-TUI self-update (leader+`U`): checks GitHub Releases for a newer version without downloading
  anything, shows "Update available: vX.Y.Z → vA.B.C" if one exists, and on `enter` downloads,
  verifies (against GitHub's own per-asset sha256 digest), and installs it in place of the running
  binary — then automatically relaunches into it, no manual restart needed. Runs on a background
  thread so the TUI never freezes on the network call. Verified live end-to-end against the real
  repo: detects an available update, declines to re-flag when already current, and a full
  download → verify → install → relaunch round trip that lands on the new version's footer.

### Security

- `git2` bumped 0.19 → 0.21, closing 3 `cargo audit` "unsound" advisories
  (`Remote::list()`/`BlameHunk` signature/`Buf` dereference UB) — shiki's code never called any of
  the affected APIs, but they showed up in every audit regardless while pinned to 0.19.
  `Commit::summary()`/`Reference::shorthand()`/`Reference::name()`/`Remote::url()` all changed from
  `Option`-returning to `Result`-returning between these versions; every call site in `git.rs` was
  updated to match. `ssh`/`https` are now explicit features (git2 0.21's `default-features` became
  empty, whereas 0.19 defaulted to them) — without this, SSH remotes and `Cred::credential_helper`
  would've silently stopped working. Verified live: full push → remote commit → pull-into-fresh-
  notebook → note-history round trip against a local bare repo. `cargo audit` is down to 4
  warnings, all transitive via `syntect`/`ratatui` and not fixable from shiki's own `Cargo.toml`.
- `.github/workflows/ci.yml` now declares `permissions: contents: read` explicitly instead of
  inheriting the repo's default token permissions — it only builds/lints, never needs write access.

## [0.4.1] - 2026-07-23

### Added

- `shiki-core`/`shiki-config`/`shiki-tui`/`shiki-cli` are now published to
  [crates.io](https://crates.io/crates/shiki-cli) — `CARGO_REGISTRY_TOKEN` is configured, so
  `cargo install shiki-cli` works directly, no `--git`/`--path` needed. This is the release that
  verifies the `publish-crates` job actually publishes for real (previous tags skipped it since the
  secret wasn't set yet).

## [0.4.0] - 2026-07-22

### Added

- Automated release packaging: `.github/workflows/ci.yml` runs fmt/clippy/build on a Linux +
  Windows + macOS matrix on every push/PR; `.github/workflows/release.yml` builds release binaries
  for all four platform targets on every `v*` tag, publishes them (with checksums) as a GitHub
  Release, auto-updates the AUR (`packaging/aur/PKGBUILD`, `shiki-bin`) and
  Scoop (`packaging/scoop/shiki.json`) manifests with the new version/hashes, and (behind
  `CARGO_REGISTRY_TOKEN`/`AUR_SSH_PRIVATE_KEY` secrets, not yet configured) publishes to crates.io
  and pushes to the real AUR git repo.
- Installable via `yay`/`paru` (`shiki-bin`, once published to the AUR — requires a one-time
  manual AUR account/SSH key setup that only the repo owner can do), `scoop` (direct manifest URL,
  no bucket needed), a prebuilt binary from GitHub Releases, or `cargo install --path shiki-cli`
  from source — see the README's expanded Install section.

### Changed

- `git2` now builds with `vendored-libgit2`/`vendored-openssl`, statically linking libgit2/OpenSSL
  instead of depending on whatever (if anything) is installed on the system — required for
  reliable Windows builds (no system libgit2 there) and makes Linux/macOS builds portable too.
- Workspace path-dependencies (`shiki-core`, `shiki-config`, `shiki-tui`) now carry an explicit
  `version` alongside `path`, required for `cargo package`/`cargo publish` to succeed (previously
  failed with "dependency does not specify a version").

## [0.3.0] - 2026-07-22

### Changed

- Footer status messages now clear themselves after 2 seconds instead of sitting there until the
  next action happens to overwrite them, and are truncated to whatever footer space is actually
  left instead of overflowing. Nothing is lost either way — every message is still recorded in
  full in the logs modal (leader+`l`) regardless of how briefly or how much of it the footer shows.
- A bit more padding around the right-aligned `? help  vX.Y.Z` in the footer, so it doesn't sit
  flush against the terminal edge or the rest of the footer content.

## [0.2.0] - 2026-07-22

### Added

- Note version history (PREVIEW-scope `H`): every commit that changed the specific note being
  read, newest first — real git history, not a separate versioning system. `Enter` views a
  revision's full content, `r` reverts to it (behind a confirmation). A revert doesn't commit by
  itself; it becomes a normal pending change picked up by `s`/`u`/`auto_sync` like any other edit.
  The footer shows the count while reading a note (`{n} changes`).
- `D` (notes-scope) toggles each note's date next to its title in the NOTES list, off by default.
- The 3-panel layout is now responsive to terminal size instead of one fixed arrangement: wide
  terminals keep the original side-by-side columns; narrow-but-tall or square terminals stack the
  same panels vertically instead (still full-width, just not side-by-side); very small terminals
  show only the focused panel, full screen, with no collapsed siblings. Navigation (`hjkl`/`tab`)
  works identically at every size. Verified by resizing an actual terminal from 200×50 down to
  20×8 with no crash or broken rendering at any point.
- Footer now shows which editor mode is active — the resolved favorite editor's name (e.g. `nvim`)
  when `general.use_favorite_editor` is on, `native` (the built-in inline editor) when it's off —
  plus a new leader+`e` shortcut to toggle it on/off and persist the change immediately, instead of
  hand-editing config.toml.
- `shiki doctor`: an environment health check (config validity, data/templates dirs, `git`/`gh`
  on `$PATH`, terminal truecolor support, configured editor, notebook/remote summary). Works even
  when `config.toml` is malformed — unlike every other command, it diagnoses that instead of
  failing outright.
- `README.md` and `LICENSE` (MIT) — install (`cargo install --path shiki-cli`), update, and
  verify (`shiki --version`, `shiki doctor`) instructions for installing from a clone, since this
  isn't published to crates.io yet. Every crate's `Cargo.toml` now also carries `repository`
  (previously only set at the workspace level but never actually inherited by any crate),
  `keywords`, `categories`, and a `readme` pointing at it.
- `auto_sync`: a notebook can sync itself (commit, + push if `auto_push`) automatically every
  `auto_sync_every` note changes, instead of only on manual `s`. Push failures (no internet, auth,
  etc.) never block — the commit already happened locally, and the next attempt just retries.
- `u`: commits and always pushes, regardless of `auto_push`/`auto_sync` — the explicit "sync right
  now" override.
- Which-key (`?`) is now a near-full-screen searchable list instead of a small centered popup: type
  to filter by key/action/scope, `↑`/`↓`/`PageUp`/`PageDown`/`Home`/`End` to move the selection,
  `Enter` runs the highlighted action immediately — doubles as a fast command palette.

### Changed

- Selecting a folder (not a note) in NOTES now previews what's actually inside it in PREVIEW
  (subfolders, then notes, or "Empty folder.") instead of a static "press enter to open this
  folder" hint — same spirit as selecting a note already showing its content.
- Collapsed (out-of-focus) panels are now 1 column wide instead of 3 — just the border line,
  since that's already enough to show there's a collapsed panel there.
- Status bar footer redesigned: no background fill, no "NORMAL" mode label (only INSERT/EDIT/
  VISUAL are shown), no theme name. Shows contextual metadata instead (character count of the note
  being read, or note count while browsing notebooks), the current git branch with a dirty/needs-
  pull indicator, and groups `? help` with the version on the right.
- Notebook sync is smarter and per-notebook: commit messages are now auto-built from the diff
  (e.g. "shiki: 2 updated, 1 added") instead of a fixed generic message, so nothing needs to be
  typed by hand. `auto_push`/`auto_sync`/`auto_sync_every` can be overridden per notebook under
  `[notebooks.<name>]`, falling back to the global `[git]` defaults.
- Footer git status now shows actual counts instead of a bare marker: `+N` uncommitted files,
  `↑N` commits not yet pushed, `↓N` commits not yet pulled in — all three at once if applicable
  (e.g. a diverged branch), instead of just one dirty/clean indicator.
- The note-preview title no longer shows a `[j/k scroll]` hint (redundant once scrolling — and
  now `PageUp`/`PageDown`/`Home`/`End` — is the obvious way to move around); shows the note's date
  in a muted tone instead.

### Fixed

- Every theme's `selection` color was defined but never actually rendered anywhere — every list
  (notebooks, notes, tree, logs, global search, theme picker, which-key) only bold-colored the
  selected row's text, with no highlighted background band, making every theme look flatter/less
  faithful than it should. Selection now gets a real background highlight in each theme's own
  `selection` color.
- Notebook-level git shortcuts (`s` sync, `u` push, `p`/`P` pull, `R` set remote) previously only
  worked while the NOTEBOOKS panel had focus — pressing `u` while reading a note in PREVIEW did
  nothing at all, with no error or explanation. They now work from any panel, since they act on the
  selected notebook, not the focused panel.
- `push` failed with "src refspec 'refs/heads/main' does not match any existing object" on any
  notebook whose real branch isn't the globally configured one (e.g. `master`, via `pull`'s
  branch-fallback) — it now pushes whatever branch `HEAD` actually points at instead of a fixed
  configured name.
- `push` reported success even when a rejection only surfaced through the remote's per-ref status
  (e.g. a rejecting server-side hook) rather than as an outright transport error — now verified via
  `push_update_reference` and turned into a real, reported error.
- `u` used to push only, without committing first — repeatedly pressing it on a notebook with
  uncommitted notes reported "pushed" every time while the dirty count never moved, since nothing
  had actually been committed. `u` now commits (same as `s`) and always pushes; every step (commit
  outcome, then push outcome including confirmation) is reported explicitly instead of a terse
  "pushed".
- `PageUp`/`PageDown`/`Home`/`End` didn't work anywhere in the app — not while reading a note in
  PREVIEW, and not in the which-key popup, logs, global search, or tree view. They now work
  everywhere: a bigger jump (10 at a time) or first/last, using the same list or scroll each modal
  already navigates with `j`/`k`.
- Which-key (`?`) had no scrolling at all — content that didn't fit the small centered popup was
  silently clipped with no way to see the rest, and any keypress just closed it (so it couldn't be
  typed into either).

## [0.1.0] - 2026-07-22

Initial release.

### Added

- Three-pane TUI (Notebooks / Notes / Preview), Yazi-inspired: collapsing Miller-columns layout,
  modal navigation (`hjkl`/arrows, `tab`, leader key), no exterior padding.
- Notes are plain Markdown with YAML frontmatter; notebooks are directories, each its own git
  repo, nestable in folders to any depth (like `nb`).
- Frontmatter is optional on read: a `.md` file with no (or malformed) frontmatter — from `nb`, an
  imported repo, or a manual edit — still shows up, with title/date synthesized instead of being
  skipped.
- Config-driven keybindings, scoped by focus (`[keybindings.global/notebooks/notes/preview]` in
  `config.toml`), with a leader key for actions that aren't tied to one panel.
- Six built-in themes (catppuccin, tokyo-night, gruvbox, nord, solarized, and a terminal-native
  default that inherits the terminal's own colors) with a live-preview picker modal (leader+`c`).
- Nerd Font icon set throughout the UI.
- Per-notebook git integration: sync (commit + optional push), pull (fast-forward only, safe
  against local commits), pull-all, and set-remote — plus a fast path where pasting a git URL as
  the new-notebook name clones and imports it in one step.
- Robust git authentication (SSH agent or the system's own credential store, so it reuses
  whatever `git`/`gh` already have cached) and automatic fallback to the remote's actual default
  branch when it isn't the one configured.
- Global fuzzy search across every notebook (leader+`g`) and in-notebook fuzzy jump (`/`).
- Tags panel, daily notes, moving a note between notebooks, cycling sort order.
- Notebook tree view (`T`): every folder and note in a notebook, fully expanded in one overview,
  with jump-to-note.
- Logs modal (leader+`l`): a scrollback of every status-bar message (so errors aren't lost the
  instant the next one overwrites the status bar), with a clipboard-copy shortcut.
- Inline editor (inside the TUI) and external-editor integration (`$EDITOR`, or the OS-detected
  favorite editor).
- CLI commands alongside the TUI: `new`, `list`, `edit`, `show`, `search`, `daily`, `sync`,
  `config`, `notebook`, `theme`.
- App version shown in the status bar footer.

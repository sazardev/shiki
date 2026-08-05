# shiki (私記 / しき)

> **Personal notes, private log.**
> TUI note-taking app in Rust — fast, configurable, with Git sync,
> inline and external editor, themes, and Yazi-style navigation.

---

## Motivation

I couldn't find any terminal note-taking tool that covered **everything** I wanted:

- Modern, responsive TUI
- Notebooks as folders (`nb`-like)
- Integrated Git sync (no external scripts)
- Inline **and** external editor (nvim) — usable interchangeably
- Markdown with frontmatter, tags, wikilinks
- Popular themes (Catppuccin, Tokyo Night, Gruvbox, Nord, Solarized…)
- Customizable keybindings (vi-mode)
- Fast fuzzy search
- Daily notes with templates
- CLI + TUI: quick commands without opening the interface
- The whole experience configurable via TOML
- Built in **Rust** — fast, reliable, a single binary

So **shiki** (私記) was born: "personal notes" or "private log" in Japanese.
A single binary that covers all of this, from the start, no phases.

---

## Tech stack

| Category | Crate |
|---|---|
| **TUI framework** | `ratatui` 0.30 + `crossterm` |
| **Async runtime** | `tokio` |
| **Git bindings** | `git2` |
| **Markdown parsing** | `comrak` |
| **Syntax highlighting** | `syntect` (note preview) |
| **Inline editor** | `tui-textarea` |
| **Fuzzy search** | `nucleo` (same as Helix) |
| **Config / themes** | `serde` + `toml` |
| **CLI parsing** | `clap` v4 |
| **Dates** | `chrono` |
| **File watcher** | `notify` (detect external changes) |
| **Frontmatter** | `serde_yaml` |
| **Logging** | `tracing` + `tracing-subscriber` |
| **Wikilinks** | regex + `pulldown-cmark` |

---

## Architecture

Workspace with 4 crates:

```
shiki/
├── Cargo.toml                     # workspace root
├── IDEA.md                        # this document
│
├── shiki-core/                    # pure logic (no TUI)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── notebook.rs            # Notebook struct, notebook CRUD
│       ├── note.rs                # Note struct, frontmatter, body
│       ├── search.rs              # fuzzy search engine (nucleo)
│       ├── git.rs                 # git2: init, commit, push, pull, status
│       ├── templates.rs           # template system
│       ├── daily.rs               # daily notes: create by date
│       ├── tags.rs                # tag indexing
│       └── wikilinks.rs           # parse [[links]] and resolve them
│
├── shiki-config/                  # TOML config parsing + themes
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── config.rs              # Config struct, load/save
│       ├── theme.rs               # Theme struct, palettes
│       └── themes/
│           ├── mod.rs
│           ├── catppuccin.rs      # mocha, latte, frappe, macchiato
│           ├── tokyo_night.rs     # storm, night, moon
│           ├── gruvbox.rs         # dark, light
│           ├── nord.rs
│           └── solarized.rs       # dark, light
│
├── shiki-tui/                     # the interface
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── app.rs                 # App state, event loop
│       ├── layout.rs              # panel layout
│       ├── panel_notebooks.rs     # left panel
│       ├── panel_notes.rs         # center panel
│       ├── panel_preview.rs       # right panel (rendered markdown)
│       ├── panel_tags.rs          # tag filter
│       ├── editor.rs              # inline editor (tui-textarea)
│       ├── status_bar.rs          # bottom status bar
│       ├── command.rs             # command palette / global fuzzy finder
│       ├── which.rs               # keybindings popup (like Yazi)
│       ├── confirm.rs             # confirmation dialog
│       ├── input.rs               # simple text input
│       ├── keybindings.rs         # configurable key map
│       └── render.rs              # render helpers
│
└── shiki-cli/                     # binary entrypoint
    ├── Cargo.toml
    └── src/
        ├── main.rs                # entrypoint: clap CLI + TUI launch
        ├── commands/
        │   ├── mod.rs
        │   ├── new.rs             # shiki new <title>
        │   ├── list.rs            # shiki list [notebook]
        │   ├── edit.rs            # shiki edit <note>
        │   ├── show.rs            # shiki show <note>
        │   ├── search.rs          # shiki search <query>
        │   ├── daily.rs           # shiki daily
        │   ├── sync.rs            # shiki sync [notebook]
        │   ├── config.rs          # shiki config (show/edit path)
        │   └── notebook.rs        # shiki notebook (create/list/rename)
        └── tui.rs                 # launch TUI from CLI
```

---

## TUI behavior

### Layout (3 panels, Yazi-inspired)

```
┌──────────────┬───────────────────────────────────────┬─────────────────────────────┐
│  NOTEBOOKS   │  NOTES                                │  PREVIEW                    │
│              │                                       │                             │
│  > personal  │  > 2026-07-22-daily                   │  # 2026-07-22 Daily         │
│    work      │    meeting-q3-planning                │                             │
│    projects  │    rust-ideas                         │  ## Tasks                   │
│              │    shiki-todo-list                    │  - [ ] Review PR            │
│  [1/3]       │    book-notes-atomic-habits           │  - [ ] Write docs           │
│              │                                       │                             │
│              │  20 notes  |  [/] to search           │                             │
├──────────────┴───────────────────────────────────────┴─────────────────────────────┤
│  NORMAL  │  personal  │  synced ✓  │  ? for help                                  │
└────────────────────────────────────────────────────────────────────────────────────┘
```

Panels behind the current focus collapse to a thin strip (Miller-columns style, like
Yazi): browsing NOTEBOOKS shows all three panels at their normal width; moving into
NOTES collapses NOTEBOOKS down; moving into PREVIEW collapses both NOTEBOOKS and NOTES
so the note has (almost) the full width to read comfortably.

This layout is responsive to the terminal's actual size, not just one fixed arrangement:

- **Wide** (≥70 columns): the 3-column layout above.
- **Narrow but tall, or square** (46–70 columns wide): the same panels, same
  focused/collapsed proportions, just stacked top-to-bottom instead of side-by-side — a
  portrait window or a many-way terminal split still gets a usable full-width panel
  instead of three unreadably thin columns.
- **Very small** (<46 columns or <14 rows): only the focused panel is shown, full screen —
  no collapsed siblings at all. `h`/`l`/`tab` still move between panels exactly as always;
  at this size that just means switching which one is on screen, one at a time.

### Modes (vi-like)

| Mode | Description |
|---|---|
| `NORMAL` | Navigation, single-key shortcuts |
| `INSERT` | Typing in search / input |
| `EDIT` | Inline editor active (tui-textarea) |
| `VISUAL` | Multi-note selection |

### Keybindings: segmented by focus

Keybindings aren't one flat list — they're scoped to *what's focused*, so the
same physical key can mean something different (but locally sensible) in
each panel. Navigation is hardcoded (not configurable, since it behaves the
same everywhere); everything else lives in its own editable `config.toml`
table: `[keybindings.global]` (needs the leader key first),
`[keybindings.notebooks]`, `[keybindings.notes]`, `[keybindings.preview]`.

#### Navigation (hardcoded, works everywhere)

| Key | Action |
|---|---|
| `j` / `k` / `↓` / `↑` | Navigate lists (down / up); scrolls the note while PREVIEW is focused |
| `PageDown` / `PageUp` | Jump 10 at a time — same lists/scroll as `j`/`k`, just a bigger step |
| `Home` / `End` | Jump to the first/last notebook or note, or the top/bottom of the note in PREVIEW |
| `l` / `→` / `enter` | Go one level deeper (Yazi-style): NOTEBOOKS → NOTES → PREVIEW |
| `h` / `←` | Go back one level (Yazi-style) |
| `tab` | Cycle focus between panels |
| `?` | Which-key — near-full-screen list of every binding, doubling as a command palette: type to filter (by key, action, or scope), `↑`/`↓`/`PageUp`/`PageDown`/`Home`/`End` move the selection, `enter` runs the highlighted action immediately, `Esc` just closes |
| `q` | Quit |
| `Esc` | Back to NORMAL / close popup / cancel leader |

`PageUp`/`PageDown`/`Home`/`End` also work inside every scrollable modal (which-key, logs, global
search, tree view) using the same list/selection they already navigate with `j`/`k`.

#### `[keybindings.global]` — press `leader` (default `space`), then:

| Key | Action |
|---|---|
| `c` | Pick a theme — modal, live preview while browsing, Enter confirms |
| `g` | Search all notes (title + body, every notebook) — modal, Enter/click to jump |
| `T` | Tags panel — `j`/`k` browse tags, `Enter`/`l` drills into the notes carrying one, `Enter`/`l` there jumps to it, `h`/`Esc` goes back a level |
| `l` | Logs — persistent scrollback of every status-bar message (survives restarts, `~/.config/shiki/shiki.log`), including errors that already scrolled past; `j`/`k` scroll, `y`/`c` copies the whole log to the clipboard (OSC 52), `x` clears all logs (confirmation required), `Esc`/`q` closes |
| `b` | Notebook drawer — left-side sidebar, every notebook's git status in color (dirty/ahead/behind); `j`/`k` or click a row to jump to it, `n`/click "New" to create a notebook, `i`/click "Import" to clone from a pasted URL, `Esc`/`b` again closes |
| `e` | Toggle `general.use_favorite_editor` on/off and persist it immediately — no need to hand-edit config.toml. The footer always shows which mode is active: the resolved editor name (e.g. `nvim`) when on, `native` (the built-in inline editor) when off |
| `p` | Scratchpad — open an in-memory editor; `Ctrl+S` saves its contents through the new-note title/template flow, while `Esc` discards it |
| `B` | Links — the same modal as PREVIEW's `L` (outgoing wikilinks / backlinks / unlinked mentions for the selected note), reachable from any panel without focusing PREVIEW first |
| `t` | Tasks — every `- [ ]` checkbox task across every notebook in one flat list, pending-only by default, sorted by urgency (overdue → due today → future → undated); every row carries its own muted location (`notebook/folders…/note title`) so where a task lives is always visible, even mid-scroll. `Enter`/`space` toggles the task directly in its source file (a normal note edit — flows through auto-sync like any other change) and updates the row in place so it can be immediately un-toggled; `l`/`o` jumps to the note; `a` also shows completed tasks. An optional `@due(YYYY-MM-DD)` tag anywhere in the task text renders the date next to it: overdue in the theme's error color, due today in warning, future in muted. Relative specs — `@due(tomorrow)`, `@due(+3d)`, `@due(+2w)`, `@due(fri)` — are pinned to the real date the moment the note is saved (inline or external editor), since they're relative to the day they were written. Also scriptable as `shiki tasks` (see CLI commands) |
| `P` | Publish the selected notebook to a themed PDF via `pretty-pdf` (external binary, auto-fetched on first use — see CLI commands), written to `{data_dir}/exports/{notebook}.pdf`, then opened. Same rendering `shiki publish` uses; the theme comes from `export.pdf_theme`, cyclable in Settings → EXPORT |
| `U` | Check for updates — modal; checks GitHub Releases in the background (never blocks the UI), shows "update available" if there's a newer version, and `Enter` downloads, verifies (against GitHub's own per-asset checksum), installs, and automatically relaunches into it |
| `u` | Undo the last delete — restores the most recently deleted note/folder (or whole batch, from a Visual-mode delete) from the trash (`~/.config/shiki/trash/`) back to exactly where it came from. A single level of undo, not a full history: only the *most recent* delete is restorable this way; an older one is still on disk in the trash, just no longer reachable from here. With nothing to undo, reports that instead of doing anything |
| `s` | Settings — near-full-screen, paged by tab (`←`/`→` switches GENERAL/THEME/GIT/EDITOR/EXPORT/NOTEBOOKS/SNIPPETS, `j`/`k` moves within one). Doesn't repeat the keybindings tables — `?` (which-key) already covers those live. Every tab is editable with `Enter`: GENERAL/GIT booleans (`use_favorite_editor`, `auto_commit`/`auto_push`/`sign_commits`/`auto_sync`) toggle in place and save immediately; every other GENERAL/GIT field opens a prompt prefilled with its current value; THEME's `name` opens the theme picker (live preview, same as leader+`c`) and `overrides` stays informational; EDITOR is twelve plain boolean toggles for the native note editor's UX (see `[editor]` below); EXPORT is one field, `pdf_theme`, cycling through `pretty-pdf`'s 17 built-in themes (the default `shiki publish`/leader+`P` fall back to); NOTEBOOKS lists every notebook's actual git remote (redacted) and drills into one to edit its remote plus its `auto_push`/`auto_sync`/`auto_sync_every` overrides (booleans cycle inherit → true → false → inherit); SNIPPETS supports `a` (new snippet) and `d` (delete, with confirmation), and drilling into one edits its `label` and its full multi-line `body` through the same inline editor a note's own body uses. `i`/`E` still jump straight to editing `config.toml` itself for anything not covered above (inline or externally, same convention as editing a note); on save, the config is re-read, re-applied, and takes effect immediately (no restart) — an invalid edit is reported and neither written nor applied, keeping the previous config running. `h`/`Esc`/`Backspace` backs out of a drilled-into notebook/snippet a level; `Esc`/`q` at the top level closes |
| `z` | Zen mode — forces the full-screen single-panel layout (the same one a very small terminal already falls into) regardless of actual terminal size, hiding NOTEBOOKS/NOTES so only the focused panel shows. A true toggle, same `leader z` both enters and exits it; purely a view state, not persisted to `config.toml` |

#### `[keybindings.notebooks]` — active while NOTEBOOKS is focused

| Key | Action |
|---|---|
| `a` | New notebook. A git URL (`https://`, `git@host:...`, `ssh://`, `git://`) derives the notebook name from the repo instead, creates it, sets its remote, and pulls immediately — importing an existing repo is just `a` + paste URL + Enter. A filesystem path (`/abs/path`, `~/docs`, `./relative`) adopts that existing directory as a notebook instead of creating an empty one — name derived from the last path segment; asks to `git init` first if it isn't already a repo |
| `r` | Rename notebook |
| `d` | Delete notebook (with confirmation) |
| `s` | Git sync — commit (message auto-built from the diff, naming files directly for a small change, e.g. "shiki: added (First note.md)"), + push if the resolved policy's `auto_push` is on |
| `u` | Sync and always push, regardless of `auto_push`/`auto_sync` — the explicit "do it now" override |
| `p` | Git pull (fetch + fast-forward merge from the configured remote) |
| `P` | Git pull for every notebook that has a remote configured |
| `R` | Set the notebook's git remote (URL or local path) |

Beyond manual `s`, a notebook can sync **itself** in the background: `[git] auto_sync = true` (off by
default) syncs automatically every `auto_sync_every` note changes (new/edited/renamed/deleted/moved),
not just on manual `s`. Since not every notebook should behave the same way — a private work repo
you always want pushed vs. a scratch notebook with no remote at all — `auto_push`, `auto_sync`, and
`auto_sync_every` can all be overridden per notebook under `[notebooks.<name>]` in `config.toml`,
falling back to the global `[git]` values for anything left unset. A failed push (no internet, auth,
etc.) never blocks or loses anything — the commit already happened locally either way, and the next
sync attempt (manual or automatic) just tries the push again.

#### `[keybindings.notes]` — active while NOTES is focused

| Key | Action |
|---|---|
| `a` | New note (empty title stamps today's date). After the title, a template picker opens — every `.md` file in `~/.config/shiki/templates/` plus a "blank" option; `j`/`k` browse, `Enter` picks one and jumps straight to editing (`{{title}}`/`{{date}}` already substituted), `Esc`/`q` cancels the note entirely. Typing `@` anywhere in the title prompt (with or without a title before it) opens a quick dropdown instead — `today`/`yesterday`/`tomorrow` (a computed date, no template) plus every available template, fuzzy-filtered as you keep typing; `Enter` creates the note and jumps straight to editing, skipping the title→Enter→pick-a-template two-step entirely |
| `f` | New folder — empty name cancels rather than creating something unnamed. Created at the current breadcrumb depth, so it can be nested arbitrarily by descending first |
| `r` | Rename note |
| `d` | Delete the selected note *or* folder (with confirmation) — a folder deletes everything inside it too. In `v` select mode, deletes every selected item at once. Moved to the trash rather than permanently removed, so leader+`u` can undo it |
| `i` | Edit inline (or the OS favorite editor if `general.use_favorite_editor`) |
| `E` | Edit externally ($EDITOR) |
| `/` | Fuzzy-jump to a note by title anywhere in the current notebook (any folder depth) |
| `t` | New/open today's daily note. On first creation each day, a "## Due today" section is appended after the template with every pending task due today or overdue, across every notebook — plain bullets with a `[[wikilink]]` back to each task's source note (deliberately not checkbox copies, which would double-count in the tasks view). Reopening later never re-injects or duplicates it |
| `m` | Move the selected note *or* folder — prompts for a `notebook/path/within/it` target, prefilled with the current one; edit the trailing segments to move within the same notebook (missing folders are created), or replace the first segment to move to a different (existing) notebook. In `v` select mode, moves every selected item at once |
| `o` | Cycle sort order (filename / title A-Z / date newest-first) |
| `T` | Tree view — every folder and note in the notebook, fully expanded, in one scrollable overview; `j`/`k` move, `enter`/`l` jumps straight to the selected note, `esc`/`q` closes |
| `D` | Toggle each note's date next to its title in the list (off by default) |
| `v` | Select mode (`Mode::Visual`) — anchors a multi-select range at the current item; `j`/`k` extend/shrink it, `v`/`Esc` cancels. `d`/`m` (above) act on the whole range instead of one item |
| `y` | Select-mode only: copies every selected note/folder to a prompted target (same `notebook/path` syntax as `m`), leaving the originals in place |

#### `[keybindings.preview]` — active while PREVIEW is focused

| Key | Action |
|---|---|
| `i` | Edit inline (or the OS favorite editor if `general.use_favorite_editor`) |
| `E` | Edit externally ($EDITOR) |
| `H` | Note history — every commit that changed this specific note, newest first, real git history (not a separate versioning system). `j`/`k`/`PageUp`/`PageDown`/`Home`/`End` move, `Enter` views a revision's full content (frontmatter included, since that's what's actually in the commit), `r` reverts to the highlighted (or currently-viewed) revision — behind a confirmation, since it overwrites the current content. The revert itself doesn't commit; it shows up as a normal pending change, picked up by `s`/`u`/`auto_sync` like any other edit. The footer shows the count while reading a note (`{n} changes`) |
| `L` | Links — the selected note's outgoing `[[wikilinks]]` (resolved against every note in the notebook, any folder depth), every other note that links back to it, and notes that *mention* this note's title in plain text without linking to it ("Outgoing"/"Backlinks"/"Mentions (unlinked)" sections; a section with nothing in it is omitted). `j`/`k`/`PageUp`/`PageDown`/`Home`/`End` move, `Enter` jumps to the selected note (an unresolved outgoing link reports that instead of jumping), `c` on a mention row *repairs* the missed link — it wraps that note's plain-text mention into a real `[[wikilink]]` (preserving its casing) and the row visibly migrates to Backlinks — and `Esc`/`q` closes. Also reachable globally via leader+`B` |
| `o` | Outline — every `#`..`######` heading in the selected note, indented by level. `j`/`k`/`PageUp`/`PageDown`/`Home`/`End` move, `Enter` scrolls PREVIEW to that heading, `Esc`/`q` closes. Also reachable as `Ctrl+O` from inside `Mode::Edit` itself — there, `Enter` moves the editor's own cursor to the heading instead of scrolling PREVIEW, and the headings come from the live, possibly-unsaved buffer rather than the note's last-saved body |

Mouse: a plain click over a note's rendered body jumps straight into the inline editor with the
cursor on the clicked line — a mouse-only alternative to `i`/vim motions. Click-and-drag instead
selects the dragged rows (highlighted with the theme's `selection` color) and copies them to the
clipboard the moment the button is released — no extra keypress needed, same OSC 52 mechanism as
the logs modal's `y`/`c`. Both are controlled by `general.mouse_drag_selection` (on by default;
toggle it in Settings' GENERAL tab or in `config.toml`).

#### Inside the inline editor (`i`)

Long lines wrap to the panel's width, the same as PREVIEW — they never scroll off the edge of the
screen. A completely empty note shows a dim placeholder hint ("Type `/` for quick blocks…"),
gone the instant you type anything.

Typing `/` as the very first character of a line (nothing to its left — a `/` anywhere else, e.g.
mid-sentence or in a URL/fraction, is just a literal slash) opens a small searchable menu right
under the cursor: keep typing to filter by name, `↑`/`↓` to move, `Enter` to insert the highlighted
block, `Esc` to dismiss the menu without leaving edit mode (a second `Esc` then saves and exits, as
usual). Built-in blocks: `h1`/`h2`/`h3`, a code fence, a math (`$$…$$`) block, a table skeleton, a
checklist item, a quote, a divider, today's date, a `Tags:` line, a YAML frontmatter skeleton, a
bullet/numbered list item, a link, an image, a note/warning callout, and a collapsible
(`<details>`) section.

Every command is customizable from `config.toml` under `[snippets.<trigger>]` — see the
Configuration section below. A custom entry with the same trigger as a built-in (case-insensitive)
replaces it instead of adding a duplicate, so nothing here is off-limits to redefine.

The rest of the editor's mouse/keyboard UX is opt-in via `[editor]` (Settings' EDITOR tab, or
`config.toml` directly — see the Configuration section):

- `mouse_selection` (on by default): click to position the cursor, click-and-drag to select,
  double-click to select a word, triple-click to select the whole line.
- `line_numbers` (off by default): a line-number gutter.
- `find_replace` (on by default): `Ctrl+F` opens a find/replace bar — typing live-jumps to the
  nearest match, `Enter`/`Shift+Enter` step to the next/previous match, `Ctrl+Enter` replaces the
  current match and advances, `Ctrl+Alt+Enter` replaces every occurrence, `Tab` switches between
  the find and replace fields, `Esc` closes the bar (a second `Esc` then saves and exits, as usual).
- `os_clipboard` (off by default): `Ctrl+C`/`Ctrl+X`/`Ctrl+V` use the real OS clipboard, falling
  back automatically to the existing OSC 52 mechanism when there's no display server to reach
  (e.g. a headless SSH session).
- `select_all_ctrl_a` (off by default): `Ctrl+A` selects the whole buffer instead of moving to
  the start of the line.
- `multi_cursor` (off by default): full multi-cursor editing. `Alt`+click adds a cursor at the
  clicked position; `Ctrl+Alt+↑`/`Ctrl+Alt+↓` (VS Code's own "Add Cursor Above/Below") adds one
  at the same column one row up/down from whichever cursor already sits furthest in that
  direction — a keyboard-only alternative to Alt+click, extending the same contiguous block on
  each repeated press. `Ctrl+D` selects the word under the cursor, and every further press adds
  the next occurrence as its own cursor (wraps around the buffer, reports "no more occurrences"
  once everything's already selected). Typing, `Backspace`/`Delete`, `Enter`, and navigation all
  replay across every cursor at once — typing over a Ctrl+D-selected occurrence replaces it, same
  as a single cursor would. `Ctrl+U`/`Ctrl+R` undo/redo a multi-cursor edit as one action, however
  many cursors it touched. `Esc` first collapses back to a single cursor (a second `Esc` then
  saves and exits, as usual). Secondary cursors render as a solid accent-colored block (bold,
  distinct from the primary's plain reverse-video) — a terminal can only blink one real caret, so
  this is the compensating visual signal rather than an attempt to actually blink more than one.
- `auto_list_continue` (on by default): `Enter` on a `- item`/`- [ ] task`/`1. item` line carries
  the same prefix onto the next line (checkboxes always reset to `[ ]`, never copy `[x]`); `Enter`
  on an already-empty list item exits the list instead of continuing it, and `Backspace` right
  after an empty prefix removes it in one step instead of one character at a time.
- `format_shortcuts` (on by default): `Ctrl+B` wraps the current selection in `**bold**`;
  `Ctrl+Alt+I` does the same for `_italic_` (not `Ctrl+I` — that's indistinguishable from `Tab` at
  the terminal level in most emulators). With nothing selected, either inserts an empty pair with
  the cursor left in the middle.
- `auto_pair_brackets` (on by default): typing `(`, `` ` ``, or `"` wraps the current selection in
  the matching pair, or inserts an empty pair with the cursor in the middle. Deliberately excludes
  `[` so it can't interfere with `[[wikilink]]` autocomplete below, which depends on the user
  typing two real `[` characters in a row.
- `paste_url_as_link` (on by default): pasting a bare URL over an active selection wraps it as
  `[selected text](url)` instead of replacing the selection with the raw URL.
- `snippet_expand_tab` (on by default): `Tab`, when the text immediately before the cursor matches
  a configured snippet trigger, replaces that trigger text with the snippet's body instead of
  inserting a literal tab.
- `typewriter_scroll` (off by default): keeps the cursor's line vertically centered in the editor
  viewport while typing, instead of only scrolling once the cursor reaches the edge.

`[[wikilink]]` autocomplete is always on too, not gated by `[editor]`: typing `[[` opens an
Obsidian-style fuzzy note picker (same fuzzy matching as `/` and global search), showing each
candidate's folder breadcrumb so notes with duplicate titles in different folders stay
distinguishable; picking one inserts `[[Title]]`. In PREVIEW, `Ctrl`+click on a rendered
`[[wikilink]]` jumps straight to the note it resolves to — a plain click still enters edit mode
everywhere, including on top of a wikilink.

Navigation inside the editor is always on, not gated by `[editor]`: `PageUp`/`PageDown` move the
cursor a page at a time, `Home`/`End` go to the start/end of the current line, `Ctrl+Home`/
`Ctrl+End` jump to the very start/end of the note, and the mouse wheel scrolls too. `PageUp`/
`PageDown`/mouse-wheel scrolling move the cursor itself (there's no independent scroll offset —
the editor's word-wrap support means it bypasses `tui-textarea`'s own rendering, and with it,
`tui-textarea`'s own viewport-based `PageUp`/`PageDown`, which otherwise does nothing here).

---

## CLI commands

```
shiki                     # launch TUI
shiki new <title>         # create note + open $EDITOR
shiki daily               # create/open today's daily note
shiki list                # list notes in the default notebook
shiki list -n work        # list notes in "work"
shiki show <note>         # show rendered content (ANSI)
shiki edit <note>         # edit with $EDITOR
shiki search <query>      # search and show results
shiki new "title" --body "text"     # create non-interactively, no $EDITOR spawned
shiki new "title" --stdin --tags work,idea  # body piped in, tags attached, still no $EDITOR
shiki list --json         # list/search/show all take --json for scripting (list/search: array, show: object)
shiki tasks               # every pending checkbox task across notebooks, urgency-sorted
shiki tasks --overdue --count  # just the number — made for waybar/polybar/tmux status modules
shiki tasks --today --json     # machine-readable, with due/overdue/location per task
shiki graph               # [[wikilink]] connection graph, force-directed, drawn in the terminal
shiki graph -n work --json     # nodes/edges/orphans as JSON, for graphviz/d3/gephi
shiki export -n work --out bundle.html            # every note in "work" as one self-contained HTML file
shiki export -n work --out bundle.md --format md  # or a plain concatenated Markdown bundle
shiki publish -n work                     # render "work" to a themed PDF via pretty-pdf (auto-fetched, see below)
shiki publish -n work --out report.pdf --theme dark   # custom path/theme; theme defaults to export.pdf_theme
shiki sync                # git commit+push default notebook
shiki sync -n work        # git sync in "work"
shiki config              # show config path
shiki notebook create <name>
shiki notebook list --json
shiki notebook rename <old> <new>
shiki notebook delete <name> --yes  # permanently deletes the notebook and every note in it
shiki theme list          # list built-in themes, marking the active one
shiki theme set <name>    # switch theme (persisted to config.toml)
shiki theme create [--from <name>]  # scaffold all 19 color overrides from a real palette
shiki doctor              # environment check: config, data dir, git, editor, terminal, keybindings, snippets
```

`shiki publish` (leader+`P` in the TUI) renders a notebook to a themed PDF through
[`go-pretty-pdf`](https://github.com/sazardev/go-pretty-pdf), a separate Go binary shelled out to
as an external process (never linked into shiki itself) — the first run fetches and caches it
automatically, so using this feature never requires a manual install step. `--theme` picks one of
`pretty-pdf`'s 17 built-in themes; the TUI's Settings → EXPORT tab cycles the same set for
`export.pdf_theme`, the config-level default `shiki publish` falls back to when `--theme` isn't
given.

`shiki doctor` also checks: unrecognized keys anywhere in `config.toml` (a generic diff against
`Config::default()`'s own shape, not a hand-maintained list); `general.default_notebook` actually
matching an existing notebook; `data_dir` being a real directory, not just existing; two notebooks
resolving to the same path on disk; `git.remote_template` containing its `{notebook}` placeholder;
and `git.sign_commits` having an actual signing key configured (`git config user.signingkey`).

---

## Filesystem layout

Respects `$XDG_CONFIG_HOME`/`$XDG_DATA_HOME` if you have them set (most people don't, so the
defaults below are what you'll actually see) — resolved via
`directories::ProjectDirs::from("", "", "shiki")`.

### Data (`~/.local/share/shiki/`)

```
~/.local/share/shiki/
├── personal/              # one notebook = one directory
│   ├── .git/              # independent git repo per notebook
│   ├── 2026-07-22-daily.md
│   ├── meeting-q3-planning.md
│   ├── rust-ideas.md
│   └── projects/          # notebooks nest folders arbitrarily deep, like `nb` —
│       └── website/       # the NOTES panel browses them one level at a time
│           └── todo.md    # (l/→/enter opens a folder, h/← goes back up)
├── work/
│   ├── .git/
│   ├── sprint-review.md
│   └── architecture.md
└── projects/
    └── ...
```

Frontmatter is optional on read: a plain `.md` file with no `---` block (from
`nb`, an existing repo, or anywhere else) still shows up as a note — its
title comes from the first `# heading` or the filename, its date from the
file's mtime. It only gains real frontmatter once you touch it through shiki
(rename/edit); until then it's left exactly as it was on disk.

### Config (`~/.config/shiki/`)

```
~/.config/shiki/
├── config.toml            # general configuration
├── keybindings.toml       # custom shortcuts (optional)
├── theme.toml             # custom theme (optional)
├── shiki.log              # persistent status/log history (leader+l to view, x to clear)
├── trash/                 # deleted notes/folders, restorable with leader+u (see below)
│   └── <notebook>/
└── templates/             # templates
    ├── default.md
    ├── daily.md
    └── meeting.md
```

Deleting a note or folder (`d` in NOTES scope) moves it here instead of removing it outright, so
leader+`u` can put it right back — see the `[keybindings.global]` table above. Same collision
reasoning as `shiki.log`: this lives in the config dir, not the data dir, since the data dir's top
level is the set of notebooks themselves (user-named directories), so a fixed name placed there
could collide with one.

### Note format (Markdown + YAML frontmatter)

```markdown
---
title: My note
date: 2026-07-22
tags: [rust, tui, ideas]
notebook: personal
links: [[another-note]], [[third-link]]
template: default
---

# My note

Content in **markdown**.

- List of items
- Code: `let x = 1;`

[[wikilink]] to another note — navigable from the TUI.
```

Notebooks also tolerate `.txt` and `.mdx` files alongside `.md` when listing/reading notes — a
notebook pointed at an existing Obsidian vault or similar commonly has both. New notes are always
created as `.md`; renaming a `.txt`/`.mdx` note preserves its original extension.

---

## Included themes (from the start)

- **Catppuccin** — Mocha, Macchiato, Frappé, Latte
- **Tokyo Night** — Storm, Night, Moon
- **Gruvbox** — Dark, Light
- **Solarized** — Dark, Light
- **Nord**
- **Dracula**
- **One Dark**
- **Monokai**
- **Default** — inherits the terminal's own colors (fallback; `bg`/`fg`/`border` are
  `"reset"`, accents use the terminal's native ANSI colors) instead of imposing a
  fixed palette

Each theme defines ~20 configurable color slots:
`bg`, `fg`, `accent`, `selection`, `border`, `statusbar`,
`highlight`, `error`, `warning`, `success`, `inactive`,
`scrollbar`, `tab_active`, `tab_inactive`, etc. Slots accept `#rrggbb` hex,
the terminal's native ANSI names (`red`, `blue`, `cyan`, `darkgray`, …), or
`"reset"` to inherit the terminal's own default for that slot.

---

## Configuration (`config.toml`)

```toml
[general]
default_notebook = "personal"
editor = "nvim"
daily_template = "daily"
# When true, `i` opens the OS's detected favorite editor (env $VISUAL/$EDITOR,
# then the desktop's default text/plain handler) instead of the inline editor.
use_favorite_editor = false
# When true, click-and-drag over a note's body in PREVIEW selects text and
# copies it to the clipboard (OSC 52, same mechanism as the logs modal's
# `y`/`c`) as soon as the mouse button is released.
mouse_drag_selection = true
# Optional override for the notebooks root directory. Set this to an
# absolute path to an existing folder of markdown notes (e.g. an Obsidian
# vault) to use it as the notebooks root instead of the platform default
# (~/.local/share/shiki/). Notebooks are subdirectories of this path.
# data_dir = "/Users/me/my-obsidian-vault"
# When true, text-input prompts that have one (e.g. new notebook) show a
# small hint line explaining non-obvious input, like pasting a git URL.
show_hints = true
# When true, quitting the TUI remembers exactly where you were — which
# notebook, which folder inside it, which note/folder was selected, and
# which panel (NOTEBOOKS/NOTES/PREVIEW) had focus — and the next launch
# restores it verbatim instead of always starting at the first notebook's
# root. A renamed/deleted notebook or moved note is silently ignored rather
# than erroring; the app just falls back to the default startup state.
remember_last_session = true

[keybindings]
leader = "space"
quit = "q"

[keybindings.global]
theme_picker = "c"
global_search = "g"
tags_panel = "T"
logs = "l"
toggle_favorite_editor = "e"
check_update = "U"
drawer = "b"
undo_delete = "u"
settings = "s"
scratchpad = "p"
# Same links modal as [keybindings.preview]'s own binding, from any panel.
links = "B"
# Global tasks view — every checkbox task in every notebook.
tasks_panel = "t"
# Full-screen single-panel layout, hiding NOTEBOOKS/NOTES — a true toggle.
zen_mode = "z"

[keybindings.notebooks]
new = "a"
rename = "r"
delete = "d"
sync = "s"
pull = "p"
pull_all = "P"
set_remote = "R"
push = "u"

[keybindings.notes]
new = "a"
new_folder = "f"
rename = "r"
delete = "d"
edit_inline = "i"
edit_external = "E"
search = "/"
daily_note = "t"
move_to_notebook = "m"
sort = "o"
tree_view = "T"
toggle_dates = "D"
visual = "v"
copy_entries = "y"

[keybindings.preview]
edit_inline = "i"
edit_external = "E"
history = "H"
links = "L"
outline = "o"

[theme]
name = "gruvbox-dark"
# Every one of a theme's 19 color slots can be overridden individually —
# accent, bg, fg, selection, border, statusbar, highlight, error, warning,
# success, inactive, scrollbar, tab_active, tab_inactive, panel_title,
# cursor, link, tag, muted. Override as many or as few as you want; anything
# left unset falls back to `name`'s own value for that slot. A couple of
# examples:
# accent = "blue"
# bg = "#1e1e2e"
# fg = "#cdd6f4"
#
# `shiki theme create [--from <theme>]` scaffolds *all 19* here at once,
# copied from a real palette (defaulting to whichever theme is active) —
# a starting point to edit slot-by-slot instead of hand-typing hex codes
# from scratch with no example to copy from.

[git]
auto_commit = true
auto_push = false
commit_prefix = "shiki: "
remote = "origin"
branch = "main"
sign_commits = false
auto_sync = false
auto_sync_every = 5
# Auto-configures a notebook's remote on creation (plain name, not a pasted
# URL) — "{notebook}" is replaced with the new notebook's name. The remote
# still has to already exist on that server; this doesn't create one via
# any hosting provider's API. Empty (the default) means don't auto-configure
# anything.
remote_template = ""
# remote_template = "git@git.example.com:notes/{notebook}.git"

# Native note editor (Mode::Edit) UX — every key here is independently
# toggleable from the EDITOR tab in Settings (leader+`s`); nothing changes
# until you opt in, except the two purely-additive ones below which default
# to true.
[editor]
mouse_selection = true   # click to position, drag/double/triple-click to select
find_replace = true      # Ctrl+F opens a find/replace bar in the editor
os_clipboard = false     # Ctrl+C/X/V use the real OS clipboard (falls back to OSC 52)
select_all_ctrl_a = false  # Ctrl+A selects everything instead of "start of line"
line_numbers = false     # shows a line-number gutter
multi_cursor = false     # Alt+Click adds a cursor, Ctrl+D adds the next occurrence
auto_list_continue = true  # Enter continues a list/checkbox line; empty item exits it
format_shortcuts = true    # Ctrl+B / Ctrl+Alt+I wrap the selection in bold/italic
auto_pair_brackets = true  # typing ( ` " wraps the selection or inserts an empty pair
paste_url_as_link = true   # pasting a URL over a selection wraps it as a markdown link
snippet_expand_tab = true  # Tab expands a matching snippet trigger
typewriter_scroll = false  # keeps the cursor's line vertically centered while typing

# PDF export (`shiki publish`, leader+`P`) — pdf_theme picks one of
# go-pretty-pdf's 17 built-in themes: default, minimal, modern, classic,
# corporate, dark, academic, editorial, sepia, terminal, blueprint, ivy,
# government, resume, legal, latex, gruvbox. Cyclable from the EXPORT tab
# in Settings.
[export]
pdf_theme = "default"

# Optional per-notebook overrides of [git] — anything left unset here falls
# back to the global values above.
#
# Each notebook can also have an independent `path`, pointing it at any
# existing directory on disk instead of the default location under the
# data directory. Useful for linking Obsidian vault subfolders or other
# existing markdown collections as notebooks without moving files.
[notebooks.alcateia]
path = "/Users/me/obsidian-vaults/alcateia"
# Standard git overrides work alongside path:
auto_sync = true

[notebooks.work]
auto_sync = true
auto_sync_every = 3
auto_push = true

# Custom entries for the inline editor's `/`-menu, keyed by trigger. Empty by
# default — the built-in commands (h1/h2/h3/code/math/table/check/quote/
# divider/date/tags/frontmatter/bullet/numbered/link/image/note/warning/
# details) aren't listed here at all, only your own additions/overrides.
# `label` falls back to the trigger when omitted; `body` supports
# {{title}}/{{date}} (substituted the same way note templates are) plus a
# {{cursor}} marker for where the cursor lands after insertion.
[snippets.callout]
label = "Info callout"
body = "> **Info:** {{cursor}}"

# Same trigger as a built-in (case-insensitive) replaces it instead of
# adding a duplicate — every command in the menu is customizable this way.
[snippets.h1]
body = "# [{{title}}] {{cursor}}"
```

---

## Design principles

1. **A single binary** — `shiki` does everything, CLI and TUI.
2. **Plain text** — notes are `.md` with frontmatter. Nothing proprietary.
3. **Git native** — each notebook is its own repo. Commit, push, pull from the app.
4. **No phases** — everything here is implemented in full. There's no "Phase 2".
5. **Fast** — Rust + async + ratatui. Sub-millisecond renders.
6. **Configurable** — keybindings, themes, editor, all in TOML.
7. **Yazi-inspired** — three panels, modal, which-key, async event loop.

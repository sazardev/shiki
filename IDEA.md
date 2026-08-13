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
| **Git bindings** | `git2` |
| **Markdown parsing** | `comrak` |
| **Syntax highlighting** | `syntect` (note preview) |
| **Inline editor** | `ratatui-textarea` 0.9 |
| **Fuzzy search** | `nucleo-matcher` (same as Helix) |
| **Config / themes** | `serde` + `toml` |
| **CLI parsing** | `clap` v4 |
| **Dates** | `chrono` |
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
│       ├── search.rs              # fuzzy search engine (nucleo-matcher)
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
│       ├── editor.rs              # inline editor (ratatui-textarea)
│       ├── status_bar.rs          # bottom status bar
│       ├── which.rs               # keybindings popup + command palette (like Yazi)
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
| `EDIT` | Inline editor active (ratatui-textarea) |
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
| `?` | Which-key — near-full-screen list of every binding, doubling as a command palette: type to filter (by key, action, or scope) — once the query is non-empty, it also fuzzy-matches notes across every notebook (title/body/notebook name) and lists up to 8 under a "notes" section — `↑`/`↓`/`PageUp`/`PageDown`/`Home`/`End` move the selection, `enter` runs the highlighted action or jumps straight to the highlighted note, `Esc` just closes |
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
| `t` | Tasks — every `- [ ]` checkbox task across every notebook in one flat list, pending-only by default (persisted default: `general.tasks_show_done_default`), sorted by urgency (overdue → due today → future → undated); every row carries its own muted location (`notebook/folders…/note title`) so where a task lives is always visible, even mid-scroll. `Enter`/`space` toggles the task directly in its source file (a normal note edit — flows through auto-sync like any other change) and updates the row in place so it can be immediately un-toggled; `l`/`o` jumps to the note; `a` also shows completed tasks. An optional `@due(YYYY-MM-DD)` tag anywhere in the task text renders the date next to it: overdue in the theme's error color, due today in warning, future in muted. Relative specs — `@due(tomorrow)`, `@due(+3d)`, `@due(+2w)`, `@due(fri)` — are pinned to the real date the moment the note is saved (inline or external editor), since they're relative to the day they were written. An optional `@every(<spec>)` tag (`day`/`daily`, `week`/`weekly`, `month`/`monthly`, `year`/`yearly`, or `Nd`/`Nw`/`Nm`) marks a task as recurring — completing it (not un-completing) inserts its next occurrence right below it, unchecked, with `@due` advanced by that interval from the existing due date (or from today if it had none); a repeat icon plus the raw spec shows next to the task text. Also scriptable as `shiki tasks` (see CLI commands) |
| `q` | Query — Dataview-style filter/sort over frontmatter across every notebook, live-editable: type the DSL at the top (e.g. `where status = pending sort due asc`), matching notes render as a table below; `↑`/`↓`/`PageUp`/`PageDown`/`Home`/`End` move the selection (deliberately no `j`/`k` — those stay typeable into the query), `Enter` jumps to the note, `Esc` closes. Same engine as `shiki query`; DSL strings can also be saved by name under `[queries]` and run with `shiki query --saved <name>` (see CLI commands) |
| `P` | Publish the selected notebook to a themed PDF via `pretty-pdf` (external binary, auto-fetched on first use — see CLI commands), written to `{data_dir}/exports/{notebook}.pdf`, then opened. Same rendering `shiki publish` uses; the theme comes from `export.pdf_theme`, cyclable in Settings → EXPORT |
| `x` | Export the selected notebook to HTML/Markdown — prompts for the output path (prefilled with `{data_dir}/exports/{notebook}.html`); same bundling `shiki export` does (see CLI commands) |
| `U` | Check for updates — modal; checks GitHub Releases in the background (never blocks the UI), shows "update available" if there's a newer version, and `Enter` downloads, verifies (against GitHub's own per-asset checksum), installs, and automatically relaunches into it |
| `u` | Undo the last delete — restores the most recently deleted note/folder (or whole batch, from a Visual-mode delete) from the trash (`~/.config/shiki/trash/`) back to exactly where it came from. A single level of undo, not a full history: only the *most recent* delete is restorable this way; an older one is still on disk in the trash, just no longer reachable from here. With nothing to undo, reports that instead of doing anything |
| `s` | Settings — near-full-screen, paged by tab (`←`/`→` switches GENERAL/THEME/GIT/EDITOR/EXPORT/NOTEBOOKS/SNIPPETS, `j`/`k` moves within one). Doesn't repeat the keybindings tables — `?` (which-key) already covers those live. Every tab is editable with `Enter`: GENERAL/GIT booleans (`use_favorite_editor`, `auto_commit`/`auto_push`/`sign_commits`/`auto_sync`) toggle in place and save immediately; every other GENERAL/GIT field opens a prompt prefilled with its current value; THEME's `name` opens the theme picker (live preview, same as leader+`c`) and `overrides` stays informational; EDITOR is fifteen plain boolean toggles for the native note editor's UX (see `[editor]` below); EXPORT is `pdf_theme` (cycling through `pretty-pdf`'s 17 built-in themes — the default `shiki publish`/leader+`P` fall back to), `export_dir` (a text prompt showing where PDFs land, resolved against the app's data dir when empty), and `ask_export_path` (a plain in-place toggle); NOTEBOOKS lists every notebook's actual git remote (redacted) and drills into one to edit its remote plus its `auto_push`/`auto_sync`/`auto_sync_every` overrides (booleans cycle inherit → true → false → inherit); SNIPPETS supports `a` (new snippet) and `d` (delete, with confirmation), and drilling into one edits its `label` and its full multi-line `body` through the same inline editor a note's own body uses. `i`/`E` still jump straight to editing `config.toml` itself for anything not covered above (inline or externally, same convention as editing a note); on save, the config is re-read, re-applied, and takes effect immediately (no restart) — an invalid edit is reported and neither written nor applied, keeping the previous config running. `h`/`Esc`/`Backspace` backs out of a drilled-into notebook/snippet a level; `Esc`/`q` at the top level closes |
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
| `a` | New note (empty title stamps today's date). After the title, a template picker opens — every `.md` file in `~/.config/shiki/templates/` plus a "blank" option; `j`/`k` browse, `Enter` picks one and jumps straight to editing (`{{title}}`/`{{date}}`/`{{time}}`/`{{notebook}}` already substituted, and a `{{cursor}}` marker — never saved to disk — leaves the cursor exactly where it was in the template instead of at the top), `Esc`/`q` cancels the note entirely. Typing `@` anywhere in the title prompt (with or without a title before it) opens a quick dropdown instead — `today`/`yesterday`/`tomorrow` (a computed date, no template) plus every available template, fuzzy-filtered as you keep typing; `Enter` creates the note and jumps straight to editing, skipping the title→Enter→pick-a-template two-step entirely |
| `f` | New folder — empty name cancels rather than creating something unnamed. Created at the current breadcrumb depth, so it can be nested arbitrarily by descending first |
| `r` | Rename note |
| `d` | Delete the selected note *or* folder (with confirmation) — a folder deletes everything inside it too. In `v` select mode, deletes every selected item at once. Moved to the trash rather than permanently removed, so leader+`u` can undo it |
| `i` | Edit inline (or the OS favorite editor if `general.use_favorite_editor`) |
| `E` | Edit externally ($EDITOR) |
| `/` | Fuzzy-jump to a note by title anywhere in the current notebook (any folder depth) |
| `t` | New/open today's daily note. On first creation each day (and only if `general.daily_agenda` is on, the default), a "## Due today" section is appended after the template with every pending task due today or overdue, across every notebook — plain bullets with a `[[wikilink]]` back to each task's source note (deliberately not checkbox copies, which would double-count in the tasks view). Reopening later never re-injects or duplicates it |
| `m` | Move the selected note *or* folder — prompts for a `notebook/path/within/it` target, prefilled with the current one; edit the trailing segments to move within the same notebook (missing folders are created), or replace the first segment to move to a different (existing) notebook. In `v` select mode, moves every selected item at once |
| `o` | Cycle sort order (filename / title A-Z / date newest-first) |
| `T` | Tree view — every folder and note in the notebook, fully expanded, in one scrollable overview; `j`/`k` move, `enter`/`l` jumps straight to the selected note, `esc`/`q` closes |
| `D` | Toggle each note's date next to its title in the list (off by default) |
| `v` | Select mode (`Mode::Visual`) — anchors a multi-select range at the current item; `j`/`k` extend/shrink it, `v`/`Esc` cancels. `d`/`m` (above) act on the whole range instead of one item |
| `y` | Select-mode only: copies every selected note/folder to a prompted target (same `notebook/path` syntax as `m`), leaving the originals in place |
| `M` | Metadata editor — the selected note's tags plus every custom frontmatter field (`status`, `priority`, `due`, or anything else), add/edit/delete in place without leaving the TUI. Also on PREVIEW scope |

#### `[keybindings.preview]` — active while PREVIEW is focused

| Key | Action |
|---|---|
| `i` | Edit inline (or the OS favorite editor if `general.use_favorite_editor`) |
| `E` | Edit externally ($EDITOR) |
| `H` | Note history — every commit that changed this specific note, newest first, real git history (not a separate versioning system). `j`/`k`/`PageUp`/`PageDown`/`Home`/`End` move, `Enter` views a revision's full content (frontmatter included, since that's what's actually in the commit), `d` views a real unified diff of that revision against its parent instead (colored `-`/`+` lines, computed by libgit2 itself, not a hand-rolled line algorithm — the first commit in a note's history has no parent, so every line comes back as an addition) — `d` also works from inside the full-content view to switch straight to the diff of the same revision, `r` reverts to the highlighted (or currently-viewed, either view) revision — behind a confirmation, since it overwrites the current content. The revert itself doesn't commit; it shows up as a normal pending change, picked up by `s`/`u`/`auto_sync` like any other edit. The footer shows the count while reading a note (`{n} changes`) |
| `L` | Links — the selected note's outgoing `[[wikilinks]]` (resolved against every note in the notebook, any folder depth), every other note that links back to it, and notes that *mention* this note's title in plain text without linking to it ("Outgoing"/"Backlinks"/"Mentions (unlinked)" sections; a section with nothing in it is omitted). `j`/`k`/`PageUp`/`PageDown`/`Home`/`End` move, `Enter` jumps to the selected note (an unresolved outgoing link reports that instead of jumping), `c` on a mention row *repairs* the missed link — it wraps that note's plain-text mention into a real `[[wikilink]]` (preserving its casing) and the row visibly migrates to Backlinks — and `Esc`/`q` closes. Also reachable globally via leader+`B` |
| `o` | Outline — every `#`..`######` heading in the selected note, indented by level. `j`/`k`/`PageUp`/`PageDown`/`Home`/`End` move, `Enter` scrolls PREVIEW to that heading, `Esc`/`q` closes. Also reachable as `Ctrl+O` from inside `Mode::Edit` itself — there, `Enter` moves the editor's own cursor to the heading instead of scrolling PREVIEW, and the headings come from the live, possibly-unsaved buffer rather than the note's last-saved body |
| `M` | Metadata editor — same action as NOTES scope's `M`, bound here too so it works with PREVIEW focused as well |

Mouse: a plain click over a note's rendered body jumps straight into the inline editor with the
cursor on the clicked line — a mouse-only alternative to `i`/vim motions. Click-and-drag instead
selects the dragged rows (highlighted with the theme's `selection` color) and copies them to the
clipboard the moment the button is released — no extra keypress needed, same OSC 52 mechanism as
the logs modal's `y`/`c`. Both are controlled by `general.mouse_drag_selection` (on by default;
toggle it in Settings' GENERAL tab or in `config.toml`).

A `<details>`/`<summary>` block in a note's body renders as a real collapsible section in PREVIEW:
expanded blocks show a `▾` handle (the summary text, the body below it), collapsed ones a `▸`
handle plus a muted hidden-line count (`▸ More  (12 hidden)`) with everything inside omitted — a
plain click on the summary row toggles it (instead of entering edit mode there), and the fold state
is kept per note for the session, so folding a long section once keeps it folded while you browse.
This is the same `<details>`/`<summary>` markup the `/`-menu's `details` block inserts.

A `$$…$$` math block (the `/`-menu's `math` block, or any `$$...$$`/`$$\n...\n$$` in the body)
renders with its content prettified from raw LaTeX to readable Unicode — `\frac{a}{b}` → `a/b`,
`\sqrt{x}` → `√x`, `^2` → `²`, `_0` → `₀`, `\pi` → `π`, `\int` → `∫`, `\infty` → `∞`, Greek
letters and common operators — so `$$\int_0^\infty e^{-x^2} dx = \frac{\sqrt{\pi}}{2}$$` reads as
`∫₀^∞ e⁻ˣ² dx = √π/2`. A lightweight hand-rolled converter, not a TeX engine: constructs it
doesn't recognize pass through unchanged rather than being mangled. Inline `$$...$$` in the middle
of a line ("so $$a^2 + b^2 = c^2$$ and more") is prettified the same way, styled with the math
accent/italic.

A ` ```mermaid ` fence renders as an actual diagram instead of flat text: flowcharts
(`graph TD`/`flowchart LR`, …) become an indented tree with box-drawing connectors, node shapes
(`A[Label]`/`A(Label)`/`A{Label}`/`A((Label))`/`A[[Label]]`), and edge labels; sequence diagrams
(`sequenceDiagram` with `participant` + `->>`/`-->>`/`->`/`--x` messages) render as participant
columns with arrows between them. Same hand-rolled-parser approach as the math converter — a
diagram it can't parse falls back to the previous flat styling rather than breaking.

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
  the same prefix onto the next line (checkboxes always reset to `[ ]`, never copy `[x]`; an
  ordered `N.` marker increments — `1.` → `2.` → `3.` … as you keep pressing `Enter`, rather than
  repeating the same number); `Enter` on an already-empty list item exits the list instead of
  continuing it, and `Backspace` right after an empty prefix removes it in one step instead of one
  character at a time. The same setting also gates `Tab`/`Shift+Tab` on a list/checkbox line with
  nothing selected: they nest it one level deeper or back out (adding/removing 2 leading spaces),
  keeping the cursor over the same character.
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
- `move_line` (on by default): `Alt+↑`/`Alt+↓` move the current line past its neighbor.
- `duplicate_line` (on by default): `Alt+D` duplicates the current line directly below itself.
- `block_indent_select` (on by default): `Tab`/`Shift+Tab` with an active selection indent/outdent
  every line the selection spans — a plain block-indent, not list-specific. With this off, `Tab`
  on a selection falls through to whatever else applies (snippet expansion, list nesting, or a
  literal tab).

`[[wikilink]]` autocomplete (gated by `[general].wikilink_autocomplete`, on by default, not by
`[editor]`): typing `[[` opens an Obsidian-style fuzzy note picker (same fuzzy matching as `/` and
global search), showing each candidate's folder breadcrumb so notes with duplicate titles in
different folders stay distinguishable; picking one inserts `[[Title]]`. Off falls through to a
literal `[[` with no menu. In PREVIEW, `Ctrl`+click on a rendered `[[wikilink]]` jumps straight to
the note it resolves to — a plain click still enters edit mode everywhere, including on top of a
wikilink.

Navigation inside the editor is always on, not gated by `[editor]`: `PageUp`/`PageDown` move the
cursor a page at a time, plain `Home` is "smart" — the first press goes to the line's first
non-whitespace character, pressing it again (or pressing it on a line already at column 0) goes to
column 0 — `End` goes to the end of the current line, `Ctrl+Home`/`Ctrl+End` jump to the very
start/end of the note, and the mouse wheel scrolls too. `PageUp`/`PageDown`/mouse-wheel scrolling
move the cursor itself (there's no independent scroll offset — the editor's word-wrap support
means it bypasses `ratatui-textarea`'s own rendering, and with it, `ratatui-textarea`'s own
viewport-based `PageUp`/`PageDown`, which otherwise does nothing here).

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
shiki capture "quick idea"       # near-instant note capture, no $EDITOR, no TUI drawn
shiki capture "text" -n work     # capture into a specific notebook instead of default_notebook
echo "piped idea" | shiki capture      # reads the text from stdin when no argument is given
shiki capture "call Ana" --tags work,idea   # comma-separated tags, same flag as `shiki new`
shiki capture "call Ana" --daily      # appends as a bullet to today's daily note instead of a new note
shiki capture "call Ana" --json       # emits {"path":..,"daemon":..,"daily":..} for scripts
shiki capture --check                 # is a capture daemon reachable right now? exits non-zero if not
shiki capture "meeting notes" --folder work/meetings -n work   # into a subfolder, not the notebook root
shiki capture "work: call Ana"        # no -n given -> routed into the "work" notebook automatically
shiki capture --undo                  # reverses the single most recent capture (any kind)
shiki new "title" --body "text"     # create non-interactively, no $EDITOR spawned
shiki new "title" --stdin --tags work,idea  # body piped in, tags attached, still no $EDITOR
shiki list --json         # list/search/show all take --json for scripting (list/search: array, show: object)
shiki tasks               # every pending checkbox task across notebooks, urgency-sorted
shiki tasks --overdue --count  # just the number — made for waybar/polybar/tmux status modules
shiki tasks --today --json     # machine-readable, with due/overdue/location per task
shiki graph               # [[wikilink]] connection graph, force-directed, drawn in the terminal
shiki graph -n work --json     # nodes/edges/orphans as JSON, for graphviz/d3/gephi
shiki graph --width 120        # custom canvas width in columns (default: the terminal's own width)
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
shiki notebook encrypt <name>       # enable encryption at rest (prompts for a passphrase, twice)
shiki notebook decrypt <name>       # reverse it — decrypts every note back to plain text
shiki notebook rekey <name>         # change the passphrase (verifies the old one, re-encrypts in place)
shiki query 'where status = pending sort due asc'   # Dataview-style filter/sort over frontmatter
shiki query 'where due < today' --count             # for status bars, like `shiki tasks --count`
shiki query --saved due-soon                        # run a query saved under [queries] in config.toml
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

## Quick capture (`shiki capture` + optional daemon)

`shiki capture "some text"` creates a note with no ceremony — no `$EDITOR` spawned, no TUI drawn,
just a single line of output. It's meant to be wired into things *outside* shiki entirely: a rofi/
wofi prompt bound to an OS hotkey, a waybar/polybar click action, a Raycast/Alfred script command,
an AutoHotkey shortcut on Windows — anything that can shell out. Shiki doesn't (and can't, from a
terminal app) register a global OS hotkey itself; `shiki capture` is the one command every one of
those launchers just needs to call.

By default it targets `general.default_notebook` (auto-created if it doesn't exist yet, same as
`shiki new`), with an auto-generated title (`Capture 2026-08-10 15:35`, no title prompt) so there's
nothing to type but the note's actual content. `-n <notebook>` overrides the target, same flag as
`shiki new`/`shiki daily`. The text itself is a plain positional argument; omit it entirely and
shiki reads it from stdin instead (`echo "idea" | shiki capture`), for wiring into another
program's own output rather than typing a literal string.

```
shiki capture "buy milk"          # -> personal/capture-2026-08-10-15-35.md
shiki capture "call Ana" -n work  # -> work/capture-....md instead
```

`--tags work,idea` sets tags on the created note, same flag/format as `shiki new --tags`.
`--daily` changes the target entirely: instead of a new note, the text is appended as a `- ` bullet
under today's daily note (created via the same `shiki_core::daily::create_or_open` the `t`
keybinding/`shiki daily` already use — template + agenda section included on first creation, an
already-existing daily is just opened and appended to) — for treating the daily note as a running
inbox for the whole day rather than one note per capture. `--tags` is ignored when `--daily` is
also given, since an appended bullet has no frontmatter of its own to carry tags on.
`--json` emits `{"path": "...", "daemon": true|false, "daily": true|false}` instead of the plain
sentence, for a script or browser extension that wants to act on the result without string-matching
`"captured (daemon): "`. `--folder work/meetings` creates the note inside that subfolder of the
notebook instead of its root (each path segment validated the same way a notebook/folder name is —
no `..`/empty segments); ignored when `--daily` is given, since a daily note's path is always fixed.

**Content-prefix routing**: if no `-n` was given and the text itself looks like `"<notebook>:
<rest>"` where `<notebook>` case-insensitively matches a real, existing notebook, that notebook is
used automatically and the prefix is stripped from the saved text — `shiki capture "work: call
Ana"` needs no `-n work` at all. An explicit `-n` always wins outright and skips this check
entirely, so a genuine note that happens to start with `"word: "` is never mis-routed as long as a
target was actually given.

**The optional daemon** (`general.enable_capture_daemon`, off by default — toggle it from
`leader+s` → GENERAL → `enable_capture_daemon`) makes a *running* TUI aware of captures the instant
they happen, instead of only writing to disk unnoticed until the next manual reload. With it on,
the TUI listens on a local TCP loopback port (`127.0.0.1`, OS-assigned, recorded in
`~/.config/shiki/capture.port`); `shiki capture` tries that socket first, and if a TUI answers, the
new note appears live in NOTES (when you're already looking at that notebook's root) with no
keypress needed. The command's own output tells you which path it took:

```
$ shiki capture "buy milk"
captured (daemon): /home/you/notes/personal/capture-2026-08-10-15-35.md   # a running TUI picked it up live
$ shiki capture "buy milk"
captured: /home/you/notes/personal/capture-2026-08-10-15-35.md            # written straight to disk, no TUI (or daemon off) noticed
```

Turning the daemon off doesn't turn *capture* off — `shiki capture` always works, with or without
a TUI running; the toggle only controls whether an already-open TUI finds out immediately. Toggling
it back on later reuses the same listener thread rather than restarting anything — the daemon, once
started this session, never actually shuts down; off just means it answers "disabled" to new
connections instead of processing them.

`shiki capture --check` reports whether a daemon is reachable right now without capturing anything
(and without touching stdin) — meant for a status-bar module or launcher script that wants to show
"capture: on"/"capture: off", or decide something differently, before committing to a real capture.
It exits `0` when a daemon answered (regardless of whether it said enabled or disabled — reachable
at all means a TUI process exists) and non-zero otherwise, so `shiki capture --check && ...`
composes naturally in a script; `--json` emits `{"reachable": true|false, "enabled": true|false}`.
Every capture handled by the daemon (not the standalone fallback — there's no running `App` to log
into in that case) is also recorded in the TUI's own log history (`leader` then `l`), so a capture
that happened while nobody was watching the screen still leaves a trace, the same way a background
git sync result does.

`shiki capture --undo` reverses the single most recent capture — a plain note is moved to trash
(restorable exactly like any other deleted note); a `--daily` append instead strips exactly the
bullet that was added off the end of the daily note's body, and only if the body still ends with it
verbatim — if the daily note was edited in between, undo refuses rather than risk removing content
you actually meant to keep. This is a *one-slot* undo, not a stack, same simplicity level as
`leader+u` (undo delete): a second `--undo` in a row reports "nothing to undo" rather than reaching
further back. The record backing it (`~/.config/shiki/last-capture.toml`) is shared between the
daemon and the standalone fallback, so undo works the same regardless of which one made the
original capture — it tries the daemon first (for a live TUI refresh if the reverted item was on
screen), then falls back to reversing it directly.

Capturing into an **encrypted** notebook only works through the daemon if that notebook is already
unlocked in the running TUI this session — the background listener thread can't itself pop up a
passphrase prompt, so it replies with a clear "locked" error instead of hanging or writing
plaintext, and `shiki capture` reports that error rather than silently falling back. Run
`shiki capture` from a plain terminal (no daemon involved) against an encrypted, locked notebook
and it prompts for the passphrase interactively instead, same as `shiki new`.

---

## Encryption at rest (per notebook, passphrase-based)

Any notebook can be encrypted independently — `leader+s` → NOTEBOOKS → drill into a notebook →
`encrypted` field, or `shiki notebook encrypt <name>` from the CLI. Every note's full content
(frontmatter + body) is encrypted as one [age](https://age-encryption.org)-armored blob using a
passphrase (`age::scrypt` — symmetric, no keypair), so the file on disk is still plain ASCII text
(`git diff`/`git log -p` don't flip to "binary files differ"), just unreadable without the
passphrase. Note filenames stay in the clear — the one accepted metadata leak, since opaque
filenames would need a bigger redesign.

**There is exactly one passphrase per notebook — not one per machine, not one per person.** It's
the actual encryption key (well, the input `age::scrypt` derives the real key from), the same way a
password-protected ZIP file or a KeePass database has one password that opens it anywhere, on any
machine. Shiki **never** stores or syncs this passphrase anywhere — not in `config.toml`, not in
the git repo, not in any file. You are the only distribution mechanism: write it down in a password
manager, remember it, whatever — just don't lose it. There is no recovery path if you do; nothing in
shiki can decrypt a notebook without the exact passphrase that encrypted it.

Walking through it end to end, encrypting `vault` with passphrase `1234567` on one machine and then
opening the same repo on a second machine:

1. **Machine A** (has the plaintext notebook): `shiki notebook encrypt vault` prompts for the
   passphrase twice (typo protection), writes a small canary file (`.shiki-encryption`, itself
   encrypted with that passphrase — exists purely to verify a passphrase attempt without risking a
   real note) at the notebook's root, re-encrypts every existing note, flips
   `[notebooks.vault] encrypt = true` in **machine A's own** `config.toml`, and commits everything.
   You push as usual — what lands on GitHub/GitLab/wherever is ciphertext notes plus the encrypted
   canary. Nobody who can see the repo (including the hosting provider) can read the content
   without the passphrase.
2. **Machine B** `git clone`s or `git pull`s that repo. It gets the same ciphertext files and the
   canary — but its *own* `config.toml` (which never travels with the git repo; it lives outside it
   entirely, under `~/.config/shiki/`, precisely so the passphrase/flag can't leak through git) has
   no idea this notebook is encrypted at all yet.
3. Opening `vault` on machine B, shiki doesn't consult that config flag to decide whether to prompt
   — it sniffs the *actual file content* for the age armor header, so it notices the encryption
   regardless of what machine B's config says, and asks: `Passphrase — unlock 'vault'`.
4. You type the **same** `1234567` you used on machine A — not a different one, not "machine B's
   own" passphrase. Shiki decrypts the canary with it; if it matches, every note in the notebook now
   shows up decrypted in the UI, and machine B's `config.toml` also gets `encrypt = true` written
   into it (so a note you create or edit on machine B from now on encrypts too, instead of silently
   falling back to plain text because that one machine's config didn't know yet).
5. Typing the wrong passphrase at step 4 fails the canary check with a clear error and nothing is
   shown or touched — no corruption, just a re-prompt.

Practical consequences worth knowing:

- **CLI read commands don't prompt for a passphrase** (`shiki list`/`tasks`/`graph`/`show`/`search`)
  — they're built for non-interactive use (status bars, scripts), and blocking on a hidden-input
  prompt mid-script isn't viable. Against an encrypted, locked notebook they fail with a clear error
  instead of hanging or printing garbage; `shiki new`/`shiki daily` (the write paths) do prompt,
  since writing plaintext into what should be an encrypted notebook would be a real correctness bug,
  not just an inconvenience.
- The note-history modal (`H`) degrades gracefully for an encrypted notebook: viewing an old
  revision decrypts it the same way a live read does, but the unified *diff* view (`d`) can't work
  at all — a tree diff of two ciphertext blobs is meaningless noise, so it falls back to showing the
  decrypted full content instead, with a status message explaining why.
- Changing the passphrase is `shiki notebook rekey <name>` — it verifies the old passphrase
  against the canary, prompts for the new one twice, and re-encrypts every note in place without
  ever writing plaintext to disk mid-operation (no `decrypt` + `encrypt` two-step needed). Every
  other machine still holding the old ciphertext needs the new passphrase from that point on.

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
├── config.toml            # general configuration (keybindings, theme, git, editor, snippets…)
├── shiki.log              # persistent status/log history (leader+l to view, x to clear)
├── capture.port           # port the capture daemon is listening on (only while a TUI has it enabled)
├── last-capture.toml      # backs `shiki capture --undo` — removed once undone
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
# When true, the TUI listens on a local loopback port so external
# `shiki capture "text"` invocations land here live instead of only
# writing to disk unnoticed. `shiki capture` itself always works either
# way — this only controls whether an already-open TUI finds out
# immediately. Off by default; toggle from leader+s -> GENERAL.
enable_capture_daemon = false
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
# Shows the "Buy Me a Coffee" segment in the footer (mouse-clickable).
show_coffee_link = true
# When true, deleting a note/folder (including a Visual-mode batch delete)
# skips the confirm dialog and deletes immediately — still restorable via
# leader+`u`. Doesn't apply to notebook delete, which asks a real
# delete-vs-untrack question, not a plain yes/no safety gate.
skip_delete_confirm = false
# Shows each note's date next to its title in the NOTES list (same as the
# notes-scope `D` toggle — this is just its persisted default).
show_dates = false
# Typing `[[` in the inline editor opens the wikilink autocomplete menu.
wikilink_autocomplete = true
# Creating a new daily note appends a "## Due today" section listing
# pending/overdue tasks across every notebook (only on creation).
daily_agenda = true
# Hides char/word count, reading time, and note-count detail from the
# footer, leaving just the essentials — notebook, git status, editor mode.
compact_footer = false
# How long a footer status message stays visible before clearing itself.
status_message_timeout_secs = 2
# Width in columns of the notebook drawer (leader+`b`). Clamped against
# the frame's actual width at render time regardless of this value.
drawer_width = 30
# Whether the tasks view (leader+`t`) starts showing every task, including
# already-done ones, instead of just pending.
tasks_show_done_default = false
# "filename", "title", or "date" — which order the NOTES list sorts by on
# a fresh launch. The notes-scope `o` cycle still changes it for the rest
# of the session on top of whichever default this resolves to.
default_note_sort = "filename"
# Max entries kept in the logs modal (leader+`l`) and the persisted log
# file. Lowering this doesn't retroactively trim an existing log file.
log_history_limit = 500
# Days a deleted note/folder stays in the trash before being permanently
# purged at startup. 0 (the default) means never auto-purge.
trash_retention_days = 0
# Words-per-minute used for the footer's "N min read" estimate.
reading_wpm = 200
# How many rows PageUp/PageDown (and the mouse wheel) move at once, across
# every scrollable list/modal in the TUI.
page_step = 10

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
# Dataview-style query modal over frontmatter — same engine as `shiki query`.
query_panel = "q"
publish = "P"
# Same HTML/Markdown bundling as `shiki export`.
export = "x"
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
# Metadata editor — tags plus custom frontmatter fields, add/edit/delete.
metadata = "M"

[keybindings.preview]
edit_inline = "i"
edit_external = "E"
history = "H"
links = "L"
outline = "o"
# Same metadata editor as NOTES scope, bound here too.
metadata = "M"

[theme]
name = "gruvbox-dark"
icons = true  # false falls back to plain text — no Nerd Font glyphs anywhere
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
# toggleable from the EDITOR tab in Settings (leader+`s`); the mostly-additive
# conveniences default to true, the ones that change existing behavior
# (clipboard wiring, line-number gutter, multi-cursor, Ctrl+A select-all,
# typewriter scrolling) default to false.
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
move_line = true          # Alt+Up/Alt+Down move the current line past its neighbor
duplicate_line = true     # Alt+D duplicates the current line
block_indent_select = true  # Tab/Shift+Tab with a selection indent/outdent every line it spans

# PDF export (`shiki publish`, leader+`P`) — pdf_theme picks one of
# go-pretty-pdf's 17 built-in themes: default, minimal, modern, classic,
# corporate, dark, academic, editorial, sepia, terminal, blueprint, ivy,
# government, resume, legal, latex, gruvbox. Cyclable from the EXPORT tab
# in Settings. export_dir relocates where PDFs land (empty = the app's own
# data dir); ask_export_path prompts for the exact save path on every
# publish instead of silently writing there.
[export]
pdf_theme = "default"
# export_dir = ""       # save PDFs elsewhere instead of the app's data dir
# ask_export_path = true  # prompt for the save path on every publish

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
# Encrypts every note at rest with a passphrase (prompted, never stored here
# or anywhere else — see "Encryption at rest" above). No global default to
# inherit from; this is opt-in per notebook, managed via `shiki notebook
# encrypt/decrypt/rekey <name>` or Settings → NOTEBOOKS, not by hand-editing
# this.
# encrypt = true
# `hidden` is set automatically when "delete notebook" is answered with "just
# remove the reference": the directory on disk is left untouched, the notebook
# just stops being listed. Un-hide it from Settings → NOTEBOOKS (drill into
# the `(hidden)` entry and clear the flag), not by hand-editing here.

# Custom entries for the inline editor's `/`-menu, keyed by trigger. Empty by
# default — the built-in commands (h1/h2/h3/code/math/table/check/quote/
# divider/date/tags/frontmatter/bullet/numbered/link/image/note/warning/
# details) aren't listed here at all, only your own additions/overrides.
# `label` falls back to the trigger when omitted; `body` supports
# {{title}}/{{date}}/{{time}}/{{notebook}} (substituted the same way note
# templates are) plus a {{cursor}} marker for where the cursor lands after
# insertion.
[snippets.callout]
label = "Info callout"
body = "> **Info:** {{cursor}}"

# Same trigger as a built-in (case-insensitive) replaces it instead of
# adding a duplicate — every command in the menu is customizable this way.
[snippets.h1]
body = "# [{{title}}] {{cursor}}"

# Named, saved `shiki query` DSL strings — run one with `shiki query --saved <name>`.
# Same Dataview-style language as the leader+`q` modal and the literal CLI arg.
[queries]
# due-soon = "where due < today sort due asc"
```

---

## Design principles

1. **A single binary** — `shiki` does everything, CLI and TUI.
2. **Plain text** — notes are `.md` with frontmatter. Nothing proprietary.
3. **Git native** — each notebook is its own repo. Commit, push, pull from the app.
4. **No phases** — everything here is implemented in full. There's no "Phase 2".
5. **Fast** — Rust + ratatui. Sub-millisecond renders.
6. **Configurable** — keybindings, themes, editor, all in TOML.
7. **Yazi-inspired** — three panels, modal, which-key, single-threaded event loop.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::theme::Theme;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("toml serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("could not determine the user's config directory")]
    NoConfigDir,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct General {
    /// Field-level default so a `[general]` table missing this one key
    /// (e.g. a hand-edit through the Settings screen that deleted a line
    /// by accident) still parses instead of failing the entire config —
    /// same reasoning as every other `#[serde(default = "...")]` in this
    /// file, just extended to the fields that predate that convention.
    #[serde(default = "default_notebook_name")]
    pub default_notebook: String,
    #[serde(default = "default_editor")]
    pub editor: String,
    #[serde(default = "default_daily_template")]
    pub daily_template: String,
    /// When true, `i` (edit inline) instead detects the OS's favorite/default
    /// text editor and opens the note there, like `E` but auto-resolved
    /// instead of using a fixed `editor` command.
    #[serde(default)]
    pub use_favorite_editor: bool,
    /// When true, the TUI listens on a local TCP loopback port
    /// (`Config::default_capture_port_path` records which one) so external
    /// `shiki capture "text"` invocations land in this running instance
    /// live instead of only writing to disk unnoticed. Off by default —
    /// `shiki capture` itself always works regardless of this setting; it
    /// only controls whether an already-open TUI finds out about it
    /// immediately.
    #[serde(default)]
    pub enable_capture_daemon: bool,
    /// When true, click-and-drag over a note's body in PREVIEW selects text
    /// and copies it to the clipboard (OSC 52) on release. Defaults to
    /// `true`, so this needs the named-default-fn form rather than bare
    /// `#[serde(default)]`, which would resolve a missing key to `false`.
    #[serde(default = "default_mouse_drag_selection")]
    pub mouse_drag_selection: bool,
    /// Optional override for the notebooks data directory. When set, Shiki
    /// will look for (and create) notebooks under this path instead of the
    /// platform default (~/.local/share/shiki/). Useful for pointing Shiki
    /// at an existing Obsidian vault or any other directory of markdown notes.
    #[serde(default)]
    pub data_dir: Option<String>,
    /// When true, text-input prompts that have one (currently just
    /// `NewNotebook` — see `PendingInput::hint`) show a small muted line
    /// under the input box explaining non-obvious input (e.g. that pasting a
    /// git URL clones instead of creating a plain notebook). Defaults to
    /// `true` since the whole point is surfacing a feature that isn't
    /// otherwise discoverable; existing configs missing this key still parse
    /// via `default_true`, same as every other bool added after this file's
    /// initial fields.
    #[serde(default = "default_true")]
    pub show_hints: bool,
    /// When true, quitting the TUI saves exactly where you were (notebook,
    /// folder, selected note/folder, and which panel had focus) and the next
    /// launch restores it verbatim instead of always starting at the first
    /// notebook's root. Defaults to `true`, so this needs the named-default-fn
    /// form rather than bare `#[serde(default)]`, which would resolve a
    /// missing key to `false`. See `shiki_config::SessionState`.
    #[serde(default = "default_true")]
    pub remember_last_session: bool,
    /// Shows the "Buy Me a Coffee" segment in the footer (right-aligned,
    /// mouse-clickable — see `status_bar::coffee_hit_at`). Defaults to
    /// `true`; off removes the segment entirely rather than leaving a
    /// disabled/greyed-out placeholder.
    #[serde(default = "default_true")]
    pub show_coffee_link: bool,
    /// When true, deleting a note/folder (notes-scope `d`, including a
    /// Visual-mode batch delete) skips the "are you sure?" confirm dialog
    /// and deletes immediately. Off by default — this is a genuine behavior
    /// change, not a pure addition. Deliberately doesn't apply to notebook
    /// delete, which asks a real three-way question (delete files / just
    /// untrack / cancel), not a plain yes/no safety gate; every note/folder
    /// delete is still restorable via leader+`u` regardless of this setting,
    /// which is what makes skipping the prompt here reasonable at all.
    #[serde(default)]
    pub skip_delete_confirm: bool,
    /// Shows each note's date next to its title in the NOTES list. Used to
    /// be a runtime-only `App` field (notes-scope `D`, reset every launch);
    /// promoted to a real config field so the choice persists. Off by
    /// default, unchanged from the runtime-only behavior it replaces.
    #[serde(default)]
    pub show_dates: bool,
    /// Typing `[[` in the inline editor opens the wikilink autocomplete
    /// menu. Defaults to `true`; off falls through to a literal `[[` with
    /// no menu, for anyone who finds the popup intrusive while typing.
    #[serde(default = "default_true")]
    pub wikilink_autocomplete: bool,
    /// Creating a new daily note appends a "## Due today" section listing
    /// pending/overdue tasks across every notebook (only on creation, never
    /// on reopen — see `shiki_core::daily::create_or_open`). Defaults to
    /// `true`; off creates a daily note with no agenda section at all.
    #[serde(default = "default_true")]
    pub daily_agenda: bool,
    /// Hides the footer's character/word count and reading-time estimate
    /// (PREVIEW) and note-count breakdown (browsing), leaving just the
    /// essentials — notebook name, git status, editor mode. Off by default:
    /// this is a visible reduction, not a pure addition.
    #[serde(default)]
    pub compact_footer: bool,
    /// How long a footer status message stays visible before clearing
    /// itself (`App::expire_status_message`), in seconds. Defaults to `2`.
    /// The full message is always in the logs modal (leader+`l`) regardless
    /// of how quickly the footer clears it.
    #[serde(default = "default_status_message_timeout_secs")]
    pub status_message_timeout_secs: u64,
    /// Width, in terminal columns, of the notebook drawer (leader+`b`).
    /// Defaults to `30`. Clamped against the frame's actual width at
    /// render time regardless of this value, so an overly large number on
    /// a narrow terminal can't push the drawer off-screen.
    #[serde(default = "default_drawer_width")]
    pub drawer_width: u16,
    /// Whether the tasks view (leader+`t`) starts showing every task,
    /// including already-done ones, instead of just pending — the
    /// persisted default for the tasks-view `a` toggle, which used to
    /// always reset to pending-only on every open. Defaults to `false`.
    #[serde(default)]
    pub tasks_show_done_default: bool,
    /// Which order the NOTES list sorts by on a fresh launch — one of
    /// `"filename"`, `"title"`, or `"date"` (case-insensitive; an
    /// unrecognized value falls back to `"filename"`, the existing
    /// default, same tolerance `pdf_theme` already has for a typo'd
    /// theme name). The notes-scope `o` cycle still changes it for the
    /// rest of the session on top of whichever default this resolves to.
    #[serde(default = "default_note_sort")]
    pub default_note_sort: String,
    /// Maximum number of entries kept in the logs modal (leader+`l`) and
    /// in the persisted log file (`Config::default_log_path`). Defaults to
    /// `500`. Lowering this doesn't retroactively trim an existing log
    /// file — it only caps growth going forward.
    #[serde(default = "default_log_history_limit")]
    pub log_history_limit: usize,
    /// Days a deleted note/folder stays in the trash (`Config::default_trash_dir`)
    /// before `App::purge_old_trash` (run once at startup) removes it for
    /// good. `0` (the default) means never auto-purge — trash is only ever
    /// cleared by hand today, and this stays that way unless explicitly
    /// configured otherwise.
    #[serde(default)]
    pub trash_retention_days: u32,
    /// Words-per-minute used for the footer's "N min read" estimate
    /// (PREVIEW). Defaults to `200`, the same figure Medium/most
    /// reading-time plugins use.
    #[serde(default = "default_reading_wpm")]
    pub reading_wpm: usize,
    /// How many rows `PageUp`/`PageDown` (and the mouse wheel, where it
    /// maps to the same delta) move at once, across every scrollable
    /// list/modal in the TUI. Defaults to `10`.
    #[serde(default = "default_page_step")]
    pub page_step: usize,
}

impl Default for General {
    fn default() -> Self {
        Self {
            default_notebook: default_notebook_name(),
            editor: default_editor(),
            daily_template: default_daily_template(),
            use_favorite_editor: false,
            enable_capture_daemon: false,
            mouse_drag_selection: true,
            data_dir: None,
            show_hints: true,
            remember_last_session: true,
            show_coffee_link: true,
            skip_delete_confirm: false,
            show_dates: false,
            wikilink_autocomplete: true,
            daily_agenda: true,
            compact_footer: false,
            status_message_timeout_secs: default_status_message_timeout_secs(),
            drawer_width: default_drawer_width(),
            tasks_show_done_default: false,
            default_note_sort: default_note_sort(),
            log_history_limit: default_log_history_limit(),
            trash_retention_days: 0,
            reading_wpm: default_reading_wpm(),
            page_step: default_page_step(),
        }
    }
}

fn default_status_message_timeout_secs() -> u64 {
    2
}

fn default_drawer_width() -> u16 {
    30
}

fn default_note_sort() -> String {
    "filename".into()
}

fn default_log_history_limit() -> usize {
    500
}

fn default_reading_wpm() -> usize {
    200
}

fn default_page_step() -> usize {
    10
}

fn default_notebook_name() -> String {
    "personal".into()
}

fn default_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| "nvim".into())
}

fn default_mouse_drag_selection() -> bool {
    true
}

fn default_daily_template() -> String {
    "daily".into()
}

/// Keybindings are segmented by *scope*: navigation (`hjkl`, arrows, `tab`,
/// `enter`, `?`) is hardcoded and not configurable here since it behaves the
/// same everywhere. Everything else is scoped to whichever panel has focus,
/// so the same physical key can mean different things in different panels
/// (e.g. `a` creates a notebook while NOTEBOOKS is focused, a note while
/// NOTES is focused) — each scope is its own small, independently editable
/// table below. `global` actions require the `leader` key first (press
/// leader, then the key) since they aren't tied to any one panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keybindings {
    /// Prefix key for `[keybindings.global]` actions — press this, then the
    /// action's key, e.g. `space` then `g` for global search.
    #[serde(default = "default_leader")]
    pub leader: String,
    #[serde(default = "default_quit")]
    pub quit: String,
    #[serde(default)]
    pub global: GlobalKeybindings,
    #[serde(default)]
    pub notebooks: NotebookKeybindings,
    #[serde(default)]
    pub notes: NoteKeybindings,
    #[serde(default)]
    pub preview: PreviewKeybindings,
}

fn default_leader() -> String {
    "space".into()
}

fn default_quit() -> String {
    "q".into()
}

impl Default for Keybindings {
    fn default() -> Self {
        Self {
            leader: default_leader(),
            quit: default_quit(),
            global: GlobalKeybindings::default(),
            notebooks: NotebookKeybindings::default(),
            notes: NoteKeybindings::default(),
            preview: PreviewKeybindings::default(),
        }
    }
}

/// `<leader>` + key — actions that aren't tied to a specific panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalKeybindings {
    #[serde(default = "default_theme_picker_key")]
    pub theme_picker: String,
    #[serde(default = "default_global_search_key")]
    pub global_search: String,
    #[serde(default = "default_tags_panel_key")]
    pub tags_panel: String,
    /// Opens the logs modal (a scrollback of every status-bar message,
    /// including errors that already scrolled past). Field-level default so
    /// an existing `[keybindings.global]` table written before this key
    /// existed still deserializes instead of erroring on the missing field.
    #[serde(default = "default_logs_key")]
    pub logs: String,
    /// Flips `general.use_favorite_editor` on/off and persists it, without
    /// hand-editing config.toml. Field-level default for the same
    /// backward-compatibility reason as `logs`.
    #[serde(default = "default_toggle_favorite_editor_key")]
    pub toggle_favorite_editor: String,
    /// Opens the update modal: checks GitHub Releases for a newer version,
    /// and on confirmation downloads + verifies + installs it in place.
    /// Field-level default for the same backward-compatibility reason as
    /// `logs`/`toggle_favorite_editor`.
    #[serde(default = "default_check_update_key")]
    pub check_update: String,
    /// Toggles the notebook drawer: a left-side sidebar listing every
    /// notebook's git status in color (dirty/ahead/behind), separate from
    /// the always-visible NOTEBOOKS panel. Field-level default for the same
    /// backward-compatibility reason as `logs`/`toggle_favorite_editor`.
    #[serde(default = "default_drawer_key")]
    pub drawer: String,
    /// Restores the most recently deleted note/folder (or batch of them)
    /// from the trash — a single level of undo, not a full history. Field-
    /// level default for the same backward-compatibility reason as
    /// `logs`/`toggle_favorite_editor`.
    #[serde(default = "default_undo_delete_key")]
    pub undo_delete: String,
    /// Opens the Settings screen: every config option editable in place,
    /// grouped by section and paged by tab (GENERAL/THEME/GIT/EDITOR/
    /// EXPORT/NOTEBOOKS/SNIPPETS) — true/false fields toggle and save
    /// immediately, anything else opens a prompt prefilled with its current
    /// value, and NOTEBOOKS/SNIPPETS drill down a level. `i`/`E` still jump
    /// straight to editing `config.toml` itself (inline or externally — same
    /// convention as editing a note) for anything not covered by the tabs.
    /// Field-level default for the same backward-compatibility reason as
    /// `logs`/`toggle_favorite_editor`.
    #[serde(default = "default_settings_key")]
    pub settings: String,
    #[serde(default = "default_scratchpad_key")]
    pub scratchpad: String,
    /// Opens the links modal for the selected note (outgoing wikilinks,
    /// backlinks, and unlinked mentions) from anywhere — the same modal
    /// PREVIEW's own `links` binding opens, reachable without having to
    /// focus PREVIEW first. Field-level default for the same
    /// backward-compatibility reason as `logs`/`toggle_favorite_editor`.
    #[serde(default = "default_global_links_key")]
    pub links: String,
    /// Opens the global tasks view: every `- [ ]` checkbox across every
    /// notebook, toggleable in place. Field-level default for the same
    /// backward-compatibility reason as `logs`/`toggle_favorite_editor`.
    #[serde(default = "default_tasks_panel_key")]
    pub tasks_panel: String,
    /// Opens the query modal: a Dataview-style `where ... sort ...` DSL
    /// filtering/sorting notes by frontmatter field, live across every
    /// notebook — see `shiki_core::query`. Field-level default for the same
    /// backward-compatibility reason as `logs`/`toggle_favorite_editor`.
    #[serde(default = "default_query_panel_key")]
    pub query_panel: String,
    /// Renders the selected notebook to a themed PDF via `pretty-pdf`
    /// (fetched/cached automatically on first use if not already on
    /// `$PATH`), then opens it. Field-level default for the same
    /// backward-compatibility reason as `logs`/`toggle_favorite_editor`.
    #[serde(default = "default_publish_key")]
    pub publish: String,
    /// Exports the selected notebook to a single HTML or Markdown bundle —
    /// the same rendering `shiki export` (CLI) uses. Field-level default
    /// for the same backward-compatibility reason as `logs`/
    /// `toggle_favorite_editor`.
    #[serde(default = "default_export_key")]
    pub export: String,
    /// Forces the full-screen single-panel layout regardless of terminal
    /// size, hiding NOTEBOOKS/NOTES for distraction-free writing. Field-
    /// level default for the same backward-compatibility reason as
    /// `logs`/`toggle_favorite_editor`.
    #[serde(default = "default_zen_mode_key")]
    pub zen_mode: String,
}

impl Default for GlobalKeybindings {
    fn default() -> Self {
        Self {
            theme_picker: default_theme_picker_key(),
            global_search: default_global_search_key(),
            tags_panel: default_tags_panel_key(),
            logs: default_logs_key(),
            toggle_favorite_editor: default_toggle_favorite_editor_key(),
            check_update: default_check_update_key(),
            drawer: default_drawer_key(),
            undo_delete: default_undo_delete_key(),
            settings: default_settings_key(),
            scratchpad: default_scratchpad_key(),
            links: default_global_links_key(),
            tasks_panel: default_tasks_panel_key(),
            query_panel: default_query_panel_key(),
            publish: default_publish_key(),
            export: default_export_key(),
            zen_mode: default_zen_mode_key(),
        }
    }
}

fn default_zen_mode_key() -> String {
    "z".into()
}

fn default_publish_key() -> String {
    "P".into()
}

fn default_export_key() -> String {
    "x".into()
}

fn default_global_links_key() -> String {
    "B".into()
}

fn default_tasks_panel_key() -> String {
    "t".into()
}

fn default_query_panel_key() -> String {
    "q".into()
}

fn default_logs_key() -> String {
    "l".into()
}

fn default_theme_picker_key() -> String {
    "c".into()
}

fn default_global_search_key() -> String {
    "g".into()
}

fn default_tags_panel_key() -> String {
    "T".into()
}

fn default_settings_key() -> String {
    "s".into()
}

fn default_undo_delete_key() -> String {
    "u".into()
}

fn default_drawer_key() -> String {
    "b".into()
}

fn default_toggle_favorite_editor_key() -> String {
    "e".into()
}

fn default_check_update_key() -> String {
    "U".into()
}

fn default_scratchpad_key() -> String {
    "p".into()
}

/// Active only while the NOTEBOOKS panel has focus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookKeybindings {
    #[serde(default = "default_nb_new_key")]
    pub new: String,
    #[serde(default = "default_nb_rename_key")]
    pub rename: String,
    #[serde(default = "default_nb_delete_key")]
    pub delete: String,
    /// Stage + commit (+ push if `git.auto_push`) all changes in the notebook.
    #[serde(default = "default_sync_key")]
    pub sync: String,
    /// Fetch + fast-forward merge from the notebook's configured remote.
    #[serde(default = "default_pull_key")]
    pub pull: String,
    /// `pull` for every notebook that has a remote configured, in one go.
    #[serde(default = "default_pull_all_key")]
    pub pull_all: String,
    /// Prompts for a URL or local path and sets it as the notebook's `origin`.
    #[serde(default = "default_set_remote_key")]
    pub set_remote: String,
    /// Commits (same as `sync`) and always pushes, regardless of the
    /// resolved `auto_push` policy — the explicit "do it now" override.
    /// Field-level default so an existing `[keybindings.notebooks]` table
    /// written before this key existed still deserializes.
    #[serde(default = "default_push_key")]
    pub push: String,
}

impl Default for NotebookKeybindings {
    fn default() -> Self {
        Self {
            new: default_nb_new_key(),
            rename: default_nb_rename_key(),
            delete: default_nb_delete_key(),
            sync: default_sync_key(),
            pull: default_pull_key(),
            pull_all: default_pull_all_key(),
            set_remote: default_set_remote_key(),
            push: default_push_key(),
        }
    }
}

fn default_nb_new_key() -> String {
    "a".into()
}

fn default_nb_rename_key() -> String {
    "r".into()
}

fn default_nb_delete_key() -> String {
    "d".into()
}

fn default_sync_key() -> String {
    "s".into()
}

fn default_pull_key() -> String {
    "p".into()
}

fn default_pull_all_key() -> String {
    "P".into()
}

fn default_set_remote_key() -> String {
    "R".into()
}

fn default_push_key() -> String {
    "u".into()
}

/// Active only while the NOTES panel has focus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteKeybindings {
    #[serde(default = "default_note_new_key")]
    pub new: String,
    #[serde(default = "default_note_rename_key")]
    pub rename: String,
    #[serde(default = "default_note_delete_key")]
    pub delete: String,
    /// Edit in the built-in inline editor (or the favorite editor, see
    /// `general.use_favorite_editor`) — vim-style insert, not "e" for "edit".
    #[serde(default = "default_note_edit_inline_key")]
    pub edit_inline: String,
    #[serde(default = "default_note_edit_external_key")]
    pub edit_external: String,
    /// Fuzzy-jump to a note by title within the current notebook.
    #[serde(default = "default_search_key")]
    pub search: String,
    #[serde(default = "default_daily_note_key")]
    pub daily_note: String,
    /// Prompts for a target notebook name and moves the selected note there.
    #[serde(default = "default_move_to_notebook_key")]
    pub move_to_notebook: String,
    /// Cycles the notes list's sort order (filename/date vs. title).
    #[serde(default = "default_sort_key")]
    pub sort: String,
    /// Opens the tree view — every folder and note in the notebook, fully
    /// expanded, in one scrollable overview (Enter jumps straight to a
    /// note). Field-level default so an existing `[keybindings.notes]` table
    /// written before this key existed still deserializes.
    #[serde(default = "default_tree_view_key")]
    pub tree_view: String,
    /// Shows each note's date next to its title in the list (off by
    /// default — off is the "clean" state, on is opt-in clutter). Same
    /// field-level-default backward-compatibility reasoning as `tree_view`.
    #[serde(default = "default_toggle_dates_key")]
    pub toggle_dates: String,
    /// Creates an empty subfolder in the current breadcrumb (the same
    /// depth `new` would create a note at). Field-level default so an
    /// existing `[keybindings.notes]` table written before this key existed
    /// still deserializes.
    #[serde(default = "default_new_folder_key")]
    pub new_folder: String,
    /// Enters `Mode::Visual` (vi-style multi-select), anchored at whatever's
    /// selected — `move`/`delete` then act on the whole range instead of
    /// just one item. Field-level default for the same backward-compat
    /// reason as `tree_view`/`toggle_dates`/`new_folder`.
    #[serde(default = "default_visual_key")]
    pub visual: String,
    /// `Mode::Visual`-only: copies every selected note/folder to a prompted
    /// target instead of moving them. Same field-level-default reasoning.
    #[serde(default = "default_copy_entries_key")]
    pub copy_entries: String,
    /// Opens the metadata modal — tags plus every custom frontmatter field
    /// (`status`, `priority`, ...), add/edit/delete in place. Same
    /// field-level-default backward-compatibility reasoning as `tree_view`/
    /// `toggle_dates`/`new_folder`/`visual`/`copy_entries`.
    #[serde(default = "default_metadata_key")]
    pub metadata: String,
}

impl Default for NoteKeybindings {
    fn default() -> Self {
        Self {
            new: default_note_new_key(),
            rename: default_note_rename_key(),
            delete: default_note_delete_key(),
            edit_inline: default_note_edit_inline_key(),
            edit_external: default_note_edit_external_key(),
            search: default_search_key(),
            daily_note: default_daily_note_key(),
            move_to_notebook: default_move_to_notebook_key(),
            sort: default_sort_key(),
            tree_view: default_tree_view_key(),
            toggle_dates: default_toggle_dates_key(),
            new_folder: default_new_folder_key(),
            visual: default_visual_key(),
            copy_entries: default_copy_entries_key(),
            metadata: default_metadata_key(),
        }
    }
}

fn default_metadata_key() -> String {
    "M".into()
}

fn default_visual_key() -> String {
    "v".into()
}

fn default_copy_entries_key() -> String {
    "y".into()
}

fn default_new_folder_key() -> String {
    "f".into()
}

fn default_toggle_dates_key() -> String {
    "D".into()
}

fn default_note_new_key() -> String {
    "a".into()
}

fn default_note_rename_key() -> String {
    "r".into()
}

fn default_note_delete_key() -> String {
    "d".into()
}

fn default_note_edit_inline_key() -> String {
    "i".into()
}

fn default_note_edit_external_key() -> String {
    "E".into()
}

fn default_search_key() -> String {
    "/".into()
}

fn default_daily_note_key() -> String {
    "t".into()
}

fn default_move_to_notebook_key() -> String {
    "m".into()
}

fn default_sort_key() -> String {
    "o".into()
}

fn default_tree_view_key() -> String {
    "T".into()
}

/// Active only while the PREVIEW panel has focus (`j`/`k`/arrows scroll
/// instead of navigating a list while here).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewKeybindings {
    #[serde(default = "default_preview_edit_inline_key")]
    pub edit_inline: String,
    #[serde(default = "default_preview_edit_external_key")]
    pub edit_external: String,
    /// Opens the note's version history — every commit that changed it,
    /// with view + revert. Field-level default so an existing
    /// `[keybindings.preview]` table written before this key existed still
    /// deserializes.
    #[serde(default = "default_history_key")]
    pub history: String,
    /// Opens the links modal — the selected note's outgoing `[[wikilinks]]`
    /// plus every other note that links back to it. Same field-level-default
    /// backward-compatibility reasoning as `history`.
    #[serde(default = "default_links_key")]
    pub links: String,
    /// Opens the outline modal — every heading in the selected note, jump
    /// straight to one. Also reachable as `Ctrl+O` inside `Mode::Edit`
    /// itself. Same field-level-default backward-compatibility reasoning
    /// as `history`.
    #[serde(default = "default_outline_key")]
    pub outline: String,
    /// Opens the metadata modal — same action as `NoteKeybindings::metadata`,
    /// bound here too so it also works with PREVIEW focused, not only NOTES.
    #[serde(default = "default_metadata_key")]
    pub metadata: String,
}

impl Default for PreviewKeybindings {
    fn default() -> Self {
        Self {
            edit_inline: default_preview_edit_inline_key(),
            edit_external: default_preview_edit_external_key(),
            history: default_history_key(),
            links: default_links_key(),
            outline: default_outline_key(),
            metadata: default_metadata_key(),
        }
    }
}

fn default_outline_key() -> String {
    "o".into()
}

fn default_preview_edit_inline_key() -> String {
    "i".into()
}

fn default_preview_edit_external_key() -> String {
    "E".into()
}

fn default_history_key() -> String {
    "H".into()
}

fn default_links_key() -> String {
    "L".into()
}

/// Theme config: `name` references a built-in theme; the optional fields
/// allow overriding individual color slots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_theme_name")]
    pub name: String,
    /// When false, every Nerd Font glyph in the TUI (notebook/note icons,
    /// git status, the list-selection marker, …) falls back to plain text
    /// instead — for a terminal font that isn't Nerd-Fonts-patched. On by
    /// default: the app has always shipped with icons on, so this only
    /// changes anything for someone who explicitly opts out.
    #[serde(default = "default_true")]
    pub icons: bool,
    #[serde(flatten)]
    pub overrides: ThemeOverrides,
}

fn default_theme_name() -> String {
    "gruvbox-dark".into()
}

/// One `Option<String>` per `Theme` color slot — every field is optional
/// (and, critically, *absent-tolerant*: serde's derive already treats a
/// missing `Option<T>` field as `None` with no `#[serde(default)]` needed,
/// which is exactly why an existing `config.toml` with only some of these
/// keys — or none at all — keeps parsing fine as more get added here), so a
/// user can override as few or as many slots as they want without needing
/// to specify the rest. `shiki theme create` (`shiki-cli`) scaffolds every
/// field at once from a real palette instead of leaving them to be found
/// and typed by hand one at a time.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThemeOverrides {
    pub bg: Option<String>,
    pub fg: Option<String>,
    pub accent: Option<String>,
    pub selection: Option<String>,
    pub border: Option<String>,
    pub statusbar: Option<String>,
    pub highlight: Option<String>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub success: Option<String>,
    pub inactive: Option<String>,
    pub scrollbar: Option<String>,
    pub tab_active: Option<String>,
    pub tab_inactive: Option<String>,
    pub panel_title: Option<String>,
    pub cursor: Option<String>,
    pub link: Option<String>,
    pub tag: Option<String>,
    pub muted: Option<String>,
}

impl ThemeOverrides {
    /// Every field set from `theme`'s actual values — used by `shiki theme
    /// create` to scaffold a full, ready-to-edit set of overrides instead
    /// of blank ones.
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            bg: Some(theme.bg.clone()),
            fg: Some(theme.fg.clone()),
            accent: Some(theme.accent.clone()),
            selection: Some(theme.selection.clone()),
            border: Some(theme.border.clone()),
            statusbar: Some(theme.statusbar.clone()),
            highlight: Some(theme.highlight.clone()),
            error: Some(theme.error.clone()),
            warning: Some(theme.warning.clone()),
            success: Some(theme.success.clone()),
            inactive: Some(theme.inactive.clone()),
            scrollbar: Some(theme.scrollbar.clone()),
            tab_active: Some(theme.tab_active.clone()),
            tab_inactive: Some(theme.tab_inactive.clone()),
            panel_title: Some(theme.panel_title.clone()),
            cursor: Some(theme.cursor.clone()),
            link: Some(theme.link.clone()),
            tag: Some(theme.tag.clone()),
            muted: Some(theme.muted.clone()),
        }
    }

    /// How many of the 19 slots are actually overridden — shown by the
    /// Settings screen's THEME tab (informational), which displays this count
    /// rather than dumping all 19 (`shiki theme create`'s job, and already
    /// fully visible by just opening `config.toml`) so the tab stays short.
    pub fn set_count(&self) -> usize {
        [
            &self.bg,
            &self.fg,
            &self.accent,
            &self.selection,
            &self.border,
            &self.statusbar,
            &self.highlight,
            &self.error,
            &self.warning,
            &self.success,
            &self.inactive,
            &self.scrollbar,
            &self.tab_active,
            &self.tab_inactive,
            &self.panel_title,
            &self.cursor,
            &self.link,
            &self.tag,
            &self.muted,
        ]
        .iter()
        .filter(|v| v.is_some())
        .count()
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: default_theme_name(),
            icons: default_true(),
            overrides: ThemeOverrides::default(),
        }
    }
}

impl ThemeConfig {
    /// Resolves the built-in theme by name and applies the configured overrides.
    pub fn resolve(&self) -> Theme {
        let mut theme = crate::themes::by_name(&self.name).unwrap_or_else(Theme::terminal_default);
        if let Some(v) = &self.overrides.bg {
            theme.bg = v.clone();
        }
        if let Some(v) = &self.overrides.fg {
            theme.fg = v.clone();
        }
        if let Some(v) = &self.overrides.accent {
            theme.accent = v.clone();
        }
        if let Some(v) = &self.overrides.selection {
            theme.selection = v.clone();
        }
        if let Some(v) = &self.overrides.border {
            theme.border = v.clone();
        }
        if let Some(v) = &self.overrides.statusbar {
            theme.statusbar = v.clone();
        }
        if let Some(v) = &self.overrides.highlight {
            theme.highlight = v.clone();
        }
        if let Some(v) = &self.overrides.error {
            theme.error = v.clone();
        }
        if let Some(v) = &self.overrides.warning {
            theme.warning = v.clone();
        }
        if let Some(v) = &self.overrides.success {
            theme.success = v.clone();
        }
        if let Some(v) = &self.overrides.inactive {
            theme.inactive = v.clone();
        }
        if let Some(v) = &self.overrides.scrollbar {
            theme.scrollbar = v.clone();
        }
        if let Some(v) = &self.overrides.tab_active {
            theme.tab_active = v.clone();
        }
        if let Some(v) = &self.overrides.tab_inactive {
            theme.tab_inactive = v.clone();
        }
        if let Some(v) = &self.overrides.panel_title {
            theme.panel_title = v.clone();
        }
        if let Some(v) = &self.overrides.cursor {
            theme.cursor = v.clone();
        }
        if let Some(v) = &self.overrides.link {
            theme.link = v.clone();
        }
        if let Some(v) = &self.overrides.tag {
            theme.tag = v.clone();
        }
        if let Some(v) = &self.overrides.muted {
            theme.muted = v.clone();
        }
        theme
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    #[serde(default = "default_true")]
    pub auto_commit: bool,
    #[serde(default)]
    pub auto_push: bool,
    #[serde(default = "default_commit_prefix")]
    pub commit_prefix: String,
    #[serde(default = "default_remote")]
    pub remote: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default)]
    pub sign_commits: bool,
    /// When on, a notebook syncs itself (commit, + push if `auto_push`)
    /// automatically after `auto_sync_every` note changes, instead of only
    /// on manual `s`. Off by default — this is opt-in per the global
    /// default, and further overridable per notebook via `[notebooks.<name>]`.
    #[serde(default)]
    pub auto_sync: bool,
    /// How many note changes (new/edited/renamed/deleted/moved) trigger an
    /// automatic sync when `auto_sync` is on.
    #[serde(default = "default_auto_sync_every")]
    pub auto_sync_every: u32,
    /// Template for the remote URL to auto-configure (`git::set_remote`)
    /// when a notebook is created with a plain name — `{notebook}` is
    /// replaced with the new notebook's name, e.g.
    /// `"git@git.example.com:notes/{notebook}.git"` or a local bare-repo
    /// path template. Empty (the default) means don't auto-configure
    /// anything — the remote still has to already exist on that server;
    /// this doesn't create one via any hosting provider's API. Doesn't
    /// apply when the typed name is itself a git URL
    /// (`create_notebook_from_url` already sets its own remote from that).
    /// Field-level default so an existing `[git]` table written before this
    /// key existed still deserializes.
    #[serde(default)]
    pub remote_template: String,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            auto_commit: default_true(),
            auto_push: false,
            commit_prefix: default_commit_prefix(),
            remote: default_remote(),
            branch: default_branch(),
            sign_commits: false,
            auto_sync: false,
            auto_sync_every: default_auto_sync_every(),
            remote_template: String::new(),
        }
    }
}

fn default_auto_sync_every() -> u32 {
    5
}

fn default_true() -> bool {
    true
}

fn default_commit_prefix() -> String {
    "shiki: ".into()
}

fn default_remote() -> String {
    "origin".into()
}

fn default_branch() -> String {
    "main".into()
}

/// Settings for the inline note editor's mouse/keyboard UX (`Mode::Edit`,
/// `shiki-tui/src/editor.rs`) — every field here gates one independently
/// toggleable behavior, off by default unless it's purely additive and safe
/// (`mouse_selection`, `find_replace`), so nothing about how the editor
/// behaves today changes for anyone who doesn't visit the EDITOR Settings
/// tab (leader+`s`) and turn something on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    /// Click-to-position, click-and-drag to select, double-click to select
    /// a word, triple-click to select a line — all inside the editor itself
    /// (distinct from `General::mouse_drag_selection`, which is PREVIEW's
    /// read-only line-selection). Defaults to `true`: purely additive, no
    /// existing keyboard behavior changes.
    #[serde(default = "default_true")]
    pub mouse_selection: bool,
    /// Ctrl+F opens a find/replace bar inside the editor. Defaults to
    /// `true`: purely additive, doesn't touch any existing binding.
    #[serde(default = "default_true")]
    pub find_replace: bool,
    /// When true, Ctrl+C/X/V inside the editor use the real OS clipboard
    /// (falling back automatically to the existing OSC 52 mechanism when
    /// the OS clipboard can't be reached, e.g. a headless SSH session with
    /// no `$DISPLAY`/`$WAYLAND_DISPLAY`) instead of ratatui-textarea's internal
    /// yank register. Off by default since it's an environment-dependent
    /// behavior change, not a pure addition.
    #[serde(default)]
    pub os_clipboard: bool,
    /// When true, Ctrl+A selects the whole buffer instead of ratatui-textarea's
    /// default Emacs-style "move to start of line". Off by default — this
    /// changes existing muscle-memory behavior rather than adding to it.
    #[serde(default)]
    pub select_all_ctrl_a: bool,
    /// Shows a line-number gutter in the editor. Off by default (cosmetic
    /// opt-in).
    #[serde(default)]
    pub line_numbers: bool,
    /// Enables Alt+Click (add a cursor) and Ctrl+D (add the next occurrence
    /// of the current word/selection) for multi-cursor editing. Off by
    /// default: it's the most involved of these behaviors, built as a replay
    /// layer on top of ratatui-textarea's single-cursor model rather than
    /// something the library supports natively.
    #[serde(default)]
    pub multi_cursor: bool,
    /// Enter on a `- item`/`- [ ] task`/`1. item` line continues the same
    /// prefix on the next line (checkboxes always reset to `[ ]`, never copy
    /// `[x]`); Enter on an *empty* list item exits the list instead of
    /// continuing it, and Backspace right after an empty prefix removes it
    /// in one step. Defaults to `true`: purely additive list-editing
    /// convenience, doesn't touch any existing binding.
    #[serde(default = "default_true")]
    pub auto_list_continue: bool,
    /// Ctrl+B wraps the selection in `**bold**` (or inserts an empty pair
    /// with the cursor in the middle if nothing's selected); Ctrl+Alt+I does
    /// the same for `_italic_` (not Ctrl+I — that collides with Tab at the
    /// terminal level in most emulators). Defaults to `true`: purely
    /// additive, doesn't touch any existing binding.
    #[serde(default = "default_true")]
    pub format_shortcuts: bool,
    /// Typing `(`, `` ` ``, or `"` wraps the current selection in the
    /// matching pair, or inserts an empty pair with the cursor in the
    /// middle if nothing's selected. Deliberately excludes `[` so it can't
    /// interfere with `[[wikilink]]` autocomplete, which depends on the
    /// user typing two real `[` characters in a row. Defaults to `true`.
    #[serde(default = "default_true")]
    pub auto_pair_brackets: bool,
    /// Pasting a bare URL over an active selection wraps it as
    /// `[selected text](url)` instead of replacing the selection with the
    /// raw URL. Defaults to `true`: only triggers when the pasted text is
    /// itself a URL, otherwise paste behaves exactly as before.
    #[serde(default = "default_true")]
    pub paste_url_as_link: bool,
    /// Tab, when the text immediately before the cursor matches a
    /// configured snippet trigger, replaces that trigger text with the
    /// snippet's body instead of inserting a literal tab. Falls through to
    /// normal Tab behavior when there's no match. Defaults to `true`.
    #[serde(default = "default_true")]
    pub snippet_expand_tab: bool,
    /// Keeps the cursor's line vertically centered in the editor viewport
    /// while typing, instead of only scrolling once the cursor reaches the
    /// edge. Off by default: it's a visible change to how scrolling feels,
    /// not a pure addition, so it needs an explicit opt-in.
    #[serde(default)]
    pub typewriter_scroll: bool,
    /// Alt+Up/Alt+Down moves the current line (or selected block) up/down
    /// past its neighbor. Defaults to `true`: purely additive, doesn't touch
    /// any existing binding.
    #[serde(default = "default_true")]
    pub move_line: bool,
    /// Alt+D duplicates the current line (or selected block) directly below
    /// itself. Defaults to `true`: purely additive, doesn't touch any
    /// existing binding.
    #[serde(default = "default_true")]
    pub duplicate_line: bool,
    /// Tab/Shift+Tab indent/outdent every line touched by an active
    /// selection, instead of Tab inserting a literal tab and Shift+Tab doing
    /// nothing. Defaults to `true`: purely additive, only changes behavior
    /// when there's a selection to act on.
    #[serde(default = "default_true")]
    pub block_indent_select: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            mouse_selection: true,
            find_replace: true,
            os_clipboard: false,
            select_all_ctrl_a: false,
            line_numbers: false,
            multi_cursor: false,
            auto_list_continue: true,
            format_shortcuts: true,
            auto_pair_brackets: true,
            paste_url_as_link: true,
            snippet_expand_tab: true,
            typewriter_scroll: false,
            move_line: true,
            duplicate_line: true,
            block_indent_select: true,
        }
    }
}

/// PDF export (`shiki publish`, leader+`P`) settings — a concrete-default
/// table like `GitConfig`, not `NotebookGitOverride`'s all-`Option` "unset
/// means inherit" shape: there's one real global default theme, nothing to
/// inherit from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    /// One of go-pretty-pdf's 17 built-in theme names (`default`, `minimal`,
    /// `modern`, `classic`, `corporate`, `dark`, `academic`, `editorial`,
    /// `sepia`, `terminal`, `blueprint`, `ivy`, `government`, `resume`,
    /// `legal`, `latex`, `gruvbox`) — not validated here, same as
    /// `theme.name` isn't validated against the theme registry either; an
    /// unrecognized name only ever surfaces as a `pretty-pdf` error at
    /// publish time.
    #[serde(default = "default_pdf_theme")]
    pub pdf_theme: String,
    /// Directory PDFs are saved into, e.g. `{data_dir}/exports/{notebook}.pdf`
    /// on Linux. Empty (the default) means "use the app's own data dir" —
    /// see `App::resolved_export_dir` (shiki-tui) / the equivalent inline
    /// resolution in `shiki publish` (shiki-cli). Set this to point exports
    /// somewhere else instead (a synced folder, Desktop, etc).
    #[serde(default)]
    pub export_dir: String,
    /// When true, publishing always opens a prompt asking exactly where to
    /// save the PDF (prefilled with the resolved default path), instead of
    /// silently writing to `export_dir`/the default location every time.
    /// Off by default: this changes existing behavior (an extra prompt on
    /// every publish) rather than being a pure addition.
    #[serde(default)]
    pub ask_export_path: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            pdf_theme: default_pdf_theme(),
            export_dir: String::new(),
            ask_export_path: false,
        }
    }
}

fn default_pdf_theme() -> String {
    "default".into()
}

/// Per-notebook override of the `[git]` sync policy — a notebook connected
/// to a private work repo might want `auto_push`, while a scratch notebook
/// with no remote at all shouldn't be forced into the same policy. Any
/// field left unset here falls back to the global `[git]` default (see
/// `Config::sync_for`), so most notebooks need no `[notebooks.<name>]`
/// table at all.
///
/// A `path` field can also be set to point this notebook at an arbitrary
/// directory on disk (e.g. an Obsidian vault subfolder) instead of the
/// default location under the data directory.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotebookGitOverride {
    pub auto_push: Option<bool>,
    pub auto_sync: Option<bool>,
    pub auto_sync_every: Option<u32>,
    /// Optional absolute path override for this notebook's directory.
    /// When set, the notebook lives at this path instead of under the
    /// data directory — useful for linking existing directories (e.g.
    /// Obsidian vault subfolders) as independent notebooks. Must be
    /// absolute — see `Config::notebook_custom_paths` for why a relative
    /// value is ignored rather than honored.
    #[serde(default)]
    pub path: Option<String>,
    /// Set when "delete notebook" was answered with "just remove the
    /// reference" instead of "delete the files" — the directory on disk is
    /// left completely untouched, this only tells `App::reload_notebooks`
    /// to stop listing it as a tracked notebook. Reversible in-app: the
    /// Settings → NOTEBOOKS tab lists hidden notebooks (marked `(hidden)`)
    /// and drilling into one lets you clear this flag to restore it.
    #[serde(default)]
    pub hidden: bool,
    /// Whether this notebook's notes are encrypted at rest (`age::scrypt`,
    /// passphrase-based — see `shiki_core::crypto`). Unlike `auto_push`/
    /// `auto_sync`, there's no global default to inherit from: encryption
    /// is opt-in per notebook, off unless explicitly set. The passphrase
    /// itself is never stored here (or anywhere on disk) — it's typed in
    /// each session (TUI) or each invocation (`shiki notebook encrypt/
    /// decrypt`).
    #[serde(default)]
    pub encrypt: bool,
}

/// `[git]` settings resolved for one specific notebook — see `Config::sync_for`.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedSync {
    pub auto_push: bool,
    pub auto_sync: bool,
    pub auto_sync_every: u32,
}

/// One entry in the inline editor's `/`-menu, defined under
/// `[snippets.<trigger>]` in `config.toml` — e.g.:
///
/// ```toml
/// [snippets.note]
/// label = "Callout note"
/// body = "> **Note:** {{cursor}}"
/// ```
///
/// Keyed by trigger (a `HashMap`, like `[notebooks.<name>]` above) rather
/// than an array of `{ trigger, label, body }` tables — deliberately not
/// `Vec<SnippetConfig>` with `[[snippets]]`: `Config::save` always writes
/// every field including empty ones, and an empty `Vec` serializes as a
/// bare `snippets = []` line, which conflicts as a "duplicate key" with
/// any `[[snippets]]` block a user later appends by hand (hit for real
/// while dogfooding this — a fresh install's already-written `config.toml`
/// has `snippets = []` from the very first launch, before anyone's added
/// one). An empty `HashMap` instead serializes as a bare `[snippets]`
/// table header, and TOML lets `[snippets.note]` extend that table
/// afterward with no conflict — the same reason `[notebooks.work]` already
/// composes cleanly with the auto-written `[notebooks]`.
///
/// The map key is what filters the `/`-menu (and, if it matches a built-in
/// command's trigger case-insensitively, *replaces* that built-in instead
/// of adding a duplicate — every command in the menu is customizable this
/// way, not just ones added from scratch). `label` falls back to the
/// trigger when omitted. `body` supports the same `{{title}}`/`{{date}}`
/// placeholders note templates do (`shiki_core::Template::render`), plus a
/// `{{cursor}}` marker (not a template placeholder — resolved separately)
/// marking where the cursor lands after insertion; omit it to leave the
/// cursor at the end of the inserted text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetConfig {
    #[serde(default)]
    pub label: Option<String>,
    /// A missing `body` (a snippet entry with just a `label`, or a typo'd
    /// key) falls back to an empty string — an inert, do-nothing snippet —
    /// rather than failing the whole config over one broken entry.
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub keybindings: Keybindings,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub export: ExportConfig,
    /// Per-notebook overrides of `[git]`, keyed by notebook name — e.g.
    /// `[notebooks.work]` with `auto_push = true`. See `NotebookGitOverride`.
    #[serde(default)]
    pub notebooks: std::collections::HashMap<String, NotebookGitOverride>,
    /// User-defined `/`-menu commands for the inline editor, keyed by
    /// trigger — see `SnippetConfig`. Empty by default; the built-in
    /// commands (`shiki-tui`'s `slash_menu::builtins`) aren't stored here
    /// at all, only ever the user's own additions/overrides.
    #[serde(default)]
    pub snippets: std::collections::HashMap<String, SnippetConfig>,
    /// Named, saved `shiki query`/leader+`q` DSL strings, keyed by name —
    /// e.g. `[queries] due-soon = "where due < today sort due asc"`. Same
    /// `HashMap` (not `Vec`) shape as `snippets` above, and for the exact
    /// same reason: an empty `Vec<_>` would serialize as a bare `queries =
    /// []` line that a later hand-added `[queries.foo]`-style table would
    /// conflict with. Ad-hoc (unsaved) queries typed directly into the CLI
    /// arg don't touch this map — the TUI's query modal does, though:
    /// `Ctrl+S` on a valid query prompts for a name and writes it here
    /// (`App::confirm_save_query`), and `Ctrl+D` on a highlighted saved
    /// suggestion removes it, so this file is genuinely round-trippable
    /// from either the TUI or by hand.
    #[serde(default)]
    pub queries: std::collections::HashMap<String, String>,
}

impl Config {
    /// Resolves the sync policy for `notebook_name`: its `[notebooks.<name>]`
    /// entry (if any) layered on top of the global `[git]` defaults.
    pub fn sync_for(&self, notebook_name: &str) -> ResolvedSync {
        let over = self.notebooks.get(notebook_name);
        ResolvedSync {
            auto_push: over.and_then(|o| o.auto_push).unwrap_or(self.git.auto_push),
            auto_sync: over.and_then(|o| o.auto_sync).unwrap_or(self.git.auto_sync),
            auto_sync_every: over
                .and_then(|o| o.auto_sync_every)
                .unwrap_or(self.git.auto_sync_every),
        }
    }
    /// Whether `notebook_name` is configured to encrypt its notes at rest.
    /// No global default to inherit from (unlike `sync_for`'s fields) —
    /// encryption is opt-in per notebook, so absent simply means off.
    pub fn encrypt_for(&self, notebook_name: &str) -> bool {
        self.notebooks.get(notebook_name).is_some_and(|o| o.encrypt)
    }
    /// Returns a map of notebook names to their custom absolute paths,
    /// as configured in `[notebooks.<name>] path = "..."`.
    /// Notebooks without a custom path are not included — they use the
    /// default location under the data directory instead.
    ///
    /// A configured `path` that isn't absolute is also excluded (falling
    /// back to the default location) rather than honored as-is: a relative
    /// path here would resolve against whatever directory the `shiki`
    /// process happens to be launched from, which is different depending on
    /// whether it's started from a terminal, a desktop entry, or a cron job
    /// — not the stable, predictable location this feature is meant to
    /// provide. `shiki doctor` separately flags any notebook with a
    /// non-absolute `path` so this doesn't fail silently.
    pub fn notebook_custom_paths(&self) -> std::collections::HashMap<String, std::path::PathBuf> {
        self.notebooks
            .iter()
            .filter_map(|(name, overrides)| {
                let path = std::path::PathBuf::from(overrides.path.as_ref()?);
                path.is_absolute().then(|| (name.clone(), path))
            })
            .collect()
    }
}

impl Config {
    /// Default path: `~/.config/shiki/config.toml` on Linux, or
    /// `~/Library/Application Support/shiki/config.toml` on macOS — but always
    /// `$XDG_CONFIG_HOME/shiki/config.toml` when that env var is set.
    ///
    /// `directories::ProjectDirs` only honors `$XDG_*` on Linux; on macOS/Windows
    /// it ignores them and uses the platform convention. Contributors (and anyone
    /// isolating a test install) set those vars on every OS, so we check them
    /// ourselves first and only fall back to `ProjectDirs` when they're unset.
    pub fn default_path() -> Result<PathBuf> {
        Ok(config_dir()?.join("config.toml"))
    }

    pub fn default_data_dir() -> Result<PathBuf> {
        data_dir()
    }

    pub fn default_templates_dir() -> Result<PathBuf> {
        Ok(Self::default_path()?
            .parent()
            .expect("config path always has a parent")
            .join("templates"))
    }

    /// Where the persistent status/log history (`App::log_history`) is
    /// appended to — the config dir, not the data dir, deliberately: the
    /// data dir's top level *is* the set of notebooks (each one a plain,
    /// user-named directory — see the filesystem layout in `IDEA.md`), so
    /// any fixed filename placed there could collide with a notebook
    /// someone names the same thing. The config dir has no such risk, only
    /// shiki's own fixed files.
    pub fn default_log_path() -> Result<PathBuf> {
        Ok(Self::default_path()?
            .parent()
            .expect("config path always has a parent")
            .join("shiki.log"))
    }

    /// Where deleted notes/folders are moved instead of being permanently
    /// removed (see `shiki_core::trash`) — the config dir, for the exact
    /// same collision reason as `default_log_path`: the data dir's top
    /// level is user-named notebooks, so a fixed directory name placed
    /// there could collide with one.
    pub fn default_trash_dir() -> Result<PathBuf> {
        Ok(Self::default_path()?
            .parent()
            .expect("config path always has a parent")
            .join("trash"))
    }

    /// Where `general.remember_last_session`'s state (`SessionState`) is
    /// persisted — the config dir, for the exact same collision reason as
    /// `default_log_path`/`default_trash_dir`: the data dir's top level is
    /// user-named notebooks, so a fixed filename placed there could collide
    /// with one.
    pub fn default_session_path() -> Result<PathBuf> {
        Ok(Self::default_path()?
            .parent()
            .expect("config path always has a parent")
            .join("session.toml"))
    }

    /// Where the capture daemon (`general.enable_capture_daemon`) records
    /// which ephemeral TCP port it bound — the config dir, for the exact
    /// same collision reason as `default_log_path`/`default_trash_dir`/
    /// `default_session_path`: the data dir's top level is user-named
    /// notebooks, so a fixed filename placed there could collide with one.
    /// Rewritten on every daemon start, so a dead process never leaves a
    /// port number here that anything would mistakenly try to connect to.
    pub fn default_capture_port_path() -> Result<PathBuf> {
        Ok(Self::default_path()?
            .parent()
            .expect("config path always has a parent")
            .join("capture.port"))
    }

    /// Where `shiki capture --undo`'s backing record (`LastCapture`) lives —
    /// same collision reasoning as every other fixed file in this list.
    /// Shared between the in-TUI daemon and the CLI's standalone fallback,
    /// so `--undo` works the same regardless of which path performed the
    /// capture being undone.
    pub fn default_last_capture_path() -> Result<PathBuf> {
        Ok(Self::default_path()?
            .parent()
            .expect("config path always has a parent")
            .join("last-capture.toml"))
    }

    /// Loads the config from `path`, or creates and saves a default config if it doesn't exist.
    pub fn load_or_init(path: &Path) -> Result<Self> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            Self::parse(&contents)
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let contents = commented_default_toml();
            std::fs::write(path, &contents)?;
            Self::parse(&contents)
        }
    }

    /// Parses a config from its TOML text — split out from `load_or_init` so
    /// callers that already have the file's contents (e.g. `shiki doctor`,
    /// diagnosing a broken config without going through the normal
    /// load-or-create startup path) don't need a direct `toml` dependency.
    pub fn parse(contents: &str) -> Result<Self> {
        Ok(toml::from_str(contents)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

/// The very first `config.toml` a fresh install ever gets, fully commented —
/// unlike every subsequent `save()` (theme picker, Settings screen, `shiki
/// theme set`, …), which stays plain `toml::to_string_pretty` with no
/// comments, since preserving hand-written comments through an edit/re-save
/// round-trip would need `toml_edit` instead of plain serde, a bigger change
/// not worth it just for this.
///
/// Deliberately *not* a hand-typed string constant with every `key = value`
/// duplicated from the various `Default` impls above: keeping ~50 literal
/// values in sync by hand across every `Default` impl in this file (and
/// getting even one wrong with no compiler check to catch it) is a real,
/// silent-drift risk for something that's supposed to be the "this is what
/// you actually get" reference. Instead, `toml::to_string_pretty(&Config::default())`
/// (the exact same call `save()` already makes) produces the real values,
/// and `section_comment` only ever prepends *prose* above specific known
/// section headers — the worst a comment can do is go mildly stale, never
/// silently ship a wrong default.
fn commented_default_toml() -> String {
    let plain =
        toml::to_string_pretty(&Config::default()).expect("Config::default() always serializes");
    let mut out = String::with_capacity(plain.len() + 2048);
    for line in plain.lines() {
        if let Some(comment) = section_comment(line) {
            out.push_str(comment);
            out.push('\n');
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn section_comment(line: &str) -> Option<&'static str> {
    Some(match line {
        "[general]" => {
            "\
# shiki configuration — every key below has a default, so nothing here is
# required. Full reference (every option, every keybinding, note format):
# https://sazardev.github.io/shiki/documentation.html
# Press `?` in the app any time for a live, searchable keybinding reference,
# and leader+`s` (Settings) for a summary of everything in this file.
#
# General app behavior.
# - default_notebook: used when a command doesn't specify -n <notebook>.
# - editor: the external editor for `E` — defaults to $EDITOR, then \"nvim\".
# - daily_template: which template (by filename, without .md) daily notes use.
# - use_favorite_editor: when true, `i` opens the OS's detected favorite/
#   default editor instead of the built-in one, same as `E` but auto-resolved.
# - mouse_drag_selection: when true, a mouse click over a note's body in
#   PREVIEW jumps straight into edit mode with the cursor on the clicked
#   line (a mouse-only alternative to `i`/vim motions); click-and-drag
#   instead selects text and copies it to the clipboard on release.
# - data_dir: optional path override for the notebooks directory. Point this
#   at an existing Obsidian vault or any markdown folder to use it as the
#   notebooks root. Unset defaults to the platform data directory.
# - show_hints: when true, text-input prompts that have one (e.g. new
#   notebook) show a small hint line explaining non-obvious input.
# - remember_last_session: when true, quitting saves exactly where you were
#   (notebook, folder, selected note/folder, focused panel) and the next
#   launch restores it instead of starting at the first notebook's root.
# - show_coffee_link: shows the Buy Me a Coffee segment in the footer.
#   Defaults to true.
# - skip_delete_confirm: when true, deleting a note/folder skips the
#   confirm dialog (still restorable via leader+`u`). Doesn't apply to
#   notebook delete, which asks a real delete-vs-untrack question.
# - show_dates: shows each note's date next to its title in the NOTES list.
# - wikilink_autocomplete: typing `[[` opens the note-picker menu. Defaults
#   to true.
# - daily_agenda: a new daily note gets a \"## Due today\" section listing
#   pending tasks across every notebook. Defaults to true.
# - compact_footer: hides char/word count, reading time, and note-count
#   detail from the footer, leaving just the essentials.
# - status_message_timeout_secs: how long a footer status message stays
#   visible before clearing itself. Defaults to 2.
# - drawer_width: width in columns of the notebook drawer (leader+`b`).
#   Defaults to 30.
# - tasks_show_done_default: whether the tasks view starts showing every
#   task, including done ones, instead of just pending.
# - default_note_sort: \"filename\", \"title\", or \"date\" — which order the
#   NOTES list sorts by on launch. Defaults to \"filename\".
# - log_history_limit: max entries kept in the logs modal and log file.
#   Defaults to 500.
# - trash_retention_days: days a deleted note/folder stays in the trash
#   before being auto-purged at startup. 0 (the default) means never.
# - reading_wpm: words-per-minute for the footer's \"N min read\" estimate.
#   Defaults to 200.
# - page_step: how many rows PageUp/PageDown move at once. Defaults to 10."
        }
        "[keybindings]" => {
            "\
# Keybindings are scoped, not one flat map — [keybindings.global] needs the
# leader key first; [keybindings.notebooks]/[keybindings.notes]/
# [keybindings.preview] only apply while that panel has focus, so the same
# key can mean something different in each one. Press `?` in the app for a
# live, searchable reference of every binding that's actually active —
# it's generated from this exact config, so it can't drift from it."
        }
        "[theme]" => {
            "\
# `name` picks a built-in theme (`shiki theme list` shows every name). Every
# one of a theme's 19 color slots can be overridden individually under
# [theme.overrides] — accent, bg, fg, selection, border, statusbar,
# highlight, error, warning, success, inactive, scrollbar, tab_active,
# tab_inactive, panel_title, cursor, link, tag, muted. Anything left unset
# falls back to `name`'s own value for that slot. `shiki theme create
# [--from <theme>]` scaffolds all 19 here at once from a real palette,
# instead of hand-typing hex codes with no example to copy from.
# - icons: when false, every Nerd Font glyph falls back to plain text —
#   for a terminal font that isn't Nerd-Fonts-patched. Defaults to true."
        }
        "[git]" => {
            "\
# Global git sync policy — auto_push/auto_sync/auto_sync_every can all be
# overridden per notebook under [notebooks.<name>] below.
# - auto_commit: commit pending changes automatically (still needs `s`/`u`,
#   or auto_sync, to actually push them anywhere).
# - remote_template: auto-configures a *new* (non-URL) notebook's remote,
#   e.g. \"git@git.example.com:notes/{notebook}.git\" — \"{notebook}\" is
#   replaced with the new notebook's name. Empty (the default) means don't
#   auto-configure anything; the remote still has to already exist."
        }
        "[editor]" => {
            "\
# Inline note editor UX (Mode::Edit) — each key defaults independently; the
# mostly-additive conveniences (mouse_selection, find_replace, auto_list_
# continue, format_shortcuts, auto_pair_brackets, paste_url_as_link,
# snippet_expand_tab, move_line, duplicate_line, block_indent_select) are on
# by default, while the ones that change existing behavior (os_clipboard,
# select_all_ctrl_a, line_numbers, multi_cursor, typewriter_scroll) are off
# until you opt in from the EDITOR tab in Settings (leader+`s`).
# - mouse_selection: click to position the cursor, drag to select,
#   double-click for a word, triple-click for a line. Defaults to true.
# - find_replace: Ctrl+F opens a find/replace bar. Defaults to true.
# - os_clipboard: Ctrl+C/X/V use the real OS clipboard, falling back to the
#   existing OSC 52 mechanism when there's no display server (e.g. SSH).
# - select_all_ctrl_a: Ctrl+A selects everything instead of moving to the
#   start of the line.
# - line_numbers: shows a line-number gutter.
# - multi_cursor: Alt+Click adds a cursor, Ctrl+D adds the next occurrence
#   of the current word/selection.
# - auto_list_continue: Enter on a list/checkbox line continues it on the
#   next line; Enter on an empty item exits the list; Backspace removes an
#   empty item's prefix in one step. Defaults to true.
# - format_shortcuts: Ctrl+B / Ctrl+Alt+I wrap the selection in bold/italic.
#   Defaults to true.
# - auto_pair_brackets: typing ( ` \" wraps the selection or inserts an
#   empty pair. Defaults to true.
# - paste_url_as_link: pasting a bare URL over a selection wraps it as a
#   markdown link. Defaults to true.
# - snippet_expand_tab: Tab expands a matching snippet trigger instead of
#   inserting a literal tab. Defaults to true.
# - typewriter_scroll: keeps the cursor's line vertically centered while
#   typing. Defaults to false.
# - move_line: Alt+Up/Alt+Down move the current line past its neighbor.
#   Defaults to true.
# - duplicate_line: Alt+D duplicates the current line. Defaults to true.
# - block_indent_select: Tab/Shift+Tab with a selection indent/outdent every
#   line it spans. Defaults to true."
        }
        "[export]" => {
            "\
# PDF export (`shiki publish`, leader+`P`) — pdf_theme picks one of
# go-pretty-pdf's 17 built-in themes: default, minimal, modern, classic,
# corporate, dark, academic, editorial, sepia, terminal, blueprint, ivy,
# government, resume, legal, latex, gruvbox. The `pretty-pdf` binary this
# feature shells out to is fetched automatically on first use if it isn't
# already on $PATH — see `shiki doctor`.
# - export_dir: directory PDFs are saved into, e.g. \"{data_dir}/exports\".
#   Empty (the default) means the app's own data dir.
# - ask_export_path: when true, publishing always prompts for the exact
#   save path instead of silently writing to export_dir. Defaults to false."
        }
        "[notebooks]" => {
            "\
# Optional per-notebook overrides of [git] above, e.g.:
# [notebooks.work]
# auto_sync = true
# auto_sync_every = 3
# auto_push = true
#
# Each notebook can have its own path (an existing directory on disk)
# instead of living under the default data directory:
# [notebooks.obsidian]
# path = \"/Users/me/Documents/Obsidian/Work\"
# auto_sync = true"
        }
        "[snippets]" => {
            "\
# Custom entries for the inline editor's `/`-menu, keyed by trigger, e.g.:
# [snippets.callout]
# label = \"Info callout\"
# body = \"> **Info:** {{cursor}}\"
# A trigger matching a built-in (h1/h2/h3/code/math/table/check/quote/
# divider/date/tags/frontmatter/bullet/numbered/link/image/note/warning/
# details) replaces it instead of adding a duplicate."
        }
        "[queries]" => {
            "\
# Named, saved `shiki query` DSL strings, keyed by name, e.g.:
# due-soon = \"where due < today sort due asc\"
# Run one with `shiki query --saved <name>`; same Dataview-style language
# as the leader+`q` modal and the literal CLI arg."
        }
        _ => return None,
    })
}

/// Non-empty `$XDG_CONFIG_HOME` / `$XDG_DATA_HOME`, if set. Empty values are
/// treated as unset — same spirit as the XDG Base Directory spec's "if unset
/// or empty" wording for the defaults.
fn xdg_home(var: &str) -> Option<PathBuf> {
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    }
}

fn project_dirs() -> Result<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "shiki").ok_or(Error::NoConfigDir)
}

/// `$XDG_*/shiki` when an override is present, otherwise the `ProjectDirs`
/// fallback. Pure so the precedence rule is unit-testable without touching
/// process env (which races under `cargo test`'s parallel runner).
fn resolve_dir(xdg: Option<PathBuf>, fallback: PathBuf) -> PathBuf {
    match xdg {
        Some(base) => base.join("shiki"),
        None => fallback,
    }
}

fn config_dir() -> Result<PathBuf> {
    Ok(resolve_dir(
        xdg_home("XDG_CONFIG_HOME"),
        project_dirs()?.config_dir().to_path_buf(),
    ))
}

fn data_dir() -> Result<PathBuf> {
    Ok(resolve_dir(
        xdg_home("XDG_DATA_HOME"),
        project_dirs()?.data_dir().to_path_buf(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `default_path`/`default_data_dir` read process env; serialize those
    /// tests so parallel workers don't clobber each other's overrides.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_dir_prefers_xdg_override_over_platform_fallback() {
        assert_eq!(
            resolve_dir(
                Some(PathBuf::from("/tmp/shiki-test-config")),
                PathBuf::from("/fallback/config")
            ),
            PathBuf::from("/tmp/shiki-test-config/shiki")
        );
        assert_eq!(
            resolve_dir(None, PathBuf::from("/fallback/data")),
            PathBuf::from("/fallback/data")
        );
    }

    #[test]
    fn default_path_and_data_dir_honor_xdg_env_vars() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let prev_config = std::env::var_os("XDG_CONFIG_HOME");
        let prev_data = std::env::var_os("XDG_DATA_HOME");
        // SAFETY: held under ENV_LOCK; no other test in this module mutates
        // these vars concurrently, and we restore them before releasing.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", "/tmp/shiki-test-config");
            std::env::set_var("XDG_DATA_HOME", "/tmp/shiki-test-data");
        }

        let path = Config::default_path().expect("config path");
        let data = Config::default_data_dir().expect("data dir");

        match prev_config {
            Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
        match prev_data {
            Some(v) => unsafe { std::env::set_var("XDG_DATA_HOME", v) },
            None => unsafe { std::env::remove_var("XDG_DATA_HOME") },
        }

        assert_eq!(
            path,
            PathBuf::from("/tmp/shiki-test-config/shiki/config.toml")
        );
        assert_eq!(data, PathBuf::from("/tmp/shiki-test-data/shiki"));
    }

    /// Every field in every table must have a `#[serde(default)]` — a
    /// hand-edited `config.toml` missing just one key from an otherwise
    /// complete table (very plausible now that the Settings screen invites
    /// editing this file directly) used to fail the *entire* parse with
    /// "missing field", refusing to start the app over one deleted line.
    /// Exercises one deliberately incomplete table per struct that had
    /// this gap before the fix, each combined with an otherwise-empty file
    /// so every other table is *also* entirely absent.
    #[test]
    fn every_table_parses_with_only_one_field_present() {
        for partial in [
            "[general]\ndefault_notebook = \"x\"\n",
            "[keybindings.global]\ntheme_picker = \"z\"\n",
            "[keybindings.notebooks]\nnew = \"z\"\n",
            "[keybindings.notes]\nnew = \"z\"\n",
            "[keybindings.preview]\nedit_inline = \"z\"\n",
            "[theme]\nname = \"nord\"\n",
            "[git]\nauto_push = true\n",
            "[editor]\nline_numbers = true\n",
            "[export]\npdf_theme = \"dark\"\n",
        ] {
            Config::parse(partial).unwrap_or_else(|e| panic!("{partial:?} failed to parse: {e}"));
        }
    }

    #[test]
    fn completely_empty_file_parses_to_all_defaults() {
        Config::parse("").expect("an empty config.toml must parse to all defaults");
    }

    #[test]
    fn notebook_custom_paths_ignores_relative_paths_but_keeps_absolute_ones() {
        let mut config = Config::default();
        config.notebooks.insert(
            "work".to_string(),
            NotebookGitOverride {
                path: Some("relative/vault/work".to_string()),
                ..Default::default()
            },
        );
        #[cfg(windows)]
        let absolute = "C:\\vaults\\personal";
        #[cfg(not(windows))]
        let absolute = "/vaults/personal";
        config.notebooks.insert(
            "personal".to_string(),
            NotebookGitOverride {
                path: Some(absolute.to_string()),
                ..Default::default()
            },
        );

        let custom_paths = config.notebook_custom_paths();

        assert_eq!(custom_paths.len(), 1, "the relative path must be excluded");
        assert_eq!(
            custom_paths.get("personal"),
            Some(&std::path::PathBuf::from(absolute))
        );
        assert!(!custom_paths.contains_key("work"));
    }

    #[test]
    fn from_theme_round_trips_every_field_through_resolve() {
        let base = crate::themes::by_name("nord").expect("nord is a built-in theme");
        let overrides = ThemeOverrides::from_theme(&base);
        let config = ThemeConfig {
            name: "nord".into(),
            icons: true,
            overrides,
        };
        // Every one of `Theme`'s 19 color fields — including the 14 that
        // used to have no override path at all — must resolve back to
        // exactly the base theme's own value, proving `from_theme` covers
        // all of them and `resolve` applies all of them.
        assert_eq!(config.resolve(), base);
    }

    #[test]
    fn resolve_only_applies_fields_that_are_actually_overridden() {
        let overrides = ThemeOverrides {
            error: Some("#ff0000".into()),
            ..Default::default()
        };
        let config = ThemeConfig {
            name: "nord".into(),
            icons: true,
            overrides,
        };
        let base = crate::themes::by_name("nord").unwrap();
        let resolved = config.resolve();
        assert_eq!(resolved.error, "#ff0000");
        assert_eq!(resolved.fg, base.fg); // untouched field falls back to the base theme
    }

    #[test]
    fn commented_default_toml_parses_back_to_the_real_defaults() {
        // The whole point of generating comments over `Config::default()`'s
        // own serialization (rather than hand-typing every value in a
        // separate template) is that a fresh install's config can never
        // silently diverge from the real defaults — proven here by
        // reserializing the parsed result and comparing it to a plain
        // `Config::default()` dump, since `Config` itself has no
        // `PartialEq` to compare directly.
        let commented = commented_default_toml();
        let parsed = Config::parse(&commented).expect("commented default must parse");
        let plain_default = toml::to_string_pretty(&Config::default()).unwrap();
        let reserialized = toml::to_string_pretty(&parsed).unwrap();
        assert_eq!(reserialized, plain_default);
    }

    #[test]
    fn commented_default_toml_comments_every_known_section() {
        let commented = commented_default_toml();
        let lines: Vec<&str> = commented.lines().collect();
        for section in [
            "[general]",
            "[keybindings]",
            "[theme]",
            "[git]",
            "[editor]",
            "[export]",
        ] {
            let idx = lines
                .iter()
                .position(|l| *l == section)
                .unwrap_or_else(|| panic!("{section} missing from commented default"));
            assert!(
                idx > 0,
                "{section} is the first line, can't have a comment above it"
            );
            assert!(
                lines[idx - 1].starts_with('#'),
                "{section} has no comment line directly above it"
            );
        }
    }
}

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::backend::Backend;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Color;
use ratatui::text::Line;
use ratatui::Terminal;
use shiki_config::{Config, Theme};
use shiki_core::search::SearchHit;
use shiki_core::{Note, Notebook, NotebookStore, SearchEngine, TagIndex};

use crate::editor::InlineEditor;
use crate::input::InputBox;
use crate::keybindings::{Action, KeyMaps};
use crate::render::hex_to_color;
use crate::{confirm, layout, panel_preview};

/// How many rows/lines `PageUp`/`PageDown` move by, across every scrollable
/// list and the PREVIEW scroll — one consistent "big jump" step everywhere
/// instead of matching whatever's currently visible on screen (not knowable
/// from most of this code without threading the render area through).
pub(crate) const PAGE_STEP: isize = 10;

/// How long a status-bar message stays visible before clearing itself —
/// it's always still in `log_history` (leader+`l`) regardless, so nothing
/// is lost by clearing the footer quickly instead of leaving it there until
/// the next action happens to overwrite it.
pub(crate) const STATUS_MESSAGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Edit,
    Visual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Notebooks,
    Notes,
    Preview,
}

impl Focus {
    /// Full cycle, used by `tab`.
    pub(crate) fn next(self) -> Self {
        match self {
            Focus::Notebooks => Focus::Notes,
            Focus::Notes => Focus::Preview,
            Focus::Preview => Focus::Notebooks,
        }
    }

    /// One level deeper (Yazi-style `l` / Right / Enter). Stays at the deepest level.
    fn forward(self) -> Self {
        match self {
            Focus::Notebooks => Focus::Notes,
            Focus::Notes | Focus::Preview => Focus::Preview,
        }
    }

    /// One level back (Yazi-style `h` / Left). Stays at the shallowest level.
    fn backward(self) -> Self {
        match self {
            Focus::Notebooks | Focus::Notes => Focus::Notebooks,
            Focus::Preview => Focus::Notes,
        }
    }

    /// For `general.remember_last_session` — `shiki_config::SessionState`
    /// doesn't know about this enum at all (shiki-config sits below
    /// shiki-tui in the dependency chain), so it's persisted as a plain
    /// lowercase string and translated on both ends here instead.
    pub(crate) fn as_session_str(self) -> &'static str {
        match self {
            Focus::Notebooks => "notebooks",
            Focus::Notes => "notes",
            Focus::Preview => "preview",
        }
    }

    pub(crate) fn from_session_str(s: &str) -> Option<Self> {
        match s {
            "notebooks" => Some(Focus::Notebooks),
            "notes" => Some(Focus::Notes),
            "preview" => Some(Focus::Preview),
            _ => None,
        }
    }
}

/// How the NOTES list is ordered; cycled by `Action::SortNotes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum NoteSort {
    #[default]
    Filename,
    TitleAz,
    DateNewest,
}

impl NoteSort {
    fn next(self) -> Self {
        match self {
            NoteSort::Filename => NoteSort::TitleAz,
            NoteSort::TitleAz => NoteSort::DateNewest,
            NoteSort::DateNewest => NoteSort::Filename,
        }
    }

    fn label(self) -> &'static str {
        match self {
            NoteSort::Filename => "filename",
            NoteSort::TitleAz => "title A-Z",
            NoteSort::DateNewest => "date (newest first)",
        }
    }
}

/// What a text-input popup is currently collecting a value for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingInput {
    NewNote,
    NewNotebook,
    NewFolder,
    RenameNote,
    RenameNotebook,
    Search,
    SetRemote,
    /// Editing a notebook's remote from inside the Settings modal's
    /// NOTEBOOKS section (level 2) rather than the notebooks-panel `R`
    /// binding — which notebook is always `App.settings_notebook_drill`,
    /// not tracked here, so this stays a plain unit variant like the rest.
    SettingsNotebookRemote,
    /// Same idea, for that notebook's `auto_sync_every` override — the
    /// boolean overrides (`auto_push`/`auto_sync`) don't need a text prompt
    /// at all, they cycle in place on `Enter` (see
    /// `App::cycle_notebook_bool_override`).
    SettingsNotebookAutoSyncEvery,
    /// A GENERAL-tab text field (`default_notebook`/`editor`/`daily_template`)
    /// — which one is always `GeneralField::ALL[App.settings_selected]`, not
    /// tracked here, same reasoning as the `SettingsNotebook*` variants above.
    SettingsGeneralText,
    /// A GIT-tab (global `[git]` defaults) text/number field — which one is
    /// `GitField::ALL[App.settings_selected]`. The boolean fields
    /// (`auto_commit`/`auto_push`/`sign_commits`/`auto_sync`) don't need a
    /// prompt at all, they toggle in place on `Enter`.
    SettingsGitText,
    /// A brand-new snippet's trigger, typed via SNIPPETS level 1's `a` —
    /// the only `Settings*` prompt not tied to `settings_selected`/a drill
    /// field, since the snippet doesn't exist yet to index into.
    SettingsSnippetTrigger,
    /// The drilled-into snippet's `label` (SNIPPETS level 2) — which
    /// snippet is `App.settings_snippet_drill`. Its `body` isn't edited
    /// through a `PendingInput` prompt at all (see `App.editing_snippet`);
    /// a snippet body is arbitrary multi-line text, which a single-line
    /// prompt can't hold.
    SettingsSnippetLabel,
    /// Move or copy — one or more items (`App.pending_batch` always holds
    /// the actual list, even for a single item, so there's exactly one
    /// apply path regardless of how many things are selected). Which of
    /// the two this is comes from `pending_batch`'s `BatchOp`, not a
    /// separate variant here.
    MoveOrCopy,
}

impl PendingInput {
    /// Falls back to this when `App.pending_input_title` is `None` — every
    /// variant except `MoveOrCopy`/the `Settings*` ones always has a plain
    /// static title; those depend on which notebook/snippet/field they're
    /// acting on (and, for `MoveOrCopy`, which op/items), so their title is
    /// always computed and set explicitly by whatever starts that
    /// particular input instead.
    pub(crate) fn title(self) -> &'static str {
        match self {
            PendingInput::NewNote => " New note (@ for quick date/template) ",
            PendingInput::NewNotebook => " New notebook ",
            PendingInput::NewFolder => " New folder ",
            PendingInput::RenameNote | PendingInput::RenameNotebook => " Rename ",
            PendingInput::Search => " Jump to note ",
            PendingInput::SetRemote => " Git remote (URL or local path) ",
            PendingInput::SettingsNotebookRemote => " Git remote (URL or local path) ",
            PendingInput::SettingsNotebookAutoSyncEvery => " Auto-sync every N changes ",
            PendingInput::SettingsGeneralText | PendingInput::SettingsGitText => " Edit value ",
            PendingInput::SettingsSnippetTrigger => " New snippet trigger ",
            PendingInput::SettingsSnippetLabel => " Snippet label ",
            PendingInput::MoveOrCopy => " Move/copy to ",
        }
    }

    /// Small muted line rendered under the input box, `None` for every
    /// variant except the ones whose valid input isn't obvious from the
    /// title alone — e.g. `NewNotebook` silently accepts a git URL instead
    /// of a plain name (see `App::confirm_input`'s `looks_like_git_url`
    /// branch), which nothing else in the modal hints at.
    pub(crate) fn hint(self) -> Option<&'static str> {
        match self {
            PendingInput::NewNotebook => Some(
                "A name creates a new local notebook. Paste a repo URL (https://, git@, ssh://) \
                 to clone it instead — make sure you're logged in first if it's private. A path \
                 (/abs, ~/docs, ./relative) adopts that existing directory instead.",
            ),
            _ => None,
        }
    }
}

/// A quick `@`-triggered shortcut typed into the `NewNote` title prompt —
/// `today`/`yesterday`/`tomorrow` resolve to a computed date (no template),
/// every other entry is a real `.md` file already sitting in the templates
/// dir (same discovery `open_template_picker` does). Picking one from the
/// dropdown finishes note creation immediately, skipping the normal
/// "type a title, Enter, then pick a template from `show_template_picker`"
/// two-step flow — that's the whole point of `@` being faster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QuickCommand {
    Today,
    Yesterday,
    Tomorrow,
    Template(String),
}

impl QuickCommand {
    pub(crate) fn label(&self) -> String {
        match self {
            QuickCommand::Today => "today".to_string(),
            QuickCommand::Yesterday => "yesterday".to_string(),
            QuickCommand::Tomorrow => "tomorrow".to_string(),
            QuickCommand::Template(name) => name.clone(),
        }
    }

    /// `None` for `Template` — only the three date commands have one.
    pub(crate) fn date(&self) -> Option<chrono::NaiveDate> {
        let today = chrono::Local::now().date_naive();
        match self {
            QuickCommand::Today => Some(today),
            QuickCommand::Yesterday => today.pred_opt(),
            QuickCommand::Tomorrow => today.succ_opt(),
            QuickCommand::Template(_) => None,
        }
    }

    /// One line for the dropdown list — date commands preview the actual
    /// resolved date (matching the `%Y-%m-%d` convention used everywhere
    /// else dates are formatted, see `daily.rs`) so the user sees exactly
    /// what they'll get before picking it.
    pub(crate) fn display(&self) -> String {
        match self {
            QuickCommand::Template(name) => format!("{name}  (template)"),
            _ => format!(
                "{}  \u{2192} {}",
                self.label(),
                self.date()
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default()
            ),
        }
    }
}

/// One note or folder captured by absolute path at the moment a move/copy/
/// delete was initiated — capturing eagerly (rather than re-deriving from
/// `selected_note()`/`selected_folder()` at confirm time) means a
/// background sync's `reload_notes` completing while the prompt/confirm
/// dialog is open can't shift the underlying list out from under an
/// in-flight action.
#[derive(Debug, Clone)]
pub(crate) enum SelectedEntry {
    Note(std::path::PathBuf),
    Folder(std::path::PathBuf),
}

/// One item moved into the trash by a delete — enough to move it back to
/// exactly where it came from.
#[derive(Debug, Clone)]
pub(crate) struct TrashedEntry {
    pub(crate) notebook: String,
    pub(crate) original_path: std::path::PathBuf,
    pub(crate) trash_path: std::path::PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatchOp {
    Move,
    Copy,
}

/// A mouse gesture in progress (or just released) over PREVIEW's note
/// body — `Option<T>` shape, not a bare field, since it's transient state
/// that only exists between a `Down` and its matching `Up`, same convention
/// as `pending_batch`/`pending_delete`/`sync_in_flight` rather than
/// `visual_anchor`'s mode-scoped one (this isn't tied to `Mode::Visual`).
/// Both rows are document-row indices into `note_preview_lines()`, already
/// resolved via `panel_preview::preview_row_at` at hit-test time. Doubles as
/// the state for two distinct gestures: a plain click (`dragged` stays
/// `false`, `anchor_row == current_row` at release) enters `Mode::Edit` at
/// that row (see `App::enter_edit_at_preview_row`); an actual click-and-drag
/// (`dragged` set the moment any `Drag` event arrives) selects and copies
/// the spanned rows to the clipboard on release, same as before this
/// struct's `dragged` field existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PreviewSelection {
    pub(crate) anchor_row: usize,
    pub(crate) current_row: usize,
    pub(crate) dragged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeleteTarget {
    Note,
    Folder,
    Notebook,
}

/// `(note path, [fg, accent, muted, link], content width, formatted lines,
/// source-line-per-row)` — see `App::note_preview_cache`'s own doc comment
/// for what each element means. Named only to keep clippy's
/// `type_complexity` lint quiet; still just a plain tuple everywhere it's
/// used.
type NotePreviewCache = (
    std::path::PathBuf,
    [Color; 4],
    u16,
    Vec<Line<'static>>,
    Vec<usize>,
);

/// Which of the find/replace bar's two fields is currently typed into —
/// `Tab` switches between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FindField {
    Query,
    Replace,
}

/// Ctrl+F's find/replace bar (`config.editor.find_replace`), open only
/// inside `Mode::Edit` — `None` means the bar is closed. Reuses `InputBox`
/// for both fields, same as the global-search bar (`global_search_input`),
/// rather than inventing new text-input handling.
pub(crate) struct EditorFindState {
    pub(crate) query: InputBox,
    pub(crate) replace: InputBox,
    pub(crate) focus: FindField,
    /// The cursor position when the bar was opened — where a fresh search
    /// (no existing selection yet, e.g. right after opening or right after
    /// closing) starts scanning from.
    pub(crate) anchor: (usize, usize),
}

pub struct App {
    pub config: Config,
    pub theme: Theme,
    pub store: NotebookStore,
    pub notebooks: Vec<Notebook>,
    pub selected_notebook: usize,
    /// Subfolder names at the current path within the selected notebook,
    /// shown above `notes` in the panel. A notebook can nest folders
    /// arbitrarily deep (like `nb`) — `notes_path` is the breadcrumb.
    pub folders: Vec<String>,
    pub notes: Vec<Note>,
    /// Index into the *combined* display list (`folders` then `notes`), not
    /// into `notes` alone — use `selected_note()`/`selected_folder()` rather
    /// than indexing `notes`/`folders` with this directly.
    pub selected_note: usize,
    /// The other end of the selection range while `mode == Mode::Visual` —
    /// set to `selected_note` when entering; the actual selected range is
    /// always `min(visual_anchor, selected_note)..=max(...)` over the same
    /// combined folders++notes list, so ordinary `j`/`k` movement (already
    /// moving `selected_note`) is all that's needed to extend/shrink it.
    /// Meaningless outside `Mode::Visual`.
    visual_anchor: usize,
    pub(crate) notes_path: Vec<String>,
    pub mode: Mode,
    pub focus: Focus,
    pub should_quit: bool,
    pub show_which_key: bool,
    pub show_tags: bool,
    /// Index into the tag list — `Vec<String>` from `TagIndex::tags()`
    /// (already sorted, `BTreeMap::keys()`), so this stays meaningful
    /// across redraws without needing to store the list itself.
    pub tags_selected: usize,
    /// `Some(tag)` while drilled into a tag's notes (level 2 of the tags
    /// modal); `None` while browsing the tag list itself (level 1).
    pub tags_viewing: Option<String>,
    /// Index into the *filtered* notes list, only meaningful while
    /// `tags_viewing.is_some()`.
    pub tags_notes_selected: usize,
    pub status_message: Option<String>,
    /// When `status_message` was last set — the footer only shows it for
    /// `STATUS_MESSAGE_TIMEOUT`, after which `expire_status_message` (called
    /// once per `run()` loop iteration) clears it. It's always still in
    /// `log_history` regardless (leader+`l`), so nothing is actually lost.
    status_message_set_at: Option<std::time::Instant>,
    /// Branch/dirty/ahead-behind for the selected notebook — refreshed
    /// whenever the notebook, folder, or notes change (`refresh_git_status`).
    pub git_status: shiki_core::git::GitStatus,
    /// Per-file git status for the selected notebook, keyed by absolute
    /// path (matches `Note::path` directly) — refreshed in lockstep with
    /// `git_status` by the same `refresh_git_status` call sites, drives
    /// the NOTES list's per-row coloring.
    pub note_statuses:
        std::collections::HashMap<std::path::PathBuf, shiki_core::git::FileGitStatus>,
    /// `(notebook name, GitStatus)` for *every* notebook, not just the
    /// selected one — only populated while the drawer (`leader+b`) is open,
    /// since computing it for every notebook on every draw tick would be
    /// pure waste when nothing's showing it.
    pub drawer_statuses: Vec<(String, shiki_core::git::GitStatus)>,
    pub show_drawer: bool,
    pub drawer_selected: usize,
    pub input: InputBox,
    pub confirm: Option<confirm::ConfirmDialog>,
    pub editor: Option<InlineEditor<'static>>,
    /// Click-count tracking for `config.editor.mouse_selection` (single vs.
    /// double vs. triple click) inside the editor — `(when, column, row)` of
    /// the last `Down` event, compared against a short time+position window
    /// on the next one; `None` once the window lapses or focus moves away
    /// from the editor.
    pub(crate) editor_last_click: Option<(std::time::Instant, u16, u16)>,
    /// How many consecutive clicks landed on the same cell within the
    /// double/triple-click window — capped at 3 (single/word/line).
    pub(crate) editor_click_count: u8,
    /// Whether `ratatui_textarea::TextArea::start_selection` has already been
    /// called for the drag gesture currently in progress — a single-click
    /// `Down` doesn't start a selection by itself (it might just be a plain
    /// click-to-position), so the *first* `Drag` event after it is what
    /// actually begins one.
    pub(crate) editor_drag_active: bool,
    /// Ctrl+F's find/replace bar — see `EditorFindState`'s own doc comment.
    pub(crate) editor_find: Option<EditorFindState>,
    /// `config.editor.multi_cursor`'s extra cursors — empty means ordinary
    /// single-cursor editing (the common case, same "absent means normal"
    /// pattern as `preview_selection`). The *primary* cursor is always
    /// `editor`'s own live `TextArea` cursor/selection; these are purely
    /// virtual, tracked only here — see `multicursor::replay_keystroke`.
    pub(crate) editor_secondary_cursors: Vec<crate::multicursor::CursorState>,
    /// One entry per historical edit action, most recent last — how many
    /// `textarea.undo()` calls it takes to fully reverse that one action
    /// (1 for an ordinary single-cursor edit, N for a multi-cursor edit
    /// that mutated N cursors). Every code path that mutates the buffer
    /// directly (the plain forward path, multi-cursor replay, find/
    /// replace, OS-clipboard cut/paste, bracketed-paste) pushes its own
    /// entry here and clears `editor_redo_groups`, so Ctrl+U popping the
    /// last entry always undoes exactly one *user-perceived* action,
    /// however many cursors it touched — verified live: typing a
    /// multi-character word across 3 Alt+Click cursors and pressing
    /// Ctrl+U three times correctly peels off one whole keystroke's worth
    /// (all 3 cursors) per press, restoring the original text exactly.
    /// A single `usize` (rather than this stack) was tried first and was
    /// wrong: it only remembered the *most recent* keystroke's group size,
    /// so undoing a 3-letter word typed across 3 cursors correctly undid
    /// the last letter as one action but then fell back to single-cursor
    /// steps for the other two letters.
    pub(crate) editor_undo_groups: Vec<usize>,
    /// Mirrors `editor_undo_groups` for Ctrl+R — an undo pushes however
    /// many steps it actually undid here, so redoing it restores all of
    /// them as one action too.
    pub(crate) editor_redo_groups: Vec<usize>,
    /// The inline editor's `/`-menu (see `slash_menu.rs`) — open only
    /// while `Mode::Edit`'s current line reads exactly `/` up to the
    /// cursor (checked live off `editor.textarea` itself in
    /// `App::slash_query`, not tracked separately, so it can never
    /// disagree with what's actually in the buffer).
    pub(crate) show_slash_menu: bool,
    pub(crate) slash_menu_selected: usize,
    /// The inline editor's `[[wikilink]]` autocomplete — opens the instant
    /// the second `[` completes a `[[` pair (checked live off the buffer,
    /// same reasoning as `show_slash_menu`'s doc comment). Unlike the
    /// `/`-menu, this one lists *notes* rather than a fixed command set, so
    /// the candidate pool is snapshotted once when it opens
    /// (`wikilink_candidates`, mirroring `global_search_pool`) rather than
    /// re-walking the notebook on every keystroke — only the fuzzy score
    /// (`wikilink_results`, via the shared `search_engine`) is recomputed
    /// as the query changes.
    pub(crate) show_wikilink_menu: bool,
    pub(crate) wikilink_menu_selected: usize,
    pub(crate) wikilink_candidates: Vec<Note>,
    pub(crate) wikilink_results: Vec<SearchHit>,
    /// Path + editor command to launch externally, picked up by `run()`
    /// between draw calls. The editor is resolved per-invocation (either the
    /// configured `general.editor` for `E`, or the detected OS favorite for
    /// `i` when `use_favorite_editor` is on) rather than always reusing the
    /// static config value.
    pub want_external_edit: Option<(std::path::PathBuf, String)>,
    /// Set alongside `want_external_edit` when the path being externally
    /// edited is `config.toml`, not a note — `run()` checks this to decide
    /// whether to reload/apply the config afterward instead of refreshing
    /// notes, since the two `want_external_edit` call sites otherwise look
    /// identical (a path + an editor command to spawn).
    pub(crate) want_external_edit_config: bool,
    pub show_theme_picker: bool,
    pub show_global_search: bool,
    pub show_logs: bool,
    /// Settings screen (leader+`s`) — a summary of the current config,
    /// paged by tab (`settings_section`) rather than one long scroll, with
    /// `i`/`E` still jumping straight to editing `config.toml` itself for
    /// anything not covered below. Every tab is genuinely actionable now:
    /// `Enter` toggles a boolean in place, opens a prompt for a text/number
    /// field, opens the theme picker (THEME's `name`), or drills into a
    /// notebook/snippet (NOTEBOOKS/SNIPPETS) — `settings_selected` doubles
    /// as both the level-1 row index and (for the read-only-by-construction
    /// cases, e.g. an empty section) the scroll position, same `List`/
    /// `ListState`-does-the-scrolling-for-free trick the logs/tree/global-
    /// search modals use.
    pub(crate) show_settings: bool,
    pub(crate) settings_selected: usize,
    /// Active tab (GENERAL/THEME/GIT/NOTEBOOKS/SNIPPETS) — left/right
    /// (`App::switch_settings_section`) cycles it, resetting
    /// `settings_selected`/`settings_notebook_drill`/`settings_snippet_drill`/
    /// `settings_field_selected` each time, same as `toggle_settings` does
    /// when the modal first opens.
    pub(crate) settings_section: crate::panel_settings::SettingsSection,
    /// `Some(notebook name)` while drilled into that notebook's own fields
    /// (NOTEBOOKS level 2); `None` while browsing the flat notebook list
    /// (level 1) — same one-`Option`-is-the-level shape `tags_viewing`
    /// already uses for the tags modal's identical two-level pattern.
    pub(crate) settings_notebook_drill: Option<String>,
    /// Same idea as `settings_notebook_drill`, for SNIPPETS' level 2 — the
    /// two are never both `Some` at once (only one section is active at a
    /// time, and `switch_settings_section` clears both), but each gets its
    /// own field rather than one shared "drilled-into name" so the two
    /// sections' drill states can't be confused with each other.
    pub(crate) settings_snippet_drill: Option<String>,
    /// Selected row among the drilled-into notebook's/snippet's fields —
    /// kept separate from `settings_selected` (which indexes the level-1
    /// list instead) so switching levels never has to reinterpret one index
    /// as the other. Shared by both NOTEBOOKS and SNIPPETS level 2, safely,
    /// since only one of the two drill fields above is ever `Some` at once.
    pub(crate) settings_field_selected: usize,
    /// True while `self.editor` holds `config.toml`'s contents rather than
    /// a note's body — `save_and_exit_edit` checks this to decide whether
    /// to write+reload the config or save a note, since both share the same
    /// `Mode::Edit`/`InlineEditor` machinery.
    pub(crate) editing_config: bool,
    /// `Some(trigger)` while `self.editor` holds a snippet's `body` instead
    /// of `config.toml` or a note's body (SNIPPETS level 2's `body` field,
    /// leader+`s`) — checked by `save_and_exit_edit` right alongside
    /// `editing_config`, same three-way "which of these is this edit
    /// actually for" dispatch.
    pub(crate) editing_snippet: Option<String>,
    /// True while the editor contains the session-only scratchpad buffer.
    /// It has no path and is discarded unless explicitly saved as a note.
    pub(crate) editing_scratchpad: bool,
    /// True from the moment THEME's `name` row opens the theme picker from
    /// inside Settings until the picker closes — `show_settings` is hidden
    /// first (its render call in `draw()` comes after the theme picker's,
    /// so leaving it `true` would paint Settings right over the picker) and
    /// restored once `handle_theme_picker_key` closes it, but *only* when
    /// it was opened this way — the normal leader+`c` picker (opened with
    /// no Settings modal underneath at all) must not reopen one.
    pub(crate) reopen_settings_after_theme_picker: bool,
    /// Staged while the confirm dialog for SNIPPETS level 1's `d` is up —
    /// mirrors `pending_delete`'s pattern (a plain trigger string instead of
    /// a `(DeleteTarget, PathBuf)` pair, since a snippet has no filesystem
    /// path of its own).
    pub(crate) pending_delete_snippet: Option<String>,
    /// Every status-bar message set *this session*, oldest first (capped
    /// at 500 in memory) — the status bar only shows the latest one, so
    /// this is what backs the logs modal (leader+`l`) for anything that
    /// scrolled past, especially errors. Pre-populated from `log_path` on
    /// startup (see `App::new`), so it isn't only this session's messages
    /// in practice — the on-disk file itself isn't capped, only this
    /// in-memory view of it.
    pub log_history: Vec<LogEntry>,
    /// Where `log_history` is appended to, one line per entry
    /// (`Config::default_log_path`) — `None` if that path couldn't be
    /// resolved at startup, or once a write to it has failed (see
    /// `persist_log_entry`); either way, `log_history` still works as an
    /// in-memory-only log for the rest of the session.
    log_path: Option<std::path::PathBuf>,
    /// Staged while the confirm dialog for leader+`l`'s `x` (clear all
    /// logs) is up — mirrors `pending_delete`/`pending_revert`'s pattern.
    pub(crate) pending_clear_logs: bool,
    /// Where deleted notes/folders go instead of being permanently removed
    /// (`Config::default_trash_dir`) — `None` if that path couldn't be
    /// resolved, in which case delete falls back to the old permanent
    /// behavior rather than failing outright.
    pub(crate) trash_root: Option<std::path::PathBuf>,
    /// The most recently deleted note/folder (or whole batch of them),
    /// restorable with leader+`u` — a single level of undo, not a full
    /// history. Cleared once restored (or once another delete overwrites
    /// it); older trashed items simply stay on disk, unreachable from here
    /// but not actually gone.
    pub(crate) last_trash: Option<Vec<TrashedEntry>>,
    pub show_template_picker: bool,
    /// `None` is always the first entry ("blank, no template"); every
    /// `Some(name)` after it is a `.md` file found in the templates dir at
    /// the moment the picker opened.
    pub(crate) template_picker_options: Vec<Option<String>>,
    pub(crate) template_picker_index: usize,
    /// The title already confirmed by the `NewNote` prompt, carried over
    /// while the template picker is up — the note isn't actually created
    /// until a template (or "blank") is chosen.
    pub(crate) pending_new_note_title: String,
    /// Scratchpad contents staged while the new-note title flow is active.
    pub(crate) pending_new_note_body: Option<String>,
    /// Selected row in the `@`-triggered quick-template dropdown (see
    /// `QuickCommand`) — only meaningful while `NewNote`'s input contains an
    /// `@`; reset to 0 on every keystroke that changes the filter, same
    /// "never leave a stale out-of-range index" rule `which_key_selected`
    /// already follows.
    pub(crate) quick_template_selected: usize,
    pub show_tree: bool,
    pub(crate) tree_rows: Vec<crate::tree::TreeRow>,
    /// Index into just the `Note` rows of `tree_rows` (folder rows are
    /// display-only and never selectable) — not a raw row index.
    pub(crate) tree_selected: usize,
    pub show_links: bool,
    pub(crate) link_rows: Vec<crate::links_panel::LinkRow>,
    /// Index into just the selectable rows of `link_rows` (section headers
    /// are display-only) — same shape as `tree_selected`.
    pub(crate) link_selected: usize,
    /// True right after the leader key is pressed, waiting for the next key
    /// to resolve against the `global` scope.
    pub leader_pending: bool,
    /// Vertical scroll offset for the preview pane (only moves while
    /// PREVIEW has focus, since there's no list to navigate there).
    pub preview_scroll: u16,
    /// A mouse drag-to-select in progress over PREVIEW's note body, if any
    /// — see `PreviewSelection`. `None` outside of an active drag.
    pub(crate) preview_selection: Option<PreviewSelection>,
    /// Terminal area as of the last draw — reused to hit-test mouse clicks
    /// against the same popup layout that was actually rendered.
    pub(crate) last_frame_area: Rect,
    pub(crate) available_themes: Vec<Theme>,
    pub(crate) theme_index: usize,
    pub(crate) theme_picker_index: usize,
    note_sort: NoteSort,
    pub(crate) pending_input: Option<PendingInput>,
    /// Overrides `PendingInput::title()`'s static text when set — only
    /// `PendingInput::MoveOrCopy` ever needs this (see its doc comment).
    pub(crate) pending_input_title: Option<String>,
    pub(crate) pending_delete: Option<(DeleteTarget, std::path::PathBuf)>,
    /// The items a move/copy is about to apply to, captured up front —
    /// populated whether it's a single note/folder or a whole Visual-mode
    /// selection, so `confirm_input`'s `MoveOrCopy` arm has exactly one
    /// apply path regardless of count.
    pub(crate) pending_batch: Option<(BatchOp, Vec<SelectedEntry>)>,
    /// `Mode::Visual`'s `d` — same eager-capture reasoning as
    /// `pending_batch`, staged behind the same `confirm` dialog
    /// `pending_delete` already uses for a single note/notebook/folder.
    pub(crate) pending_batch_delete: Option<Vec<SelectedEntry>>,
    pub(crate) global_search_pool: Vec<(Notebook, Note)>,
    pub(crate) global_search_input: InputBox,
    pub(crate) global_search_results: Vec<SearchHit>,
    pub(crate) global_search_selected: usize,
    pub(crate) search_engine: SearchEngine,
    pub(crate) keymaps: KeyMaps,
    pub(crate) logs_selected: usize,
    /// Note changes (new/edited/renamed/deleted/moved) since each
    /// notebook's last sync, keyed by notebook name — drives `auto_sync`'s
    /// `auto_sync_every` threshold (`note_changed`). Not persisted across
    /// restarts; a relaunch just starts counting from zero again.
    pub(crate) pending_changes: std::collections::HashMap<String, u32>,
    /// Filter query typed into the which-key modal (leader-less, `?`) —
    /// matches against the key, action label, or scope name.
    pub which_key_input: InputBox,
    pub which_key_selected: usize,
    /// The OS-detected favorite editor, resolved once at startup (not
    /// per-render — detection can shell out to `xdg-mime` on Linux, too
    /// expensive to redo every ~100ms draw tick) and reused both for the
    /// footer's editor-mode indicator and `Action::EditInline`'s dispatch,
    /// so they can never disagree with each other.
    pub favorite_editor: Option<String>,
    /// Shows each note's date next to its title in the NOTES list — off by
    /// default, toggled by `Action::ToggleDates`.
    pub show_dates: bool,
    pub show_history: bool,
    pub(crate) history_entries: Vec<shiki_core::git::FileRevision>,
    pub(crate) history_selected: usize,
    /// `Some((commit_id, content))` while viewing one historical revision's
    /// full content inside the history modal; `None` while just browsing
    /// the revision list.
    pub(crate) history_viewing: Option<(String, String)>,
    /// `(note path, commit id)` to revert to, staged while the `confirm`
    /// dialog is up — mirrors `pending_delete`'s pattern so `y`/`n` in
    /// `handle_confirm_key` can handle either kind of pending action.
    pub(crate) pending_revert: Option<(std::path::PathBuf, String)>,
    /// `(name, path)` staged while the `confirm` dialog asks whether to
    /// `git init` a directory being adopted as a notebook (see
    /// `adopt_notebook_from_path`) — same pattern as `pending_revert`.
    pub(crate) pending_notebook_adopt: Option<(String, std::path::PathBuf)>,
    /// Cache for the footer's "{n} changes" indicator: `(note path, revision
    /// count)` for whichever note was last checked, so `run()` calling this
    /// every draw tick only actually re-walks history when the selected
    /// note has changed, not on every idle redraw.
    pub(crate) history_count_cache: Option<(std::path::PathBuf, usize)>,
    /// Cache for the PREVIEW panel's folder peek: `(folder's absolute path,
    /// subfolder names, note titles)` for whichever folder was last read, so
    /// `run()` calling this every draw tick only actually re-lists the
    /// directory (and re-parses each note's frontmatter) when the selected
    /// folder has changed, not on every idle redraw.
    pub(crate) folder_preview_cache: Option<(std::path::PathBuf, [Color; 4], Vec<Line<'static>>)>,
    /// Cache for the PREVIEW panel's note view: `(note path, [fg, accent,
    /// muted, link], formatted lines, source-line-per-row)` for whichever
    /// note was last formatted, so `run()` calling this every draw tick only
    /// re-runs `markdown_to_lines` (a full scan of the note body — real CPU
    /// cost on a large note, unlike the folder cache above this isn't I/O)
    /// when the selected note or the active theme's colors actually
    /// changed, not on every idle redraw. Colors are part of the key
    /// because the theme picker live-previews by mutating `self.theme`
    /// while browsing, and a stale-colored cache hit would show the wrong
    /// theme until the note changed. The last element parallels the
    /// formatted lines 1:1, giving the 0-based `body.lines()` index each
    /// rendered (and now word-wrapped) row came from — see
    /// `note_preview_source_line`, which click-to-edit uses to jump into
    /// `Mode::Edit` at the right raw source line.
    pub(crate) note_preview_cache: Option<NotePreviewCache>,
    /// Cache for the tags modal's `TagIndex::build(&self.notes)` — rebuilt
    /// (`refresh_tag_index_cache`) only when the modal is open and the cache
    /// is empty; `reload_notes`/`refresh_notes_preserve_selection` clear it
    /// to `None` whenever `self.notes` actually changes underneath it, the
    /// same invalidation trigger `note_preview_cache`/`folder_preview_cache`
    /// already use. `None` whenever the modal is closed, so it doesn't hold
    /// a stale index in memory for no reason.
    pub(crate) tag_index_cache: Option<shiki_core::TagIndex>,
    pub show_update: bool,
    pub update_state: Option<UpdateState>,
    /// Set while a background thread is checking/installing, so `run()`'s
    /// poll loop (`poll_update_channel`) can pick up the result without
    /// blocking the render loop on the network call. `self_update`'s HTTP
    /// calls are synchronous/blocking, and nothing else in this app uses
    /// async — a plain `std::thread` + channel matches the rest of the
    /// codebase's synchronous poll-loop style instead of pulling in an
    /// async runtime for this one feature.
    pub(crate) update_rx: Option<std::sync::mpsc::Receiver<UpdateMsg>>,
    /// Set once `install_latest` succeeds — picked up by `run()` right after
    /// the next draw to spawn the freshly-installed binary and exit this
    /// process, the same "leave the alternate screen, hand off to a
    /// subprocess" shape as `want_external_edit`/`suspend_and_edit`, except
    /// this one doesn't come back (`should_quit` follows immediately after).
    pub want_relaunch: bool,
    /// The running binary's path, captured *before* `install_latest` runs.
    /// `self_replace` (used internally by `self_update`) replaces the file
    /// via unlink-then-recreate, not an atomic rename-over — so querying
    /// `std::env::current_exe()` again *after* the replace resolves to the
    /// old, now-deleted inode (`".../shiki (deleted)"` on Linux) rather than
    /// the fresh binary at that same path. The path string itself is still
    /// valid throughout (only the file's *content* changed), so capturing it
    /// early and reusing it in `relaunch_into_updated_binary` is what
    /// actually works — hit this exact bug live (`spawn FAILED: No such
    /// file or directory ... "shiki (deleted)"`) before fixing it this way.
    pub(crate) relaunch_exe_path: Option<std::path::PathBuf>,
    /// Set while a background thread is running a sync/push/pull, so
    /// `run()`'s poll loop (`poll_sync_channel`) can pick up the result
    /// without blocking the render loop on the network call — same
    /// `std::thread` + `mpsc` shape as `update_rx` above, applied to the
    /// normal git operations instead of the self-updater. Holds the label
    /// shown by the footer's spinner (e.g. the notebook's name) while
    /// something's running; `None` means idle.
    pub sync_in_flight: Option<String>,
    pub(crate) sync_rx: Option<std::sync::mpsc::Receiver<crate::sync::GitOpResult>>,
    /// Advanced once per `run()` iteration while `sync_in_flight` is set —
    /// indexes into a small Braille frame set for the footer's spinner. Not
    /// reset when idle: picking back up from wherever it left off is fine,
    /// nobody's watching for an exact starting frame. The self-updater's own
    /// in-flight state (`update_state: Checking`/`Downloading`) does *not*
    /// advance this today — nothing in the update-check UI renders a
    /// spinner glyph from it yet. If one is ever wired in there, extend the
    /// gating in `run()` (`if app.sync_in_flight.is_some() { ... }`) to
    /// cover that case too rather than assuming this field already does.
    pub spinner_frame: usize,
}

/// State of the update modal (leader+`U`), across its whole lifecycle: a
/// cheap version check, then — only on explicit confirmation — the real
/// download+verify+install.
#[derive(Debug, Clone)]
pub enum UpdateState {
    Checking,
    Available(String),
    UpToDate,
    Downloading,
    /// Installed; `run()` will relaunch into it right after this renders once.
    Installed(String),
    Error(String),
}

/// Sent back from the background thread spawned by `open_update_check`/
/// `start_update_install` — `poll_update_channel` (called once per `run()`
/// loop iteration, same as `refresh_history_cache`) applies it to `update_state`.
pub(crate) enum UpdateMsg {
    CheckResult(Result<Option<String>, String>),
    InstallResult(Result<String, String>),
}

/// One recorded status-bar message, with the time it was set — shown in the
/// logs modal (leader+`l`) so an error isn't lost the moment the next status
/// update overwrites `status_message`.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub at: chrono::DateTime<chrono::Local>,
    pub message: String,
}

/// One line per entry: RFC3339 timestamp, a tab, the message — chosen so
/// parsing back is unambiguous (split on the first tab) even if the message
/// itself contains brackets, colons, or anything else a more "readable"
/// format might collide with.
fn format_log_line(entry: &LogEntry) -> String {
    format!("{}\t{}\n", entry.at.to_rfc3339(), entry.message)
}

fn parse_log_line(line: &str) -> Option<LogEntry> {
    let (at, message) = line.split_once('\t')?;
    let at = chrono::DateTime::parse_from_rfc3339(at)
        .ok()?
        .with_timezone(&chrono::Local);
    Some(LogEntry {
        at,
        message: message.to_string(),
    })
}

/// Reads whatever `log_path` already has from previous sessions, capped to
/// the same 500-entry window `log_history` keeps in memory — the file
/// itself isn't capped (that's what the logs modal's `x`/clear is for), but
/// there's no reason to load more than the in-memory view will ever show.
/// Missing file, unreadable file, or unparseable lines are all silently
/// tolerated here (an empty prior history is the correct fallback, not a
/// startup error) — malformed *individual* lines are just skipped rather
/// than failing the whole read, same "one bad file doesn't blank out
/// everything" spirit as `Notebook::list_notes`.
fn load_log_history(path: &std::path::Path) -> Vec<LogEntry> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut entries: Vec<LogEntry> = contents.lines().filter_map(parse_log_line).collect();
    if entries.len() > 500 {
        entries.drain(..entries.len() - 500);
    }
    entries
}

impl App {
    pub fn new(config: Config, store: NotebookStore) -> shiki_core::Result<Self> {
        let theme = config.theme.resolve();
        let keymaps = KeyMaps::from_config(&config.keybindings);
        let notebooks = store.list()?;
        let (folders, notes) = notebooks
            .first()
            .map(|nb| nb.list_dir(std::path::Path::new("")))
            .transpose()?
            .unwrap_or_default();
        let git_status = notebooks
            .first()
            .map(|nb| shiki_core::git::status(&nb.path, &config.git.remote))
            .unwrap_or_default();
        let note_statuses = notebooks
            .first()
            .and_then(|nb| shiki_core::git::file_statuses(&nb.path).ok())
            .unwrap_or_default();
        let log_path = Config::default_log_path().ok();
        let log_history = log_path
            .as_deref()
            .map(load_log_history)
            .unwrap_or_default();
        let trash_root = Config::default_trash_dir().ok();
        let available_themes = shiki_config::themes::all();
        let theme_index = available_themes
            .iter()
            .position(|t| t.name == theme.name)
            .unwrap_or(0);

        let mut app = Self {
            config,
            theme,
            store,
            notebooks,
            selected_notebook: 0,
            folders,
            notes,
            selected_note: 0,
            visual_anchor: 0,
            notes_path: Vec::new(),
            mode: Mode::Normal,
            focus: Focus::Notebooks,
            should_quit: false,
            show_which_key: false,
            show_tags: false,
            tags_selected: 0,
            tags_viewing: None,
            tags_notes_selected: 0,
            status_message: None,
            status_message_set_at: None,
            git_status,
            note_statuses,
            drawer_statuses: Vec::new(),
            show_drawer: false,
            drawer_selected: 0,
            input: InputBox::default(),
            confirm: None,
            editor: None,
            editor_last_click: None,
            editor_click_count: 0,
            editor_drag_active: false,
            editor_find: None,
            editor_secondary_cursors: Vec::new(),
            editor_undo_groups: Vec::new(),
            editor_redo_groups: Vec::new(),
            show_slash_menu: false,
            slash_menu_selected: 0,
            show_wikilink_menu: false,
            wikilink_menu_selected: 0,
            wikilink_candidates: Vec::new(),
            wikilink_results: Vec::new(),
            want_external_edit: None,
            want_external_edit_config: false,
            show_theme_picker: false,
            show_global_search: false,
            show_logs: false,
            show_settings: false,
            settings_selected: 0,
            settings_section: crate::panel_settings::SettingsSection::General,
            settings_notebook_drill: None,
            settings_snippet_drill: None,
            settings_field_selected: 0,
            editing_config: false,
            editing_snippet: None,
            editing_scratchpad: false,
            reopen_settings_after_theme_picker: false,
            pending_delete_snippet: None,
            log_history,
            log_path,
            pending_clear_logs: false,
            trash_root,
            last_trash: None,
            show_template_picker: false,
            template_picker_options: Vec::new(),
            template_picker_index: 0,
            pending_new_note_title: String::new(),
            pending_new_note_body: None,
            quick_template_selected: 0,
            show_tree: false,
            tree_rows: Vec::new(),
            tree_selected: 0,
            show_links: false,
            link_rows: Vec::new(),
            link_selected: 0,
            leader_pending: false,
            preview_scroll: 0,
            preview_selection: None,
            last_frame_area: Rect::default(),
            available_themes,
            theme_index,
            theme_picker_index: theme_index,
            note_sort: NoteSort::default(),
            pending_input: None,
            pending_input_title: None,
            pending_delete: None,
            pending_batch: None,
            pending_batch_delete: None,
            global_search_pool: Vec::new(),
            global_search_input: InputBox::default(),
            global_search_results: Vec::new(),
            global_search_selected: 0,
            search_engine: SearchEngine::new(),
            keymaps,
            logs_selected: 0,
            pending_changes: std::collections::HashMap::new(),
            which_key_input: InputBox::default(),
            which_key_selected: 0,
            favorite_editor: shiki_core::editor::detect_favorite_editor(),
            show_dates: false,
            show_history: false,
            history_entries: Vec::new(),
            history_selected: 0,
            history_viewing: None,
            pending_revert: None,
            pending_notebook_adopt: None,
            history_count_cache: None,
            folder_preview_cache: None,
            note_preview_cache: None,
            tag_index_cache: None,
            show_update: false,
            update_state: None,
            update_rx: None,
            want_relaunch: false,
            relaunch_exe_path: None,
            sync_in_flight: None,
            sync_rx: None,
            spinner_frame: 0,
        };

        if app.config.general.remember_last_session {
            if let Some(session) = Config::default_session_path()
                .ok()
                .and_then(|path| shiki_config::SessionState::load(&path))
            {
                app.restore_session(session);
            }
        }

        Ok(app)
    }

    /// Applies a previously saved `SessionState` (`general.remember_last_session`)
    /// on top of the just-constructed default state (first notebook, root
    /// folder, `Focus::Notebooks`) — called once from `new`, right after
    /// construction, since restoring is really a post-construction mutation
    /// (select a different notebook, descend into a folder, re-select an
    /// entry) rather than something `new` can compute up front. Anything that
    /// no longer resolves (a renamed/deleted notebook, a moved note, an
    /// unrecognized focus string) is silently left at that default instead
    /// of erroring — a stale session file should degrade gracefully, never
    /// block startup.
    fn restore_session(&mut self, session: shiki_config::SessionState) {
        let Some(idx) = self
            .notebooks
            .iter()
            .position(|nb| nb.name == session.notebook)
        else {
            return;
        };
        self.selected_notebook = idx;
        self.notes_path = session.notes_path;
        self.reload_notes();

        self.selected_note = match session.selected {
            Some(shiki_config::session::SelectedEntry::Folder { name }) => {
                self.folders.iter().position(|f| *f == name).unwrap_or(0)
            }
            Some(shiki_config::session::SelectedEntry::Note { stem }) => self
                .notes
                .iter()
                .position(|n| n.file_stem() == stem)
                .map(|i| self.folders.len() + i)
                .unwrap_or(0),
            None => 0,
        };

        if let Some(focus) = Focus::from_session_str(&session.focus) {
            self.focus = focus;
        }
    }

    /// Sets the status-bar message and records it in `log_history`, so an
    /// error isn't lost once the footer clears it — see the logs modal
    /// (leader+`l`) for the permanent record.
    pub(crate) fn set_status(&mut self, message: String) {
        let entry = LogEntry {
            at: chrono::Local::now(),
            message: message.clone(),
        };
        self.persist_log_entry(&entry);
        self.log_history.push(entry);
        if self.log_history.len() > 500 {
            self.log_history.remove(0);
        }
        self.status_message = Some(message);
        self.status_message_set_at = Some(std::time::Instant::now());
    }

    /// Appends one line to `log_path`, creating the parent directory (the
    /// config dir — always exists already in practice, since a config file
    /// had to be loaded to get this far, but cheap to make sure) if needed.
    /// A write failure disables further persistence for the rest of the
    /// session (`log_path = None`) rather than retrying every single status
    /// update — and is reported once, by pushing directly into
    /// `log_history`/`status_message` instead of going back through
    /// `set_status` (which would call this again, recursing).
    fn persist_log_entry(&mut self, entry: &LogEntry) {
        let Some(path) = self.log_path.clone() else {
            return;
        };
        let write_result = (|| -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            file.write_all(format_log_line(entry).as_bytes())
        })();
        if let Err(e) = write_result {
            self.log_path = None;
            let failure = LogEntry {
                at: chrono::Local::now(),
                message: format!("log persistence disabled ({}): {e}", path.display()),
            };
            self.status_message = Some(failure.message.clone());
            self.status_message_set_at = Some(std::time::Instant::now());
            self.log_history.push(failure);
        }
    }

    /// leader+`l` then `x` (behind a confirmation, `App.pending_clear_logs`)
    /// — wipes both the in-memory view and the on-disk file. Reports itself
    /// via `set_status` same as any other action, which doubles as the
    /// first line of the now-empty file: "when was this last cleared" is
    /// itself useful context for whoever reads it next.
    pub(crate) fn clear_logs(&mut self) {
        self.log_history.clear();
        self.logs_selected = 0;
        if let Some(path) = &self.log_path {
            let _ = std::fs::write(path, "");
        }
        self.set_status("logs cleared".into());
    }

    /// Clears the footer's status message once it's been showing for
    /// `STATUS_MESSAGE_TIMEOUT` — called once per `run()` loop iteration.
    /// It stays in `log_history` regardless, so this only shortens how long
    /// it sits in the footer pushing other segments around, not how long
    /// it's actually retrievable.
    fn expire_status_message(&mut self) {
        if let Some(set_at) = self.status_message_set_at {
            if set_at.elapsed() >= STATUS_MESSAGE_TIMEOUT {
                self.status_message = None;
                self.status_message_set_at = None;
            }
        }
    }

    pub fn selected_notebook(&self) -> Option<&Notebook> {
        self.notebooks.get(self.selected_notebook)
    }

    /// `None` both when nothing's selected and when the current selection is
    /// a folder, not a note — check `selected_folder()` to tell those apart.
    pub fn selected_note(&self) -> Option<&Note> {
        self.selected_note
            .checked_sub(self.folders.len())
            .and_then(|idx| self.notes.get(idx))
    }

    pub fn selected_folder(&self) -> Option<&str> {
        self.folders.get(self.selected_note).map(String::as_str)
    }

    fn combined_len(&self) -> usize {
        self.folders.len() + self.notes.len()
    }

    /// `v` in NOTES: anchors a `Mode::Visual` selection at whatever's
    /// currently selected. Pressing it again (already in Visual mode) is
    /// the cancel — same toggle shape as the drawer's leader binding.
    pub(crate) fn toggle_visual(&mut self) {
        if self.mode == Mode::Visual {
            self.mode = Mode::Normal;
            return;
        }
        if self.focus != Focus::Notes {
            self.set_status("select mode only works in NOTES".into());
            return;
        }
        if self.combined_len() == 0 {
            self.set_status("nothing to select".into());
            return;
        }
        self.visual_anchor = self.selected_note;
        self.mode = Mode::Visual;
    }

    /// The selected range over the combined folders++notes list — ordinary
    /// `j`/`k` (already moving `selected_note`) is all `Mode::Visual` needs
    /// to extend/shrink this, `visual_anchor` just stays where it was set.
    fn visual_range(&self) -> std::ops::RangeInclusive<usize> {
        self.visual_anchor.min(self.selected_note)..=self.visual_anchor.max(self.selected_note)
    }

    /// How many rows are currently in the visual selection — for the
    /// footer's `"VISUAL (n selected)"` label (`status_bar.rs`) and
    /// `panel_notes.rs`'s range highlighting. `0` outside `Mode::Visual`.
    pub(crate) fn visual_selection_count(&self) -> usize {
        if self.mode != Mode::Visual {
            return 0;
        }
        self.visual_range().count()
    }

    /// Whether row `idx` (an index into the combined folders++notes list)
    /// falls inside the current visual selection — used by
    /// `panel_notes.rs` to tint every selected row, not just the one under
    /// the cursor.
    pub(crate) fn is_visually_selected(&self, idx: usize) -> bool {
        self.mode == Mode::Visual && self.visual_range().contains(&idx)
    }

    /// Every note/folder currently inside `visual_range`, as absolute
    /// paths — the eager capture `pending_batch`/`pending_batch_delete`
    /// both rely on (see their doc comments for why).
    pub(crate) fn visual_selected_entries(&self) -> Vec<SelectedEntry> {
        let Some(nb) = self.selected_notebook() else {
            return Vec::new();
        };
        let relative = self.notes_relative_path();
        self.visual_range()
            .filter_map(|idx| {
                if let Some(name) = self.folders.get(idx) {
                    Some(SelectedEntry::Folder(nb.path.join(&relative).join(name)))
                } else {
                    self.notes
                        .get(idx - self.folders.len())
                        .map(|n| SelectedEntry::Note(n.path.clone()))
                }
            })
            .collect()
    }

    /// Where the Notes panel currently is within the selected notebook —
    /// `""` at the notebook root, otherwise the breadcrumb joined as a path.
    pub fn notes_relative_path(&self) -> std::path::PathBuf {
        self.notes_path.iter().collect()
    }

    /// Path to the notebook's breadcrumb, for display (`"personal / projects"`).
    pub fn notes_breadcrumb(&self) -> Option<String> {
        if self.notes_path.is_empty() {
            None
        } else {
            Some(self.notes_path.join(" / "))
        }
    }

    pub(crate) fn apply_sort(&mut self) {
        match self.note_sort {
            NoteSort::Filename => self.notes.sort_by(|a, b| a.path.cmp(&b.path)),
            NoteSort::TitleAz => self.notes.sort_by(|a, b| {
                a.frontmatter
                    .title
                    .to_lowercase()
                    .cmp(&b.frontmatter.title.to_lowercase())
            }),
            NoteSort::DateNewest => self
                .notes
                .sort_by_key(|n| std::cmp::Reverse(n.frontmatter.date)),
        }
    }

    pub(crate) fn cycle_sort(&mut self) {
        let stem = self.selected_note().map(|n| n.file_stem());
        self.note_sort = self.note_sort.next();
        self.apply_sort();
        if let Some(stem) = stem {
            if let Some(idx) = self.notes.iter().position(|n| n.file_stem() == stem) {
                self.selected_note = self.folders.len() + idx;
            }
        }
        self.set_status(format!("sort: {}", self.note_sort.label()));
    }

    /// Default name for the empty-input fast path when creating a notebook:
    /// "notebook", or "notebook-2", "notebook-3", … the first one that
    /// doesn't already exist, so pressing Enter with no name repeatedly
    /// never collides or silently fails.
    pub(crate) fn unique_default_notebook_name(&self) -> String {
        let base = "notebook";
        if self.store.get(base).is_err() {
            return base.to_string();
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base}-{n}");
            if self.store.get(&candidate).is_err() {
                return candidate;
            }
            n += 1;
        }
    }

    /// New-notebook fast path for pasting a git URL directly: derives the
    /// notebook name from the repo name, creates it, points its remote at
    /// the URL, and pulls right away.
    pub(crate) fn create_notebook_from_url(&mut self, url: &str) {
        // Redacted before it ever reaches a status message/log_history —
        // never the real `url`, which still goes to `set_remote`/`pull`
        // below since those actually need the credentials to work.
        let redacted = shiki_core::git::redact_credentials(url);
        let Some(name) = notebook_name_from_git_url(url) else {
            self.set_status(format!(
                "could not derive a notebook name from '{redacted}'"
            ));
            return;
        };
        let notebook = match self.store.create(&name) {
            Ok(nb) => nb,
            Err(e) => {
                self.set_status(format!("could not create '{name}': {e}"));
                return;
            }
        };
        if let Err(e) = shiki_core::git::set_remote(&notebook.path, url) {
            self.reload_notebooks();
            self.set_status(format!("created '{name}' but could not set remote: {e}"));
            return;
        }
        self.reload_notebooks();
        if let Some(idx) = self.notebooks.iter().position(|nb| nb.name == name) {
            self.selected_notebook = idx;
        }
        match shiki_core::git::pull(
            &notebook.path,
            &self.config.git.remote,
            &self.config.git.branch,
        ) {
            Ok(branch) => {
                self.reload_notes();
                if branch == self.config.git.branch {
                    self.set_status(format!("cloned '{name}' from {redacted}"));
                } else {
                    self.set_status(format!(
                        "cloned '{name}' from {redacted} (branch '{branch}')"
                    ));
                }
            }
            Err(e) => self.set_status(format!(
                "created '{name}' and set remote, but pull failed: {e}"
            )),
        }
    }

    /// New-notebook fast path for pointing at an existing directory on disk
    /// (`/abs/path`, `~/docs`, `./relative`) instead of creating a fresh
    /// empty one or cloning a URL — derives the name from the last path
    /// segment (same idea as `notebook_name_from_git_url`), and, if the
    /// directory has no `.git` yet, asks for confirmation before
    /// initializing one rather than silently adopting a non-git-managed
    /// folder: every other notebook in this app is git-managed from
    /// creation, and letting one in without a repo would break sync/push/
    /// pull the moment something tried to act on it.
    pub(crate) fn adopt_notebook_from_path(&mut self, raw: &str) {
        let path = expand_home(raw);
        if !path.is_dir() {
            self.set_status(format!("'{}' is not a directory", path.display()));
            return;
        }
        let Some(name) = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|n| !n.is_empty())
        else {
            self.set_status(format!(
                "could not derive a notebook name from '{}'",
                path.display()
            ));
            return;
        };
        if self.store.get(&name).is_ok() {
            self.set_status(format!("notebook '{name}' already exists"));
            return;
        }
        if path.join(".git").is_dir() {
            self.finish_notebook_adopt(name, path, "adopted");
        } else {
            self.pending_notebook_adopt = Some((name, path.clone()));
            self.confirm = Some(crate::confirm::ConfirmDialog::new(format!(
                "'{}' has no git repo — initialize one and adopt it as a notebook?",
                path.display()
            )));
        }
    }

    /// Shared tail of adoption — used both when `.git` was already there
    /// (no confirmation needed) and after `handle_confirm_key` just ran
    /// `git::init_repo` on a confirmed one. Registers the custom path both
    /// in the live `NotebookStore` (so it shows up without restarting) and
    /// in `config.toml`'s `[notebooks.<name>] path = "..."` — the same field
    /// `Config::notebook_custom_paths` already reads for the pre-existing
    /// "point at an Obsidian vault subfolder" use case, so nothing new had
    /// to be added on the config side for this to persist across restarts.
    pub(crate) fn finish_notebook_adopt(
        &mut self,
        name: String,
        path: std::path::PathBuf,
        verb: &str,
    ) {
        self.store.custom_paths.insert(name.clone(), path.clone());
        self.config.notebooks.entry(name.clone()).or_default().path =
            Some(path.to_string_lossy().to_string());
        self.save_config();
        self.reload_notebooks();
        if let Some(idx) = self.notebooks.iter().position(|n| n.name == name) {
            self.selected_notebook = idx;
            self.reload_notes();
        }
        self.set_status(format!("{verb} '{name}' from {}", path.display()));
    }

    pub(crate) fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Notebooks => {
                if !self.notebooks.is_empty() {
                    self.selected_notebook =
                        shift(self.selected_notebook, delta, self.notebooks.len());
                    self.notes_path.clear();
                    self.reload_notes();
                }
            }
            Focus::Notes => {
                let len = self.combined_len();
                if len > 0 {
                    self.selected_note = shift(self.selected_note, delta, len);
                    self.preview_scroll = 0;
                }
            }
            // No list to navigate here — reuse the same keys to scroll the note instead.
            Focus::Preview => {
                let amount = delta.unsigned_abs() as u16;
                self.preview_scroll = if delta > 0 {
                    self.preview_scroll.saturating_add(amount)
                } else {
                    self.preview_scroll.saturating_sub(amount)
                };
            }
        }
    }

    /// `Home`/`Ctrl+Home`-style jump to the very first item — first
    /// notebook, first note, or the top of the note in PREVIEW.
    pub(crate) fn jump_to_start(&mut self) {
        match self.focus {
            Focus::Notebooks => self.move_selection(-(self.selected_notebook as isize)),
            Focus::Notes => self.move_selection(-(self.selected_note as isize)),
            Focus::Preview => self.preview_scroll = 0,
        }
    }

    /// `End`-style jump to the very last item — last notebook, last note, or
    /// (approximately) the bottom of the note in PREVIEW. The PREVIEW case
    /// clamps against the panel's actual visible height (via `layout::split`
    /// on `last_frame_area`, the same layout `draw()` uses) so it lands at
    /// the last screenful instead of overshooting into blank space the way
    /// scrolling straight to the raw source line count would; it can still
    /// slightly undershoot the true last *rendered* line for paragraphs
    /// that wrap across the panel width, since source line count doesn't
    /// account for wrapping (a `PageDown` or two closes the gap).
    pub(crate) fn jump_to_end(&mut self) {
        match self.focus {
            Focus::Notebooks => {
                if !self.notebooks.is_empty() {
                    let last =
                        (self.notebooks.len() - 1) as isize - self.selected_notebook as isize;
                    self.move_selection(last);
                }
            }
            Focus::Notes => {
                let len = self.combined_len();
                if len > 0 {
                    self.move_selection((len - 1) as isize - self.selected_note as isize);
                }
            }
            Focus::Preview => {
                let total_lines = self
                    .selected_note()
                    .map(|n| n.body.lines().count() as u16)
                    .unwrap_or(0);
                let content_height = layout::split(self.last_frame_area, self.focus)
                    .preview
                    .height
                    .saturating_sub(2);
                self.preview_scroll = total_lines.saturating_sub(content_height);
            }
        }
    }

    /// Yazi-style "go deeper": into a folder if one's selected in NOTES,
    /// otherwise the normal panel-to-panel forward move.
    pub(crate) fn navigate_forward(&mut self) {
        if self.focus == Focus::Notes {
            if let Some(folder) = self.selected_folder() {
                self.notes_path.push(folder.to_string());
                self.reload_notes();
                return;
            }
        }
        self.focus = self.focus.forward();
    }

    /// Yazi-style "go back": up one folder level if NOTES is inside one,
    /// otherwise the normal panel-to-panel backward move.
    pub(crate) fn navigate_backward(&mut self) {
        if self.focus == Focus::Notes && !self.notes_path.is_empty() {
            self.notes_path.pop();
            self.reload_notes();
            return;
        }
        self.focus = self.focus.backward();
    }

    pub(crate) fn start_input(&mut self, kind: PendingInput, prefill: String) {
        self.input.value = prefill;
        self.pending_input = Some(kind);
        self.mode = Mode::Insert;
    }

    /// The editor mode actually in effect right now: the resolved favorite
    /// editor's bare binary name when `use_favorite_editor` is on (falling
    /// back to the configured `general.editor` if none could be detected,
    /// matching `Action::EditInline`'s own fallback so this never claims a
    /// mode that isn't what would really happen), or `"native"` — the
    /// built-in inline editor — when it's off.
    pub fn editor_status_label(&self) -> String {
        if self.config.general.use_favorite_editor {
            let editor = self
                .favorite_editor
                .as_deref()
                .unwrap_or(&self.config.general.editor);
            editor
                .split_whitespace()
                .next()
                .unwrap_or(editor)
                .to_string()
        } else {
            "native".to_string()
        }
    }

    /// Flips `general.use_favorite_editor` and persists it immediately, so
    /// switching between the built-in editor and the OS favorite doesn't
    /// require hand-editing config.toml.
    pub(crate) fn toggle_favorite_editor(&mut self) {
        self.config.general.use_favorite_editor = !self.config.general.use_favorite_editor;
        if let Ok(path) = Config::default_path() {
            let _ = self.config.save(&path);
        }
        self.set_status(format!("favorite editor: {}", self.editor_status_label()));
    }

    /// Swaps in a freshly (re)loaded `Config` and re-derives everything
    /// that was computed from it once at startup (`App::new`) — the theme,
    /// the keymaps, and the detected favorite editor — so editing
    /// `config.toml` through the Settings screen takes effect immediately,
    /// the same "no restart needed" experience the theme picker already
    /// gives for just the `[theme]` table. `theme_index` is re-derived too,
    /// so a later `leader+c` (theme picker) opens positioned on whatever
    /// theme is now actually active instead of the one from before the edit.
    /// Re-reads `config.toml` from disk and applies it — used after an
    /// external editor (`E` from Settings) has just written to `path`
    /// directly, the same way `refresh_notes_preserve_selection` re-reads a
    /// note after external-editing it. A parse error (invalid TOML,
    /// something hand-typed wrong) is reported and the *previous* in-memory
    /// config is kept running rather than applying a broken one — same
    /// "never crash on a bad file" stance `Notebook::list_notes` already
    /// takes for a malformed note.
    pub(crate) fn reload_config_from_disk(&mut self, path: &std::path::Path) {
        match std::fs::read_to_string(path).map(|s| Config::parse(&s)) {
            Ok(Ok(new_config)) => {
                self.apply_config(new_config);
                self.set_status("config reloaded".into());
            }
            Ok(Err(e)) => self.set_status(format!("config not reloaded — invalid TOML: {e}")),
            Err(e) => self.set_status(format!("config not reloaded — {e}")),
        }
    }

    pub(crate) fn apply_config(&mut self, new_config: Config) {
        self.theme = new_config.theme.resolve();
        self.theme_index = self
            .available_themes
            .iter()
            .position(|t| t.name == self.theme.name)
            .unwrap_or(0);
        self.keymaps = KeyMaps::from_config(&new_config.keybindings);
        self.favorite_editor = shiki_core::editor::detect_favorite_editor();
        self.config = new_config;
        // Both caches key on the theme's colors too, so a reload that
        // doesn't actually change the theme would still hit correctly even
        // without this — but that's an accident of the cache key, not a
        // documented exemption, and a future cache keyed differently would
        // silently break on a config reload. Clear both unconditionally,
        // same as every other place that can change what PREVIEW should
        // show underneath an unchanged path (`reload_notes`/
        // `refresh_notes_preserve_selection`).
        self.folder_preview_cache = None;
        self.note_preview_cache = None;
    }

    /// Resolves the selected note's path relative to its notebook's root —
    /// what `shiki_core::git::file_history`/`show_file_at`/`revert_file_to`
    /// need, since git works in repo-relative paths.
    pub(crate) fn selected_note_relative_path(&self) -> Option<(Notebook, std::path::PathBuf)> {
        let nb = self.selected_notebook()?.clone();
        let note = self.selected_note()?;
        let relative = note.path.strip_prefix(&nb.path).ok()?.to_path_buf();
        Some((nb, relative))
    }

    /// Keeps the footer's "{n} changes" indicator up to date without
    /// re-walking the note's git history on every draw tick — only when the
    /// selected note has actually changed since the last check. Called once
    /// per `run()` loop iteration, right before drawing.
    fn refresh_history_cache(&mut self) {
        let current_path = self.selected_note().map(|n| n.path.clone());
        let Some(current_path) = current_path else {
            self.history_count_cache = None;
            return;
        };
        if self.history_count_cache.as_ref().map(|(p, _)| p) == Some(&current_path) {
            return;
        }
        let count = self
            .selected_note_relative_path()
            .and_then(|(nb, relative)| shiki_core::git::file_history(&nb.path, &relative).ok())
            .map(|revisions| revisions.len())
            .unwrap_or(0);
        self.history_count_cache = Some((current_path, count));
    }

    /// Keeps the PREVIEW panel's folder peek up to date without re-listing
    /// the directory (and re-parsing every note's frontmatter in it), *and*
    /// without re-formatting the resulting `Line`s (`format!`-ing a name or
    /// title per row) on every draw tick — only when the selected folder or
    /// the active theme's colors have actually changed since the last
    /// check. Formatting a few entries is cheap, but a folder with tens of
    /// thousands of notes made re-running it ~10x/second a real, measured
    /// CPU cost (caught by `scripts/benchmark.sh`'s `big-folder-100k`
    /// scenario) even after the underlying listing itself was cached.
    /// Called once per `run()` loop iteration, right before drawing, same
    /// spot as `refresh_history_cache`/`refresh_note_preview_cache`.
    fn refresh_folder_preview_cache(&mut self) {
        let Some(folder) = self.selected_folder().map(str::to_owned) else {
            self.folder_preview_cache = None;
            return;
        };
        let Some(nb_path) = self.selected_notebook().map(|nb| nb.path.clone()) else {
            self.folder_preview_cache = None;
            return;
        };
        let relative = self.notes_relative_path();
        let current_key = nb_path.join(&relative).join(&folder);
        let colors = [
            hex_to_color(&self.theme.fg),
            hex_to_color(&self.theme.accent),
            hex_to_color(&self.theme.muted),
            hex_to_color(&self.theme.link),
        ];
        if self
            .folder_preview_cache
            .as_ref()
            .is_some_and(|(p, c, _)| *p == current_key && *c == colors)
        {
            return;
        }
        let sub_path = relative.join(&folder);
        let (subfolders, notes) = self
            .selected_notebook()
            .and_then(|nb| nb.list_dir(&sub_path).ok())
            .unwrap_or_default();
        let note_titles: Vec<String> = notes.into_iter().map(|n| n.frontmatter.title).collect();
        let lines = panel_preview::format_folder_entries(
            &subfolders,
            &note_titles,
            colors[0],
            colors[1],
            colors[2],
        );
        self.folder_preview_cache = Some((current_key, colors, lines));
    }

    /// The cached, already-formatted lines for whichever folder is
    /// currently selected (not entered) in NOTES, for the PREVIEW panel's
    /// peek — `None` if no folder is selected or the cache hasn't caught up
    /// yet (the very next draw tick fills it in).
    pub(crate) fn folder_preview_lines(&self) -> Option<&[Line<'static>]> {
        self.folder_preview_cache
            .as_ref()
            .map(|(_, _, lines)| lines.as_slice())
    }

    /// Keeps the PREVIEW panel's note view up to date without re-running
    /// `markdown_to_lines` (a full line-by-line scan of the note body) on
    /// every draw tick — only when the selected note, the active theme's
    /// colors, or the panel's content width have actually changed since the
    /// last check. Called once per `run()` loop iteration, right before
    /// drawing, same spot as `refresh_history_cache`/`refresh_folder_preview_cache`.
    /// The cache stores rows already wrapped to `width` (`crate::wrap::wrap_lines`)
    /// rather than raw `markdown_to_lines` output — see `panel_preview::render`,
    /// which relies on that to display rows verbatim and to make
    /// `preview_scroll`/mouse hit-testing operate on exact row boundaries
    /// instead of `Paragraph`'s own internal (and unexposed) wrapping.
    fn refresh_note_preview_cache(&mut self) {
        let Some(note) = self.selected_note() else {
            self.note_preview_cache = None;
            return;
        };
        let path = note.path.clone();
        let colors = [
            hex_to_color(&self.theme.fg),
            hex_to_color(&self.theme.accent),
            hex_to_color(&self.theme.muted),
            hex_to_color(&self.theme.link),
        ];
        let width = layout::split(self.last_frame_area, self.focus)
            .preview
            .width
            .saturating_sub(2);
        if self
            .note_preview_cache
            .as_ref()
            .is_some_and(|(p, c, w, _, _)| *p == path && *c == colors && *w == width)
        {
            return;
        }
        let body = note.body.clone();
        let indexed = crate::render::markdown_to_lines_indexed(
            &body, colors[0], colors[1], colors[2], colors[3],
        );
        let (source_indices, plain_lines): (Vec<usize>, Vec<Line<'static>>) =
            indexed.into_iter().unzip();
        let grouped = crate::wrap::wrap_lines_grouped(&plain_lines, width);
        let mut lines = Vec::with_capacity(plain_lines.len());
        let mut sources = Vec::with_capacity(plain_lines.len());
        for (src, rows) in source_indices.into_iter().zip(grouped) {
            sources.extend(std::iter::repeat_n(src, rows.len()));
            lines.extend(rows);
        }
        self.note_preview_cache = Some((path, colors, width, lines, sources));
    }

    /// Keeps the tags modal's `TagIndex` up to date without rebuilding it on
    /// every draw tick while the modal is open — same "compute once, not per
    /// draw" discipline as `refresh_history_cache`/`refresh_folder_preview_cache`/
    /// `refresh_note_preview_cache`. A no-op while the modal is closed
    /// (`tag_index_cache` stays `None`, cleared by `reload_notes`/
    /// `refresh_notes_preserve_selection` whenever the underlying note list
    /// changes); rebuilt exactly once per "modal opened or notes changed"
    /// event, not per keystroke while browsing it.
    fn refresh_tag_index_cache(&mut self) {
        if !self.show_tags {
            self.tag_index_cache = None;
            return;
        }
        if self.tag_index_cache.is_some() {
            return;
        }
        self.tag_index_cache = Some(TagIndex::build(&self.notes));
    }

    /// The tags modal's tag index — always up to date by the time this is
    /// called, since `refresh_tag_index_cache` runs once per `run()`
    /// iteration before drawing, same spot as the other caches. Returns a
    /// borrowed reference rather than a clone: `panel_tags::render` and
    /// `draw()` both call this on every single draw tick while the modal is
    /// open, and cloning the whole `TagIndex` (a `BTreeMap<String,
    /// Vec<PathBuf>>`) on every one of those calls would silently reintroduce
    /// the exact per-draw-tick cost the cache exists to avoid, just one
    /// indirection later. The `unwrap_or_else` fallback (a shared empty
    /// index, not a rebuild) only matters if this is ever called before
    /// `refresh_tag_index_cache` has run at all — it never legitimately is,
    /// since both callers are gated on `show_tags`.
    pub(crate) fn tag_index(&self) -> &TagIndex {
        static EMPTY: std::sync::OnceLock<TagIndex> = std::sync::OnceLock::new();
        self.tag_index_cache
            .as_ref()
            .unwrap_or_else(|| EMPTY.get_or_init(TagIndex::default))
    }

    /// The cached, pre-wrapped formatted lines for whichever note is
    /// currently selected, for the PREVIEW panel — `None` if no note is
    /// selected or the cache hasn't caught up yet (the very next draw tick
    /// fills it in).
    pub(crate) fn note_preview_lines(&self) -> Option<&[Line<'static>]> {
        self.note_preview_cache
            .as_ref()
            .map(|(_, _, _, lines, _)| lines.as_slice())
    }

    /// The 0-based raw-source (`note.body.lines()`) index that rendered
    /// PREVIEW row `row` came from — `None` if there's no note preview
    /// cached yet or `row` is past the end of it. Used only by
    /// click-to-edit (`App::enter_edit_at_preview_row`) to resolve which
    /// line of the actual Markdown source a clicked, rendered-and-wrapped
    /// row corresponds to.
    pub(crate) fn note_preview_source_line(&self, row: usize) -> Option<usize> {
        self.note_preview_cache
            .as_ref()
            .and_then(|(_, _, _, _, sources)| sources.get(row).copied())
    }

    /// The cached revision count for whichever note is currently selected,
    /// for the footer — `None` when no note is selected at all (vs. `Some(0)`
    /// for a note that's never been committed).
    pub fn note_revision_count(&self) -> Option<usize> {
        let note = self.selected_note()?;
        match &self.history_count_cache {
            Some((path, count)) if path == &note.path => Some(*count),
            _ => None,
        }
    }

    /// How many `Note` rows are in `tree_rows` — the bound for `tree_selected`.
    pub(crate) fn tree_note_count(&self) -> usize {
        self.tree_rows
            .iter()
            .filter(|r| matches!(r, crate::tree::TreeRow::Note { .. }))
            .count()
    }

    /// The row index (into `tree_rows`, folders included) of the
    /// `tree_selected`-th note — what `ListState::select` needs to highlight
    /// the right visual row, since folder headers are interspersed.
    pub(crate) fn tree_selected_row(&self) -> Option<usize> {
        self.tree_rows
            .iter()
            .enumerate()
            .filter(|(_, r)| matches!(r, crate::tree::TreeRow::Note { .. }))
            .nth(self.tree_selected)
            .map(|(row, _)| row)
    }

    /// How many rows in `link_rows` are actually selectable (headers aren't).
    pub(crate) fn link_selectable_count(&self) -> usize {
        crate::links_panel::selectable_count(&self.link_rows)
    }

    /// Tags are scoped to the current directory's notes (`app.notes`), same
    /// as the tags modal always was — sorted, since `TagIndex` is backed by
    /// a `BTreeMap`, so this order is stable across redraws without storing
    /// the list on `App` itself.
    pub(crate) fn current_tags(&self) -> Vec<String> {
        self.tag_index().tags().cloned().collect()
    }

    /// Notes in the current directory carrying `tag` — every match is
    /// already in `self.notes`, since `current_tags` is scoped the same
    /// way, so jumping to one never needs `reload_notes`/`notes_path`.
    pub(crate) fn notes_with_tag<'a>(&'a self, tag: &str) -> Vec<&'a Note> {
        self.notes
            .iter()
            .filter(|n| n.frontmatter.tags.iter().any(|t| t == tag))
            .collect()
    }

    /// `"{notebook}/{breadcrumb}"` — the default move/copy target, editable
    /// down to just the notebook name (root) or out to a different
    /// notebook entirely by replacing the first segment. Always joined
    /// with a literal `/`, not `PathBuf`'s `Display` (which uses `\` on
    /// Windows) — the parser (`parse_move_target`) only ever splits on `/`.
    pub(crate) fn current_address(&self) -> String {
        let Some(nb) = self.selected_notebook() else {
            return String::new();
        };
        let mut segments = vec![nb.name.clone()];
        segments.extend(self.notes_path.iter().cloned());
        segments.join("/")
    }

    /// First segment is always a notebook name — it must already exist
    /// (never auto-created; a notebook is a new git repo, so creating one
    /// from a typo would be surprising). Everything after it is the
    /// destination folder within that notebook, auto-created as needed
    /// (same as `create_note_in`/`create_folder_in` already do) — not
    /// checked for existence up front, since creating it is always fine.
    pub(crate) fn parse_move_target(
        &self,
        value: &str,
    ) -> Result<(Notebook, std::path::PathBuf), String> {
        let mut parts = value.split('/').filter(|s| !s.is_empty());
        let notebook_name = parts.next().ok_or_else(|| "empty target".to_string())?;
        let dest_notebook = self
            .store
            .get(notebook_name)
            .map_err(|_| format!("notebook '{notebook_name}' not found"))?;
        let rest: std::path::PathBuf = parts.collect();
        Ok((dest_notebook, rest))
    }

    pub fn keymaps(&self) -> &KeyMaps {
        &self.keymaps
    }
}

/// Whether the new-notebook input looks like something to clone rather than
/// a plain name — covers the schemes/forms an `origin` remote can actually
/// take (`set_remote` accepts the same set): `https://`, `git@host:...`
/// (SSH scp-like syntax), `ssh://`, `git://`.
/// Notebook actions that operate on "the selected notebook" as a concept
/// rather than "the NOTEBOOKS panel" — safe to reach from any focus (see the
/// fallback in `handle_normal_key`). Intentionally excludes New/Rename/
/// Delete-notebook, which share letters with Notes-scope actions.
pub(crate) fn is_notebook_git_action(action: Action) -> bool {
    matches!(
        action,
        Action::SyncNotebook
            | Action::PullNotebook
            | Action::PullAllNotebooks
            | Action::SetRemote
            | Action::PushNotebook
    )
}

pub(crate) fn looks_like_git_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("git@")
        || s.starts_with("ssh://")
        || s.starts_with("git://")
}

/// Whether the new-notebook input looks like a filesystem path to adopt
/// rather than a plain name to create or a git URL to clone — `/absolute`,
/// `~`/`~/...` (home-relative), or `./relative`. None of these prefixes
/// overlap with `looks_like_git_url`'s, and a plain notebook name can never
/// start with `/` either (`validate_name` rejects any name containing one),
/// so checking this after `looks_like_git_url` is unambiguous.
pub(crate) fn looks_like_path(s: &str) -> bool {
    s.starts_with('/') || s.starts_with('~') || s.starts_with("./")
}

/// Expands a leading `~` (or `~/...`) to the user's home directory; anything
/// else — including a plain `/absolute` or `./relative` path — is returned
/// unchanged for the caller to resolve against the current directory itself.
pub(crate) fn expand_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
            let rest = rest.strip_prefix('/').unwrap_or(rest);
            return if rest.is_empty() {
                home
            } else {
                home.join(rest)
            };
        }
    }
    std::path::PathBuf::from(path)
}

/// Notebook name derived from a git URL's repo name — the last path
/// segment, minus a trailing `.git`. Handles both `.../owner/repo` (split on
/// `/`) and `git@host:owner/repo.git` (split on `:` for the host separator,
/// `/` for the owner/repo one) since splitting on either character and
/// taking the last piece lands on `repo[.git]` either way.
pub(crate) fn notebook_name_from_git_url(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty()).then(|| name.to_string())
}

pub(crate) fn shift(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = len as isize;
    let next = (current as isize + delta).rem_euclid(len);
    next as usize
}

/// The folder breadcrumb (as path components) of `note_path`'s containing
/// directory, relative to `notebook_path` — how far to descend to land on a
/// note found via search/jump instead of always assuming it's at the root.
pub(crate) fn relative_folder(
    note_path: &std::path::Path,
    notebook_path: &std::path::Path,
) -> Vec<String> {
    note_path
        .parent()
        .and_then(|dir| dir.strip_prefix(notebook_path).ok())
        .map(|rel| {
            rel.components()
                .filter_map(|c| c.as_os_str().to_str())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    area
}

/// Fixed width of the notebook drawer (`leader+b`) — narrow enough to leave
/// most of the screen for the 3-column layout underneath, wide enough for a
/// notebook name plus `↑3 ↓2 +5` worth of status.
pub const DRAWER_WIDTH: u16 = 30;

/// The notebook drawer's rect — left-anchored, not center-flexed like every
/// other popup here, since it's meant to read as a persistent sidebar
/// rather than a centered dialog. Excludes the bottom status-bar row so it
/// never paints over it. Shared by rendering and mouse hit-testing, same
/// reason `global_search_popup_area` is.
pub fn drawer_area(frame_area: Rect) -> Rect {
    Rect {
        x: frame_area.x,
        y: frame_area.y,
        width: DRAWER_WIDTH.min(frame_area.width),
        height: frame_area.height.saturating_sub(1),
    }
}

/// The global search modal's outer popup rect — shared by rendering and by
/// mouse hit-testing so they always agree on where things are.
pub fn global_search_popup_area(frame_area: Rect) -> Rect {
    centered_rect(
        frame_area,
        (frame_area.width * 3 / 4).max(40),
        (frame_area.height * 2 / 3).max(10),
    )
}

/// Splits the global search popup into (input box, results list).
pub fn global_search_layout(popup_area: Rect) -> (Rect, Rect) {
    let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(popup_area);
    (chunks[0], chunks[1])
}

pub fn run<B: Backend<Error = io::Error>>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    while !app.should_quit {
        if let Ok(size) = terminal.size() {
            app.last_frame_area = Rect::new(0, 0, size.width, size.height);
        }
        app.refresh_history_cache();
        app.refresh_folder_preview_cache();
        app.refresh_note_preview_cache();
        app.refresh_tag_index_cache();
        app.expire_status_message();
        app.poll_update_channel();
        app.poll_sync_channel();
        if app.sync_in_flight.is_some() {
            app.spinner_frame = app.spinner_frame.wrapping_add(1);
        }
        terminal.draw(|frame| crate::draw::draw(frame, app))?;

        if app.want_relaunch {
            if let Some(exe_path) = app.relaunch_exe_path.take() {
                relaunch_into_updated_binary(&exe_path)?;
            }
            app.should_quit = true;
            continue;
        }

        if let Some((path, editor)) = app.want_external_edit.take() {
            if app.want_external_edit_config {
                app.want_external_edit_config = false;
                suspend_and_edit(terminal, &editor, &path)?;
                app.reload_config_from_disk(&path);
            } else {
                let notebook_name = app.selected_notebook().map(|nb| nb.name.clone());
                suspend_and_edit(terminal, &editor, &path)?;
                app.refresh_notes_preserve_selection();
                if let Some(notebook_name) = notebook_name {
                    app.note_changed(&notebook_name);
                }
            }
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    // Accept Press and Repeat, only drop Release — some
                    // terminals report held/fast-typed keys as Repeat rather
                    // than Press, and filtering to `== Press` was silently
                    // swallowing those.
                    if key.kind != KeyEventKind::Release {
                        app.on_key(key);
                    }
                }
                Event::Mouse(mouse) => app.on_mouse(mouse),
                Event::Paste(text) => app.on_paste(text),
                _ => {}
            }
        }
    }
    app.save_session();
    Ok(())
}

/// Leaves the alternate screen (same teardown half as `suspend_and_edit`,
/// deliberately without the restore half — this process is exiting, not
/// resuming) and spawns the just-installed binary at the same path
/// (`install_latest` replaced it in place) so the update feels like a
/// restart rather than "go run `shiki` again yourself".
fn relaunch_into_updated_binary(exe_path: &std::path::Path) -> io::Result<()> {
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        io::stdout(),
        crossterm::event::DisableBracketedPaste,
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    let _ = std::process::Command::new(exe_path).spawn();
    Ok(())
}

/// Leaves the alternate screen and disables raw mode so `$EDITOR` gets a
/// normal terminal, then restores everything and forces a full redraw.
fn suspend_and_edit<B: Backend<Error = io::Error>>(
    terminal: &mut Terminal<B>,
    editor: &str,
    path: &std::path::Path,
) -> io::Result<()> {
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        io::stdout(),
        crossterm::event::DisableBracketedPaste,
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    let _ = shiki_core::editor::command_for(editor, path).status();
    crossterm::execute!(
        io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
        crossterm::event::EnableBracketedPaste
    )?;
    crossterm::terminal::enable_raw_mode()?;
    terminal.clear()
}

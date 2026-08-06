use crossterm::event::KeyCode;
use shiki_config::config::Keybindings as KeybindingsConfig;
use std::collections::HashMap;

use crate::app::Focus;

/// Every action reachable from a keybinding, across every scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // Global (leader-prefixed)
    ThemePicker,
    GlobalSearch,
    ToggleTags,
    ShowLogs,
    ToggleFavoriteEditor,
    CheckForUpdate,
    ToggleDrawer,
    /// Restores the most recently deleted note/folder (or batch) from the
    /// trash — a single level of undo.
    UndoDelete,
    /// Opens the Settings screen — a read-only summary of the current
    /// config, with a key to jump straight to editing `config.toml` itself.
    ToggleSettings,
    Scratchpad,
    /// Opens the global tasks view: every `- [ ]` checkbox across every
    /// notebook, toggleable in place without opening the note.
    ToggleTasks,
    /// Opens the query modal: a Dataview-style `where ... sort ...` DSL
    /// over frontmatter, filtered/sorted live across every notebook.
    ToggleQuery,
    /// Renders the selected notebook to a themed PDF via `pretty-pdf`
    /// (go-pretty-pdf), then opens it.
    PublishNotebook,
    /// Exports the selected notebook to a single HTML or Markdown bundle —
    /// the same rendering `shiki export` (CLI) uses.
    ExportNotebook,
    /// Forces the full-screen single-panel layout regardless of terminal
    /// size, hiding NOTEBOOKS/NOTES for distraction-free writing.
    ToggleZenMode,
    // Notebooks-focus
    NewNotebook,
    RenameNotebook,
    DeleteNotebook,
    SyncNotebook,
    PullNotebook,
    PullAllNotebooks,
    SetRemote,
    PushNotebook,
    // Notes-focus
    NewNote,
    NewFolder,
    RenameNote,
    DeleteNote,
    JumpSearch,
    DailyNote,
    MoveNote,
    SortNotes,
    ToggleTreeView,
    ToggleDates,
    /// Enters `Mode::Visual` (vi-style multi-select), anchored at whatever's
    /// currently selected — `j`/`k` extend the range, `Esc` cancels.
    ToggleVisual,
    /// `Mode::Visual`-only: copies every selected note/folder to a prompted
    /// target, leaving the originals in place. A no-op outside Visual mode
    /// (nothing to copy without a selection) — there's no single-item
    /// duplicate shortcut, only the batch one.
    CopyEntries,
    // Notes- and Preview-focus
    EditInline,
    EditExternal,
    // Preview-focus
    ShowHistory,
    /// Opens the links modal — the selected note's outgoing `[[wikilinks]]`
    /// plus every other note that links back to it.
    ShowLinks,
    /// Opens the outline modal — every `#`..`######` heading in the
    /// selected note, jump straight to one. Also reachable as `Ctrl+O`
    /// inside `Mode::Edit` itself (bound directly in `handle_edit_key`,
    /// not through this scoped map).
    ShowOutline,
}

/// Translates a config string (e.g. `"enter"`, `"tab"`, `"a"`, `"space"`) into a `KeyCode`.
///
/// Matching is done on `KeyCode` alone, deliberately ignoring modifiers:
/// crossterm reports typed uppercase letters as `Char('A')` with the `SHIFT`
/// modifier set, so comparing full `KeyEvent`s against a hardcoded
/// `KeyModifiers::NONE` would silently drop every Shift-based binding.
///
/// `pub`, not just `pub(crate)`: `shiki doctor` (in `shiki-cli`) calls this
/// directly to detect two config fields in the same scope that resolve to
/// the same actual key (e.g. `"space"` and `" "`, not just literal string
/// duplicates) — that's real collision detection `bind`'s silent
/// last-one-wins `HashMap::insert` can't surface on its own.
pub fn parse_key(spec: &str) -> Option<KeyCode> {
    match spec.to_ascii_lowercase().as_str() {
        "enter" => Some(KeyCode::Enter),
        "tab" => Some(KeyCode::Tab),
        "esc" | "escape" => Some(KeyCode::Esc),
        "space" => Some(KeyCode::Char(' ')),
        "backspace" => Some(KeyCode::Backspace),
        s if s.chars().count() == 1 => Some(KeyCode::Char(spec.chars().next()?)),
        _ => None,
    }
}

fn bind(map: &mut HashMap<KeyCode, Action>, spec: &str, action: Action) {
    if let Some(code) = parse_key(spec) {
        map.insert(code, action);
    }
}

/// The full set of keymaps, one per scope. Navigation (`hjkl`, arrows, `tab`,
/// `enter`, `?`) isn't here — it's hardcoded in `app.rs` since it behaves the
/// same regardless of config. Everything else resolves against whichever
/// scope applies: `quit` is bare and universal, `global` requires the
/// leader key first, and `notebooks`/`notes`/`preview` are only consulted
/// while that panel has focus.
pub struct KeyMaps {
    leader: KeyCode,
    quit: KeyCode,
    global: HashMap<KeyCode, Action>,
    notebooks: HashMap<KeyCode, Action>,
    notes: HashMap<KeyCode, Action>,
    preview: HashMap<KeyCode, Action>,
}

impl KeyMaps {
    pub fn from_config(cfg: &KeybindingsConfig) -> Self {
        let leader = parse_key(&cfg.leader).unwrap_or(KeyCode::Char(' '));
        let quit = parse_key(&cfg.quit).unwrap_or(KeyCode::Char('q'));

        let mut global = HashMap::new();
        bind(&mut global, &cfg.global.theme_picker, Action::ThemePicker);
        bind(&mut global, &cfg.global.global_search, Action::GlobalSearch);
        bind(&mut global, &cfg.global.tags_panel, Action::ToggleTags);
        bind(&mut global, &cfg.global.logs, Action::ShowLogs);
        bind(
            &mut global,
            &cfg.global.toggle_favorite_editor,
            Action::ToggleFavoriteEditor,
        );
        bind(
            &mut global,
            &cfg.global.check_update,
            Action::CheckForUpdate,
        );
        bind(&mut global, &cfg.global.drawer, Action::ToggleDrawer);
        bind(&mut global, &cfg.global.undo_delete, Action::UndoDelete);
        bind(&mut global, &cfg.global.settings, Action::ToggleSettings);
        bind(&mut global, &cfg.global.scratchpad, Action::Scratchpad);
        // The same links modal PREVIEW's own binding opens — global so
        // "what links here?" is answerable without focusing PREVIEW first.
        bind(&mut global, &cfg.global.links, Action::ShowLinks);
        bind(&mut global, &cfg.global.tasks_panel, Action::ToggleTasks);
        bind(&mut global, &cfg.global.query_panel, Action::ToggleQuery);
        bind(&mut global, &cfg.global.publish, Action::PublishNotebook);
        bind(&mut global, &cfg.global.export, Action::ExportNotebook);
        bind(&mut global, &cfg.global.zen_mode, Action::ToggleZenMode);

        let mut notebooks = HashMap::new();
        bind(&mut notebooks, &cfg.notebooks.new, Action::NewNotebook);
        bind(
            &mut notebooks,
            &cfg.notebooks.rename,
            Action::RenameNotebook,
        );
        bind(
            &mut notebooks,
            &cfg.notebooks.delete,
            Action::DeleteNotebook,
        );
        bind(&mut notebooks, &cfg.notebooks.sync, Action::SyncNotebook);
        bind(&mut notebooks, &cfg.notebooks.pull, Action::PullNotebook);
        bind(
            &mut notebooks,
            &cfg.notebooks.pull_all,
            Action::PullAllNotebooks,
        );
        bind(&mut notebooks, &cfg.notebooks.set_remote, Action::SetRemote);
        bind(&mut notebooks, &cfg.notebooks.push, Action::PushNotebook);

        let mut notes = HashMap::new();
        bind(&mut notes, &cfg.notes.new, Action::NewNote);
        bind(&mut notes, &cfg.notes.new_folder, Action::NewFolder);
        bind(&mut notes, &cfg.notes.rename, Action::RenameNote);
        bind(&mut notes, &cfg.notes.delete, Action::DeleteNote);
        bind(&mut notes, &cfg.notes.edit_inline, Action::EditInline);
        bind(&mut notes, &cfg.notes.edit_external, Action::EditExternal);
        bind(&mut notes, &cfg.notes.search, Action::JumpSearch);
        bind(&mut notes, &cfg.notes.daily_note, Action::DailyNote);
        bind(&mut notes, &cfg.notes.move_to_notebook, Action::MoveNote);
        bind(&mut notes, &cfg.notes.sort, Action::SortNotes);
        bind(&mut notes, &cfg.notes.tree_view, Action::ToggleTreeView);
        bind(&mut notes, &cfg.notes.toggle_dates, Action::ToggleDates);
        bind(&mut notes, &cfg.notes.visual, Action::ToggleVisual);
        bind(&mut notes, &cfg.notes.copy_entries, Action::CopyEntries);

        let mut preview = HashMap::new();
        bind(&mut preview, &cfg.preview.edit_inline, Action::EditInline);
        bind(
            &mut preview,
            &cfg.preview.edit_external,
            Action::EditExternal,
        );
        bind(&mut preview, &cfg.preview.history, Action::ShowHistory);
        bind(&mut preview, &cfg.preview.links, Action::ShowLinks);
        bind(&mut preview, &cfg.preview.outline, Action::ShowOutline);

        Self {
            leader,
            quit,
            global,
            notebooks,
            notes,
            preview,
        }
    }

    pub fn is_leader(&self, code: KeyCode) -> bool {
        code == self.leader
    }

    pub fn leader_key(&self) -> KeyCode {
        self.leader
    }

    pub fn is_quit(&self, code: KeyCode) -> bool {
        code == self.quit
    }

    pub fn resolve_global(&self, code: KeyCode) -> Option<Action> {
        self.global.get(&code).copied()
    }

    /// Resolves `code` against whichever scope's map matches `focus`.
    pub fn resolve_scoped(&self, focus: Focus, code: KeyCode) -> Option<Action> {
        let map = match focus {
            Focus::Notebooks => &self.notebooks,
            Focus::Notes => &self.notes,
            Focus::Preview => &self.preview,
        };
        map.get(&code).copied()
    }

    /// (scope label, key description, action) for the which-key popup,
    /// grouped by scope then sorted by key within each group.
    pub fn entries(&self) -> Vec<(&'static str, String, Action)> {
        let mut out = Vec::new();
        for (code, action) in &self.global {
            out.push(("GLOBAL (leader)", describe_key(*code), *action));
        }
        for (code, action) in &self.notebooks {
            out.push(("NOTEBOOKS", describe_key(*code), *action));
        }
        for (code, action) in &self.notes {
            out.push(("NOTES", describe_key(*code), *action));
        }
        for (code, action) in &self.preview {
            out.push(("PREVIEW", describe_key(*code), *action));
        }
        out.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(&b.1)));
        out
    }

    /// Core navigation/quit rows for the which-key popup — `hjkl`/arrows/
    /// `Tab`/`Enter`/`?`/quit are hardcoded in `handle_normal_key` (they
    /// behave identically in every scope, so they were never given `Action`
    /// entries of their own), which meant `entries()` — and therefore the
    /// one built-in help screen — omitted the most-used keys in the app
    /// entirely. These aren't real `Action`s (nothing to dispatch — `Enter`
    /// on one of these rows in the which-key modal is a no-op, see
    /// `WhichKeyRow`), just documentation of keys that already work.
    pub fn nav_rows(&self) -> Vec<(&'static str, String, &'static str)> {
        vec![
            ("NAVIGATION", "j / ↓".into(), "move down"),
            ("NAVIGATION", "k / ↑".into(), "move up"),
            ("NAVIGATION", "PageDown".into(), "move down a page"),
            ("NAVIGATION", "PageUp".into(), "move up a page"),
            ("NAVIGATION", "Home".into(), "jump to the top"),
            ("NAVIGATION", "End".into(), "jump to the bottom"),
            (
                "NAVIGATION",
                "l / → / enter".into(),
                "open (a folder, or the next panel)",
            ),
            (
                "NAVIGATION",
                "h / ←".into(),
                "back (up a folder, or the previous panel)",
            ),
            ("NAVIGATION", "Tab".into(), "switch panel"),
            ("NAVIGATION", "?".into(), "this help / command palette"),
            ("NAVIGATION", describe_key(self.quit), "quit shiki"),
        ]
    }
}

/// A row in the which-key popup: either a real, dispatchable `Action`, or a
/// purely informational hardcoded navigation key (see `KeyMaps::nav_rows`)
/// that `Enter` can't act on since there's no `Action` behind it.
#[derive(Debug, Clone)]
pub enum WhichKeyRow {
    Bound {
        scope: &'static str,
        key: String,
        action: Action,
    },
    Nav {
        scope: &'static str,
        key: String,
        label: &'static str,
    },
    /// A note matching the current filter, from a fuzzy search across every
    /// notebook (`App.global_search_pool`, same pool/scoring the standalone
    /// global search modal uses) — only ever appended while the filter is
    /// non-empty, so an empty which-key still reads as "browse every
    /// keybinding" and doesn't also dump every note in every notebook.
    /// `pool_index` is the row's index into `global_search_pool`, resolved
    /// by `App::jump_to_global_hit` on `Enter`, the same jump global
    /// search's own `Enter` uses.
    NoteHit { pool_index: usize, label: String },
}

impl WhichKeyRow {
    pub fn scope(&self) -> &'static str {
        match self {
            WhichKeyRow::Bound { scope, .. } | WhichKeyRow::Nav { scope, .. } => scope,
            WhichKeyRow::NoteHit { .. } => "notes",
        }
    }

    pub fn key(&self) -> &str {
        match self {
            WhichKeyRow::Bound { key, .. } | WhichKeyRow::Nav { key, .. } => key,
            WhichKeyRow::NoteHit { .. } => "",
        }
    }

    pub fn label(&self) -> &str {
        match self {
            WhichKeyRow::Bound { action, .. } => action_label(*action),
            WhichKeyRow::Nav { label, .. } => label,
            WhichKeyRow::NoteHit { label, .. } => label,
        }
    }

    pub fn icon(&self) -> crate::icons::Icon {
        match self {
            WhichKeyRow::Bound { action, .. } => action_icon(*action),
            WhichKeyRow::Nav { .. } => crate::icons::ARROW,
            WhichKeyRow::NoteHit { .. } => crate::icons::NOTE,
        }
    }
}

pub fn describe_key(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "space".into(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Backspace => "backspace".into(),
        other => format!("{other:?}").to_lowercase(),
    }
}

pub fn action_label(action: Action) -> &'static str {
    match action {
        Action::ThemePicker => "pick theme",
        Action::GlobalSearch => "search all notes",
        Action::ToggleTags => "tags panel",
        Action::ShowLogs => "view logs",
        Action::ToggleFavoriteEditor => "toggle favorite editor",
        Action::CheckForUpdate => "check for update",
        Action::ToggleDrawer => "toggle notebook drawer",
        Action::UndoDelete => "undo last delete",
        Action::ToggleSettings => "settings (view + edit config.toml)",
        Action::Scratchpad => "open scratchpad",
        Action::NewNotebook => "new notebook",
        Action::RenameNotebook => "rename notebook",
        Action::DeleteNotebook => "delete notebook",
        Action::SyncNotebook => "git sync",
        Action::PullNotebook => "git pull",
        Action::PullAllNotebooks => "git pull (all notebooks)",
        Action::SetRemote => "set git remote",
        Action::PushNotebook => "sync + push now (ignores auto_push)",
        Action::NewNote => "new note",
        Action::NewFolder => "new folder",
        Action::RenameNote => "rename note",
        Action::DeleteNote => "delete note",
        Action::JumpSearch => "jump to note (fuzzy)",
        Action::DailyNote => "daily note",
        Action::MoveNote => "move to notebook",
        Action::SortNotes => "cycle sort order",
        Action::ToggleTreeView => "notebook tree (all notes)",
        Action::ToggleDates => "toggle note dates in list",
        Action::ToggleVisual => "select mode (multi-select)",
        Action::CopyEntries => "copy selection to… (visual mode)",
        Action::EditInline => "edit (insert mode)",
        Action::EditExternal => "edit externally ($EDITOR)",
        Action::ShowHistory => "note history (view/revert)",
        Action::ShowLinks => "links (outgoing / backlinks / mentions)",
        Action::ToggleTasks => "tasks (all notebooks)",
        Action::ToggleQuery => "query notes (frontmatter filter/sort)",
        Action::PublishNotebook => "publish notebook to PDF",
        Action::ExportNotebook => "export notebook to HTML/Markdown",
        Action::ToggleZenMode => "zen mode (full-screen, hide side panels)",
        Action::ShowOutline => "outline (jump to a heading)",
    }
}

pub fn action_icon(action: Action) -> crate::icons::Icon {
    match action {
        Action::ThemePicker => crate::icons::EYE,
        Action::GlobalSearch | Action::JumpSearch => crate::icons::SEARCH,
        Action::ToggleTags => crate::icons::TAG,
        Action::ShowLogs => crate::icons::LIST,
        Action::ToggleFavoriteEditor => crate::icons::PENCIL,
        Action::CheckForUpdate => crate::icons::DOWNLOAD,
        Action::ToggleDrawer => crate::icons::COLUMNS,
        Action::NewNotebook | Action::NewNote => crate::icons::NOTE,
        Action::NewFolder => crate::icons::NOTEBOOK,
        Action::RenameNotebook | Action::RenameNote => crate::icons::PENCIL,
        Action::DeleteNotebook | Action::DeleteNote => crate::icons::WARNING,
        Action::SyncNotebook
        | Action::PullNotebook
        | Action::PullAllNotebooks
        | Action::SetRemote
        | Action::PushNotebook => crate::icons::GIT,
        Action::EditInline | Action::EditExternal => crate::icons::PENCIL,
        Action::DailyNote => crate::icons::CALENDAR,
        Action::MoveNote => crate::icons::ARROW,
        Action::SortNotes => crate::icons::COLUMNS,
        Action::ToggleTreeView => crate::icons::TREE,
        Action::ToggleDates | Action::ShowHistory => crate::icons::HISTORY,
        Action::ToggleVisual | Action::ToggleTasks => crate::icons::CHECK,
        Action::ToggleQuery => crate::icons::FILTER,
        Action::CopyEntries => crate::icons::CLIPBOARD,
        Action::ShowLinks => crate::icons::LINK,
        Action::UndoDelete => crate::icons::UNDO,
        Action::ToggleSettings => crate::icons::GEAR,
        Action::Scratchpad => crate::icons::PENCIL,
        Action::PublishNotebook => crate::icons::PDF,
        Action::ExportNotebook => crate::icons::NOTE,
        Action::ToggleZenMode => crate::icons::EXPAND,
        Action::ShowOutline => crate::icons::TREE,
    }
}

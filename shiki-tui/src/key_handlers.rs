use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use shiki_config::Config;
use shiki_core::{wikilinks, Note, Notebook};

use crate::app::{
    drawer_area, global_search_layout, global_search_popup_area, is_notebook_git_action,
    looks_like_git_url, looks_like_path, relative_folder, shift, App, BatchOp, ConflictView,
    DeleteTarget, EditorFindState, FindField, Focus, MetadataPrompt, Mode, PassphrasePurpose,
    PendingInput, PreviewSelection, QuerySuggestion, QuickCommand, SelectedEntry, TrashedEntry,
    UpdateMsg, UpdateState,
};
use crate::editor::InlineEditor;
use crate::icons;
use crate::input::InputBox;
use crate::keybindings::{Action, WhichKeyRow};
use crate::panel_query;
use crate::render::{hex_to_color, panel_block};
use crate::{
    confirm, layout, panel_drawer, panel_notebooks, panel_notes, panel_preview, slash_menu,
    status_bar,
};

/// Spaces added/removed per level by the editor's list indent/outdent
/// (`try_indent_list_line`/`indent_selected_lines`) — 2 lines up under a
/// `- `/`* `/`+ ` marker's own width, the same convention CommonMark's
/// nested-list rules already expect.
const LIST_INDENT_STEP: usize = 2;

/// Which whole side of a conflict to keep — v1's resolution model (see
/// `App::resolve_selected_conflict`'s doc comment for why line-level manual
/// merging isn't offered here).
enum ConflictSideChoice {
    Ours,
    Theirs,
}

impl App {
    fn open_theme_picker(&mut self) {
        self.theme_picker_index = self.theme_index;
        self.show_theme_picker = true;
    }
    fn handle_theme_picker_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                // Cancel: revert the live preview back to the theme that was active.
                if let Some(t) = self.available_themes.get(self.theme_index) {
                    self.theme = t.clone();
                }
                self.close_theme_picker();
            }
            KeyCode::Enter => {
                self.theme_index = self.theme_picker_index;
                // Only reset overrides when actually switching to a
                // different base theme — compared against `config.theme.name`
                // (the last *committed* value), not `self.theme.name` (the
                // live-preview value while browsing). Re-confirming the
                // theme that was already active with no real change used to
                // silently wipe any hand-written custom colors.
                if self.config.theme.name != self.theme.name {
                    self.config.theme.overrides = Default::default();
                }
                self.config.theme.name = self.theme.name.clone();
                if let Ok(path) = Config::default_path() {
                    let _ = self.config.save(&path);
                }
                self.set_status(format!("theme: {}", self.theme.name));
                self.close_theme_picker();
            }
            KeyCode::Char('j') | KeyCode::Down => self.preview_theme_at(1),
            KeyCode::Char('k') | KeyCode::Up => self.preview_theme_at(-1),
            _ => {}
        }
    }
    /// Shared by both the picker's cancel and confirm paths — reopens
    /// Settings only when this picker was opened *from* Settings (THEME's
    /// `name` row setting `reopen_settings_after_theme_picker` first), never
    /// for the normal standalone leader+`c` picker.
    fn close_theme_picker(&mut self) {
        self.show_theme_picker = false;
        if self.reopen_settings_after_theme_picker {
            self.reopen_settings_after_theme_picker = false;
            self.show_settings = true;
        }
    }
    /// Moves the picker cursor and immediately applies that theme so the
    /// whole UI re-themes live while browsing, before you've committed to it.
    fn preview_theme_at(&mut self, delta: isize) {
        if self.available_themes.is_empty() {
            return;
        }
        self.theme_picker_index =
            shift(self.theme_picker_index, delta, self.available_themes.len());
        if let Some(t) = self.available_themes.get(self.theme_picker_index) {
            self.theme = t.clone();
        }
    }
    fn open_logs(&mut self) {
        self.logs_selected = self.log_history.len().saturating_sub(1);
        self.show_logs = true;
    }
    /// Unlike `open_logs`/`open_tree` (one-directional, closed via `Esc`
    /// inside their own key handler), the drawer is a true toggle — pressing
    /// its leader binding again collapses it, matching how it was asked for
    /// ("abrir o descolapsar" with the same key).
    fn toggle_drawer(&mut self) {
        self.show_drawer = !self.show_drawer;
        if self.show_drawer {
            self.drawer_selected = self
                .selected_notebook
                .min(self.notebooks.len().saturating_sub(1));
            self.refresh_drawer_statuses();
        }
    }
    /// A true toggle like `toggle_drawer` above, not one-directional — the
    /// same `leader z` both enters and exits zen mode. Purely a layout
    /// flag (`layout::split` reads it); nothing about focus, selection, or
    /// which notes are loaded changes when it flips.
    fn toggle_zen_mode(&mut self) {
        self.zen_mode = !self.zen_mode;
        self.set_status(format!(
            "zen mode: {}",
            if self.zen_mode { "on" } else { "off" }
        ));
    }
    fn handle_drawer_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.show_drawer = false,
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.drawer_statuses.is_empty() {
                    self.drawer_selected = (self.drawer_selected + 1) % self.drawer_statuses.len();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if !self.drawer_statuses.is_empty() {
                    self.drawer_selected = self
                        .drawer_selected
                        .checked_sub(1)
                        .unwrap_or(self.drawer_statuses.len() - 1);
                }
            }
            KeyCode::Enter => self.jump_to_drawer_notebook(),
            // Both open the same `PendingInput::NewNotebook` prompt — it
            // already detects a pasted git URL and clones instead of
            // creating a plain notebook (`looks_like_git_url`), so "import"
            // isn't separate logic, just a second entry point into it.
            KeyCode::Char('n') | KeyCode::Char('i') => {
                self.show_drawer = false;
                self.start_input(PendingInput::NewNotebook, String::new());
            }
            _ => {}
        }
    }
    /// Jumps to whichever notebook is selected in the drawer — same
    /// `notes_path.clear()` + `reload_notes()` pair `move_selection` already
    /// uses when switching `selected_notebook` via `j`/`k` in NOTEBOOKS.
    fn jump_to_drawer_notebook(&mut self) {
        if let Some((name, _)) = self.drawer_statuses.get(self.drawer_selected) {
            if let Some(idx) = self.notebooks.iter().position(|nb| &nb.name == name) {
                self.selected_notebook = idx;
                self.notes_path.clear();
                self.reload_notes();
            }
        }
        self.show_drawer = false;
    }
    fn toggle_settings(&mut self) {
        self.show_settings = !self.show_settings;
        if self.show_settings {
            self.settings_section = crate::panel_settings::SettingsSection::General;
            self.settings_selected = 0;
            self.settings_notebook_drill = None;
            self.settings_snippet_drill = None;
            self.settings_field_selected = 0;
        }
    }
    /// Left/right always means "change tab," regardless of whether
    /// NOTEBOOKS/SNIPPETS is currently drilled into a specific item —
    /// checked before the drill-state branch in `handle_settings_key` so it
    /// works from either level, and always resets back to level 1 in the
    /// new tab rather than carrying a stale drill state into a section it
    /// doesn't apply to.
    fn switch_settings_section(&mut self, forward: bool) {
        self.settings_section = if forward {
            self.settings_section.next()
        } else {
            self.settings_section.prev()
        };
        self.settings_selected = 0;
        self.settings_notebook_drill = None;
        self.settings_snippet_drill = None;
        self.settings_field_selected = 0;
    }
    fn settings_row_count(&self) -> usize {
        crate::panel_settings::build(self).len()
    }
    fn handle_settings_key(&mut self, key: KeyEvent) {
        use crate::panel_settings::SettingsSection;
        match key.code {
            KeyCode::Left => {
                self.switch_settings_section(false);
                return;
            }
            KeyCode::Right => {
                self.switch_settings_section(true);
                return;
            }
            _ => {}
        }
        if self.settings_notebook_drill.is_some() {
            self.handle_settings_notebook_field_key(key);
            return;
        }
        if self.settings_snippet_drill.is_some() {
            self.handle_settings_snippet_field_key(key);
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.show_settings = false,
            KeyCode::Char('j') | KeyCode::Down => {
                let len = self.settings_row_count();
                if self.settings_selected + 1 < len {
                    self.settings_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.settings_selected = self.settings_selected.saturating_sub(1);
            }
            KeyCode::PageDown => {
                let len = self.settings_row_count();
                self.settings_selected =
                    (self.settings_selected + self.page_step() as usize).min(len.saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.settings_selected = self
                    .settings_selected
                    .saturating_sub(self.page_step() as usize);
            }
            KeyCode::Home => self.settings_selected = 0,
            KeyCode::End => self.settings_selected = self.settings_row_count().saturating_sub(1),
            // Every tab's `Enter`/`l` does whatever that tab's selected row
            // calls for — toggle a boolean, open a prompt, open the theme
            // picker, or drill into a notebook/snippet. Each section's own
            // handler decides which of those it is; see each fn's doc
            // comment for that section's specific field list.
            KeyCode::Enter | KeyCode::Char('l') => match self.settings_section {
                SettingsSection::General => self.handle_general_field_enter(),
                SettingsSection::Theme => self.handle_theme_field_enter(),
                SettingsSection::Git => self.handle_git_field_enter(),
                SettingsSection::Editor => self.handle_editor_field_enter(),
                SettingsSection::Export => self.handle_export_field_enter(),
                SettingsSection::Notebooks => {
                    let names = crate::panel_settings::sorted_notebook_names(self);
                    if let Some(name) = names.get(self.settings_selected) {
                        self.settings_notebook_drill = Some(name.clone());
                        self.settings_field_selected = 0;
                    }
                }
                SettingsSection::Snippets => {
                    let triggers = crate::panel_settings::sorted_snippet_triggers(self);
                    if let Some(trigger) = triggers.get(self.settings_selected) {
                        self.settings_snippet_drill = Some(trigger.clone());
                        self.settings_field_selected = 0;
                    }
                }
            },
            // SNIPPETS-only: create/delete a snippet at level 1. A no-op in
            // every other tab (nothing else in Settings has a variable-size
            // collection you'd want to add/remove entries from this way).
            KeyCode::Char('a') if self.settings_section == SettingsSection::Snippets => {
                self.start_new_snippet();
            }
            KeyCode::Char('d') if self.settings_section == SettingsSection::Snippets => {
                self.start_delete_snippet();
            }
            // Same `i`/`E` convention as editing a note: `i` respects
            // `use_favorite_editor` (native inline vs. the OS favorite),
            // `E` always uses the configured `general.editor`.
            KeyCode::Char('i') => {
                self.show_settings = false;
                if self.config.general.use_favorite_editor {
                    let editor = self
                        .favorite_editor
                        .clone()
                        .unwrap_or_else(|| self.config.general.editor.clone());
                    self.start_external_config_edit(editor);
                } else {
                    self.start_edit_config_inline();
                }
            }
            KeyCode::Char('E') => {
                self.show_settings = false;
                self.start_external_config_edit(self.config.general.editor.clone());
            }
            _ => {}
        }
    }
    /// GENERAL — `use_favorite_editor`/`mouse_drag_selection`/`show_hints`
    /// toggle in place; the three text fields open a single-line prompt
    /// (`PendingInput::SettingsGeneralText`, resolved back to a field via
    /// `GeneralField::ALL[settings_selected]` once it's confirmed).
    fn handle_general_field_enter(&mut self) {
        use crate::panel_settings::GeneralField;
        let field = GeneralField::ALL[self.settings_selected];
        if field == GeneralField::UseFavoriteEditor {
            self.config.general.use_favorite_editor = !self.config.general.use_favorite_editor;
            self.save_config();
            self.set_status(format!(
                "use_favorite_editor -> {}",
                self.config.general.use_favorite_editor
            ));
            return;
        }
        if field == GeneralField::EnableCaptureDaemon {
            let new_value = !self.config.general.enable_capture_daemon;
            self.set_capture_daemon_enabled(new_value);
            return;
        }
        if field == GeneralField::MouseDragSelection {
            self.config.general.mouse_drag_selection = !self.config.general.mouse_drag_selection;
            self.save_config();
            self.set_status(format!(
                "mouse_drag_selection -> {}",
                self.config.general.mouse_drag_selection
            ));
            return;
        }
        if field == GeneralField::ShowHints {
            self.config.general.show_hints = !self.config.general.show_hints;
            self.save_config();
            self.set_status(format!("show_hints -> {}", self.config.general.show_hints));
            return;
        }
        if field == GeneralField::RememberLastSession {
            self.config.general.remember_last_session = !self.config.general.remember_last_session;
            self.save_config();
            self.set_status(format!(
                "remember_last_session -> {}",
                self.config.general.remember_last_session
            ));
            return;
        }
        if field == GeneralField::ShowCoffeeLink {
            self.config.general.show_coffee_link = !self.config.general.show_coffee_link;
            self.save_config();
            self.set_status(format!(
                "show_coffee_link -> {}",
                self.config.general.show_coffee_link
            ));
            return;
        }
        if field == GeneralField::SkipDeleteConfirm {
            self.config.general.skip_delete_confirm = !self.config.general.skip_delete_confirm;
            self.save_config();
            self.set_status(format!(
                "skip_delete_confirm -> {}",
                self.config.general.skip_delete_confirm
            ));
            return;
        }
        if field == GeneralField::ShowDates {
            self.config.general.show_dates = !self.config.general.show_dates;
            self.show_dates = self.config.general.show_dates;
            self.save_config();
            self.set_status(format!("show_dates -> {}", self.config.general.show_dates));
            return;
        }
        if field == GeneralField::WikilinkAutocomplete {
            self.config.general.wikilink_autocomplete = !self.config.general.wikilink_autocomplete;
            self.save_config();
            self.set_status(format!(
                "wikilink_autocomplete -> {}",
                self.config.general.wikilink_autocomplete
            ));
            return;
        }
        if field == GeneralField::DailyAgenda {
            self.config.general.daily_agenda = !self.config.general.daily_agenda;
            self.save_config();
            self.set_status(format!(
                "daily_agenda -> {}",
                self.config.general.daily_agenda
            ));
            return;
        }
        if field == GeneralField::CompactFooter {
            self.config.general.compact_footer = !self.config.general.compact_footer;
            self.save_config();
            self.set_status(format!(
                "compact_footer -> {}",
                self.config.general.compact_footer
            ));
            return;
        }
        if field == GeneralField::TasksShowDoneDefault {
            self.config.general.tasks_show_done_default =
                !self.config.general.tasks_show_done_default;
            self.save_config();
            self.set_status(format!(
                "tasks_show_done_default -> {}",
                self.config.general.tasks_show_done_default
            ));
            return;
        }
        let (label, prefill) = match field {
            GeneralField::DefaultNotebook => (
                "default_notebook",
                self.config.general.default_notebook.clone(),
            ),
            GeneralField::Editor => ("editor", self.config.general.editor.clone()),
            GeneralField::DailyTemplate => {
                ("daily_template", self.config.general.daily_template.clone())
            }
            GeneralField::StatusMessageTimeoutSecs => (
                "status_message_timeout_secs",
                self.config.general.status_message_timeout_secs.to_string(),
            ),
            GeneralField::DrawerWidth => {
                ("drawer_width", self.config.general.drawer_width.to_string())
            }
            GeneralField::DefaultNoteSort => (
                "default_note_sort",
                self.config.general.default_note_sort.clone(),
            ),
            GeneralField::LogHistoryLimit => (
                "log_history_limit",
                self.config.general.log_history_limit.to_string(),
            ),
            GeneralField::TrashRetentionDays => (
                "trash_retention_days",
                self.config.general.trash_retention_days.to_string(),
            ),
            GeneralField::ReadingWpm => {
                ("reading_wpm", self.config.general.reading_wpm.to_string())
            }
            GeneralField::PageStep => ("page_step", self.config.general.page_step.to_string()),
            GeneralField::UseFavoriteEditor
            | GeneralField::EnableCaptureDaemon
            | GeneralField::MouseDragSelection
            | GeneralField::ShowHints
            | GeneralField::RememberLastSession
            | GeneralField::ShowCoffeeLink
            | GeneralField::SkipDeleteConfirm
            | GeneralField::ShowDates
            | GeneralField::WikilinkAutocomplete
            | GeneralField::DailyAgenda
            | GeneralField::CompactFooter
            | GeneralField::TasksShowDoneDefault => unreachable!(),
        };
        self.show_settings = false;
        self.pending_input_title = Some(format!(" {label} "));
        self.start_input(PendingInput::SettingsGeneralText, prefill);
    }
    /// THEME — `name` opens the existing theme picker (reusing its
    /// live-preview/commit logic rather than duplicating it); `icons`
    /// toggles in place, same "flip and save immediately" shape as
    /// GIT/EDITOR's booleans; `overrides` is informational only, since 19
    /// individual color slots don't fit a single-row edit.
    fn handle_theme_field_enter(&mut self) {
        use crate::panel_settings::ThemeField;
        match ThemeField::ALL[self.settings_selected] {
            ThemeField::Name => {
                self.show_settings = false;
                self.reopen_settings_after_theme_picker = true;
                self.open_theme_picker();
            }
            ThemeField::Icons => {
                self.config.theme.icons = !self.config.theme.icons;
                self.save_config();
                self.set_status(format!("icons -> {}", self.config.theme.icons));
            }
            ThemeField::Overrides => {
                self.set_status(
                    "customize individual colors with leader+c (live preview) or `shiki theme create --from <name>`"
                        .into(),
                );
            }
        }
    }
    /// GIT (the global `[git]` defaults) — the four booleans toggle in
    /// place; the rest open a text/number prompt
    /// (`PendingInput::SettingsGitText`, resolved the same way
    /// `SettingsGeneralText` is).
    fn handle_git_field_enter(&mut self) {
        use crate::panel_settings::GitField;
        let field = GitField::ALL[self.settings_selected];
        match field {
            GitField::AutoCommit
            | GitField::AutoPush
            | GitField::SignCommits
            | GitField::AutoSync => {
                self.toggle_git_bool(field);
            }
            GitField::AutoSyncEvery => {
                let prefill = self.config.git.auto_sync_every.to_string();
                self.show_settings = false;
                self.pending_input_title = Some(" auto_sync_every ".to_string());
                self.start_input(PendingInput::SettingsGitText, prefill);
            }
            GitField::CommitPrefix
            | GitField::Remote
            | GitField::Branch
            | GitField::RemoteTemplate => {
                let (label, prefill) = match field {
                    GitField::CommitPrefix => {
                        ("commit_prefix", self.config.git.commit_prefix.clone())
                    }
                    GitField::Remote => ("remote", self.config.git.remote.clone()),
                    GitField::Branch => ("branch", self.config.git.branch.clone()),
                    GitField::RemoteTemplate => {
                        ("remote_template", self.config.git.remote_template.clone())
                    }
                    _ => unreachable!(),
                };
                self.show_settings = false;
                self.pending_input_title = Some(format!(" {label} "));
                self.start_input(PendingInput::SettingsGitText, prefill);
            }
        }
    }
    fn toggle_git_bool(&mut self, field: crate::panel_settings::GitField) {
        use crate::panel_settings::GitField;
        let (label, new_val) = match field {
            GitField::AutoCommit => {
                self.config.git.auto_commit = !self.config.git.auto_commit;
                ("auto_commit", self.config.git.auto_commit)
            }
            GitField::AutoPush => {
                self.config.git.auto_push = !self.config.git.auto_push;
                ("auto_push", self.config.git.auto_push)
            }
            GitField::SignCommits => {
                self.config.git.sign_commits = !self.config.git.sign_commits;
                ("sign_commits", self.config.git.sign_commits)
            }
            GitField::AutoSync => {
                self.config.git.auto_sync = !self.config.git.auto_sync;
                ("auto_sync", self.config.git.auto_sync)
            }
            _ => return,
        };
        self.save_config();
        self.set_status(format!("{label} -> {new_val}"));
    }
    /// EDITOR — every field is a plain bool toggle, no text/drill-down
    /// fields at all, so `Enter` always just flips the selected row.
    fn handle_editor_field_enter(&mut self) {
        use crate::panel_settings::EditorField;
        self.toggle_editor_bool(EditorField::ALL[self.settings_selected]);
    }
    fn toggle_editor_bool(&mut self, field: crate::panel_settings::EditorField) {
        use crate::panel_settings::EditorField;
        let (label, new_val) = match field {
            EditorField::MouseSelection => {
                self.config.editor.mouse_selection = !self.config.editor.mouse_selection;
                ("mouse_selection", self.config.editor.mouse_selection)
            }
            EditorField::FindReplace => {
                self.config.editor.find_replace = !self.config.editor.find_replace;
                ("find_replace", self.config.editor.find_replace)
            }
            EditorField::OsClipboard => {
                self.config.editor.os_clipboard = !self.config.editor.os_clipboard;
                ("os_clipboard", self.config.editor.os_clipboard)
            }
            EditorField::SelectAllCtrlA => {
                self.config.editor.select_all_ctrl_a = !self.config.editor.select_all_ctrl_a;
                ("select_all_ctrl_a", self.config.editor.select_all_ctrl_a)
            }
            EditorField::LineNumbers => {
                self.config.editor.line_numbers = !self.config.editor.line_numbers;
                ("line_numbers", self.config.editor.line_numbers)
            }
            EditorField::MultiCursor => {
                self.config.editor.multi_cursor = !self.config.editor.multi_cursor;
                ("multi_cursor", self.config.editor.multi_cursor)
            }
            EditorField::AutoListContinue => {
                self.config.editor.auto_list_continue = !self.config.editor.auto_list_continue;
                ("auto_list_continue", self.config.editor.auto_list_continue)
            }
            EditorField::FormatShortcuts => {
                self.config.editor.format_shortcuts = !self.config.editor.format_shortcuts;
                ("format_shortcuts", self.config.editor.format_shortcuts)
            }
            EditorField::AutoPairBrackets => {
                self.config.editor.auto_pair_brackets = !self.config.editor.auto_pair_brackets;
                ("auto_pair_brackets", self.config.editor.auto_pair_brackets)
            }
            EditorField::PasteUrlAsLink => {
                self.config.editor.paste_url_as_link = !self.config.editor.paste_url_as_link;
                ("paste_url_as_link", self.config.editor.paste_url_as_link)
            }
            EditorField::SnippetExpandTab => {
                self.config.editor.snippet_expand_tab = !self.config.editor.snippet_expand_tab;
                ("snippet_expand_tab", self.config.editor.snippet_expand_tab)
            }
            EditorField::TypewriterScroll => {
                self.config.editor.typewriter_scroll = !self.config.editor.typewriter_scroll;
                ("typewriter_scroll", self.config.editor.typewriter_scroll)
            }
            EditorField::MoveLine => {
                self.config.editor.move_line = !self.config.editor.move_line;
                ("move_line", self.config.editor.move_line)
            }
            EditorField::DuplicateLine => {
                self.config.editor.duplicate_line = !self.config.editor.duplicate_line;
                ("duplicate_line", self.config.editor.duplicate_line)
            }
            EditorField::BlockIndentSelect => {
                self.config.editor.block_indent_select = !self.config.editor.block_indent_select;
                (
                    "block_indent_select",
                    self.config.editor.block_indent_select,
                )
            }
        };
        self.save_config();
        self.set_status(format!("{label} -> {new_val}"));
    }
    /// EXPORT — `pdf_theme` cycles through `PDF_THEMES` (wrapping) and saves
    /// immediately, same "advance in place, no prompt" shape as
    /// `toggle_git_bool`/`toggle_editor_bool`; `ask_export_path` toggles in
    /// place the same way; `export_dir` opens a text prompt
    /// (`PendingInput::SettingsExportText`, resolved back via
    /// `ExportField::ALL[settings_selected]` once confirmed, same pattern as
    /// `SettingsGeneralText`/`SettingsGitText`).
    fn handle_export_field_enter(&mut self) {
        use crate::panel_settings::{ExportField, PDF_THEMES};
        match ExportField::ALL[self.settings_selected] {
            ExportField::PdfTheme => {
                let current = self.config.export.pdf_theme.as_str();
                let next_index = PDF_THEMES
                    .iter()
                    .position(|t| *t == current)
                    .map(|i| (i + 1) % PDF_THEMES.len())
                    .unwrap_or(0);
                self.config.export.pdf_theme = PDF_THEMES[next_index].to_string();
                self.save_config();
                self.set_status(format!("pdf_theme -> {}", self.config.export.pdf_theme));
            }
            ExportField::AskExportPath => {
                self.config.export.ask_export_path = !self.config.export.ask_export_path;
                self.save_config();
                self.set_status(format!(
                    "ask_export_path -> {}",
                    self.config.export.ask_export_path
                ));
            }
            ExportField::ExportDir => {
                let prefill = self.config.export.export_dir.clone();
                self.show_settings = false;
                self.pending_input_title = Some(" export_dir ".to_string());
                self.start_input(PendingInput::SettingsExportText, prefill);
            }
        }
    }
    /// SNIPPETS level 1's `a` — prompts for a brand-new trigger; the
    /// snippet itself (empty label/body) is created once that's confirmed
    /// (see `confirm_input`'s `SettingsSnippetTrigger` arm), which then
    /// drills straight into level 2 so the label/body can be filled in
    /// immediately.
    fn start_new_snippet(&mut self) {
        self.show_settings = false;
        self.pending_input_title = Some(" New snippet trigger ".to_string());
        self.start_input(PendingInput::SettingsSnippetTrigger, String::new());
    }
    /// SNIPPETS level 1's `d` — same confirm-dialog gate every other delete
    /// in this app goes through (`pending_delete_snippet` mirrors
    /// `pending_delete`'s shape, just keyed by trigger instead of path).
    fn start_delete_snippet(&mut self) {
        let triggers = crate::panel_settings::sorted_snippet_triggers(self);
        let Some(trigger) = triggers.get(self.settings_selected).cloned() else {
            return;
        };
        let message = format!("Delete snippet '{trigger}'?");
        self.pending_delete_snippet = Some(trigger);
        self.confirm = Some(confirm::ConfirmDialog::new(message));
    }
    /// SNIPPETS level 2 — browsing/editing one snippet's `label` (a text
    /// prompt) and `body` (the full inline editor, `Mode::Edit`, since a
    /// snippet body is arbitrary multi-line text). Same `h`/`Esc`/
    /// `Backspace`-back convention every other level-2 view in this modal
    /// uses.
    fn handle_settings_snippet_field_key(&mut self, key: KeyEvent) {
        use crate::panel_settings::SnippetField;
        let Some(trigger) = self.settings_snippet_drill.clone() else {
            return;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('h') => {
                self.settings_snippet_drill = None;
                self.settings_field_selected = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.settings_field_selected + 1 < SnippetField::ALL.len() {
                    self.settings_field_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.settings_field_selected = self.settings_field_selected.saturating_sub(1);
            }
            KeyCode::Enter => match SnippetField::ALL[self.settings_field_selected] {
                SnippetField::Label => {
                    let prefill = self
                        .config
                        .snippets
                        .get(&trigger)
                        .and_then(|s| s.label.clone())
                        .unwrap_or_default();
                    self.show_settings = false;
                    self.pending_input_title = Some(format!(" Label — '{trigger}' "));
                    self.start_input(PendingInput::SettingsSnippetLabel, prefill);
                }
                SnippetField::Body => {
                    let body = self
                        .config
                        .snippets
                        .get(&trigger)
                        .map(|s| s.body.clone())
                        .unwrap_or_default();
                    let mut editor = InlineEditor::from_contents(&body);
                    let title = format!(" {}Editing snippet body — '{trigger}' ", icons::GEAR);
                    self.style_inline_editor(&mut editor, title);
                    self.editor = Some(editor);
                    self.editing_snippet = Some(trigger);
                    self.show_settings = false;
                    self.mode = Mode::Edit;
                }
            },
            _ => {}
        }
    }
    /// NOTEBOOKS level 2 — browsing/editing one notebook's remote and
    /// sync-policy overrides. `h`/`Backspace`/`Esc` all go back to level 1
    /// (same trio the tags modal's own level-2 uses); a *second* `Esc` from
    /// level 1 is what actually closes the whole modal, handled by the
    /// level-1 branch in `handle_settings_key`.
    fn handle_settings_notebook_field_key(&mut self, key: KeyEvent) {
        use crate::panel_settings::NotebookField;
        let Some(name) = self.settings_notebook_drill.clone() else {
            return;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Char('h') => {
                self.settings_notebook_drill = None;
                self.settings_field_selected = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.settings_field_selected + 1 < NotebookField::ALL.len() {
                    self.settings_field_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.settings_field_selected = self.settings_field_selected.saturating_sub(1);
            }
            KeyCode::Enter => match NotebookField::ALL[self.settings_field_selected] {
                NotebookField::Remote => {
                    let prefill = self
                        .notebooks
                        .iter()
                        .find(|nb| nb.name == name)
                        .and_then(|nb| shiki_core::git::remote_url(&nb.path))
                        .unwrap_or_default();
                    self.show_settings = false;
                    self.pending_input_title = Some(format!(" Git remote — '{name}' "));
                    self.start_input(PendingInput::SettingsNotebookRemote, prefill);
                }
                NotebookField::AutoPush => {
                    self.cycle_notebook_bool_override(&name, NotebookField::AutoPush)
                }
                NotebookField::AutoSync => {
                    self.cycle_notebook_bool_override(&name, NotebookField::AutoSync)
                }
                NotebookField::AutoSyncEvery => {
                    let prefill = self
                        .config
                        .notebooks
                        .get(&name)
                        .and_then(|over| over.auto_sync_every)
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    self.show_settings = false;
                    self.pending_input_title =
                        Some(format!(" Auto-sync every N changes — '{name}' "));
                    self.start_input(PendingInput::SettingsNotebookAutoSyncEvery, prefill);
                }
                NotebookField::Encryption => {
                    self.show_settings = false;
                    self.reopen_settings_after_passphrase = true;
                    let purpose = if self.config.encrypt_for(&name) {
                        PassphrasePurpose::Disable
                    } else {
                        PassphrasePurpose::Enable
                    };
                    self.start_passphrase_prompt(name, purpose);
                }
                NotebookField::Hidden => {
                    if let Some(over) = self.config.notebooks.get_mut(&name) {
                        over.hidden = false;
                    }
                    self.prune_empty_notebook_override(&name);
                    self.save_config();
                    self.reload_notebooks();
                    self.settings_notebook_drill = None;
                    self.settings_field_selected = 0;
                    self.set_status(format!("notebook '{name}' restored — it's listed again"));
                }
            },
            _ => {}
        }
    }
    /// Cycles a per-notebook boolean override: unset (inherit the global
    /// `[git]` default) → `true` → `false` → unset again — applied and
    /// persisted immediately on `Enter`, no confirmation, since it's a
    /// single reversible toggle rather than anything destructive.
    fn cycle_notebook_bool_override(
        &mut self,
        name: &str,
        field: crate::panel_settings::NotebookField,
    ) {
        use crate::panel_settings::NotebookField;
        let over = self.config.notebooks.entry(name.to_string()).or_default();
        let (label, new_val) = match field {
            NotebookField::AutoPush => {
                over.auto_push = match over.auto_push {
                    None => Some(true),
                    Some(true) => Some(false),
                    Some(false) => None,
                };
                ("auto_push", over.auto_push)
            }
            NotebookField::AutoSync => {
                over.auto_sync = match over.auto_sync {
                    None => Some(true),
                    Some(true) => Some(false),
                    Some(false) => None,
                };
                ("auto_sync", over.auto_sync)
            }
            NotebookField::Remote
            | NotebookField::AutoSyncEvery
            | NotebookField::Encryption
            | NotebookField::Hidden => return,
        };
        self.prune_empty_notebook_override(name);
        self.save_config();
        let shown = new_val
            .map(|v| v.to_string())
            .unwrap_or_else(|| "inherit".to_string());
        self.set_status(format!("notebook '{name}': {label} -> {shown}"));
    }
    /// Removes a notebook's `[notebooks.<name>]` table once every override
    /// field in it has been cycled back to unset — otherwise cycling all
    /// three back to "inherit" would still leave a pointless empty table in
    /// `config.toml`.
    fn prune_empty_notebook_override(&mut self, name: &str) {
        if let Some(over) = self.config.notebooks.get(name) {
            if over.auto_push.is_none()
                && over.auto_sync.is_none()
                && over.auto_sync_every.is_none()
                && !over.hidden
                && !over.encrypt
            {
                self.config.notebooks.remove(name);
            }
        }
    }
    pub(crate) fn save_config(&mut self) {
        if let Ok(path) = Config::default_path() {
            let _ = self.config.save(&path);
        }
    }
    /// The passphrase-derived key `notebook_name`'s note I/O should use
    /// right now — `None` when the notebook isn't configured as encrypted
    /// at all, *or* when it is but nothing's been typed in yet this
    /// session (in which case reads/writes on it will surface a clear
    /// `Error::Encryption` rather than silently succeeding on garbage).
    pub(crate) fn resolved_notebook_crypto(
        &self,
        notebook_name: &str,
    ) -> Option<shiki_core::crypto::NotebookCrypto> {
        if !self.config.encrypt_for(notebook_name) {
            return None;
        }
        self.notebook_passphrases
            .get(notebook_name)
            .map(|p| shiki_core::crypto::NotebookCrypto::new(p.clone()))
    }
    /// Opens a masked passphrase prompt for `notebook` — see
    /// `PassphrasePurpose` for what the answer will be used for.
    pub(crate) fn start_passphrase_prompt(&mut self, notebook: String, purpose: PassphrasePurpose) {
        let title = match purpose {
            PassphrasePurpose::Unlock => format!(" Passphrase — unlock '{notebook}' "),
            PassphrasePurpose::Enable => format!(" New passphrase — encrypt '{notebook}' "),
            PassphrasePurpose::EnableConfirm => " Confirm passphrase ".to_string(),
            PassphrasePurpose::Disable => format!(" Passphrase — decrypt '{notebook}' "),
        };
        self.passphrase_prompt_notebook = Some(notebook);
        self.passphrase_purpose = Some(purpose);
        self.pending_input_title = Some(title);
        self.start_masked_input(PendingInput::NotebookPassphrase, String::new());
    }
    /// Called when `reload_notes` hits an encrypted-but-locked notebook
    /// (`Error::Encryption` from `list_dir`, meaning no passphrase is
    /// cached yet, or the cached one was wrong) — clears any wrong cached
    /// passphrase and opens the unlock prompt, unless one's already open.
    pub(crate) fn maybe_prompt_for_notebook_passphrase(&mut self) {
        if self.pending_input.is_some() || self.confirm.is_some() {
            return;
        }
        let Some(name) = self.selected_notebook().map(|nb| nb.name.clone()) else {
            return;
        };
        if self.passphrase_prompt_notebook.as_deref() == Some(name.as_str()) {
            return;
        }
        self.notebook_passphrases.remove(&name);
        self.start_passphrase_prompt(name, PassphrasePurpose::Unlock);
    }
    /// Verifies the typed passphrase against the notebook's canary
    /// (`.shiki-encryption`, committed to git alongside the encrypted
    /// notes — so it's already there on every machine after a pull) before
    /// trusting it at all. On success this also persists `encrypt = true`
    /// into *this machine's own* config.toml, not just the in-memory cache
    /// — `config.toml` never travels with the git repo (it lives outside
    /// it entirely, precisely so the passphrase itself never can), so a
    /// second/third/… machine that only ever `git clone`d or `git pull`ed
    /// this notebook has no `[notebooks.<name>]` entry for it at all until
    /// this happens once. Without this, a successful *read* on that
    /// machine would work (content is sniffed directly, not gated by the
    /// config flag), but a subsequent *write* on the same machine would
    /// silently fall back to plaintext, since `resolved_notebook_crypto`
    /// consults the config flag, not just whatever's cached in memory.
    fn unlock_notebook_passphrase(&mut self, notebook: &str, passphrase: String) {
        let Some(nb) = self.notebooks.iter().find(|n| n.name == notebook).cloned() else {
            self.set_status(format!("'{notebook}' no longer available"));
            return;
        };
        let crypto = shiki_core::crypto::NotebookCrypto::new(passphrase.clone());
        let canary_path = nb.path.join(shiki_core::crypto::CANARY_FILE);
        let verified = match std::fs::read_to_string(&canary_path) {
            Ok(canary) => shiki_core::crypto::verify_canary(&crypto, &canary),
            // No canary at all is unexpected for a notebook `encrypt` ever
            // wrote (it's committed alongside the encrypted notes) — fall
            // back to "trust it, let list_dir prove it" rather than
            // refusing outright, so an older/hand-set-up encrypted
            // notebook without one still works.
            Err(_) => Ok(true),
        };
        match verified {
            Ok(true) => {
                self.notebook_passphrases
                    .insert(notebook.to_string(), passphrase);
                self.config
                    .notebooks
                    .entry(notebook.to_string())
                    .or_default()
                    .encrypt = true;
                self.save_config();
                self.reload_notebooks();
            }
            Ok(false) => {
                self.set_status("canary file is corrupted, not just a wrong passphrase".into());
            }
            Err(e) => {
                self.set_status(format!("wrong passphrase: {e}"));
                // Reopen immediately rather than leaving the user stuck —
                // nothing else currently re-triggers this prompt on demand.
                self.start_passphrase_prompt(notebook.to_string(), PassphrasePurpose::Unlock);
            }
        }
    }
    /// `Enter` on a `NotebookPassphrase` prompt — dispatches on
    /// `PassphrasePurpose`, taken (not just read) along with which notebook
    /// it's for, so a stray leftover value can't linger into an unrelated
    /// later prompt.
    fn confirm_notebook_passphrase(&mut self) {
        // Deliberately *not* trimmed, unlike every other text prompt
        // (`confirm_input`'s shared `value`) — a passphrase's leading/
        // trailing whitespace is part of it, not accidental formatting.
        let passphrase = self.input.value.clone();
        let Some(notebook) = self.passphrase_prompt_notebook.take() else {
            return;
        };
        let Some(purpose) = self.passphrase_purpose.take() else {
            return;
        };
        match purpose {
            PassphrasePurpose::Unlock => self.unlock_notebook_passphrase(&notebook, passphrase),
            PassphrasePurpose::Enable => {
                if passphrase.is_empty() {
                    self.set_status("passphrase must not be empty — encryption not enabled".into());
                    self.maybe_reopen_settings_after_passphrase();
                    return;
                }
                self.passphrase_pending_first = Some(passphrase);
                self.start_passphrase_prompt(notebook, PassphrasePurpose::EnableConfirm);
                // Re-set: `start_passphrase_prompt` doesn't know this
                // chained prompt should still reopen Settings when it
                // eventually finishes/cancels.
                self.reopen_settings_after_passphrase = true;
            }
            PassphrasePurpose::EnableConfirm => {
                let first = self.passphrase_pending_first.take();
                if first.as_deref() != Some(passphrase.as_str()) {
                    self.set_status("passphrases did not match — encryption not enabled".into());
                    self.maybe_reopen_settings_after_passphrase();
                    return;
                }
                self.enable_notebook_encryption(&notebook, passphrase);
                self.maybe_reopen_settings_after_passphrase();
            }
            PassphrasePurpose::Disable => {
                self.disable_notebook_encryption(&notebook, passphrase);
                self.maybe_reopen_settings_after_passphrase();
            }
        }
    }
    fn maybe_reopen_settings_after_passphrase(&mut self) {
        if self.reopen_settings_after_passphrase {
            self.reopen_settings_after_passphrase = false;
            self.show_settings = true;
        }
    }
    /// Turns encryption on for a plaintext notebook: writes the canary
    /// (`.shiki-encryption`) first, so a mistyped passphrase is caught
    /// before anything real is touched, then re-encrypts every existing
    /// note, flips the config flag, and commits. Any failure partway
    /// through (a bad file permission, disk full) leaves some notes
    /// re-encrypted and some not — same "no data lost, just retry" spirit
    /// as a failed push elsewhere in this codebase, since re-running this
    /// (once the underlying problem's fixed) just re-encrypts everything
    /// again, which is idempotent.
    fn enable_notebook_encryption(&mut self, name: &str, passphrase: String) {
        let Some(nb) = self.notebooks.iter().find(|n| n.name == name).cloned() else {
            self.set_status(format!("'{name}' no longer available"));
            return;
        };
        let crypto = shiki_core::crypto::NotebookCrypto::new(passphrase.clone());
        let canary = match shiki_core::crypto::canary_blob(&crypto) {
            Ok(c) => c,
            Err(e) => {
                self.set_status(format!("could not set up encryption: {e}"));
                return;
            }
        };
        if let Err(e) = std::fs::write(nb.path.join(shiki_core::crypto::CANARY_FILE), canary) {
            self.set_status(format!("could not write canary file: {e}"));
            return;
        }
        let notes = match nb.all_notes_recursive() {
            Ok(n) => n,
            Err(e) => {
                self.set_status(format!("could not read existing notes: {e}"));
                return;
            }
        };
        for note in &notes {
            if let Err(e) = note.save_with_crypto(Some(&crypto)) {
                self.set_status(format!(
                    "could not re-encrypt '{}': {e}",
                    note.path.display()
                ));
                return;
            }
        }
        self.config
            .notebooks
            .entry(name.to_string())
            .or_default()
            .encrypt = true;
        self.save_config();
        self.notebook_passphrases
            .insert(name.to_string(), passphrase);
        match shiki_core::git::commit_all(&nb.path, "shiki: enable encryption") {
            Ok(_) => self.set_status(format!(
                "'{name}' is now encrypted ({} note{} re-encrypted)",
                notes.len(),
                if notes.len() == 1 { "" } else { "s" }
            )),
            Err(e) => self.set_status(format!("encrypted '{name}' but could not commit: {e}")),
        }
        self.reload_notebooks();
    }
    /// Reverses `enable_notebook_encryption`: verifies the passphrase
    /// against the canary *before* touching any real note (a wrong
    /// passphrase must never get the chance to overwrite a note with
    /// corrupted plaintext), decrypts everything back, removes the canary,
    /// clears the config flag, and commits.
    fn disable_notebook_encryption(&mut self, name: &str, passphrase: String) {
        let Some(nb) = self.notebooks.iter().find(|n| n.name == name).cloned() else {
            self.set_status(format!("'{name}' no longer available"));
            return;
        };
        let crypto = shiki_core::crypto::NotebookCrypto::new(passphrase);
        let canary_path = nb.path.join(shiki_core::crypto::CANARY_FILE);
        let canary = match std::fs::read_to_string(&canary_path) {
            Ok(c) => c,
            Err(e) => {
                self.set_status(format!("could not read canary file: {e}"));
                return;
            }
        };
        match shiki_core::crypto::verify_canary(&crypto, &canary) {
            Ok(true) => {}
            Ok(false) => {
                self.set_status("canary file is corrupted, not just a wrong passphrase".into());
                return;
            }
            Err(e) => {
                self.set_status(format!("wrong passphrase: {e}"));
                return;
            }
        }
        let nb_unlocked = nb.clone().with_crypto(Some(crypto));
        let notes = match nb_unlocked.all_notes_recursive() {
            Ok(n) => n,
            Err(e) => {
                self.set_status(format!("could not decrypt existing notes: {e}"));
                return;
            }
        };
        for note in &notes {
            if let Err(e) = note.save_with_crypto(None) {
                self.set_status(format!("could not decrypt '{}': {e}", note.path.display()));
                return;
            }
        }
        std::fs::remove_file(&canary_path).ok();
        if let Some(over) = self.config.notebooks.get_mut(name) {
            over.encrypt = false;
        }
        self.prune_empty_notebook_override(name);
        self.save_config();
        self.notebook_passphrases.remove(name);
        match shiki_core::git::commit_all(&nb.path, "shiki: disable encryption") {
            Ok(_) => self.set_status(format!(
                "'{name}' is now decrypted ({} note{} restored to plain text)",
                notes.len(),
                if notes.len() == 1 { "" } else { "s" }
            )),
            Err(e) => self.set_status(format!("decrypted '{name}' but could not commit: {e}")),
        }
        self.reload_notebooks();
    }
    /// Persists `general.remember_last_session`'s state — called once, right
    /// after the main loop in `run()` exits, just before the process actually
    /// quits. See `App::restore_session` for the read side, applied at
    /// startup. A no-op when the setting is off, or when there's no selected
    /// notebook at all (an empty store) — nothing meaningful to remember.
    pub(crate) fn save_session(&self) {
        if !self.config.general.remember_last_session {
            return;
        }
        let Some(notebook) = self.selected_notebook() else {
            return;
        };
        let Ok(path) = Config::default_session_path() else {
            return;
        };
        let selected = if self.selected_note < self.folders.len() {
            self.folders
                .get(self.selected_note)
                .map(|name| shiki_config::session::SelectedEntry::Folder { name: name.clone() })
        } else {
            self.selected_note()
                .map(|note| shiki_config::session::SelectedEntry::Note {
                    stem: note.file_stem(),
                })
        };
        let session = shiki_config::SessionState {
            notebook: notebook.name.clone(),
            notes_path: self.notes_path.clone(),
            selected,
            focus: self.focus.as_session_str().to_string(),
        };
        let _ = session.save(&path);
    }
    /// Queues `config.toml` for external editing — same
    /// `want_external_edit` mechanism a note's `E`/favorite-editor `i` use,
    /// plus `want_external_edit_config` so `run()` reloads/applies the
    /// config afterward instead of refreshing notes.
    fn start_external_config_edit(&mut self, editor: String) {
        if let Ok(path) = Config::default_path() {
            self.want_external_edit = Some((path, editor));
            self.want_external_edit_config = true;
        } else {
            self.set_status("config path error".into());
        }
    }
    fn handle_logs_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.show_logs = false,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.logs_selected + 1 < self.log_history.len() {
                    self.logs_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.logs_selected = self.logs_selected.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.logs_selected = (self.logs_selected + self.page_step() as usize)
                    .min(self.log_history.len().saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.logs_selected = self.logs_selected.saturating_sub(self.page_step() as usize);
            }
            KeyCode::Home => self.logs_selected = 0,
            KeyCode::End => self.logs_selected = self.log_history.len().saturating_sub(1),
            // Copies the whole scrollback in one go — meant for pasting the
            // full context of an error somewhere else, not just one line.
            KeyCode::Char('y') | KeyCode::Char('c') => {
                let text = self
                    .log_history
                    .iter()
                    .map(|entry| format!("[{}] {}", entry.at.format("%H:%M:%S"), entry.message))
                    .collect::<Vec<_>>()
                    .join("\n");
                let count = self.log_history.len();
                crate::clipboard::copy(&text);
                self.set_status(format!("copied {count} log lines to clipboard"));
            }
            // Destructive and irreversible (wipes the on-disk history too,
            // the whole point of which is surviving a crash) — behind the
            // same confirm-dialog pattern as delete note/notebook, not an
            // immediate clear on one keypress.
            KeyCode::Char('x') => {
                self.pending_clear_logs = true;
                self.confirm = Some(confirm::ConfirmDialog::new(
                    "Clear all logs? This can't be undone.",
                ));
            }
            _ => {}
        }
    }
    /// Opens the update modal and kicks off a background version check
    /// against GitHub Releases — never blocks the render loop.
    fn open_update_check(&mut self) {
        self.show_update = true;
        self.update_state = Some(UpdateState::Checking);
        let (tx, rx) = std::sync::mpsc::channel();
        let current = env!("CARGO_PKG_VERSION").to_string();
        std::thread::spawn(move || {
            let result = shiki_core::update::check_latest(&current).map_err(|e| e.to_string());
            let _ = tx.send(UpdateMsg::CheckResult(result));
        });
        self.update_rx = Some(rx);
    }

    /// Only reachable once the check reported an available version — starts
    /// the real download+verify+install on a background thread, pinned to
    /// the exact version the confirm dialog showed (`UpdateState::Available`'s
    /// own string) rather than re-resolving "latest" a second time — a new
    /// release landing in the gap between the check and this confirmation
    /// must not silently install a different version than what was shown.
    fn start_update_install(&mut self) {
        let Some(UpdateState::Available(target_version)) = self.update_state.clone() else {
            return;
        };
        self.update_state = Some(UpdateState::Downloading);
        // Captured now, before the replace happens — see the field doc on
        // `relaunch_exe_path` for why this can't just be re-queried later.
        self.relaunch_exe_path = std::env::current_exe().ok();
        let (tx, rx) = std::sync::mpsc::channel();
        let current = env!("CARGO_PKG_VERSION").to_string();
        std::thread::spawn(move || {
            let result = shiki_core::update::install_version(&current, &target_version)
                .map_err(|e| e.to_string());
            let _ = tx.send(UpdateMsg::InstallResult(result));
        });
        self.update_rx = Some(rx);
    }

    /// Non-blocking: called once per `run()` loop iteration, same as
    /// `refresh_history_cache`. Applies whatever the background thread has
    /// sent so far, if anything — `try_recv` never waits.
    pub(crate) fn poll_update_channel(&mut self) {
        let Some(rx) = &self.update_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(UpdateMsg::CheckResult(Ok(Some(version)))) => {
                self.update_state = Some(UpdateState::Available(version));
                self.update_rx = None;
            }
            Ok(UpdateMsg::CheckResult(Ok(None))) => {
                self.update_state = Some(UpdateState::UpToDate);
                self.update_rx = None;
            }
            Ok(UpdateMsg::CheckResult(Err(e))) => {
                self.update_state = Some(UpdateState::Error(e));
                self.update_rx = None;
            }
            Ok(UpdateMsg::InstallResult(Ok(version))) => {
                self.update_state = Some(UpdateState::Installed(version));
                self.update_rx = None;
                // Automatic per the feature request — no keypress required:
                // `run()` checks this right after the next draw, so the
                // "Installed" frame renders at least once before the swap.
                self.want_relaunch = true;
            }
            Ok(UpdateMsg::InstallResult(Err(e))) => {
                self.update_state = Some(UpdateState::Error(e));
                self.update_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.update_state = Some(UpdateState::Error(
                    "update check thread ended unexpectedly".into(),
                ));
                self.update_rx = None;
            }
        }
    }

    /// Non-blocking: called once per `run()` loop iteration, same spot as
    /// `poll_update_channel`/`poll_sync_channel`. Drains every pending
    /// `CaptureRequest`, not just one — several `shiki capture` invocations
    /// could queue up between two frames — and answers each one over its
    /// own one-shot reply channel before moving to the next.
    pub(crate) fn poll_capture_channel(&mut self) {
        while let Ok(request) = self.capture_rx.try_recv() {
            let reply = crate::capture::handle_request(self, &request.kind);
            let _ = request.reply_tx.send(reply);
        }
    }
    fn handle_update_key(&mut self, key: KeyEvent) {
        match &self.update_state {
            // Downloading is deliberately not escapable: closing the modal
            // wouldn't stop the background thread anyway, and re-entering
            // leader+`U` mid-download would spawn a second overlapping
            // install — simplest to just make the user wait it out.
            Some(UpdateState::Downloading) => {}
            Some(UpdateState::Available(_)) => match key.code {
                KeyCode::Enter => self.start_update_install(),
                KeyCode::Esc => {
                    self.show_update = false;
                    self.update_state = None;
                }
                _ => {}
            },
            Some(UpdateState::Installed(_)) => {
                // Any key dismisses — `run()` picks up `want_relaunch` right
                // after this same key event regardless, so this mostly just
                // avoids sitting on a stale "Installed" state if the relaunch
                // spawn itself somehow fails.
                self.show_update = false;
            }
            _ => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
                    self.show_update = false;
                    self.update_state = None;
                }
            }
        }
    }
    /// Loads every note from every notebook into `global_search_pool` —
    /// same pool `open_global_search` builds, reused here so which-key's
    /// note results and the standalone global search modal can never
    /// disagree about which notes exist or how they're identified
    /// (`pool_index`). Safe to share the one field since the two modals
    /// are never open at the same time.
    fn open_which_key(&mut self) {
        self.which_key_input.clear();
        self.which_key_selected = 0;
        self.global_search_pool = self.store.all_notes().unwrap_or_default();
        self.which_key_note_hits.clear();
        self.show_which_key = true;
    }
    /// Re-scores `global_search_pool` against the current which-key query
    /// (title + body + notebook name, same haystack `refresh_global_search`
    /// scores against) — called on every keystroke, not from
    /// `which_key_filtered_entries` itself, since `SearchEngine::search_text`
    /// needs `&mut self` but that function is `&self` (called from
    /// rendering). Deliberately empty while the query is empty: an
    /// unfiltered which-key should still read as "browse every keybinding,"
    /// not also dump every note in every notebook.
    fn refresh_which_key_notes(&mut self) {
        let query = self.which_key_input.value.clone();
        if query.is_empty() {
            self.which_key_note_hits.clear();
            return;
        }
        let haystacks: Vec<String> = self
            .global_search_pool
            .iter()
            .map(|(nb, note)| format!("{} {} {}", nb.name, note.frontmatter.title, note.body))
            .collect();
        let haystack_refs: Vec<&str> = haystacks.iter().map(String::as_str).collect();
        let mut hits = self.search_engine.search_text(&query, &haystack_refs);
        hits.truncate(8);
        self.which_key_note_hits = hits;
    }
    /// Every keybinding entry whose key, action label, or scope name
    /// contains the current query (case-insensitive) — all of them if the
    /// query is empty — plus, once the query is non-empty, up to 8 matching
    /// notes from `which_key_note_hits` (see `refresh_which_key_notes`).
    /// Backs both rendering and `Enter`'s execute-in-place.
    pub fn which_key_filtered_entries(&self) -> Vec<WhichKeyRow> {
        let query = self.which_key_input.value.to_lowercase();
        let bound = self
            .keymaps
            .entries()
            .into_iter()
            .map(|(scope, key, action)| WhichKeyRow::Bound { scope, key, action });
        let nav = self
            .keymaps
            .nav_rows()
            .into_iter()
            .map(|(scope, key, label)| WhichKeyRow::Nav { scope, key, label });
        let mut rows: Vec<WhichKeyRow> = bound
            .chain(nav)
            .filter(|row| {
                query.is_empty()
                    || row.key().to_lowercase().contains(&query)
                    || row.label().to_lowercase().contains(&query)
                    || row.scope().to_lowercase().contains(&query)
            })
            .collect();
        rows.extend(self.which_key_note_hits.iter().filter_map(|hit| {
            let (nb, note) = self.global_search_pool.get(hit.index)?;
            Some(WhichKeyRow::NoteHit {
                pool_index: hit.index,
                label: format!("{}  —  {}", note.frontmatter.title, nb.name),
            })
        }));
        rows
    }
    fn handle_which_key_key(&mut self, key: KeyEvent) {
        let len = self.which_key_filtered_entries().len();
        match key.code {
            KeyCode::Esc => self.show_which_key = false,
            // Executes the highlighted entry directly, or jumps straight to
            // it if it's a note — the which-key modal doubles as a fast
            // command palette: type to filter (actions AND notes), Enter to
            // run/open, instead of memorizing the key and closing the modal
            // first.
            KeyCode::Enter => {
                match self
                    .which_key_filtered_entries()
                    .get(self.which_key_selected)
                    .cloned()
                {
                    Some(WhichKeyRow::Bound { action, .. }) => {
                        self.show_which_key = false;
                        self.handle_action(action);
                    }
                    Some(WhichKeyRow::NoteHit { pool_index, .. }) => {
                        self.show_which_key = false;
                        self.jump_to_global_hit(pool_index);
                    }
                    Some(WhichKeyRow::Nav { .. }) | None => {}
                }
            }
            KeyCode::Down => {
                if self.which_key_selected + 1 < len {
                    self.which_key_selected += 1;
                }
            }
            KeyCode::Up => self.which_key_selected = self.which_key_selected.saturating_sub(1),
            KeyCode::PageDown => {
                self.which_key_selected = (self.which_key_selected + 10).min(len.saturating_sub(1))
            }
            KeyCode::PageUp => self.which_key_selected = self.which_key_selected.saturating_sub(10),
            KeyCode::Home => self.which_key_selected = 0,
            KeyCode::End => self.which_key_selected = len.saturating_sub(1),
            KeyCode::Backspace => {
                self.which_key_input.backspace();
                self.which_key_selected = 0;
                self.refresh_which_key_notes();
            }
            KeyCode::Char(c) => {
                self.which_key_input.push(c);
                self.which_key_selected = 0;
                self.refresh_which_key_notes();
            }
            _ => {}
        }
    }
    /// Loads the selected note's headings and opens the outline modal.
    /// While actively editing (`Mode::Edit`, reached via `Ctrl+O`), reads
    /// the *live* editor buffer instead of the last-saved body, so a
    /// heading typed but not yet saved still shows up.
    fn open_outline(&mut self) {
        let body = if self.mode == Mode::Edit {
            match &self.editor {
                Some(editor) => editor.textarea.lines().join("\n"),
                None => {
                    self.set_status("no note selected".into());
                    return;
                }
            }
        } else {
            match self.selected_note() {
                Some(note) => note.body.clone(),
                None => {
                    self.set_status("no note selected".into());
                    return;
                }
            }
        };
        self.outline_headings = shiki_core::headings::extract(&body);
        self.outline_selected = 0;
        self.show_outline = true;
        if self.outline_headings.is_empty() {
            self.set_status("no headings in this note".into());
        }
    }
    /// `Enter` jumps to the selected heading: inside `Mode::Edit`, moves
    /// the editor's cursor there directly; otherwise (opened from
    /// PREVIEW), scrolls the preview panel to that source line. Either way
    /// the modal closes.
    fn handle_outline_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.show_outline = false,
            KeyCode::Enter => {
                if let Some(heading) = self.outline_headings.get(self.outline_selected).cloned() {
                    if self.mode == Mode::Edit {
                        if let Some(editor) = &mut self.editor {
                            editor.textarea.cancel_selection();
                            editor
                                .textarea
                                .move_cursor(ratatui_textarea::CursorMove::Jump(
                                    heading.line as u16,
                                    0,
                                ));
                        }
                    } else {
                        self.preview_scroll = heading.line as u16;
                    }
                }
                self.show_outline = false;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.outline_selected + 1 < self.outline_headings.len() {
                    self.outline_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.outline_selected = self.outline_selected.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.outline_selected = (self.outline_selected + self.page_step() as usize)
                    .min(self.outline_headings.len().saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.outline_selected = self
                    .outline_selected
                    .saturating_sub(self.page_step() as usize);
            }
            KeyCode::Home => self.outline_selected = 0,
            KeyCode::End => {
                self.outline_selected = self.outline_headings.len().saturating_sub(1);
            }
            _ => {}
        }
    }
    /// Loads the selected note's real version history (every commit that
    /// changed it) and opens the history modal.
    fn open_history(&mut self) {
        let Some((nb, relative)) = self.selected_note_relative_path() else {
            self.set_status("no note selected".into());
            return;
        };
        self.history_entries =
            shiki_core::git::file_history(&nb.path, &relative).unwrap_or_default();
        self.history_selected = 0;
        self.history_viewing = None;
        self.history_diff_viewing = None;
        self.show_history = true;
        if self.history_entries.is_empty() {
            self.set_status("no history yet — sync (`s`) to commit this note first".into());
        }
    }
    fn handle_history_key(&mut self, key: KeyEvent) {
        if self.history_diff_viewing.is_some() {
            match key.code {
                KeyCode::Esc => self.history_diff_viewing = None,
                KeyCode::Char('r') => self.start_revert_selected_history(),
                _ => {}
            }
            return;
        }
        if self.history_viewing.is_some() {
            match key.code {
                KeyCode::Esc => self.history_viewing = None,
                KeyCode::Char('r') => self.start_revert_selected_history(),
                KeyCode::Char('d') => {
                    self.history_viewing = None;
                    self.view_selected_history_diff();
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.show_history = false,
            KeyCode::Enter => self.view_selected_history(),
            KeyCode::Char('d') => self.view_selected_history_diff(),
            KeyCode::Char('r') => self.start_revert_selected_history(),
            KeyCode::Char('j') | KeyCode::Down => {
                if self.history_selected + 1 < self.history_entries.len() {
                    self.history_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.history_selected = self.history_selected.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.history_selected = (self.history_selected + self.page_step() as usize)
                    .min(self.history_entries.len().saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.history_selected = self
                    .history_selected
                    .saturating_sub(self.page_step() as usize);
            }
            KeyCode::Home => self.history_selected = 0,
            KeyCode::End => self.history_selected = self.history_entries.len().saturating_sub(1),
            _ => {}
        }
    }
    /// Fetches and shows the highlighted revision's full content — a plain
    /// read-only look at what the note used to say, before deciding whether
    /// to `r`evert to it.
    fn view_selected_history(&mut self) {
        let Some((nb, relative)) = self.selected_note_relative_path() else {
            return;
        };
        let Some(entry) = self.history_entries.get(self.history_selected).cloned() else {
            return;
        };
        match shiki_core::git::show_file_at(&nb.path, &entry.commit_id, &relative) {
            // The blob is a real git revision, so it's ciphertext for an
            // encrypted notebook — decrypt before showing it, the same way
            // a live `Note::from_file_with_crypto` read does. A wrong/
            // missing passphrase surfaces as the same clear error the
            // normal read path already gives, not raw armor text.
            Ok(content) => match nb.crypto.as_ref() {
                Some(crypto) if shiki_core::crypto::looks_encrypted(&content) => {
                    match crypto.decrypt(&content) {
                        Ok(plain) => self.history_viewing = Some((entry.commit_id, plain)),
                        Err(e) => self.set_status(format!("could not decrypt revision: {e}")),
                    }
                }
                _ => self.history_viewing = Some((entry.commit_id, content)),
            },
            Err(e) => self.set_status(format!("could not load revision: {e}")),
        }
    }
    /// Fetches and shows the highlighted revision's diff against its parent
    /// — "what did this commit actually change here," the `d` alternative
    /// to `Enter`'s full-content view. The very first commit in the file's
    /// history has no parent to diff against, so every line comes back as
    /// an addition — the correct answer (the whole file really is new at
    /// that point), not an error.
    fn view_selected_history_diff(&mut self) {
        let Some((nb, relative)) = self.selected_note_relative_path() else {
            return;
        };
        // A real tree diff of two ciphertext blobs is meaningless noise —
        // libgit2 has no notion of what's inside them, so it can't tell
        // "the whole file changed" from "one word changed." Rather than
        // show that noise, degrade to the same decrypted full-content view
        // `Enter` already gives; a real diff over decrypted text is future
        // work, not something this v1 attempts.
        if nb.crypto.is_some() {
            self.view_selected_history();
            if self.history_viewing.is_some() {
                self.set_status(
                    "diff isn't available for encrypted notebooks — showing content instead".into(),
                );
            }
            return;
        }
        let Some(entry) = self.history_entries.get(self.history_selected).cloned() else {
            return;
        };
        match shiki_core::git::diff_file_at(&nb.path, &entry.commit_id, &relative) {
            Ok(lines) => self.history_diff_viewing = Some((entry.commit_id, lines)),
            Err(e) => self.set_status(format!("could not load diff: {e}")),
        }
    }
    /// Stages a revert of the currently highlighted (or viewed) revision
    /// behind the usual `y`/`n` confirmation, since it overwrites the
    /// note's current working content.
    fn start_revert_selected_history(&mut self) {
        let Some(note) = self.selected_note() else {
            return;
        };
        let commit_id = self
            .history_viewing
            .as_ref()
            .map(|(id, _)| id.clone())
            .or_else(|| self.history_diff_viewing.as_ref().map(|(id, _)| id.clone()))
            .or_else(|| {
                self.history_entries
                    .get(self.history_selected)
                    .map(|e| e.commit_id.clone())
            });
        let Some(commit_id) = commit_id else {
            return;
        };
        let short = commit_id.chars().take(7).collect::<String>();
        let message = format!(
            "Revert '{}' to {short} — overwrites the current content?",
            note.file_stem()
        );
        self.pending_revert = Some((note.path.clone(), commit_id));
        self.confirm = Some(confirm::ConfirmDialog::new(message));
    }
    /// Writes the reverted content back to disk and lets the normal sync
    /// flow pick it up as a pending change, same as any other edit.
    fn perform_revert(&mut self, note_path: &std::path::Path, commit_id: &str) {
        let Some(nb) = self.selected_notebook().cloned() else {
            return;
        };
        let Ok(relative) = note_path.strip_prefix(&nb.path) else {
            return;
        };
        match shiki_core::git::revert_file_to(&nb.path, commit_id, relative) {
            Ok(()) => {
                let short = commit_id.chars().take(7).collect::<String>();
                self.refresh_notes_preserve_selection();
                self.note_changed(&nb.name);
                self.set_status(format!("reverted to {short}"));
                self.show_history = false;
                self.history_viewing = None;
                self.history_diff_viewing = None;
                self.history_count_cache = None;
            }
            Err(e) => self.set_status(format!("revert error: {e}")),
        }
    }
    /// Reopens the conflict resolver for the currently selected notebook,
    /// if it's genuinely mid-merge — the manual "I closed the modal with
    /// `Esc` and want it back" path, since closing it (unlike every other
    /// close-only-forward modal) doesn't resolve or abort anything.
    pub(crate) fn reopen_conflicts_if_merging(&mut self) {
        let Some(nb) = self.selected_notebook().cloned() else {
            self.set_status("no notebook selected".into());
            return;
        };
        if !shiki_core::git::merge_in_progress(&nb.path) {
            self.set_status(format!("'{}' has no merge in progress", nb.name));
            return;
        }
        match shiki_core::git::conflicted_files(&nb.path) {
            Ok(files) => {
                self.conflict_notebook = nb.name.clone();
                self.conflict_files = files;
                self.conflict_selected = 0;
                self.conflict_branch = self.config.git.branch.clone();
                self.conflict_viewing = None;
                self.show_conflicts = true;
            }
            Err(e) => self.set_status(format!("could not read conflicts: {e}")),
        }
    }
    /// Dispatcher for the conflict resolver modal — two tiers, same shape
    /// as `handle_history_key`'s diff-viewing/list split: `conflict_viewing`
    /// drills into one file's side-by-side ours/theirs diff; otherwise it's
    /// the flat conflicted-file list.
    fn handle_conflicts_key(&mut self, key: KeyEvent) {
        if self.conflict_viewing.is_some() {
            match key.code {
                KeyCode::Esc => self.conflict_viewing = None,
                KeyCode::Char('o') => self.resolve_selected_conflict(ConflictSideChoice::Ours),
                KeyCode::Char('t') => self.resolve_selected_conflict(ConflictSideChoice::Theirs),
                KeyCode::Char('e') => self.mark_selected_conflict_resolved(),
                KeyCode::Char('j') | KeyCode::Down => {
                    if let Some(view) = &mut self.conflict_viewing {
                        view.scroll = view.scroll.saturating_add(1);
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if let Some(view) = &mut self.conflict_viewing {
                        view.scroll = view.scroll.saturating_sub(1);
                    }
                }
                KeyCode::PageDown => {
                    let step = self.page_step().max(0) as u16;
                    if let Some(view) = &mut self.conflict_viewing {
                        view.scroll = view.scroll.saturating_add(step);
                    }
                }
                KeyCode::PageUp => {
                    let step = self.page_step().max(0) as u16;
                    if let Some(view) = &mut self.conflict_viewing {
                        view.scroll = view.scroll.saturating_sub(step);
                    }
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.show_conflicts = false,
            KeyCode::Enter => self.view_selected_conflict(),
            KeyCode::Char('o') => self.resolve_selected_conflict(ConflictSideChoice::Ours),
            KeyCode::Char('t') => self.resolve_selected_conflict(ConflictSideChoice::Theirs),
            KeyCode::Char('a') => self.start_abort_merge(),
            KeyCode::Char('j') | KeyCode::Down => {
                if self.conflict_selected + 1 < self.conflict_files.len() {
                    self.conflict_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.conflict_selected = self.conflict_selected.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.conflict_selected = (self.conflict_selected + self.page_step() as usize)
                    .min(self.conflict_files.len().saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.conflict_selected = self
                    .conflict_selected
                    .saturating_sub(self.page_step() as usize);
            }
            KeyCode::Home => self.conflict_selected = 0,
            KeyCode::End => self.conflict_selected = self.conflict_files.len().saturating_sub(1),
            _ => {}
        }
    }
    /// Loads the selected conflicted file's real side-by-side diff (ours
    /// and theirs, each against the common ancestor) via
    /// `shiki_core::git::conflict_diff` — the genuine two-pane comparison
    /// this modal exists for, not the unified single-pane style the history
    /// modal uses (there, there's only ever one side to show).
    fn view_selected_conflict(&mut self) {
        let Some(nb) = self.conflict_notebook_ref() else {
            return;
        };
        let Some(file) = self.conflict_files.get(self.conflict_selected).cloned() else {
            return;
        };
        match shiki_core::git::conflict_diff(&nb.path, &file) {
            Ok((ours, theirs)) => {
                self.conflict_viewing = Some(ConflictView {
                    file,
                    ours,
                    theirs,
                    scroll: 0,
                });
            }
            Err(e) => self.set_status(format!("could not load conflict: {e}")),
        }
    }
    /// Resolves the current file (the one drilled into, or the selected row
    /// in the flat list) to one whole side — "keep ours"/"keep theirs" is
    /// v1's resolution model; manual line-level merging isn't supported
    /// here (see `mark_selected_conflict_resolved` for the escape hatch:
    /// edit the file externally, then mark it resolved as-is).
    fn resolve_selected_conflict(&mut self, side: ConflictSideChoice) {
        let Some(nb) = self.conflict_notebook_ref() else {
            return;
        };
        let Some(file) = self.conflict_target_file() else {
            return;
        };
        let sides = match shiki_core::git::conflict_sides(&nb.path, &file) {
            Ok(s) => s,
            Err(e) => {
                self.set_status(format!("could not read conflict: {e}"));
                return;
            }
        };
        let content = match side {
            ConflictSideChoice::Ours => sides.ours,
            ConflictSideChoice::Theirs => sides.theirs,
        };
        let Some(content) = content else {
            self.set_status("that side doesn't have this file (it was deleted there)".into());
            return;
        };
        self.finish_resolving_conflict_file(&nb.path.clone(), &file, &content);
    }
    /// The escape hatch for anything more than a whole-side pick: the user
    /// edits the file by hand (external editor, or shiki's own editor
    /// outside this modal) to remove the conflict markers themselves, then
    /// `e` stages whatever's currently on disk as the resolution — no
    /// separate write, just re-reading and re-staging the working copy.
    fn mark_selected_conflict_resolved(&mut self) {
        let Some(nb) = self.conflict_notebook_ref() else {
            return;
        };
        let Some(file) = self.conflict_target_file() else {
            return;
        };
        let content = match std::fs::read_to_string(nb.path.join(&file)) {
            Ok(c) => c,
            Err(e) => {
                self.set_status(format!("could not read '{}': {e}", file.display()));
                return;
            }
        };
        self.finish_resolving_conflict_file(&nb.path.clone(), &file, &content);
    }
    fn finish_resolving_conflict_file(
        &mut self,
        nb_path: &std::path::Path,
        file: &std::path::Path,
        content: &str,
    ) {
        match shiki_core::git::resolve_conflict(nb_path, file, content) {
            Ok(()) => {
                self.conflict_files.retain(|f| f != file);
                self.conflict_selected = self
                    .conflict_selected
                    .min(self.conflict_files.len().saturating_sub(1));
                self.conflict_viewing = None;
                self.set_status(format!(
                    "resolved '{}' ({} left)",
                    file.display(),
                    self.conflict_files.len()
                ));
                if self.conflict_files.is_empty() {
                    self.pending_finish_merge = Some(self.conflict_notebook.clone());
                    self.confirm = Some(confirm::ConfirmDialog::new(
                        "all conflicts resolved — commit this merge?".to_string(),
                    ));
                }
            }
            Err(e) => self.set_status(format!("could not resolve '{}': {e}", file.display())),
        }
    }
    fn start_abort_merge(&mut self) {
        self.pending_abort_merge = Some(self.conflict_notebook.clone());
        self.confirm = Some(confirm::ConfirmDialog::new(
            "abort this merge? unresolved changes will be discarded".to_string(),
        ));
    }
    /// Commits the resolved merge — the `y` side of the "commit this
    /// merge?" confirm staged in `finish_resolving_conflict_file`.
    fn finish_merge_notebook(&mut self, notebook: &str) {
        let Some(nb) = self.notebooks.iter().find(|n| n.name == notebook).cloned() else {
            self.set_status(format!("'{notebook}' no longer available"));
            return;
        };
        let message = format!("shiki: merge '{}'", self.conflict_branch);
        match shiki_core::git::finish_merge(&nb.path, &message) {
            Ok(()) => {
                self.show_conflicts = false;
                self.conflict_files.clear();
                self.conflict_viewing = None;
                self.refresh_notes_preserve_selection();
                self.set_status(format!("'{notebook}': merge committed"));
            }
            Err(e) => self.set_status(format!("could not finish merge: {e}")),
        }
    }
    /// Discards the in-progress merge — the `y` side of `start_abort_merge`'s
    /// confirm.
    fn abort_merge_notebook(&mut self, notebook: &str) {
        let Some(nb) = self.notebooks.iter().find(|n| n.name == notebook).cloned() else {
            self.set_status(format!("'{notebook}' no longer available"));
            return;
        };
        match shiki_core::git::abort_merge(&nb.path) {
            Ok(()) => {
                self.show_conflicts = false;
                self.conflict_files.clear();
                self.conflict_viewing = None;
                self.refresh_notes_preserve_selection();
                self.set_status(format!("'{notebook}': merge aborted"));
            }
            Err(e) => self.set_status(format!("could not abort merge: {e}")),
        }
    }
    fn conflict_notebook_ref(&mut self) -> Option<Notebook> {
        let nb = self
            .notebooks
            .iter()
            .find(|n| n.name == self.conflict_notebook)
            .cloned();
        if nb.is_none() {
            self.set_status(format!("'{}' no longer available", self.conflict_notebook));
        }
        nb
    }
    /// The file a resolve action targets: whichever one is drilled into, or
    /// the selected row in the flat list otherwise.
    fn conflict_target_file(&self) -> Option<std::path::PathBuf> {
        self.conflict_viewing
            .as_ref()
            .map(|v| v.file.clone())
            .or_else(|| self.conflict_files.get(self.conflict_selected).cloned())
    }
    /// Flattens the selected notebook's whole folder tree and opens the tree
    /// view — every folder and note expanded at once, instead of navigating
    /// one level at a time.
    fn open_tree(&mut self) {
        let Some(nb) = self.selected_notebook() else {
            self.set_status("no notebook selected".into());
            return;
        };
        self.tree_rows = crate::tree::build(nb);
        self.tree_selected = 0;
        self.show_tree = true;
    }
    fn handle_tree_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.show_tree = false,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.tree_selected + 1 < self.tree_note_count() {
                    self.tree_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.tree_selected = self.tree_selected.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.tree_selected = (self.tree_selected + self.page_step() as usize)
                    .min(self.tree_note_count().saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.tree_selected = self.tree_selected.saturating_sub(self.page_step() as usize);
            }
            KeyCode::Home => self.tree_selected = 0,
            KeyCode::End => self.tree_selected = self.tree_note_count().saturating_sub(1),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.jump_to_tree_selection(),
            _ => {}
        }
    }
    /// The deep link: points the breadcrumb at the selected note's folder,
    /// reloads, selects it, and focuses the preview so it's ready to read.
    fn jump_to_tree_selection(&mut self) {
        let Some(row) = self.tree_selected_row() else {
            self.show_tree = false;
            return;
        };
        let crate::tree::TreeRow::Note { note, .. } = &self.tree_rows[row] else {
            self.show_tree = false;
            return;
        };
        let note_path = note.path.clone();
        let title = note.frontmatter.title.clone();
        let notebook_path = self.selected_notebook().map(|nb| nb.path.clone());
        if let Some(notebook_path) = notebook_path {
            self.notes_path = relative_folder(&note_path, &notebook_path);
        }
        self.reload_notes();
        if let Some(idx) = self.notes.iter().position(|n| n.path == note_path) {
            self.selected_note = self.folders.len() + idx;
        }
        self.focus = Focus::Preview;
        self.set_status(format!("opened '{title}'"));
        self.show_tree = false;
    }
    /// Opens the links modal for the currently selected note: its own
    /// outgoing `[[wikilinks]]` plus every other note in the notebook that
    /// links back to it. Built fresh every time it opens (like the tags
    /// modal) rather than kept in sync incrementally — cheap enough for a
    /// single note's worth of links, and it means an edit made just before
    /// opening this is never stale.
    fn open_links(&mut self) {
        let Some(note) = self.selected_note().cloned() else {
            self.set_status("select a note first".into());
            return;
        };
        let Some(nb) = self.selected_notebook() else {
            return;
        };
        let all_notes = nb.all_notes_recursive().unwrap_or_default();
        let global = self.store.all_notes().unwrap_or_default();
        self.link_rows = crate::links_panel::build(&note, &all_notes, &global);
        self.link_selected = 0;
        if self.link_rows.is_empty() {
            self.set_status("no links or backlinks for this note".into());
            return;
        }
        self.show_links = true;
    }
    fn handle_links_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.show_links = false,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.link_selected + 1 < self.link_selectable_count() {
                    self.link_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.link_selected = self.link_selected.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.link_selected = (self.link_selected + self.page_step() as usize)
                    .min(self.link_selectable_count().saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.link_selected = self.link_selected.saturating_sub(self.page_step() as usize);
            }
            KeyCode::Home => self.link_selected = 0,
            KeyCode::End => self.link_selected = self.link_selectable_count().saturating_sub(1),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.jump_to_link_selection(),
            KeyCode::Char('c') => self.link_selected_mention(),
            _ => {}
        }
    }
    /// `c` on a "Mentions (unlinked)" row: turns that note's plain-text
    /// mention into a real `[[wikilink]]` (`wikilinks::link_mention` — the
    /// repair to the missed link the section exists to surface). The modal
    /// is rebuilt afterwards, so the row visibly migrates from Mentions to
    /// Backlinks. A no-op on any other row kind.
    fn link_selected_mention(&mut self) {
        let Some(row) = crate::links_panel::selected_row(&self.link_rows, self.link_selected)
        else {
            return;
        };
        let crate::links_panel::LinkRow::Mention { note } = &self.link_rows[row] else {
            return;
        };
        let mention_path = note.path.clone();
        let mention_title = note.frontmatter.title.clone();
        let Some(target) = self.selected_note().cloned() else {
            return;
        };
        match shiki_core::wikilinks::link_mention(&mention_path, &target.frontmatter.title) {
            Ok(true) => {
                if let Some(nb) = self.selected_notebook() {
                    let name = nb.name.clone();
                    self.note_changed(&name);
                }
                self.refresh_notes_preserve_selection();
                self.set_status(format!(
                    "linked [[{}]] in '{mention_title}'",
                    target.frontmatter.title
                ));
                // Rebuild so the mention shows up under Backlinks now —
                // open_links re-reads everything from disk fresh.
                self.open_links();
            }
            Ok(false) => {
                self.set_status(format!(
                    "no linkable mention left in '{mention_title}' \u{2014} note changed on disk?"
                ));
            }
            Err(e) => self.set_status(format!("couldn't link mention: {e}")),
        }
    }
    /// The deep link: an outgoing link jumps to its resolved note (a broken
    /// one — no matching note found — just reports that instead), a
    /// backlink always jumps since `links_panel::build` only ever includes
    /// notes that actually resolved. Same "point breadcrumb at the note's
    /// folder, reload, select, focus PREVIEW" shape as `jump_to_tree_
    /// selection`/`jump_to_tag_note`/`jump_to_global_hit`.
    fn jump_to_link_selection(&mut self) {
        let Some(row) = crate::links_panel::selected_row(&self.link_rows, self.link_selected)
        else {
            self.show_links = false;
            return;
        };
        let (note_path, title) = match &self.link_rows[row] {
            crate::links_panel::LinkRow::Outgoing {
                resolved: Some(path),
                text,
            } => (path.clone(), text.clone()),
            crate::links_panel::LinkRow::Outgoing {
                resolved: None,
                text,
            } => {
                self.set_status(format!("'{text}' doesn't match any note"));
                return;
            }
            crate::links_panel::LinkRow::Backlink { note }
            | crate::links_panel::LinkRow::Mention { note } => {
                (note.path.clone(), note.frontmatter.title.clone())
            }
            crate::links_panel::LinkRow::Header(_) => {
                self.show_links = false;
                return;
            }
        };
        self.jump_to_note(note_path, &title);
        self.show_links = false;
    }
    /// Points the breadcrumb at `path`'s folder, reloads NOTES, selects it,
    /// and focuses PREVIEW — the deep-link tail shared by every "jump
    /// straight to a resolved note" flow (the links modal above, and
    /// Ctrl+Click on a rendered `[[wikilink]]` in PREVIEW below). Each
    /// caller still owns closing whatever modal/state got it here.
    ///
    /// Cross-notebook aware: if `path` lives in a *different* notebook than
    /// the one currently selected (a link resolved through
    /// `wikilinks::resolve_one_global`'s fallback, e.g. a daily-note agenda
    /// bullet pointing at another notebook), it switches
    /// `selected_notebook` first so the breadcrumb and NOTES reload are
    /// computed against the right notebook root — the same shape as
    /// `jump_to_global_hit`, just resolved by path-prefix instead of a pool
    /// index.
    fn jump_to_note(&mut self, path: std::path::PathBuf, title: &str) {
        if let Some((nb_idx, nb)) = self
            .notebooks
            .iter()
            .enumerate()
            .find(|(_, nb)| path.starts_with(&nb.path))
        {
            if self.selected_notebook != nb_idx {
                self.selected_notebook = nb_idx;
            }
            self.notes_path = relative_folder(&path, &nb.path);
        }
        self.reload_notes();
        if let Some(idx) = self.notes.iter().position(|n| n.path == path) {
            self.selected_note = self.folders.len() + idx;
        }
        self.focus = Focus::Preview;
        self.set_status(format!("opened '{title}'"));
    }

    /// Opens the global tasks view: every checkbox task across every
    /// notebook (`NotebookStore::all_notes`, the same walk global search
    /// does on open), pending-only unless `config.general.tasks_show_done_default`
    /// says otherwise. Built fresh every time it opens, same as the
    /// tags/links modals.
    fn open_tasks(&mut self) {
        self.tasks_show_done = self.config.general.tasks_show_done_default;
        self.task_selected = 0;
        self.rebuild_task_rows();
        if self.task_rows.is_empty() {
            self.set_status("no pending tasks in any notebook".into());
            return;
        }
        self.show_tasks = true;
    }
    fn rebuild_task_rows(&mut self) {
        let pool = self.store.all_notes().unwrap_or_default();
        self.task_rows = crate::panel_tasks::build(&pool, self.tasks_show_done);
    }
    fn task_selectable_count(&self) -> usize {
        self.task_rows.len()
    }
    fn handle_tasks_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.show_tasks = false,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.task_selected + 1 < self.task_selectable_count() {
                    self.task_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.task_selected = self.task_selected.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.task_selected = (self.task_selected + self.page_step() as usize)
                    .min(self.task_selectable_count().saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.task_selected = self.task_selected.saturating_sub(self.page_step() as usize);
            }
            KeyCode::Home => self.task_selected = 0,
            KeyCode::End => self.task_selected = self.task_selectable_count().saturating_sub(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.toggle_selected_task(),
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Char('o') => self.jump_to_task_note(),
            KeyCode::Char('a') => {
                self.tasks_show_done = !self.tasks_show_done;
                self.rebuild_task_rows();
                self.task_selected = self
                    .task_selected
                    .min(self.task_selectable_count().saturating_sub(1));
            }
            _ => {}
        }
    }
    /// Flips the selected task's checkbox in its source file and updates
    /// the row in place — deliberately *not* a rebuild, so a task checked
    /// off while the pending-only filter is active stays visible (checked,
    /// struck through) and can be immediately un-toggled instead of
    /// vanishing out from under the cursor. The one exception is completing
    /// an `@every(...)` task: `shiki_core::tasks::toggle` itself spawned a
    /// brand-new line for the next occurrence, which the in-place patch
    /// below has no way to also insert — a full `rebuild_task_rows` is the
    /// only way that new row can show up at all.
    fn toggle_selected_task(&mut self) {
        let Some(row) = self.task_rows.get(self.task_selected) else {
            return;
        };
        let notebook = row.notebook.clone();
        let note_path = row.note_path.clone();
        let task = &row.task;
        match shiki_core::tasks::toggle(&note_path, &task.raw_line, task.occurrence) {
            Ok(toggled) => {
                let done = toggled.done;
                self.refresh_notes_preserve_selection();
                self.note_changed(&notebook);
                if let Some(next_due) = toggled.spawned_next {
                    self.rebuild_task_rows();
                    self.task_selected = self
                        .task_selected
                        .min(self.task_selectable_count().saturating_sub(1));
                    self.set_status(format!("task completed — next due {next_due}"));
                } else {
                    let task = &mut self.task_rows[self.task_selected].task;
                    task.done = toggled.done;
                    task.raw_line = toggled.raw_line;
                    task.occurrence = toggled.occurrence;
                    self.set_status(format!(
                        "task {}",
                        if done { "completed" } else { "reopened" }
                    ));
                }
            }
            Err(e) => self.set_status(format!("couldn't toggle task: {e}")),
        }
    }
    /// Cross-notebook jump to the selected task's note — same shape as
    /// `jump_to_global_hit`, which is the other flow that can land on a
    /// note in a *different* notebook than the selected one.
    fn jump_to_task_note(&mut self) {
        let Some(row) = self.task_rows.get(self.task_selected) else {
            return;
        };
        let notebook = row.notebook.clone();
        let note_path = row.note_path.clone();
        if let Some(nb_idx) = self.notebooks.iter().position(|n| n.name == notebook) {
            self.selected_notebook = nb_idx;
            let nb_path = self.notebooks[nb_idx].path.clone();
            self.notes_path = relative_folder(&note_path, &nb_path);
            self.reload_notes();
            if let Some(idx) = self.notes.iter().position(|n| n.path == note_path) {
                self.selected_note = self.folders.len() + idx;
            }
            self.focus = Focus::Preview;
            if let Some(note) = self.selected_note() {
                let title = note.frontmatter.title.clone();
                self.set_status(format!("opened '{title}'"));
            }
        }
        self.show_tasks = false;
    }
    /// Loads the note pool **once**, on open — same "expensive walk once,
    /// cheap re-filter per keystroke" split `global_search_pool`/
    /// `wikilink_candidates` already established, rather than re-walking
    /// every notebook's frontmatter on every typed character.
    fn open_query(&mut self) {
        self.query_input.clear();
        self.query_selected = 0;
        self.query_error = None;
        self.query_rows.clear();
        self.query_pool = self.store.all_notes().unwrap_or_default();
        self.query_known_fields = shiki_core::query::known_fields(&self.query_pool);
        self.query_suggestions = self.build_query_suggestions(&self.query_pool);
        self.query_suggestions_visible = self.query_suggestions.clone();
        self.show_query = true;
    }
    /// Every saved query (`Config.queries`, alphabetical by name) first,
    /// then the generated examples (`shiki_core::query::suggest_queries`
    /// over `pool`) — saved ones lead since they're a deliberate choice the
    /// user already made, more relevant than an auto-generated guess at
    /// what might be useful. Takes `pool` explicitly (rather than reading
    /// `self.query_pool`) so both query surfaces can call this against
    /// their own already-loaded pool (`query_pool`/`global_search_pool`)
    /// without one having to shadow the other's field first.
    fn build_query_suggestions(
        &self,
        pool: &[(shiki_core::Notebook, shiki_core::Note)],
    ) -> Vec<QuerySuggestion> {
        let mut saved: Vec<(&String, &String)> = self.config.queries.iter().collect();
        saved.sort_by_key(|(name, _)| name.to_lowercase());
        let mut list: Vec<QuerySuggestion> = saved
            .into_iter()
            .map(|(name, dsl)| QuerySuggestion::saved(name, dsl.clone()))
            .collect();
        list.extend(
            shiki_core::query::suggest_queries(pool)
                .into_iter()
                .map(QuerySuggestion::generated),
        );
        list
    }
    /// Filters `query_suggestions` down to ones whose display text or
    /// underlying DSL contains `text` — used both for the immediate
    /// "here's what you can ask" list (empty text, so every suggestion
    /// matches) and, once typing starts, as a live "did you mean" list for
    /// whatever's in progress but doesn't parse yet. Shared by
    /// `refresh_query` and `refresh_global_search`'s query mode so the two
    /// can't drift into filtering differently.
    fn matching_suggestions(&self, text: &str) -> Vec<QuerySuggestion> {
        let needle = text.trim().to_lowercase();
        self.query_suggestions
            .iter()
            .filter(|s| {
                needle.is_empty()
                    || s.display.to_lowercase().contains(&needle)
                    || s.dsl.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect()
    }
    /// Re-parses and re-evaluates the DSL against the already-loaded
    /// `query_pool` — pure in-memory work, safe to redo on every keystroke.
    /// An empty box shows every suggestion (the "here's what you can ask"
    /// list) rather than a blank screen; a query that doesn't parse yet
    /// shows whichever suggestions still match what's typed so far instead
    /// of a bare error, falling back to the real parse error (with its own
    /// field hint, see `panel_query::render_result_table`) only once no
    /// suggestion matches either.
    fn refresh_query(&mut self) {
        let text = self.query_input.value.trim().to_string();
        if text.is_empty() {
            self.query_error = None;
            self.query_rows.clear();
            self.query_suggestions_visible = self.query_suggestions.clone();
            self.query_selected = 0;
            return;
        }
        let today = chrono::Local::now().date_naive();
        match shiki_core::query::parse(&text) {
            Ok(q) => {
                self.query_error = None;
                self.query_suggestions_visible.clear();
                self.query_rows = shiki_core::query::run_query(&self.query_pool, &q, None, today);
            }
            Err(e) => {
                self.query_rows.clear();
                let matches = self.matching_suggestions(&text);
                if matches.is_empty() {
                    self.query_error = Some(e.to_string());
                    self.query_suggestions_visible.clear();
                } else {
                    self.query_error = None;
                    self.query_suggestions_visible = matches;
                }
            }
        }
        let visible_len = if self.query_suggestions_visible.is_empty() {
            self.query_rows.len()
        } else {
            self.query_suggestions_visible.len()
        };
        self.query_selected = self.query_selected.min(visible_len.saturating_sub(1));
    }
    /// Navigation is arrows/PageUp/PageDown/Home/End only, deliberately no
    /// `j`/`k` — same reasoning as `which.rs`: those letters need to be
    /// typeable into the query itself.
    fn handle_query_key(&mut self, key: KeyEvent) {
        let showing_suggestions = !self.query_suggestions_visible.is_empty();
        let len = if showing_suggestions {
            self.query_suggestions_visible.len()
        } else {
            self.query_rows.len()
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('s') if ctrl => self.start_save_query_prompt(),
            KeyCode::Char('d') if ctrl && showing_suggestions => {
                if let Some(name) = self
                    .query_suggestions_visible
                    .get(self.query_selected)
                    .and_then(|s| s.saved_name.clone())
                {
                    self.delete_saved_query(&name);
                }
            }
            KeyCode::Esc => self.show_query = false,
            KeyCode::Enter => {
                if showing_suggestions {
                    if let Some(s) = self.query_suggestions_visible.get(self.query_selected) {
                        self.query_input.value = s.dsl.clone();
                        self.refresh_query();
                    }
                } else {
                    self.jump_to_query_note();
                }
            }
            KeyCode::Down => {
                if self.query_selected + 1 < len {
                    self.query_selected += 1;
                }
            }
            KeyCode::Up => self.query_selected = self.query_selected.saturating_sub(1),
            KeyCode::PageDown => {
                self.query_selected =
                    (self.query_selected + self.page_step() as usize).min(len.saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.query_selected = self
                    .query_selected
                    .saturating_sub(self.page_step() as usize);
            }
            KeyCode::Home => self.query_selected = 0,
            KeyCode::End => self.query_selected = len.saturating_sub(1),
            KeyCode::Backspace => {
                self.query_input.backspace();
                self.refresh_query();
            }
            KeyCode::Char(c) => {
                self.query_input.push(c);
                self.refresh_query();
            }
            _ => {}
        }
    }
    /// `Ctrl+S` in the query modal — only a query that already parses is
    /// worth saving (a broken one saved under a name would just replay the
    /// same parse error every time it's picked from the suggestions list
    /// later). Hides the modal and opens the name prompt, same "hide
    /// yourself, restore after" convention as `start_metadata_prompt`/
    /// `handle_tags_key`'s `r`.
    fn start_save_query_prompt(&mut self) {
        let text = self.query_input.value.trim().to_string();
        if text.is_empty() || self.query_error.is_some() {
            self.set_status("type a valid query first".into());
            return;
        }
        self.pending_save_query_dsl = Some(text);
        self.show_query = false;
        self.start_input(PendingInput::SaveQuery, String::new());
    }
    /// Removes a saved query and immediately rebuilds the suggestions list
    /// so it stops showing up — reuses `refresh_query` afterward rather
    /// than hand-patching `query_suggestions_visible`, since the deleted
    /// entry might currently be the *filtered* list shown, not just the
    /// full one.
    fn delete_saved_query(&mut self, name: &str) {
        self.config.queries.remove(name);
        self.save_config();
        self.query_suggestions = self.build_query_suggestions(&self.query_pool);
        self.refresh_query();
        self.set_status(format!("deleted saved query '{name}'"));
    }
    /// Writes `pending_save_query_dsl` into `config.queries` under the
    /// typed name and persists it — an existing name overwrites (reported
    /// distinctly from a fresh save, so overwriting isn't a silent
    /// surprise). Reopens the query modal either way, same as
    /// `confirm_rename_tag` does for its own modal.
    fn confirm_save_query(&mut self, value: String) {
        let Some(dsl) = self.pending_save_query_dsl.take() else {
            self.show_query = true;
            return;
        };
        let name = value.trim().to_string();
        if name.is_empty() {
            self.set_status("save cancelled (name can't be empty)".into());
        } else {
            let overwritten = self.config.queries.contains_key(&name);
            self.config.queries.insert(name.clone(), dsl);
            self.save_config();
            self.query_suggestions = self.build_query_suggestions(&self.query_pool);
            self.set_status(format!(
                "{} query '{name}'",
                if overwritten {
                    "updated saved"
                } else {
                    "saved"
                }
            ));
        }
        self.show_query = true;
    }
    /// Cross-notebook jump to the selected result's note — same shape as
    /// `jump_to_task_note`/`jump_to_global_hit`.
    fn jump_to_query_note(&mut self) {
        let Some(row) = self.query_rows.get(self.query_selected) else {
            return;
        };
        let notebook = row.notebook.clone();
        let note_path = row.path.clone();
        if let Some(nb_idx) = self.notebooks.iter().position(|n| n.name == notebook) {
            self.selected_notebook = nb_idx;
            let nb_path = self.notebooks[nb_idx].path.clone();
            self.notes_path = relative_folder(&note_path, &nb_path);
            self.reload_notes();
            if let Some(idx) = self.notes.iter().position(|n| n.path == note_path) {
                self.selected_note = self.folders.len() + idx;
            }
            self.focus = Focus::Preview;
            if let Some(note) = self.selected_note() {
                let title = note.frontmatter.title.clone();
                self.set_status(format!("opened '{title}'"));
            }
        }
        self.show_query = false;
    }
    /// Opens the metadata modal for the selected note — a no-op with a
    /// status message rather than a blank modal when nothing's selected
    /// (e.g. NOTES is on an empty notebook).
    fn open_metadata(&mut self) {
        if self.selected_note().is_none() {
            self.set_status("select a note first".into());
            return;
        }
        self.metadata_selected = 0;
        self.show_metadata = true;
    }
    /// `tags` is always the first row, even when empty — it's the one
    /// always-present, always-discoverable place to add tags without a
    /// separate mechanism. Every other row is one `extra` frontmatter
    /// field, in the YAML file's own order (a `serde_yaml::Mapping`
    /// preserves insertion order, so this matches what's actually on disk).
    pub(crate) fn metadata_rows(&self) -> Vec<(String, String)> {
        let Some(note) = self.selected_note() else {
            return Vec::new();
        };
        let mut rows = vec![("tags".to_string(), note.frontmatter.tags.join(", "))];
        for (k, v) in note.frontmatter.extra.iter() {
            if let Some(key) = k.as_str() {
                rows.push((key.to_string(), panel_query::yaml_cell_text(v)));
            }
        }
        rows
    }
    /// Hides the modal and opens the shared `PendingInput::Metadata` prompt
    /// — every metadata edit funnels through here so hiding/restoring
    /// `show_metadata` and stamping an explicit `pending_input_title` (this
    /// prompt's meaning changes per step, unlike most `PendingInput`
    /// variants — see `MetadataPrompt`) can't be forgotten in one call site
    /// but not another.
    fn start_metadata_prompt(&mut self, kind: MetadataPrompt, title: String, prefill: String) {
        self.show_metadata = false;
        let pool = self.store.all_notes().unwrap_or_default();
        let input_value = match &kind {
            // Tags keeps its prefill (the note's current comma-separated
            // list) — unlike a single-value field, there's nothing to
            // "select instead of the current value", only more to append,
            // so the box starts exactly where editing naturally continues.
            MetadataPrompt::Tags => {
                self.metadata_value_options = shiki_core::tags::all_tags(&pool);
                self.metadata_value_selected = 0;
                prefill
            }
            MetadataPrompt::NewFieldKey => {
                self.metadata_value_options = Vec::new();
                self.metadata_value_selected = 0;
                prefill
            }
            MetadataPrompt::FieldValue(field) | MetadataPrompt::NewFieldValue(field) => {
                let options = Self::metadata_value_suggestions(&pool, field);
                // When there are suggestions, the box starts *empty* rather
                // than prefilled with the current value — prefilling it
                // would make the live substring filter immediately narrow
                // the dropdown down to just that one value (nothing else
                // contains it), defeating the point of seeing every option
                // at once. The current value isn't lost: `metadata_value_
                // selected` points at its position in the list instead, so
                // it's already highlighted and a bare `Enter` with nothing
                // typed keeps it unchanged, same safety a real prefill
                // would have given.
                let value = if options.is_empty() {
                    self.metadata_value_selected = 0;
                    prefill
                } else {
                    self.metadata_value_selected = options
                        .iter()
                        .position(|o| o.eq_ignore_ascii_case(&prefill))
                        .unwrap_or(0);
                    String::new()
                };
                self.metadata_value_options = options;
                value
            }
        };
        self.metadata_prompt = Some(kind);
        self.start_input(PendingInput::Metadata, input_value);
        self.pending_input_title = Some(title);
    }
    /// `true` while the prompt currently open is specifically the `Tags`
    /// one — the one case where the box holds several comma-separated
    /// values instead of one, so the suggestions dropdown has to filter/
    /// insert against just the segment being typed, not the whole box (see
    /// `metadata_value_query`/`metadata_value_filtered` and the `Tab`
    /// handling in `handle_insert_key`).
    pub(crate) fn is_tags_prompt(&self) -> bool {
        matches!(self.metadata_prompt, Some(MetadataPrompt::Tags))
    }
    /// Built-in suggestions for the handful of field names that have an
    /// obvious convention — `due` in particular resolves relative to *now*
    /// (never cached), so re-opening the prompt tomorrow suggests tomorrow's
    /// dates, not stale ones from whenever the app started. Any other field
    /// name (e.g. `project`) has no universal default; it's suggested purely
    /// from history via `metadata_value_suggestions`.
    fn default_metadata_value_suggestions(field: &str) -> Vec<String> {
        let today = chrono::Local::now().date_naive();
        match field.to_ascii_lowercase().as_str() {
            "status" => ["pending", "in-progress", "done"]
                .into_iter()
                .map(String::from)
                .collect(),
            "priority" => ["high", "medium", "low"]
                .into_iter()
                .map(String::from)
                .collect(),
            "due" => [
                today,
                today.succ_opt().unwrap_or(today),
                today + chrono::Duration::days(7),
            ]
            .into_iter()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .collect(),
            _ => Vec::new(),
        }
    }
    /// Built-in defaults first (the common, "hasn't been used yet" case),
    /// then every value already used for `field` anywhere across every
    /// notebook (`shiki_core::query::field_values`, deduped against the
    /// defaults) — `pool` is passed in (rather than fetched here) so
    /// `start_metadata_prompt` only walks every notebook once regardless of
    /// which kind of prompt it's opening.
    fn metadata_value_suggestions(
        pool: &[(shiki_core::Notebook, shiki_core::Note)],
        field: &str,
    ) -> Vec<String> {
        let mut values = Self::default_metadata_value_suggestions(field);
        for v in shiki_core::query::field_values(pool, field) {
            if !values.iter().any(|d| d.eq_ignore_ascii_case(&v)) {
                values.push(v);
            }
        }
        values
    }
    /// `Some` only while a `PendingInput::Metadata` prompt has suggestions
    /// to show (`NewFieldKey` never does — see `start_metadata_prompt`) —
    /// gates the suggestions dropdown in `handle_insert_key`/`draw.rs`,
    /// same shape as `quick_template_query`. For `Tags`, this is only the
    /// segment after the last comma (whatever's currently being typed),
    /// not the whole box — matching several already-picked tags against
    /// "what's typed" would filter the dropdown down to nothing the moment
    /// there's more than one tag.
    pub(crate) fn metadata_value_query(&self) -> Option<&str> {
        if self.pending_input != Some(PendingInput::Metadata)
            || self.metadata_value_options.is_empty()
        {
            return None;
        }
        Some(if self.is_tags_prompt() {
            self.input
                .value
                .rsplit(',')
                .next()
                .unwrap_or("")
                .trim_start()
        } else {
            self.input.value.as_str()
        })
    }
    /// `metadata_value_options` narrowed to whatever's typed so far
    /// (case-insensitive substring, same filter shape as
    /// `App::matching_suggestions` for the query modal) — empty input
    /// shows every suggestion, exactly like that one too. For `Tags`,
    /// additionally drops any tag already picked earlier in the same
    /// comma-separated list, so a half-finished "work, wor" doesn't offer
    /// "work" again as if it were a fresh option.
    pub(crate) fn metadata_value_filtered(&self) -> Vec<String> {
        let Some(query) = self.metadata_value_query() else {
            return Vec::new();
        };
        let needle = query.to_lowercase();
        let already_picked: Vec<String> = if self.is_tags_prompt() {
            self.input
                .value
                .split(',')
                .rev()
                .skip(1)
                .map(|s| s.trim().to_lowercase())
                .collect()
        } else {
            Vec::new()
        };
        self.metadata_value_options
            .iter()
            .filter(|v| {
                (needle.is_empty() || v.to_lowercase().contains(&needle))
                    && !already_picked.contains(&v.to_lowercase())
            })
            .cloned()
            .collect()
    }
    /// `Tab`-only (see `handle_insert_key`): replaces the segment currently
    /// being typed (after the last comma) with `suggestion` and appends a
    /// fresh `", "` separator, ready to keep typing the next tag — doesn't
    /// touch or confirm the rest of the box, unlike a single-value field's
    /// `Enter` (`self.input.value = s`), since there can be other
    /// already-picked tags before it that must survive untouched.
    fn apply_tag_suggestion(&mut self, suggestion: &str) {
        match self.input.value.rfind(',') {
            Some(pos) => {
                self.input.value.truncate(pos + 1);
                self.input.value.push(' ');
            }
            None => self.input.value.clear(),
        }
        self.input.value.push_str(suggestion);
        self.input.value.push_str(", ");
        self.metadata_value_selected = 0;
    }
    fn handle_metadata_key(&mut self, key: KeyEvent) {
        let rows = self.metadata_rows();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.show_metadata = false,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.metadata_selected + 1 < rows.len() {
                    self.metadata_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.metadata_selected = self.metadata_selected.saturating_sub(1);
            }
            KeyCode::Char('a') => {
                self.start_metadata_prompt(
                    MetadataPrompt::NewFieldKey,
                    " New field name ".into(),
                    String::new(),
                );
            }
            KeyCode::Enter => {
                if let Some((key, value)) = rows.get(self.metadata_selected).cloned() {
                    if key == "tags" {
                        self.start_metadata_prompt(
                            MetadataPrompt::Tags,
                            " Tags (comma-separated) ".into(),
                            value,
                        );
                    } else {
                        self.start_metadata_prompt(
                            MetadataPrompt::FieldValue(key.clone()),
                            format!(" Value for '{key}' (current: {value}) "),
                            value,
                        );
                    }
                }
            }
            KeyCode::Char('d') => {
                if let Some((key, _)) = rows.get(self.metadata_selected).cloned() {
                    if let Some(mut note) = self.selected_note().cloned() {
                        if key == "tags" {
                            note.frontmatter.tags.clear();
                        } else {
                            note.frontmatter
                                .extra
                                .remove(serde_yaml::Value::String(key));
                        }
                        self.save_metadata_note(note);
                        self.metadata_selected = self
                            .metadata_selected
                            .min(self.metadata_rows().len().saturating_sub(1));
                    }
                }
            }
            _ => {}
        }
    }
    /// Resolves whichever step of the metadata prompt flow just confirmed
    /// (see `MetadataPrompt`) — `NewFieldKey` is the one non-terminal step,
    /// chaining straight into `NewFieldValue` via `start_metadata_prompt`
    /// again instead of saving anything yet; every other step is terminal
    /// and restores `show_metadata` once it's done.
    fn confirm_metadata_prompt(&mut self, value: String) {
        match self.metadata_prompt.take() {
            Some(MetadataPrompt::Tags) => {
                if let Some(mut note) = self.selected_note().cloned() {
                    note.frontmatter.tags = value
                        .split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect();
                    self.save_metadata_note(note);
                }
                self.show_metadata = true;
            }
            Some(MetadataPrompt::NewFieldKey) => {
                let key = value.trim().to_string();
                if key.is_empty() {
                    self.set_status("metadata field cancelled (name can't be empty)".into());
                    self.show_metadata = true;
                } else {
                    self.start_metadata_prompt(
                        MetadataPrompt::NewFieldValue(key.clone()),
                        format!(" Value for '{key}' "),
                        String::new(),
                    );
                }
            }
            Some(MetadataPrompt::NewFieldValue(key)) | Some(MetadataPrompt::FieldValue(key)) => {
                if let Some(mut note) = self.selected_note().cloned() {
                    note.frontmatter.extra.insert(
                        serde_yaml::Value::String(key),
                        serde_yaml::Value::String(value),
                    );
                    self.save_metadata_note(note);
                }
                self.show_metadata = true;
            }
            None => {}
        }
    }
    /// Every metadata edit funnels through here — same crypto-aware save
    /// path `save_and_exit_edit` uses for the note's body, just for
    /// frontmatter instead. Refreshes the same caches a body edit would
    /// (`note_changed` for auto-sync's counter, `refresh_notes_preserve_selection`
    /// so PREVIEW's metadata header reflects the change immediately).
    fn save_metadata_note(&mut self, note: shiki_core::Note) {
        let crypto = self.selected_notebook().and_then(|nb| nb.crypto.clone());
        match note.save_with_crypto(crypto.as_ref()) {
            Ok(_) => {
                self.note_changed(&note.frontmatter.notebook);
                self.refresh_notes_preserve_selection();
                self.set_status("metadata saved".into());
            }
            Err(e) => self.set_status(format!("could not save metadata: {e}")),
        }
    }
    /// The tags modal has two levels: the tag list itself, and (after
    /// drilling into one) the notes that carry it — reset to level 1 every
    /// time it opens, so it never reopens showing a stale drill-down from
    /// last time.
    fn toggle_tags(&mut self) {
        self.show_tags = !self.show_tags;
        if self.show_tags {
            self.tags_selected = 0;
            self.tags_viewing = None;
            self.tags_notes_selected = 0;
        }
    }
    fn handle_tags_key(&mut self, key: KeyEvent) {
        let Some(tag) = self.tags_viewing.clone() else {
            let tags = self.current_tags();
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.show_tags = false,
                KeyCode::Char('j') | KeyCode::Down => {
                    if self.tags_selected + 1 < tags.len() {
                        self.tags_selected += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.tags_selected = self.tags_selected.saturating_sub(1);
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    if let Some(tag) = tags.get(self.tags_selected) {
                        self.tags_viewing = Some(tag.clone());
                        self.tags_notes_selected = 0;
                    }
                }
                KeyCode::Char('r') => {
                    if let Some(tag) = tags.get(self.tags_selected).cloned() {
                        self.pending_rename_tag = Some(tag.clone());
                        self.show_tags = false;
                        self.start_input(PendingInput::RenameTag, tag);
                    }
                }
                _ => {}
            }
            return;
        };

        let notes_len = self.notes_with_tag(&tag).len();
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') | KeyCode::Backspace | KeyCode::Left => {
                self.tags_viewing = None;
            }
            KeyCode::Char('q') => self.show_tags = false,
            KeyCode::Char('j') | KeyCode::Down => {
                if self.tags_notes_selected + 1 < notes_len {
                    self.tags_notes_selected += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.tags_notes_selected = self.tags_notes_selected.saturating_sub(1);
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.jump_to_tag_note(&tag),
            _ => {}
        }
    }
    /// The deep link from level 2 of the tags modal: every match is already
    /// in the current directory's `self.notes` (see `notes_with_tag`), so
    /// this is just "select it and close", not a full `reload_notes` jump
    /// like the tree view's/global search's cross-folder equivalents.
    fn jump_to_tag_note(&mut self, tag: &str) {
        let target = self
            .notes_with_tag(tag)
            .get(self.tags_notes_selected)
            .map(|n| n.path.clone());
        let Some(path) = target else {
            self.show_tags = false;
            return;
        };
        if let Some(idx) = self.notes.iter().position(|n| n.path == path) {
            self.selected_note = self.folders.len() + idx;
        }
        self.focus = Focus::Preview;
        self.show_tags = false;
        self.tags_viewing = None;
    }
    /// Renames (or merges) `pending_rename_tag` across every notebook —
    /// see `shiki_core::tags::rename_tag`'s own doc comment for why this
    /// deliberately isn't scoped to the current directory the tags modal
    /// itself browses. Reopens the tags modal either way (success or
    /// error), same "restore what hid you" convention every other
    /// modal-launched prompt in this codebase follows.
    fn confirm_rename_tag(&mut self, value: String) {
        let Some(old) = self.pending_rename_tag.take() else {
            self.show_tags = true;
            return;
        };
        let new = value.trim();
        if new.is_empty() || new == old {
            self.set_status("tag rename cancelled".into());
            self.show_tags = true;
            return;
        }
        let pool = self.store.all_notes().unwrap_or_default();
        match shiki_core::tags::rename_tag(&pool, &old, new) {
            Ok((count, notebooks)) => {
                for nb in &notebooks {
                    self.note_changed(nb);
                }
                self.refresh_notes_preserve_selection();
                self.set_status(format!(
                    "renamed tag '{old}' \u{2192} '{new}' in {count} note{}",
                    if count == 1 { "" } else { "s" }
                ));
            }
            Err(e) => self.set_status(format!("could not rename tag: {e}")),
        }
        self.show_tags = true;
    }
    /// Loads every note from every notebook and opens the global search modal.
    fn open_global_search(&mut self) {
        self.global_search_pool = self.store.all_notes().unwrap_or_default();
        self.global_search_input = InputBox::default();
        self.global_search_query_rows.clear();
        self.global_search_query_error = None;
        self.query_known_fields = shiki_core::query::known_fields(&self.global_search_pool);
        self.query_suggestions = self.build_query_suggestions(&self.global_search_pool);
        self.query_suggestions_visible.clear();
        self.refresh_global_search();
        self.show_global_search = true;
    }
    /// A leading `!` switches the box from plain text search into the query
    /// DSL (`shiki_core::query`) — same box, same pool, one extra character
    /// to opt in, so there's no separate modal to remember a binding for.
    pub(crate) fn global_search_is_query(&self) -> bool {
        self.global_search_input.value.starts_with('!')
    }
    /// Re-scores `global_search_pool` against the current query (title +
    /// body + notebook name, so this behaves like a grep across all notes,
    /// not just a title filter) — or, in query mode (see
    /// `global_search_is_query`), re-parses and re-evaluates the DSL
    /// against the same pool instead, mirroring `refresh_query` exactly
    /// (including the suggestions list — see `matching_suggestions`).
    fn refresh_global_search(&mut self) {
        if self.global_search_is_query() {
            self.global_search_results.clear();
            let text = self.global_search_input.value[1..].trim().to_string();
            if text.is_empty() {
                self.global_search_query_error = None;
                self.global_search_query_rows.clear();
                self.query_suggestions_visible = self.query_suggestions.clone();
            } else {
                let today = chrono::Local::now().date_naive();
                match shiki_core::query::parse(&text) {
                    Ok(q) => {
                        self.global_search_query_error = None;
                        self.query_suggestions_visible.clear();
                        self.global_search_query_rows =
                            shiki_core::query::run_query(&self.global_search_pool, &q, None, today);
                    }
                    Err(e) => {
                        self.global_search_query_rows.clear();
                        let matches = self.matching_suggestions(&text);
                        if matches.is_empty() {
                            self.global_search_query_error = Some(e.to_string());
                            self.query_suggestions_visible.clear();
                        } else {
                            self.global_search_query_error = None;
                            self.query_suggestions_visible = matches;
                        }
                    }
                }
            }
            let visible_len = if self.query_suggestions_visible.is_empty() {
                self.global_search_query_rows.len()
            } else {
                self.query_suggestions_visible.len()
            };
            self.global_search_selected = self
                .global_search_selected
                .min(visible_len.saturating_sub(1));
            return;
        }
        self.global_search_query_rows.clear();
        self.global_search_query_error = None;
        let query = self.global_search_input.value.clone();
        let haystacks: Vec<String> = self
            .global_search_pool
            .iter()
            .map(|(nb, note)| format!("{} {} {}", nb.name, note.frontmatter.title, note.body))
            .collect();
        let haystack_refs: Vec<&str> = haystacks.iter().map(String::as_str).collect();
        let mut hits = self.search_engine.search_text(&query, &haystack_refs);
        hits.truncate(30);
        self.global_search_results = hits;
        self.global_search_selected = 0;
    }
    fn handle_global_search_key(&mut self, key: KeyEvent) {
        let is_query = self.global_search_is_query();
        let showing_suggestions = is_query && !self.query_suggestions_visible.is_empty();
        let len = if showing_suggestions {
            self.query_suggestions_visible.len()
        } else if is_query {
            self.global_search_query_rows.len()
        } else {
            self.global_search_results.len()
        };
        match key.code {
            KeyCode::Esc => self.show_global_search = false,
            KeyCode::Enter => {
                if showing_suggestions {
                    if let Some(s) = self
                        .query_suggestions_visible
                        .get(self.global_search_selected)
                    {
                        self.global_search_input.value = format!("!{}", s.dsl);
                        self.refresh_global_search();
                    }
                } else if is_query {
                    self.jump_to_global_search_query_note();
                } else if let Some(hit) = self
                    .global_search_results
                    .get(self.global_search_selected)
                    .copied()
                {
                    self.jump_to_global_hit(hit.index);
                }
            }
            KeyCode::Down => {
                if self.global_search_selected + 1 < len {
                    self.global_search_selected += 1;
                }
            }
            KeyCode::Up => {
                self.global_search_selected = self.global_search_selected.saturating_sub(1)
            }
            KeyCode::PageDown => {
                self.global_search_selected = (self.global_search_selected
                    + self.page_step() as usize)
                    .min(len.saturating_sub(1));
            }
            KeyCode::PageUp => {
                self.global_search_selected = self
                    .global_search_selected
                    .saturating_sub(self.page_step() as usize);
            }
            KeyCode::Home => self.global_search_selected = 0,
            KeyCode::End => self.global_search_selected = len.saturating_sub(1),
            KeyCode::Backspace => {
                self.global_search_input.backspace();
                self.refresh_global_search();
            }
            KeyCode::Char(c) => {
                self.global_search_input.push(c);
                self.refresh_global_search();
            }
            _ => {}
        }
    }
    /// The deep link: switches to the hit's notebook, selects the note, and
    /// focuses the preview so you land reading it immediately.
    fn jump_to_global_hit(&mut self, pool_index: usize) {
        if let Some((nb, note)) = self.global_search_pool.get(pool_index).cloned() {
            if let Some(nb_idx) = self.notebooks.iter().position(|n| n.name == nb.name) {
                self.selected_notebook = nb_idx;
            }
            // The hit might be nested inside a subfolder of its notebook —
            // point the breadcrumb at it before reloading so it's visible.
            self.notes_path = relative_folder(&note.path, &nb.path);
            self.reload_notes();
            if let Some(note_idx) = self.notes.iter().position(|n| n.path == note.path) {
                self.selected_note = self.folders.len() + note_idx;
            }
            self.focus = Focus::Preview;
            self.set_status(format!("opened '{}'", note.frontmatter.title));
        }
        self.show_global_search = false;
    }
    /// Cross-notebook jump to a query-mode result — same shape as
    /// `jump_to_global_hit`/`jump_to_query_note`, just resolving by path
    /// against `global_search_pool` since `QueryRow` carries no pool index.
    fn jump_to_global_search_query_note(&mut self) {
        let Some(row) = self
            .global_search_query_rows
            .get(self.global_search_selected)
        else {
            return;
        };
        let notebook = row.notebook.clone();
        let note_path = row.path.clone();
        if let Some(nb_idx) = self.notebooks.iter().position(|n| n.name == notebook) {
            self.selected_notebook = nb_idx;
            let nb_path = self.notebooks[nb_idx].path.clone();
            self.notes_path = relative_folder(&note_path, &nb_path);
            self.reload_notes();
            if let Some(idx) = self.notes.iter().position(|n| n.path == note_path) {
                self.selected_note = self.folders.len() + idx;
            }
            self.focus = Focus::Preview;
            if let Some(note) = self.selected_note() {
                let title = note.frontmatter.title.clone();
                self.set_status(format!("opened '{title}'"));
            }
        }
        self.show_global_search = false;
    }
    /// Hit-tests a mouse click against the global search results list, using
    /// the same layout math `draw` used to render it last frame.
    fn global_search_hit_at(&self, col: u16, row: u16) -> Option<usize> {
        let popup_area = global_search_popup_area(self.last_frame_area);
        let (_, list_area) = global_search_layout(popup_area);
        let inner_left = list_area.x + 1;
        let inner_right = list_area.x + list_area.width.saturating_sub(1);
        let inner_top = list_area.y + 1;
        let inner_bottom = list_area.y + list_area.height.saturating_sub(1);
        if col < inner_left || col >= inner_right || row < inner_top || row >= inner_bottom {
            return None;
        }
        let index = (row - inner_top) as usize;
        (index < self.global_search_results.len()).then_some(index)
    }
    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Alt+Click adds a multi-cursor at the clicked position
                // instead of moving the primary cursor there — crossterm
                // doesn't report Alt-modified mouse events uniformly
                // across every terminal emulator, so a terminal that
                // doesn't forward the modifier just degrades gracefully
                // to a plain single-cursor click, never a crash or a
                // stuck state.
                if self.mode == Mode::Edit
                    && self.config.editor.mouse_selection
                    && self.config.editor.multi_cursor
                    && mouse.modifiers.contains(KeyModifiers::ALT)
                {
                    self.on_editor_alt_click(mouse.column, mouse.row);
                } else if self.mode != Mode::Edit
                    && mouse.modifiers.contains(KeyModifiers::CONTROL)
                    && self.try_follow_preview_wikilink(mouse.column, mouse.row)
                {
                    // Consumed: the click landed on a resolvable
                    // `[[wikilink]]` and already navigated there. A plain
                    // click still enters edit mode (see `on_mouse_down`) —
                    // this is deliberately opt-in via Ctrl so clicking to
                    // edit a note that happens to contain links doesn't
                    // become impossible.
                } else {
                    self.on_mouse_down(mouse.column, mouse.row);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.on_mouse_drag(mouse.column, mouse.row);
            }
            MouseEventKind::Up(MouseButton::Left) => self.on_mouse_up(),
            MouseEventKind::ScrollUp => self.on_mouse_scroll(-1),
            MouseEventKind::ScrollDown => self.on_mouse_scroll(1),
            _ => {}
        }
    }

    /// Mouse wheel scroll — never handled at all before this (verified
    /// live: scrolling over PREVIEW or the editor did nothing whatsoever).
    /// Moves the editor's cursor by a few rows (`Mode::Edit` — the only way
    /// to scroll the view at all, since `InlineEditor`'s own scroll offset
    /// is entirely cursor-driven, not an independent viewport state) or
    /// reuses `move_selection`'s existing delta logic (`Mode::Normal`/
    /// `Visual`, covers NOTEBOOKS/NOTES/PREVIEW identically to `j`/`k`) —
    /// gated on `no_modal_open()` so scrolling over a popup can't reach the
    /// layout underneath it.
    fn on_mouse_scroll(&mut self, dir: isize) {
        const SCROLL_STEP: isize = 3;
        if !self.no_modal_open() {
            return;
        }
        if self.mode == Mode::Edit {
            self.editor_scroll_cursor(dir * SCROLL_STEP);
        } else if self.mode == Mode::Normal || self.mode == Mode::Visual {
            self.move_selection(dir * SCROLL_STEP);
        }
    }

    /// Handles a terminal bracketed-paste event (`Event::Paste`, enabled
    /// unconditionally at startup — see `shiki-cli/src/tui.rs`). This has
    /// to be handled somewhere regardless of `config.editor.os_clipboard`:
    /// once bracketed-paste mode is on, *every* terminal paste anywhere in
    /// the app arrives this way instead of as a burst of individual
    /// `Event::Key`s, so silently dropping it here would break pasting
    /// into every text prompt in the app (new note, rename, global search,
    /// ...), not just the note editor.
    ///
    /// Plain-text editing (`Mode::Edit`, no find bar or slash menu open)
    /// gets the one genuine improvement: the whole paste lands as a single
    /// `insert_str` — one undo step, and immune to the `/`-menu
    /// mis-firing if the pasted text happens to start with `/` (see
    /// `handle_edit_key`'s own `/`-detection comment). Every other context
    /// (the find/replace bar, the slash menu, and any `InputBox`-driven
    /// prompt — new note, rename, global search, settings text fields,
    /// which-key's filter, ...) has no bulk-insert of its own, so it's
    /// replayed as if each character had arrived as an ordinary keystroke
    /// — exactly what it looked like before bracketed-paste mode existed.
    pub fn on_paste(&mut self, text: String) {
        if self.mode == Mode::Edit
            && self.editor_find.is_none()
            && !self.show_slash_menu
            && !self.show_wikilink_menu
        {
            let edits = self.insert_pasted_text(&text);
            if edits > 0 {
                self.editor_undo_groups.push(edits);
                self.editor_redo_groups.clear();
            }
            return;
        }
        for c in text.chars() {
            let code = if c == '\n' {
                KeyCode::Enter
            } else {
                KeyCode::Char(c)
            };
            self.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        }
    }

    /// Ctrl+Click on a rendered `[[wikilink]]` in PREVIEW jumps straight to
    /// the note it resolves to, instead of the plain click's "enter edit
    /// mode at this row" (`enter_edit_at_preview_row`) — same gate
    /// (`can_start_preview_selection`) a plain click already uses, so this
    /// respects `mouse_drag_selection`/`no_modal_open` identically. Returns
    /// `false` (falls through to the ordinary click handling in `on_mouse`)
    /// whenever the click isn't on a note, isn't on a preview row at all,
    /// or the row's rendered spans don't have a `[[...]]` one under the
    /// clicked column — Ctrl+Click on ordinary text still just edits.
    fn try_follow_preview_wikilink(&mut self, column: u16, row: u16) -> bool {
        if !self.can_start_preview_selection() {
            return false;
        }
        let preview = layout::split(self.last_frame_area, self.focus, self.zen_mode).preview;
        let content_left = preview.x + 1;
        if column < content_left {
            return false;
        }
        let row_count = self.note_preview_lines().map(|l| l.len()).unwrap_or(0);
        let Some(doc_row) =
            panel_preview::preview_row_at(preview, self.preview_scroll, row_count, column, row)
        else {
            return false;
        };
        let click_col = (column - content_left) as usize;
        let Some(text) = self
            .note_preview_lines()
            .and_then(|lines| lines.get(doc_row))
            .and_then(|line| panel_preview::wikilink_at(line, click_col))
        else {
            return false;
        };
        let Some(nb) = self.selected_notebook() else {
            return false;
        };
        let all_notes = nb.all_notes_recursive().unwrap_or_default();
        let global = self.store.all_notes().unwrap_or_default();
        match wikilinks::resolve_one_global(&text, &all_notes, &global) {
            Some((path, Some(other_notebook))) => {
                self.jump_to_note(path, &text);
                self.set_status(format!(
                    "opened '{text}' (from notebook '{other_notebook}')"
                ));
            }
            Some((path, None)) => self.jump_to_note(path, &text),
            None => self.set_status(format!("'{text}' doesn't match any note")),
        }
        true
    }
    fn on_mouse_down(&mut self, column: u16, row: u16) {
        if self.show_global_search {
            // Query mode renders a `Table` (with its own header row), not
            // the plain `List` this hit-test's math assumes — clicking is
            // a no-op there rather than risk jumping to the wrong row.
            if !self.global_search_is_query() {
                if let Some(index) = self.global_search_hit_at(column, row) {
                    if let Some(hit) = self.global_search_results.get(index).copied() {
                        self.jump_to_global_hit(hit.index);
                    }
                }
            }
            return;
        }
        if self.show_drawer {
            let area = drawer_area(self.last_frame_area, self.config.general.drawer_width);
            let hit = panel_drawer::drawer_hit_at(self.drawer_statuses.len(), area, column, row);
            match hit {
                Some(panel_drawer::DrawerHit::Notebook(index)) => {
                    self.drawer_selected = index;
                    self.jump_to_drawer_notebook();
                }
                Some(
                    panel_drawer::DrawerHit::NewButton | panel_drawer::DrawerHit::ImportButton,
                ) => {
                    self.show_drawer = false;
                    self.start_input(PendingInput::NewNotebook, String::new());
                }
                None => {}
            }
            return;
        }

        if self.mode == Mode::Edit && self.config.editor.mouse_selection {
            self.on_editor_mouse_down(column, row);
            return;
        }

        // Clicking a row in NOTEBOOKS or NOTES selects it and does exactly
        // what Right/Enter/`l` would do from there (`navigate_forward`) —
        // for NOTEBOOKS that's moving focus into NOTES; for a NOTES folder,
        // descending into it; for a NOTES note, moving focus into PREVIEW.
        // Restricted to `Mode::Normal` (Visual's click semantics — extend
        // the range? jump the anchor? — aren't designed yet, so this
        // deliberately doesn't touch it) and gated by `no_modal_open()` so a
        // click reaching the layout underneath an open popup can't be
        // misread as a panel click.
        if self.mode == Mode::Normal && self.no_modal_open() {
            let areas = layout::split(self.last_frame_area, self.focus, self.zen_mode);
            if let Some(index) = panel_notebooks::notebooks_hit_at(
                self.notebooks.len(),
                areas.notebooks,
                column,
                row,
            ) {
                self.focus = Focus::Notebooks;
                self.selected_notebook = index;
                self.notes_path.clear();
                self.reload_notes();
                self.navigate_forward();
                return;
            }
            let total_notes = self.folders.len() + self.notes.len();
            if let Some(index) = panel_notes::notes_hit_at(total_notes, areas.notes, column, row) {
                self.focus = Focus::Notes;
                self.selected_note = index;
                self.preview_scroll = 0;
                self.navigate_forward();
                return;
            }
        }

        if self.can_start_preview_selection() {
            let preview = layout::split(self.last_frame_area, self.focus, self.zen_mode).preview;
            let row_count = self.note_preview_lines().map(|l| l.len()).unwrap_or(0);
            if let Some(hit) =
                panel_preview::preview_row_at(preview, self.preview_scroll, row_count, column, row)
            {
                self.preview_selection = Some(PreviewSelection {
                    anchor_row: hit,
                    current_row: hit,
                    dragged: false,
                });
                return;
            }
        }

        let footer = layout::split(self.last_frame_area, self.focus, self.zen_mode).status_bar;
        if status_bar::coffee_hit_at(footer, column, row, self.config.general.show_coffee_link) {
            self.open_coffee_link();
        }
    }

    fn on_mouse_drag(&mut self, column: u16, row: u16) {
        if self.mode == Mode::Edit && self.config.editor.mouse_selection {
            if self.editor_click_count > 0 {
                self.on_editor_mouse_drag(column, row);
            }
            return;
        }
        if self.preview_selection.is_none() {
            return;
        }
        // Any `Drag` event at all — even one that lands back on the anchor
        // row — means this is a real drag gesture, not a plain click; see
        // `PreviewSelection::dragged`'s own doc comment for why that
        // distinction is what `on_mouse_up` branches on.
        self.preview_selection.as_mut().unwrap().dragged = true;
        let preview = layout::split(self.last_frame_area, self.focus, self.zen_mode).preview;
        let row_count = self.note_preview_lines().map(|l| l.len()).unwrap_or(0);
        let scroll = self.preview_scroll;
        // Dragging outside the panel clamps to its nearest edge instead of
        // losing the selection — no auto-scroll in v1 (see wrap.rs's own
        // doc comment for why the panel is pre-wrapped in the first place;
        // an edge-triggered auto-scroll would need a repeat-tick mechanism
        // this app's synchronous poll loop doesn't have anywhere).
        // `.max(left)`/`.max(top)` guard against a terminal resize shrinking
        // the panel mid-drag to where the naive upper bound would fall
        // below the lower one — `u16::clamp` panics if min > max.
        let left = preview.x + 1;
        let right = (preview.x + preview.width).saturating_sub(2).max(left);
        let top = preview.y + 1;
        let bottom = (preview.y + preview.height).saturating_sub(2).max(top);
        let clamped_column = column.clamp(left, right);
        let clamped_row = row.clamp(top, bottom);
        if let Some(hit) =
            panel_preview::preview_row_at(preview, scroll, row_count, clamped_column, clamped_row)
        {
            if let Some(selection) = &mut self.preview_selection {
                selection.current_row = hit;
            }
        }
    }

    fn on_mouse_up(&mut self) {
        self.editor_drag_active = false;
        let Some(selection) = self.preview_selection.take() else {
            return;
        };
        if !selection.dragged {
            // A click on a `<details>` `<summary>` row toggles that block's
            // fold instead of starting to edit there — folding is what a
            // click on a summary means, not "place the cursor in the raw
            // source" (which the summary's one-line handle can't usefully
            // do anyway).
            if self.toggle_details_block(selection.anchor_row) {
                return;
            }
            self.enter_edit_at_preview_row(selection.anchor_row);
            return;
        }
        let Some(lines) = self.note_preview_lines() else {
            return;
        };
        let start = selection.anchor_row.min(selection.current_row);
        let end = selection
            .anchor_row
            .max(selection.current_row)
            .min(lines.len().saturating_sub(1));
        let text = lines[start..=end]
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        let count = end - start + 1;
        crate::clipboard::copy(&text);
        self.set_status(format!(
            "copied {count} line{} to clipboard",
            if count == 1 { "" } else { "s" }
        ));
    }

    /// A plain (non-dragged) left-click on a PREVIEW row — a mouse-only way
    /// into `Mode::Edit` for anyone not reaching for `i`/vim motions to get
    /// there: click the line you're reading, land in the editor with the
    /// cursor already on it. Only the *row* is trustworthy — PREVIEW shows
    /// rendered Markdown (headers/bold/tables reformatted, syntax stripped),
    /// so the clicked screen column doesn't correspond to a raw source
    /// column at all. `note_preview_source_line` only tracks which source
    /// line a rendered (and wrapped) row came from, not which column within
    /// it, so the cursor always lands at column 0 of that line rather than
    /// guessing a column that would frequently be wrong once any Markdown
    /// syntax on the line had been stripped or a table reformatted it.
    fn enter_edit_at_preview_row(&mut self, row: usize) {
        let Some(target_line) = self.note_preview_source_line(row) else {
            return;
        };
        self.start_edit_inline();
        if let Some(editor) = &mut self.editor {
            let max_row = editor.textarea.lines().len().saturating_sub(1);
            let target = target_line.min(max_row) as u16;
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(target, 0));
        }
    }

    /// Alt+Click (`config.editor.multi_cursor`): adds a plain cursor at
    /// the clicked position — dedups against the primary and every
    /// existing secondary (see `multicursor::add_cursor_at`), so clicking
    /// an already-present cursor's exact cell is a harmless no-op rather
    /// than a duplicate.
    fn on_editor_alt_click(&mut self, column: u16, row: u16) {
        let preview = layout::split(self.last_frame_area, self.focus, self.zen_mode).preview;
        let line_numbers = self.config.editor.line_numbers;
        let Some(editor) = &self.editor else {
            return;
        };
        let Some(pos) = editor.position_at(preview, line_numbers, column, row) else {
            return;
        };
        let primary = crate::editor::cursor_tuple(&editor.textarea);
        crate::multicursor::add_cursor_at(&mut self.editor_secondary_cursors, primary, pos);
    }
    /// `config.editor.mouse_selection`'s click handling: single click
    /// positions the cursor, a second click within `MULTI_CLICK_WINDOW` on
    /// the same cell selects the word under it, a third selects the whole
    /// line — same click-counting idea as a real GUI text editor, since
    /// crossterm itself has no concept of a "double click" event. Resets
    /// `editor_drag_active` for the new gesture: `true` when the click
    /// itself already established a selection (word/line), so the first
    /// `Drag` event that follows extends *that* selection instead of
    /// re-anchoring it at the drag's own start (which would discard the
    /// word/line just selected); `false` for a plain single click, which
    /// hasn't anchored anything yet.
    fn on_editor_mouse_down(&mut self, column: u16, row: u16) {
        let preview = layout::split(self.last_frame_area, self.focus, self.zen_mode).preview;
        let line_numbers = self.config.editor.line_numbers;
        let Some(editor) = &mut self.editor else {
            return;
        };
        let Some((doc_row, doc_col)) = editor.position_at(preview, line_numbers, column, row)
        else {
            return;
        };
        let now = std::time::Instant::now();
        const MULTI_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(400);
        let same_cell = self.editor_last_click.is_some_and(|(t, c, r)| {
            c == column && r == row && now.duration_since(t) < MULTI_CLICK_WINDOW
        });
        self.editor_click_count = if same_cell {
            (self.editor_click_count + 1).min(3)
        } else {
            1
        };
        self.editor_last_click = Some((now, column, row));

        match self.editor_click_count {
            2 => {
                let chars: Vec<char> = editor.textarea.lines()[doc_row].chars().collect();
                let (start, end) = crate::editor::word_range(&chars, doc_col);
                editor.textarea.cancel_selection();
                editor
                    .textarea
                    .move_cursor(ratatui_textarea::CursorMove::Jump(
                        doc_row as u16,
                        start as u16,
                    ));
                editor.textarea.start_selection();
                editor
                    .textarea
                    .move_cursor(ratatui_textarea::CursorMove::Jump(
                        doc_row as u16,
                        end as u16,
                    ));
            }
            3 => {
                editor.textarea.cancel_selection();
                editor
                    .textarea
                    .move_cursor(ratatui_textarea::CursorMove::Jump(doc_row as u16, 0));
                editor.textarea.start_selection();
                editor
                    .textarea
                    .move_cursor(ratatui_textarea::CursorMove::Jump(doc_row as u16, u16::MAX));
            }
            _ => {
                editor.textarea.cancel_selection();
                editor
                    .textarea
                    .move_cursor(ratatui_textarea::CursorMove::Jump(
                        doc_row as u16,
                        doc_col as u16,
                    ));
            }
        }
        self.editor_drag_active = self.editor_click_count > 1;
    }
    /// Extends the in-progress editor selection while dragging — clamps to
    /// the editor's own inner area (no auto-scroll in v1, same reasoning
    /// and convention as PREVIEW's own `on_mouse_drag` above). The first
    /// drag event after a plain single click anchors the selection there
    /// (`ratatui_textarea::TextArea::move_cursor` auto-extends from an anchor
    /// set by `start_selection`, so no separate anchor bookkeeping is
    /// needed in shiki itself); a drag following a double/triple click
    /// skips re-anchoring since one already exists (see `editor_drag_active`'s
    /// own doc comment).
    fn on_editor_mouse_drag(&mut self, column: u16, row: u16) {
        let preview = layout::split(self.last_frame_area, self.focus, self.zen_mode).preview;
        let line_numbers = self.config.editor.line_numbers;
        let Some(editor) = &mut self.editor else {
            return;
        };
        let inner = editor.inner_area(preview);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let left = inner.x;
        let right = (inner.x + inner.width).saturating_sub(1).max(left);
        let top = inner.y;
        let bottom = (inner.y + inner.height).saturating_sub(1).max(top);
        let clamped_column = column.clamp(left, right);
        let clamped_row = row.clamp(top, bottom);
        let Some((doc_row, doc_col)) =
            editor.position_at(preview, line_numbers, clamped_column, clamped_row)
        else {
            return;
        };
        if !self.editor_drag_active {
            editor.textarea.start_selection();
            self.editor_drag_active = true;
        }
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(
                doc_row as u16,
                doc_col as u16,
            ));
    }
    /// Guards `on_mouse_down`'s preview-selection branch — which now covers
    /// two gestures a released `PreviewSelection` can resolve to (see its
    /// own doc comment): drag-to-select-and-copy, and a plain click into
    /// `Mode::Edit`. One config flag (`mouse_drag_selection`) still governs
    /// both, since they're really "mouse text interaction in PREVIEW" as a
    /// single feature, not two independently toggleable ones — turning it
    /// off means a click in PREVIEW does nothing at all, same as before
    /// click-to-edit existed. Also off while we're already editing the note
    /// inline (the preview `Rect` belongs to the editor then, not
    /// `panel_preview::render`), or some modal is currently covering
    /// PREVIEW. `show_global_search`/`show_drawer` are deliberately not
    /// repeated here — `on_mouse_down` already returns early for both
    /// before this guard is ever reached. Mirrors the rest of the overlay
    /// flags `draw.rs` layers on top of the 3-pane layout — a click
    /// reaching any of these shouldn't be reinterpreted as a PREVIEW
    /// gesture.
    fn can_start_preview_selection(&self) -> bool {
        self.config.general.mouse_drag_selection && self.mode != Mode::Edit && self.no_modal_open()
    }
    /// Whether any overlay is currently covering the 3-pane layout — every
    /// popup/modal flag this app has, in one place, so a click or scroll
    /// reaching the layout underneath one of them can't be misinterpreted
    /// as belonging to NOTEBOOKS/NOTES/PREVIEW. Shared by
    /// `can_start_preview_selection` and mouse-wheel scrolling.
    fn no_modal_open(&self) -> bool {
        !self.show_slash_menu
            && !self.show_wikilink_menu
            && self.pending_input.is_none()
            && !self.show_tags
            && !self.show_theme_picker
            && !self.show_template_picker
            && !self.show_global_search
            && !self.show_logs
            && !self.show_tree
            && !self.show_links
            && !self.show_tasks
            && !self.show_history
            && !self.show_update
            && !self.show_settings
            && !self.show_which_key
            && !self.show_metadata
            && self.confirm.is_none()
    }
    /// Best-effort: a browser failing to launch (no GUI, headless SSH
    /// session, etc.) shouldn't do anything worse than a status message —
    /// same "fire and forget, report the failure" spirit as external-editor
    /// spawns elsewhere in this file.
    fn open_coffee_link(&mut self) {
        match shiki_core::browser::open_url(status_bar::COFFEE_URL) {
            Ok(_) => self.set_status(format!("opening {}…", status_bar::COFFEE_URL)),
            Err(err) => self.set_status(format!("couldn't open browser: {err}")),
        }
    }
    /// Notebook delete gets a real three-way choice instead of the shared
    /// y/n confirm every other delete target uses — a notebook's directory
    /// can hold files that exist nowhere else (unlike a note/folder delete,
    /// which always has `trash_path`'s safety net) and, for an adopted
    /// notebook, might not even live under shiki's own data dir at all (an
    /// Obsidian vault, an existing repo someone pointed shiki at). Answered
    /// in `handle_delete_notebook_confirm_key`.
    fn start_delete_notebook(&mut self) {
        if let Some(nb) = self.selected_notebook() {
            let message = format!("Delete notebook '{}'?", nb.name);
            self.pending_delete = Some((DeleteTarget::Notebook, nb.path.clone()));
            self.confirm = Some(confirm::ConfirmDialog::with_hint(
                message,
                "[d] delete files  [r] keep files, just untrack  [Esc] cancel",
            ));
        }
    }
    /// The three-way answer to `start_delete_notebook`'s prompt. `d`
    /// actually removes the directory (the old, only behavior); `r` leaves
    /// the directory completely untouched on disk and just sets
    /// `NotebookGitOverride::hidden` so `App::reload_notebooks` stops
    /// listing it — anything else (`Esc`, `n`, ...) cancels.
    fn handle_delete_notebook_confirm_key(&mut self, key: KeyEvent) {
        self.confirm = None;
        let Some((DeleteTarget::Notebook, path)) = self.pending_delete.take() else {
            return;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        match key.code {
            KeyCode::Char('d') | KeyCode::Char('D') => {
                let _ = self.store.delete(&name);
                self.reload_notebooks();
                self.set_status(format!("notebook '{name}' deleted"));
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.config
                    .notebooks
                    .entry(name.clone())
                    .or_default()
                    .hidden = true;
                self.save_config();
                self.reload_notebooks();
                self.set_status(format!(
                    "notebook '{name}' untracked — files left in place at '{}'",
                    path.display()
                ));
            }
            _ => {
                self.set_status("notebook delete cancelled".into());
            }
        }
    }
    /// Handles either a note or a folder selection — folders never had a
    /// delete path at all before (`Notebook::delete_folder_at` didn't
    /// exist), so selecting one and pressing `d` used to silently no-op —
    /// and, in `Mode::Visual`, the whole selected range at once instead of
    /// just the one item under the cursor.
    fn start_delete_note(&mut self) {
        let skip_confirm = self.config.general.skip_delete_confirm;
        if self.mode == Mode::Visual {
            let entries = self.visual_selected_entries();
            if entries.is_empty() {
                self.set_status("nothing selected".into());
                return;
            }
            if skip_confirm {
                self.apply_batch_delete(entries);
                return;
            }
            let (notes, folders) = entries.iter().fold((0, 0), |(n, f), e| match e {
                SelectedEntry::Note(_) => (n + 1, f),
                SelectedEntry::Folder(_) => (n, f + 1),
            });
            let message = format!(
                "Delete {notes} note(s) and {folders} folder(s) (and everything inside them)? Restorable with leader+u."
            );
            self.pending_batch_delete = Some(entries);
            self.confirm = Some(confirm::ConfirmDialog::new(message));
            return;
        }
        if let Some(note) = self.selected_note() {
            let message = format!(
                "Delete note '{}'? Restorable with leader+u.",
                note.file_stem()
            );
            self.pending_delete = Some((DeleteTarget::Note, note.path.clone()));
            if skip_confirm {
                self.handle_confirm_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
            } else {
                self.confirm = Some(confirm::ConfirmDialog::new(message));
            }
        } else if let (Some(folder), Some(nb)) = (self.selected_folder(), self.selected_notebook())
        {
            let path = nb.path.join(self.notes_relative_path()).join(folder);
            let message = format!(
                "Delete folder '{folder}' and everything inside it? Restorable with leader+u."
            );
            self.pending_delete = Some((DeleteTarget::Folder, path));
            if skip_confirm {
                self.handle_confirm_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
            } else {
                self.confirm = Some(confirm::ConfirmDialog::new(message));
            }
        }
    }
    fn start_rename_notebook(&mut self) {
        if let Some(name) = self.selected_notebook().map(|nb| nb.name.clone()) {
            self.start_input(PendingInput::RenameNotebook, name);
        }
    }
    fn start_rename_note(&mut self) {
        if let Some(title) = self.selected_note().map(|n| n.frontmatter.title.clone()) {
            self.start_input(PendingInput::RenameNote, title);
        }
    }
    /// Starts a move or copy — in `Mode::Visual`, for every item in the
    /// selected range; otherwise for whichever single note/folder is
    /// currently selected. Both branches populate the same `pending_batch`
    /// shape (a `Vec` either way, just one entry long in the single-item
    /// case), so `apply_pending_batch` has exactly one code path regardless
    /// of how many things are being acted on.
    fn start_move_or_copy(&mut self, op: BatchOp) {
        if self.selected_notebook().is_none() {
            self.set_status("no notebook selected".into());
            return;
        }
        let verb = if op == BatchOp::Copy { "Copy" } else { "Move" };
        let (entries, label) = if self.mode == Mode::Visual {
            let entries = self.visual_selected_entries();
            if entries.is_empty() {
                self.set_status("nothing selected".into());
                return;
            }
            let label = format!("{} items", entries.len());
            (entries, label)
        } else if let Some(note) = self.selected_note() {
            (
                vec![SelectedEntry::Note(note.path.clone())],
                format!("'{}'", note.frontmatter.title),
            )
        } else if let Some(folder) = self.selected_folder() {
            let Some(nb) = self.selected_notebook() else {
                self.set_status("no notebook selected".into());
                return;
            };
            let path = nb.path.join(self.notes_relative_path()).join(folder);
            (vec![SelectedEntry::Folder(path)], format!("'{folder}'"))
        } else {
            self.set_status("nothing selected".into());
            return;
        };
        self.pending_input_title = Some(format!(" {verb} {label} to "));
        self.pending_batch = Some((op, entries));
        let prefill = self.current_address();
        self.start_input(PendingInput::MoveOrCopy, prefill);
    }
    /// Applies whichever `pending_batch` is waiting (move or copy, one item
    /// or many) to the parsed target — the single code path for both the
    /// single-selection case (`start_move_or_copy`) and `Mode::Visual`'s
    /// batch case, since both populate the exact same shape.
    fn apply_pending_batch(&mut self, target: &str) {
        let Some((op, entries)) = self.pending_batch.take() else {
            return;
        };
        let Some(source_nb) = self.selected_notebook().cloned() else {
            return;
        };
        let (dest_notebook, dest_relative) = match self.parse_move_target(target) {
            Ok(v) => v,
            Err(e) => {
                self.set_status(e);
                return;
            }
        };
        let verb = if op == BatchOp::Copy {
            "copied"
        } else {
            "moved"
        };
        let (mut ok, mut failed) = (0u32, 0u32);
        let mut first_err = None;
        for entry in &entries {
            let result = match (entry, op) {
                (SelectedEntry::Note(path), BatchOp::Move) => source_nb
                    .move_note_to(path, &dest_notebook, &dest_relative)
                    .map(|_| ()),
                (SelectedEntry::Note(path), BatchOp::Copy) => source_nb
                    .copy_note_to(path, &dest_notebook, &dest_relative)
                    .map(|_| ()),
                (SelectedEntry::Folder(path), BatchOp::Move) => path
                    .strip_prefix(&source_nb.path)
                    .map_err(|_| shiki_core::Error::NoteNotFound(path.display().to_string()))
                    .and_then(|relative| {
                        source_nb.move_folder_to(relative, &dest_notebook, &dest_relative)
                    }),
                (SelectedEntry::Folder(path), BatchOp::Copy) => path
                    .strip_prefix(&source_nb.path)
                    .map_err(|_| shiki_core::Error::NoteNotFound(path.display().to_string()))
                    .and_then(|relative| {
                        source_nb.copy_folder_to(relative, &dest_notebook, &dest_relative)
                    }),
            };
            match result {
                Ok(()) => ok += 1,
                Err(e) => {
                    failed += 1;
                    first_err.get_or_insert(e);
                }
            }
        }
        self.reload_notes();
        self.note_changed(&source_nb.name);
        self.note_changed(&dest_notebook.name);
        let count = entries.len();
        if failed == 0 {
            self.set_status(format!(
                "{verb} {count} item{} to '{}'",
                if count == 1 { "" } else { "s" },
                target
            ));
        } else {
            let suffix = first_err.map_or(String::new(), |e| format!(" ({e})"));
            self.set_status(format!(
                "{verb} {ok}/{count} to '{target}', {failed} failed{suffix}"
            ));
        }
    }
    fn start_set_remote(&mut self) {
        if self.selected_notebook().is_none() {
            self.set_status("no notebook selected".into());
            return;
        }
        let prefill = self
            .selected_notebook()
            .and_then(|nb| shiki_core::git::remote_url(&nb.path))
            .unwrap_or_default();
        self.start_input(PendingInput::SetRemote, prefill);
    }
    /// leader+`x` — exports the selected notebook to a single HTML or
    /// Markdown bundle via `shiki_core::export` (the same rendering
    /// `shiki export` uses). Prefilled with an `.html` path under
    /// `{data_dir}/exports/`, same location `publish_notebook` writes PDFs
    /// to, so exported files don't land inside the git-tracked notebook
    /// directory either.
    fn start_export_notebook(&mut self) {
        let Some(nb) = self.selected_notebook() else {
            self.set_status("no notebook selected".into());
            return;
        };
        let prefill = self
            .store
            .root
            .join("exports")
            .join(format!("{}.html", nb.name))
            .to_string_lossy()
            .into_owned();
        self.start_input(PendingInput::ExportNotebook, prefill);
    }
    /// Opens when `[export].ask_export_path` is on — prefilled with the same
    /// path `publish_notebook` would otherwise silently use
    /// (`App::resolved_export_dir`), so accepting it as-is behaves exactly
    /// like the setting being off.
    pub(crate) fn start_publish_path_prompt(&mut self) {
        let Some(nb) = self.selected_notebook() else {
            self.set_status("no notebook selected".into());
            return;
        };
        let prefill = self
            .resolved_export_dir()
            .join(format!("{}.pdf", nb.name))
            .to_string_lossy()
            .into_owned();
        self.start_input(PendingInput::PublishPath, prefill);
    }
    fn create_daily_note(&mut self) {
        let Some(nb) = self.selected_notebook().cloned() else {
            self.set_status("no notebook selected".into());
            return;
        };
        let templates_dir = match Config::default_templates_dir() {
            Ok(dir) => dir,
            Err(e) => {
                self.set_status(format!("daily note error: {e}"));
                return;
            }
        };
        let today = chrono::Local::now().date_naive();
        // Today's due/overdue tasks across *every* notebook, injected only
        // if the daily is actually being created (an existing one opens
        // untouched — create_or_open ignores the agenda then).
        let agenda = self
            .config
            .general
            .daily_agenda
            .then(|| {
                self.store
                    .all_notes()
                    .ok()
                    .and_then(|pool| shiki_core::tasks::agenda_section(&pool, today))
            })
            .flatten();
        match shiki_core::daily::create_or_open(
            &nb,
            today,
            &templates_dir,
            &self.config.general.daily_template,
            agenda.as_deref(),
        ) {
            Ok(note) => {
                // Daily notes always live at the notebook root — jump the
                // breadcrumb back there so the new note is visible.
                self.notes_path.clear();
                self.reload_notes();
                if let Some(idx) = self.notes.iter().position(|n| n.path == note.path) {
                    self.selected_note = self.folders.len() + idx;
                }
                self.focus = Focus::Notes;
                self.note_changed(&nb.name);
                self.set_status(format!("daily note: {}", note.frontmatter.title));
            }
            Err(e) => self.set_status(format!("daily note error: {e}")),
        }
    }
    /// Every `.md` file currently in the templates dir, sorted — the raw
    /// listing shared by `open_template_picker` (wrapped in `Option` there,
    /// with a leading "blank") and `quick_command_options` (wrapped in
    /// `QuickCommand::Template` there, with no "blank" — `@` typing an empty
    /// query already shows every option, a blank-note entry has nothing
    /// distinctive to filter towards).
    fn template_names(&self) -> Vec<String> {
        let Ok(dir) = Config::default_templates_dir() else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) != Some("md") {
                    return None;
                }
                path.file_stem().map(|s| s.to_string_lossy().to_string())
            })
            .collect();
        names.sort();
        names
    }
    /// Opens the template picker for a note titled `title` — every `.md`
    /// file in the templates dir, plus a leading "blank" option, listed
    /// fresh each time (mirrors the tags/tree modals' "rebuild on open"
    /// approach) so a template added or removed between two `a` presses is
    /// always reflected without needing its own invalidation logic.
    fn open_template_picker(&mut self, title: String) {
        self.pending_new_note_title = title;
        let mut options = vec![None];
        options.extend(self.template_names().into_iter().map(Some));
        self.template_picker_options = options;
        self.template_picker_index = 0;
        self.show_template_picker = true;
    }
    /// Full, unfiltered `@`-dropdown option list: the three relative-date
    /// commands first (always available, don't depend on disk state), then
    /// every real template — rebuilt on every keystroke the same way
    /// `open_template_picker` rebuilds its own list on every open, so a
    /// template added/removed mid-session is picked up immediately.
    fn quick_command_options(&self) -> Vec<QuickCommand> {
        let mut options = vec![
            QuickCommand::Today,
            QuickCommand::Yesterday,
            QuickCommand::Tomorrow,
        ];
        options.extend(
            self.template_names()
                .into_iter()
                .map(QuickCommand::Template),
        );
        options
    }
    /// The text after the *last* `@` in the `NewNote` input, if any — e.g.
    /// `"errand @da"` yields `Some("da")`. `None` (not just an empty
    /// string) means "no `@` typed at all yet", which is what gates the
    /// dropdown's visibility — an empty-but-present query (`"@"` alone)
    /// still yields `Some("")`, matching every option.
    pub(crate) fn quick_template_query(&self) -> Option<&str> {
        if self.pending_input != Some(PendingInput::NewNote) {
            return None;
        }
        self.input.value.rsplit_once('@').map(|(_, after)| after)
    }
    /// `quick_command_options()` narrowed to whatever's typed after the
    /// `@` — same case-insensitive substring match `which_key_filtered_entries`
    /// already uses for its own typed filter.
    pub(crate) fn quick_template_filtered(&self) -> Vec<QuickCommand> {
        let Some(query) = self.quick_template_query() else {
            return Vec::new();
        };
        let query = query.to_lowercase();
        self.quick_command_options()
            .into_iter()
            .filter(|cmd| cmd.label().to_lowercase().contains(&query))
            .collect()
    }
    /// Runs the chosen `@`-dropdown entry, finishing note creation right
    /// away instead of handing off to `show_template_picker` — the title
    /// text before the `@` becomes the note's title (for `Template`
    /// entries; the three date commands ignore it and always use the
    /// computed date instead, per their whole purpose).
    fn apply_quick_template(&mut self, cmd: QuickCommand) {
        let prefix = self
            .input
            .value
            .rsplit_once('@')
            .map(|(before, _)| before.trim().to_string())
            .unwrap_or_default();
        self.input.clear();
        self.quick_template_selected = 0;
        self.pending_input = None;
        self.mode = Mode::Normal;

        let (title, template_choice) = match &cmd {
            QuickCommand::Today | QuickCommand::Yesterday | QuickCommand::Tomorrow => {
                let date = cmd
                    .date()
                    .unwrap_or_else(|| chrono::Local::now().date_naive());
                (date.format("%Y-%m-%d").to_string(), None)
            }
            QuickCommand::Template(name) => {
                let title = if prefix.is_empty() {
                    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
                } else {
                    prefix
                };
                (title, Some(name.clone()))
            }
        };
        self.create_note_with_template(title, template_choice);
    }
    fn handle_template_picker_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.show_template_picker = false;
                self.pending_new_note_title.clear();
                self.pending_new_note_body = None;
                self.set_status("new note cancelled".into());
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.template_picker_index + 1 < self.template_picker_options.len() {
                    self.template_picker_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.template_picker_index = self.template_picker_index.saturating_sub(1);
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                self.confirm_template_choice();
            }
            _ => {}
        }
    }
    /// Reads the picker's current selection and hands off to
    /// `create_note_with_template` — the actual creation `confirm_input`'s
    /// old `NewNote` arm used to do directly, now happening once a
    /// template's actually been picked instead of always being empty.
    fn confirm_template_choice(&mut self) {
        let title = std::mem::take(&mut self.pending_new_note_title);
        let template_choice = self
            .template_picker_options
            .get(self.template_picker_index)
            .cloned()
            .flatten();
        self.show_template_picker = false;
        self.create_note_with_template(title, template_choice);
    }
    /// Creates a note titled `title` in the current folder, with the given
    /// template's rendered body (or an empty one for `None`, "blank") — the
    /// single shared creation path for both the `show_template_picker` flow
    /// (`confirm_template_choice`) and the `@`-dropdown fast path
    /// (`apply_quick_template`), so the two can't drift into creating notes
    /// differently from each other.
    fn create_note_with_template(&mut self, title: String, template_choice: Option<String>) {
        let pending_body = self.pending_new_note_body.take();
        // A scratchpad-originated body always wins over the template — see
        // the caller's own guard, which skips the picker in that case — but
        // the `@`-dropdown fast path can still hand in a `template_choice`
        // alongside a pending scratchpad body, so this stays defensive here
        // too: `frontmatter.template` must only ever record a template that
        // actually rendered the body, not one that was picked but discarded.
        let template_applied = pending_body.is_none() && template_choice.is_some();
        let rendered = match pending_body {
            Some(body) => body,
            None => match &template_choice {
                Some(name) => Config::default_templates_dir()
                    .ok()
                    .and_then(|dir| shiki_core::Template::load(&dir, name).ok())
                    .map(|template| {
                        let now = chrono::Local::now();
                        let mut vars = std::collections::HashMap::new();
                        vars.insert("title", title.clone());
                        vars.insert("date", now.format("%Y-%m-%d").to_string());
                        vars.insert("time", now.format("%H:%M").to_string());
                        vars.insert(
                            "notebook",
                            self.selected_notebook()
                                .map(|nb| nb.name.clone())
                                .unwrap_or_default(),
                        );
                        template.render(&vars)
                    })
                    .unwrap_or_default(),
                None => String::new(),
            },
        };
        // A literal `{{cursor}}` marker (same convention slash-menu snippets
        // use) is never meant to be saved to disk — split it out before the
        // note is written, and remember where the split landed so the
        // cursor can be moved there once the inline editor actually opens.
        let (before, cursor_marker) = match rendered.split_once("{{cursor}}") {
            Some((before, after)) => (before.to_string(), Some(after.to_string())),
            None => (rendered, None),
        };
        let body = match &cursor_marker {
            Some(after) => format!("{before}{after}"),
            None => before.clone(),
        };

        match self.selected_notebook().cloned() {
            Some(nb) => match nb.create_note_in(&self.notes_relative_path(), &title, body) {
                Ok(mut note) => {
                    if template_applied {
                        if let Some(name) = &template_choice {
                            note.frontmatter.template = Some(name.clone());
                            let _ = note.save_with_crypto(nb.crypto.as_ref());
                        }
                    }
                    self.reload_notes();
                    if let Some(idx) = self.notes.iter().position(|n| n.path == note.path) {
                        self.selected_note = self.folders.len() + idx;
                    }
                    // Jump straight to Preview so NOTEBOOKS/NOTES collapse and
                    // the fresh note's own editor is what's full-screen (in the
                    // narrow/short `single` layout tier, the inline editor
                    // always renders into `areas.preview` — leaving focus on
                    // Notes would render it into a zero-sized area there).
                    self.focus = Focus::Preview;
                    self.set_status(format!("created '{title}'"));
                    // Drop straight into the inline editor — a fresh note
                    // (blank or templated) isn't useful to just sit on.
                    self.start_edit_inline();
                    if cursor_marker.is_some() {
                        if let Some(editor) = &mut self.editor {
                            let target_row = before.matches('\n').count();
                            let target_col =
                                before.rsplit('\n').next().unwrap_or("").chars().count();
                            editor
                                .textarea
                                .move_cursor(ratatui_textarea::CursorMove::Jump(
                                    target_row as u16,
                                    target_col as u16,
                                ));
                        }
                    }
                }
                Err(e) => self.set_status(format!("could not create note: {e}")),
            },
            None => self.set_status("create a notebook first".into()),
        }
    }
    /// Shared block/style setup for the inline editor — used both for
    /// editing a note (`start_edit_inline`) and for editing `config.toml`
    /// itself (`start_edit_config_inline`), so the two can't visually drift
    /// apart from each other.
    fn style_inline_editor(&self, editor: &mut InlineEditor, title: String) {
        editor.textarea.set_block(panel_block(
            ratatui::text::Line::from(title),
            true,
            &self.theme,
        ));
        editor.textarea.set_style(
            ratatui::style::Style::default()
                .fg(hex_to_color(&self.theme.fg))
                .bg(hex_to_color(&self.theme.bg)),
        );
        editor.textarea.set_cursor_line_style(
            ratatui::style::Style::default().fg(hex_to_color(&self.theme.fg)),
        );
    }
    /// `Note::from_file` has no notion of git conflict markers — a file
    /// left mid-merge (`<<<<<<< HEAD` etc.) parses as if it simply had no
    /// `---` frontmatter block at all (`synthesize_frontmatter` falls
    /// through and treats the markers as ordinary body text), so editing
    /// and saving it through the normal note-save path would happily
    /// commit those markers as real content. Gate entry into edit mode on
    /// `merge_in_progress` instead — reading/browsing stays allowed (the
    /// selected notebook's `git_status`/`merge_active` is already
    /// refreshed by `refresh_git_status`), only the write path is blocked.
    fn merge_blocks_editing(&mut self) -> bool {
        let Some(nb) = self.selected_notebook() else {
            return false;
        };
        if self.merge_active {
            self.set_status(format!(
                "'{}' has a merge in progress — resolve conflicts first (press p)",
                nb.name
            ));
            true
        } else {
            false
        }
    }
    fn start_edit_inline(&mut self) {
        if let Some(note) = self.selected_note() {
            let mut editor = InlineEditor::from_contents(&note.body);
            let title = format!(" {}Editing: {} ", icons::PENCIL, note.frontmatter.title);
            self.style_inline_editor(&mut editor, title);
            // Only ever rendered while the note is completely empty (see
            // `InlineEditor::render`'s placeholder branch) — the `/`-menu
            // has no other in-app hint pointing at it, so this is its one
            // discoverability surface, gone the instant you type anything.
            editor.textarea.set_placeholder_text(
                "Type '/' for quick blocks (headers, code, tables, tags, frontmatter...)",
            );
            editor.textarea.set_placeholder_style(
                ratatui::style::Style::default().fg(hex_to_color(&self.theme.muted)),
            );
            self.editor = Some(editor);
            self.mode = Mode::Edit;
        }
    }
    /// Opens a session-only editor buffer. Nothing is written when it closes;
    /// Ctrl+S stages the contents for the ordinary new-note flow instead.
    fn start_scratchpad(&mut self) {
        if self.mode == Mode::Edit {
            return;
        }
        let mut editor = InlineEditor::from_contents("");
        self.style_inline_editor(&mut editor, format!(" {}Scratchpad ", icons::PENCIL));
        editor
            .textarea
            .set_placeholder_text("Scratchpad — Ctrl+S saves as a note; Esc discards");
        editor.textarea.set_placeholder_style(
            ratatui::style::Style::default().fg(hex_to_color(&self.theme.muted)),
        );
        self.editor = Some(editor);
        self.editing_scratchpad = true;
        self.mode = Mode::Edit;
    }
    /// Opens `config.toml`'s actual on-disk contents (whatever's really
    /// there — comments included, if the file has any) in the same inline
    /// editor notes use. `editing_config` tells `save_and_exit_edit` which
    /// of the two save paths to take.
    fn start_edit_config_inline(&mut self) {
        let path = match Config::default_path() {
            Ok(p) => p,
            Err(e) => {
                self.set_status(format!("config path error: {e}"));
                return;
            }
        };
        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        let mut editor = InlineEditor::from_contents(&contents);
        let title = format!(" {}Editing: config.toml ", icons::GEAR);
        self.style_inline_editor(&mut editor, title);
        self.editor = Some(editor);
        self.editing_config = true;
        self.mode = Mode::Edit;
    }
    fn save_and_exit_edit(&mut self) {
        let editor = self.editor.take();
        if self.editing_config {
            self.editing_config = false;
            if let Some(editor) = editor {
                self.save_config_from_editor(&editor.contents());
            }
            self.mode = Mode::Normal;
            return;
        }
        if let Some(trigger) = self.editing_snippet.take() {
            if let Some(editor) = editor {
                if let Some(snippet) = self.config.snippets.get_mut(&trigger) {
                    snippet.body = editor.contents();
                }
            }
            self.save_config();
            self.mode = Mode::Normal;
            self.show_settings = true;
            self.set_status(format!("snippet '{trigger}': body saved"));
            return;
        }
        if self.editing_scratchpad {
            self.editing_scratchpad = false;
            self.set_status("scratchpad discarded".into());
            self.mode = Mode::Normal;
            return;
        }
        if let (Some(editor), Some(mut note)) = (editor, self.selected_note().cloned()) {
            note.body = editor.contents();
            // Pin relative due specs (`@due(+3d)`, `@due(mon)`) to real
            // dates now — they're relative to the day they were written,
            // so save time is the one moment they can be resolved
            // unambiguously.
            let today = chrono::Local::now().date_naive();
            if let Some(normalized) = shiki_core::tasks::normalize_due_tags(&note.body, today) {
                note.body = normalized;
            }
            let crypto = self.selected_notebook().and_then(|nb| nb.crypto.clone());
            let _ = note.save_with_crypto(crypto.as_ref());
            self.note_changed(&note.frontmatter.notebook);
        }
        self.mode = Mode::Normal;
        self.refresh_notes_preserve_selection();
    }
    /// Writes the editor's raw text (comments and all) straight to
    /// `config.toml`, verbatim — never round-tripped through
    /// `toml::to_string_pretty`, which would silently strip any comments
    /// the user just wrote, since only *parsing* it (to validate it and
    /// apply it live via `App::apply_config`) is needed, not
    /// re-serializing it. An invalid edit is reported and neither written
    /// to disk nor applied — the previous config keeps running, same
    /// "never apply/save something broken" stance `reload_config_from_disk`
    /// takes for the external-editor path.
    fn save_config_from_editor(&mut self, raw: &str) {
        let new_config = match Config::parse(raw) {
            Ok(c) => c,
            Err(e) => {
                self.set_status(format!("config not saved — invalid TOML: {e}"));
                return;
            }
        };
        let path = match Config::default_path() {
            Ok(p) => p,
            Err(e) => {
                self.set_status(format!("config not saved — {e}"));
                return;
            }
        };
        if let Err(e) = std::fs::write(&path, raw) {
            self.set_status(format!("config not saved — {e}"));
            return;
        }
        self.apply_config(new_config);
        self.set_status("config saved and reloaded".into());
    }
    fn handle_action(&mut self, action: Action) {
        match action {
            Action::ThemePicker => self.open_theme_picker(),
            Action::GlobalSearch => self.open_global_search(),
            Action::ToggleTags => self.toggle_tags(),
            Action::ShowLogs => self.open_logs(),
            Action::CheckForUpdate => self.open_update_check(),
            Action::ToggleDrawer => self.toggle_drawer(),
            Action::UndoDelete => self.undo_delete(),
            Action::ToggleSettings => self.toggle_settings(),
            Action::Scratchpad => self.start_scratchpad(),
            Action::ToggleTasks => self.open_tasks(),
            Action::ToggleQuery => self.open_query(),
            Action::EditMetadata => self.open_metadata(),
            Action::PublishNotebook => self.publish_notebook(),
            Action::ExportNotebook => self.start_export_notebook(),
            Action::ToggleZenMode => self.toggle_zen_mode(),

            Action::NewNotebook => self.start_input(PendingInput::NewNotebook, String::new()),
            Action::RenameNotebook => self.start_rename_notebook(),
            Action::DeleteNotebook => self.start_delete_notebook(),
            Action::SyncNotebook => self.sync_notebook(),
            Action::PushNotebook => self.push_notebook(),
            Action::PullNotebook => self.pull_notebook(),
            Action::PullAllNotebooks => self.pull_all_notebooks(),
            Action::SetRemote => self.start_set_remote(),

            Action::NewNote => {
                self.pending_new_note_body = None;
                self.start_input(PendingInput::NewNote, String::new());
            }
            Action::NewFolder => self.start_input(PendingInput::NewFolder, String::new()),
            Action::RenameNote => self.start_rename_note(),
            Action::DeleteNote => self.start_delete_note(),
            Action::JumpSearch => self.start_input(PendingInput::Search, String::new()),
            Action::DailyNote => self.create_daily_note(),
            Action::MoveNote => self.start_move_or_copy(BatchOp::Move),
            Action::SortNotes => self.cycle_sort(),
            Action::ToggleTreeView => self.open_tree(),
            Action::ToggleDates => {
                self.show_dates = !self.show_dates;
                self.config.general.show_dates = self.show_dates;
                self.save_config();
                self.set_status(format!(
                    "note dates: {}",
                    if self.show_dates { "on" } else { "off" }
                ));
            }
            Action::ShowHistory => self.open_history(),
            Action::ShowLinks => self.open_links(),
            Action::ShowOutline => self.open_outline(),
            Action::ToggleFavoriteEditor => self.toggle_favorite_editor(),
            Action::ToggleVisual => self.toggle_visual(),
            Action::CopyEntries => {
                if self.mode == Mode::Visual {
                    self.start_move_or_copy(BatchOp::Copy);
                } else {
                    self.set_status("select items first with v".into());
                }
            }

            Action::EditInline => {
                if self.merge_blocks_editing() {
                    return;
                }
                if self.config.general.use_favorite_editor {
                    if let Some(note) = self.selected_note() {
                        let editor = self
                            .favorite_editor
                            .clone()
                            .unwrap_or_else(|| self.config.general.editor.clone());
                        self.want_external_edit = Some((note.path.clone(), editor));
                    }
                } else if self.focus == Focus::Preview
                    && self
                        .note_preview_source_line(self.preview_scroll as usize)
                        .is_some()
                {
                    self.enter_edit_at_preview_row(self.preview_scroll as usize);
                } else {
                    self.start_edit_inline();
                }
            }
            Action::EditExternal => {
                if self.merge_blocks_editing() {
                    return;
                }
                if let Some(note) = self.selected_note() {
                    self.want_external_edit =
                        Some((note.path.clone(), self.config.general.editor.clone()));
                }
            }
        }
    }
    fn confirm_input(&mut self) {
        let value = self.input.value.trim().to_string();
        let kind = self.pending_input.take();
        match kind {
            Some(PendingInput::NewNote) => {
                // Enter on an empty title doesn't cancel — it's the fast path:
                // stamp today's date as the title and go straight to writing.
                let title = if value.is_empty() {
                    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
                } else {
                    value
                };
                // A scratchpad save already has its body — merging that
                // freeform text with a template has no clear insertion
                // point, so skip the picker entirely and create it blank
                // rather than letting a chosen template silently not apply
                // (see `create_note_with_template`'s own guard for the case
                // where a template is still picked via the `@`-dropdown).
                self.mode = Mode::Normal;
                if self.pending_new_note_body.is_some() {
                    self.create_note_with_template(title, None);
                    return;
                }
                // The note itself isn't created yet — `open_template_picker`
                // takes over from here and creates it once a template (or
                // "blank") is actually chosen.
                self.open_template_picker(title);
                return;
            }
            Some(PendingInput::NewFolder) => {
                // Unlike NewNote, an empty name has no sensible default (a
                // timestamp makes a fine note title but a confusing folder
                // name) — cancel instead of creating one.
                if value.is_empty() {
                    self.set_status("new folder cancelled (name can't be empty)".into());
                } else {
                    match self.selected_notebook().cloned() {
                        Some(nb) => {
                            match nb.create_folder_in(&self.notes_relative_path(), &value) {
                                Ok(_) => {
                                    self.reload_notes();
                                    if let Some(idx) = self.folders.iter().position(|f| f == &value)
                                    {
                                        self.selected_note = idx;
                                    }
                                    self.set_status(format!("created folder '{value}'"));
                                }
                                Err(e) => self.set_status(format!("could not create folder: {e}")),
                            }
                        }
                        None => self.set_status("create a notebook first".into()),
                    }
                }
            }
            Some(PendingInput::NewNotebook) => {
                // Pasting a URL is the "import someone else's repo" fast
                // path: derive the name from the repo, create, set the
                // remote, and pull, instead of new notebook + name + `R` +
                // URL + `p` as four separate steps.
                if !value.is_empty() && looks_like_git_url(&value) {
                    self.create_notebook_from_url(&value);
                } else if !value.is_empty() && looks_like_path(&value) {
                    // Pointing at `/abs/path`, `~/docs`, or `./relative`
                    // adopts that existing directory as a notebook instead
                    // of creating a new empty one — see
                    // `adopt_notebook_from_path`.
                    self.adopt_notebook_from_path(&value);
                } else {
                    // Same fast path as NewNote: Enter on an empty name
                    // doesn't cancel, it just picks a default so something
                    // visibly appears instead of the modal silently closing.
                    let name = if value.is_empty() {
                        self.unique_default_notebook_name()
                    } else {
                        value
                    };
                    match self.store.create(&name) {
                        Ok(nb) => {
                            self.reload_notebooks();
                            if let Some(idx) = self.notebooks.iter().position(|n| n.name == name) {
                                self.selected_notebook = idx;
                                self.reload_notes();
                            }
                            let mut status = format!("notebook '{name}' created");
                            // Auto-configure a remote from `git.remote_template`
                            // (e.g. "git@git.example.com:notes/{notebook}.git")
                            // — the remote still has to already exist on that
                            // server; this doesn't create one via any hosting
                            // API. Not a push yet: a fresh notebook has no
                            // commits, so there's nothing to push until the
                            // first note is created/synced — the existing
                            // auto_push/auto_sync machinery picks it up from
                            // here naturally.
                            if !self.config.git.remote_template.is_empty() {
                                let url =
                                    self.config.git.remote_template.replace("{notebook}", &name);
                                match shiki_core::git::set_remote(&nb.path, &url) {
                                    Ok(()) => {
                                        let redacted = shiki_core::git::redact_credentials(&url);
                                        status = format!("{status}, remote set to '{redacted}'");
                                    }
                                    Err(e) => {
                                        status = format!("{status}, but could not set remote: {e}")
                                    }
                                }
                            }
                            self.set_status(status);
                        }
                        Err(e) => self.set_status(format!("could not create: {e}")),
                    }
                }
            }
            Some(PendingInput::RenameNote) => {
                if value.is_empty() {
                    self.set_status("rename cancelled (title can't be empty)".into());
                } else if let (Some(nb), Some(path)) = (
                    self.selected_notebook().cloned(),
                    self.selected_note().map(|n| n.path.clone()),
                ) {
                    match nb.rename_note_at(&path, &value) {
                        Ok(_) => {
                            self.refresh_notes_preserve_selection();
                            self.note_changed(&nb.name);
                            self.set_status(format!("renamed to '{value}'"));
                        }
                        Err(e) => self.set_status(format!("could not rename: {e}")),
                    }
                }
            }
            Some(PendingInput::RenameNotebook) => {
                if value.is_empty() {
                    self.set_status("rename cancelled (name can't be empty)".into());
                } else if let Some(old_name) = self.selected_notebook().map(|nb| nb.name.clone()) {
                    match self.store.rename(&old_name, &value) {
                        Ok(_) => {
                            self.reload_notebooks();
                            self.set_status(format!("renamed to '{value}'"));
                        }
                        Err(e) => self.set_status(format!("could not rename: {e}")),
                    }
                }
            }
            Some(PendingInput::Search) => {
                if value.is_empty() {
                    self.set_status("jump cancelled".into());
                } else {
                    // Searches the whole notebook (any folder depth), not
                    // just the folder currently open, then hops there.
                    let pool = self
                        .selected_notebook()
                        .and_then(|nb| nb.all_notes_recursive().ok())
                        .unwrap_or_default();
                    let hits = self.search_engine.search(&value, &pool);
                    if let Some(hit) = hits.first().map(|h| pool[h.index].clone()) {
                        if let Some(nb) = self.selected_notebook().cloned() {
                            self.notes_path = relative_folder(&hit.path, &nb.path);
                        }
                        self.reload_notes();
                        if let Some(idx) = self.notes.iter().position(|n| n.path == hit.path) {
                            self.selected_note = self.folders.len() + idx;
                        }
                        self.preview_scroll = 0;
                        self.focus = Focus::Notes;
                        self.set_status(format!("jumped to '{value}'"));
                    } else {
                        self.set_status(format!("no match for '{value}'"));
                    }
                }
            }
            Some(PendingInput::SetRemote) => {
                if value.is_empty() {
                    self.set_status("remote cancelled (empty)".into());
                } else if let Some(nb) = self.selected_notebook().cloned() {
                    match shiki_core::git::set_remote(&nb.path, &value) {
                        Ok(()) => {
                            let redacted = shiki_core::git::redact_credentials(&value);
                            self.set_status(format!("remote set to '{redacted}'"));
                        }
                        Err(e) => self.set_status(format!("could not set remote: {e}")),
                    }
                }
            }
            Some(PendingInput::ExportNotebook) => {
                if value.is_empty() {
                    self.set_status("export cancelled (empty path)".into());
                } else if let Some(nb) = self.selected_notebook().cloned() {
                    let format = if value.ends_with(".md") || value.ends_with(".markdown") {
                        shiki_core::export::Format::Md
                    } else {
                        shiki_core::export::Format::Html
                    };
                    match nb.all_notes_recursive() {
                        Ok(mut notes) => {
                            notes.sort_by(|a, b| {
                                a.frontmatter
                                    .date
                                    .cmp(&b.frontmatter.date)
                                    .then_with(|| a.frontmatter.title.cmp(&b.frontmatter.title))
                            });
                            let content = shiki_core::export::render(&nb.name, &notes, format);
                            let path = std::path::Path::new(&value);
                            let write_result = path
                                .parent()
                                .map(std::fs::create_dir_all)
                                .transpose()
                                .and_then(|_| std::fs::write(path, content));
                            match write_result {
                                Ok(()) => self.set_status(format!(
                                    "exported {} notes to {value}",
                                    notes.len()
                                )),
                                Err(e) => self.set_status(format!("export error: {e}")),
                            }
                        }
                        Err(e) => self.set_status(format!("export error: {e}")),
                    }
                }
            }
            Some(PendingInput::PublishPath) => {
                if value.is_empty() {
                    self.set_status("publish cancelled (empty path)".into());
                } else if let Some(nb) = self.selected_notebook().cloned() {
                    self.publish_notebook_to(nb, std::path::PathBuf::from(&value));
                }
            }
            Some(PendingInput::SettingsNotebookRemote) => {
                self.show_settings = true;
                if let Some(name) = self.settings_notebook_drill.clone() {
                    if value.is_empty() {
                        self.set_status("remote unchanged (empty)".into());
                    } else if let Some(nb) = self.notebooks.iter().find(|n| n.name == name).cloned()
                    {
                        match shiki_core::git::set_remote(&nb.path, &value) {
                            Ok(()) => {
                                let redacted = shiki_core::git::redact_credentials(&value);
                                self.set_status(format!(
                                    "notebook '{name}': remote set to '{redacted}'"
                                ));
                            }
                            Err(e) => self.set_status(format!("could not set remote: {e}")),
                        }
                    }
                }
            }
            Some(PendingInput::SettingsNotebookAutoSyncEvery) => {
                self.show_settings = true;
                if let Some(name) = self.settings_notebook_drill.clone() {
                    if value.is_empty() {
                        if let Some(over) = self.config.notebooks.get_mut(&name) {
                            over.auto_sync_every = None;
                        }
                        self.prune_empty_notebook_override(&name);
                        self.save_config();
                        self.set_status(format!("notebook '{name}': auto_sync_every -> inherit"));
                    } else {
                        match value.parse::<u32>() {
                            Ok(n) => {
                                self.config
                                    .notebooks
                                    .entry(name.clone())
                                    .or_default()
                                    .auto_sync_every = Some(n);
                                self.save_config();
                                self.set_status(format!(
                                    "notebook '{name}': auto_sync_every -> {n}"
                                ));
                            }
                            Err(_) => self.set_status(format!("'{value}' isn't a whole number")),
                        }
                    }
                }
            }
            Some(PendingInput::SettingsGeneralText) => {
                use crate::panel_settings::GeneralField;
                self.show_settings = true;
                let field = GeneralField::ALL[self.settings_selected];
                // default_note_sort is free text ("filename"/"title"/"date",
                // tolerantly parsed — see NoteSort::from_config_str), so an
                // empty value here isn't "cancelled" the way it is for
                // every other field; it just resolves to the fallback.
                //
                // A labeled block (not an early `return`) so a parse error
                // still reaches this arm's shared tail below, which is
                // itself just the very end of this match arm — but more
                // importantly so it can't ever skip the *function's* own
                // tail after the outer `match kind` closes
                // (`pending_input_title = None; mode = Mode::Normal;`),
                // which an early `return` from inside a nested macro would.
                let label: Option<&'static str> = 'field: {
                    if value.is_empty() && field != GeneralField::DefaultNoteSort {
                        self.set_status("unchanged (empty)".into());
                        break 'field None;
                    }
                    macro_rules! parse_or_report {
                        ($ty:ty) => {
                            match value.parse::<$ty>() {
                                Ok(n) => n,
                                Err(_) => {
                                    self.set_status(format!("'{value}' isn't a whole number"));
                                    break 'field None;
                                }
                            }
                        };
                    }
                    Some(match field {
                        GeneralField::DefaultNotebook => {
                            self.config.general.default_notebook = value.clone();
                            "default_notebook"
                        }
                        GeneralField::Editor => {
                            self.config.general.editor = value.clone();
                            "editor"
                        }
                        GeneralField::DailyTemplate => {
                            self.config.general.daily_template = value.clone();
                            "daily_template"
                        }
                        GeneralField::StatusMessageTimeoutSecs => {
                            self.config.general.status_message_timeout_secs = parse_or_report!(u64);
                            "status_message_timeout_secs"
                        }
                        GeneralField::DrawerWidth => {
                            self.config.general.drawer_width = parse_or_report!(u16);
                            "drawer_width"
                        }
                        GeneralField::DefaultNoteSort => {
                            self.config.general.default_note_sort = value.clone();
                            "default_note_sort"
                        }
                        GeneralField::LogHistoryLimit => {
                            self.config.general.log_history_limit = parse_or_report!(usize);
                            "log_history_limit"
                        }
                        GeneralField::TrashRetentionDays => {
                            self.config.general.trash_retention_days = parse_or_report!(u32);
                            "trash_retention_days"
                        }
                        GeneralField::ReadingWpm => {
                            self.config.general.reading_wpm = parse_or_report!(usize);
                            "reading_wpm"
                        }
                        GeneralField::PageStep => {
                            self.config.general.page_step = parse_or_report!(usize);
                            "page_step"
                        }
                        GeneralField::UseFavoriteEditor => "use_favorite_editor",
                        GeneralField::EnableCaptureDaemon => "enable_capture_daemon",
                        GeneralField::MouseDragSelection => "mouse_drag_selection",
                        GeneralField::ShowHints => "show_hints",
                        GeneralField::RememberLastSession => "remember_last_session",
                        GeneralField::ShowCoffeeLink => "show_coffee_link",
                        GeneralField::SkipDeleteConfirm => "skip_delete_confirm",
                        GeneralField::ShowDates => "show_dates",
                        GeneralField::WikilinkAutocomplete => "wikilink_autocomplete",
                        GeneralField::DailyAgenda => "daily_agenda",
                        GeneralField::CompactFooter => "compact_footer",
                        GeneralField::TasksShowDoneDefault => "tasks_show_done_default",
                    })
                };
                if let Some(label) = label {
                    self.save_config();
                    self.set_status(format!("{label} -> '{value}'"));
                }
            }
            Some(PendingInput::SettingsGitText) => {
                use crate::panel_settings::GitField;
                self.show_settings = true;
                match GitField::ALL[self.settings_selected] {
                    GitField::AutoSyncEvery => match value.parse::<u32>() {
                        Ok(n) => {
                            self.config.git.auto_sync_every = n;
                            self.save_config();
                            self.set_status(format!("auto_sync_every -> {n}"));
                        }
                        Err(_) => self.set_status(format!("'{value}' isn't a whole number")),
                    },
                    // Empty is a meaningful value here ("no template"), so —
                    // unlike every other text field — it's not treated as
                    // "cancelled".
                    GitField::RemoteTemplate => {
                        self.config.git.remote_template = value.clone();
                        self.save_config();
                        self.set_status(format!("remote_template -> '{value}'"));
                    }
                    field @ (GitField::CommitPrefix | GitField::Remote | GitField::Branch) => {
                        if value.is_empty() {
                            self.set_status("unchanged (empty)".into());
                        } else {
                            let label = match field {
                                GitField::CommitPrefix => {
                                    self.config.git.commit_prefix = value.clone();
                                    "commit_prefix"
                                }
                                GitField::Remote => {
                                    self.config.git.remote = value.clone();
                                    "remote"
                                }
                                GitField::Branch => {
                                    self.config.git.branch = value.clone();
                                    "branch"
                                }
                                _ => unreachable!(),
                            };
                            self.save_config();
                            self.set_status(format!("{label} -> '{value}'"));
                        }
                    }
                    GitField::AutoCommit
                    | GitField::AutoPush
                    | GitField::SignCommits
                    | GitField::AutoSync => {}
                }
            }
            Some(PendingInput::SettingsExportText) => {
                // Empty is meaningful here too ("use the default location"),
                // same as GitField::RemoteTemplate above — not "cancelled".
                self.show_settings = true;
                self.config.export.export_dir = value.clone();
                self.save_config();
                self.set_status(format!("export_dir -> '{value}'"));
            }
            Some(PendingInput::SettingsSnippetTrigger) => {
                self.show_settings = true;
                if value.is_empty() {
                    self.set_status("new snippet cancelled (trigger can't be empty)".into());
                } else if self.config.snippets.contains_key(&value) {
                    self.set_status(format!("a snippet with trigger '{value}' already exists"));
                } else {
                    self.config.snippets.insert(
                        value.clone(),
                        shiki_config::config::SnippetConfig {
                            label: None,
                            body: String::new(),
                        },
                    );
                    self.save_config();
                    self.settings_snippet_drill = Some(value.clone());
                    self.settings_field_selected = 0;
                    self.set_status(format!(
                        "snippet '{value}' created — enter on label/body to edit"
                    ));
                }
            }
            Some(PendingInput::SettingsSnippetLabel) => {
                self.show_settings = true;
                if let Some(trigger) = self.settings_snippet_drill.clone() {
                    if let Some(snippet) = self.config.snippets.get_mut(&trigger) {
                        snippet.label = if value.is_empty() {
                            None
                        } else {
                            Some(value.clone())
                        };
                    }
                    self.save_config();
                    self.set_status(format!("snippet '{trigger}': label -> '{value}'"));
                }
            }
            Some(PendingInput::MoveOrCopy) => {
                if value.is_empty() {
                    self.set_status("move/copy cancelled (empty)".into());
                    self.pending_batch = None;
                } else {
                    self.apply_pending_batch(&value);
                }
            }
            Some(PendingInput::NotebookPassphrase) => self.confirm_notebook_passphrase(),
            Some(PendingInput::Metadata) => self.confirm_metadata_prompt(value),
            Some(PendingInput::RenameTag) => self.confirm_rename_tag(value),
            Some(PendingInput::SaveQuery) => self.confirm_save_query(value),
            None => {}
        }
        // A guard, not a no-op: `confirm_notebook_passphrase`'s `Enable`
        // purpose immediately chains into a second masked prompt
        // (`EnableConfirm`) by calling `start_masked_input` from *within*
        // this same match — unconditionally resetting to `Mode::Normal`
        // afterward would silently break that chained prompt's input
        // routing (keys would stop reaching `handle_insert_key`) the
        // instant it opened.
        if self.pending_input.is_none() {
            self.pending_input_title = None;
            self.mode = Mode::Normal;
        }
    }
    fn handle_confirm_key(&mut self, key: KeyEvent) {
        if matches!(self.pending_delete, Some((DeleteTarget::Notebook, _))) {
            self.handle_delete_notebook_confirm_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some((target, path)) = self.pending_delete.take() {
                    match target {
                        DeleteTarget::Note => {
                            let name = path
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let mut trashed = false;
                            if let Some(nb) = self.selected_notebook().cloned() {
                                let suffix = chrono::Local::now().timestamp_millis().to_string();
                                match self.trash_path(&nb, &path, &suffix) {
                                    Some(entry) => {
                                        self.last_trash = Some(vec![entry]);
                                        trashed = true;
                                    }
                                    None => {
                                        let _ = nb.delete_note_at(&path);
                                        self.last_trash = None;
                                    }
                                }
                                self.note_changed(&nb.name);
                            }
                            self.reload_notes();
                            self.set_status(if trashed {
                                format!("deleted '{name}' (undo: leader+u)")
                            } else {
                                format!("deleted '{name}'")
                            });
                        }
                        DeleteTarget::Notebook => unreachable!(
                            "notebook deletes are intercepted earlier, in handle_delete_notebook_confirm_key"
                        ),
                        DeleteTarget::Folder => {
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let mut trashed = false;
                            if let Some(nb) = self.selected_notebook().cloned() {
                                let suffix = chrono::Local::now().timestamp_millis().to_string();
                                match self.trash_path(&nb, &path, &suffix) {
                                    Some(entry) => {
                                        self.last_trash = Some(vec![entry]);
                                        trashed = true;
                                    }
                                    None => {
                                        if let Ok(relative) = path.strip_prefix(&nb.path) {
                                            let _ = nb.delete_folder_at(relative);
                                        }
                                        self.last_trash = None;
                                    }
                                }
                                self.note_changed(&nb.name);
                            }
                            self.reload_notes();
                            self.set_status(if trashed {
                                format!("deleted folder '{name}' (undo: leader+u)")
                            } else {
                                format!("deleted folder '{name}'")
                            });
                        }
                    }
                } else if let Some((note_path, commit_id)) = self.pending_revert.take() {
                    self.perform_revert(&note_path, &commit_id);
                } else if self.pending_clear_logs {
                    self.pending_clear_logs = false;
                    self.clear_logs();
                } else if let Some(entries) = self.pending_batch_delete.take() {
                    self.apply_batch_delete(entries);
                } else if let Some(trigger) = self.pending_delete_snippet.take() {
                    self.config.snippets.remove(&trigger);
                    self.save_config();
                    if self.settings_snippet_drill.as_deref() == Some(trigger.as_str()) {
                        self.settings_snippet_drill = None;
                        self.settings_field_selected = 0;
                    }
                    let remaining = crate::panel_settings::sorted_snippet_triggers(self).len();
                    self.settings_selected =
                        self.settings_selected.min(remaining.saturating_sub(1));
                    self.set_status(format!("deleted snippet '{trigger}'"));
                } else if let Some((name, path)) = self.pending_notebook_adopt.take() {
                    match shiki_core::git::init_repo(&path) {
                        Ok(_) => {
                            self.finish_notebook_adopt(name, path, "initialized git and adopted")
                        }
                        Err(e) => self.set_status(format!(
                            "could not initialize git at '{}': {e}",
                            path.display()
                        )),
                    }
                } else if let Some(notebook) = self.pending_finish_merge.take() {
                    self.finish_merge_notebook(&notebook);
                } else if let Some(notebook) = self.pending_abort_merge.take() {
                    self.abort_merge_notebook(&notebook);
                }
            }
            _ => {
                self.pending_delete = None;
                self.pending_revert = None;
                self.pending_clear_logs = false;
                self.pending_batch_delete = None;
                self.pending_delete_snippet = None;
                self.pending_notebook_adopt = None;
                self.pending_finish_merge = None;
                self.pending_abort_merge = None;
            }
        }
        self.confirm = None;
    }
    /// `Mode::Visual`'s `d`, once confirmed — deletes every captured entry
    /// (best-effort: one failure doesn't stop the rest) and always exits
    /// back to `Mode::Normal` afterward, since the selected range no longer
    /// means anything once its contents are gone.
    fn apply_batch_delete(&mut self, entries: Vec<SelectedEntry>) {
        let Some(nb) = self.selected_notebook().cloned() else {
            return;
        };
        // Shared across the whole batch, with a per-entry index appended
        // (`trash_path`'s `suffix`) — same-named items from different
        // folders deleted in the same batch still can't collide in the
        // trash, the same reasoning as a single delete's own timestamp.
        let batch_id = chrono::Local::now().timestamp_millis();
        let (mut ok, mut failed) = (0u32, 0u32);
        let mut trashed = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let path: &std::path::Path = match entry {
                SelectedEntry::Note(path) | SelectedEntry::Folder(path) => path,
            };
            let suffix = format!("{batch_id}-{index}");
            if let Some(entry) = self.trash_path(&nb, path, &suffix) {
                trashed.push(entry);
                ok += 1;
                continue;
            }
            let result = match entry {
                SelectedEntry::Note(path) => nb.delete_note_at(path),
                SelectedEntry::Folder(path) => path
                    .strip_prefix(&nb.path)
                    .map_err(|_| shiki_core::Error::NoteNotFound(path.display().to_string()))
                    .and_then(|relative| nb.delete_folder_at(relative)),
            };
            match result {
                Ok(()) => ok += 1,
                Err(_) => failed += 1,
            }
        }
        let any_trashed = !trashed.is_empty();
        self.last_trash = any_trashed.then_some(trashed);
        self.note_changed(&nb.name);
        self.reload_notes();
        self.mode = Mode::Normal;
        if failed == 0 {
            self.set_status(if any_trashed {
                format!("deleted {ok} item(s) (undo: leader+u)")
            } else {
                format!("deleted {ok} item(s)")
            });
        } else {
            self.set_status(format!("deleted {ok} item(s), {failed} failed"));
        }
    }
    /// Moves `path` (an absolute path to a note file or a whole folder)
    /// into the trash for notebook `nb`, tagged with `suffix` (unique per
    /// call — see the batch-delete call site for why). `None` if the trash
    /// directory couldn't be resolved or the move itself failed, in which
    /// case the caller should fall back to actually deleting `path`
    /// outright — a delete the user just confirmed should always visibly
    /// remove the item; trash is a safety net on top of that, not a
    /// precondition for it.
    fn trash_path(
        &self,
        nb: &Notebook,
        path: &std::path::Path,
        suffix: &str,
    ) -> Option<TrashedEntry> {
        let root = self.trash_root.as_ref()?;
        let trash_dir = root.join(&nb.name);
        let trash_path = shiki_core::trash::move_to_trash(path, &trash_dir, suffix).ok()?;
        Some(TrashedEntry {
            notebook: nb.name.clone(),
            original_path: path.to_path_buf(),
            trash_path,
        })
    }
    /// Restores everything from the last delete back to where it came from
    /// (leader+`u`) — a no-op, reported as such, if nothing's been deleted
    /// yet this session or a later delete already replaced the undo slot.
    fn undo_delete(&mut self) {
        let Some(entries) = self.last_trash.take() else {
            self.set_status("nothing to undo".into());
            return;
        };
        let (mut ok, mut failed) = (0u32, 0u32);
        let mut notebooks_touched = std::collections::HashSet::new();
        for entry in &entries {
            match shiki_core::trash::restore(&entry.trash_path, &entry.original_path) {
                Ok(()) => {
                    ok += 1;
                    notebooks_touched.insert(entry.notebook.clone());
                }
                Err(_) => failed += 1,
            }
        }
        for name in &notebooks_touched {
            self.note_changed(name);
        }
        self.reload_notes();
        if failed == 0 {
            self.set_status(format!("restored {ok} item(s)"));
        } else {
            self.set_status(format!("restored {ok} item(s), {failed} failed"));
        }
    }
    fn handle_insert_key(&mut self, key: KeyEvent) {
        // The `@`-quick-template dropdown only ever applies to the `NewNote`
        // prompt, and only once an `@` has actually been typed — checked
        // first so it can intercept `Up`/`Down`/`Enter` before they'd
        // otherwise do nothing (`Up`/`Down` aren't bound at all below) or
        // the wrong thing (a plain `Enter` would open `show_template_picker`
        // instead of running the already-highlighted quick command).
        if self.quick_template_query().is_some() {
            let filtered = self.quick_template_filtered();
            match key.code {
                // First `Esc` only dismisses the dropdown (drops the typed
                // `@word`) so the user can keep editing the title; a second
                // `Esc` (now with no `@` left) falls through to cancelling
                // the whole prompt, same as before this feature existed.
                KeyCode::Esc => {
                    if let Some(pos) = self.input.value.rfind('@') {
                        self.input.value.truncate(pos);
                    }
                    self.quick_template_selected = 0;
                    return;
                }
                KeyCode::Enter => {
                    if let Some(cmd) = filtered.get(self.quick_template_selected).cloned() {
                        self.apply_quick_template(cmd);
                    }
                    return;
                }
                KeyCode::Down => {
                    if self.quick_template_selected + 1 < filtered.len() {
                        self.quick_template_selected += 1;
                    }
                    return;
                }
                KeyCode::Up => {
                    self.quick_template_selected = self.quick_template_selected.saturating_sub(1);
                    return;
                }
                KeyCode::Backspace => {
                    self.input.backspace();
                    self.quick_template_selected = 0;
                    return;
                }
                KeyCode::Char(c) => {
                    self.input.push(c);
                    self.quick_template_selected = 0;
                    return;
                }
                _ => return,
            }
        }
        // The metadata field-value suggestions dropdown (`due`/`status`/
        // `priority`/any field with prior history) — deliberately doesn't
        // intercept `Esc`, unlike the `@`-dropdown above: there's no typed
        // trigger substring to strip first, so the very first `Esc` should
        // fall straight through to the ordinary cancel-the-whole-prompt
        // handling below (which reopens `show_metadata` for `Metadata`).
        if self.metadata_value_query().is_some() {
            match key.code {
                // `Tags` holds several comma-separated values, so `Enter`
                // always submits whatever's literally typed — overwriting
                // it with just the highlighted suggestion (the single-value
                // fields' behavior) would silently drop every other tag
                // already picked. `Tab` is the accept-a-suggestion key
                // there instead (see `apply_tag_suggestion`), the same role
                // a real shell/editor completion binds it to.
                KeyCode::Tab if self.is_tags_prompt() => {
                    let filtered = self.metadata_value_filtered();
                    if let Some(s) = filtered.get(self.metadata_value_selected).cloned() {
                        self.apply_tag_suggestion(&s);
                    }
                    return;
                }
                KeyCode::Enter => {
                    if !self.is_tags_prompt() {
                        let filtered = self.metadata_value_filtered();
                        if let Some(s) = filtered.get(self.metadata_value_selected).cloned() {
                            self.input.value = s;
                        }
                    }
                    self.confirm_input();
                    return;
                }
                KeyCode::Down => {
                    let len = self.metadata_value_filtered().len();
                    if self.metadata_value_selected + 1 < len {
                        self.metadata_value_selected += 1;
                    }
                    return;
                }
                KeyCode::Up => {
                    self.metadata_value_selected = self.metadata_value_selected.saturating_sub(1);
                    return;
                }
                KeyCode::Backspace => {
                    self.input.backspace();
                    self.metadata_value_selected = 0;
                    return;
                }
                KeyCode::Char(c) => {
                    self.input.push(c);
                    self.metadata_value_selected = 0;
                    return;
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Esc => {
                let kind = self.pending_input.take();
                self.pending_input_title = None;
                self.pending_batch = None;
                if kind == Some(PendingInput::NewNote) {
                    self.pending_new_note_body = None;
                }
                self.mode = Mode::Normal;
                // Every `Settings*` prompt is only ever started from inside
                // the Settings modal, which hides it first since a modal
                // underneath an `Insert`-mode prompt would otherwise still
                // intercept the keystrokes (`on_key` checks `show_settings`
                // before `self.mode`) — cancelling must reopen it, same as
                // confirming does.
                if matches!(
                    kind,
                    Some(PendingInput::SettingsNotebookRemote)
                        | Some(PendingInput::SettingsNotebookAutoSyncEvery)
                        | Some(PendingInput::SettingsGeneralText)
                        | Some(PendingInput::SettingsGitText)
                        | Some(PendingInput::SettingsExportText)
                        | Some(PendingInput::SettingsSnippetTrigger)
                        | Some(PendingInput::SettingsSnippetLabel)
                ) {
                    self.show_settings = true;
                }
                // `NotebookPassphrase` is reachable from two different
                // places (an auto-unlock prompt when switching into a
                // locked notebook, or Settings' Encryption field) — only
                // the latter should reopen Settings on cancel, hence the
                // flag instead of unconditionally listing the variant above.
                if kind == Some(PendingInput::NotebookPassphrase) {
                    self.passphrase_prompt_notebook = None;
                    self.passphrase_purpose = None;
                    self.passphrase_pending_first = None;
                    if self.reopen_settings_after_passphrase {
                        self.reopen_settings_after_passphrase = false;
                        self.show_settings = true;
                    }
                }
                // Cancelling any step of the metadata modal's prompt flow —
                // including mid-chain, on the `NewFieldKey` -> `NewFieldValue`
                // hop — reopens the metadata modal exactly like confirming
                // one does, rather than dropping back to bare NOTES/PREVIEW.
                if kind == Some(PendingInput::Metadata) {
                    self.metadata_prompt = None;
                    self.metadata_value_options.clear();
                    self.show_metadata = true;
                }
                // Same reopen-on-cancel reasoning as `Metadata` above.
                if kind == Some(PendingInput::RenameTag) {
                    self.pending_rename_tag = None;
                    self.show_tags = true;
                }
                if kind == Some(PendingInput::SaveQuery) {
                    self.pending_save_query_dsl = None;
                    self.show_query = true;
                }
            }
            KeyCode::Enter => self.confirm_input(),
            KeyCode::Backspace => self.input.backspace(),
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }
    fn handle_edit_key(&mut self, key: KeyEvent) {
        if self.editor_find.is_some() {
            self.handle_editor_find_key(key);
            return;
        }
        if self.show_slash_menu {
            self.handle_slash_menu_key(key);
            return;
        }
        if self.show_wikilink_menu {
            self.handle_wikilink_menu_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('s')
                if self.editing_scratchpad && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                let Some(editor) = self.editor.take() else {
                    return;
                };
                self.pending_new_note_body = Some(editor.contents());
                self.editing_scratchpad = false;
                self.start_input(PendingInput::NewNote, String::new());
                self.set_status("scratchpad ready to save as a note".into());
            }
            // Esc first collapses multi-cursor editing back to just the
            // primary (VS Code's own "Escape removes secondary cursors"
            // convention) — only a *second* Esc, with no secondaries left,
            // actually saves and exits.
            KeyCode::Esc => {
                if self.editor_secondary_cursors.is_empty() {
                    self.save_and_exit_edit();
                } else {
                    self.editor_secondary_cursors.clear();
                }
            }
            KeyCode::Char('f')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.config.editor.find_replace =>
            {
                self.open_editor_find();
            }
            KeyCode::Char('c')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.config.editor.os_clipboard =>
            {
                self.editor_copy_selection();
            }
            KeyCode::Char('x')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.config.editor.os_clipboard =>
            {
                self.editor_cut_selection();
            }
            KeyCode::Char('v')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.config.editor.os_clipboard =>
            {
                self.editor_paste_os();
            }
            // Off by default: ratatui-textarea's own Ctrl+A is Emacs-style
            // "move to start of line," existing muscle memory this
            // shouldn't change unless explicitly opted into. Also
            // collapses any secondary cursors first — "select everything,
            // replicated across N cursors" isn't a meaningful state.
            KeyCode::Char('a')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.config.editor.select_all_ctrl_a =>
            {
                self.editor_secondary_cursors.clear();
                if let Some(editor) = &mut self.editor {
                    editor.textarea.select_all();
                }
            }
            // Off by default, and collides with ratatui-textarea's own Emacs
            // Ctrl+D ("delete next character") the moment it's turned on —
            // an accepted tradeoff, same category as `select_all_ctrl_a`'s
            // own collision with Emacs Ctrl+A, since multi-cursor is opt-in.
            KeyCode::Char('d')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.config.editor.multi_cursor =>
            {
                self.editor_add_next_occurrence();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.editor_undo();
            }
            // Opens the same outline modal PREVIEW's `o` binding does,
            // without leaving Mode::Edit — jumping to a heading from here
            // moves the editor's own cursor instead of scrolling PREVIEW.
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_outline();
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.editor_redo();
            }
            // Ctrl+B wraps the selection in `**bold**` (or inserts an empty
            // pair with the cursor in the middle if nothing's selected).
            KeyCode::Char('b')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                    && self.config.editor.format_shortcuts =>
            {
                self.wrap_or_insert_pair("**", "**");
            }
            // Ctrl+Alt+I, not plain Ctrl+I — most terminal emulators send
            // Ctrl+I identically to Tab, so it isn't a reliable binding.
            KeyCode::Char('i')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.modifiers.contains(KeyModifiers::ALT)
                    && self.config.editor.format_shortcuts =>
            {
                self.wrap_or_insert_pair("_", "_");
            }
            // Auto-pair: typing an opening bracket/quote wraps the current
            // selection in the matching pair, or inserts an empty pair with
            // the cursor left in the middle. `[` is deliberately excluded —
            // pairing it would break `[[wikilink]]` autocomplete just below,
            // which depends on the user typing two real `[` in a row. Skips
            // while a multi-cursor edit is in flight, same as the
            // list-continuation features above.
            KeyCode::Char(c @ ('(' | '`' | '"'))
                if self.config.editor.auto_pair_brackets
                    && self.editor_secondary_cursors.is_empty() =>
            {
                let (open, close) = match c {
                    '(' => ("(", ")"),
                    '`' => ("`", "`"),
                    '"' => ("\"", "\""),
                    _ => unreachable!(),
                };
                self.wrap_or_insert_pair(open, close);
            }
            // VS Code's own "Add Cursor Above/Below" binding — a keyboard
            // alternative to Alt+Click that doesn't need the mouse at all,
            // requested after Alt+Click felt uncomfortable in practice.
            KeyCode::Up
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.modifiers.contains(KeyModifiers::ALT)
                    && self.config.editor.multi_cursor =>
            {
                self.editor_add_cursor_vertical(-1);
            }
            KeyCode::Down
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.modifiers.contains(KeyModifiers::ALT)
                    && self.config.editor.multi_cursor =>
            {
                self.editor_add_cursor_vertical(1);
            }
            // Alt+Up/Alt+Down move the current line past its neighbor;
            // Alt+D duplicates it. Excludes Ctrl so it can't shadow the
            // Ctrl+Alt+Up/Down multi-cursor bindings above.
            KeyCode::Up
                if key.modifiers.contains(KeyModifiers::ALT)
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.config.editor.move_line =>
            {
                self.editor_move_line(-1);
            }
            KeyCode::Down
                if key.modifiers.contains(KeyModifiers::ALT)
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.config.editor.move_line =>
            {
                self.editor_move_line(1);
            }
            KeyCode::Char('d')
                if key.modifiers.contains(KeyModifiers::ALT)
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.config.editor.duplicate_line =>
            {
                self.editor_duplicate_line();
            }
            // Tab: with an active selection, indents every line it spans
            // (a plain block-indent, not list-specific — same as any code
            // editor's Tab-on-a-selection); otherwise expands a matching
            // snippet trigger; otherwise, on a list/checkbox line, nests it
            // one level deeper. Falls through to plain Tab when none of
            // those apply. Shift+Tab (`KeyCode::BackTab`) mirrors this in
            // reverse (outdent), with no snippet step — there's nothing to
            // "un-expand".
            KeyCode::Tab => {
                let has_selection = self.config.editor.block_indent_select
                    && self
                        .editor
                        .as_ref()
                        .is_some_and(|e| e.textarea.selection_range().is_some());
                if has_selection {
                    self.indent_selected_lines(1);
                } else if self.config.editor.snippet_expand_tab && self.try_expand_snippet_on_tab()
                {
                    // handled
                } else if !(self.config.editor.auto_list_continue && self.try_indent_list_line(1)) {
                    self.editor_forward_key_default(key);
                }
            }
            KeyCode::BackTab => {
                let has_selection = self.config.editor.block_indent_select
                    && self
                        .editor
                        .as_ref()
                        .is_some_and(|e| e.textarea.selection_range().is_some());
                if has_selection {
                    self.indent_selected_lines(-1);
                } else if !(self.config.editor.auto_list_continue && self.try_indent_list_line(-1))
                {
                    self.editor_forward_key_default(key);
                }
            }
            // Always replaces the generic forward for these two — see
            // `editor_scroll_cursor`'s own doc comment for why forwarding
            // them to `ratatui-textarea` does nothing in this editor.
            KeyCode::PageDown => self.editor_scroll_cursor(self.page_step()),
            KeyCode::PageUp => self.editor_scroll_cursor(-self.page_step()),
            // Smart Home (no modifiers): toggles between the first
            // non-whitespace character and column 0, instead of always
            // column 0 — the common modern-editor convention (VS Code,
            // JetBrains, Emacs' `back-to-indentation`). Shift+Home is left
            // to the generic forward below, since this doesn't extend a
            // selection — only plain, unmodified Home gets the toggle.
            KeyCode::Home if key.modifiers.is_empty() => {
                self.smart_home();
            }
            // Plain End already works via the generic forward
            // (`ratatui-textarea`'s own `CursorMove::End` — end of the
            // *current* line, no viewport dependency). Ctrl+Home/Ctrl+End
            // add the jump-to-document-start/end that plain Home/End
            // don't cover, the common convention this was missing.
            KeyCode::Home if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(editor) = &mut self.editor {
                    editor.textarea.cancel_selection();
                    editor
                        .textarea
                        .move_cursor(ratatui_textarea::CursorMove::Jump(0, 0));
                }
            }
            KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(editor) = &mut self.editor {
                    let last_row = editor.textarea.lines().len().saturating_sub(1);
                    let last_col = editor.textarea.lines()[last_row].chars().count();
                    editor.textarea.cancel_selection();
                    editor
                        .textarea
                        .move_cursor(ratatui_textarea::CursorMove::Jump(
                            last_row as u16,
                            last_col as u16,
                        ));
                }
            }
            // Enter continues a list/checkbox line onto the next line, or
            // clears an already-empty item's prefix — see
            // `try_auto_continue_list`'s own doc comment. Falls through to
            // plain newline insertion when it doesn't apply.
            KeyCode::Enter if self.config.editor.auto_list_continue => {
                if !self.try_auto_continue_list() {
                    self.editor_forward_key_default(key);
                }
            }
            // Backspace right after an empty list/checkbox prefix removes
            // the whole prefix in one step — see
            // `try_backspace_exit_list`'s own doc comment.
            KeyCode::Backspace if self.config.editor.auto_list_continue => {
                if !self.try_backspace_exit_list() {
                    self.editor_forward_key_default(key);
                }
            }
            _ => self.editor_forward_key_default(key),
        }
    }
    /// The default "just type it" path for any key `handle_edit_key` didn't
    /// intercept above — plain character input, Enter/Backspace when
    /// `auto_list_continue` doesn't apply or is off, etc. Factored out of
    /// the old catch-all `_` arm so `try_auto_continue_list`/
    /// `try_backspace_exit_list` can fall back to exactly this same path
    /// instead of duplicating it.
    fn editor_forward_key_default(&mut self, key: KeyEvent) {
        if let Some(editor) = &mut self.editor {
            let edits = if self.editor_secondary_cursors.is_empty() {
                // A plain `input()` call can itself push *two*
                // history entries (typing over an active selection
                // is "delete selection, then insert" — see
                // `multicursor::undo_history_depth`'s own doc
                // comment for why this is measured, not assumed).
                let snapshot = editor.textarea.lines().to_vec();
                if editor.textarea.input(key) {
                    crate::multicursor::undo_history_depth(&mut editor.textarea, &snapshot)
                } else {
                    0
                }
            } else {
                crate::multicursor::replay_keystroke(
                    &mut editor.textarea,
                    key,
                    &mut self.editor_secondary_cursors,
                )
            };
            if edits > 0 {
                self.editor_undo_groups.push(edits);
                self.editor_redo_groups.clear();
            }
        }
        // `/` only opens the menu when it lands as the very first
        // character of the line (cursor was at column 0, so it's
        // at column 1 right after) — typing it anywhere else (a
        // fraction, a URL, mid-sentence) is just a literal slash.
        if key.code == KeyCode::Char('/') {
            let at_line_start = self
                .editor
                .as_ref()
                .is_some_and(|e| e.textarea.cursor().1 == 1);
            if at_line_start {
                self.slash_menu_selected = 0;
                self.show_slash_menu = true;
            }
        }
        // `[[` opens the wikilink menu the instant the second `[`
        // completes the pair, anywhere in the line — unlike `/`,
        // a wikilink is meaningful mid-sentence ("see [[Some
        // Note]] for details"), not just at line start.
        if key.code == KeyCode::Char('[') && self.config.general.wikilink_autocomplete {
            let opens_wikilink = self.editor.as_ref().is_some_and(|e| {
                let (row, col) = crate::editor::cursor_tuple(&e.textarea);
                e.textarea
                    .lines()
                    .get(row)
                    .is_some_and(|line| col >= 2 && line.chars().nth(col - 2) == Some('['))
            });
            if opens_wikilink {
                self.open_wikilink_menu();
            }
        }
    }
    /// Parses `line`'s leading list/checkbox marker, if any: optional
    /// leading spaces, then `-`/`*`/`+` or `N.`, then a single space,
    /// optionally followed by `[ ]`/`[x]`/`[X]` and another space. Returns
    /// the exact prefix substring (byte-for-byte, so `line[prefix.len()..]`
    /// is always a valid slice) or `None` if the line isn't a list item.
    fn list_prefix(line: &str) -> Option<&str> {
        let indent_len = line.len() - line.trim_start_matches(' ').len();
        let rest = &line[indent_len..];
        let marker_len =
            if rest.starts_with("- ") || rest.starts_with("* ") || rest.starts_with("+ ") {
                2
            } else {
                let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
                if digits > 0 && rest[digits..].starts_with(". ") {
                    digits + 2
                } else {
                    return None;
                }
            };
        let after_marker = &rest[marker_len..];
        let checkbox_len = if after_marker.starts_with("[ ] ")
            || after_marker.starts_with("[x] ")
            || after_marker.starts_with("[X] ")
        {
            4
        } else {
            0
        };
        Some(&line[..indent_len + marker_len + checkbox_len])
    }
    /// The prefix to carry onto the *next* line when continuing a list.
    /// A checkbox always resets to unchecked, since copying `[x]` would
    /// silently mark the new item done before it's even been written. An
    /// ordered marker (`N.`) increments — pressing Enter repeatedly after
    /// `1. ` walks `2. `, `3. `, … the same way Word/Notion do, rather than
    /// repeating the same number on every line; a bullet marker (`-`/`*`/
    /// `+`) is otherwise carried over unchanged.
    fn continuation_prefix(prefix: &str) -> String {
        let indent_len = prefix.len() - prefix.trim_start_matches(' ').len();
        let indent = &prefix[..indent_len];
        let rest = &prefix[indent_len..];
        let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && rest.get(digits..digits + 2) == Some(". ") {
            let number: u64 = rest[..digits].parse().unwrap_or(1);
            let after = &rest[digits + 2..];
            let checkbox = if after.starts_with("[ ] ")
                || after.starts_with("[x] ")
                || after.starts_with("[X] ")
            {
                "[ ] "
            } else {
                ""
            };
            return format!("{indent}{}. {checkbox}", number + 1);
        }
        match prefix.find("[x] ").or_else(|| prefix.find("[X] ")) {
            Some(idx) => format!("{}[ ] ", &prefix[..idx]),
            None => prefix.to_string(),
        }
    }
    /// Enter (`config.editor.auto_list_continue`): continues a
    /// `- item`/`- [ ] task`/`1. item` line onto the next line with the
    /// same prefix (checkboxes always reset to unchecked); Enter on an
    /// already-empty list item clears the prefix in place instead of
    /// starting a new empty item, matching Word/Notion's "empty item exits
    /// the list" convention. Only fires when the cursor sits at the end of
    /// the line and no multi-cursor edit is in flight — everywhere else,
    /// Enter is just a plain newline via `editor_forward_key_default`.
    fn try_auto_continue_list(&mut self) -> bool {
        if !self.editor_secondary_cursors.is_empty() {
            return false;
        }
        let Some(editor) = &mut self.editor else {
            return false;
        };
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        let Some(line) = editor.textarea.lines().get(row).cloned() else {
            return false;
        };
        if col != line.chars().count() {
            return false;
        }
        let Some(prefix) = Self::list_prefix(&line) else {
            return false;
        };
        let prefix_len = prefix.len();
        let is_empty_item = line[prefix_len..].trim().is_empty();
        let continuation = Self::continuation_prefix(prefix);
        let mut edits = 0usize;
        if is_empty_item {
            editor.textarea.cancel_selection();
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, 0));
            editor.textarea.start_selection();
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, col as u16));
            if editor.textarea.cut() {
                edits += 1;
            }
        } else if editor.textarea.insert_str(format!("\n{continuation}")) {
            edits += 1;
        }
        if edits > 0 {
            self.editor_undo_groups.push(edits);
            self.editor_redo_groups.clear();
        }
        true
    }
    /// Backspace (`config.editor.auto_list_continue`): if the cursor sits
    /// right after a list/checkbox prefix with nothing else on the line,
    /// removes the whole prefix in one step instead of one character.
    fn try_backspace_exit_list(&mut self) -> bool {
        if !self.editor_secondary_cursors.is_empty() {
            return false;
        }
        let Some(editor) = &mut self.editor else {
            return false;
        };
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        let Some(line) = editor.textarea.lines().get(row).cloned() else {
            return false;
        };
        let Some(prefix) = Self::list_prefix(&line) else {
            return false;
        };
        if col != prefix.chars().count() || !line[prefix.len()..].trim().is_empty() {
            return false;
        }
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, 0));
        editor.textarea.start_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, col as u16));
        if editor.textarea.cut() {
            self.editor_undo_groups.push(1);
            self.editor_redo_groups.clear();
        }
        true
    }
    /// Tab/Shift+Tab (`config.editor.auto_list_continue`), no selection:
    /// nests the cursor's list/checkbox line one level deeper (positive
    /// `direction`) or back out (negative `direction`) by adding/removing
    /// `LIST_INDENT_STEP` leading spaces — a no-op (but still "handled",
    /// so it doesn't fall through to snippet-expand/plain-Tab) on a line
    /// that isn't a list item, or when outdenting past column 0. The
    /// cursor stays over the same character, shifting with the indent.
    fn try_indent_list_line(&mut self, direction: i8) -> bool {
        if !self.editor_secondary_cursors.is_empty() {
            return false;
        }
        let Some(editor) = &self.editor else {
            return false;
        };
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        let Some(line) = editor.textarea.lines().get(row).cloned() else {
            return false;
        };
        if Self::list_prefix(&line).is_none() {
            return false;
        }
        let (new_line, shift) = Self::indent_line(&line, direction);
        if new_line == line {
            return true;
        }
        let Some(editor) = &mut self.editor else {
            return false;
        };
        let edits = Self::replace_whole_line(editor, row, &line, &new_line);
        let new_col = (col as isize + shift).max(0) as u16;
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, new_col));
        if edits > 0 {
            self.editor_undo_groups.push(edits);
            self.editor_redo_groups.clear();
        }
        true
    }
    /// Tab/Shift+Tab with an active selection: indents/outdents every line
    /// the selection spans, list or not — the general "block indent"
    /// behavior any code editor gives a multi-line selection, not gated by
    /// `auto_list_continue` since it isn't list-specific. A selection
    /// whose end sits at column 0 doesn't visually include that row (the
    /// common result of a `Shift+Down`-built selection), so that row is
    /// excluded — otherwise a 3-line visual selection would indent a 4th,
    /// untouched-looking row.
    fn indent_selected_lines(&mut self, direction: i8) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let Some((start, end)) = editor.textarea.selection_range() else {
            return;
        };
        let (start_row, _) = start;
        let (end_row, end_col) = end;
        let last_row = editor.textarea.lines().len().saturating_sub(1);
        let end_row = if end_col == 0 && end_row > start_row {
            end_row - 1
        } else {
            end_row
        }
        .min(last_row);
        let lines: Vec<String> = editor.textarea.lines().to_vec();
        editor.textarea.cancel_selection();
        let mut edits = 0usize;
        for row in start_row..=end_row {
            let Some(line) = lines.get(row) else {
                continue;
            };
            let (new_line, _shift) = Self::indent_line(line, direction);
            if new_line == *line {
                continue;
            }
            edits += Self::replace_whole_line(editor, row, line, &new_line);
        }
        if edits > 0 {
            self.editor_undo_groups.push(edits);
            self.editor_redo_groups.clear();
        }
    }
    /// Adds (`direction > 0`) or removes (`direction < 0`) one
    /// `LIST_INDENT_STEP`-wide chunk of leading spaces from `line`,
    /// returning the new text plus the column shift that keeps whatever
    /// was under the cursor lined up. Outdenting never removes more than
    /// the line's own existing indent.
    fn indent_line(line: &str, direction: i8) -> (String, isize) {
        if direction > 0 {
            (
                format!("{}{line}", " ".repeat(LIST_INDENT_STEP)),
                LIST_INDENT_STEP as isize,
            )
        } else {
            let current_indent = line.len() - line.trim_start_matches(' ').len();
            let remove = LIST_INDENT_STEP.min(current_indent);
            (line[remove..].to_string(), -(remove as isize))
        }
    }
    /// Replaces `row`'s entire content with `new_line` — select the whole
    /// line (by its *old* text's char length) and cut, then insert the
    /// replacement, the same two-step primitive `editor_move_line`/
    /// `editor_duplicate_line` already use for whole-line edits. Returns
    /// how many of the two steps actually changed anything, for the
    /// caller's own undo-group bookkeeping.
    fn replace_whole_line(
        editor: &mut InlineEditor,
        row: usize,
        old_line: &str,
        new_line: &str,
    ) -> usize {
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, 0));
        editor.textarea.start_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(
                row as u16,
                old_line.chars().count() as u16,
            ));
        let mut edits = 0usize;
        if editor.textarea.cut() {
            edits += 1;
        }
        if editor.textarea.insert_str(new_line) {
            edits += 1;
        }
        edits
    }
    /// Snapshots the current notebook's notes (excluding the one being
    /// edited — linking to yourself isn't useful) once, the moment `[[`
    /// opens the menu — same "expensive walk once, cheap re-score per
    /// keystroke" shape `open_global_search`/`refresh_global_search`
    /// already established, just triggered by typing instead of an
    /// explicit action.
    fn open_wikilink_menu(&mut self) {
        let current_path = self.selected_note().map(|n| n.path.clone());
        let Some(nb) = self.selected_notebook() else {
            return;
        };
        self.wikilink_candidates = nb
            .all_notes_recursive()
            .unwrap_or_default()
            .into_iter()
            .filter(|n| Some(&n.path) != current_path.as_ref())
            .collect();
        self.wikilink_menu_selected = 0;
        self.show_wikilink_menu = true;
        self.refresh_wikilink_menu();
    }
    /// The typed filter for the wikilink menu: everything between the
    /// opening `[[` closest to (and before) the cursor and the cursor
    /// itself, read live off the buffer — same reasoning as `slash_query`.
    /// `None` (closing the menu) covers the query no longer being
    /// findable (backspaced past the opening `[[`) or already containing
    /// a `]` (the pair was closed by hand instead of via a selection).
    pub(crate) fn wikilink_query(&self) -> Option<String> {
        if !self.show_wikilink_menu {
            return None;
        }
        let editor = self.editor.as_ref()?;
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        let line = editor.textarea.lines().get(row)?;
        let chars: Vec<char> = line.chars().collect();
        let upto = &chars[..col.min(chars.len())];
        let start = upto.windows(2).rposition(|w| w == ['[', '['])?;
        let query: String = upto[start + 2..].iter().collect();
        if query.contains(['[', ']']) {
            return None;
        }
        Some(query)
    }
    /// Re-scores `wikilink_candidates` by title against the current query,
    /// via the same shared `search_engine` global search/notebook-jump
    /// already use — fuzzy, not a plain substring match, so `[[whr]]` can
    /// still find "Weekend Hiking Trip" the way `/` (notebook jump) would.
    fn refresh_wikilink_menu(&mut self) {
        let query = self.wikilink_query().unwrap_or_default();
        let mut hits = self.search_engine.search(&query, &self.wikilink_candidates);
        hits.truncate(30);
        self.wikilink_results = hits;
    }
    pub(crate) fn wikilink_menu_filtered(&self) -> Vec<&Note> {
        self.wikilink_results
            .iter()
            .filter_map(|hit| self.wikilink_candidates.get(hit.index))
            .collect()
    }
    fn handle_wikilink_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.show_wikilink_menu = false,
            KeyCode::Up => {
                self.wikilink_menu_selected = self.wikilink_menu_selected.saturating_sub(1)
            }
            KeyCode::Down => {
                let len = self.wikilink_results.len();
                if self.wikilink_menu_selected + 1 < len {
                    self.wikilink_menu_selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(title) = self
                    .wikilink_menu_filtered()
                    .get(self.wikilink_menu_selected)
                    .map(|n| n.frontmatter.title.clone())
                {
                    self.show_wikilink_menu = false;
                    self.apply_wikilink_selection(&title);
                }
            }
            KeyCode::Char(_) | KeyCode::Backspace => {
                if let Some(editor) = &mut self.editor {
                    editor.textarea.input(key);
                }
                match self.wikilink_query() {
                    Some(_) => {
                        self.wikilink_menu_selected = 0;
                        self.refresh_wikilink_menu();
                    }
                    None => self.show_wikilink_menu = false,
                }
            }
            _ => {}
        }
    }
    /// Replaces the typed `[[query` (the same range `wikilink_query` reads)
    /// with a complete `[[Title]]`, cursor landing right after the closing
    /// `]]` — same "delete the exact range that was being typed, then
    /// insert the resolved text" shape as `apply_slash_command`.
    fn apply_wikilink_selection(&mut self, title: &str) {
        // The typed "[[query" itself gets replayed across every secondary
        // cursor (the generic per-keystroke path in `replay_keystroke`
        // handles that fine), but applying the actual selection below only
        // ever edits the primary cursor's `textarea` — a secondary cursor
        // would be left with its own unconsumed literal "[[query" text and
        // a now-stale `(row, col)` once the primary's edit shifts things
        // out from under it. Collapsing to a single cursor first is the
        // same "not a meaningful multi-cursor state" call already made for
        // Ctrl+A/select-all above.
        self.editor_secondary_cursors.clear();
        let Some(editor) = &mut self.editor else {
            return;
        };
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        let Some(line) = editor.textarea.lines().get(row) else {
            return;
        };
        let chars: Vec<char> = line.chars().collect();
        let upto = &chars[..col.min(chars.len())];
        let Some(start) = upto.windows(2).rposition(|w| w == ['[', '[']) else {
            return;
        };
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, start as u16));
        editor.textarea.delete_str(col - start);
        editor.textarea.insert_str(format!("[[{title}]]"));
    }
    /// Ctrl+C (`config.editor.os_clipboard`): extracts the selected text
    /// *before* mutating anything (`copy()` may collapse/alter the
    /// selection as a side effect), writes it to the real OS clipboard —
    /// falling back automatically to the existing OSC 52 mechanism when
    /// arboard can't reach a display server (headless SSH) — and still
    /// calls `copy()` too, so the internal yank register (and thus Ctrl+V
    /// with `os_clipboard` off) stays in sync regardless.
    fn editor_copy_selection(&mut self) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let Some((start, end)) = editor.textarea.selection_range() else {
            return;
        };
        let text = crate::editor::selection_text(editor.textarea.lines(), start, end);
        editor.textarea.copy();
        if !crate::clipboard::copy_os(&text) {
            crate::clipboard::copy(&text);
        }
    }
    /// Ctrl+X — same as `editor_copy_selection` but deletes the selection
    /// (`cut()`) instead of leaving it in place.
    fn editor_cut_selection(&mut self) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let Some((start, end)) = editor.textarea.selection_range() else {
            return;
        };
        let text = crate::editor::selection_text(editor.textarea.lines(), start, end);
        let cut = editor.textarea.cut();
        if !crate::clipboard::copy_os(&text) {
            crate::clipboard::copy(&text);
        }
        if cut {
            self.editor_undo_groups.push(1);
            self.editor_redo_groups.clear();
        }
    }
    /// Ctrl+V — tries the real OS clipboard first; if arboard can't reach
    /// one (no display server), falls through to `ratatui-textarea`'s own
    /// internal yank register (`paste()`), exactly what Ctrl+V already did
    /// before `os_clipboard` existed, so nothing regresses in that case.
    fn editor_paste_os(&mut self) {
        match crate::clipboard::paste_os() {
            Some(text) => {
                let edits = self.insert_pasted_text(&text);
                if edits > 0 {
                    self.editor_undo_groups.push(edits);
                    self.editor_redo_groups.clear();
                }
            }
            None => {
                let Some(editor) = &mut self.editor else {
                    return;
                };
                if editor.textarea.paste() {
                    self.editor_undo_groups.push(1);
                    self.editor_redo_groups.clear();
                }
            }
        }
    }
    /// Shared by `editor_paste_os` (Ctrl+V) and `on_paste` (bracketed
    /// paste): inserts `text` as-is, unless `paste_url_as_link` is on,
    /// there's an active selection, and `text` (trimmed) is a bare
    /// `http(s)://` URL with no whitespace — in that case the selected
    /// text is wrapped as `[selected](url)` instead of being replaced by
    /// the raw URL. Returns how many textarea-level edits happened, for
    /// the caller's own undo-group bookkeeping.
    fn insert_pasted_text(&mut self, text: &str) -> usize {
        let Some(editor) = &mut self.editor else {
            return 0;
        };
        let trimmed = text.trim();
        let is_bare_url = (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
            && !trimmed.contains(char::is_whitespace);
        if self.config.editor.paste_url_as_link && is_bare_url {
            if let Some((start, end)) = editor.textarea.selection_range() {
                let selected = crate::editor::selection_text(editor.textarea.lines(), start, end);
                if !selected.is_empty() {
                    let mut edits = 0usize;
                    if editor.textarea.cut() {
                        edits += 1;
                    }
                    if editor
                        .textarea
                        .insert_str(format!("[{selected}]({trimmed})"))
                    {
                        edits += 1;
                    }
                    return edits;
                }
            }
        }
        usize::from(editor.textarea.insert_str(text))
    }
    /// Shared by the bold/italic shortcuts (`format_shortcuts`) and
    /// bracket/quote auto-pairing (`auto_pair_brackets`): wraps the active
    /// selection in `open`/`close`, or — with nothing selected — inserts
    /// the empty pair and leaves the cursor between the two, ready to type
    /// inside it.
    fn wrap_or_insert_pair(&mut self, open: &str, close: &str) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let mut edits = 0usize;
        if let Some((start, end)) = editor.textarea.selection_range() {
            let text = crate::editor::selection_text(editor.textarea.lines(), start, end);
            if editor.textarea.cut() {
                edits += 1;
            }
            if editor.textarea.insert_str(format!("{open}{text}{close}")) {
                edits += 1;
            }
        } else {
            if editor.textarea.insert_str(format!("{open}{close}")) {
                edits += 1;
            }
            let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
            let close_len = close.chars().count();
            if col >= close_len {
                editor
                    .textarea
                    .move_cursor(ratatui_textarea::CursorMove::Jump(
                        row as u16,
                        (col - close_len) as u16,
                    ));
            }
        }
        if edits > 0 {
            self.editor_undo_groups.push(edits);
            self.editor_redo_groups.clear();
        }
    }
    /// Plain Home (no modifiers): the first press moves to the line's
    /// first non-whitespace character; pressing it again from there (or
    /// pressing it on a line that's already at column 0, e.g. blank or
    /// unindented) moves to column 0 — the standard "smart home" toggle.
    fn smart_home(&mut self) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        let Some(line) = editor.textarea.lines().get(row) else {
            return;
        };
        let first_non_ws = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        let target = if col == first_non_ws { 0 } else { first_non_ws };
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(
                row as u16,
                target as u16,
            ));
    }
    /// Alt+Up/Alt+Down: swaps the cursor's line with the neighbor `delta`
    /// rows away (`delta` is always `-1` or `1` — a single step, repeated
    /// presses walk it further). No-op at either edge of the buffer. Cursor
    /// column is preserved (clamped to the destination line's length) so
    /// the same character stays under the cursor after the move.
    fn editor_move_line(&mut self, delta: isize) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        let line_count = editor.textarea.lines().len();
        let target = row as isize + delta;
        if target < 0 || target as usize >= line_count {
            return;
        }
        let target = target as usize;
        let a = row.min(target);
        let b = row.max(target);
        let line_a = editor.textarea.lines()[a].clone();
        let line_b = editor.textarea.lines()[b].clone();
        let b_len = line_b.chars().count();
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(a as u16, 0));
        editor.textarea.start_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(b as u16, b_len as u16));
        let mut edits = 0usize;
        if editor.textarea.cut() {
            edits += 1;
        }
        if editor.textarea.insert_str(format!("{line_b}\n{line_a}")) {
            edits += 1;
        }
        let new_col = col.min(if target == a {
            line_b.chars().count()
        } else {
            line_a.chars().count()
        });
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(
                target as u16,
                new_col as u16,
            ));
        if edits > 0 {
            self.editor_undo_groups.push(edits);
            self.editor_redo_groups.clear();
        }
    }
    /// Alt+D: duplicates the cursor's current line directly below it,
    /// keeping the cursor at the same column on the new copy.
    fn editor_duplicate_line(&mut self) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        let Some(line) = editor.textarea.lines().get(row).cloned() else {
            return;
        };
        let line_len = line.chars().count();
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(
                row as u16,
                line_len as u16,
            ));
        let mut edits = 0usize;
        if editor.textarea.insert_str(format!("\n{line}")) {
            edits += 1;
        }
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(
                (row + 1) as u16,
                col.min(line_len) as u16,
            ));
        if edits > 0 {
            self.editor_undo_groups.push(edits);
            self.editor_redo_groups.clear();
        }
    }
    /// Ctrl+U — pops the most recent entry off `editor_undo_groups` and
    /// undoes exactly that many steps as one action, stopping early if
    /// `textarea.undo()` ever returns `false` (nothing left to undo — an
    /// empty stack pops `1` as a harmless default, which just no-ops
    /// against `undo()`'s own "nothing to undo" case). Pushes however many
    /// steps were *actually* undone onto `editor_redo_groups`, so the next
    /// Ctrl+R restores exactly that many, not the (possibly larger)
    /// originally-requested count.
    fn editor_undo(&mut self) {
        let count = self.editor_undo_groups.pop().unwrap_or(1);
        let Some(editor) = &mut self.editor else {
            return;
        };
        let mut undone = 0;
        for _ in 0..count {
            if !editor.textarea.undo() {
                break;
            }
            undone += 1;
        }
        if undone > 0 {
            self.editor_redo_groups.push(undone);
        }
    }
    /// Ctrl+R — mirrors `editor_undo` for redo.
    fn editor_redo(&mut self) {
        let count = self.editor_redo_groups.pop().unwrap_or(1);
        let Some(editor) = &mut self.editor else {
            return;
        };
        let mut redone = 0;
        for _ in 0..count {
            if !editor.textarea.redo() {
                break;
            }
            redone += 1;
        }
        if redone > 0 {
            self.editor_undo_groups.push(redone);
        }
    }
    /// Moves the editor's cursor `delta` logical rows (clamped to the
    /// buffer), preserving column as closely as the target row allows —
    /// drives both `PageUp`/`PageDown` (`delta = ±self.page_step()`) and mouse
    /// wheel scrolling. This exists because forwarding `PageUp`/`PageDown`
    /// to `ratatui-textarea` (as the generic catch-all does for every other
    /// key) does nothing at all in this editor: verified live.
    /// `ratatui-textarea`'s own `Scrolling::PageUp/PageDown` scrolls its
    /// *internal* `Viewport`, which is only ever populated by its own
    /// `Widget` impl — and `InlineEditor::render` deliberately bypasses
    /// that entirely (see the struct doc comment), so the viewport
    /// `Scrolling` scrolls is permanently zero-sized and the cursor
    /// (adjusted to "stay in viewport" afterward) never actually moves.
    /// Moving the cursor directly sidesteps that viewport entirely, and
    /// `InlineEditor`'s own scroll-follow (driven purely by cursor
    /// position, not an independent scroll offset) then scrolls the view
    /// to match on the next render — the only "scroll" this editor has.
    fn editor_scroll_cursor(&mut self, delta: isize) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        let last_row = editor.textarea.lines().len().saturating_sub(1);
        let new_row = (row as isize + delta).clamp(0, last_row as isize) as usize;
        if new_row == row {
            return;
        }
        let new_col = col.min(editor.textarea.lines()[new_row].chars().count());
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(
                new_row as u16,
                new_col as u16,
            ));
    }
    /// Ctrl+Alt+Up/Down (`config.editor.multi_cursor`): adds a cursor one
    /// row above (`dir < 0`) or below (`dir > 0`) whichever existing
    /// cursor (primary or secondary) already sits furthest in that
    /// direction — so repeated presses keep extending the same contiguous
    /// block of cursors upward/downward, matching VS Code's own behavior,
    /// rather than always adding relative to the primary alone. The new
    /// cursor's column is clamped to its target line's length, same as a
    /// plain `Up`/`Down` keypress would clamp against a shorter line.
    fn editor_add_cursor_vertical(&mut self, dir: isize) {
        let Some(editor) = &self.editor else {
            return;
        };
        let primary = crate::editor::cursor_tuple(&editor.textarea);
        let reference = self
            .editor_secondary_cursors
            .iter()
            .map(|c| c.pos)
            .chain(std::iter::once(primary))
            .reduce(|a, b| if dir < 0 { a.min(b) } else { a.max(b) });
        let Some((row, col)) = reference else {
            return;
        };
        let target_row = if dir < 0 {
            match row.checked_sub(1) {
                Some(r) => r,
                None => return,
            }
        } else {
            row + 1
        };
        if target_row >= editor.textarea.lines().len() {
            return;
        }
        let target_col = col.min(editor.textarea.lines()[target_row].chars().count());
        crate::multicursor::add_cursor_at(
            &mut self.editor_secondary_cursors,
            primary,
            (target_row, target_col),
        );
    }
    /// Ctrl+D (`config.editor.multi_cursor`): the first press (primary has
    /// no selection yet) just selects the word under the cursor, matching
    /// VS Code's own first-press behavior — no new cursor yet. Every press
    /// after that adds a new selecting cursor at the next occurrence of
    /// the current selection's text, searching forward from whichever
    /// existing cursor (primary or secondary) sits furthest along in the
    /// document, and wrapping around the whole buffer. Reports "no more
    /// occurrences" instead of adding a duplicate once every occurrence
    /// already has its own cursor.
    fn editor_add_next_occurrence(&mut self) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        if editor.textarea.selection_range().is_none() {
            let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
            let chars: Vec<char> = editor.textarea.lines()[row].chars().collect();
            let (start, end) = crate::editor::word_range(&chars, col);
            if start == end {
                return;
            }
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, start as u16));
            editor.textarea.start_selection();
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, end as u16));
            return;
        }
        let (start, end) = editor.textarea.selection_range().unwrap();
        let query = crate::editor::selection_text(editor.textarea.lines(), start, end);
        if query.is_empty() {
            return;
        }
        let matches = crate::editor::find_all_matches(editor.textarea.lines(), &query);
        let last_end = self
            .editor_secondary_cursors
            .iter()
            .map(|c| c.pos)
            .chain(std::iter::once(end))
            .max()
            .unwrap_or(end);
        let Some((row, mstart, mend)) = crate::editor::next_match(&matches, last_end, false) else {
            self.set_status("no more occurrences".into());
            return;
        };
        if !crate::multicursor::add_occurrence(
            &mut self.editor_secondary_cursors,
            row,
            mstart,
            mend,
        ) {
            self.set_status("no more occurrences".into());
        }
    }
    /// Opens Ctrl+F's find/replace bar, seeding the query from the current
    /// selection's text when one exists (so selecting a word first and
    /// pressing Ctrl+F searches for it immediately, same convenience a GUI
    /// editor's find bar has) — otherwise empty. `anchor` (where a fresh
    /// search starts scanning from) is the cursor position at the moment
    /// the bar opens.
    fn open_editor_find(&mut self) {
        let Some(editor) = &self.editor else {
            return;
        };
        let seed = editor
            .textarea
            .selection_range()
            .map(|(start, end)| crate::editor::selection_text(editor.textarea.lines(), start, end))
            .unwrap_or_default();
        let anchor = crate::editor::cursor_tuple(&editor.textarea);
        self.editor_find = Some(EditorFindState {
            query: InputBox {
                value: seed,
                masked: false,
            },
            replace: InputBox::default(),
            focus: FindField::Query,
            anchor,
        });
    }
    fn handle_editor_find_key(&mut self, key: KeyEvent) {
        let Some(state) = &mut self.editor_find else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.editor_find = None;
                if let Some(editor) = &mut self.editor {
                    editor.textarea.cancel_selection();
                }
            }
            KeyCode::Tab => {
                state.focus = match state.focus {
                    FindField::Query => FindField::Replace,
                    FindField::Replace => FindField::Query,
                };
            }
            KeyCode::Backspace => {
                match state.focus {
                    FindField::Query => state.query.backspace(),
                    FindField::Replace => state.replace.backspace(),
                }
                if state.focus == FindField::Query {
                    self.editor_find_step(false);
                }
            }
            KeyCode::Char(c) => {
                match state.focus {
                    FindField::Query => state.query.push(c),
                    FindField::Replace => state.replace.push(c),
                }
                if state.focus == FindField::Query {
                    self.editor_find_step(false);
                }
            }
            KeyCode::Enter => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                let alt = key.modifiers.contains(KeyModifiers::ALT);
                if ctrl && alt {
                    self.editor_replace_all();
                } else if ctrl {
                    self.editor_replace_current_and_advance();
                } else if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.editor_find_step(true);
                } else {
                    self.editor_find_step(false);
                }
            }
            _ => {}
        }
    }
    /// Jumps to the next (or, if `backward`, previous) match, selecting it
    /// (`cancel_selection` + `start_selection` + `move_cursor(Jump(..))`,
    /// the exact same pattern double/triple-click already established) so
    /// it's visible via the editor's own selection-highlight rendering —
    /// there's no separate "search highlight" style, the match just becomes
    /// the current selection. Search continues from the current
    /// selection's end (forward) or start (backward) once one exists, or
    /// from the bar's `anchor` before any match has been jumped to yet.
    fn editor_find_step(&mut self, backward: bool) {
        let Some(state) = &self.editor_find else {
            return;
        };
        let query = state.query.value.clone();
        let anchor = state.anchor;
        let Some(editor) = &mut self.editor else {
            return;
        };
        let matches = crate::editor::find_all_matches(editor.textarea.lines(), &query);
        if matches.is_empty() {
            editor.textarea.cancel_selection();
            return;
        }
        let from = editor
            .textarea
            .selection_range()
            .map(|(start, end)| if backward { start } else { end })
            .unwrap_or(anchor);
        if let Some((row, start, end)) = crate::editor::next_match(&matches, from, backward) {
            editor.textarea.cancel_selection();
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, start as u16));
            editor.textarea.start_selection();
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, end as u16));
        }
    }
    /// Ctrl+Enter: replaces the currently-selected match (if the selection
    /// is in fact a match — it always is right after `editor_find_step`,
    /// since a match becoming the selection is the only way one gets
    /// created while the find bar is open) and advances to the next one.
    /// `cut()` and `insert_str()` are two *separate* undo-history entries
    /// (verified directly: undoing once after a cut+insert only reverts
    /// the insert, leaving the cut text still missing) — pushing a flat
    /// `1` here would leave Ctrl+U in a broken halfway state after every
    /// single replacement, so the group size is the sum of their own
    /// return values (each `true` means that call actually pushed an
    /// entry) instead of an assumed constant.
    fn editor_replace_current_and_advance(&mut self) {
        let replacement = match &self.editor_find {
            Some(state) => state.replace.value.clone(),
            None => return,
        };
        let mut group = 0usize;
        if let Some(editor) = &mut self.editor {
            if editor.textarea.selection_range().is_some() {
                group += usize::from(editor.textarea.cut());
                group += usize::from(editor.textarea.insert_str(&replacement));
            }
        }
        if group > 0 {
            self.editor_undo_groups.push(group);
            self.editor_redo_groups.clear();
        }
        self.editor_find_step(false);
    }
    /// Ctrl+Alt+Enter: replaces every occurrence. Always re-searches from
    /// scratch each iteration (the buffer just changed) but only accepts a
    /// match at or after `search_from` — which advances to the real cursor
    /// position after each replacement — so a replacement that happens to
    /// contain the query itself (e.g. "a" -> "aa") can never re-match its
    /// own freshly-inserted text and loop forever. The *whole* replace-all
    /// undoes as one Ctrl+U (one pushed group summing every individual
    /// `cut`/`insert_str`'s own real return value — see
    /// `editor_replace_current_and_advance`'s doc comment for why that sum
    /// matters instead of an assumed constant), not one occurrence at a
    /// time — cheap to get right now that `editor_undo_groups` is a real
    /// stack instead of a single pending value.
    fn editor_replace_all(&mut self) {
        let (query, replacement) = match &self.editor_find {
            Some(state) => (state.query.value.clone(), state.replace.value.clone()),
            None => return,
        };
        if query.is_empty() {
            return;
        }
        let Some(editor) = &mut self.editor else {
            return;
        };
        let mut count = 0usize;
        let mut group = 0usize;
        let mut search_from = (0usize, 0usize);
        loop {
            let matches = crate::editor::find_all_matches(editor.textarea.lines(), &query);
            let Some(&(row, start, end)) = matches.iter().find(|&&(r, s, _)| (r, s) >= search_from)
            else {
                break;
            };
            editor.textarea.cancel_selection();
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, start as u16));
            editor.textarea.start_selection();
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, end as u16));
            group += usize::from(editor.textarea.cut());
            group += usize::from(editor.textarea.insert_str(&replacement));
            search_from = crate::editor::cursor_tuple(&editor.textarea);
            count += 1;
        }
        editor.textarea.cancel_selection();
        if group > 0 {
            self.editor_undo_groups.push(group);
            self.editor_redo_groups.clear();
        }
        self.set_status(format!(
            "replaced {count} occurrence{}",
            if count == 1 { "" } else { "s" }
        ));
    }
    /// The typed filter for the `/`-menu: everything after the leading `/`
    /// up to the cursor, read live off the editor's own buffer rather than
    /// tracked in a separate field — the same reasoning `App.show_slash_menu`'s
    /// doc comment gives, just for the query text instead of the open/closed
    /// state. `None` means the menu should close (the query no longer starts
    /// with `/`, e.g. it was backspaced away).
    pub(crate) fn slash_query(&self) -> Option<String> {
        if !self.show_slash_menu {
            return None;
        }
        let editor = self.editor.as_ref()?;
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        if col == 0 {
            return None;
        }
        let line = editor.textarea.lines().get(row)?;
        let chars: Vec<char> = line.chars().collect();
        if chars.first() != Some(&'/') {
            return None;
        }
        Some(chars[1..col.min(chars.len())].iter().collect())
    }
    /// `slash_menu::all_commands` narrowed to whatever's typed after the
    /// `/` — same case-insensitive substring match the `@` quick-template
    /// dropdown and which-key modal both already use for their own typed
    /// filters.
    pub(crate) fn slash_menu_filtered(&self) -> Vec<slash_menu::SlashCommand> {
        let Some(query) = self.slash_query() else {
            return Vec::new();
        };
        let query = query.to_lowercase();
        slash_menu::all_commands(&self.config)
            .into_iter()
            .filter(|cmd| {
                cmd.trigger.to_lowercase().contains(&query)
                    || cmd.label.to_lowercase().contains(&query)
            })
            .collect()
    }
    fn handle_slash_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.show_slash_menu = false,
            KeyCode::Up => self.slash_menu_selected = self.slash_menu_selected.saturating_sub(1),
            KeyCode::Down => {
                let len = self.slash_menu_filtered().len();
                if self.slash_menu_selected + 1 < len {
                    self.slash_menu_selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(cmd) = self
                    .slash_menu_filtered()
                    .get(self.slash_menu_selected)
                    .cloned()
                {
                    self.show_slash_menu = false;
                    self.apply_slash_command(&cmd);
                }
            }
            KeyCode::Char(_) | KeyCode::Backspace => {
                if let Some(editor) = &mut self.editor {
                    editor.textarea.input(key);
                }
                match self.slash_query() {
                    Some(_) => self.slash_menu_selected = 0,
                    None => self.show_slash_menu = false,
                }
            }
            _ => {}
        }
    }
    /// Substitutes `{{title}}`/`{{date}}`/`{{time}}`/`{{notebook}}` in a
    /// snippet body the same way note templates are rendered
    /// (`shiki_core::Template::render`), then splits off a literal
    /// `{{cursor}}` marker if present — shared by `apply_slash_command`
    /// (the `/`-menu) and `try_expand_snippet_on_tab` (Tab-expansion), the
    /// two places a snippet's body actually gets turned into text.
    fn render_snippet_template(&self, body: &str) -> (String, Option<String>) {
        let title = self
            .selected_note()
            .map(|n| n.frontmatter.title.clone())
            .unwrap_or_default();
        let now = chrono::Local::now();
        let mut vars = std::collections::HashMap::new();
        vars.insert("title", title);
        vars.insert("date", now.format("%Y-%m-%d").to_string());
        vars.insert("time", now.format("%H:%M").to_string());
        vars.insert(
            "notebook",
            self.selected_notebook()
                .map(|nb| nb.name.clone())
                .unwrap_or_default(),
        );
        let rendered = shiki_core::Template {
            name: String::new(),
            contents: body.to_string(),
        }
        .render(&vars);
        match rendered.split_once("{{cursor}}") {
            Some((before, after)) => (before.to_string(), Some(after.to_string())),
            None => (rendered, None),
        }
    }
    /// Tab (`config.editor.snippet_expand_tab`): if the run of
    /// non-whitespace characters immediately before the cursor matches a
    /// snippet trigger — built-in or `[snippets.<trigger>]`, the exact same
    /// set the `/`-menu draws from (`slash_menu::all_commands`) — replaces
    /// just that trigger text with the snippet's rendered body. Falls
    /// through to plain Tab otherwise, or while a multi-cursor edit is in
    /// flight (same scope limit as the other new editor behaviors above).
    fn try_expand_snippet_on_tab(&mut self) -> bool {
        if !self.editor_secondary_cursors.is_empty() {
            return false;
        }
        let Some(editor) = &self.editor else {
            return false;
        };
        let (row, col) = crate::editor::cursor_tuple(&editor.textarea);
        let Some(line) = editor.textarea.lines().get(row) else {
            return false;
        };
        let chars: Vec<char> = line.chars().collect();
        let upto = &chars[..col.min(chars.len())];
        let trigger_start = upto
            .iter()
            .rposition(|c| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let trigger: String = upto[trigger_start..].iter().collect();
        if trigger.is_empty() {
            return false;
        }
        let Some(cmd) = slash_menu::all_commands(&self.config)
            .into_iter()
            .find(|c| c.trigger.eq_ignore_ascii_case(&trigger))
        else {
            return false;
        };
        let (before, cursor_marker) = self.render_snippet_template(&cmd.body);
        let Some(editor) = &mut self.editor else {
            return false;
        };
        editor.textarea.cancel_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(
                row as u16,
                trigger_start as u16,
            ));
        editor.textarea.start_selection();
        editor
            .textarea
            .move_cursor(ratatui_textarea::CursorMove::Jump(row as u16, col as u16));
        let mut edits = 0usize;
        if editor.textarea.cut() {
            edits += 1;
        }
        let inserted = match &cursor_marker {
            Some(after) => editor.textarea.insert_str(format!("{before}{after}")),
            None => editor.textarea.insert_str(&before),
        };
        if inserted {
            edits += 1;
        }
        if cursor_marker.is_some() {
            let target_row = row + before.matches('\n').count();
            let target_col = before.rsplit('\n').next().unwrap_or("").chars().count();
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(
                    target_row as u16,
                    target_col as u16,
                ));
        }
        if edits > 0 {
            self.editor_undo_groups.push(edits);
            self.editor_redo_groups.clear();
        }
        true
    }
    /// Runs the chosen `/`-menu entry: deletes the typed `/query` (the
    /// same range `slash_query` reads, from the start of the line up to
    /// the cursor — `delete_line_by_head` is exactly that operation) and
    /// inserts the command's body in its place, after substituting
    /// `{{title}}`/`{{date}}` the same way note templates are rendered.
    /// A `{{cursor}}` marker in the body is resolved to an absolute
    /// `CursorMove::Jump` afterward instead of being inserted literally.
    fn apply_slash_command(&mut self, cmd: &slash_menu::SlashCommand) {
        // Same reasoning as `apply_wikilink_selection`: this only edits the
        // primary cursor's `textarea`, and a snippet body can be multi-line
        // — leaving secondary cursors in place after this would desync
        // their row/col against the rows the primary's insertion just
        // shifted, on top of each one still holding its own unconsumed
        // literal "/command" text.
        self.editor_secondary_cursors.clear();
        let (before, cursor_marker) = self.render_snippet_template(&cmd.body);

        let Some(editor) = &mut self.editor else {
            return;
        };
        let (insert_row, _) = crate::editor::cursor_tuple(&editor.textarea);
        editor.textarea.delete_line_by_head();
        match &cursor_marker {
            Some(after) => editor.textarea.insert_str(format!("{before}{after}")),
            None => editor.textarea.insert_str(&before),
        };
        if cursor_marker.is_some() {
            let target_row = insert_row + before.matches('\n').count();
            let target_col = before.rsplit('\n').next().unwrap_or("").chars().count();
            editor
                .textarea
                .move_cursor(ratatui_textarea::CursorMove::Jump(
                    target_row as u16,
                    target_col as u16,
                ));
        }
    }
    fn handle_normal_key(&mut self, key: KeyEvent) {
        if self.leader_pending {
            self.leader_pending = false;
            if key.code != KeyCode::Esc {
                match self.keymaps.resolve_global(key.code) {
                    Some(action) => self.handle_action(action),
                    None => self.set_status("unknown leader shortcut".into()),
                }
            }
            return;
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(self.page_step()),
            KeyCode::PageUp => self.move_selection(-self.page_step()),
            KeyCode::Home => self.jump_to_start(),
            KeyCode::End => self.jump_to_end(),
            KeyCode::Esc if self.mode == Mode::Visual => self.mode = Mode::Normal,
            // Yazi-style: right/l/enter opens (a folder, or one level deeper
            // panel-wise), left/h backs out (up a folder, then a panel) —
            // suspended while `Mode::Visual` is selecting: entering/leaving
            // a folder reloads the underlying list and would strand
            // `visual_anchor` pointing at a completely different one.
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter if self.mode != Mode::Visual => {
                self.navigate_forward()
            }
            KeyCode::Char('h') | KeyCode::Left if self.mode != Mode::Visual => {
                self.navigate_backward()
            }
            KeyCode::Tab if self.mode != Mode::Visual => self.focus = self.focus.next(),
            KeyCode::Char('?') => self.open_which_key(),
            code if self.keymaps.is_quit(code) => self.should_quit = true,
            code if self.keymaps.is_leader(code) => self.leader_pending = true,
            _ => {
                if let Some(action) = self.keymaps.resolve_scoped(self.focus, key.code) {
                    self.handle_action(action);
                } else if let Some(action) = self.keymaps.resolve_scoped(Focus::Notebooks, key.code)
                {
                    // Git actions operate on whichever notebook is
                    // *selected*, not on whichever panel is *focused* — `u`
                    // (push) while reading a note in PREVIEW should still
                    // push that note's notebook instead of silently doing
                    // nothing just because NOTEBOOKS isn't the active panel.
                    // Deliberately NOT NewNotebook/RenameNotebook/
                    // DeleteNotebook here: those share letters with
                    // Notes-scope actions (`a`/`r`/`d`), and falling back to
                    // them from the wrong panel would be a dangerous
                    // accidental-notebook-deletion footgun.
                    if is_notebook_git_action(action) {
                        self.handle_action(action);
                    } else {
                        // Notebook new/rename/delete deliberately isn't
                        // included in the fallback above (see the comment
                        // there) — but silently doing nothing at all reads
                        // as a dead key, indistinguishable from one that's
                        // just unbound in this scope. Say so instead.
                        self.set_status(
                            "no action bound here — switch to NOTEBOOKS to manage notebooks"
                                .to_string(),
                        );
                    }
                }
            }
        }
    }
    pub fn on_key(&mut self, key: KeyEvent) {
        // Checked first, ahead of every other modal: a revert confirmation
        // can be opened *from inside* the history modal (confirm-over-modal),
        // and confirm must intercept `y`/`n` in that case rather than the
        // modal underneath it swallowing the keypress first.
        if self.confirm.is_some() {
            self.handle_confirm_key(key);
            return;
        }
        if self.show_which_key {
            self.handle_which_key_key(key);
            return;
        }
        if self.show_tags {
            self.handle_tags_key(key);
            return;
        }
        if self.show_theme_picker {
            self.handle_theme_picker_key(key);
            return;
        }
        if self.show_template_picker {
            self.handle_template_picker_key(key);
            return;
        }
        if self.show_global_search {
            self.handle_global_search_key(key);
            return;
        }
        if self.show_logs {
            self.handle_logs_key(key);
            return;
        }
        if self.show_update {
            self.handle_update_key(key);
            return;
        }
        if self.show_tree {
            self.handle_tree_key(key);
            return;
        }
        if self.show_links {
            self.handle_links_key(key);
            return;
        }
        if self.show_tasks {
            self.handle_tasks_key(key);
            return;
        }
        if self.show_query {
            self.handle_query_key(key);
            return;
        }
        if self.show_history {
            self.handle_history_key(key);
            return;
        }
        if self.show_conflicts {
            self.handle_conflicts_key(key);
            return;
        }
        if self.show_outline {
            self.handle_outline_key(key);
            return;
        }
        if self.show_drawer {
            self.handle_drawer_key(key);
            return;
        }
        if self.show_settings {
            self.handle_settings_key(key);
            return;
        }
        if self.show_metadata {
            self.handle_metadata_key(key);
            return;
        }
        match self.mode {
            Mode::Insert => self.handle_insert_key(key),
            Mode::Edit => self.handle_edit_key(key),
            Mode::Normal | Mode::Visual => self.handle_normal_key(key),
        }
    }
}

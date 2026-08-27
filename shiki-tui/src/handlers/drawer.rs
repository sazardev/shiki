use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, PendingInput};

impl App {
    /// Unlike `open_logs`/`open_tree` (one-directional, closed via `Esc`
    /// inside their own key handler), the drawer is a true toggle — pressing
    /// its leader binding again collapses it, matching how it was asked for
    /// ("abrir o descolapsar" with the same key).
    pub(crate) fn toggle_drawer(&mut self) {
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
    pub(crate) fn toggle_zen_mode(&mut self) {
        self.zen_mode = !self.zen_mode;
        self.set_status(format!(
            "zen mode: {}",
            if self.zen_mode { "on" } else { "off" }
        ));
    }

    pub(crate) fn handle_drawer_key(&mut self, key: KeyEvent) {
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
    pub(crate) fn jump_to_drawer_notebook(&mut self) {
        if let Some((name, _)) = self.drawer_statuses.get(self.drawer_selected) {
            if let Some(idx) = self.notebooks.iter().position(|nb| &nb.name == name) {
                self.set_selected_notebook(idx);
                self.notes_path.clear();
                self.reload_notes();
            }
        }
        self.show_drawer = false;
    }
}

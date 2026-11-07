use crossterm::event::{KeyCode, KeyEvent};
use shiki_config::Config;

use crate::app::{shift, App};

impl App {
    pub(crate) fn open_theme_picker(&mut self) {
        self.theme_search.clear();
        // The list is displayed grouped by family, so position the cursor on
        // the current theme's row within that grouped order rather than its
        // `theme_index` in the flat `all()` list. Use the *configured* base
        // name, not `self.theme.name`: under `name = "auto"` the resolved
        // palette changes with the terminal scheme, while the picker's cursor
        // should stay on the `auto` row.
        let notebook = self.selected_notebook().map(|nb| nb.name.as_str());
        let active_name = self.config.theme_name_for(notebook).to_string();
        self.theme_picker_index = self
            .theme_picker_filtered()
            .iter()
            .position(|t| t.name == active_name)
            .unwrap_or(0);
        self.show_theme_picker = true;
    }

    /// Themes matching the search query (case-insensitive on the name *or*
    /// the family, so "hack"/"pok"/"lol" narrow by group too), ordered for
    /// grouped display: family first, then name — all of them while the
    /// query is empty, `default` (System) naturally landing last. Backs both
    /// the picker's rendering and its key handling, so navigation and
    /// selection always operate on exactly what's on screen, same pattern as
    /// `which_key_filtered_entries`.
    pub fn theme_picker_filtered(&self) -> Vec<shiki_config::theme::Theme> {
        let query = self.theme_search.value.to_lowercase();
        let mut list: Vec<shiki_config::theme::Theme> = self
            .available_themes
            .iter()
            .filter(|t| {
                query.is_empty()
                    || t.name.to_lowercase().contains(&query)
                    || t.family.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        list.sort_by(|a, b| {
            a.family
                .to_lowercase()
                .cmp(&b.family.to_lowercase())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        list
    }

    pub(crate) fn handle_theme_picker_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                // First Esc clears the filter (keeping the picker open and
                // the theme you're browsing applied); only a second Esc on an
                // already-empty query cancels: revert the live preview back
                // to the theme that was active.
                if !self.theme_search.value.is_empty() {
                    self.theme_search.clear();
                    self.theme_picker_index = 0;
                    self.preview_theme_at(0);
                    return;
                }
                if let Some(t) = self.available_themes.get(self.theme_index) {
                    self.theme = t.clone();
                }
                self.close_theme_picker();
            }
            KeyCode::Enter => {
                // With no filter match under the cursor there's nothing to
                // select — don't "commit" the theme that happened to be
                // live-previewed before the query narrowed to nothing.
                let filtered = self.theme_picker_filtered();
                let Some(t) = filtered.get(self.theme_picker_index) else {
                    return;
                };
                let picked = t.clone();
                // Only reset overrides when actually switching to a
                // different base theme — compared against the *configured*
                // value (the last theme actually saved for the current
                // notebook, or the global `name`), not `self.theme.name`
                // (the live-preview value while browsing). `auto` counts as
                // its own base name even though `self.theme` currently holds
                // the resolved light/dark palette.
                let committed_base = self
                    .config
                    .theme_name_for(self.selected_notebook().map(|nb| nb.name.as_str()))
                    .to_string();
                let base_changed = committed_base != picked.name;
                // A focused notebook gets its own `[notebooks.<name>]`
                // entry (base name and this notebook's own color overrides
                // reset together, since they're this notebook's state) —
                // not the legacy `[theme.notebooks]` map, which nothing
                // writes to anymore. With no notebook selected, the global
                // `name`/`overrides` change instead.
                let scope = match self.selected_notebook() {
                    Some(nb) => {
                        let name = nb.name.clone();
                        let over = self.config.notebooks.entry(name.clone()).or_default();
                        over.theme_name = Some(picked.name.clone());
                        if base_changed {
                            over.theme_overrides = Default::default();
                        }
                        format!(" for notebook '{name}'")
                    }
                    None => {
                        self.config.theme.name = picked.name.clone();
                        if base_changed {
                            self.config.theme.overrides = Default::default();
                        }
                        String::new()
                    }
                };
                if let Ok(path) = Config::default_path() {
                    let _ = self.config.save(&path);
                }
                // Re-resolve instead of keeping `picked` verbatim: for
                // `"auto"` the palette depends on the live terminal scheme,
                // and this also re-derives `theme_index` for the concrete
                // palette that will actually render.
                self.refresh_theme_for_selected_notebook();
                self.set_status(format!("theme: {}{scope}", self.theme.name));
                self.close_theme_picker();
            }
            KeyCode::Char('j') | KeyCode::Down => self.preview_theme_at(1),
            KeyCode::Char('k') | KeyCode::Up => self.preview_theme_at(-1),
            KeyCode::PageDown => self.preview_theme_at(10),
            KeyCode::PageUp => self.preview_theme_at(-10),
            KeyCode::Home => self.select_theme_picker(0),
            KeyCode::End => self.select_theme_picker(usize::MAX),
            KeyCode::Backspace => {
                if !self.theme_search.value.is_empty() {
                    self.theme_search.backspace();
                    self.theme_picker_index = 0;
                    self.preview_theme_at(0);
                }
            }
            KeyCode::Char(c) => {
                self.theme_search.push(c);
                self.theme_picker_index = 0;
                self.preview_theme_at(0);
            }
            _ => {}
        }
    }

    /// Shared by both the picker's cancel and confirm paths — reopens
    /// Settings only when this picker was opened *from* Settings (THEME's
    /// `name` row setting `reopen_settings_after_theme_picker` first), never
    /// for the normal standalone leader+`c` picker.
    pub(crate) fn close_theme_picker(&mut self) {
        self.show_theme_picker = false;
        self.theme_search.clear();
        // The terminal scheme may have changed while the picker was open (its
        // live preview intentionally wins over the background watch); now
        // that the preview is over, resolve the committed config again
        // against the current scheme so `auto` never closes on a stale half.
        self.refresh_theme_for_selected_notebook();
        if self.reopen_settings_after_theme_picker {
            self.reopen_settings_after_theme_picker = false;
            self.show_settings = true;
        }
    }

    /// Moves the picker cursor and immediately applies that theme so the
    /// whole UI re-themes live while browsing, before you've committed to it.
    /// Operates on the filtered list (`theme_picker_filtered`), so j/k/PageUp/
    /// PageDown all move within whatever the current search matches.
    pub(crate) fn preview_theme_at(&mut self, delta: isize) {
        let filtered = self.theme_picker_filtered();
        if filtered.is_empty() {
            return;
        }
        self.theme_picker_index = shift(self.theme_picker_index, delta, filtered.len());
        if let Some(t) = filtered.get(self.theme_picker_index).cloned() {
            self.preview_theme(t);
        }
    }

    /// Absolute jump (Home/End) to a row of the filtered list — clamps to the
    /// last match rather than wrapping, so End with a query lands on the
    /// final match, not the last theme overall.
    pub(crate) fn select_theme_picker(&mut self, idx: usize) {
        let filtered = self.theme_picker_filtered();
        if filtered.is_empty() {
            return;
        }
        self.theme_picker_index = idx.min(filtered.len() - 1);
        if let Some(t) = filtered.get(self.theme_picker_index).cloned() {
            self.preview_theme(t);
        }
    }

    /// Applies a picker row as the live preview. The synthetic `auto` entry
    /// is a marker, not a palette: preview what its configured dark/light
    /// half would actually look like right now, so browsing onto `auto`
    /// doesn't flash the terminal-default stand-in before Enter.
    fn preview_theme(&mut self, theme: shiki_config::Theme) {
        if theme.name == "auto" {
            let notebook = self.selected_notebook().map(|nb| nb.name.as_str());
            self.theme = self.config.theme_for_scheme(notebook, self.terminal_scheme);
        } else {
            self.theme = theme;
        }
    }
}

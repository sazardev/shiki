//! IPC commands exposed to the desktop UI (`shiki-desktop/ui`).
//!
//! F0 scope: three read-only commands proving the whole stack end to end —
//! config loads from the real `config.toml`, notebooks list straight from
//! disk, and a theme resolves to CSS custom properties (the pipeline every
//! future screen will hang off). No editing, no git, no search yet.
//!
//! The load path mirrors `shiki-cli`'s `Context::load()` on purpose: one
//! place decides how config + data dir + custom notebook paths combine, and
//! the desktop app must agree with it or notes would land in different
//! places depending on which frontend you opened.

use std::sync::Mutex;

use serde::Serialize;
use shiki_config::themes;
use shiki_config::{Config, Theme};
use shiki_core::NotebookStore;

/// Everything the UI needs, resolved once at startup.
pub struct AppState {
    /// `Mutex`-wrapped rather than a plain `Config`: several commands
    /// (`set_theme`, `toggle_favorite_editor`, `save_full_config`) persist a
    /// change to `config.toml` on disk — if `state.config` itself stayed a
    /// stale in-memory copy from process startup, `get_config`/
    /// `get_theme_css`'s fallback would keep serving the *old* value for the
    /// rest of the process's life. That's exactly what happened before this
    /// was a `Mutex`: picking a theme wrote the file correctly, but any
    /// webview reload that didn't also restart the Rust process (routine in
    /// `tauri dev`) re-read `state.config.theme.name` and showed the old
    /// theme again — looking indistinguishable from "the selection doesn't
    /// persist" even though the disk write always succeeded.
    pub config: Mutex<Config>,
    /// `Mutex`-wrapped for the same reason `config` is: cloning/importing a
    /// notebook (`create_notebook_from_url`/`adopt_notebook_folder`) needs to
    /// register a brand-new entry in `custom_paths` so it shows up in
    /// `list_notebooks` immediately, without waiting for a full app restart
    /// to re-derive it from `config.toml`.
    pub store: Mutex<NotebookStore>,
    /// Why `config.toml` couldn't be used, if it couldn't. The window still
    /// opens (same spirit as `shiki doctor`: a broken config diagnoses, not
    /// crashes) — F0 surfaces this as a banner in the UI.
    pub load_error: Option<String>,
}

impl AppState {
    pub fn load() -> Self {
        match Self::try_load() {
            Ok((config, store)) => Self {
                config: Mutex::new(config),
                store: Mutex::new(store),
                load_error: None,
            },
            Err(e) => Self {
                config: Mutex::new(Config::default()),
                store: Mutex::new(NotebookStore::new(
                    std::env::temp_dir().join("shiki-unavailable"),
                )),
                load_error: Some(e),
            },
        }
    }

    fn try_load() -> Result<(Config, NotebookStore), String> {
        let config_path = Config::default_path().map_err(|e| e.to_string())?;
        let config = Config::load_or_init(&config_path).map_err(|e| e.to_string())?;
        let data_dir = match config.general.data_dir.as_ref() {
            Some(dir) => std::path::PathBuf::from(dir),
            None => Config::default_data_dir().map_err(|_| {
                "could not determine a data directory — set general.data_dir in config.toml"
                    .to_string()
            })?,
        };
        let store = NotebookStore::new_with_custom_paths(data_dir, config.notebook_custom_paths());
        Ok((config, store))
    }

    /// Cheap read snapshot — `Config` is small and `Clone`, so cloning out
    /// of the lock is simpler than threading a `MutexGuard` borrow through
    /// every call site (several of which need an owned value anyway, e.g.
    /// across another `Result`-returning call).
    pub fn config(&self) -> Config {
        self.config.lock().unwrap().clone()
    }

    /// The single place a config change is allowed to happen: mutates the
    /// shared in-memory `Config` and persists it to disk under the same
    /// lock, so no caller can update one without the other.
    pub fn update_config(&self, f: impl FnOnce(&mut Config)) -> Result<(), String> {
        let path = Config::default_path().map_err(|e| e.to_string())?;
        let mut guard = self.config.lock().unwrap();
        f(&mut guard);
        guard.save(&path).map_err(|e| e.to_string())
    }

    /// Cheap read snapshot, same reasoning as `config()`.
    pub fn store(&self) -> NotebookStore {
        self.store.lock().unwrap().clone()
    }

    /// Registers a notebook at a custom absolute path — both in the live
    /// `NotebookStore` (so it shows up in `list_notebooks` without an app
    /// restart) and in `config.toml`'s `[notebooks.<name>] path = "..."` (so
    /// it's still there next launch). Mirrors shiki-tui's
    /// `App::finish_notebook_adopt`.
    pub fn register_custom_path(
        &self,
        name: String,
        path: std::path::PathBuf,
    ) -> Result<(), String> {
        self.store
            .lock()
            .unwrap()
            .custom_paths
            .insert(name.clone(), path.clone());
        self.update_config(|c| {
            c.notebooks.entry(name).or_default().path = Some(path.to_string_lossy().to_string());
        })
    }
}

/// One row of the NOTEBOOKS sidebar. `encrypted` comes from the config's
/// `[notebooks.<name>] encrypt = true` flag (the same source the CLI uses),
/// not from the in-memory crypto state, which only exists after a unlock.
#[derive(Debug, Serialize)]
pub struct NotebookInfo {
    pub name: String,
    pub path: String,
    pub encrypted: bool,
}

#[tauri::command]
pub fn get_config(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    serde_json::to_value(state.config()).map_err(|e| e.to_string())
}

/// Settings screen's save path — the frontend reads the whole config once
/// via `get_config`, lets the user edit fields on that in-memory copy
/// across every tab, then resubmits the whole object here rather than
/// exposing one Rust command per field (General alone has 26). Round-trips
/// through `Config` (not written as raw JSON) so a value that doesn't
/// deserialize cleanly is rejected with a clear error instead of writing a
/// config.toml the app itself can't load next launch.
#[tauri::command]
pub fn save_full_config(
    state: tauri::State<'_, AppState>,
    config: serde_json::Value,
) -> Result<(), String> {
    let parsed: Config = serde_json::from_value(config).map_err(|e| e.to_string())?;
    state.update_config(|c| *c = parsed)
}

/// Mirrors the TUI footer's `env!("CARGO_PKG_VERSION")` — same single
/// workspace version (see CLAUDE.md's Versioning section), just read from
/// shiki-desktop's own inherited manifest instead of shiki-tui's.
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub fn rename_notebook(
    state: tauri::State<'_, AppState>,
    old_name: String,
    new_name: String,
) -> Result<(), String> {
    state
        .store()
        .rename(&old_name, &new_name)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_notebook(state: tauri::State<'_, AppState>, name: String) -> Result<(), String> {
    state.store().delete(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_notebooks(state: tauri::State<'_, AppState>) -> Result<Vec<NotebookInfo>, String> {
    let mut out = Vec::new();
    let cfg = state.config();
    for nb in state.store().list().map_err(|e| e.to_string())? {
        out.push(NotebookInfo {
            name: nb.name.clone(),
            path: nb.path.display().to_string(),
            encrypted: cfg.encrypt_for(&nb.name),
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn get_theme_css(
    state: tauri::State<'_, AppState>,
    name: Option<String>,
) -> Result<String, String> {
    Ok(theme_to_css(&resolve_theme(
        &state.config().theme.name,
        name.as_deref(),
    )))
}

#[derive(Debug, Serialize)]
pub struct ThemeInfo {
    pub name: String,
    pub family: String,
}

/// Every built-in theme, for the theme picker (leader+c) — `get_theme_css`
/// already accepts an explicit `name` for live preview, so the picker just
/// needs the list of names/families to render and filter.
#[tauri::command]
pub fn list_themes() -> Vec<ThemeInfo> {
    themes::all()
        .into_iter()
        .map(|t| ThemeInfo {
            name: t.name,
            family: t.family.to_string(),
        })
        .collect()
}

/// Persists the picked theme to both `config.toml` and `state.config`
/// (via `update_config`) — see the `AppState.config` doc comment for why
/// both have to change together.
#[tauri::command]
pub fn set_theme(state: tauri::State<'_, AppState>, name: String) -> Result<(), String> {
    state.update_config(|c| c.theme.name = name)
}

#[derive(Debug, Serialize)]
pub struct LogEntryInfo {
    pub at: String,
    pub message: String,
}

/// One line per entry: RFC3339 timestamp, a tab, the message — same format
/// `shiki-tui`'s own `format_log_line`/`parse_log_line` use (duplicated
/// here rather than imported, since shiki-desktop doesn't depend on
/// shiki-tui) so a log file written by one is readable by the other; both
/// write to the same `Config::default_log_path()`.
fn format_log_line(message: &str) -> String {
    format!("{}\t{}\n", chrono::Local::now().to_rfc3339(), message)
}

fn parse_log_line(line: &str) -> Option<LogEntryInfo> {
    let (at, message) = line.split_once('\t')?;
    Some(LogEntryInfo {
        at: at.to_string(),
        message: message.to_string(),
    })
}

#[tauri::command]
pub fn append_log(message: String) -> Result<(), String> {
    let path = Config::default_log_path().map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(format_log_line(&message).as_bytes())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_logs() -> Result<Vec<LogEntryInfo>, String> {
    let path = Config::default_log_path().map_err(|e| e.to_string())?;
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Ok(Vec::new()),
    };
    Ok(content.lines().filter_map(parse_log_line).collect())
}

#[tauri::command]
pub fn clear_logs() -> Result<(), String> {
    let path = Config::default_log_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, "").map_err(|e| e.to_string())?;
    append_log("logs cleared".to_string())
}

/// Flips `general.use_favorite_editor` and persists it to both disk and
/// `state.config`. Returns the new value so the caller doesn't need a
/// second round trip to know it.
#[tauri::command]
pub fn toggle_favorite_editor(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    state.update_config(|c| c.general.use_favorite_editor = !c.general.use_favorite_editor)?;
    Ok(state.config().general.use_favorite_editor)
}

/// Resolves the theme the UI should paint with: an explicit request wins,
/// else the config's active theme; unknown names fall back to the built-in
/// default so a typo'd `theme.name` in config.toml degrades to a working
/// palette instead of an error page.
fn resolve_theme(config_name: &str, requested: Option<&str>) -> Theme {
    let wanted = requested.unwrap_or(config_name);
    themes::by_name(wanted)
        .or_else(|| themes::by_name(config_name))
        .or_else(|| themes::by_name("gruvbox-dark"))
        .expect("built-in fallback themes always exist")
}

/// Renders a theme as a `:root { --var: #hex; ... }` block. shiki-config's
/// colors are already hex strings (it stays ratatui-free by design), so the
/// web side consumes them verbatim — the same guarantee the TUI's
/// `hex_to_color` conversion sits behind, mirrored here for CSS.
fn theme_to_css(theme: &Theme) -> String {
    let pairs = [
        ("bg", &theme.bg),
        ("fg", &theme.fg),
        ("accent", &theme.accent),
        ("selection", &theme.selection),
        ("border", &theme.border),
        ("statusbar", &theme.statusbar),
        ("highlight", &theme.highlight),
        ("error", &theme.error),
        ("warning", &theme.warning),
        ("success", &theme.success),
        ("inactive", &theme.inactive),
        ("scrollbar", &theme.scrollbar),
        ("tab-active", &theme.tab_active),
        ("tab-inactive", &theme.tab_inactive),
        ("panel-title", &theme.panel_title),
        ("cursor", &theme.cursor),
        ("link", &theme.link),
        ("tag", &theme.tag),
        ("muted", &theme.muted),
    ];
    let body: String = pairs.iter().map(|(k, v)| format!("--{k}:{v};")).collect();
    format!(":root{{{body}}}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_to_css_exposes_every_palette_slot_as_a_var() {
        let theme = resolve_theme("catppuccin-mocha", None);
        let css = theme_to_css(&theme);
        assert!(css.starts_with(":root{"));
        assert!(css.ends_with('}'));
        // Every slot the UI is allowed to consume — a missing var would
        // silently fall back to browser defaults and look "almost right".
        for var in [
            "--bg:",
            "--fg:",
            "--accent:",
            "--selection:",
            "--border:",
            "--statusbar:",
            "--highlight:",
            "--error:",
            "--warning:",
            "--success:",
            "--inactive:",
            "--scrollbar:",
            "--tab-active:",
            "--tab-inactive:",
            "--panel-title:",
            "--cursor:",
            "--link:",
            "--tag:",
            "--muted:",
        ] {
            assert!(css.contains(var), "missing {var} in: {css}");
        }
    }

    #[test]
    fn theme_to_css_carries_real_hex_values_through_unchanged() {
        let css = theme_to_css(&resolve_theme("catppuccin-mocha", None));
        assert!(
            css.contains("--bg:#1e1e2e;"),
            "catppuccin-mocha bg should pass through verbatim: {css}"
        );
    }

    #[test]
    fn unknown_requested_theme_falls_back_to_config_then_builtin() {
        // Typo'd request → config's own name → gruvbox-dark builtin.
        let t = resolve_theme("nord", Some("no-such-theme"));
        assert_eq!(t.name, "nord");
        // Both broken → last-resort builtin keeps the window usable.
        let t = resolve_theme("also-nope", Some("still-nope"));
        assert_eq!(t.name, "gruvbox-dark");
    }
}

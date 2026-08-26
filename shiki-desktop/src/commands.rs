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

use serde::Serialize;
use shiki_config::themes;
use shiki_config::{Config, Theme};
use shiki_core::NotebookStore;

/// Everything the UI needs, resolved once at startup.
pub struct AppState {
    pub config: Config,
    pub store: NotebookStore,
    /// Why `config.toml` couldn't be used, if it couldn't. The window still
    /// opens (same spirit as `shiki doctor`: a broken config diagnoses, not
    /// crashes) — F0 surfaces this as a banner in the UI.
    pub load_error: Option<String>,
}

impl AppState {
    pub fn load() -> Self {
        match Self::try_load() {
            Ok((config, store)) => Self {
                config,
                store,
                load_error: None,
            },
            Err(e) => Self {
                config: Config::default(),
                store: NotebookStore::new(std::env::temp_dir().join("shiki-unavailable")),
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
    serde_json::to_value(&state.config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_notebooks(state: tauri::State<'_, AppState>) -> Result<Vec<NotebookInfo>, String> {
    let mut out = Vec::new();
    for nb in state.store.list().map_err(|e| e.to_string())? {
        out.push(NotebookInfo {
            name: nb.name.clone(),
            path: nb.path.display().to_string(),
            encrypted: state.config.encrypt_for(&nb.name),
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
        &state.config.theme.name,
        name.as_deref(),
    )))
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

//! Spell-check IPC commands — a thin wrapper over `shiki_core::spell`
//! (hunspell shelled out, same as the TUI's own Ctrl+E pass), reading
//! `[editor] spellcheck_lang` from the already-loaded `AppState.config` so
//! the frontend doesn't have to pass it on every call.

use serde::Serialize;

use crate::commands::AppState;

#[derive(Debug, Serialize)]
pub struct MisspellDto {
    pub word: String,
    pub start: usize,
    pub end: usize,
}

fn lang(state: &AppState) -> Option<String> {
    let cfg = state.config();
    let lang = cfg.editor.spellcheck_lang.trim();
    (!lang.is_empty()).then(|| lang.to_string())
}

#[tauri::command]
pub fn spell_available() -> bool {
    shiki_core::spell::hunspell_available()
}

#[tauri::command]
pub fn spell_check(
    state: tauri::State<'_, AppState>,
    text: String,
) -> Result<Vec<MisspellDto>, String> {
    let misspells =
        shiki_core::spell::check_text(&text, lang(&state).as_deref()).map_err(|e| e.to_string())?;
    Ok(misspells
        .into_iter()
        .map(|m| MisspellDto {
            word: m.word,
            start: m.start,
            end: m.end,
        })
        .collect())
}

#[tauri::command]
pub fn spell_suggestions(state: tauri::State<'_, AppState>, word: String) -> Vec<String> {
    shiki_core::spell::suggestions(&word, lang(&state).as_deref())
}

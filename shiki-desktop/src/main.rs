//! shiki Desktop — GUI companion to the terminal app.
//!
//! Same logic crate (`shiki-core`) and same config (`shiki-config`) as the
//! TUI and CLI; this binary only adds a window and an IPC surface over them.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod notes;

fn main() {
    let app_state = commands::AppState::load();
    if let Some(err) = &app_state.load_error {
        eprintln!("shiki-desktop: config problem (window will still open): {err}");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::list_notebooks,
            commands::get_theme_css,
            notes::list_notes,
            notes::read_note,
            notes::save_note,
            notes::create_note,
            notes::rename_note,
            notes::delete_note,
            notes::create_notebook,
            notes::daily_note,
            notes::search_notes,
            notes::render_note,
            notes::git_status,
            notes::git_commit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running shiki desktop");
}

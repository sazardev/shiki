//! shiki Desktop — GUI companion to the terminal app (F0 scaffold).
//!
//! Same logic crate (`shiki-core`) and same config (`shiki-config`) as the
//! TUI and CLI; this binary only adds a window and an IPC surface over them.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running shiki desktop");
}

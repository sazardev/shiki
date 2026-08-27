//! shiki Desktop — GUI companion to the terminal app.
//!
//! Same logic crate (`shiki-core`) and same config (`shiki-config`) as the
//! TUI and CLI; this binary only adds a window and an IPC surface over them.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod attachments;
mod commands;
mod notes;
mod spell;

fn main() {
    let app_state = commands::AppState::load();
    if let Some(err) = &app_state.load_error {
        eprintln!("shiki-desktop: config problem (window will still open): {err}");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_full_config,
            commands::list_notebooks,
            commands::rename_notebook,
            commands::delete_notebook,
            commands::get_theme_css,
            commands::get_app_version,
            commands::list_themes,
            commands::set_theme,
            commands::toggle_favorite_editor,
            commands::append_log,
            commands::read_logs,
            commands::clear_logs,
            notes::list_notes,
            notes::read_note,
            notes::save_note,
            notes::create_note,
            notes::rename_note,
            notes::delete_note,
            notes::create_notebook,
            notes::create_notebook_from_url,
            notes::adopt_notebook_folder,
            notes::daily_note,
            notes::search_notes,
            notes::render_note,
            notes::git_status,
            notes::git_commit,
            notes::pull_notebook,
            notes::pull_all_notebooks,
            notes::set_notebook_remote,
            notes::list_tasks,
            notes::toggle_task,
            notes::get_links,
            notes::note_history,
            notes::revert_note,
            notes::undo_delete_note,
            notes::create_folder,
            notes::move_note,
            notes::copy_note,
            notes::run_note_query,
            notes::export_notebook,
            notes::publish_notebook,
            notes::open_external_editor,
            notes::open_favorite_editor,
            notes::working_diff,
            notes::notebook_tree,
            spell::spell_available,
            spell::spell_check,
            spell::spell_suggestions,
            attachments::save_pasted_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running shiki desktop");
}

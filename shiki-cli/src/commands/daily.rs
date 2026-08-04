use std::path::Path;

use anyhow::{Context, Result};
use shiki_core::NotebookStore;

use super::open_in_editor;

pub fn run(
    store: &NotebookStore,
    notebook: &str,
    templates_dir: &Path,
    editor: &str,
    daily_template: &str,
) -> Result<()> {
    let nb = match store.get(notebook) {
        Ok(nb) => nb,
        Err(_) => store
            .create(notebook)
            .with_context(|| format!("could not create notebook '{notebook}'"))?,
    };
    let today = chrono::Local::now().date_naive();
    // Same agenda injection as the TUI's `t` — today's due/overdue tasks
    // across every notebook, only when the daily is newly created.
    let agenda = store
        .all_notes()
        .ok()
        .and_then(|pool| shiki_core::tasks::agenda_section(&pool, today));
    let note = shiki_core::daily::create_or_open(
        &nb,
        today,
        templates_dir,
        daily_template,
        agenda.as_deref(),
    )?;
    println!("daily: {}", note.path.display());
    open_in_editor(editor, &note.path)
}

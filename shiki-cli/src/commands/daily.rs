use std::path::Path;

use anyhow::{Context, Result};
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{open_in_editor, unlock_if_encrypted};

pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    templates_dir: &Path,
    editor: &str,
    daily_template: &str,
    daily_agenda: bool,
) -> Result<()> {
    let nb = match store.get(notebook) {
        Ok(nb) => nb,
        Err(_) => store
            .create(notebook)
            .with_context(|| format!("could not create notebook '{notebook}'"))?,
    };
    let nb = unlock_if_encrypted(config, nb)?;
    let today = chrono::Local::now().date_naive();
    // Same agenda injection as the TUI's `t` — today's due/overdue tasks
    // across every notebook, only when the daily is newly created, and only
    // when [general].daily_agenda is on.
    let agenda = daily_agenda
        .then(|| {
            store
                .all_notes()
                .ok()
                .and_then(|pool| shiki_core::tasks::agenda_section(&pool, today))
        })
        .flatten();
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

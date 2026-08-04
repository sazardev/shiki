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
    let note = shiki_core::daily::create_or_open(&nb, today, templates_dir, daily_template)?;
    println!("daily: {}", note.path.display());
    open_in_editor(editor, &note.path)
}

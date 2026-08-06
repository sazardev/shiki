use anyhow::{Context, Result};
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{open_in_editor, unlock_if_encrypted};

/// Creates a note. When `body` is `Some` (from `--body`/`--stdin`), the note
/// is written with that content and `$EDITOR` is never spawned — the
/// non-interactive path needed for scripting/automation (e.g. another
/// program piping generated content in). `tags` is applied either way; an
/// empty slice is a no-op, so the interactive path (still the default) is
/// unaffected when neither flag is passed.
pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    title: &str,
    editor: &str,
    body: Option<&str>,
    tags: &[String],
) -> Result<()> {
    let nb = match store.get(notebook) {
        Ok(nb) => nb,
        Err(_) => store
            .create(notebook)
            .with_context(|| format!("could not create notebook '{notebook}'"))?,
    };
    let nb = unlock_if_encrypted(config, nb)?;
    let mut note = nb.create_note(title, body.unwrap_or(""))?;
    println!("created: {}", note.path.display());

    if !tags.is_empty() {
        note.frontmatter.tags = tags.to_vec();
        note.save_with_crypto(nb.crypto.as_ref())?;
    }

    if body.is_some() {
        return Ok(());
    }
    open_in_editor(editor, &note.path)
}

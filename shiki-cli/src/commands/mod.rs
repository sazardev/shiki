pub mod capture;
pub mod config;
pub mod daemon;
pub mod daily;
pub mod diff;
pub mod doctor;
pub mod edit;
pub mod export;
pub mod extension;
pub mod graph;
pub mod import;
pub mod list;
pub mod log;
pub mod new;
pub mod notebook;
pub mod publish;
pub mod query;
pub mod search;
pub mod show;
pub mod sync;
pub mod tasks;
pub mod theme;

use anyhow::{Context, Result};
use shiki_core::{Note, NotebookStore};

/// Resolves a note by slug or by (case-insensitive) title match within a
/// notebook — searched recursively across every folder, so two notes with
/// the same title/slug in different folders both match `needle`. Errors
/// with a clear disambiguation message (listing each match's folder) rather
/// than silently returning whichever one the recursive walk happened to
/// find first — a real ambiguity risk that root-only lookup never had
/// before `all_notes_recursive` replaced it.
pub fn find_note(store: &NotebookStore, notebook: &str, needle: &str) -> Result<Note> {
    let nb = store.get(notebook).with_context(|| {
        format!("notebook '{notebook}' not found \u{2014} see `shiki notebook list`")
    })?;
    let notes = nb.all_notes_recursive()?;
    let slug = shiki_core::note::Note::slugify(needle);
    let mut matches: Vec<Note> = notes
        .into_iter()
        .filter(|n| n.file_stem() == slug || n.frontmatter.title.eq_ignore_ascii_case(needle))
        .collect();
    match matches.len() {
        0 => anyhow::bail!("note '{needle}' not found in '{notebook}'"),
        1 => Ok(matches.remove(0)),
        _ => {
            let mut locations: Vec<String> = matches
                .iter()
                .map(|n| {
                    n.path
                        .strip_prefix(&nb.path)
                        .unwrap_or(&n.path)
                        .display()
                        .to_string()
                })
                .collect();
            locations.sort();
            anyhow::bail!(
                "'{needle}' matches {} notes in '{notebook}' \u{2014} be more specific: {}",
                matches.len(),
                locations.join(", ")
            )
        }
    }
}

/// Attaches this session's passphrase to `nb` if it's configured as
/// encrypted — prompted interactively (hidden input), never cached or
/// stored anywhere beyond the lifetime of this one CLI invocation. A
/// plaintext notebook passes through untouched, no prompt at all.
pub fn unlock_if_encrypted(
    config: &shiki_config::Config,
    nb: shiki_core::Notebook,
) -> Result<shiki_core::Notebook> {
    if !config.encrypt_for(&nb.name) {
        return Ok(nb);
    }
    let passphrase = rpassword::prompt_password(format!("Passphrase for '{}': ", nb.name))?;
    Ok(nb.with_crypto(Some(shiki_core::crypto::NotebookCrypto::new(passphrase))))
}

/// Opens `path` with the configured external editor, waiting for it to finish.
pub fn open_in_editor(editor: &str, path: &std::path::Path) -> Result<()> {
    let status = shiki_core::editor::command_for(editor, path)
        .status()
        .with_context(|| format!("could not run editor '{editor}'"))?;
    if !status.success() {
        anyhow::bail!("'{editor}' exited with an error");
    }
    Ok(())
}

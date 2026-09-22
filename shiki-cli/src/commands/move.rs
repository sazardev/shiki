use anyhow::Result;
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{find_note, get_notebook, unlock_if_encrypted};

/// `shiki move <note> <destination> -n <nb> [--copy]` — moves (or, with
/// `--copy`, copies) a note to `notebook/path/within/it`, any depth, any
/// notebook — the CLI counterpart of the TUI's `m`/`y`. `destination` is
/// parsed via `shiki_core::notebook::parse_address`: the first segment
/// must already be a real notebook (never auto-created), everything after
/// it is a destination folder, auto-created as needed. Both the source and
/// destination notebooks are unlocked independently, since a move/copy can
/// cross from a plaintext notebook into an encrypted one or vice versa —
/// `copy_note_to`/`move_note_to` decrypt with the source's key and
/// re-encrypt (or leave plain) with the destination's.
pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    note: &str,
    destination: &str,
    copy: bool,
    json: bool,
) -> Result<()> {
    let source_nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let source_note = find_note(&source_nb, note)?;
    let (dest_notebook, dest_relative) = shiki_core::notebook::parse_address(store, destination)?;
    let dest_notebook = unlock_if_encrypted(config, dest_notebook)?;

    let result = if copy {
        source_nb.copy_note_to(&source_note.path, &dest_notebook, &dest_relative)?
    } else {
        source_nb.move_note_to(&source_note.path, &dest_notebook, &dest_relative)?
    };

    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "source_path": source_note.path,
                "dest_path": result.path,
                "copied": copy,
            }))?
        );
    } else {
        let verb = if copy { "copied" } else { "moved" };
        println!(
            "{verb}: {} -> {}",
            source_note.path.display(),
            result.path.display()
        );
    }
    Ok(())
}

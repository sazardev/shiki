use anyhow::Result;
use chrono::Local;
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{find_note, get_notebook, unlock_if_encrypted};

/// Deletes a note — trash-first, same as the TUI's `d` on a note
/// (`shiki-tui/src/key_handlers.rs`'s `DeleteTarget::Note` confirm arm):
/// moved into `{config_dir}/trash/{notebook}/`, tagged with a millisecond
/// timestamp suffix so two deletes of same-named notes can't collide, and
/// only permanently removed via `delete_note_at` if trashing itself fails
/// (an unresolvable trash dir, a permissions error, …) — a delete that was
/// just confirmed with `--yes` should always visibly remove the note; the
/// trash step is a safety net on top of that, not a precondition for it.
/// There's no CLI-level undo for this yet (the TUI's `leader+u` is
/// session-only, in-memory state); recovery means finding the file under
/// the trash directory printed on success.
pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    note: &str,
    yes: bool,
    json: bool,
) -> Result<()> {
    if !yes {
        anyhow::bail!("this permanently removes the note from its usual location \u{2014} re-run with --yes to confirm");
    }
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let note = find_note(&nb, note)?;

    let suffix = Local::now().timestamp_millis().to_string();
    let trash_path = Config::default_trash_dir().ok().and_then(|root| {
        let trash_dir = root.join(&nb.name);
        shiki_core::trash::move_to_trash(&note.path, &trash_dir, &suffix).ok()
    });
    if trash_path.is_none() {
        nb.delete_note_at(&note.path)?;
    }

    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "path": note.path,
                "trashed": trash_path,
            }))?
        );
    } else {
        match &trash_path {
            Some(t) => println!(
                "deleted: {} (trashed at {})",
                note.path.display(),
                t.display()
            ),
            None => println!("deleted: {}", note.path.display()),
        }
    }
    Ok(())
}

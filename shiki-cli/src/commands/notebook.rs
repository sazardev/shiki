use anyhow::Result;
use shiki_core::NotebookStore;

pub fn create(store: &NotebookStore, name: &str) -> Result<()> {
    let nb = store.create(name)?;
    println!("notebook created: {}", nb.path.display());
    Ok(())
}

pub fn list(store: &NotebookStore) -> Result<()> {
    let notebooks = store.list()?;
    if notebooks.is_empty() {
        println!("(no notebooks)");
        return Ok(());
    }
    for nb in notebooks {
        match nb.all_notes_recursive() {
            Ok(notes) => println!("{}  ({} notes)", nb.name, notes.len()),
            // A failed walk (permissions, I/O) is not the same thing as a
            // genuinely empty notebook — `unwrap_or(0)` used to make the two
            // indistinguishable in this output.
            Err(e) => println!("{}  (error reading notes: {e})", nb.name),
        }
    }
    Ok(())
}

pub fn rename(store: &NotebookStore, old: &str, new: &str) -> Result<()> {
    let nb = store.rename(old, new)?;
    println!("renamed to: {}", nb.path.display());
    Ok(())
}

/// Permanently deletes a notebook and every note in it (`NotebookStore::delete`
/// is a plain `remove_dir_all` — no trash/undo). `yes` must be explicitly
/// passed (`--yes`), mirroring the TUI's own confirm dialog for the same
/// action, rather than deleting on the bare name alone.
pub fn delete(store: &NotebookStore, name: &str, yes: bool) -> Result<()> {
    if !yes {
        anyhow::bail!(
            "this permanently deletes '{name}' and every note in it \u{2014} re-run with --yes \
             to confirm"
        );
    }
    store.delete(name)?;
    println!("deleted: {name}");
    Ok(())
}

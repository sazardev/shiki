use anyhow::Result;
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{get_notebook, unlock_if_encrypted};

/// `shiki folder create <path> -n <nb>` — creates an empty folder inside a
/// notebook, at any depth (`Notebook::create_folder_in`). A note creates
/// its folder as a side effect already (`create_dir_all`), but there was
/// previously no way to get an *empty* folder up front from the CLI —
/// only ones that already existed on disk, or that happened to contain a
/// note, were reachable.
pub fn create(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    path: &str,
    json: bool,
) -> Result<()> {
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let (parent, name) = split_relative(path)?;
    let created = nb.create_folder_in(&parent, &name)?;

    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({ "path": created }))?
        );
    } else {
        println!("created: {}", created.display());
    }
    Ok(())
}

/// `shiki folder delete <path> -n <nb> --yes` — recursive, same
/// irreversible-action `--yes` gate as `shiki delete`/`shiki notebook
/// delete`. Unlike a note delete, this doesn't go through trash
/// (`Notebook::delete_folder_at` is a plain `remove_dir_all`, same as the
/// TUI's own folder delete) — a folder can contain an arbitrary,
/// unbounded number of notes, so trash-then-restore for a whole subtree
/// isn't the same small, obviously-reversible operation a single file
/// move is.
pub fn delete(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    path: &str,
    yes: bool,
    json: bool,
) -> Result<()> {
    if !yes {
        anyhow::bail!(
            "this permanently deletes '{path}' and everything inside it \u{2014} re-run with --yes to confirm"
        );
    }
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let relative = shiki_core::notebook::validate_relative_path(path)?;
    nb.delete_folder_at(&relative)?;

    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({ "path": path, "deleted": true }))?
        );
    } else {
        println!("deleted: {path}");
    }
    Ok(())
}

/// `shiki folder move <path> <destination> -n <nb> [--copy]` — same
/// `notebook/path/within/it` destination address `shiki move` uses
/// (`shiki_core::notebook::parse_address`), just for a whole folder
/// (`copy_folder_to`/`move_folder_to`) instead of a single note.
pub fn move_folder(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    path: &str,
    destination: &str,
    copy: bool,
    json: bool,
) -> Result<()> {
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let relative = shiki_core::notebook::validate_relative_path(path)?;
    let (dest_notebook, dest_relative) = shiki_core::notebook::parse_address(store, destination)?;
    let dest_notebook = unlock_if_encrypted(config, dest_notebook)?;

    if copy {
        nb.copy_folder_to(&relative, &dest_notebook, &dest_relative)?;
    } else {
        nb.move_folder_to(&relative, &dest_notebook, &dest_relative)?;
    }

    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "source": path,
                "destination": destination,
                "copied": copy,
            }))?
        );
    } else {
        let verb = if copy { "copied" } else { "moved" };
        println!("{verb}: {path} -> {destination}");
    }
    Ok(())
}

/// Splits `"work/meetings/2026-planning"` into its parent
/// (`"work/meetings"`, validated segment-by-segment the same way a
/// notebook name is) and its own final component
/// (`"2026-planning"`) — `create_folder_in`'s own two-argument shape.
fn split_relative(path: &str) -> Result<(std::path::PathBuf, String)> {
    let full = shiki_core::notebook::validate_relative_path(path)?;
    let name = full
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("'{path}' has no folder name"))?;
    let parent = full
        .parent()
        .unwrap_or(std::path::Path::new(""))
        .to_path_buf();
    Ok((parent, name))
}

use std::path::Path;

use anyhow::{Context, Result};
use shiki_core::{git, NotebookStore};

use super::find_note;

/// `shiki diff [note]` — pending changes (working tree vs last commit),
/// the same diff the TUI's `d` shows on a dirty note. Without a note,
/// every pending change in the notebook. Encrypted notebooks are refused:
/// both sides of their diffs are ciphertext blobs, so any +/- output would
/// be meaningless noise.
pub fn run(store: &NotebookStore, notebook: &str, note: Option<&str>) -> Result<()> {
    let nb = store.get(notebook).with_context(|| {
        format!("notebook '{notebook}' not found \u{2014} see `shiki notebook list`")
    })?;
    if nb.crypto.is_some() {
        anyhow::bail!("diff isn't available for encrypted notebooks");
    }

    match note {
        Some(note) => {
            let note = find_note(store, notebook, note)?;
            let relative = relative_to(&note.path, &nb.path);
            print_file_diff(
                &relative,
                &git::working_tree_diff(&nb.path, Path::new(&relative))?,
            );
        }
        None => {
            let dirty = git::dirty_files(&nb.path)?;
            if dirty.is_empty() {
                println!("'{notebook}' has no pending changes");
                return Ok(());
            }
            for file in &dirty {
                print_file_diff(
                    file,
                    &git::working_tree_diff(&nb.path, Path::new(file))
                        .with_context(|| format!("could not diff '{file}'"))?,
                );
            }
        }
    }
    Ok(())
}

fn print_file_diff(name: &str, lines: &[git::DiffLine]) {
    if lines.is_empty() {
        return;
    }
    let added = lines.iter().filter(|l| l.origin == '+').count();
    let removed = lines.iter().filter(|l| l.origin == '-').count();
    println!("=== {name} (+{added} -{removed})");
    for line in lines {
        println!("{} {}", line.origin, line.content);
    }
    println!();
}

/// Repo-relative, forward-slash path of a note inside its notebook —
/// matching the shape `statuses`/pathspecs expect on every platform.
fn relative_to(path: &std::path::Path, root: &std::path::Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

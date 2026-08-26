use std::path::Path;

use anyhow::{Context, Result};
use shiki_core::{git, NotebookStore};

use super::find_note;

/// `shiki log [note]` — the notebook's recent commits, or every commit
/// that touched one specific note (the TUI history modal's list, on the
/// command line).
pub fn run(store: &NotebookStore, notebook: &str, note: Option<&str>) -> Result<()> {
    let nb = store.get(notebook).with_context(|| {
        format!("notebook '{notebook}' not found \u{2014} see `shiki notebook list`")
    })?;

    let revisions = match note {
        Some(note) => {
            let note = find_note(store, notebook, note)?;
            let relative = note
                .path
                .strip_prefix(&nb.path)
                .unwrap_or(&note.path)
                .display()
                .to_string()
                .replace('\\', "/");
            let revs = git::file_history(&nb.path, Path::new(&relative))?;
            if revs.is_empty() {
                println!(
                    "no history yet for '{relative}' \u{2014} sync (`shiki sync`) to commit it first"
                );
                return Ok(());
            }
            revs
        }
        None => {
            let revs = git::recent_commits(&nb.path, 20)?;
            if revs.is_empty() {
                println!("'{notebook}' has no commits yet");
                return Ok(());
            }
            println!("last {} commit(s) in '{notebook}'", revs.len());
            revs
        }
    };

    for rev in &revisions {
        let short = rev.commit_id.chars().take(7).collect::<String>();
        println!(
            "{} {} {}",
            short,
            rev.date.format("%Y-%m-%d %H:%M"),
            rev.message
        );
    }
    Ok(())
}

use std::path::Path;

use anyhow::Result;
use shiki_config::Config;
use shiki_core::{git, NotebookStore};

use super::{find_note, get_notebook, page_footer, page_json, paginate, unlock_if_encrypted};

/// `shiki log [note]` — the notebook's recent commits, or every commit
/// that touched one specific note (the TUI history modal's list, on the
/// command line). Only the note-specific branch ever needs a decrypted
/// notebook (to match `note` against real titles) — commit metadata itself
/// is plaintext git history regardless of whether the files it touched are
/// encrypted at rest, so the notebook-wide branch never prompts/unlocks.
/// Both branches fetch the *entire* history unconditionally (git revwalks
/// are cheap even for thousands of commits) and paginate the result the
/// same way every other unbounded command does — so `total`/`has_more` are
/// always exact, never an approximation.
pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    note: Option<&str>,
    json: bool,
    offset: usize,
    limit: Option<usize>,
) -> Result<()> {
    let nb = get_notebook(store, notebook)?;

    let (revisions, empty_message) = match note {
        Some(note) => {
            let unlocked = unlock_if_encrypted(config, nb.clone())?;
            let note = find_note(&unlocked, note)?;
            let relative = note
                .path
                .strip_prefix(&nb.path)
                .unwrap_or(&note.path)
                .display()
                .to_string()
                .replace('\\', "/");
            let revs = git::file_history(&nb.path, Path::new(&relative))?;
            (
                revs,
                format!("no history yet for '{relative}' \u{2014} sync (`shiki sync`) to commit it first"),
            )
        }
        None => {
            let revs = git::recent_commits(&nb.path, usize::MAX)?;
            (revs, format!("'{notebook}' has no commits yet"))
        }
    };
    let (page, total) = paginate(revisions, offset, limit);

    if json {
        let items: Vec<serde_json::Value> = page
            .iter()
            .map(|rev| {
                serde_json::json!({
                    "commit": rev.commit_id,
                    "date": rev.date.to_rfc3339(),
                    "message": rev.message,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string(&page_json(items, total, offset, limit))?
        );
        return Ok(());
    }

    if page.is_empty() {
        if total == 0 {
            println!("{empty_message}");
        } else {
            println!("(nothing at offset {offset} \u{2014} {total} commit(s) total)");
        }
        return Ok(());
    }
    if note.is_none() {
        println!("last {} commit(s) in '{notebook}'", page.len());
    }
    let shown = page.len();
    for rev in &page {
        let short = rev.commit_id.chars().take(7).collect::<String>();
        println!(
            "{} {} {}",
            short,
            rev.date.format("%Y-%m-%d %H:%M"),
            rev.message
        );
    }
    if let Some(footer) = page_footer(shown, offset, total) {
        println!("{footer}");
    }
    Ok(())
}

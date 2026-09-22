use anyhow::Result;
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{find_note, get_notebook, unlock_if_encrypted};

/// Renames a note and, by default, rewrites every inbound `[[wikilink]]`
/// across every notebook the store can see — the non-interactive
/// equivalent of the TUI's rename confirm dialog (`shiki-tui/src/
/// key_handlers.rs`'s `perform_rename_with_link_update`/
/// `perform_rename_note`), which asks "rewrite links too?" only because a
/// human is there to answer; a script isn't, so the safe default (rewrite)
/// is what runs unless `--no-rewrite-links` opts out. Links must be
/// rewritten *before* the actual rename — the old title/slug has to still
/// be current for `rewrite_links_to`'s match to find them.
///
/// One real limitation versus the TUI: `NotebookStore::all_notes` (the
/// link-rewrite pool here) never attaches crypto to any notebook, so a
/// `[[link]]` living inside a *different*, encrypted notebook is silently
/// left untouched — only the notebook actually being renamed into gets
/// unlocked (via `SHIKI_PASSPHRASE`/an interactive prompt). The TUI can do
/// better here because it reuses whichever notebooks were already unlocked
/// earlier in the same session.
pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    note: &str,
    new_title: &str,
    rewrite_links: bool,
    json: bool,
) -> Result<()> {
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let old = find_note(&nb, note)?;

    let (links, files) = if rewrite_links {
        let old_targets = vec![old.frontmatter.title.clone(), old.file_stem()];
        let pool = store.all_notes()?;
        let (links, files, _touched) =
            shiki_core::wikilinks::rewrite_links_to(&old_targets, new_title, &pool)?;
        (links, files)
    } else {
        (0, 0)
    };

    let renamed = nb.rename_note_at(&old.path, new_title)?;

    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "old_path": old.path,
                "new_path": renamed.path,
                "title": renamed.frontmatter.title,
                "links_rewritten": links,
                "notes_touched": files,
            }))?
        );
    } else if links > 0 {
        println!(
            "renamed to '{new_title}' \u{2014} {links} link(s) updated across {files} note(s)"
        );
    } else {
        println!("renamed to '{new_title}': {}", renamed.path.display());
    }
    Ok(())
}

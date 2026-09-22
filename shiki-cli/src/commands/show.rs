use anyhow::Result;
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{find_note, get_notebook, unlock_if_encrypted};

pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    note: &str,
    json: bool,
) -> Result<()> {
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let note = find_note(&nb, note)?;

    if json {
        let value = serde_json::json!({
            "title": note.frontmatter.title,
            "date": note.frontmatter.date.to_string(),
            "tags": note.frontmatter.tags,
            "slug": note.file_stem(),
            "path": note.path,
            "body": note.body,
        });
        println!("{}", serde_json::to_string(&value)?);
        return Ok(());
    }

    println!("# {}", note.frontmatter.title);
    println!("date: {}", note.frontmatter.date);
    if !note.frontmatter.tags.is_empty() {
        println!("tags: {}", note.frontmatter.tags.join(", "));
    }
    println!();
    println!("{}", note.body);
    Ok(())
}

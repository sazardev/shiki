use anyhow::Result;
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{get_notebook, page_footer, page_json, paginate, unlock_if_encrypted};

pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    json: bool,
    offset: usize,
    limit: Option<usize>,
) -> Result<()> {
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let notes = nb.all_notes_recursive()?;
    let (page, total) = paginate(notes, offset, limit);

    if json {
        let items: Vec<serde_json::Value> = page
            .iter()
            .map(|note| {
                serde_json::json!({
                    "title": note.frontmatter.title,
                    "date": note.frontmatter.date.to_string(),
                    "tags": note.frontmatter.tags,
                    "slug": note.file_stem(),
                    "path": note.path,
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
            println!("({notebook} is empty)");
        } else {
            println!("(nothing at offset {offset} \u{2014} {total} note(s) total)");
        }
        return Ok(());
    }
    let shown = page.len();
    for note in page {
        let tags = if note.frontmatter.tags.is_empty() {
            String::new()
        } else {
            format!("  [{}]", note.frontmatter.tags.join(", "))
        };
        println!(
            "{}  {}{tags}",
            note.frontmatter.date, note.frontmatter.title
        );
    }
    if let Some(footer) = page_footer(shown, offset, total) {
        println!("{footer}");
    }
    Ok(())
}

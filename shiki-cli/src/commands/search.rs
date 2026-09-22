use anyhow::Result;
use shiki_config::Config;
use shiki_core::{NotebookStore, SearchEngine};

use super::{get_notebook, page_footer, page_json, paginate, unlock_if_encrypted};

pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    query: &str,
    json: bool,
    offset: usize,
    limit: Option<usize>,
) -> Result<()> {
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let notes = nb.all_notes_recursive()?;
    let mut engine = SearchEngine::new();
    let hits = engine.search(query, &notes);
    let (page, total) = paginate(hits, offset, limit);

    if json {
        let items: Vec<serde_json::Value> = page
            .iter()
            .map(|hit| {
                let note = &notes[hit.index];
                serde_json::json!({
                    "title": note.frontmatter.title,
                    "date": note.frontmatter.date.to_string(),
                    "tags": note.frontmatter.tags,
                    "slug": note.file_stem(),
                    "path": note.path,
                    "score": hit.score,
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
            println!("(no results)");
        } else {
            println!("(nothing at offset {offset} \u{2014} {total} result(s) total)");
        }
        return Ok(());
    }
    let shown = page.len();
    for hit in page {
        let note = &notes[hit.index];
        println!("{}  ({})", note.frontmatter.title, note.file_stem());
    }
    if let Some(footer) = page_footer(shown, offset, total) {
        println!("{footer}");
    }
    Ok(())
}

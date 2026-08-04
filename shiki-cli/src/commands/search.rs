use anyhow::{Context, Result};
use shiki_core::{NotebookStore, SearchEngine};

pub fn run(store: &NotebookStore, notebook: &str, query: &str, json: bool) -> Result<()> {
    let nb = store.get(notebook).with_context(|| {
        format!("notebook '{notebook}' not found \u{2014} see `shiki notebook list`")
    })?;
    let notes = nb.all_notes_recursive()?;
    let mut engine = SearchEngine::new();
    let hits = engine.search(query, &notes);

    if json {
        let items: Vec<serde_json::Value> = hits
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
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if hits.is_empty() {
        println!("(no results)");
        return Ok(());
    }
    for hit in hits {
        let note = &notes[hit.index];
        println!("{}  ({})", note.frontmatter.title, note.file_stem());
    }
    Ok(())
}

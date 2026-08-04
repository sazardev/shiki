use anyhow::{Context, Result};
use shiki_core::{NotebookStore, SearchEngine};

pub fn run(store: &NotebookStore, notebook: &str, query: &str) -> Result<()> {
    let nb = store.get(notebook).with_context(|| {
        format!("notebook '{notebook}' not found \u{2014} see `shiki notebook list`")
    })?;
    let notes = nb.all_notes_recursive()?;
    let mut engine = SearchEngine::new();
    let hits = engine.search(query, &notes);
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

use anyhow::{Context, Result};
use shiki_core::NotebookStore;

pub fn run(store: &NotebookStore, notebook: &str, json: bool) -> Result<()> {
    let nb = store.get(notebook).with_context(|| {
        format!("notebook '{notebook}' not found \u{2014} see `shiki notebook list`")
    })?;
    let notes = nb.all_notes_recursive()?;

    if json {
        let items: Vec<serde_json::Value> = notes
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
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if notes.is_empty() {
        println!("({notebook} is empty)");
        return Ok(());
    }
    for note in notes {
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
    Ok(())
}

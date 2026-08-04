use std::path::Path;

use anyhow::{Context, Result};
use shiki_core::NotebookStore;

/// Renders every note in `notebook` (recursively) into a themed PDF at `out`
/// via `pretty-pdf` (`shiki_core::publish`) — same notebook resolution and
/// `(date, title)` sort as `export.rs`, just a different renderer.
/// `cache_dir` is where the `pretty-pdf` binary is downloaded/cached on
/// first use if it isn't already on `$PATH`.
pub fn run(
    store: &NotebookStore,
    notebook: &str,
    out: &Path,
    theme: &str,
    cache_dir: &Path,
) -> Result<()> {
    let nb = store.get(notebook).with_context(|| {
        format!("notebook '{notebook}' not found \u{2014} see `shiki notebook list`")
    })?;
    let mut notes = nb.all_notes_recursive()?;
    notes.sort_by(|a, b| {
        a.frontmatter
            .date
            .cmp(&b.frontmatter.date)
            .then_with(|| a.frontmatter.title.cmp(&b.frontmatter.title))
    });

    shiki_core::publish::publish(&notes, theme, cache_dir, out)
        .with_context(|| format!("failed to publish '{notebook}' to {}", out.display()))?;
    println!("published {} notes to {}", notes.len(), out.display());
    Ok(())
}

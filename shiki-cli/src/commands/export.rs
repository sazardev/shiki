use anyhow::{Context, Result};
use clap::ValueEnum;
use shiki_core::NotebookStore;
use std::path::Path;

#[derive(Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    /// A single self-contained HTML file.
    Html,
    /// A single plain-Markdown bundle.
    Md,
}

impl From<ExportFormat> for shiki_core::export::Format {
    fn from(format: ExportFormat) -> Self {
        match format {
            ExportFormat::Html => shiki_core::export::Format::Html,
            ExportFormat::Md => shiki_core::export::Format::Md,
        }
    }
}

/// Exports every note in `notebook` (recursively, so nested folders are
/// included) into one file at `out`, sorted by date then title so the
/// output reads chronologically. The actual rendering lives in
/// `shiki_core::export` — shared with the TUI's own export action so
/// there's exactly one implementation.
pub fn run(store: &NotebookStore, notebook: &str, out: &Path, format: ExportFormat) -> Result<()> {
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

    let content = shiki_core::export::render(notebook, &notes, format.into());
    std::fs::write(out, content).with_context(|| format!("failed to write '{}'", out.display()))?;
    println!("exported {} notes to {}", notes.len(), out.display());
    Ok(())
}

use anyhow::{Context, Result};
use clap::ValueEnum;
use pulldown_cmark::{html, Options, Parser};
use shiki_core::{Note, NotebookStore};
use std::path::Path;

#[derive(Clone, Copy, ValueEnum)]
pub enum ExportFormat {
    /// A single self-contained HTML file — real Markdown-to-HTML via
    /// `pulldown-cmark` (a workspace dependency that, before this command,
    /// wasn't actually used anywhere in the codebase), styled with a small
    /// embedded stylesheet so it's readable on its own with no external
    /// assets to ship alongside it.
    Html,
    /// A single plain-Markdown bundle — every note concatenated in order,
    /// each preceded by its title/date/tags as a heading. No HTML/CSS,
    /// just Markdown a note-taking tool (or a human) can read directly.
    Md,
}

/// Exports every note in `notebook` (recursively, so nested folders are
/// included) into one file at `out`, sorted by date then title so the
/// output reads chronologically. `all_notes_recursive` already loads
/// everything shiki considers a note; anything that failed to parse at all
/// is already excluded upstream (`Notebook::list_notes`'s own tolerance for
/// non-shiki `.md` files), so there's nothing extra to filter out here.
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

    let content = match format {
        ExportFormat::Html => render_html(notebook, &notes),
        ExportFormat::Md => render_markdown(notebook, &notes),
    };
    std::fs::write(out, content).with_context(|| format!("failed to write '{}'", out.display()))?;
    println!("exported {} notes to {}", notes.len(), out.display());
    Ok(())
}

fn render_markdown(notebook: &str, notes: &[Note]) -> String {
    let mut buf = format!("# {notebook}\n\n");
    for note in notes {
        buf.push_str(&format!("## {}\n\n", note.frontmatter.title));
        buf.push_str(&format!("*{}*", note.frontmatter.date));
        if !note.frontmatter.tags.is_empty() {
            buf.push_str(&format!(
                " \u{2014} tags: {}",
                note.frontmatter.tags.join(", ")
            ));
        }
        buf.push_str("\n\n");
        buf.push_str(&note.body);
        buf.push_str("\n\n---\n\n");
    }
    buf
}

/// Bare `&`/`<`/`>` escaping for the metadata (title/tags) that gets
/// interpolated straight into the HTML shell rather than run through
/// `pulldown-cmark` — the note body itself doesn't need this, since
/// `html::push_html` already escapes it as part of normal Markdown
/// rendering.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn render_html(notebook: &str, notes: &[Note]) -> String {
    let mut articles = String::new();
    for note in notes {
        articles.push_str("<article>\n<h2>");
        articles.push_str(&escape_html(&note.frontmatter.title));
        articles.push_str("</h2>\n<p class=\"meta\">");
        articles.push_str(&note.frontmatter.date.to_string());
        if !note.frontmatter.tags.is_empty() {
            articles.push_str(" &mdash; ");
            articles.push_str(
                &note
                    .frontmatter
                    .tags
                    .iter()
                    .map(|t| format!("<span class=\"tag\">{}</span>", escape_html(t)))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        articles.push_str("</p>\n");
        let mut body_html = String::new();
        html::push_html(&mut body_html, Parser::new_ext(&note.body, Options::all()));
        articles.push_str(&body_html);
        articles.push_str("</article>\n<hr>\n");
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{title}</title>
<style>
  body {{ max-width: 46rem; margin: 2rem auto; padding: 0 1rem;
          font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
          line-height: 1.6; color: #222; }}
  h1 {{ border-bottom: 2px solid #ccc; padding-bottom: 0.3rem; }}
  article h2 {{ margin-bottom: 0.2rem; }}
  .meta {{ color: #777; font-size: 0.85rem; margin-top: 0; }}
  .tag {{ background: #eee; border-radius: 0.3rem; padding: 0.1rem 0.4rem; font-size: 0.8rem; }}
  pre {{ background: #f5f5f5; padding: 0.8rem; overflow-x: auto; border-radius: 0.3rem; }}
  code {{ background: #f5f5f5; padding: 0.1rem 0.3rem; border-radius: 0.2rem; }}
  pre code {{ background: none; padding: 0; }}
  blockquote {{ border-left: 3px solid #ccc; margin-left: 0; padding-left: 1rem; color: #555; }}
  hr {{ border: none; border-top: 1px solid #eee; margin: 2rem 0; }}
  @media (prefers-color-scheme: dark) {{
    body {{ background: #1a1a1a; color: #ddd; }}
    h1 {{ border-color: #444; }}
    .meta {{ color: #999; }}
    .tag {{ background: #333; }}
    pre, code {{ background: #262626; }}
    blockquote {{ border-color: #444; color: #aaa; }}
    hr {{ border-color: #333; }}
  }}
</style>
</head>
<body>
<h1>{title}</h1>
{articles}
</body>
</html>
"#,
        title = escape_html(notebook),
        articles = articles,
    )
}

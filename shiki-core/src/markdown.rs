//! Markdown → HTML rendering for consumers that need real HTML, unlike
//! `shiki-tui`'s PREVIEW pane (which renders straight to styled ratatui
//! `Line`s, a different concern entirely — see `shiki-tui/src/render.rs`).
//! `shiki-desktop`'s note preview is the first real caller.

use comrak::Options;

/// Renders `md` to HTML with the extension set `shiki-desktop`'s preview
/// needs: strikethrough, tables, autolinks, task lists, and
/// `[[wikilinks]]` — plus raw HTML passthrough so `<details>`/`<summary>`
/// folding blocks render instead of being escaped. Callers that need to
/// post-process the output (e.g. rewriting relative image `src`s for a
/// webview's asset protocol) do that themselves on the returned string.
pub fn note_to_html(md: &str) -> String {
    let mut opts = Options::default();
    opts.extension.strikethrough = true;
    opts.extension.table = true;
    opts.extension.autolink = true;
    opts.extension.tasklist = true;
    opts.extension.wikilinks_title_after_pipe = true;
    opts.render.r#unsafe = true;
    comrak::markdown_to_html(md, &opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_headings_and_lists() {
        let html = note_to_html("# Title\n\n- one\n- two\n");
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<li>one</li>"));
    }

    #[test]
    fn renders_strikethrough_and_tables() {
        let html = note_to_html("~~gone~~\n\n| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(html.contains("<del>gone</del>"));
        assert!(html.contains("<table>"));
    }

    #[test]
    fn renders_task_list_items() {
        let html = note_to_html("- [ ] todo\n- [x] done\n");
        assert!(html.contains("checkbox"));
    }

    #[test]
    fn renders_wikilinks() {
        let html = note_to_html("See [[Some Note]] for more.");
        assert!(html.contains("Some Note"));
    }
}

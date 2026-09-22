use anyhow::Result;
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{find_note, get_notebook, open_in_editor, unlock_if_encrypted};

/// Non-interactive replacement content for `shiki edit`, built from
/// whichever of `--body`/`--stdin`/`--append`/`--append-stdin` was given
/// (`main.rs` enforces they're mutually exclusive via clap's
/// `conflicts_with_all`, so `run` only ever sees at most one). `None`
/// means none of them were passed — the original, interactive `$EDITOR`
/// path, unchanged.
pub enum EditBody {
    Replace(String),
    Append(String),
}

pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    note: &str,
    editor: &str,
    body: Option<EditBody>,
    json: bool,
) -> Result<()> {
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let Some(body) = body else {
        let note = find_note(&nb, note)?;
        return open_in_editor(editor, &note.path);
    };

    let mut note = find_note(&nb, note)?;
    note.body = apply_edit_body(&note.body, body);
    note.save_with_crypto(nb.crypto.as_ref())?;

    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "path": note.path,
                "title": note.frontmatter.title,
            }))?
        );
    } else {
        println!("updated: {}", note.path.display());
    }
    Ok(())
}

/// `Replace` swaps the body wholesale; `Append` adds `text` after
/// `existing`, inserting a separating newline only when `existing` is
/// non-empty and doesn't already end with one — so appending to an empty
/// note doesn't leave a stray leading blank line, and appending to a note
/// that already ends mid-line doesn't glue the new text onto the same
/// line as the old.
fn apply_edit_body(existing: &str, body: EditBody) -> String {
    match body {
        EditBody::Replace(text) => text,
        EditBody::Append(text) => {
            let mut out = existing.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&text);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_ignores_the_existing_body_entirely() {
        assert_eq!(
            apply_edit_body("old content", EditBody::Replace("new content".to_string())),
            "new content"
        );
    }

    #[test]
    fn append_to_empty_body_has_no_leading_blank_line() {
        assert_eq!(
            apply_edit_body("", EditBody::Append("first line".to_string())),
            "first line"
        );
    }

    #[test]
    fn append_inserts_a_newline_when_the_body_lacks_a_trailing_one() {
        assert_eq!(
            apply_edit_body("line one", EditBody::Append("line two".to_string())),
            "line one\nline two"
        );
    }

    #[test]
    fn append_does_not_double_up_an_existing_trailing_newline() {
        assert_eq!(
            apply_edit_body("line one\n", EditBody::Append("line two".to_string())),
            "line one\nline two"
        );
    }
}

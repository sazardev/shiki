//! Read-only MCP resources: a global index, one resource per notebook, and
//! a URI template for individual notes. Everything here goes through the
//! same `get_and_unlock` path as the tools, so encrypted notebooks behave
//! exactly the same way (clear error when `SHIKI_PASSPHRASE` is missing).

use std::path::PathBuf;

use rmcp::model::{
    ListResourceTemplatesResult, ListResourcesResult, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate,
};
use rmcp::ErrorData;
use shiki_core::notebook;

use crate::helpers::{get_and_unlock, tool_err};
use crate::Shiki;

const INDEX_URI: &str = "shiki://index";
const NOTE_TEMPLATE: &str = "shiki://note/{notebook}/{path}";

/// Percent-encodes one URI path segment (UTF-8 bytes, RFC 3986
/// unreserved characters left alone) — notebook names and note paths can
/// contain spaces, `#`, `%` or non-ASCII, none of which are safe raw.
fn encode_segment(segment: &str) -> String {
    let mut out = String::new();
    for byte in segment.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn decode_segment(segment: &str) -> Result<String, ErrorData> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(ErrorData::invalid_params(
                    format!("malformed percent escape in resource URI segment '{segment}'"),
                    None,
                ));
            }
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).map_err(|_| {
                ErrorData::invalid_params(
                    format!("malformed percent escape in resource URI segment '{segment}'"),
                    None,
                )
            })?;
            let byte = u8::from_str_radix(hex, 16).map_err(|_| {
                ErrorData::invalid_params(
                    format!("malformed percent escape in resource URI segment '{segment}'"),
                    None,
                )
            })?;
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| {
        ErrorData::invalid_params(
            format!("resource URI segment '{segment}' isn't valid UTF-8"),
            None,
        )
    })
}

fn notebook_uri(name: &str) -> String {
    format!("shiki://notebook/{}", encode_segment(name))
}

#[cfg(test)]
fn note_uri(notebook: &str, relative: &str) -> String {
    let path = relative
        .split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/");
    format!("shiki://note/{}/{path}", encode_segment(notebook))
}

fn relative_path(nb: &shiki_core::Notebook, path: &std::path::Path) -> String {
    path.strip_prefix(&nb.path)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Index,
    Notebook(String),
    Note { notebook: String, path: PathBuf },
}

fn parse_uri(uri: &str) -> Result<Target, ErrorData> {
    let rest = uri.strip_prefix("shiki://").ok_or_else(|| {
        ErrorData::invalid_params(
            format!("unknown resource URI '{uri}' — expected shiki://…"),
            None,
        )
    })?;
    let rest = rest.trim_end_matches('/');
    if rest == "index" {
        return Ok(Target::Index);
    }
    if let Some(name) = rest.strip_prefix("notebook/") {
        if name.is_empty() || name.contains('/') {
            return Err(ErrorData::invalid_params(
                format!("unknown resource URI '{uri}'"),
                None,
            ));
        }
        return Ok(Target::Notebook(decode_segment(name)?));
    }
    if let Some(rest) = rest.strip_prefix("note/") {
        let (name, path) = rest.split_once('/').ok_or_else(|| {
            ErrorData::invalid_params(
                format!("unknown resource URI '{uri}' — expected shiki://note/<notebook>/<path>"),
                None,
            )
        })?;
        if name.is_empty() || path.is_empty() {
            return Err(ErrorData::invalid_params(
                format!("unknown resource URI '{uri}'"),
                None,
            ));
        }
        let notebook = decode_segment(name)?;
        let path = path
            .split('/')
            .map(decode_segment)
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        let path = notebook::validate_relative_path(&path).map_err(tool_err)?;
        return Ok(Target::Note { notebook, path });
    }
    Err(ErrorData::invalid_params(
        format!("unknown resource URI '{uri}'"),
        None,
    ))
}

pub fn list(shiki: &Shiki) -> Result<ListResourcesResult, ErrorData> {
    let config = shiki.config();
    let mut resources = vec![Resource::new(INDEX_URI, "shiki index")
        .with_title("shiki index")
        .with_description("Every visible notebook and its note count — call this first to orient.")
        .with_mime_type("text/markdown")];
    for nb in shiki.store.list().map_err(tool_err)? {
        if config
            .notebooks
            .get(&nb.name)
            .is_some_and(|over| over.hidden)
        {
            continue;
        }
        let encrypted = config.encrypt_for(&nb.name);
        let description = if encrypted {
            "Encrypted notebook: its note list needs SHIKI_PASSPHRASE in the server's environment."
        } else {
            "Notes in this notebook (metadata only, no bodies)."
        };
        resources.push(
            Resource::new(notebook_uri(&nb.name), format!("{} notebook", nb.name))
                .with_title(nb.name.clone())
                .with_description(description)
                .with_mime_type("text/markdown"),
        );
    }
    Ok(ListResourcesResult::with_all_items(resources))
}

pub fn list_templates() -> ListResourceTemplatesResult {
    ListResourceTemplatesResult::with_all_items(vec![ResourceTemplate::new(
        NOTE_TEMPLATE,
        "shiki note",
    )
    .with_title("shiki note")
    .with_description(
        "One note's full Markdown (frontmatter + body), addressed by notebook and \
                 percent-encoded path.",
    )
    .with_mime_type("text/markdown")])
}

fn markdown(uri: &str, text: String) -> ReadResourceResult {
    ReadResourceResult::new(vec![
        ResourceContents::text(text, uri).with_mime_type("text/markdown")
    ])
}

pub fn read(shiki: &Shiki, uri: &str) -> Result<ReadResourceResult, ErrorData> {
    match parse_uri(uri)? {
        Target::Index => {
            let mut out = String::from("# shiki index\n\n");
            for nb in shiki.store.list().map_err(tool_err)? {
                let hidden = shiki
                    .config()
                    .notebooks
                    .get(&nb.name)
                    .is_some_and(|over| over.hidden);
                if hidden {
                    continue;
                }
                match get_and_unlock(&shiki.store, &shiki.config(), &nb.name) {
                    Ok(unlocked) => match unlocked.all_notes_recursive() {
                        Ok(notes) => out.push_str(&format!(
                            "- **{}** — {} note(s)\n",
                            unlocked.name,
                            notes.len()
                        )),
                        Err(e) => {
                            out.push_str(&format!("- **{}** — unavailable ({e})\n", unlocked.name))
                        }
                    },
                    Err(_) => out.push_str(&format!(
                        "- **{}** — locked (set SHIKI_PASSPHRASE to list it)\n",
                        nb.name
                    )),
                }
            }
            Ok(markdown(uri, out))
        }
        Target::Notebook(name) => {
            let nb = get_and_unlock(&shiki.store, &shiki.config(), &name)?;
            let notes = nb.all_notes_recursive().map_err(tool_err)?;
            let mut out = format!("# {} notebook\n\n", nb.name);
            if notes.is_empty() {
                out.push_str("_No notes._\n");
            }
            for note in &notes {
                out.push_str(&format!(
                    "- **{}** — {} — `{}`{}\n",
                    note.frontmatter.title,
                    note.frontmatter.date,
                    relative_path(&nb, &note.path),
                    if note.frontmatter.tags.is_empty() {
                        String::new()
                    } else {
                        format!(" — tags: {}", note.frontmatter.tags.join(", "))
                    }
                ));
            }
            Ok(markdown(uri, out))
        }
        Target::Note { notebook, path } => {
            let nb = get_and_unlock(&shiki.store, &shiki.config(), &notebook)?;
            let full = nb.path.join(&path);
            if !full.starts_with(&nb.path) {
                return Err(ErrorData::invalid_params(
                    format!("note path escapes notebook: {notebook}/{}", path.display()),
                    None,
                ));
            }
            let note = nb
                .all_notes_recursive()
                .map_err(tool_err)?
                .into_iter()
                .find(|note| note.path == full)
                .ok_or_else(|| {
                    ErrorData::invalid_params(
                        format!("note not found: {notebook}/{}", path.display()),
                        None,
                    )
                })?;
            let text = note.to_file_contents().map_err(tool_err)?;
            Ok(markdown(uri, text))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encoding_round_trips_spaces_hashes_and_unicode() {
        for raw in ["personal", "work notes", "a#b", "café/ß", "100%"] {
            assert_eq!(decode_segment(&encode_segment(raw)).unwrap(), raw);
        }
    }

    #[test]
    fn note_uris_encode_each_path_segment_but_keep_separators() {
        let uri = note_uri("work notes", "meetings/2026 kickoff.md");
        assert_eq!(uri, "shiki://note/work%20notes/meetings/2026%20kickoff.md");
        assert_eq!(
            parse_uri(&uri).unwrap(),
            Target::Note {
                notebook: "work notes".into(),
                path: PathBuf::from("meetings/2026 kickoff.md"),
            }
        );
    }

    #[test]
    fn parse_uri_accepts_index_and_notebook_resources() {
        assert_eq!(parse_uri("shiki://index").unwrap(), Target::Index);
        assert_eq!(
            parse_uri("shiki://notebook/work%20notes").unwrap(),
            Target::Notebook("work notes".into())
        );
    }

    #[test]
    fn parse_uri_rejects_traversal_and_unknown_schemes() {
        assert!(parse_uri("https://example.com/note").is_err());
        assert!(parse_uri("shiki://note/work/../../etc/passwd").is_err());
        assert!(parse_uri("shiki://note/work").is_err());
        assert!(parse_uri("shiki://unknown").is_err());
    }
}

//! Small shared pieces every tool body in `main.rs` uses — kept out of
//! `main.rs` so the `#[tool_router]` impl block reads as a flat list of
//! tool bodies, not a mix of tool logic and plumbing.

use rmcp::ErrorData;
use shiki_config::Config;
use shiki_core::{Note, Notebook, NotebookStore};

/// Turns any displayable error into a proper MCP tool error instead of a
/// panic or a silently-swallowed `Result`.
pub fn tool_err(e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

/// `notebook` if given, else `config.general.default_notebook` — the same
/// fallback `shiki-cli`'s `Context::notebook_name` applies.
pub fn resolve_notebook_name(config: &Config, notebook: Option<&str>) -> String {
    notebook
        .map(str::to_string)
        .unwrap_or_else(|| config.general.default_notebook.clone())
}

/// Resolves a notebook by name and, if it's configured as encrypted,
/// attaches crypto from `SHIKI_PASSPHRASE` — the *only* passphrase source
/// available here, since an MCP server is spawned headless by its client
/// and has no TTY to ever fall back to a prompt on, unlike the CLI.
pub fn get_and_unlock(
    store: &NotebookStore,
    config: &Config,
    name: &str,
) -> Result<Notebook, ErrorData> {
    let nb = shiki_core::notebook::get_notebook(store, name).map_err(tool_err)?;
    if !config.encrypt_for(&nb.name) {
        return Ok(nb);
    }
    match std::env::var("SHIKI_PASSPHRASE") {
        Ok(passphrase) => Ok(nb.with_crypto(Some(shiki_core::crypto::NotebookCrypto::new(
            passphrase,
        )))),
        Err(_) => Err(ErrorData::invalid_params(
            format!(
                "notebook '{}' is encrypted \u{2014} set SHIKI_PASSPHRASE in the environment this MCP server runs in",
                nb.name
            ),
            None,
        )),
    }
}

/// The same note-summary JSON shape `shiki list`/`shiki search` emit —
/// title/date/tags/slug/path, never the body (that's `show_note`'s job).
pub fn note_summary(note: &Note) -> serde_json::Value {
    serde_json::json!({
        "title": note.frontmatter.title,
        "date": note.frontmatter.date.to_string(),
        "tags": note.frontmatter.tags,
        "slug": note.file_stem(),
        "path": note.path,
    })
}

/// Parses a `set_field` value as a YAML scalar (`3` -> int, `true` ->
/// bool, else string), matching the CLI's `shiki field --set` — falls
/// back to a plain string on anything that doesn't parse as YAML rather
/// than failing the whole call.
pub fn parse_field_value(raw: &str) -> serde_yaml::Value {
    serde_yaml::from_str::<serde_yaml::Value>(raw)
        .unwrap_or_else(|_| serde_yaml::Value::String(raw.to_string()))
}

/// The six named `Frontmatter` fields `set_field`/`unset` must reject —
/// same list and reasoning as the CLI's `shiki field` command.
pub fn reserved_field_owner(key: &str) -> Option<&'static str> {
    const RESERVED: &[(&str, &str)] = &[
        ("title", "rename_note"),
        (
            "date",
            "not settable \u{2014} dates come from the note's own history",
        ),
        ("tags", "tag_note"),
        ("aliases", "not yet settable via MCP \u{2014} use the TUI"),
        ("notebook", "move_note"),
        (
            "links",
            "computed from the body's own [[wikilinks]], not settable",
        ),
        ("template", "set at creation via new_note"),
    ];
    RESERVED
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, owner)| *owner)
}

/// Reads a passphrase from the named environment variable, or a clear tool
/// error — the *only* source `encrypt_notebook`/`decrypt_notebook`/
/// `rekey_notebook` have, since this server never has a TTY to prompt on.
pub fn passphrase_from_env(var: &str) -> Result<String, ErrorData> {
    std::env::var(var).map_err(|_| {
        ErrorData::invalid_params(
            format!("set {var} in the environment this MCP server runs in"),
            None,
        )
    })
}

/// The same per-file diff summary shape the CLI's `shiki diff` prints —
/// file name plus added/removed counts plus every changed line.
pub fn diff_file_json(name: &str, lines: &[shiki_core::git::DiffLine]) -> serde_json::Value {
    let added = lines.iter().filter(|l| l.origin == '+').count();
    let removed = lines.iter().filter(|l| l.origin == '-').count();
    serde_json::json!({
        "file": name,
        "added": added,
        "removed": removed,
        "lines": lines.iter().map(|l| serde_json::json!({
            "origin": l.origin.to_string(),
            "content": l.content,
        })).collect::<Vec<_>>(),
    })
}

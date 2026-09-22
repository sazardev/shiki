use anyhow::Result;
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{find_note, get_notebook, unlock_if_encrypted};

/// The six named `Frontmatter` fields — anything else lives in `extra`
/// (the map `shiki query` reads for Dataview-style filtering). Setting one
/// of these through `field` would either silently do nothing (serde's
/// `#[serde(flatten)]` on `extra` means a key matching a named field never
/// actually reaches it) or, worse, shadow the real field with a
/// same-named `extra` entry that only some code paths see — rejected
/// outright instead, pointing at the command that actually owns each one.
const RESERVED_KEYS: &[(&str, &str)] = &[
    ("title", "shiki rename"),
    (
        "date",
        "shiki edit (dates come from the note's own history)",
    ),
    ("tags", "shiki tag"),
    (
        "aliases",
        "not yet CLI-editable \u{2014} use the TUI or edit the file directly",
    ),
    ("notebook", "shiki move"),
    (
        "links",
        "computed from the body's own [[wikilinks]], not settable",
    ),
    (
        "template",
        "set at creation via `shiki new`/`shiki capture --template`",
    ),
];

/// One `--set key=value` or `--unset key`.
pub enum FieldOp {
    Set(String, String),
    Unset(String),
}

pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    note: &str,
    ops: &[FieldOp],
    json: bool,
) -> Result<()> {
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let mut note = find_note(&nb, note)?;

    for op in ops {
        let key = match op {
            FieldOp::Set(key, _) | FieldOp::Unset(key) => key,
        };
        if let Some(owner) = reserved_owner(key) {
            anyhow::bail!("'{key}' isn't a custom field \u{2014} use {owner} instead");
        }
        match op {
            FieldOp::Set(key, raw_value) => {
                note.frontmatter.extra.insert(
                    serde_yaml::Value::String(key.clone()),
                    parse_field_value(raw_value),
                );
            }
            FieldOp::Unset(key) => {
                note.frontmatter.extra.remove(key.as_str());
            }
        }
    }
    note.save_with_crypto(nb.crypto.as_ref())?;

    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "path": note.path,
                "fields": note.frontmatter.extra,
            }))?
        );
    } else if note.frontmatter.extra.is_empty() {
        println!("(no custom fields)");
    } else {
        for (k, v) in note.frontmatter.extra.iter() {
            println!(
                "{}: {}",
                shiki_core::query::yaml_value_to_string(k),
                shiki_core::query::yaml_value_to_string(v)
            );
        }
    }
    Ok(())
}

/// `Some(command)` naming the CLI command that actually owns `key`, if
/// it's one of the six named `Frontmatter` fields — `None` for any real
/// custom field, which is the common case.
fn reserved_owner(key: &str) -> Option<&'static str> {
    RESERVED_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, owner)| *owner)
}

/// Parses a `--set key=value` value as a YAML scalar — `3` becomes an
/// int, `true`/`false` a bool, `null`/`~` null, anything else (including
/// something that merely looks like broken YAML, e.g. an unmatched quote)
/// a plain string. This is what lets `shiki field note --set priority=3`
/// and `shiki query 'where priority > 2'` agree on the field's real type,
/// matching the round-trip guarantee `note.rs` already tests for `extra`.
fn parse_field_value(raw: &str) -> serde_yaml::Value {
    serde_yaml::from_str::<serde_yaml::Value>(raw)
        .unwrap_or_else(|_| serde_yaml::Value::String(raw.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_owner_flags_named_fields_only() {
        assert_eq!(reserved_owner("title"), Some("shiki rename"));
        assert_eq!(reserved_owner("tags"), Some("shiki tag"));
        assert_eq!(reserved_owner("status"), None);
        assert_eq!(reserved_owner("priority"), None);
    }

    #[test]
    fn parse_field_value_keeps_real_yaml_types() {
        assert_eq!(parse_field_value("3"), serde_yaml::Value::from(3));
        assert_eq!(parse_field_value("true"), serde_yaml::Value::from(true));
        assert_eq!(
            parse_field_value("pending"),
            serde_yaml::Value::from("pending")
        );
    }

    #[test]
    fn parse_field_value_falls_back_to_a_plain_string_on_unparseable_input() {
        // An unmatched quote isn't valid YAML on its own — still usable as
        // a plain string value instead of erroring the whole command.
        assert_eq!(
            parse_field_value("\"unterminated"),
            serde_yaml::Value::String("\"unterminated".to_string())
        );
    }
}

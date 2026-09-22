use anyhow::Result;
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::{find_note, get_notebook, unlock_if_encrypted};

/// `shiki tag <note> -n <nb> [--add a,b] [--remove c,d]` — edits a note's
/// `tags:` frontmatter list directly (`Vec<String>`), the non-interactive
/// counterpart of the TUI's tags modal. `--add` dedupes (a tag already
/// present is a no-op, not a duplicate entry); `--remove` is a plain
/// filter. Both may be given together in one call. With neither, this is
/// just a read of the current tags.
pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: &str,
    note: &str,
    add: &[String],
    remove: &[String],
    json: bool,
) -> Result<()> {
    let nb = unlock_if_encrypted(config, get_notebook(store, notebook)?)?;
    let mut note = find_note(&nb, note)?;

    if apply_tag_ops(&mut note.frontmatter.tags, add, remove) {
        note.save_with_crypto(nb.crypto.as_ref())?;
    }

    if json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "path": note.path,
                "tags": note.frontmatter.tags,
            }))?
        );
    } else if note.frontmatter.tags.is_empty() {
        println!("(no tags)");
    } else {
        println!("tags: {}", note.frontmatter.tags.join(", "));
    }
    Ok(())
}

/// Mutates `tags` in place per `--add`/`--remove`, both applied in the
/// same call so `shiki tag foo --add x --remove y` reads left to right as
/// one operation rather than two. Returns whether anything actually
/// changed, so `run` only writes the file back when there's something to
/// save. `--add` dedupes (adding an already-present tag is a no-op, never
/// a duplicate entry); blank entries (e.g. a trailing comma in
/// `--add a,b,`) are silently skipped rather than becoming an empty-string
/// tag.
fn apply_tag_ops(tags: &mut Vec<String>, add: &[String], remove: &[String]) -> bool {
    let mut changed = false;
    for tag in add {
        let tag = tag.trim();
        if !tag.is_empty() && !tags.iter().any(|t| t == tag) {
            tags.push(tag.to_string());
            changed = true;
        }
    }
    if !remove.is_empty() {
        let before = tags.len();
        tags.retain(|t| !remove.iter().any(|r| r.trim() == t));
        changed |= tags.len() != before;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_dedupes_and_skips_blank_entries() {
        let mut tags = vec!["work".to_string()];
        let changed = apply_tag_ops(
            &mut tags,
            &["work".to_string(), " idea ".to_string(), "".to_string()],
            &[],
        );
        assert!(changed);
        assert_eq!(tags, vec!["work".to_string(), "idea".to_string()]);
    }

    #[test]
    fn remove_filters_trimmed_matches() {
        let mut tags = vec!["work".to_string(), "idea".to_string()];
        let changed = apply_tag_ops(&mut tags, &[], &[" work ".to_string()]);
        assert!(changed);
        assert_eq!(tags, vec!["idea".to_string()]);
    }

    #[test]
    fn add_and_remove_together_apply_in_one_pass() {
        let mut tags = vec!["work".to_string()];
        let changed = apply_tag_ops(&mut tags, &["idea".to_string()], &["work".to_string()]);
        assert!(changed);
        assert_eq!(tags, vec!["idea".to_string()]);
    }

    #[test]
    fn no_ops_reports_no_change() {
        let mut tags = vec!["work".to_string()];
        assert!(!apply_tag_ops(&mut tags, &[], &[]));
        assert!(!apply_tag_ops(&mut tags, &["work".to_string()], &[]));
        assert_eq!(tags, vec!["work".to_string()]);
    }
}

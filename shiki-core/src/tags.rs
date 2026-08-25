use std::collections::BTreeMap;
use std::path::PathBuf;

use regex::Regex;
use std::sync::OnceLock;

use crate::note::Note;
use crate::notebook::Notebook;

/// Index of tags -> notes containing them, for the tag-filtering panel.
#[derive(Debug, Default, Clone)]
pub struct TagIndex {
    index: BTreeMap<String, Vec<PathBuf>>,
}

impl TagIndex {
    pub fn build(notes: &[Note]) -> Self {
        let mut index: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
        for note in notes {
            for tag in &note.frontmatter.tags {
                index
                    .entry(tag.clone())
                    .or_default()
                    .push(note.path.clone());
            }
        }
        Self { index }
    }

    pub fn tags(&self) -> impl Iterator<Item = &String> {
        self.index.keys()
    }

    pub fn notes_for(&self, tag: &str) -> &[PathBuf] {
        self.index.get(tag).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

/// Every distinct tag used anywhere across `pool` — deduped, sorted. Used
/// by the TUI's metadata editor to suggest existing tags while typing new
/// ones (so "cliente" and "clientes" don't quietly become two different
/// tags by accident), and as the candidate list `rename_tag`'s target
/// could plausibly be one of.
pub fn all_tags(pool: &[(Notebook, Note)]) -> Vec<String> {
    let mut tags: Vec<String> = pool
        .iter()
        .flat_map(|(_, note)| note.frontmatter.tags.iter().cloned())
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

/// Renames (or merges, if `new` already exists on some of the same notes)
/// tag `old` to `new` across every note in `pool` that carries it — writes
/// each affected note back to disk immediately, using its own notebook's
/// crypto if that notebook is encrypted. A note that already has `new` as
/// well as `old` just drops `old` rather than ending up with a duplicate.
/// Walks the whole pool (typically `NotebookStore::all_notes()`, every
/// notebook) rather than being scoped to one notebook or folder — a typo'd
/// tag is just as likely to have spread across the whole vault as to be
/// contained to wherever it was first typed, and there is no cheaper
/// "rename it here for now" version of fixing that. Returns the number of
/// notes actually rewritten and the distinct notebook names touched (for
/// the caller to feed into whatever per-notebook change-tracking it does —
/// see the TUI's `App::note_changed`) — a no-op (`old` not used anywhere)
/// returns `Ok((0, vec![]))`, not an error.
pub fn rename_tag(
    pool: &[(Notebook, Note)],
    old: &str,
    new: &str,
) -> crate::Result<(usize, Vec<String>)> {
    let new = new.trim();
    if new.is_empty() {
        return Err(crate::Error::EmptyTagName);
    }
    let mut notes_updated = 0;
    let mut notebooks_touched = Vec::new();
    for (nb, note) in pool {
        if !note.frontmatter.tags.iter().any(|t| t == old) {
            continue;
        }
        let mut updated = note.clone();
        let mut tags: Vec<String> = updated
            .frontmatter
            .tags
            .iter()
            .filter(|t| t.as_str() != old)
            .cloned()
            .collect();
        if !tags.iter().any(|t| t == new) {
            tags.push(new.to_string());
        }
        updated.frontmatter.tags = tags;
        updated.save_with_crypto(nb.crypto.as_ref())?;
        notes_updated += 1;
        if !notebooks_touched.contains(&nb.name) {
            notebooks_touched.push(nb.name.clone());
        }
    }
    Ok((notes_updated, notebooks_touched))
}

/// Every inline `#hashtag` in a markdown body, deduplicated in order of
/// first appearance — the Obsidian convention the import path converts
/// into frontmatter tags (shiki itself only indexes frontmatter). Rules:
/// the tag starts at a `#` immediately followed by a non-space character
/// (which is exactly what keeps ATX headings like `# Heading` out), allows
/// letters/digits/`_`/`-`/`/` (nested tags), must contain at least one
/// letter so `#1`-style issue references don't count, and fenced code
/// blocks are skipped entirely — a `# comment` inside a code example is
/// text.
pub fn inline_hashtags(body: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?:^|[\s(\[{>])#([\w/\-]+)").unwrap());
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;
    for line in body.split('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        for caps in re.captures_iter(line) {
            let tag = caps[1].to_string();
            if !tag.chars().any(char::is_alphabetic) {
                continue;
            }
            if !out.contains(&tag) {
                out.push(tag);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Frontmatter;

    fn note_with_tags(path: &str, tags: &[&str]) -> Note {
        let mut fm = Frontmatter::new("Title", "test");
        fm.tags = tags.iter().map(|t| t.to_string()).collect();
        Note::new(PathBuf::from(path), fm, String::new())
    }

    #[test]
    fn build_indexes_notes_by_every_tag_they_carry() {
        let notes = vec![
            note_with_tags("a.md", &["work", "urgent"]),
            note_with_tags("b.md", &["work"]),
            note_with_tags("c.md", &[]),
        ];

        let index = TagIndex::build(&notes);

        assert_eq!(index.len(), 2);
        assert_eq!(
            index.notes_for("work"),
            &[PathBuf::from("a.md"), PathBuf::from("b.md")]
        );
        assert_eq!(index.notes_for("urgent"), &[PathBuf::from("a.md")]);
    }

    #[test]
    fn notes_for_unknown_tag_is_empty() {
        let notes = vec![note_with_tags("a.md", &["work"])];
        let index = TagIndex::build(&notes);
        assert!(index.notes_for("nonexistent").is_empty());
    }

    #[test]
    fn build_of_no_tags_at_all_is_empty() {
        let notes = vec![note_with_tags("a.md", &[])];
        let index = TagIndex::build(&notes);
        assert!(index.is_empty());
        assert_eq!(index.tags().count(), 0);
    }

    #[test]
    fn tags_are_returned_in_sorted_order() {
        let notes = vec![note_with_tags("a.md", &["zeta", "alpha", "mid"])];
        let index = TagIndex::build(&notes);
        let tags: Vec<&String> = index.tags().collect();
        assert_eq!(tags, vec!["alpha", "mid", "zeta"]);
    }

    fn test_notebook(root: &std::path::Path, name: &str) -> Notebook {
        let path = root.join(name);
        std::fs::create_dir_all(&path).unwrap();
        Notebook::new(name, path)
    }

    #[test]
    fn all_tags_deduped_and_sorted_across_notes() {
        let nb = Notebook::new("nb", PathBuf::from("/tmp/nb"));
        let notes = vec![
            note_with_tags("a.md", &["zeta", "work"]),
            note_with_tags("b.md", &["work", "alpha"]),
        ];
        let pool: Vec<(Notebook, Note)> = notes.into_iter().map(|n| (nb.clone(), n)).collect();
        assert_eq!(
            all_tags(&pool),
            vec!["alpha".to_string(), "work".to_string(), "zeta".to_string()]
        );
    }

    #[test]
    fn inline_hashtags_extracts_dedupes_and_skips_fences_and_headings() {
        let body = "# Heading one\n\nnote with #work and #work again, nested #work/rust, \
                    issue #123 stays out, code:\n```sh\n# not-a-tag here\n```\nend #ok_1\n";
        assert_eq!(
            inline_hashtags(body),
            vec![
                "work".to_string(),
                "work/rust".to_string(),
                "ok_1".to_string()
            ]
        );
    }

    #[test]
    fn inline_hashtags_empty_body_is_empty() {
        assert!(inline_hashtags("").is_empty());
        assert!(inline_hashtags("# \n## \n123 #456").is_empty());
    }

    #[test]
    fn rename_tag_rewrites_every_note_that_carries_it_across_notebooks() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        let b = test_notebook(tmp.path(), "b");

        let mut n1 = a.create_note("One", "body").unwrap();
        n1.frontmatter.tags = vec!["proyecto".to_string()];
        n1.save().unwrap();

        let mut n2 = b.create_note("Two", "body").unwrap();
        n2.frontmatter.tags = vec!["proyecto".to_string(), "urgent".to_string()];
        n2.save().unwrap();

        let mut n3 = a.create_note("Three", "body").unwrap();
        n3.frontmatter.tags = vec!["other".to_string()];
        n3.save().unwrap();

        let pool = vec![
            (a.clone(), Note::from_file(&n1.path).unwrap()),
            (b.clone(), Note::from_file(&n2.path).unwrap()),
            (a.clone(), Note::from_file(&n3.path).unwrap()),
        ];

        let (count, mut notebooks) = rename_tag(&pool, "proyecto", "proyectos").unwrap();
        assert_eq!(count, 2);
        notebooks.sort();
        assert_eq!(notebooks, vec!["a".to_string(), "b".to_string()]);

        assert_eq!(
            Note::from_file(&n1.path).unwrap().frontmatter.tags,
            vec!["proyectos".to_string()]
        );
        assert_eq!(
            Note::from_file(&n2.path).unwrap().frontmatter.tags,
            vec!["urgent".to_string(), "proyectos".to_string()]
        );
        // Untouched — never had the old tag.
        assert_eq!(
            Note::from_file(&n3.path).unwrap().frontmatter.tags,
            vec!["other".to_string()]
        );
    }

    #[test]
    fn rename_tag_merges_into_an_existing_tag_without_duplicating() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        let mut n = a.create_note("One", "body").unwrap();
        n.frontmatter.tags = vec!["proyecto".to_string(), "proyectos".to_string()];
        n.save().unwrap();

        let pool = vec![(a.clone(), Note::from_file(&n.path).unwrap())];
        let (count, _) = rename_tag(&pool, "proyecto", "proyectos").unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            Note::from_file(&n.path).unwrap().frontmatter.tags,
            vec!["proyectos".to_string()]
        );
    }

    #[test]
    fn rename_tag_rejects_empty_new_name() {
        let nb = Notebook::new("nb", PathBuf::from("/tmp/nb"));
        let pool = vec![(nb, note_with_tags("a.md", &["proyecto"]))];
        assert!(rename_tag(&pool, "proyecto", "   ").is_err());
    }

    #[test]
    fn rename_tag_is_a_no_op_when_the_old_tag_is_unused() {
        let nb = Notebook::new("nb", PathBuf::from("/tmp/nb"));
        let pool = vec![(nb, note_with_tags("a.md", &["other"]))];
        let (count, notebooks) = rename_tag(&pool, "proyecto", "proyectos").unwrap();
        assert_eq!(count, 0);
        assert!(notebooks.is_empty());
    }
}

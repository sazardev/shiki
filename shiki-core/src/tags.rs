use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::note::Note;

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
}

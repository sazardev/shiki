use std::collections::HashMap;
use std::path::PathBuf;

use regex::Regex;
use std::sync::OnceLock;

use crate::note::Note;

/// Finds all `[[wikilinks]]` in a note's markdown body.
pub fn extract(body: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\[\[([^\[\]|]+)(?:\|[^\[\]]+)?\]\]").unwrap());
    re.captures_iter(body)
        .map(|c| c[1].trim().to_string())
        .collect()
}

/// Resolves a single wikilink's target text to an existing note's path,
/// matched against every note in `notes` — not just a notebook's root, since
/// a link can point at a note nested in any folder. Tries an exact
/// (case-insensitive) title match first, then falls back to comparing
/// slugs, so `[[Weekend Hiking Trip]]` and `[[weekend-hiking-trip]]` both
/// resolve to the same note.
pub fn resolve_one(link: &str, notes: &[Note]) -> Option<PathBuf> {
    let link = link.trim();
    let slug = Note::slugify(link);
    notes
        .iter()
        .find(|n| n.frontmatter.title.eq_ignore_ascii_case(link) || n.file_stem() == slug)
        .map(|n| n.path.clone())
}

/// Every note in `notes` (excluding `target` itself) whose body contains a
/// `[[wikilink]]` that resolves to `target` — the reverse of `extract` +
/// `resolve_one`, used to answer "what links here?" without every note
/// needing to maintain its own back-reference list.
///
/// Builds a title/slug -> path index once up front rather than calling
/// `resolve_one` (a linear scan + `slugify` over every note) once per link
/// per note — that combination made this function cost O(notes × total
/// links in the notebook), visibly slow on a notebook with a few thousand
/// notes even though each note only has a handful of links.
pub fn backlinks<'a>(target: &std::path::Path, notes: &'a [Note]) -> Vec<&'a Note> {
    let mut by_title: HashMap<String, &std::path::Path> = HashMap::with_capacity(notes.len());
    let mut by_slug: HashMap<String, &std::path::Path> = HashMap::with_capacity(notes.len());
    for n in notes {
        by_title
            .entry(n.frontmatter.title.to_ascii_lowercase())
            .or_insert(&n.path);
        by_slug.entry(n.file_stem()).or_insert(&n.path);
    }
    let resolve = |link: &str| -> Option<&std::path::Path> {
        let link = link.trim();
        by_title
            .get(&link.to_ascii_lowercase())
            .or_else(|| by_slug.get(&Note::slugify(link)))
            .copied()
    };
    notes
        .iter()
        .filter(|n| {
            n.path != target
                && extract(&n.body)
                    .iter()
                    .any(|link| resolve(link) == Some(target))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Frontmatter;

    fn note(path: &str, title: &str, body: &str) -> Note {
        Note::new(
            PathBuf::from(path),
            Frontmatter::new(title, "test"),
            body.to_string(),
        )
    }

    #[test]
    fn extract_finds_every_link_ignoring_aliases() {
        let body = "See [[Weekend Hiking Trip]] and [[Gift Ideas|gifts]] for more.";
        assert_eq!(
            extract(body),
            vec!["Weekend Hiking Trip".to_string(), "Gift Ideas".to_string()]
        );
    }

    #[test]
    fn resolve_one_matches_by_title_case_insensitively() {
        let notes = vec![note("a/hiking.md", "Weekend Hiking Trip", "")];
        assert_eq!(
            resolve_one("weekend hiking trip", &notes),
            Some(PathBuf::from("a/hiking.md"))
        );
    }

    #[test]
    fn resolve_one_falls_back_to_slug_match() {
        // On disk the file name is always `slugify(title)` (`create_note_in`)
        // — a link using that slug directly (rather than the exact title
        // text) should still resolve via the file-stem fallback.
        let notes = vec![note("a/weekend-hiking-trip.md", "Weekend Hiking Trip!", "")];
        assert_eq!(
            resolve_one("weekend-hiking-trip", &notes),
            Some(PathBuf::from("a/weekend-hiking-trip.md"))
        );
    }

    #[test]
    fn resolve_one_finds_nested_notes_not_just_root() {
        let notes = vec![note("nb/projects/roadmap.md", "shiki roadmap", "")];
        assert_eq!(
            resolve_one("shiki roadmap", &notes),
            Some(PathBuf::from("nb/projects/roadmap.md"))
        );
    }

    #[test]
    fn resolve_one_returns_none_for_unknown_link() {
        let notes = vec![note("a/hiking.md", "Weekend Hiking Trip", "")];
        assert_eq!(resolve_one("nonexistent", &notes), None);
    }

    #[test]
    fn backlinks_finds_notes_linking_to_target_and_excludes_self_and_unrelated() {
        // The target's own body contains a self-reference, which must not
        // count as its own backlink.
        let target = note(
            "a/hiking.md",
            "Weekend Hiking Trip",
            "[[Weekend Hiking Trip]] (self-reference, shouldn't count)",
        );
        let linker = note(
            "a/journal.md",
            "Journal",
            "Planning [[Weekend Hiking Trip]] now.",
        );
        let unrelated = note("a/other.md", "Other", "Nothing to see here.");
        let notes = vec![target.clone(), linker.clone(), unrelated];

        let result = backlinks(&target.path, &notes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, linker.path);
    }

    #[test]
    fn backlinks_ignores_unresolvable_links() {
        let target = note("a/hiking.md", "Weekend Hiking Trip", "");
        let notes = vec![
            target.clone(),
            note("a/journal.md", "Journal", "See [[Nonexistent Note]]."),
        ];
        assert!(backlinks(&target.path, &notes).is_empty());
    }
}

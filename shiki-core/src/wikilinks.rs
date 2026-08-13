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

/// Resolves a wikilink against the current notebook's notes first, then —
/// if it doesn't match there — falls back to every other notebook's notes.
/// `resolve_one` alone is notebook-scoped, which silently breaks any link
/// that points across notebooks: the daily note's "## Due today" agenda
/// bullets `[[link]]` tasks living in *other* notebooks, and any note that
/// links to one elsewhere, would report "doesn't match any note". Local
/// resolution always wins (two notebooks can legitimately have different
/// notes with the same title — see `graph.rs`'s note about not resolving
/// globally), so the fallback only ever sees links the current notebook
/// genuinely can't satisfy. Returns the resolved path plus the name of the
/// notebook it lives in (`None` when it resolved locally), so the caller
/// can switch notebooks before jumping.
pub fn resolve_one_global<'a>(
    link: &str,
    local_notes: &[Note],
    global: &'a [(crate::Notebook, Note)],
) -> Option<(PathBuf, Option<&'a str>)> {
    if let Some(path) = resolve_one(link, local_notes) {
        return Some((path, None));
    }
    let link = link.trim();
    let slug = Note::slugify(link);
    for (nb, note) in global {
        if note.frontmatter.title.eq_ignore_ascii_case(link) || note.file_stem() == slug {
            return Some((note.path.clone(), Some(&nb.name)));
        }
    }
    None
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

/// Every note whose body *mentions* `target`'s title as plain text without
/// actually linking to it — the notes `backlinks` deliberately doesn't
/// return, surfaced so a link that "should" exist can be noticed. A note
/// that already links to `target` is excluded even if it also mentions the
/// title elsewhere (it's a backlink, not a missed one), and so is `target`
/// itself. Matching is a case-insensitive substring search on the title;
/// an empty title matches nothing rather than everything.
pub fn unlinked_mentions<'a>(target: &Note, notes: &'a [Note]) -> Vec<&'a Note> {
    let title = target.frontmatter.title.trim().to_lowercase();
    if title.is_empty() {
        return Vec::new();
    }
    let linked: std::collections::HashSet<&std::path::Path> = backlinks(&target.path, notes)
        .into_iter()
        .map(|n| n.path.as_path())
        .collect();
    notes
        .iter()
        .filter(|n| {
            n.path != target.path
                && !linked.contains(n.path.as_path())
                && n.body.to_lowercase().contains(&title)
        })
        .collect()
}

/// Every resolved link between notes in `notes`, as deduplicated
/// `(from, to)` index pairs — the adjacency list `shiki graph` renders and
/// `orphans` derives connectivity from. Self-links are dropped (a note
/// linking to itself isn't a connection). Uses the same title/slug index
/// `backlinks` builds, for the same O(notes × links) reason.
pub fn edges(notes: &[Note]) -> Vec<(usize, usize)> {
    let mut index: HashMap<String, usize> = HashMap::with_capacity(notes.len() * 2);
    for (i, n) in notes.iter().enumerate() {
        index
            .entry(n.frontmatter.title.to_ascii_lowercase())
            .or_insert(i);
        index.entry(n.file_stem()).or_insert(i);
    }
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (from, n) in notes.iter().enumerate() {
        for link in extract(&n.body) {
            let link = link.trim();
            let to = index
                .get(&link.to_ascii_lowercase())
                .or_else(|| index.get(&Note::slugify(link)));
            if let Some(&to) = to {
                if to != from && seen.insert((from, to)) {
                    out.push((from, to));
                }
            }
        }
    }
    out
}

/// Notes with no connections at all — nothing links to them and they link
/// to nothing (mentions don't count; only real, resolved `[[links]]` do).
/// Returned as indices into `notes`, in order.
pub fn orphans(notes: &[Note]) -> Vec<usize> {
    let mut connected = vec![false; notes.len()];
    for (from, to) in edges(notes) {
        connected[from] = true;
        connected[to] = true;
    }
    (0..notes.len()).filter(|&i| !connected[i]).collect()
}

/// Turns the first plain-text mention of `title` in the file at `path`
/// into a real `[[wikilink]]`, preserving the mention's own casing (the
/// resolver is case-insensitive, so `[[proyecto shiki]]` still resolves to
/// "Proyecto shiki"). Occurrences already inside a `[[...]]` are skipped —
/// double-wrapping an existing link would corrupt it — and so is the
/// frontmatter block. `Ok(false)` means no linkable mention was found
/// (e.g. the file changed since the mention was detected); an I/O failure
/// is a real error.
pub fn link_mention(path: &std::path::Path, title: &str) -> crate::Result<bool> {
    let title = title.trim();
    if title.is_empty() {
        return Ok(false);
    }
    let contents = std::fs::read_to_string(path)?;
    let mut lines: Vec<String> = contents.split('\n').map(str::to_string).collect();

    let mut start = 0;
    if lines.first().map(|l| l.trim_end_matches('\r')) == Some("---") {
        if let Some(close) = lines[1..]
            .iter()
            .position(|l| l.trim_end_matches('\r') == "---")
        {
            start = close + 2;
        }
    }

    let needle = title.to_lowercase();
    for line in lines.iter_mut().skip(start) {
        if let Some(pos) = find_unlinked(line, &needle) {
            let end = pos + needle.len();
            line.replace_range(pos..end, &format!("[[{}]]", &line[pos..end]));
            std::fs::write(path, lines.join("\n"))?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Byte offset of the first case-insensitive occurrence of `needle` in
/// `line` that isn't already inside a `[[...]]` span — `None` if every
/// occurrence is linked (or there are none). Assumes `needle` is already
/// lowercase. Offsets are computed on the lowercased copy and applied to
/// the original; `to_lowercase` can shift byte offsets for a handful of
/// exotic characters (e.g. İ), so the match is re-verified against the
/// original before being trusted.
fn find_unlinked(line: &str, needle: &str) -> Option<usize> {
    let lower = line.to_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(needle) {
        let pos = from + rel;
        let inside_link = {
            let before = &lower[..pos];
            let open = before.rfind("[[");
            let close = before.rfind("]]");
            matches!((open, close), (Some(o), c) if c.is_none_or(|c| c < o))
        };
        let verifiable = line
            .get(pos..pos + needle.len())
            .is_some_and(|s| s.to_lowercase() == needle);
        if !inside_link && verifiable {
            return Some(pos);
        }
        from = pos + needle.len().max(1);
        if from >= lower.len() {
            break;
        }
    }
    None
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
    fn resolve_one_global_prefers_local_notebook() {
        let local_dupe = vec![
            note("a/hub.md", "Roadmap", ""),
            note("a/spoke.md", "Spoke", ""),
        ];
        let global = vec![(
            crate::Notebook::new("work", PathBuf::from("a")),
            note("a/spoke.md", "Spoke", ""),
        )];
        assert_eq!(
            resolve_one_global("spoke", &local_dupe, &global),
            Some((PathBuf::from("a/spoke.md"), None))
        );
    }

    #[test]
    fn resolve_one_global_falls_back_to_other_notebooks() {
        let local = vec![note("a/hub.md", "Hub", "See [[Roadmap]].")];
        let global = vec![(
            crate::Notebook::new("work", PathBuf::from("b")),
            note("b/roadmap.md", "Roadmap", ""),
        )];
        let (path, notebook) = resolve_one_global("roadmap", &local, &global).unwrap();
        assert_eq!(path, PathBuf::from("b/roadmap.md"));
        assert_eq!(notebook, Some("work"));
    }

    #[test]
    fn resolve_one_global_returns_none_when_nowhere() {
        let local = vec![note("a/hub.md", "Hub", "")];
        let global = vec![(
            crate::Notebook::new("work", PathBuf::from("b")),
            note("b/other.md", "Other", ""),
        )];
        assert_eq!(resolve_one_global("nope", &local, &global), None);
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
    fn unlinked_mentions_finds_plain_text_mentions_only() {
        let target = note("a/hiking.md", "Weekend Hiking Trip", "");
        let linker = note(
            "a/journal.md",
            "Journal",
            "Planning [[Weekend Hiking Trip]].",
        );
        // Mentions the title in plain text, case-insensitively, no link.
        let mentioner = note("a/ideas.md", "Ideas", "that weekend hiking trip was great");
        let unrelated = note("a/other.md", "Other", "nothing relevant");
        let notes = vec![target.clone(), linker, mentioner.clone(), unrelated];

        let result = unlinked_mentions(&target, &notes);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, mentioner.path);
    }

    #[test]
    fn unlinked_mentions_excludes_notes_that_already_link() {
        let target = note("a/hiking.md", "Weekend Hiking Trip", "");
        // Links AND separately mentions the title in plain text — already a
        // backlink, so it must not also show up as a missed link.
        let both = note(
            "a/journal.md",
            "Journal",
            "[[Weekend Hiking Trip]] — best weekend hiking trip ever.",
        );
        let notes = vec![target.clone(), both];
        assert!(unlinked_mentions(&target, &notes).is_empty());
    }

    #[test]
    fn edges_resolves_links_dedupes_and_drops_self_links() {
        let notes = vec![
            note(
                "a/hub.md",
                "Hub",
                "[[Spoke]] and [[Spoke]] again, plus [[Hub]] itself.",
            ),
            note("a/spoke.md", "Spoke", "back to [[Hub]], and [[Nowhere]]."),
            note("a/loner.md", "Loner", "no links"),
        ];
        assert_eq!(edges(&notes), vec![(0, 1), (1, 0)]);
    }

    #[test]
    fn orphans_are_notes_with_no_resolved_links_either_way() {
        let notes = vec![
            note("a/hub.md", "Hub", "[[Spoke]]"),
            note("a/spoke.md", "Spoke", ""),
            note("a/loner.md", "Loner", "mentions Hub in plain text only"),
        ];
        assert_eq!(orphans(&notes), vec![2]);
    }

    #[test]
    fn link_mention_wraps_first_plain_occurrence_preserving_case() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(
            &path,
            "---\ntitle: Journal\n---\n\nthat weekend hiking trip was great\n",
        )
        .unwrap();

        assert!(link_mention(&path, "Weekend Hiking Trip").unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "---\ntitle: Journal\n---\n\nthat [[weekend hiking trip]] was great\n"
        );
    }

    #[test]
    fn link_mention_skips_occurrences_already_inside_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "[[Hub]] is linked; but hub in plain text isn't.\n").unwrap();

        assert!(link_mention(&path, "Hub").unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[[Hub]] is linked; but [[hub]] in plain text isn't.\n"
        );
    }

    #[test]
    fn link_mention_reports_false_when_nothing_linkable_remains() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "only [[Hub]] as a link, no plain mention.\n").unwrap();
        assert!(!link_mention(&path, "Hub").unwrap());
    }

    #[test]
    fn unlinked_mentions_empty_title_matches_nothing() {
        let target = note("a/blank.md", "  ", "");
        let other = note("a/other.md", "Other", "any text at all");
        let notes = vec![target.clone(), other];
        assert!(unlinked_mentions(&target, &notes).is_empty());
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

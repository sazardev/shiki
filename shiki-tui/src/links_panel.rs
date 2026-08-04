//! Flattens a note's outgoing `[[wikilinks]]` and incoming backlinks into one
//! list for the links modal (`Action::ShowLinks`) — same "flat rows, some of
//! them non-selectable headers" shape as `tree::TreeRow`, so selection/
//! navigation reuses the exact same index-mapping pattern.

use std::path::PathBuf;

use shiki_core::{wikilinks, Note};

/// One row in the flattened links list: a section header (display only —
/// selection always skips these), an outgoing link (resolved against the
/// notebook, or not — a broken link is still shown, just not jumpable), or
/// an incoming backlink (always a real note, since `wikilinks::backlinks`
/// only ever returns notes that actually resolved).
#[derive(Debug, Clone)]
pub enum LinkRow {
    Header(&'static str),
    Outgoing {
        text: String,
        resolved: Option<PathBuf>,
    },
    Backlink {
        note: Note,
    },
    /// A note that mentions the current note's title in plain text without
    /// linking to it (`wikilinks::unlinked_mentions`) — always jumpable,
    /// same as a backlink.
    Mention {
        note: Note,
    },
}

/// Builds the rows for `current`: its own outgoing links first (in the
/// order they appear in the body), then every other note that links back to
/// it. `notes` should be the *whole notebook* (`all_notes_recursive`), not
/// just the current folder — links can point anywhere in it. A section
/// with nothing in it is omitted entirely rather than shown with a header
/// and no rows under it.
pub fn build(current: &Note, notes: &[Note]) -> Vec<LinkRow> {
    let mut rows = Vec::new();

    let outgoing: Vec<LinkRow> = wikilinks::extract(&current.body)
        .into_iter()
        .map(|text| {
            let resolved = wikilinks::resolve_one(&text, notes);
            LinkRow::Outgoing { text, resolved }
        })
        .collect();
    if !outgoing.is_empty() {
        rows.push(LinkRow::Header("Outgoing"));
        rows.extend(outgoing);
    }

    let backlinks: Vec<LinkRow> = wikilinks::backlinks(&current.path, notes)
        .into_iter()
        .cloned()
        .map(|note| LinkRow::Backlink { note })
        .collect();
    if !backlinks.is_empty() {
        rows.push(LinkRow::Header("Backlinks"));
        rows.extend(backlinks);
    }

    let mentions: Vec<LinkRow> = wikilinks::unlinked_mentions(current, notes)
        .into_iter()
        .cloned()
        .map(|note| LinkRow::Mention { note })
        .collect();
    if !mentions.is_empty() {
        rows.push(LinkRow::Header("Mentions (unlinked)"));
        rows.extend(mentions);
    }

    rows
}

/// How many rows are actually selectable (everything but headers) — the
/// bound for a `selected` index into this list.
pub fn selectable_count(rows: &[LinkRow]) -> usize {
    rows.iter()
        .filter(|r| !matches!(r, LinkRow::Header(_)))
        .count()
}

/// The row index (into `rows`, headers included) of the `selected`-th
/// selectable row — what `ListState::select` needs to highlight the right
/// visual row, since headers are interspersed.
pub fn selected_row(rows: &[LinkRow], selected: usize) -> Option<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| !matches!(r, LinkRow::Header(_)))
        .nth(selected)
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiki_core::note::Frontmatter;

    fn note(path: &str, title: &str, body: &str) -> Note {
        Note::new(
            PathBuf::from(path),
            Frontmatter::new(title, "test"),
            body.to_string(),
        )
    }

    #[test]
    fn build_omits_empty_sections() {
        let current = note("a/lonely.md", "Lonely", "No links here at all.");
        let notes = vec![current.clone()];
        assert!(build(&current, &notes).is_empty());
    }

    #[test]
    fn build_includes_outgoing_and_backlinks_with_headers() {
        let current = note("a/hub.md", "Hub", "See [[Other]].");
        let other = note("a/other.md", "Other", "Back to [[Hub]].");
        let notes = vec![current.clone(), other];

        let rows = build(&current, &notes);

        assert!(matches!(rows[0], LinkRow::Header("Outgoing")));
        assert!(matches!(rows[1], LinkRow::Outgoing { .. }));
        assert!(matches!(rows[2], LinkRow::Header("Backlinks")));
        assert!(matches!(rows[3], LinkRow::Backlink { .. }));
        assert_eq!(selectable_count(&rows), 2);
    }

    #[test]
    fn selected_row_skips_headers() {
        let current = note("a/hub.md", "Hub", "See [[Other]].");
        let other = note("a/other.md", "Other", "Back to [[Hub]].");
        let notes = vec![current.clone(), other];
        let rows = build(&current, &notes);

        // Index 0 -> the Outgoing row at position 1 (after the header).
        assert_eq!(selected_row(&rows, 0), Some(1));
        // Index 1 -> the Backlink row at position 3 (after its own header).
        assert_eq!(selected_row(&rows, 1), Some(3));
        assert_eq!(selected_row(&rows, 2), None);
    }

    #[test]
    fn unlinked_mentions_get_their_own_section_after_backlinks() {
        let current = note("a/hub.md", "Hub", "");
        let linker = note("a/linker.md", "Linker", "See [[Hub]].");
        let mentioner = note("a/mentioner.md", "Mentioner", "the hub is central");
        let notes = vec![current.clone(), linker, mentioner];

        let rows = build(&current, &notes);

        assert!(matches!(rows[0], LinkRow::Header("Backlinks")));
        assert!(matches!(rows[1], LinkRow::Backlink { .. }));
        assert!(matches!(rows[2], LinkRow::Header("Mentions (unlinked)")));
        assert!(
            matches!(&rows[3], LinkRow::Mention { note } if note.frontmatter.title == "Mentioner")
        );
        assert_eq!(selectable_count(&rows), 2);
    }

    #[test]
    fn unresolved_outgoing_link_is_still_shown() {
        let current = note("a/hub.md", "Hub", "See [[Nowhere]].");
        let notes = vec![current.clone()];
        let rows = build(&current, &notes);
        assert!(matches!(
            &rows[1],
            LinkRow::Outgoing { resolved: None, text } if text == "Nowhere"
        ));
    }
}

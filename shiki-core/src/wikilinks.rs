use std::collections::HashMap;
use std::path::PathBuf;

use regex::Regex;
use std::sync::OnceLock;

use crate::note::Note;

/// Finds all `[[wikilinks]]` in a note's markdown body.
///
/// The captured target is the *base* name only — Obsidian-style
/// sub-addresses (`[[note#heading]]`, `[[note^block-id]]`) and display
/// aliases (`[[note|my text]]`) are consumed by the regex but not included,
/// since resolution is per-note and neither suffix affects which note a
/// link points at. `[[#heading]]` (a link into the *current* note) captures
/// an empty target, which resolves to nothing rather than accidentally
/// matching some other note's empty title.
pub fn extract(body: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"\[\[([^\[\]|#^]*)(?:[#^][^\[\]|]*)?(?:\|[^\[\]]+)?\]\]").unwrap()
    });
    re.captures_iter(body)
        .map(|c| c[1].trim().to_string())
        .collect()
}

/// Does this raw link target (as written between the brackets, possibly
/// with a `#heading`/`^block` suffix) refer to `name`, case-insensitively?
/// Comparison happens on the base part alone; the suffix is irrelevant for
/// identity.
fn split_link_base(link: &str) -> &str {
    match link.find(['#', '^']) {
        Some(pos) => &link[..pos],
        None => link,
    }
}

/// Resolves a single wikilink's target text to an existing note's path,
/// matched against every note in `notes` — not just a notebook's root, since
/// a link can point at a note nested in any folder. Tries an exact
/// (case-insensitive) title match first, then falls back to comparing
/// slugs, so `[[Weekend Hiking Trip]]` and `[[weekend-hiking-trip]]` both
/// resolve to the same note. A note's frontmatter aliases participate in
/// the same matching, so `[[its-old-name]]` still lands on a renamed note
/// that carries that alias.
pub fn resolve_one(link: &str, notes: &[Note]) -> Option<PathBuf> {
    let link = link.trim();
    let slug = Note::slugify(link);
    notes
        .iter()
        .find(|n| {
            n.frontmatter.title.eq_ignore_ascii_case(link)
                || n.file_stem() == slug
                || n.frontmatter.aliases.iter().any(|a| {
                    let alias = a.trim();
                    alias.eq_ignore_ascii_case(link) || Note::slugify(alias) == slug
                })
        })
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
        if note.frontmatter.title.eq_ignore_ascii_case(link)
            || note.file_stem() == slug
            || note
                .frontmatter
                .aliases
                .iter()
                .any(|a| a.trim().eq_ignore_ascii_case(link))
        {
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
/// Builds a title/slug/alias -> path index once up front rather than
/// calling `resolve_one` (a linear scan + `slugify` over every note) once per link
/// per note — that combination made this function cost O(notes × total
/// links in the notebook), visibly slow on a notebook with a few thousand
/// notes even though each note only has a handful of links.
pub fn backlinks<'a>(target: &std::path::Path, notes: &'a [Note]) -> Vec<&'a Note> {
    let mut by_title: HashMap<String, &std::path::Path> = HashMap::with_capacity(notes.len());
    let mut by_slug: HashMap<String, &std::path::Path> = HashMap::with_capacity(notes.len());
    let mut by_alias: HashMap<String, &std::path::Path> = HashMap::with_capacity(notes.len());
    for n in notes {
        by_title
            .entry(n.frontmatter.title.to_ascii_lowercase())
            .or_insert(&n.path);
        by_slug.entry(n.file_stem()).or_insert(&n.path);
        for alias in &n.frontmatter.aliases {
            let alias = alias.trim().to_ascii_lowercase();
            if !alias.is_empty() {
                by_alias.entry(alias).or_insert(&n.path);
            }
        }
    }
    let resolve = |link: &str| -> Option<&std::path::Path> {
        let base = split_link_base(link).trim();
        by_title
            .get(&base.to_ascii_lowercase())
            .or_else(|| by_slug.get(&Note::slugify(base)))
            .or_else(|| by_alias.get(&base.to_ascii_lowercase()))
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
/// linking to itself isn't a connection). Uses the same title/slug/alias
/// index `backlinks` builds, for the same O(notes × links) reason.
pub fn edges(notes: &[Note]) -> Vec<(usize, usize)> {
    let mut index: HashMap<String, usize> = HashMap::with_capacity(notes.len() * 2);
    for (i, n) in notes.iter().enumerate() {
        index
            .entry(n.frontmatter.title.to_ascii_lowercase())
            .or_insert(i);
        index.entry(n.file_stem()).or_insert(i);
        for alias in &n.frontmatter.aliases {
            let alias = alias.trim().to_ascii_lowercase();
            if !alias.is_empty() {
                index.entry(alias).or_insert(i);
            }
        }
    }
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (from, n) in notes.iter().enumerate() {
        for link in extract(&n.body) {
            let to = index
                .get(&link.to_ascii_lowercase())
                .or_else(|| index.get(Note::slugify(&link).as_str()));
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

/// Rewrites every `[[wikilink]]` in `pool` that points at a renamed note so
/// it points at the new title instead — the rename-note counterpart of
/// `tags::rename_tag`, run over the same kind of whole-store pool so links
/// from *other* notebooks are fixed too (cross-notebook resolution means
/// they'd otherwise dangle silently). `old_targets` is every name the note
/// used to answer to — pass both its old frontmatter title and its old
/// filename slug; a link matches when its base target (before any
/// `#heading`/`^block` suffix) equals one of them case-insensitively.
/// Display aliases (`[[target|text]]`) and suffixes are preserved verbatim;
/// only the target itself is swapped.
///
/// Files are rewritten as raw text rather than round-tripped through
/// `Note::save_with_crypto`, deliberately: a plain `.md` file that never
/// had frontmatter must not grow one just because it linked to the renamed
/// note. Encrypted notebooks work through the same path — the file is
/// decrypted with that notebook's key before matching and re-encrypted on
/// write, exactly like any other read-modify-write; an encrypted file with
/// no cached passphrase for its notebook is skipped (there's nothing safe
/// to do to it). Fenced code blocks are left alone — a `[[link]]` inside a
/// code example is text, not a link. Returns `(links_rewritten,
/// notes_updated)` across the whole pool plus the names of every notebook
/// that ended up touched (for per-notebook change tracking); a no-op is
/// `Ok((0, 0, vec![]))`.
pub fn rewrite_links_to(
    old_targets: &[String],
    new_title: &str,
    pool: &[(crate::Notebook, Note)],
) -> crate::Result<(usize, usize, Vec<String>)> {
    use crate::crypto::looks_encrypted;

    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // Same shape `extract` matches, but capturing everything around the
        // target so it can be stitched back on unchanged: the `[[` opener,
        // the base target, the optional `#heading`/`^block` suffix, the
        // optional `|display alias`, and the `]]` closer.
        Regex::new(r"(\[\[)([^\[\]|#^]*)([#^][^\[\]|]*)?(\|[^\[\]]+)?(\]\])").unwrap()
    });
    let old_lc: Vec<String> = old_targets
        .iter()
        .map(|t| t.trim().to_lowercase())
        .collect();
    let new_title = new_title.trim();
    let new_lc = new_title.to_lowercase();
    if old_lc.is_empty() || new_lc.is_empty() {
        return Ok((0, 0, Vec::new()));
    }

    let mut links_rewritten = 0usize;
    let mut notes_updated = 0usize;
    let mut notebooks_touched: Vec<String> = Vec::new();
    for (nb, note) in pool {
        let raw = match std::fs::read_to_string(&note.path) {
            Ok(raw) => raw,
            Err(_) => continue,
        };
        // Decrypt up front when needed — an encrypted notebook without its
        // passphrase cached this session can't be touched at all, so its
        // files are skipped rather than half-handled.
        let (text, was_encrypted) = if looks_encrypted(&raw) {
            let Some(crypto) = nb.crypto.as_ref() else {
                continue;
            };
            match crypto.decrypt(&raw) {
                Ok(plain) => (plain, true),
                Err(_) => continue,
            }
        } else {
            (raw, false)
        };

        // Same frontmatter-skip as `link_mention`: the YAML block never
        // contains body links.
        let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
        let mut start = 0;
        if lines.first().map(|l| l.trim_end_matches('\r')) == Some("---") {
            if let Some(close) = lines[1..]
                .iter()
                .position(|l| l.trim_end_matches('\r') == "---")
            {
                start = close + 2;
            }
        }

        let mut changed = false;
        let mut in_fence = false;
        for line in lines.iter_mut().skip(start) {
            if line.trim_start().starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence {
                continue;
            }
            let rewritten = re.replace_all(line.as_str(), |caps: &regex::Captures| {
                let target = caps[2].trim();
                let tl = target.to_lowercase();
                if !old_lc.contains(&tl) || tl == new_lc {
                    return caps[0].to_string();
                }
                links_rewritten += 1;
                format!(
                    "[[{new_title}{}{}]]",
                    caps.get(3).map(|m| m.as_str()).unwrap_or(""),
                    caps.get(4).map(|m| m.as_str()).unwrap_or("")
                )
            });
            if rewritten != line.as_str() {
                *line = rewritten.into_owned();
                changed = true;
            }
        }

        if changed {
            let out = if was_encrypted {
                nb.crypto
                    .as_ref()
                    .expect("checked above")
                    .encrypt(&lines.join("\n"))?
            } else {
                lines.join("\n")
            };
            std::fs::write(&note.path, out)?;
            notes_updated += 1;
            if !notebooks_touched.contains(&nb.name) {
                notebooks_touched.push(nb.name.clone());
            }
        }
    }
    Ok((links_rewritten, notes_updated, notebooks_touched))
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
    fn extract_strips_heading_block_and_alias_suffixes() {
        let body = "a [[Note#Section]] b [[Other^block-id]] c [[Third|label]] d [[#self]] e";
        assert_eq!(
            extract(body),
            vec![
                "Note".to_string(),
                "Other".to_string(),
                "Third".to_string(),
                String::new()
            ]
        );
    }

    fn note_with_aliases(path: &str, title: &str, aliases: &[&str], body: &str) -> Note {
        let mut n = note(path, title, body);
        n.frontmatter.aliases = aliases.iter().map(|a| a.to_string()).collect();
        n
    }

    #[test]
    fn resolve_one_matches_frontmatter_aliases() {
        let notes = vec![note_with_aliases(
            "a/hiking.md",
            "Weekend Hiking Trip",
            &["Old Name"],
            "",
        )];
        assert_eq!(
            resolve_one("old name", &notes),
            Some(PathBuf::from("a/hiking.md"))
        );
        // An alias's slug form resolves too, same as titles do.
        assert_eq!(
            resolve_one("old-name", &notes),
            Some(PathBuf::from("a/hiking.md"))
        );
        // And an unknown alias still misses.
        assert_eq!(resolve_one("other alias", &notes), None);
    }

    #[test]
    fn backlinks_count_alias_and_heading_links() {
        let target = note_with_aliases("a/hiking.md", "Weekend Hiking Trip", &["Old Name"], "");
        let linker = note(
            "a/journal.md",
            "Journal",
            "[[Old Name#Gear]] and [[weekend hiking trip]]",
        );
        let notes = vec![target.clone(), linker];
        assert_eq!(backlinks(&target.path, &notes).len(), 1);
    }

    /// Writes `contents` to a real file under `root` and returns the Note
    /// (path matching what's on disk) plus its notebook — the shape
    /// `rewrite_links_to`'s pool needs.
    fn on_disk(root: &std::path::Path, name: &str, contents: &str) -> (crate::Notebook, Note) {
        std::fs::create_dir_all(root).unwrap();
        let path = root.join(name);
        std::fs::write(&path, contents).unwrap();
        let nb = crate::Notebook::new("nb", root.to_path_buf());
        let n = note(path.to_str().unwrap(), "Whatever", "");
        (nb, n)
    }

    #[test]
    fn rewrite_links_to_rewrites_title_and_slug_preserving_suffix_and_alias() {
        let tmp = tempfile::tempdir().unwrap();
        let body = "---\ntitle: Journal\n---\n\nSee [[Old Name]], [[old-name|custom text]] \
                   and [[Old Name#Gear]].\n";
        let (nb, linker) = on_disk(tmp.path(), "journal.md", body);

        let (links, files, touched) = rewrite_links_to(
            &[String::from("Old Name"), String::from("old-name")],
            "New Name",
            &[(nb, linker)],
        )
        .unwrap();

        assert_eq!((links, files), (3, 1));
        assert_eq!(touched, vec!["nb".to_string()]);
        let out = std::fs::read_to_string(tmp.path().join("journal.md")).unwrap();
        assert_eq!(
            out,
            "---\ntitle: Journal\n---\n\nSee [[New Name]], [[New Name|custom text]] \
             and [[New Name#Gear]].\n"
        );
    }

    #[test]
    fn rewrite_links_to_skips_code_fences() {
        let tmp = tempfile::tempdir().unwrap();
        let body = "real: [[Old Name]]\n\n```md\nexample: [[Old Name]] stays\n```\n";
        let (nb, linker) = on_disk(tmp.path(), "journal.md", body);

        let (links, _, _) =
            rewrite_links_to(&[String::from("Old Name")], "New Name", &[(nb, linker)]).unwrap();

        assert_eq!(links, 1);
        let out = std::fs::read_to_string(tmp.path().join("journal.md")).unwrap();
        assert!(out.contains("real: [[New Name]]"));
        assert!(out.contains("example: [[Old Name]] stays"));
    }

    #[test]
    fn rewrite_links_to_never_adds_frontmatter_to_plain_files() {
        let tmp = tempfile::tempdir().unwrap();
        let body = "plain note linking [[Old Name]]\n";
        let (nb, linker) = on_disk(tmp.path(), "plain.md", body);

        let (_, files, _) =
            rewrite_links_to(&[String::from("Old Name")], "New Name", &[(nb, linker)]).unwrap();

        assert_eq!(files, 1);
        let out = std::fs::read_to_string(tmp.path().join("plain.md")).unwrap();
        assert_eq!(out, "plain note linking [[New Name]]\n");
    }

    #[test]
    fn rewrite_links_to_reencrypts_encrypted_notebooks_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let crypto = crate::crypto::NotebookCrypto::new("passphrase");
        let encrypted = crypto
            .encrypt("---\ntitle: Journal\n---\n\nlinking [[Old Name]] here\n")
            .unwrap();
        std::fs::write(tmp.path().join("journal.md"), &encrypted).unwrap();
        let nb =
            crate::Notebook::new("nb", tmp.path().to_path_buf()).with_crypto(Some(crypto.clone()));
        let linker = note(
            tmp.path().join("journal.md").to_str().unwrap(),
            "Journal",
            "",
        );

        let (links, files, _) =
            rewrite_links_to(&[String::from("Old Name")], "New Name", &[(nb, linker)]).unwrap();

        assert_eq!((links, files), (1, 1));
        let raw = std::fs::read_to_string(tmp.path().join("journal.md")).unwrap();
        assert!(crate::crypto::looks_encrypted(&raw));
        assert_eq!(
            crypto.decrypt(&raw).unwrap(),
            "---\ntitle: Journal\n---\n\nlinking [[New Name]] here\n"
        );
    }

    #[test]
    fn rewrite_links_to_skips_a_locked_notebook_without_touching_it() {
        let tmp = tempfile::tempdir().unwrap();
        let crypto = crate::crypto::NotebookCrypto::new("passphrase");
        let encrypted = crypto.encrypt("[[Old Name]]\n").unwrap();
        std::fs::write(tmp.path().join("locked.md"), &encrypted).unwrap();
        // No crypto attached — the notebook is locked this session.
        let nb = crate::Notebook::new("nb", tmp.path().to_path_buf());
        let linker = note(tmp.path().join("locked.md").to_str().unwrap(), "L", "");

        let before = std::fs::read_to_string(tmp.path().join("locked.md")).unwrap();
        let (links, files, _) =
            rewrite_links_to(&[String::from("Old Name")], "New Name", &[(nb, linker)]).unwrap();

        assert_eq!((links, files), (0, 0));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("locked.md")).unwrap(),
            before
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

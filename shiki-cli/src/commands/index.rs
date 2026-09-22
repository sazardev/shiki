//! `shiki index` — a structural overview (counts, folders, tags; never
//! note bodies or even titles) meant as the *first* call for an agent or
//! script orienting itself in an unfamiliar shiki setup, before deciding
//! what to actually page through with `list`/`search`/`query`. Always
//! small and bounded regardless of how large the underlying notebook(s)
//! are — folders and tags are capped to the busiest `TOP_N` each, with a
//! flag noting when something got cut, the same "never accidentally dump
//! everything" principle `paginate`/`page_json` apply to result lists.

use std::collections::HashMap;

use anyhow::Result;
use shiki_config::Config;
use shiki_core::{Notebook, NotebookStore};

use super::unlock_if_encrypted;

const TOP_N: usize = 25;

struct Count {
    name: String,
    count: usize,
}

struct Summary {
    name: String,
    note_count: usize,
    folders: Vec<Count>,
    folders_truncated: bool,
    tags: Vec<Count>,
    tags_truncated: bool,
    /// `Some(reason)` when this notebook couldn't be read at all (an
    /// encrypted notebook with no passphrase available) — only possible
    /// when scanning *every* notebook; a `-n <notebook>` request for a
    /// specific one propagates that same error instead, since silently
    /// reporting "locked" for the one notebook actually asked for would
    /// hide a real failure.
    locked: Option<String>,
}

pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: Option<&str>,
    json: bool,
) -> Result<()> {
    let explicit = notebook.is_some();
    let targets: Vec<Notebook> = match notebook {
        Some(name) => vec![super::get_notebook(store, name)?],
        None => store
            .list()?
            .into_iter()
            .filter(|nb| {
                !config
                    .notebooks
                    .get(&nb.name)
                    .is_some_and(|over| over.hidden)
            })
            .collect(),
    };

    let mut summaries = Vec::with_capacity(targets.len());
    for nb in targets {
        let name = nb.name.clone();
        match summarize(config, nb) {
            Ok(summary) => summaries.push(summary),
            Err(e) if !explicit => summaries.push(Summary {
                name,
                note_count: 0,
                folders: Vec::new(),
                folders_truncated: false,
                tags: Vec::new(),
                tags_truncated: false,
                locked: Some(e.to_string()),
            }),
            Err(e) => return Err(e),
        }
    }

    if json {
        let items: Vec<serde_json::Value> = summaries.iter().map(summary_json).collect();
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({ "notebooks": items }))?
        );
        return Ok(());
    }

    if summaries.is_empty() {
        println!("(no notebooks)");
        return Ok(());
    }
    for s in &summaries {
        print_summary(s);
    }
    Ok(())
}

fn summarize(config: &Config, nb: Notebook) -> Result<Summary> {
    let nb = unlock_if_encrypted(config, nb)?;
    let notes = nb.all_notes_recursive()?;

    let mut folder_counts: HashMap<String, usize> = HashMap::new();
    let mut tag_counts: HashMap<String, usize> = HashMap::new();
    for note in &notes {
        let folder = note
            .path
            .strip_prefix(&nb.path)
            .unwrap_or(&note.path)
            .parent()
            .map(|p| p.display().to_string().replace('\\', "/"))
            .unwrap_or_default();
        *folder_counts.entry(folder).or_insert(0) += 1;
        for tag in &note.frontmatter.tags {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
        }
    }

    let (folders, folders_truncated) = top_n(folder_counts);
    let (tags, tags_truncated) = top_n(tag_counts);

    Ok(Summary {
        name: nb.name,
        note_count: notes.len(),
        folders,
        folders_truncated,
        tags,
        tags_truncated,
        locked: None,
    })
}

/// Sorts by count descending (ties broken alphabetically, for a stable,
/// deterministic order run to run) and keeps only the busiest `TOP_N` —
/// the index stays small even for a notebook with hundreds of distinct
/// tags or folders.
fn top_n(counts: HashMap<String, usize>) -> (Vec<Count>, bool) {
    let mut all: Vec<Count> = counts
        .into_iter()
        .map(|(name, count)| Count { name, count })
        .collect();
    all.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    let truncated = all.len() > TOP_N;
    all.truncate(TOP_N);
    (all, truncated)
}

fn summary_json(s: &Summary) -> serde_json::Value {
    if let Some(reason) = &s.locked {
        return serde_json::json!({ "name": s.name, "locked": reason });
    }
    serde_json::json!({
        "name": s.name,
        "note_count": s.note_count,
        "folders": s.folders.iter().map(|c| serde_json::json!({"path": c.name, "note_count": c.count})).collect::<Vec<_>>(),
        "folders_truncated": s.folders_truncated,
        "tags": s.tags.iter().map(|c| serde_json::json!({"tag": c.name, "count": c.count})).collect::<Vec<_>>(),
        "tags_truncated": s.tags_truncated,
    })
}

fn print_summary(s: &Summary) {
    if let Some(reason) = &s.locked {
        println!("{}  (locked: {reason})", s.name);
        return;
    }
    println!("{}  ({} note(s))", s.name, s.note_count);
    if !s.folders.is_empty() {
        let suffix = if s.folders_truncated { "+" } else { "" };
        print!("  folders:");
        for c in &s.folders {
            let label = if c.name.is_empty() { "." } else { &c.name };
            print!("  {label} ({}{suffix})", c.count);
        }
        println!();
    }
    if !s.tags.is_empty() {
        let suffix = if s.tags_truncated { "+" } else { "" };
        print!("  tags:");
        for c in &s.tags {
            print!("  {}({}{suffix})", c.name, c.count);
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_n_sorts_by_count_descending_then_alphabetically() {
        let mut counts = HashMap::new();
        counts.insert("idea".to_string(), 3);
        counts.insert("work".to_string(), 5);
        counts.insert("home".to_string(), 3);

        let (top, truncated) = top_n(counts);

        assert!(!truncated);
        let names: Vec<&str> = top.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["work", "home", "idea"]);
    }

    #[test]
    fn top_n_caps_at_top_n_and_reports_truncation() {
        let counts: HashMap<String, usize> = (0..(TOP_N + 5))
            .map(|i| (format!("tag-{i:03}"), i))
            .collect();

        let (top, truncated) = top_n(counts);

        assert!(truncated);
        assert_eq!(top.len(), TOP_N);
        // Busiest first: tag-(TOP_N+4) has the highest count.
        assert_eq!(top[0].name, format!("tag-{:03}", TOP_N + 4));
    }
}

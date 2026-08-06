//! Flattens a notebook's whole folder tree for the tree view
//! (`Action::ToggleTreeView`) — every folder and note, fully expanded, in
//! one scrollable list instead of navigating one level at a time.

use std::path::Path;

use shiki_core::{Note, Notebook};

/// One row in the flattened tree: a folder header (display only — selection
/// always skips these) or a note at some nesting `depth`, for indentation.
#[derive(Debug, Clone)]
pub enum TreeRow {
    Folder { depth: usize, name: String },
    Note { depth: usize, note: Box<Note> },
}

/// Depth-first flatten of `nb`'s entire tree, folders (and everything under
/// them) before the notes at that same level — same per-level ordering the
/// Notes panel normally uses, just applied at every depth instead of one.
pub fn build(nb: &Notebook) -> Vec<TreeRow> {
    let mut out = Vec::new();
    build_at(nb, Path::new(""), 0, &mut out);
    out
}

fn build_at(nb: &Notebook, relative: &Path, depth: usize, out: &mut Vec<TreeRow>) {
    let Ok((folders, notes)) = nb.list_dir(relative) else {
        return;
    };
    for folder in &folders {
        out.push(TreeRow::Folder {
            depth,
            name: folder.clone(),
        });
        build_at(nb, &relative.join(folder), depth + 1, out);
    }
    for note in notes {
        out.push(TreeRow::Note {
            depth,
            note: Box::new(note),
        });
    }
}

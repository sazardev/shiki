use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use ratatui::style::Color;
use ratatui::text::Line;

/// Cached preview for the currently selected note — the result of the full
/// `markdown_to_lines` + wrapping + metadata-header pipeline, keyed by the
/// note's path, the active theme's colors, the panel's content width, and the
/// session's `<details>` fold state. Replacing the previous `(PathBuf,
/// [Color;8], u16, HashSet<usize>, Vec<Line>, Vec<Option<usize>>, HashMap)`
/// tuple so field access is named (`cache.path` vs `cache.0`) and the
/// cache-key check reads as `cache.path == path && cache.colors == colors`.
///
/// Equality is checked field-by-field in `App::refresh_note_preview_cache`;
/// the struct itself is not `PartialEq` to avoid accidentally comparing the
/// rendered `lines`/`sources` (heavy) when only the key matters.
#[derive(Debug, Clone)]
pub(crate) struct NotePreviewCache {
    pub(crate) path: PathBuf,
    pub(crate) colors: [Color; 8],
    pub(crate) width: u16,
    pub(crate) folded: HashSet<usize>,
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) sources: Vec<Option<usize>>,
    pub(crate) summary_blocks: HashMap<usize, usize>,
}

impl NotePreviewCache {
    pub(crate) fn new(
        path: PathBuf,
        colors: [Color; 8],
        width: u16,
        folded: HashSet<usize>,
        lines: Vec<Line<'static>>,
        sources: Vec<Option<usize>>,
        summary_blocks: HashMap<usize, usize>,
    ) -> Self {
        Self {
            path,
            colors,
            width,
            folded,
            lines,
            sources,
            summary_blocks,
        }
    }

    /// Whether the cached render is still valid for the given key.
    pub(crate) fn is_valid_for(
        &self,
        path: &PathBuf,
        colors: &[Color; 8],
        width: u16,
        folded: &HashSet<usize>,
    ) -> bool {
        &self.path == path
            && &self.colors == colors
            && self.width == width
            && &self.folded == folded
    }
}

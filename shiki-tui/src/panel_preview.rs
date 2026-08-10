use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Focus};
use crate::icons;
use crate::render::{borrow_lines, hex_to_color, panel_block, panel_block_reading};

/// A small read-only header — tags (one combined line), then every custom
/// frontmatter field (`status`, `priority`, whatever a note's own YAML
/// happens to carry, via `Frontmatter::extra`) one per line — prepended to
/// the note's body by `App::refresh_note_preview_cache`. Deliberately part
/// of the same scrollable, wrapped, cached document as the body rather than
/// a separate fixed-height area above it: reusing the exact same row/scroll/
/// mouse-hit-test math the body already has (`preview_row_at`,
/// `note_preview_source_line`) means this needed no new offset arithmetic
/// anywhere, at the cost of the header scrolling away with the rest of the
/// note instead of staying pinned — an acceptable trade for a modal-free,
/// always-visible view of metadata that previously had no in-app view at
/// all (only visible before by opening the raw file in an external editor).
/// Empty when there's nothing to show (no tags, no extra fields) — the
/// common case, which must look exactly like a note with no header at all.
pub(crate) fn metadata_lines(
    note: &shiki_core::Note,
    muted: Color,
    tag_color: Color,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !note.frontmatter.tags.is_empty() {
        let mut spans = vec![Span::styled(
            format!("{} ", icons::TAG),
            Style::default().fg(tag_color),
        )];
        for (i, t) in note.frontmatter.tags.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(
                format!("#{t}"),
                Style::default().fg(tag_color),
            ));
        }
        lines.push(Line::from(spans));
    }
    for (k, v) in note.frontmatter.extra.iter() {
        let Some(key) = k.as_str() else { continue };
        let val = crate::panel_query::yaml_cell_text(v);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{key}: "),
                Style::default().fg(muted).add_modifier(Modifier::ITALIC),
            ),
            Span::styled(val, Style::default().fg(muted)),
        ]));
    }
    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Preview;
    let muted = hex_to_color(&app.theme.muted);

    let title: Line = match (app.selected_note(), app.selected_folder()) {
        (Some(n), _) => Line::from(vec![
            Span::raw(format!(" {}{}  ", icons::EYE, n.frontmatter.title)),
            Span::styled(
                format!("({}) ", n.frontmatter.date.format("%Y-%m-%d")),
                Style::default().fg(muted),
            ),
        ]),
        (None, Some(folder)) => Line::from(format!(" {}{folder}/ ", icons::NOTEBOOK)),
        (None, None) => Line::from(format!(" {}Preview ", icons::EYE)),
    };
    let block = if focused && app.selected_note().is_some() {
        panel_block_reading(title, &app.theme)
    } else {
        panel_block(title, focused, &app.theme)
    };

    let mut lines = match (app.selected_note(), app.selected_folder()) {
        // Both branches read from an `App`-level cache
        // (`note_preview_lines`/`folder_preview_lines`) that only re-does
        // the real work — `markdown_to_lines`'s full pass over the note
        // body, or `format_folder_entries`'s per-row `format!` calls —
        // when the selection or theme colors actually changed, not on
        // every one of these render calls. `note_preview_lines` is already
        // pre-wrapped to the panel's content width (see
        // `App::refresh_note_preview_cache`), so this `Paragraph` displays
        // rows verbatim instead of asking ratatui to wrap them again.
        (Some(_), _) => borrow_lines(app.note_preview_lines().unwrap_or(&[])),
        (None, Some(_)) => borrow_lines(app.folder_preview_lines().unwrap_or(&[])),
        (None, None) => vec![Line::from(Span::styled(
            "No notes yet in this notebook — press `a` to create one.",
            Style::default().fg(muted).add_modifier(Modifier::ITALIC),
        ))],
    };

    // Drag-selection highlight — only meaningful over a note's own body,
    // never the folder-peek/empty-notebook placeholders, since
    // `preview_selection` is only ever populated by a click that already
    // hit-tested against `note_preview_lines`.
    if let (Some(_), Some(selection)) = (app.selected_note(), &app.preview_selection) {
        let start = selection.anchor_row.min(selection.current_row);
        let end = selection.anchor_row.max(selection.current_row);
        let selection_bg = hex_to_color(&app.theme.selection);
        for line in lines.iter_mut().take(end + 1).skip(start) {
            for span in &mut line.spans {
                span.style = span.style.bg(selection_bg);
            }
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((app.preview_scroll, 0));
    frame.render_widget(paragraph, area);
}

/// Maps a mouse event at `(column, row)` to a document-row index into the
/// panel's pre-wrapped rows (see `App::refresh_note_preview_cache`), or
/// `None` if the position falls on/outside the panel's border, or past the
/// last real row. `scroll` is `App::preview_scroll`; `row_count` is
/// `app.note_preview_lines().len()`, so dragging past the end of a short
/// note doesn't resolve to a phantom trailing row. Plain function of
/// coordinates/counts, not `&App`, same convention as
/// `panel_drawer::drawer_hit_at`/`status_bar::coffee_hit_at`.
pub fn preview_row_at(
    area: Rect,
    scroll: u16,
    row_count: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    let content_left = area.x + 1;
    let content_right = area.x + area.width.saturating_sub(1);
    let content_top = area.y + 1;
    let content_bottom = area.y + area.height.saturating_sub(1);
    if column < content_left
        || column >= content_right
        || row < content_top
        || row >= content_bottom
    {
        return None;
    }
    let doc_row = scroll as usize + (row - content_top) as usize;
    (doc_row < row_count).then_some(doc_row)
}

/// Given one already-rendered PREVIEW row (from `App::note_preview_lines`)
/// and a column offset within it (0-based from the panel's own left content
/// edge — the caller subtracts `preview_row_at`'s `content_left` first),
/// resolves which `[[wikilink]]` span, if any, the column lands on —
/// returning its inner text with the brackets stripped. `inline_spans`
/// (render.rs) renders a wikilink as a single span whose *content* is the
/// literal `[[Title]]` text; that's the one thing distinguishing it from an
/// ordinary `[label](url)` link or an image placeholder, which share the
/// exact same `link` style but never carry doubled brackets in their span
/// content. Plain function of a `Line`/column, not `&App`, same convention
/// as `preview_row_at`/`panel_drawer::drawer_hit_at`.
pub fn wikilink_at(line: &Line<'_>, column: usize) -> Option<String> {
    let mut start = 0usize;
    for span in &line.spans {
        let width = unicode_width::UnicodeWidthStr::width(span.content.as_ref());
        if column >= start && column < start + width {
            return span
                .content
                .strip_prefix("[[")
                .and_then(|s| s.strip_suffix("]]"))
                .map(str::to_string);
        }
        start += width;
    }
    None
}

/// What's inside a selected-but-not-entered folder, so landing on it already
/// shows what you'd find there instead of just a "press enter" hint — a
/// quick peek, same spirit as the note preview itself. Pure formatting, no
/// `App` access — called from `App::refresh_folder_preview_cache` to build
/// the cached lines, not from `render` directly (see there for why).
pub(crate) fn format_folder_entries(
    subfolders: &[String],
    note_titles: &[String],
    fg: Color,
    accent: Color,
    muted: Color,
) -> Vec<Line<'static>> {
    if subfolders.is_empty() && note_titles.is_empty() {
        return vec![Line::from(Span::styled(
            "Empty folder.".to_string(),
            Style::default().fg(muted).add_modifier(Modifier::ITALIC),
        ))];
    }

    let mut lines = Vec::with_capacity(subfolders.len() + note_titles.len());
    for name in subfolders {
        lines.push(Line::from(Span::styled(
            format!("{}{name}/", icons::NOTEBOOK),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        )));
    }
    for title in note_titles {
        lines.push(Line::from(Span::styled(
            format!("{}{title}", icons::NOTE),
            Style::default().fg(fg),
        )));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::new(0, 0, 20, 10)
    }

    #[test]
    fn top_left_content_cell_hits_row_zero() {
        let a = area();
        assert_eq!(preview_row_at(a, 0, 5, a.x + 1, a.y + 1), Some(0));
    }

    #[test]
    fn scroll_offsets_the_hit_row() {
        let a = area();
        assert_eq!(preview_row_at(a, 3, 20, a.x + 1, a.y + 1), Some(3));
        assert_eq!(preview_row_at(a, 3, 20, a.x + 1, a.y + 2), Some(4));
    }

    #[test]
    fn clicking_the_border_misses() {
        let a = area();
        assert_eq!(preview_row_at(a, 0, 5, a.x, a.y + 1), None);
        assert_eq!(preview_row_at(a, 0, 5, a.x + 1, a.y), None);
        assert_eq!(preview_row_at(a, 0, 5, a.x + a.width - 1, a.y + 1), None);
        assert_eq!(preview_row_at(a, 0, 5, a.x + 1, a.y + a.height - 1), None);
    }

    #[test]
    fn past_the_last_row_misses() {
        let a = area();
        assert_eq!(preview_row_at(a, 0, 3, a.x + 1, a.y + 4), None);
    }

    #[test]
    fn empty_document_never_hits() {
        let a = area();
        assert_eq!(preview_row_at(a, 0, 0, a.x + 1, a.y + 1), None);
    }
}

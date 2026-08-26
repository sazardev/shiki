use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

use crate::app::{App, Focus};
use crate::icons;
use crate::render::{hex_to_color, panel_block, render_scrollbar};
use shiki_core::git::FileGitStatus;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Notes;
    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);
    let total = app.folders.len() + app.notes.len();
    // A softer tint than the cursor's own `highlight_style` (below) —
    // every row in a `Mode::Visual` range gets this on its own `ListItem`,
    // so the whole selection reads as a band while the actual cursor row
    // still stands out more, via `highlight_style` overriding this for
    // whichever one row `ListState::select` points at.
    let visual_bg = hex_to_color(&app.theme.selection);

    let items: Vec<ListItem> = if total == 0 {
        vec![ListItem::new("  no notes yet — press `a` to create one")
            .style(Style::default().fg(muted))]
    } else {
        app.folders
            .iter()
            .enumerate()
            .map(|(idx, name)| {
                let mut style = Style::default()
                    .fg(hex_to_color(&app.theme.accent))
                    .add_modifier(Modifier::BOLD);
                if app.is_visually_selected(idx) {
                    style = style.bg(visual_bg);
                }
                ListItem::new(format!("{}{name}/", icons::NOTEBOOK)).style(style)
            })
            .chain(app.notes.iter().enumerate().map(|(note_idx, note)| {
                let idx = app.folders.len() + note_idx;
                // Colored by the same-priority scheme as the drawer/footer's
                // per-notebook status, just per-file: at a glance, which
                // notes are new/changed without opening the history modal
                // or reading the footer's aggregate `{n} changes` count.
                let title_color = match app.note_statuses.get(&note.path) {
                    Some(FileGitStatus::New) => hex_to_color(&app.theme.success),
                    Some(FileGitStatus::Modified) | Some(FileGitStatus::Renamed) => {
                        hex_to_color(&app.theme.warning)
                    }
                    Some(FileGitStatus::Deleted) => hex_to_color(&app.theme.error),
                    None => fg,
                };
                let mut spans = vec![ratatui::text::Span::styled(
                    format!("{}{}", icons::NOTE, note.frontmatter.title),
                    Style::default().fg(title_color),
                )];
                // The explicit marker beside the color: the selected row's
                // `highlight_style` repaints every span in the accent color,
                // which would otherwise erase the only dirty cue on exactly
                // the row the user is looking at. The glyph survives that
                // repaint (same reason `ARROW`'s fallback is real).
                if app.note_statuses.contains_key(&note.path) {
                    spans.push(ratatui::text::Span::styled(
                        format!(" {}", icons::DIRTY),
                        Style::default().fg(title_color),
                    ));
                }
                if app.show_dates {
                    spans.push(ratatui::text::Span::styled(
                        format!("  ({})", note.frontmatter.date.format("%Y-%m-%d")),
                        Style::default().fg(muted),
                    ));
                }
                let item = ListItem::new(Line::from(spans));
                if app.is_visually_selected(idx) {
                    item.style(Style::default().bg(visual_bg))
                } else {
                    item
                }
            }))
            .collect()
    };

    let breadcrumb = app
        .notes_breadcrumb()
        .map(|b| format!(" {b}"))
        .unwrap_or_default();
    let title = format!(
        " {}Notes{breadcrumb} [{}]  {}[/] search ",
        icons::NOTE,
        app.notes.len(),
        icons::SEARCH
    );
    let highlight_symbol = format!("{}", icons::ARROW);
    let list = List::new(items)
        .block(panel_block(Line::from(title), focused, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    if total != 0 {
        state.select(Some(app.selected_note));
    }
    frame.render_stateful_widget(list, area, &mut state);
    render_scrollbar(frame, area, total, app.selected_note, &app.theme);
}

/// Maps a click's `(column, row)` to a combined folders++notes index (the
/// same indexing `App::selected_note`/`selected_folder` already use — a
/// folder is `idx < folder_count`, a note is `idx - folder_count`), or
/// `None` if it missed. Same plain-coordinates shape as
/// `panel_drawer::drawer_hit_at`/`panel_notebooks::notebooks_hit_at`.
pub fn notes_hit_at(total: usize, area: Rect, column: u16, row: u16) -> Option<usize> {
    if column < area.x || column >= area.x + area.width {
        return None;
    }
    let list_top = area.y + 1;
    let list_bottom = area.y + area.height.saturating_sub(1);
    if row < list_top || row >= list_bottom {
        return None;
    }
    let index = (row - list_top) as usize;
    (index < total).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::new(0, 0, 40, 20)
    }

    #[test]
    fn clicking_a_row_hits_its_index() {
        assert_eq!(notes_hit_at(5, area(), 5, 1), Some(0));
        assert_eq!(notes_hit_at(5, area(), 5, 5), Some(4));
    }

    #[test]
    fn clicking_past_the_last_row_misses() {
        assert_eq!(notes_hit_at(5, area(), 5, 6), None);
    }

    #[test]
    fn clicking_the_border_or_outside_the_area_misses() {
        assert_eq!(notes_hit_at(5, area(), 5, 0), None);
        assert_eq!(notes_hit_at(5, area(), 5, 19), None);
        assert_eq!(notes_hit_at(5, area(), 45, 1), None);
    }
}

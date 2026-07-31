use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

use crate::app::App;
use crate::icons;
use crate::render::{git_status_color, git_status_suffix, hex_to_color, panel_block};

/// Row height reserved at the bottom of the drawer for the button strip —
/// keep in sync with `render`'s `Layout::vertical` split.
const BUTTON_ROWS: u16 = 2;

/// What a mouse click inside the drawer landed on — mirrors
/// `App::handle_drawer_key`'s `Enter`/`n`/`i` branches exactly, so
/// `App::on_mouse` can dispatch a click the same way a keypress would.
pub enum DrawerHit {
    Notebook(usize),
    NewButton,
    ImportButton,
}

/// Left-side sidebar (`leader+b`): every notebook's git status in color,
/// plus two minimal, unboxed "buttons" for New/Import — the mouse-click
/// counterpart of `App::handle_drawer_key`'s `n`/`i`.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let muted = hex_to_color(&app.theme.muted);

    let [list_area, buttons_area] = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Min(1),
        ratatui::layout::Constraint::Length(BUTTON_ROWS),
    ])
    .areas(area);

    let items: Vec<ListItem> = if app.drawer_statuses.is_empty() {
        vec![ListItem::new("  no notebooks yet").style(Style::default().fg(muted))]
    } else {
        app.drawer_statuses
            .iter()
            .map(|(name, gs)| {
                let color = git_status_color(&app.theme, gs);
                let suffix = git_status_suffix(gs);
                ListItem::new(Line::from(Span::styled(
                    format!("{}  {name}{suffix}", icons::NOTEBOOK),
                    Style::default().fg(color),
                )))
            })
            .collect()
    };

    let title = format!(
        " {}  Notebooks [{}] ",
        icons::NOTEBOOK,
        app.drawer_statuses.len()
    );
    let highlight_symbol = format!("{} ", icons::ARROW);
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    if !app.drawer_statuses.is_empty() {
        state.select(Some(app.drawer_selected));
    }
    frame.render_stateful_widget(list, list_area, &mut state);

    let buttons = Line::from(vec![
        Span::styled("  [n] New  ", Style::default().fg(muted)),
        Span::styled("[i] Import", Style::default().fg(muted)),
    ]);
    frame.render_widget(ratatui::widgets::Paragraph::new(buttons), buttons_area);
}

/// Maps a click at `(column, row)` inside the drawer to whatever it landed
/// on — `area` must be the exact rect `render` was called with (same
/// "shared by render and hit-testing" convention as
/// `App::global_search_hit_at`/`global_search_popup_area`), so the two
/// never disagree about where things are. Takes `notebook_count` rather
/// than `&App` since that's the only piece of `App` state this actually
/// needs — a plain function of numbers is trivial to unit-test without
/// constructing a full `App`.
pub fn drawer_hit_at(
    notebook_count: usize,
    area: Rect,
    column: u16,
    row: u16,
) -> Option<DrawerHit> {
    if column < area.x || column >= area.x + area.width {
        return None;
    }
    // The button row is the *first* row of the `Length(BUTTON_ROWS)` area
    // `render`'s `Layout::vertical` reserves at the bottom — a `Paragraph`
    // with one line of unwrapped text top-aligns within its area, it
    // doesn't center vertically, so there's no offset to add here.
    let buttons_row = area.y + area.height.saturating_sub(BUTTON_ROWS);
    if row == buttons_row {
        // Matches the button strip's own layout: "  [n] New  [i] Import".
        return if column < area.x + 13 {
            Some(DrawerHit::NewButton)
        } else {
            Some(DrawerHit::ImportButton)
        };
    }

    // List rows start one line down (the panel's top border) and are
    // 1:1 with `app.drawer_statuses` — no wrapping, one notebook per row.
    let list_top = area.y + 1;
    if row < list_top || row >= buttons_row {
        return None;
    }
    let index = (row - list_top) as usize;
    (index < notebook_count).then_some(DrawerHit::Notebook(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        // Same shape `App::drawer_area` produces: full height minus the
        // status bar row, at a fixed width.
        Rect::new(0, 0, 30, 40)
    }

    #[test]
    fn clicking_a_notebook_row_hits_its_index() {
        let a = area();
        // Row 0 is the top border, row 1 is the first notebook.
        assert!(matches!(
            drawer_hit_at(3, a, 5, 1),
            Some(DrawerHit::Notebook(0))
        ));
        assert!(matches!(
            drawer_hit_at(3, a, 5, 3),
            Some(DrawerHit::Notebook(2))
        ));
    }

    #[test]
    fn clicking_past_the_last_notebook_row_misses() {
        let a = area();
        assert!(drawer_hit_at(2, a, 5, 3).is_none());
    }

    #[test]
    fn clicking_the_button_row_hits_new_or_import() {
        let a = area();
        let buttons_row = a.y + a.height - BUTTON_ROWS;
        assert!(matches!(
            drawer_hit_at(3, a, a.x + 2, buttons_row),
            Some(DrawerHit::NewButton)
        ));
        assert!(matches!(
            drawer_hit_at(3, a, a.x + 20, buttons_row),
            Some(DrawerHit::ImportButton)
        ));
    }

    #[test]
    fn clicking_outside_the_drawer_column_range_misses() {
        let a = area();
        assert!(drawer_hit_at(3, a, a.x + a.width, 1).is_none());
        assert!(drawer_hit_at(3, a, a.x.wrapping_sub(1), 1).is_none());
    }
}

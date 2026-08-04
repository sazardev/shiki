use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

use crate::app::{App, Focus};
use crate::icons;
use crate::render::{hex_to_color, panel_block, render_scrollbar};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Notebooks;
    let fg = hex_to_color(&app.theme.fg);

    let items: Vec<ListItem> = if app.notebooks.is_empty() {
        vec![ListItem::new("  press `a` to create one")
            .style(Style::default().fg(hex_to_color(&app.theme.muted)))]
    } else {
        app.notebooks
            .iter()
            .map(|nb| {
                ListItem::new(format!("{}  {}", icons::NOTEBOOK, nb.name))
                    .style(Style::default().fg(fg))
            })
            .collect()
    };

    let count = app.notebooks.len();
    let title = if count == 0 {
        format!(" {}  Notebooks ", icons::NOTEBOOK)
    } else {
        format!(
            " {}  Notebooks [{}/{count}] ",
            icons::NOTEBOOK,
            app.selected_notebook + 1
        )
    };
    let highlight_symbol = format!("{} ", icons::ARROW);
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
    if !app.notebooks.is_empty() {
        state.select(Some(app.selected_notebook));
    }
    frame.render_stateful_widget(list, area, &mut state);
    render_scrollbar(
        frame,
        area,
        app.notebooks.len(),
        app.selected_notebook,
        &app.theme,
    );
}

/// Maps a click's `(column, row)` to a notebook index, or `None` if it
/// missed the list entirely (outside the panel, on a border, or past the
/// last row) — same plain-coordinates shape as `panel_drawer::drawer_hit_at`,
/// so it's unit-testable without constructing a full `App`. List rows start
/// one line down (the panel's top border) and are 1:1 with `app.notebooks`,
/// no wrapping, one notebook per row — identical layout convention to the
/// drawer's own list.
pub fn notebooks_hit_at(count: usize, area: Rect, column: u16, row: u16) -> Option<usize> {
    if column < area.x || column >= area.x + area.width {
        return None;
    }
    let list_top = area.y + 1;
    let list_bottom = area.y + area.height.saturating_sub(1);
    if row < list_top || row >= list_bottom {
        return None;
    }
    let index = (row - list_top) as usize;
    (index < count).then_some(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::new(0, 0, 40, 20)
    }

    #[test]
    fn clicking_a_notebook_row_hits_its_index() {
        assert_eq!(notebooks_hit_at(3, area(), 5, 1), Some(0));
        assert_eq!(notebooks_hit_at(3, area(), 5, 3), Some(2));
    }

    #[test]
    fn clicking_past_the_last_row_misses() {
        assert_eq!(notebooks_hit_at(3, area(), 5, 4), None);
    }

    #[test]
    fn clicking_the_border_or_outside_the_area_misses() {
        assert_eq!(notebooks_hit_at(3, area(), 5, 0), None);
        assert_eq!(notebooks_hit_at(3, area(), 5, 19), None);
        assert_eq!(notebooks_hit_at(3, area(), 45, 1), None);
    }
}

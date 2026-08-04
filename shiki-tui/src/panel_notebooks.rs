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

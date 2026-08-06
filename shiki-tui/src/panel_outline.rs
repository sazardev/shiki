use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

use crate::app::App;
use crate::icons;
use crate::render::{hex_to_color, panel_block};

/// Outline/heading-jump popup (PREVIEW-scope `o`, or `Ctrl+O` inside
/// `Mode::Edit`) — a flat list of the selected note's `#`..`######`
/// headings (`shiki_core::headings::extract`), indented by level. Read-only
/// rendering; `App::handle_outline_key` drives selection/close/jump.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);

    let items: Vec<ListItem> = if app.outline_headings.is_empty() {
        vec![ListItem::new("  no headings in this note").style(Style::default().fg(muted))]
    } else {
        app.outline_headings
            .iter()
            .map(|h| {
                let indent = "  ".repeat((h.level.saturating_sub(1)) as usize);
                ListItem::new(format!(
                    "{indent}{} {}",
                    "#".repeat(h.level as usize),
                    h.text
                ))
                .style(Style::default().fg(fg))
            })
            .collect()
    };

    let title = format!(" {}Outline [{}] ", icons::TREE, app.outline_headings.len());
    let highlight_symbol = format!("{}", icons::ARROW);
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
    state.select(Some(app.outline_selected));
    frame.render_stateful_widget(list, area, &mut state);
}

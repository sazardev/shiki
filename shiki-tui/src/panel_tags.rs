use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

use crate::app::App;
use crate::icons;
use crate::render::{hex_to_color, panel_block};

/// Tag-filtering popup (leader+`T`) — two levels: the tag list itself
/// (`app.tags_viewing.is_none()`), and after drilling into one (`Enter`),
/// the notes that carry it. `App::handle_tags_key` drives which level this
/// renders; this function only reads state, never mutates it.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let fg = hex_to_color(&app.theme.fg);
    let tag_color = hex_to_color(&app.theme.tag);
    let muted = hex_to_color(&app.theme.muted);

    let (title, items, selected): (String, Vec<ListItem>, usize) = match &app.tags_viewing {
        None => {
            let tags = app.tag_index();
            let items = tags
                .tags()
                .map(|tag| {
                    ListItem::new(format!(
                        "{} {tag} ({})",
                        icons::TAG,
                        tags.notes_for(tag).len()
                    ))
                    .style(Style::default().fg(fg))
                })
                .collect();
            (
                format!(" {}  Tags [{}] ", icons::TAG, tags.len()),
                items,
                app.tags_selected,
            )
        }
        Some(tag) => {
            let notes: Vec<_> = app
                .notes
                .iter()
                .filter(|n| n.frontmatter.tags.iter().any(|t| t == tag))
                .collect();
            let items = if notes.is_empty() {
                vec![ListItem::new("  no notes with this tag here")
                    .style(Style::default().fg(muted))]
            } else {
                notes
                    .iter()
                    .map(|n| {
                        ListItem::new(format!("{}  {}", icons::NOTE, n.frontmatter.title))
                            .style(Style::default().fg(fg))
                    })
                    .collect()
            };
            (
                format!(" {}  {tag} [{}]  (h/esc back) ", icons::TAG, notes.len()),
                items,
                app.tags_notes_selected,
            )
        }
    };

    let highlight_symbol = format!("{} ", icons::ARROW);
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(tag_color)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

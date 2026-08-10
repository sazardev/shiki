use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::icons;
use crate::render::{hex_to_color, panel_block};

/// Tag-filtering popup (leader+`T`) — two levels: the tag list itself
/// (`app.tags_viewing.is_none()`), and after drilling into one (`Enter`),
/// the notes that carry it. `App::handle_tags_key` drives which level this
/// renders; this function only reads state, never mutates it. Level 1 gets
/// a dedicated hint footer row for `r` (rename/merge) — same "don't cram
/// shortcuts into the title" fix `panel_metadata` needed: a title-embedded
/// hint gets silently truncated by the block's own border the moment the
/// title is long enough, which is exactly why `a` (add a metadata field)
/// went unnoticed for a while. Level 2 has no actions of its own beyond
/// navigating/jumping, so it keeps the plain single-block layout.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let fg = hex_to_color(&app.theme.fg);
    let tag_color = hex_to_color(&app.theme.tag);
    let muted = hex_to_color(&app.theme.muted);

    let Some(tag) = &app.tags_viewing else {
        let tags = app.tag_index();
        let items: Vec<ListItem> = tags
            .tags()
            .map(|tag| {
                ListItem::new(format!(
                    "{}{tag} ({})",
                    icons::TAG,
                    tags.notes_for(tag).len()
                ))
                .style(Style::default().fg(fg))
            })
            .collect();
        let title = format!(" {}Tags [{}] ", icons::TAG, tags.len());

        let block = panel_block(Line::from(title), true, &app.theme);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let [list_area, hint_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(hex_to_color(&app.theme.selection))
                    .fg(tag_color)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(format!("{} ", icons::ARROW));
        let mut state = ListState::default();
        if !tags.is_empty() {
            state.select(Some(app.tags_selected));
        }
        frame.render_stateful_widget(list, list_area, &mut state);

        let key_style = Style::default().fg(tag_color).add_modifier(Modifier::BOLD);
        let label_style = Style::default().fg(muted);
        let hint = Line::from(vec![
            Span::styled("enter", key_style),
            Span::styled(" open   ", label_style),
            Span::styled("r", key_style),
            Span::styled(" rename/merge   ", label_style),
            Span::styled("esc", key_style),
            Span::styled(" close", label_style),
        ]);
        frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), hint_area);
        return;
    };

    let notes: Vec<_> = app
        .notes
        .iter()
        .filter(|n| n.frontmatter.tags.iter().any(|t| t == tag))
        .collect();
    let items = if notes.is_empty() {
        vec![ListItem::new("  no notes with this tag here").style(Style::default().fg(muted))]
    } else {
        notes
            .iter()
            .map(|n| {
                ListItem::new(format!("{}{}", icons::NOTE, n.frontmatter.title))
                    .style(Style::default().fg(fg))
            })
            .collect()
    };
    let title = format!(" {}{tag} [{}]  (h/esc back) ", icons::TAG, notes.len());

    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(tag_color)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(format!("{} ", icons::ARROW));

    let mut state = ListState::default();
    state.select(Some(app.tags_notes_selected));
    frame.render_stateful_widget(list, area, &mut state);
}

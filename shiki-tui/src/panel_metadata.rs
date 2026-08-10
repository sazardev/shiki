use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::icons;
use crate::render::{hex_to_color, panel_block};

/// The metadata modal (notes-scope/preview-scope `M`) — a flat list, unlike
/// the tags modal's two levels, since there's only ever one note's fields
/// to show: `tags` first (always present, even empty, so it's the one
/// always-discoverable place to add tags), then every custom frontmatter
/// field. `App::handle_metadata_key` drives editing; this only reads state.
///
/// The key hints used to be crammed into the title string itself
/// (`" Metadata: {title}  ·  enter edit · a add · d delete · esc close "`)
/// — that's exactly why `a` (add a field) was invisible in practice: a
/// `ratatui::widgets::Block` title is truncated to whatever fits the
/// popup's own border width, and a real note title plus that whole hint
/// string never did. Hints now live in their own dedicated, always-visible
/// footer row inside the same border instead — the block is rendered
/// separately (not via `.block()` on the list) specifically so its inner
/// area can be split into a list row area and a one-line footer, the same
/// "render the border once, split what's inside it" shape `panel_preview`
/// uses for its own metadata header.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let fg = hex_to_color(&app.theme.fg);
    let tag_color = hex_to_color(&app.theme.tag);
    let muted = hex_to_color(&app.theme.muted);

    let rows = app.metadata_rows();
    let title = match app.selected_note() {
        Some(note) => format!(" {}Metadata: {} ", icons::TAG, note.frontmatter.title),
        None => format!(" {}Metadata ", icons::TAG),
    };

    let block = panel_block(Line::from(title), true, &app.theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [list_area, hint_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    let items: Vec<ListItem> = rows
        .iter()
        .map(|(key, value)| {
            let color = if key == "tags" { tag_color } else { fg };
            let value = if value.is_empty() {
                "—"
            } else {
                value.as_str()
            };
            ListItem::new(format!("{key:<12} {value}")).style(Style::default().fg(color))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(tag_color)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(format!("{} ", icons::ARROW));

    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(app.metadata_selected.min(rows.len() - 1)));
    }
    frame.render_stateful_widget(list, list_area, &mut state);

    let key_style = Style::default().fg(tag_color).add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(muted);
    let hint = Line::from(vec![
        Span::styled("a", key_style),
        Span::styled(" add   ", label_style),
        Span::styled("enter", key_style),
        Span::styled(" edit   ", label_style),
        Span::styled("d", key_style),
        Span::styled(" delete   ", label_style),
        Span::styled("esc", key_style),
        Span::styled(" close", label_style),
    ]);
    frame.render_widget(Paragraph::new(hint).alignment(Alignment::Center), hint_area);
}

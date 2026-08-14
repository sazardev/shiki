use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::icons;
use crate::render::{hex_to_color, panel_block};

/// The outline's headings narrowed by the modal's live filter query — every
/// heading whose text contains `query` (case-insensitive); an empty query
/// matches all. A pure function of plain values, tested here without
/// touching `App`.
pub fn filtered_headings<'a>(
    query: &str,
    headings: &'a [shiki_core::headings::Heading],
) -> Vec<&'a shiki_core::headings::Heading> {
    let q = query.to_lowercase();
    headings
        .iter()
        .filter(|h| q.is_empty() || h.text.to_lowercase().contains(&q))
        .collect()
}

/// Outline/heading-jump popup (PREVIEW-scope `o`, or `Ctrl+O` inside
/// `Mode::Edit`) — a flat list of the selected note's `#`..`######`
/// headings (`shiki_core::headings::extract`), indented by level, narrowed
/// by a live filter query typed at the top. Read-only rendering;
/// `App::handle_outline_key` drives query/filter/selection/close/jump.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);
    let accent = hex_to_color(&app.theme.accent);

    let filtered = filtered_headings(&app.outline_query, &app.outline_headings);

    let [query_area, list_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(area);

    let query_line = if app.outline_query.is_empty() {
        Line::from(vec![
            Span::styled("  ", Style::default().fg(muted)),
            Span::styled("filter headings…", Style::default().fg(muted)),
        ])
    } else {
        Line::from(vec![
            Span::styled("  ", Style::default().fg(muted)),
            Span::styled(
                format!("{}▌", app.outline_query),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
        ])
    };
    let query = Paragraph::new(query_line).block(panel_block("Filter", false, &app.theme));
    frame.render_widget(query, query_area);

    let items: Vec<ListItem> = if filtered.is_empty() {
        vec![ListItem::new("  no matching headings").style(Style::default().fg(muted))]
    } else {
        filtered
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

    let title = format!(" {}Outline [{}] ", icons::TREE, filtered.len());
    let highlight_symbol = format!("{}", icons::ARROW);
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    let selected = if app.outline_selected < filtered.len() {
        app.outline_selected
    } else {
        filtered.len().saturating_sub(1)
    };
    state.select(Some(selected));
    frame.render_stateful_widget(list, list_area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heading(line: usize, level: u8, text: &str) -> shiki_core::headings::Heading {
        shiki_core::headings::Heading {
            line,
            level,
            text: text.to_string(),
        }
    }

    #[test]
    fn empty_query_matches_everything() {
        let headings = vec![
            heading(0, 1, "Intro"),
            heading(3, 2, "Setup"),
            heading(9, 1, "Deploy"),
        ];
        let filtered = filtered_headings("", &headings);
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].line, 0);
        assert_eq!(filtered[2].line, 9);
    }

    #[test]
    fn query_filters_case_insensitively() {
        let headings = vec![
            heading(0, 1, "Instalación"),
            heading(3, 2, "setup"),
            heading(9, 1, "DEPLOY"),
        ];
        let filtered = filtered_headings("dep", &headings);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].line, 9);
        assert_eq!(filtered[0].text, "DEPLOY");
    }

    #[test]
    fn query_with_no_matches_is_empty() {
        let headings = vec![heading(0, 1, "Intro"), heading(3, 2, "Setup")];
        assert!(filtered_headings("zzz", &headings).is_empty());
    }

    #[test]
    fn query_matches_substring_in_heading_text_only() {
        let headings = vec![
            heading(0, 1, "Intro"),
            heading(3, 2, "Setup"),
            heading(9, 1, "Deploy"),
        ];
        let filtered = filtered_headings("set", &headings);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].text, "Setup");
    }
}

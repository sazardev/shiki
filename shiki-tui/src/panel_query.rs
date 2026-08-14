//! The query modal (leader+`q`): a live-editable Dataview-style DSL box
//! (`where status = pending sort due asc`) over a `Table` of matching notes
//! — the first use of `ratatui::widgets::Table` in this codebase (everywhere
//! else renders a `List`), since this is genuinely tabular, multi-column
//! data rather than a flat list of rows. Modeled on `which.rs`'s shape (an
//! editable `InputBox` on top, results below, navigation via arrows/
//! PageUp/PageDown/Home/End — deliberately not `j`/`k`, which need to stay
//! typeable into the query) rather than `panel_tasks`'s pure-navigation one.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Clear, List, ListItem, ListState, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{centered_rect, App};
use crate::icons;
use crate::render::{hex_to_color, panel_block};

pub(crate) fn yaml_cell_text(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

pub fn render(frame: &mut Frame, frame_area: Rect, app: &App) {
    let height = (frame_area.height * 3 / 4).max(8);
    let popup_area = centered_rect(frame_area, (frame_area.width * 3 / 4).max(50), height);
    frame.render_widget(Clear, popup_area);

    let [input_area, list_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(popup_area);

    let accent = hex_to_color(&app.theme.accent);

    app.query_input.render(
        frame,
        input_area,
        &format!(
            " {}Query  —  where field = value [and/or ...] [sort field [asc|desc]]  ·  enter jump \u{B7} esc close ",
            icons::FILTER
        ),
        accent,
        hex_to_color(&app.theme.bg),
    );

    render_result_table(
        frame,
        list_area,
        app,
        &app.query_rows,
        &app.query_suggestions_visible,
        app.query_selected,
        app.query_error.as_deref(),
        accent,
        true,
    );
}

/// The query DSL's results table — a `Table` (not a `List`, since this is
/// genuinely tabular, multi-column data), shared by the dedicated query
/// modal (leader+`q`, above) and the global search modal's own `!`-prefixed
/// query mode (`draw.rs::render_global_search_query`), so a query means the
/// same thing and renders the same way in either place. `suggestions`
/// (`shiki_core::query::suggest_queries`, filtered live by
/// `App::matching_suggestions`) takes priority over both the table and the
/// error message whenever it's non-empty — either nothing's been typed yet
/// (every suggestion shows, as a "here's what you can ask" starting point)
/// or what's in progress doesn't parse yet but still resembles something
/// real (a live "did you mean" list) — the raw parse error only surfaces
/// once no suggestion matches either.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_result_table(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    rows: &[shiki_core::query::QueryRow],
    suggestions: &[crate::app::QuerySuggestion],
    selected: usize,
    error: Option<&str>,
    accent: ratatui::style::Color,
    can_manage_saved: bool,
) {
    let muted = hex_to_color(&app.theme.muted);
    let error_color = hex_to_color(&app.theme.error);

    if !suggestions.is_empty() {
        // `Ctrl+S`/`Ctrl+D` (save/delete a saved query) only exist in the
        // dedicated leader+`q` modal, not global search's `!`-prefixed
        // mode — the hint says so only where it's actually true, rather
        // than advertising a shortcut that would silently do nothing here.
        let title = if can_manage_saved {
            format!(
                " {}try one of these, from your own notes  ·  enter fills it in \u{B7} ctrl+s save \u{B7} ctrl+d delete saved ",
                icons::FILTER
            )
        } else {
            format!(
                " {}try one of these, from your own notes  ·  enter fills it in ",
                icons::FILTER
            )
        };
        let items: Vec<ListItem> = suggestions
            .iter()
            .map(|s| ListItem::new(s.display.as_str()))
            .collect();
        let list = List::new(items)
            .block(panel_block(Line::from(title), true, &app.theme))
            .highlight_style(
                Style::default()
                    .bg(hex_to_color(&app.theme.selection))
                    .fg(accent)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(format!("{} ", icons::ARROW));
        let mut state = ListState::default();
        state.select(Some(selected.min(suggestions.len().saturating_sub(1))));
        frame.render_stateful_widget(list, area, &mut state);
        return;
    }

    if let Some(err) = error {
        let block = panel_block(Line::from(" Query "), true, &app.theme);
        let mut lines = vec![Line::from(Span::styled(
            err.to_string(),
            Style::default().fg(error_color),
        ))];
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("built-in fields: {}", shiki_core::query::BUILTIN_FIELDS),
            Style::default().fg(muted),
        )));
        if !app.query_known_fields.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("seen in your notes: {}", app.query_known_fields.join(", ")),
                Style::default().fg(muted),
            )));
        }
        lines.push(Line::from(Span::styled(
            format!("example: {}", shiki_core::query::EXAMPLE_QUERY),
            Style::default().fg(muted),
        )));
        let msg = ratatui::widgets::Paragraph::new(lines)
            .wrap(ratatui::widgets::Wrap { trim: true })
            .block(block);
        frame.render_widget(msg, area);
        return;
    }

    let title = format!(
        " {}{} note{} matched ",
        icons::FILTER,
        rows.len(),
        if rows.len() == 1 { "" } else { "s" }
    );

    let table_rows: Vec<Row> = rows
        .iter()
        .map(|r| {
            let fields = r
                .fields
                .iter()
                .filter_map(|(k, v)| Some(format!("{}={}", k.as_str()?, yaml_cell_text(v))))
                .collect::<Vec<_>>()
                .join(" ");
            Row::new(vec![
                Cell::from(r.note_title.clone()),
                Cell::from(r.notebook.clone()),
                Cell::from(fields).style(Style::default().fg(muted)),
                Cell::from(r.location.clone()).style(Style::default().fg(muted)),
            ])
        })
        .collect();

    let header = Row::new(vec!["Title", "Notebook", "Fields", "Location"])
        .style(Style::default().fg(muted).add_modifier(Modifier::ITALIC));

    let widths = [
        Constraint::Percentage(30),
        Constraint::Percentage(15),
        Constraint::Percentage(30),
        Constraint::Percentage(25),
    ];

    let table = Table::new(table_rows, widths)
        .header(header)
        .block(panel_block(Line::from(title), true, &app.theme))
        .row_highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(format!("{} ", icons::ARROW));

    let mut state = TableState::default();
    if !rows.is_empty() {
        state.select(Some(selected));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

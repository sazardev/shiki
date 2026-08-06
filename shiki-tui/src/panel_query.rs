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
use ratatui::text::Line;
use ratatui::widgets::{Cell, Clear, Row, Table, TableState};
use ratatui::Frame;

use crate::app::{centered_rect, App};
use crate::icons;
use crate::render::{hex_to_color, panel_block};

fn yaml_cell_text(v: &serde_yaml::Value) -> String {
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
    let muted = hex_to_color(&app.theme.muted);
    let error = hex_to_color(&app.theme.error);

    app.query_input.render(
        frame,
        input_area,
        &format!(
            " {}Query  —  where field = value [and/or ...] [sort field [asc|desc]]  ·  enter jump \u{B7} esc close ",
            icons::FILTER
        ),
        accent,
    );

    if let Some(err) = &app.query_error {
        let block = panel_block(Line::from(" Query "), true, &app.theme);
        let msg = ratatui::widgets::Paragraph::new(err.as_str())
            .style(Style::default().fg(error))
            .block(block);
        frame.render_widget(msg, list_area);
        return;
    }

    let title = format!(
        " {}{} note{} matched ",
        icons::FILTER,
        app.query_rows.len(),
        if app.query_rows.len() == 1 { "" } else { "s" }
    );

    let rows: Vec<Row> = app
        .query_rows
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

    let table = Table::new(rows, widths)
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
    if !app.query_rows.is_empty() {
        state.select(Some(app.query_selected));
    }
    frame.render_stateful_widget(table, list_area, &mut state);
}

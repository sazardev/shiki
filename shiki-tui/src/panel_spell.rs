use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

use crate::app::App;
use crate::icons;
use crate::render::{hex_to_color, panel_block};

/// Spell-check popup (`Ctrl+E` in `Mode::Edit`, `config.editor.spellcheck`):
/// every misspelled word the last pass found, in document order, each with
/// its top suggestion(s) and `(row:col)` location. The selected row is
/// marked with a plain `▸` cursor plus the theme's selection background;
/// `Enter` opens the selected word's suggestions submenu
/// (`render_suggestions`), where the actual replacement is chosen.
/// Read-only rendering — `App::handle_spell_key`/`handle_spell_suggestions_key`
/// drive selection/apply/close.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);
    let accent = hex_to_color(&app.theme.accent);

    let Some(report) = &app.spell_report else {
        return;
    };

    let items: Vec<ListItem> = if report.misses.is_empty() {
        vec![ListItem::new("  no misspellings \u{1f389}").style(Style::default().fg(muted))]
    } else {
        report
            .misses
            .iter()
            .map(|m| {
                let mut spans = Vec::new();
                spans.push(Span::styled(
                    format!("  {}  ", m.word),
                    Style::default().fg(accent).add_modifier(Modifier::BOLD),
                ));
                let suggestions = if m.suggestions.is_empty() {
                    "no suggestions".to_string()
                } else {
                    m.suggestions.join(", ")
                };
                spans.push(Span::styled(suggestions, Style::default().fg(fg)));
                spans.push(Span::styled(
                    format!("  ({}:{})", m.row + 1, m.col_start + 1),
                    Style::default().fg(muted),
                ));
                ListItem::new(Line::from(spans))
            })
            .collect()
    };

    let title = format!(" {}Spell check [{}] ", icons::CHECK, report.misses.len());
    // A plain `▸` — universal in every terminal font — rather than the
    // Nerd Font arrow, so the selected-word cursor is always visible.
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    let selected = if app.spell_selected < report.misses.len() {
        app.spell_selected
    } else {
        report.misses.len().saturating_sub(1)
    };
    state.select(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

/// The suggestions submenu (`Enter` on a word in the spell-check popup):
/// one row per correction candidate, with the same `▸` cursor, so the user
/// picks exactly which replacement to apply rather than always taking the
/// first. `App::handle_spell_suggestions_key` drives selection/apply/back.
pub fn render_suggestions(frame: &mut Frame, area: Rect, app: &App) {
    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);
    let accent = hex_to_color(&app.theme.accent);

    let Some(miss) = app
        .spell_report
        .as_ref()
        .and_then(|r| r.misses.get(app.spell_selected))
    else {
        return;
    };

    let items: Vec<ListItem> = if miss.suggestions.is_empty() {
        vec![ListItem::new("  no suggestions").style(Style::default().fg(muted))]
    } else {
        miss.suggestions
            .iter()
            .map(|s| ListItem::new(format!("  {s}")).style(Style::default().fg(fg)))
            .collect()
    };

    let title = format!(" {}Replace '{}' with ", icons::CHECK, miss.word);
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    let selected = if app.spell_suggestion_selected < miss.suggestions.len() {
        app.spell_suggestion_selected
    } else {
        miss.suggestions.len().saturating_sub(1)
    };
    state.select(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

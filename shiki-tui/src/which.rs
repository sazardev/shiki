use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState};
use ratatui::Frame;

use crate::app::App;
use crate::icons;
use crate::keybindings::{action_icon, action_label, describe_key};
use crate::render::{hex_to_color, panel_block};

/// Near-full-screen popup listing every keybinding, grouped by scope
/// (Yazi-style which-key, but segmented: GLOBAL entries need the leader key
/// first, NOTEBOOKS/NOTES/PREVIEW only apply while that panel has focus).
/// Doubles as a fast command palette: type to filter (by key, action label,
/// or scope), `j`/`k`/arrows/PageUp/PageDown/Home/End move the selection,
/// `Enter` runs the highlighted action immediately, `Esc` just closes.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let margin_x = area.width / 10;
    let margin_y = area.height / 10;
    let popup_area = Rect {
        x: area.x + margin_x,
        y: area.y + margin_y,
        width: area.width.saturating_sub(margin_x * 2),
        height: area.height.saturating_sub(margin_y * 2),
    };
    frame.render_widget(Clear, popup_area);

    let [input_area, list_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(popup_area);

    app.which_key_input.render(
        frame,
        input_area,
        &format!(
            " {}  Which Key  —  type to filter · enter run · esc close ",
            icons::KEYBOARD
        ),
        hex_to_color(&app.theme.accent),
    );

    let accent = hex_to_color(&app.theme.accent);
    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);

    let entries = app.which_key_filtered_entries();
    let mut items: Vec<ListItem> = Vec::with_capacity(entries.len() + 4);
    let mut last_scope: Option<&'static str> = None;
    // Tracks which rendered row each filtered entry lands on, since scope
    // header lines are interspersed — `ListState::select` needs a row
    // index, not an entry index.
    let mut selected_row = 0usize;
    for (i, (scope, key, action)) in entries.iter().enumerate() {
        if last_scope != Some(scope) {
            items.push(ListItem::new(Line::from(Span::styled(
                format!("── {scope} ──"),
                Style::default().fg(muted).add_modifier(Modifier::ITALIC),
            ))));
            last_scope = Some(scope);
        }
        if i == app.which_key_selected {
            selected_row = items.len();
        }
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{key:>8} "),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{} ", action_icon(*action)),
                Style::default().fg(accent),
            ),
            Span::styled(action_label(*action), Style::default().fg(fg)),
        ])));
    }

    let title = format!(
        " {}  {} of {} bindings — leader is {} ",
        icons::KEYBOARD,
        entries.len(),
        app.keymaps().entries().len(),
        describe_key(app.keymaps().leader_key())
    );
    let highlight_symbol = format!("{} ", icons::ARROW);
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
    if !entries.is_empty() {
        state.select(Some(selected_row));
    }
    frame.render_stateful_widget(list, list_area, &mut state);
}

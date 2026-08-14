use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// Simple single-line text input (used for search, new note, rename, etc).
/// `masked` renders every character as `*` (a passphrase prompt) without
/// changing how `value` itself is stored or edited — only `render` reads it.
#[derive(Debug, Default, Clone)]
pub struct InputBox {
    pub value: String,
    pub masked: bool,
}

impl InputBox {
    pub fn push(&mut self, c: char) {
        self.value.push(c);
    }

    pub fn backspace(&mut self) {
        self.value.pop();
    }

    pub fn clear(&mut self) {
        self.value.clear();
    }

    /// `bg` fills both the block and the text area — modals `Clear` their
    /// popup rect first (which resets those cells to the terminal default,
    /// *not* the theme's `bg`), so without an explicit background every input
    /// box showed a flat terminal-colored band that visibly clashed while the
    /// theme picker live-previews different palettes.
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        border_color: ratatui::style::Color,
        bg: ratatui::style::Color,
    ) {
        let display: std::borrow::Cow<str> = if self.masked {
            "*".repeat(self.value.chars().count()).into()
        } else {
            self.value.as_str().into()
        };
        let style = Style::default().bg(bg);
        let paragraph = Paragraph::new(display.into_owned()).style(style).block(
            Block::default()
                .title(title.to_string())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(style),
        );
        frame.render_widget(paragraph, area);

        let cursor_x = area.x + 1 + self.value.chars().count() as u16;
        let max_x = area.x + area.width.saturating_sub(2);
        frame.set_cursor_position((cursor_x.min(max_x), area.y + 1));
    }
}

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Frame;
use shiki_config::Theme;

/// The color a `GitStatus` gets in the footer and the notebook drawer —
/// dirty (uncommitted changes) outranks ahead/behind, which outranks fully
/// clean, same one-color-per-notebook priority both places already agreed
/// on before this was pulled out into a shared helper: dirty is more
/// urgent than "just needs a push/pull", and clean is the good case.
pub fn git_status_color(theme: &Theme, gs: &shiki_core::git::GitStatus) -> Color {
    if gs.status_error.is_some() {
        hex_to_color(&theme.error)
    } else if gs.dirty_count > 0 {
        hex_to_color(&theme.warning)
    } else if gs.ahead > 0 || gs.behind > 0 {
        hex_to_color(&theme.accent)
    } else {
        hex_to_color(&theme.success)
    }
}

/// The `" +{dirty} {UPLOAD}{ahead} {DOWNLOAD}{behind}"` suffix describing a
/// `GitStatus` — empty when there's nothing to report. Shared by the footer
/// and the notebook drawer so the same notebook never shows different
/// numbers in two places because one of them drifted from the other.
pub fn git_status_suffix(gs: &shiki_core::git::GitStatus) -> String {
    if gs.status_error.is_some() {
        // The status check itself failed (locked index, permission issue,
        // corrupted repo) — showing "+0" here would silently read as
        // "clean", which is exactly the wrong signal when the real state
        // is genuinely unknown.
        return " status?".to_string();
    }
    let mut extras = String::new();
    if gs.dirty_count > 0 {
        extras.push_str(&format!(" +{}", gs.dirty_count));
    }
    if gs.ahead > 0 {
        extras.push_str(&format!(" {}{}", crate::icons::UPLOAD, gs.ahead));
    }
    if gs.behind > 0 {
        extras.push_str(&format!(" {}{}", crate::icons::DOWNLOAD, gs.behind));
    }
    extras
}

/// Whether a resolved theme color reads as "dark" (perceptual luminance
/// under half) — used to pick a syntect syntax-highlighting theme
/// (`syntax::CodeHighlighter`) that won't clash with the active shiki theme
/// (e.g. a light-on-light syntect theme under catppuccin-latte). `Reset`
/// (the "default" theme's un-set background) defaults to `true` — most
/// terminals default to a dark background, and a wrong guess here only
/// affects code-fence coloring, not correctness.
pub fn is_dark_color(color: Color) -> bool {
    match color {
        Color::Rgb(r, g, b) => {
            let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            luminance < 128.0
        }
        Color::Black | Color::DarkGray => true,
        Color::White | Color::Gray => false,
        _ => true,
    }
}

/// Converts a theme color slot to `ratatui::Color`. Accepts `#rrggbb` hex
/// (every built-in palette), the terminal's native ANSI names, or `"reset"`
/// to inherit whatever the terminal's own default color is — that's what
/// the "default" theme uses, so it looks right in any terminal color scheme
/// instead of imposing a fixed palette on top of it.
pub fn hex_to_color(value: &str) -> Color {
    match value.to_ascii_lowercase().as_str() {
        "reset" | "" => return Color::Reset,
        "black" => return Color::Black,
        "red" => return Color::Red,
        "green" => return Color::Green,
        "yellow" => return Color::Yellow,
        "blue" => return Color::Blue,
        "magenta" => return Color::Magenta,
        "cyan" => return Color::Cyan,
        "white" => return Color::White,
        "gray" | "grey" => return Color::Gray,
        "darkgray" | "darkgrey" => return Color::DarkGray,
        _ => {}
    }
    let hex = value.trim_start_matches('#');
    if hex.len() != 6 {
        return Color::Reset;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    Color::Rgb(r, g, b)
}

/// Themed panel `Block`: fills bg/fg from the theme, uses a thicker accent
/// border when focused and a plain square one otherwise. Shared by every
/// panel and popup so the whole UI reads as one consistent surface instead of
/// bare unstyled borders on the terminal's default background.
pub fn panel_block<'a>(title: impl Into<Line<'a>>, focused: bool, theme: &Theme) -> Block<'a> {
    let border_color = if focused {
        hex_to_color(&theme.accent)
    } else {
        hex_to_color(&theme.border)
    };
    let border_type = if focused {
        BorderType::Thick
    } else {
        BorderType::Plain
    };
    styled_block(title, border_color, border_type, theme)
}

/// Same themed surface as `panel_block`, but always a plain (thin) border,
/// still accent-colored to show focus. Used for PREVIEW specifically while
/// actually reading a note's content — a full-width `Thick` border around a
/// large block of body text otherwise reads as visually loud/shouty in a way
/// the same border on a narrow list (Notebooks/Notes) doesn't; every other
/// focused panel/popup keeps the regular `Thick` emphasis from `panel_block`.
pub fn panel_block_reading<'a>(title: impl Into<Line<'a>>, theme: &Theme) -> Block<'a> {
    styled_block(title, hex_to_color(&theme.accent), BorderType::Plain, theme)
}

fn styled_block<'a>(
    title: impl Into<Line<'a>>,
    border_color: Color,
    border_type: BorderType,
    theme: &Theme,
) -> Block<'a> {
    Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(hex_to_color(&theme.panel_title))
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(border_color))
        .style(
            Style::default()
                .bg(hex_to_color(&theme.bg))
                .fg(hex_to_color(&theme.fg)),
        )
}

/// Renders a vertical scrollbar along the right edge of `area` for a list of
/// `total` items currently positioned at `selected`. No-op when everything
/// fits, so short lists don't grow a useless track.
pub fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    total: usize,
    selected: usize,
    theme: &Theme,
) {
    if total <= area.height.saturating_sub(2) as usize {
        return;
    }
    let mut state = ScrollbarState::new(total).position(selected);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(None)
        .style(Style::default().fg(hex_to_color(&theme.scrollbar)));
    frame.render_stateful_widget(
        scrollbar,
        area.inner(ratatui::layout::Margin::new(0, 1)),
        &mut state,
    );
}

/// A single left-to-right pass over `text` recognizing, at each position in
/// priority order: `[[wikilink]]`, `![alt](url)`, `[text](url)`,
/// `**bold**`, `` `code` ``, then `*italic*` — anything else accumulates as
/// plain text in `base` style. An opening marker with no matching closer
/// (a bare `*`, an unterminated `**`) is left as literal text for just
/// that character rather than swallowing the rest of the line looking for
/// a closer that isn't there.
///
/// Bold/italic are applied as `base.add_modifier(...)` rather than a fixed
/// style, so e.g. bold text inside a dim/italic blockquote still reads as
/// dim+bold, not a jarring unrelated color — the same reason `inline_spans`
/// takes `base` as a parameter instead of assuming plain body-text style.
fn inline_spans(text: &str, base: Style, link: Style) -> Vec<Span<'static>> {
    fn flush(plain: &mut String, spans: &mut Vec<Span<'static>>, style: Style) {
        if !plain.is_empty() {
            spans.push(Span::styled(std::mem::take(plain), style));
        }
    }
    fn find_pair(chars: &[char], from: usize, marker: char) -> Option<usize> {
        chars[from..]
            .iter()
            .position(|&c| c == marker)
            .map(|p| p + from)
    }
    /// For `[text](url)` (or `![alt](url)` when `open` is the `[` right
    /// after a leading `!`): the index of the closing `]` and of the `)`,
    /// if the syntax is well-formed starting at `open`. Doesn't handle
    /// nested `[`/`]` inside the label — real Markdown rarely needs that,
    /// and this is a preview renderer, not a full parser.
    fn find_link_parts(chars: &[char], open: usize) -> Option<(usize, usize)> {
        let close_bracket = find_pair(chars, open + 1, ']')?;
        if chars.get(close_bracket + 1) != Some(&'(') {
            return None;
        }
        let close_paren = find_pair(chars, close_bracket + 2, ')')?;
        Some((close_bracket, close_paren))
    }

    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' && chars.get(i + 1) == Some(&'[') {
            if let Some(end) =
                find_pair(&chars, i + 2, ']').filter(|&e| chars.get(e + 1) == Some(&']'))
            {
                flush(&mut plain, &mut spans, base);
                spans.push(Span::styled(
                    chars[i..end + 2].iter().collect::<String>(),
                    link,
                ));
                i = end + 2;
                continue;
            }
        }
        if chars[i] == '!' && chars.get(i + 1) == Some(&'[') {
            if let Some((close_bracket, close_paren)) = find_link_parts(&chars, i + 1) {
                flush(&mut plain, &mut spans, base);
                let alt: String = chars[i + 2..close_bracket].iter().collect();
                let label = if alt.is_empty() {
                    format!("{}image", crate::icons::IMAGE)
                } else {
                    format!("{}{alt}", crate::icons::IMAGE)
                };
                spans.push(Span::styled(label, link));
                i = close_paren + 1;
                continue;
            }
        }
        if chars[i] == '[' {
            if let Some((close_bracket, close_paren)) = find_link_parts(&chars, i) {
                flush(&mut plain, &mut spans, base);
                let label: String = chars[i + 1..close_bracket].iter().collect();
                spans.push(Span::styled(label, link));
                i = close_paren + 1;
                continue;
            }
        }
        if chars[i] == '*' && chars.get(i + 1) == Some(&'*') {
            if let Some(end) =
                find_pair(&chars, i + 2, '*').filter(|&e| chars.get(e + 1) == Some(&'*'))
            {
                flush(&mut plain, &mut spans, base);
                let inner: String = chars[i + 2..end].iter().collect();
                spans.push(Span::styled(inner, base.add_modifier(Modifier::BOLD)));
                i = end + 2;
                continue;
            }
        }
        if chars[i] == '`' {
            if let Some(end) = find_pair(&chars, i + 1, '`') {
                flush(&mut plain, &mut spans, base);
                let inner: String = chars[i + 1..end].iter().collect();
                // `Modifier::DIM`, not `ITALIC` — distinct from `*italic*`
                // below, since both would otherwise render identically.
                spans.push(Span::styled(inner, base.add_modifier(Modifier::DIM)));
                i = end + 1;
                continue;
            }
        }
        if chars[i] == '*' {
            if let Some(end) = find_pair(&chars, i + 1, '*') {
                flush(&mut plain, &mut spans, base);
                let inner: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(inner, base.add_modifier(Modifier::ITALIC)));
                i = end + 1;
                continue;
            }
        }
        plain.push(chars[i]);
        i += 1;
    }
    flush(&mut plain, &mut spans, base);
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), base));
    }
    spans
}

/// Splits a `| cell | cell |` row into trimmed cell strings, dropping the
/// (possibly absent) leading/trailing empty cell the outer pipes produce.
fn table_cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

/// Whether `line` is a table's own separator row (`| --- | :-- |`, etc.) —
/// every cell only `-`/`:` characters, and at least one cell. Checked
/// against the line *after* a candidate header row before committing to
/// table rendering, so a stray `|` in ordinary prose (a shell pipe
/// example, say) doesn't get misread as a table.
fn is_table_separator(line: &str) -> bool {
    let cells = table_cells(line);
    !cells.is_empty()
        && cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
}

/// `N. rest` → `Some(("N.", rest))` — the digit run can be any length
/// (`1.` through `99.` and beyond), unlike a fixed `strip_prefix`.
fn ordered_list_prefix(line: &str) -> Option<(&str, &str)> {
    let digits = line.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
    if digits == 0
        || line.as_bytes().get(digits) != Some(&b'.')
        || line.as_bytes().get(digits + 1) != Some(&b' ')
    {
        return None;
    }
    Some((&line[..=digits], &line[digits + 2..]))
}

/// A horizontal rule: 3+ of the same `-`/`*`/`_` character (optionally
/// surrounded by whitespace), nothing else on the line.
fn is_horizontal_rule(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 3
        && (t.chars().all(|c| c == '-')
            || t.chars().all(|c| c == '*')
            || t.chars().all(|c| c == '_'))
}

/// Minimal markdown-to-styled-lines render: bold headings, checkbox/list
/// bullets (ordered and unordered), tables, math/code fences, blockquotes
/// with working inline formatting, a stripped-down `<details>`/`<summary>`
/// (shown fully expanded — a static `Paragraph` has no fold state to toggle
/// against), horizontal rules, and styled `[[wikilinks]]`/`[links](url)`/
/// `![images](url)`. Good enough for the preview panel; not a replacement
/// for a full comrak/syntect-based renderer.
/// Rebuilds `Line<'a>`s that borrow their text from an existing
/// `&'a [Line<'static>]` instead of cloning it — used to hand a cached,
/// already-formatted preview (`App::note_preview_lines`/
/// `folder_preview_lines`) to `Paragraph::new`, which needs an owned
/// `Vec<Line<'a>>`. A plain `.to_vec()` would deep-clone every `String`
/// backing every span — cheap for a handful of lines, but a real,
/// measured cost on a huge note or a folder with tens of thousands of
/// entries, re-paid on every single draw tick even though the *content*
/// hasn't changed. Only the small `Style`/`Vec` scaffolding gets rebuilt
/// here; every span's actual text is a `Cow::Borrowed` pointing at the
/// cache's own bytes, so nothing text-sized gets copied.
pub fn borrow_lines<'a>(lines: &'a [Line<'static>]) -> Vec<Line<'a>> {
    lines
        .iter()
        .map(|line| Line {
            style: line.style,
            alignment: line.alignment,
            spans: line
                .spans
                .iter()
                .map(|span| Span {
                    style: span.style,
                    content: std::borrow::Cow::Borrowed(span.content.as_ref()),
                })
                .collect(),
        })
        .collect()
}

pub fn markdown_to_lines(
    body: &str,
    fg: Color,
    accent: Color,
    muted: Color,
    link: Color,
    dark: bool,
) -> Vec<Line<'static>> {
    markdown_to_lines_indexed(body, fg, accent, muted, link, dark)
        .into_iter()
        .map(|(_, line)| line)
        .collect()
}

/// Same as `markdown_to_lines`, but pairs every rendered row with the
/// 0-based index into `body.lines()` it was rendered from — lets a click on
/// a rendered PREVIEW row (see `App::note_preview_source_line`) jump into
/// `Mode::Edit` at the right line of the *raw* Markdown source, even though
/// the rendered text itself (headers/bold/tables reformatted, syntax
/// stripped) doesn't correspond character-for-character to it. Every row
/// maps 1:1 to its source line except a table's separator row, which is
/// consumed but produces no rendered row of its own — its index is reused
/// by the header-divider row rendered in its place instead, so clicking the
/// divider still lands on a real source line.
pub fn markdown_to_lines_indexed(
    body: &str,
    fg: Color,
    accent: Color,
    muted: Color,
    link: Color,
    dark: bool,
) -> Vec<(usize, Line<'static>)> {
    let heading = Style::default().fg(accent).add_modifier(Modifier::BOLD);
    let text = Style::default().fg(fg);
    let dim = Style::default().fg(muted).add_modifier(Modifier::ITALIC);
    // No dedicated theme slot for "math block" — reusing `accent` (instead
    // of `muted`, which code fences already use) is enough to make a math
    // block visually distinct from a code block at a glance, without
    // needing a 20th configurable color just for this.
    let math = Style::default().fg(accent).add_modifier(Modifier::ITALIC);
    let link_style = Style::default().fg(link).add_modifier(Modifier::UNDERLINED);

    let mut in_code_block = false;
    let mut in_math_block = false;
    let mut code_lang: Option<String> = None;
    let mut code_highlighter: Option<crate::syntax::CodeHighlighter> = None;
    let mut lines: Vec<(usize, Line<'static>)> = Vec::new();

    let mut source = body.lines().enumerate().peekable();
    while let Some((idx, line)) = source.next() {
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            if in_code_block {
                let lang = line
                    .trim_start()
                    .trim_start_matches("```")
                    .trim()
                    .to_ascii_lowercase();
                code_highlighter = if lang.is_empty() || lang == "mermaid" {
                    None
                } else {
                    crate::syntax::CodeHighlighter::new(&lang, dark)
                };
                // The opening fence line itself gets a distinct style when
                // the language is recognized (real highlighting follows) or
                // is a mermaid diagram (styled like the math-block accent
                // below, not the flat code dim — a terminal can't render an
                // actual diagram, but at least the fence reads as "special"
                // rather than indistinguishable from an unrecognized one).
                let fence_style = if lang == "mermaid" {
                    math
                } else if crate::syntax::is_known_language(&lang) {
                    heading
                } else {
                    dim
                };
                code_lang = Some(lang);
                lines.push((idx, Line::from(Span::styled(line.to_string(), fence_style))));
            } else {
                code_highlighter = None;
                code_lang = None;
                lines.push((idx, Line::from(Span::styled(line.to_string(), dim))));
            }
            continue;
        }
        if in_code_block {
            let rendered = if code_lang.as_deref() == Some("mermaid") {
                Line::from(Span::styled(line.to_string(), math))
            } else if let Some(hl) = code_highlighter.as_mut() {
                Line::from(hl.highlight(line))
            } else {
                Line::from(Span::styled(line.to_string(), dim))
            };
            lines.push((idx, rendered));
            continue;
        }
        if line.trim_start().starts_with("$$") {
            in_math_block = !in_math_block;
            lines.push((idx, Line::from(Span::styled(line.to_string(), math))));
            continue;
        }
        if in_math_block {
            lines.push((idx, Line::from(Span::styled(line.to_string(), math))));
            continue;
        }

        let trimmed = line.trim();
        if trimmed == "<details>" || trimmed == "</details>" {
            continue;
        }
        if let Some(inner) = trimmed
            .strip_prefix("<summary>")
            .and_then(|s| s.strip_suffix("</summary>"))
        {
            lines.push((idx, Line::from(Span::styled(format!("▸ {inner}"), heading))));
            continue;
        }

        if is_horizontal_rule(line) {
            lines.push((idx, Line::from(Span::styled("─".repeat(40), dim))));
            continue;
        }

        if line.trim_start().starts_with('|')
            && source
                .peek()
                .is_some_and(|(_, next)| is_table_separator(next))
        {
            let header = table_cells(line);
            let (sep_idx, _) = source.next().expect("peeked Some above"); // consume the separator row itself
            let mut rows = vec![header];
            let mut row_indices = vec![idx];
            while let Some(&(next_idx, next)) = source.peek() {
                if next.trim_start().starts_with('|') {
                    rows.push(table_cells(next));
                    row_indices.push(next_idx);
                    source.next();
                } else {
                    break;
                }
            }
            let col_count = rows.iter().map(Vec::len).max().unwrap_or(0);
            let mut widths = vec![0usize; col_count];
            for row in &rows {
                for (i, cell) in row.iter().enumerate() {
                    widths[i] = widths[i].max(cell.chars().count());
                }
            }
            for (ri, row) in rows.iter().enumerate() {
                let mut spans = Vec::new();
                for (i, width) in widths.iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::styled(" │ ", dim));
                    }
                    let cell = row.get(i).map(String::as_str).unwrap_or("");
                    let style = if ri == 0 { heading } else { text };
                    spans.push(Span::styled(format!("{cell:<width$}"), style));
                }
                lines.push((row_indices[ri], Line::from(spans)));
                if ri == 0 {
                    let rule_width =
                        widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 3;
                    lines.push((
                        sep_idx,
                        Line::from(Span::styled("─".repeat(rule_width), dim)),
                    ));
                }
            }
            continue;
        }

        let rendered = if let Some(rest) = line.strip_prefix("### ") {
            Line::from(inline_spans(rest, heading, link_style))
        } else if let Some(rest) = line.strip_prefix("## ") {
            Line::from(inline_spans(rest, heading, link_style))
        } else if let Some(rest) = line.strip_prefix("# ") {
            Line::from(inline_spans(
                rest,
                heading.add_modifier(Modifier::UNDERLINED),
                link_style,
            ))
        } else if let Some(rest) = line.strip_prefix("- [x] ").or(line.strip_prefix("- [X] ")) {
            Line::from(vec![
                Span::styled(
                    format!("{}", crate::icons::CHECK),
                    Style::default().fg(accent),
                ),
                Span::styled(
                    rest.to_string(),
                    Style::default()
                        .fg(muted)
                        .add_modifier(Modifier::CROSSED_OUT),
                ),
            ])
        } else if let Some(rest) = line.strip_prefix("- [ ] ") {
            let mut spans = vec![Span::styled("☐ ", Style::default().fg(muted))];
            spans.extend(inline_spans(rest, text, link_style));
            Line::from(spans)
        } else if let Some(rest) = line.strip_prefix("- ") {
            let mut spans = vec![Span::styled("• ", Style::default().fg(accent))];
            spans.extend(inline_spans(rest, text, link_style));
            Line::from(spans)
        } else if let Some((marker, rest)) = ordered_list_prefix(line) {
            let mut spans = vec![Span::styled(
                format!("{marker} "),
                Style::default().fg(accent),
            )];
            spans.extend(inline_spans(rest, text, link_style));
            Line::from(spans)
        } else if let Some(rest) = line.strip_prefix("> ") {
            let mut spans = vec![Span::styled("▏ ", dim)];
            spans.extend(inline_spans(rest, dim, link_style));
            Line::from(spans)
        } else {
            Line::from(inline_spans(line, text, link_style))
        };
        lines.push((idx, rendered));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const FG: Color = Color::White;
    const ACCENT: Color = Color::Blue;
    const MUTED: Color = Color::Gray;
    const LINK: Color = Color::Cyan;

    fn line_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn bold_is_stripped_and_styled() {
        let spans = inline_spans("hello **world** today", Style::default(), Style::default());
        let bold = spans
            .iter()
            .find(|s| s.content.as_ref() == "world")
            .expect("bold segment present");
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));
        // No literal `**` survives anywhere in the reconstructed text.
        let full: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full, "hello world today");
    }

    #[test]
    fn italic_is_stripped_and_styled() {
        let spans = inline_spans("a *b* c", Style::default(), Style::default());
        let italic = spans.iter().find(|s| s.content.as_ref() == "b").unwrap();
        assert!(italic.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn inline_code_is_stripped_and_styled() {
        let spans = inline_spans("run `cargo test` now", Style::default(), Style::default());
        let code = spans
            .iter()
            .find(|s| s.content.as_ref() == "cargo test")
            .unwrap();
        assert!(code.style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn unterminated_marker_is_left_literal() {
        // No closing `*` anywhere — must not swallow the rest of the line
        // looking for one, and must not panic.
        let spans = inline_spans("a * b c", Style::default(), Style::default());
        let full: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full, "a * b c");
    }

    #[test]
    fn markdown_link_shows_only_the_label() {
        let spans = inline_spans(
            "see [the docs](https://example.com) here",
            Style::default(),
            Style::default(),
        );
        let full: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full, "see the docs here");
    }

    #[test]
    fn wikilink_still_renders_verbatim() {
        let spans = inline_spans(
            "see [[Some Note]] please",
            Style::default(),
            Style::default(),
        );
        assert!(spans.iter().any(|s| s.content.as_ref() == "[[Some Note]]"));
    }

    #[test]
    fn image_becomes_an_icon_plus_alt_text() {
        let spans = inline_spans("![a photo](pic.png)", Style::default(), Style::default());
        let full: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(full.contains("a photo"));
        assert!(!full.contains("pic.png"));
    }

    #[test]
    fn bold_inside_a_blockquote_is_still_bold_not_literal_asterisks() {
        let lines = markdown_to_lines("> **Warning:** be careful", FG, ACCENT, MUTED, LINK, true);
        assert_eq!(lines.len(), 1);
        let full = line_text(&lines[0]);
        assert!(!full.contains('*'), "asterisks must not survive: {full:?}");
        assert!(full.contains("Warning:"));
        let bold = lines[0]
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "Warning:")
            .expect("bold span present");
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn ordered_list_items_are_recognized() {
        assert_eq!(ordered_list_prefix("1. first"), Some(("1.", "first")));
        assert_eq!(ordered_list_prefix("12. twelfth"), Some(("12.", "twelfth")));
        assert_eq!(ordered_list_prefix("not a list"), None);
        assert_eq!(ordered_list_prefix("1.no space"), None);
    }

    #[test]
    fn horizontal_rule_is_detected() {
        assert!(is_horizontal_rule("---"));
        assert!(is_horizontal_rule("***"));
        assert!(is_horizontal_rule("______"));
        assert!(!is_horizontal_rule("- - -")); // not handled; still just a dash run check
        assert!(!is_horizontal_rule("hello"));
    }

    #[test]
    fn horizontal_rule_renders_as_a_visible_divider() {
        let lines = markdown_to_lines("above\n---\nbelow", FG, ACCENT, MUTED, LINK, true);
        assert_eq!(line_text(&lines[1]), "─".repeat(40));
    }

    #[test]
    fn table_renders_aligned_columns_with_a_header_rule() {
        let body = "| Name | Age |\n| --- | --- |\n| Alice | 30 |\n| Bo | 7 |";
        let lines = markdown_to_lines(body, FG, ACCENT, MUTED, LINK, true);
        // header row + divider row + 2 body rows = 4 lines, no leftover
        // raw `|---|` separator line rendered literally.
        assert_eq!(lines.len(), 4);
        assert!(line_text(&lines[0]).contains("Name"));
        assert!(line_text(&lines[1]).chars().all(|c| c == '─'));
        // Columns line up: "Alice" (5 chars) and "Bo" (2 chars) both
        // padded to the same width as the "Name"/"Age" header cells.
        let alice_row = line_text(&lines[2]);
        let bo_row = line_text(&lines[3]);
        let col_sep = " │ ";
        let alice_col1_width = alice_row.split(col_sep).next().unwrap().chars().count();
        let bo_col1_width = bo_row.split(col_sep).next().unwrap().chars().count();
        assert_eq!(alice_col1_width, bo_col1_width);
    }

    #[test]
    fn table_like_prose_without_a_real_separator_is_left_alone() {
        // A single line with pipes (e.g. a shell example) but no valid
        // `---`-only separator row after it must not trigger table mode.
        let lines = markdown_to_lines("ls | grep foo | wc -l", FG, ACCENT, MUTED, LINK, true);
        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "ls | grep foo | wc -l");
    }

    #[test]
    fn details_summary_is_shown_expanded_with_tags_stripped() {
        let body = "<details>\n<summary>Click to expand</summary>\nhidden content\n</details>";
        let lines = markdown_to_lines(body, FG, ACCENT, MUTED, LINK, true);
        // <details>/</details> themselves produce no line; summary becomes
        // a styled header; the body content still renders normally.
        assert_eq!(lines.len(), 2);
        assert_eq!(line_text(&lines[0]), "▸ Click to expand");
        assert_eq!(line_text(&lines[1]), "hidden content");
    }

    #[test]
    fn math_block_is_styled_distinctly_from_code_block() {
        let body = "$$\nx = y\n$$";
        let lines = markdown_to_lines(body, FG, ACCENT, MUTED, LINK, true);
        assert_eq!(lines.len(), 3);
        // Content line inside the math block uses `accent`, not `muted`
        // (which code fences use) — the whole point of the distinction.
        assert_eq!(lines[1].spans[0].style.fg, Some(ACCENT));
    }
}

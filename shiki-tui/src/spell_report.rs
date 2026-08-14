//! Spell-check state shared by the editor renderer (which underlines
//! misspelled words) and the spell-check popup (which lists them with
//! suggestions). Core's `shiki_core::spell::check_text` reports byte ranges
//! into the whole joined buffer; converting those to `(row, char-column)`
//! pairs once, at check time, keeps both consumers in the same space the
//! editor's `TextArea` cursor already works in.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// One misspelled word, located in `(row, char-column)` space and carrying
/// its correction candidates (empty when hunspell has none).
#[derive(Debug, Clone)]
pub struct SpellMiss {
    pub row: usize,
    pub col_start: usize,
    pub col_end: usize,
    pub word: String,
    pub suggestions: Vec<String>,
}

/// The result of one `Ctrl+E` spell-check pass: the misses plus a snapshot
/// of the buffer lines that were checked. The editor renderer compares each
/// live row against this snapshot and skips underlining rows edited since
/// the check, so stale ranges can't paint the wrong words.
#[derive(Debug, Clone)]
pub struct SpellReport {
    pub misses: Vec<SpellMiss>,
    pub checked_lines: Vec<String>,
}

/// A transient highlight over the word the spell-check popup just fixed —
/// `(row, col_start, col_len)` in the *edited* buffer, plus when it was set
/// so `App::expire_spell_flash` clears it after a moment. This is the
/// visible "this is what changed" cue: without it, applying a suggestion
/// only changed a silent footer message.
#[derive(Debug, Clone, Copy)]
pub struct SpellFlash {
    pub row: usize,
    pub col_start: usize,
    pub col_len: usize,
    pub set_at: std::time::Instant,
}

/// Converts core byte-offset misses (into the joined buffer text) into
/// per-line `(row, char-column)` misses, fetching suggestions for each word
/// on the way. `checked_lines` is the buffer snapshot the report was built
/// from.
pub fn build_report(
    text: &str,
    lines: &[String],
    misses: &[shiki_core::spell::Misspell],
    lang: &str,
) -> SpellReport {
    let lang = if lang.trim().is_empty() {
        None
    } else {
        Some(lang.trim())
    };
    let converted = misses
        .iter()
        .map(|m| {
            let prefix = &text[..m.start];
            let row = prefix.bytes().filter(|&b| b == b'\n').count();
            let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
            let col_start = prefix[line_start..].chars().count();
            let col_end = col_start + text[m.start..m.end].chars().count();
            SpellMiss {
                row,
                col_start,
                col_end,
                word: m.word.clone(),
                suggestions: shiki_core::spell::suggestions(&m.word, lang),
            }
        })
        .collect();
    SpellReport {
        misses: converted,
        checked_lines: lines.to_vec(),
    }
}

/// Applies the `UNDERLINED` modifier to the character ranges of `line` that
/// fall inside any `(start, end)` miss range (absolute char columns within
/// the source row). Rebuilds each span split at underline boundaries so a
/// correct word next to a misspelled one isn't underlined along with it.
pub fn underline_missed_ranges(
    line: Line<'static>,
    seg_start: usize,
    misses: &[(usize, usize)],
) -> Line<'static> {
    let in_miss = |col: usize| misses.iter().any(|&(s, e)| s <= col && col < e);
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut col = seg_start;
    for span in line.spans {
        let text = span.content.to_string();
        let mut run = String::new();
        let mut cur = in_miss(col);
        for (i, ch) in text.chars().enumerate() {
            let is_miss = in_miss(col + i);
            if is_miss != cur && !run.is_empty() {
                let style = miss_style(span.style, cur);
                out.push(Span::styled(std::mem::take(&mut run), style));
                cur = is_miss;
            }
            run.push(ch);
        }
        if !run.is_empty() {
            out.push(Span::styled(run, miss_style(span.style, cur)));
        }
        col += text.chars().count();
    }
    Line::from(out)
}

fn miss_style(base: Style, is_miss: bool) -> Style {
    if is_miss {
        base.add_modifier(Modifier::UNDERLINED)
    } else {
        base
    }
}

/// Recolors the character range `[start, start + len)` (absolute char
/// columns within the source row) of `line` with `style`, leaving everything
/// else untouched — the flash highlight over the word the spell-check popup
/// just replaced. `len == 0` is a no-op.
pub fn flash_range(
    line: Line<'static>,
    seg_start: usize,
    start: usize,
    len: usize,
    style: Style,
) -> Line<'static> {
    let in_flash = |col: usize| start <= col && col < start + len;
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut col = seg_start;
    for span in line.spans {
        let text = span.content.to_string();
        let mut run = String::new();
        let mut cur = in_flash(col);
        for (i, ch) in text.chars().enumerate() {
            let is_flash = in_flash(col + i);
            if is_flash != cur && !run.is_empty() {
                out.push(Span::styled(
                    std::mem::take(&mut run),
                    if cur { style } else { span.style },
                ));
                cur = is_flash;
            }
            run.push(ch);
        }
        if !run.is_empty() {
            out.push(Span::styled(run, if cur { style } else { span.style }));
        }
        col += text.chars().count();
    }
    Line::from(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn text(line: &Line<'static>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn build_report_converts_byte_offsets_to_row_and_col() {
        let text = "first line\nseconnd line\nthird";
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        // "seconnd" is at byte offset 11 in the whole text.
        let misses = vec![shiki_core::spell::Misspell {
            word: "seconnd".to_string(),
            start: 11,
            end: 18,
        }];
        let report = build_report(text, &lines, &misses, "");
        assert_eq!(report.checked_lines, lines);
        assert_eq!(report.misses.len(), 1);
        assert_eq!(report.misses[0].row, 1);
        assert_eq!(
            &lines[1][report.misses[0].col_start..report.misses[0].col_end],
            "seconnd"
        );
    }

    #[test]
    fn underlines_only_the_misspelled_range() {
        let line = Line::from(vec![Span::raw("good seconnd fine")]);
        // "seconnd" spans chars 5..12.
        let out = underline_missed_ranges(line, 0, &[(5, 12)]);
        assert_eq!(text(&out), "good seconnd fine");
        let underlined: Vec<&str> = out
            .spans
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::UNDERLINED))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(underlined, vec!["seconnd"]);
    }

    #[test]
    fn underlines_work_inside_a_wrapped_segment_offset() {
        // The segment line only holds the wrapped piece of the row: here
        // row-cols 3..15 ("good seconnd"), so the first char is at row col
        // 3. The miss "seconnd" sits at row-cols 8..15, i.e. segment chars
        // 5..12.
        let line = Line::from(vec![Span::raw("good seconnd")]);
        let out = underline_missed_ranges(line, 3, &[(8, 15)]);
        let underlined: String = out
            .spans
            .iter()
            .filter(|s| s.style.add_modifier.contains(Modifier::UNDERLINED))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(underlined, "seconnd");
    }

    #[test]
    fn empty_miss_list_leaves_line_untouched() {
        let line = Line::from(vec![Span::raw("plain text")]);
        let out = underline_missed_ranges(line, 0, &[]);
        assert_eq!(out.spans.len(), 1);
        assert!(!out.spans[0]
            .style
            .add_modifier
            .contains(Modifier::UNDERLINED));
    }

    #[test]
    fn flash_recolors_only_the_replaced_range() {
        let highlight = Style::default()
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let line = Line::from(vec![Span::raw("wrrds here")]);
        let out = flash_range(line, 0, 0, 5, highlight);
        assert_eq!(text(&out), "wrrds here");
        let flashed: Vec<&str> = out
            .spans
            .iter()
            .filter(|s| s.style.bg == Some(Color::Green))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(flashed, vec!["wrrds"]);
        let plain: Vec<&str> = out
            .spans
            .iter()
            .filter(|s| s.style.bg != Some(Color::Green))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(plain, vec![" here"]);
    }

    #[test]
    fn flash_respects_a_wrapped_segment_offset() {
        let highlight = Style::default().bg(Color::Green);
        let line = Line::from(vec![Span::raw("good seconnd")]);
        // Segment starts at row col 3; the replaced word is row-cols 8..15,
        // i.e. segment chars 5..12.
        let out = flash_range(line, 3, 8, 7, highlight);
        let flashed: String = out
            .spans
            .iter()
            .filter(|s| s.style.bg == Some(Color::Green))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(flashed, "seconnd");
    }

    #[test]
    fn flash_zero_length_is_a_no_op() {
        let line = Line::from(vec![Span::raw("unchanged")]);
        let out = flash_range(line, 0, 3, 0, Style::default().bg(Color::Green));
        assert_eq!(out.spans.len(), 1);
        assert_eq!(out.spans[0].style.bg, None);
    }
}

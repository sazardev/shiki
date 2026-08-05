use std::cell::Cell;

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use ratatui_textarea::TextArea;

/// One source line's `wrap_line` break points — `(start, end)` char-index
/// ranges, one per visual (post-wrap) segment. A plain type alias purely to
/// keep signatures like `char_rows_and_wraps`'s readable.
type WrapRanges = Vec<(usize, usize)>;

/// `TextArea::cursor` returns a `DataCursor` struct as of `ratatui-textarea` 0.9 (it used to
/// be a plain tuple in `tui-textarea` 0.7) — converts back to the `(row, col)` shape every
/// call site in this codebase already assumes.
pub(crate) fn cursor_tuple(ta: &TextArea) -> (usize, usize) {
    let ratatui_textarea::DataCursor(row, col) = ta.cursor();
    (row, col)
}

/// Thin wrapper over `ratatui-textarea` for the inline editor (key `e`/`i`).
///
/// This crate was `tui-textarea` 0.7 until the fix for
/// [GHSA-rhfx-m35p-ff5j](https://github.com/advisories/GHSA-rhfx-m35p-ff5j) (an `lru`
/// soundness issue pulled in transitively via `ratatui`) forced a bump to `ratatui` 0.30,
/// which `tui-textarea` 0.7 doesn't support — `ratatui-textarea` 0.9 is its
/// maintained-under-the-ratatui-org successor and the only version that does. At the time
/// this module's manual render path was written, that predecessor crate had no soft
/// line-wrap at all — confirmed against its own `Widget for &TextArea` impl: it always
/// rendered exactly one screen row per *source* line and horizontally scrolled to keep the
/// cursor visible instead, with no `set_wrap`-style toggle anywhere in its public API.
/// `ratatui-textarea` 0.9 does have one now (`WrapMode`), but this module still doesn't use
/// it — switching to it would be a real behavior change (its wrap semantics haven't been
/// verified against `wrap_line`'s own word/grapheme-wrap rules), not something to fold into
/// a dependency-swap. That's fine for the read-only PREVIEW panel, which builds its own
/// plain `Paragraph` and turns on `Wrap { trim: false }` itself (`panel_preview.rs`) — but it
/// meant a long line typed directly into the editor just ran off the edge
/// of the panel instead of wrapping, only becoming visible again once you
/// left edit mode and PREVIEW re-rendered the same text wrapped.
///
/// Rather than adopting the upstream `WrapMode` (see above) or patching it further, `render`
/// bypasses its `Widget` impl entirely and draws the buffer itself — `TextArea` still owns every
/// bit of actual editing (insert/delete/undo/cursor movement/selection all
/// keep going through `editor.textarea.input(key)` unchanged in
/// `key_handlers.rs`), it's just not used to *paint* itself anymore.
/// `wrap_line` computes the word-wrap break points once and reuses the
/// exact same ones both to lay out the visible text and to translate the
/// cursor's logical (row, col) into a screen row/col, so the two can never
/// disagree about where a wrapped line actually breaks.
pub struct InlineEditor<'a> {
    pub textarea: TextArea<'a>,
    /// Topmost visible *visual* (post-wrap) line, auto-following the
    /// cursor on every render — replaces `ratatui-textarea`'s own private
    /// `Viewport` scroll tracking (which only ever handled horizontal
    /// scroll, and is bypassed along with the rest of its rendering). A
    /// `Cell`, not a plain field, because `render` only borrows `&self` —
    /// the same reason `ratatui-textarea` itself stores its `Viewport` in an
    /// `AtomicU64` rather than a plain field: rendering can't take `&mut`.
    scroll_top: Cell<u16>,
    /// The cursor's screen row on the *last* render, relative to the
    /// editor's own inner area (post-block, post-scroll) — lets `draw.rs`
    /// anchor the `/`-menu popup right under the current line without
    /// duplicating the wrap/scroll math `render` already does.
    cursor_screen_row: Cell<u16>,
}

/// Bundles `InlineEditor::render`'s style/state parameters (everything
/// besides `&self`/`frame`/`area`) into one value — keeps the function's
/// own parameter count under clippy's `too_many_arguments` threshold, which
/// `typewriter_scroll` pushed past once it was added as a plain 8th param.
pub(crate) struct RenderOptions<'a> {
    pub line_numbers: bool,
    pub gutter_style: Style,
    pub secondary_cursor_style: Style,
    pub secondary_cursors: &'a [crate::multicursor::CursorState],
    pub typewriter_scroll: bool,
}

impl<'a> InlineEditor<'a> {
    pub fn from_contents(contents: &str) -> Self {
        let lines: Vec<String> = if contents.is_empty() {
            vec![String::new()]
        } else {
            contents.lines().map(str::to_string).collect()
        };
        Self {
            textarea: TextArea::new(lines),
            scroll_top: Cell::new(0),
            cursor_screen_row: Cell::new(0),
        }
    }

    pub fn contents(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// The cursor's row within the editor's inner area as of the last
    /// `render` call — see the field's own doc comment.
    pub fn cursor_screen_row(&self) -> u16 {
        self.cursor_screen_row.get()
    }

    /// `area` minus whatever block/border `ratatui-textarea` has configured —
    /// the one place this match lives, reused by `render`, `position_at`,
    /// and `draw.rs`'s slash-menu anchoring (which used to duplicate this
    /// same match independently).
    pub fn inner_area(&self, area: Rect) -> Rect {
        match self.textarea.block() {
            Some(block) => block.inner(area),
            None => area,
        }
    }

    /// Columns reserved for the line-number gutter when `line_numbers` is
    /// on — digit count of the last line number, plus one padding column —
    /// or `0` when the feature is off, so callers can subtract it from the
    /// available width unconditionally without an extra branch.
    fn gutter_width(&self, line_numbers: bool) -> u16 {
        if !line_numbers {
            return 0;
        }
        (self.textarea.lines().len().to_string().len() + 1) as u16
    }

    /// Computes each source line's characters plus its `wrap_line` break
    /// points, at `width` columns — the one place both `render` and
    /// `position_at` derive this, so a hit-test can never disagree with
    /// what was actually painted (see the struct doc comment's own
    /// reasoning for why `render` bypasses `ratatui-textarea`'s `Widget` impl
    /// in the first place).
    fn char_rows_and_wraps(&self, width: usize) -> (Vec<Vec<char>>, Vec<WrapRanges>) {
        let char_rows: Vec<Vec<char>> = self
            .textarea
            .lines()
            .iter()
            .map(|l| l.chars().collect())
            .collect();
        let wraps: Vec<WrapRanges> = char_rows
            .iter()
            .map(|chars| wrap_line(chars, width))
            .collect();
        (char_rows, wraps)
    }

    /// Inverse of `render`'s own painting loop: maps a screen coordinate
    /// (already known to be a mouse event's `column`/`row`) back onto a
    /// logical `(row, col)` in the buffer, or `None` if it falls outside
    /// the editor's inner area. Relies on `scroll_top` as of the *last*
    /// `render` call, same assumption `cursor_screen_row()` already makes.
    pub fn position_at(
        &self,
        area: Rect,
        line_numbers: bool,
        column: u16,
        row: u16,
    ) -> Option<(usize, usize)> {
        let inner = self.inner_area(area);
        if inner.width == 0
            || inner.height == 0
            || column < inner.x
            || column >= inner.x + inner.width
            || row < inner.y
            || row >= inner.y + inner.height
        {
            return None;
        }
        let gutter = self.gutter_width(line_numbers);
        let width = (inner.width as usize)
            .saturating_sub(gutter as usize)
            .max(1);
        let (char_rows, wraps) = self.char_rows_and_wraps(width);
        let target_visual_row = self.scroll_top.get() as usize + (row - inner.y) as usize;
        let col_in_row = (column - inner.x).saturating_sub(gutter) as usize;

        let mut visual_row = 0usize;
        for (logical_row, _) in char_rows.iter().enumerate() {
            for &(start, end) in &wraps[logical_row] {
                if visual_row == target_visual_row {
                    return Some((logical_row, (start + col_in_row).min(end)));
                }
                visual_row += 1;
            }
        }
        // Clicked below the last rendered line — clamp to the end of the
        // buffer rather than returning `None`, so a click just past the
        // last line of a short note still lands somewhere sensible.
        let last_row = char_rows.len().saturating_sub(1);
        Some((last_row, char_rows.get(last_row).map(Vec::len).unwrap_or(0)))
    }

    /// Renders the buffer word-wrapped to `area`'s width, instead of
    /// `ratatui-textarea`'s own unwrapped-with-horizontal-scroll rendering —
    /// see the struct doc comment for why this exists at all.
    pub(crate) fn render(&self, frame: &mut Frame, area: Rect, opts: RenderOptions) {
        if let Some(block) = self.textarea.block() {
            frame.render_widget(block.clone(), area);
        }
        let inner = self.inner_area(area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let gutter = self.gutter_width(opts.line_numbers);
        let width = (inner.width as usize)
            .saturating_sub(gutter as usize)
            .max(1);
        let height = inner.height as usize;

        // Bypassing `ratatui-textarea`'s own `Widget` impl (see the struct doc
        // comment) means its placeholder-when-empty behavior needs its own
        // reimplementation too — same trigger condition as its `widget.rs`
        // (`!placeholder.is_empty() && self.is_empty()`), reusing
        // `wrap_line` so a long placeholder still wraps instead of running
        // off the edge.
        if !self.textarea.placeholder_text().is_empty() && self.textarea.is_empty() {
            self.scroll_top.set(0);
            self.cursor_screen_row.set(0);
            let chars: Vec<char> = self.textarea.placeholder_text().chars().collect();
            let style = self.textarea.placeholder_style().unwrap_or_default();
            let gutter_pad = " ".repeat(gutter as usize);
            let lines: Vec<Line<'static>> = wrap_line(&chars, width)
                .into_iter()
                .map(|(s, e)| {
                    Line::from(vec![
                        Span::raw(gutter_pad.clone()),
                        Span::styled(chars[s..e].iter().collect::<String>(), style),
                    ])
                })
                .collect();
            let paragraph = Paragraph::new(lines).style(self.textarea.style());
            frame.render_widget(paragraph, inner);
            return;
        }

        let (char_rows, wraps) = self.char_rows_and_wraps(width);

        let (cursor_row, cursor_col) = cursor_tuple(&self.textarea);
        let (cursor_local_row, _) = locate_in_wrap(&wraps[cursor_row], cursor_col);
        let visual_offset_of: usize = wraps[..cursor_row].iter().map(Vec::len).sum();
        let cursor_visual_row = visual_offset_of + cursor_local_row;
        let total_visual_rows: usize = wraps.iter().map(Vec::len).sum();

        // `typewriter_scroll` (`config.editor.typewriter_scroll`): keep the
        // cursor's row vertically centered instead of only scrolling once
        // it reaches the viewport's edge — a real re-centering every
        // keystroke, not a threshold like `next_scroll_top`'s default.
        let mut scroll_top = if opts.typewriter_scroll {
            (cursor_visual_row as u16).saturating_sub(height as u16 / 2)
        } else {
            next_scroll_top(
                self.scroll_top.get(),
                cursor_visual_row as u16,
                height as u16,
            )
        };
        let max_scroll = total_visual_rows.saturating_sub(height) as u16;
        scroll_top = scroll_top.min(max_scroll);
        self.scroll_top.set(scroll_top);
        let scroll_top = scroll_top as usize;
        self.cursor_screen_row
            .set((cursor_visual_row as u16).saturating_sub(scroll_top as u16));

        let styles = RowStyles {
            base: self.textarea.style(),
            cursor: self.textarea.cursor_style(),
            cursor_line: self.textarea.cursor_line_style(),
            // `ratatui-textarea` only exposes `selection_style` via a getter
            // that oddly takes `&mut self` (a quirk of its 0.7.0 API) and
            // shiki never calls `set_selection_style`, so this is the same
            // value `TextArea::default()` already uses internally.
            select: Style::default().bg(Color::LightBlue),
            secondary_cursor: opts.secondary_cursor_style,
        };
        let selection = self.textarea.selection_range();

        let mut rendered: Vec<Line<'static>> = Vec::with_capacity(height);
        let mut visual_row = 0usize;
        'rows: for (row, chars) in char_rows.iter().enumerate() {
            for (local_idx, &seg) in wraps[row].iter().enumerate() {
                if visual_row >= scroll_top + height {
                    break 'rows;
                }
                if visual_row >= scroll_top {
                    let ctx = SegmentCtx {
                        row,
                        cursor_row,
                        cursor_col,
                        is_cursor_segment: row == cursor_row && local_idx == cursor_local_row,
                        selection,
                        secondary: opts.secondary_cursors,
                    };
                    let mut line = build_segment_line(chars, seg, &ctx, &styles);
                    if gutter > 0 {
                        let prefix = if local_idx == 0 {
                            format!("{:>w$} ", row + 1, w = (gutter as usize).saturating_sub(1))
                        } else {
                            " ".repeat(gutter as usize)
                        };
                        line.spans
                            .insert(0, Span::styled(prefix, opts.gutter_style));
                    }
                    rendered.push(line);
                }
                visual_row += 1;
            }
        }

        let paragraph = Paragraph::new(rendered)
            .style(styles.base)
            .alignment(self.textarea.alignment());
        frame.render_widget(paragraph, inner);
    }
}

struct RowStyles {
    base: Style,
    cursor: Style,
    cursor_line: Style,
    select: Style,
    /// A solid, theme-accent-colored block — deliberately *not* the same
    /// style as `cursor` (a bare `Modifier::REVERSED`, ratatui-textarea's
    /// default). A terminal can only ever blink *one* real caret, so
    /// secondary cursors can't blink like the primary does regardless of
    /// styling — but reusing the exact same subtle reverse-video look for
    /// both made every secondary cursor easy to miss entirely at a glance
    /// (reported live: "solo hay 1 visualmente, no parpadean varios
    /// cursores"). A distinct solid color is the compensating signal real
    /// multi-cursor TUIs (e.g. Helix) use for the same hard constraint.
    secondary_cursor: Style,
}

struct SegmentCtx<'a> {
    row: usize,
    cursor_row: usize,
    cursor_col: usize,
    /// Whether *this* wrapped segment (not just `row`) is the one
    /// `locate_in_wrap` picked as the cursor's visual line — a row that
    /// wraps into several segments must highlight the cursor in exactly
    /// one of them, never zero or more than one.
    is_cursor_segment: bool,
    selection: Option<((usize, usize), (usize, usize))>,
    /// `config.editor.multi_cursor`'s extra cursors, each with its own
    /// independent selection — painted the same way the primary
    /// cursor/selection already is, just looped over instead of singular.
    secondary: &'a [crate::multicursor::CursorState],
}

/// Whether `(row, col)` falls within `[start, end)` (inclusive of every
/// row strictly between them) — the one range-check both the primary
/// selection and every secondary cursor's own selection use, so they can't
/// disagree on what "inside the selection" means at a row boundary.
fn pos_in_range(row: usize, col: usize, start: (usize, usize), end: (usize, usize)) -> bool {
    let (sr, sc) = start;
    let (er, ec) = end;
    if row < sr || row > er {
        false
    } else if sr == er {
        col >= sc && col < ec
    } else if row == sr {
        col >= sc
    } else if row == er {
        col < ec
    } else {
        true
    }
}

impl SegmentCtx<'_> {
    fn is_selected(&self, col: usize) -> bool {
        if let Some((start, end)) = self.selection {
            if pos_in_range(self.row, col, start, end) {
                return true;
            }
        }
        self.secondary.iter().any(|c| {
            c.anchor.is_some_and(|anchor| {
                pos_in_range(self.row, col, anchor.min(c.pos), anchor.max(c.pos))
            })
        })
    }

    /// Whether `(self.row, col)` is exactly one of the secondary cursors'
    /// own position — painted with the same `cursor` style the primary
    /// cursor uses, distinguishing them from ordinary selected text.
    fn is_secondary_cursor(&self, col: usize) -> bool {
        self.secondary.iter().any(|c| c.pos == (self.row, col))
    }
}

/// Builds one visual (already-wrapped) screen line's spans — plain runs
/// get `cursor_line` style when `ctx.row` is the cursor's row (mirroring
/// `ratatui-textarea`'s own current-line underline), individual selected
/// characters get `select`, and the cursor's own cell gets `cursor`
/// (a styled space past the last character when the cursor sits at the
/// end of the line, same fallback `ratatui-textarea`'s `LineHighlighter` uses).
fn build_segment_line(
    chars: &[char],
    (start, end): (usize, usize),
    ctx: &SegmentCtx,
    styles: &RowStyles,
) -> Line<'static> {
    let line_style = if ctx.row == ctx.cursor_row {
        styles.cursor_line
    } else {
        Style::default()
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut plain = String::new();

    for (col, &ch) in chars.iter().enumerate().take(end).skip(start) {
        let is_primary_cursor =
            ctx.is_cursor_segment && ctx.row == ctx.cursor_row && col == ctx.cursor_col;
        if is_primary_cursor {
            if !plain.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut plain), line_style));
            }
            spans.push(Span::styled(ch.to_string(), styles.cursor));
        } else if ctx.is_secondary_cursor(col) {
            if !plain.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut plain), line_style));
            }
            spans.push(Span::styled(ch.to_string(), styles.secondary_cursor));
        } else if ctx.is_selected(col) {
            if !plain.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut plain), line_style));
            }
            spans.push(Span::styled(ch.to_string(), styles.select));
        } else {
            plain.push(ch);
        }
    }
    if !plain.is_empty() {
        spans.push(Span::styled(plain, line_style));
    }
    if ctx.is_cursor_segment && ctx.row == ctx.cursor_row && ctx.cursor_col == end {
        spans.push(Span::styled(" ", styles.cursor));
    } else if ctx.is_secondary_cursor(end) {
        // A secondary cursor sitting past the last character of the line
        // (very common — it's exactly where a cursor lands right after
        // typing) has no character of its own to attach a style to,
        // same fallback the primary cursor's own end-of-line case above
        // already needed.
        spans.push(Span::styled(" ", styles.secondary_cursor));
    }

    Line::from(spans)
}

/// Word-wraps `chars` to `width` columns, returning each visual sub-line's
/// `(start, end)` char-index range (end exclusive). Computed ourselves —
/// rather than relying on `ratatui::widgets::Paragraph`'s built-in wrap,
/// whose exact break points aren't exposed by its public API — so the
/// very same break points can also be used by `locate_in_wrap` to map the
/// cursor's logical (row, col) onto the right visual row/col.
fn wrap_line(chars: &[char], width: usize) -> Vec<(usize, usize)> {
    let len = chars.len();
    if len == 0 {
        return vec![(0, 0)];
    }
    if width == 0 {
        return vec![(0, len)];
    }
    let mut ranges = Vec::new();
    let mut start = 0;
    while len - start > width {
        let limit = start + width;
        let mut break_at = None;
        let mut i = limit;
        while i > start {
            if chars[i] == ' ' {
                break_at = Some(i);
                break;
            }
            i -= 1;
        }
        let end = match break_at {
            Some(pos) if pos > start => pos,
            // No space anywhere in this width's worth of the line (one
            // very long word) — hard-break mid-word, same as `Paragraph`'s
            // own wrap does in that situation.
            _ => limit,
        };
        ranges.push((start, end));
        start = if chars.get(end) == Some(&' ') {
            end + 1
        } else {
            end
        };
    }
    ranges.push((start, len));
    ranges
}

/// Maps a char-index `col` within a source line onto `(local visual row,
/// visual col)` given that line's own `wrap_line` output. A `col` sitting
/// exactly on a wrap boundary belongs to the *earlier* segment only when a
/// space was actually dropped there (a soft break) — for a hard break
/// (a word too long to fit, split with no space to drop) the boundary
/// value is itself the first character of the *next* segment, so it must
/// resolve there instead, or the cursor would visually land one segment
/// too early.
fn locate_in_wrap(ranges: &[(usize, usize)], col: usize) -> (usize, usize) {
    for (i, &(s, e)) in ranges.iter().enumerate() {
        let is_last = i + 1 == ranges.len();
        let next_start = if is_last { e } else { ranges[i + 1].0 };
        let boundary_is_gap = next_start > e;
        if col < e || (col == e && (is_last || boundary_is_gap)) {
            return (i, col - s);
        }
    }
    let last = ranges.len() - 1;
    (last, col - ranges[last].0)
}

/// Classifies a char the way a double-click "select the word under the
/// cursor" needs to: whitespace never joins a word, and a run of
/// word-characters (alphanumeric/`_`) is a different "word" than an
/// adjacent run of punctuation — `"foo.bar"` double-clicked on `foo` should
/// select just `foo`, not the whole dotted path. Mirrors the idea of
/// `ratatui-textarea`'s own internal (non-`pub`, unreachable from shiki)
/// `word.rs` classifier, reimplemented here since it can't be imported.
#[derive(PartialEq, Eq)]
enum CharKind {
    Space,
    Word,
    Punct,
}

fn char_kind(c: char) -> CharKind {
    if c.is_whitespace() {
        CharKind::Space
    } else if c.is_alphanumeric() || c == '_' {
        CharKind::Word
    } else {
        CharKind::Punct
    }
}

/// The char-range `[start, end)` of the "word" containing `col` within
/// `chars` — used by double-click (select word) and, later, by Ctrl+D
/// (select the word under the cursor as the initial multi-select query).
/// `col == chars.len()` (cursor sitting past the last character) uses the
/// *previous* character's kind, matching where a click there visually is.
pub fn word_range(chars: &[char], col: usize) -> (usize, usize) {
    if chars.is_empty() {
        return (0, 0);
    }
    let at = col.min(chars.len() - 1);
    let kind = char_kind(chars[at]);
    if kind == CharKind::Space {
        return (at, at + 1);
    }
    let mut start = at;
    while start > 0 && char_kind(chars[start - 1]) == kind {
        start -= 1;
    }
    let mut end = at + 1;
    while end < chars.len() && char_kind(chars[end]) == kind {
        end += 1;
    }
    (start, end)
}

/// Every occurrence of `query` across `lines`, case-insensitive, as
/// `(row, start_col, end_col)` char-index triples in document order —
/// plain literal substring matching (no regex), which is what Ctrl+F
/// searches by default. A pure function of `&[String]` (not `&TextArea`)
/// so it's independently testable, same reasoning as `word_range`.
pub(crate) fn find_all_matches(lines: &[String], query: &str) -> Vec<(usize, usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let query_lower: Vec<char> = query.to_lowercase().chars().collect();
    let qlen = query_lower.len();
    let mut matches = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let lower: Vec<char> = line.to_lowercase().chars().collect();
        if lower.len() < qlen {
            continue;
        }
        let mut col = 0;
        while col + qlen <= lower.len() {
            if lower[col..col + qlen] == query_lower[..] {
                matches.push((row, col, col + qlen));
                col += qlen;
            } else {
                col += 1;
            }
        }
    }
    matches
}

/// The next (or, if `backward`, previous) match strictly after/before
/// `from` in document order, wrapping around the whole buffer when there
/// is none — same "find next wraps to the top" behavior a real editor's
/// find bar has. `from` is compared against each match's *start* position.
pub(crate) fn next_match(
    matches: &[(usize, usize, usize)],
    from: (usize, usize),
    backward: bool,
) -> Option<(usize, usize, usize)> {
    if backward {
        matches
            .iter()
            .rev()
            .find(|&&(r, s, _)| (r, s) < from)
            .or_else(|| matches.last())
            .copied()
    } else {
        matches
            .iter()
            .find(|&&(r, s, _)| (r, s) > from)
            .or_else(|| matches.first())
            .copied()
    }
}

/// The literal text between two logical `(row, col)` positions (`start`
/// assumed `<= end` in document order) — used to seed Ctrl+F's query from
/// an existing selection, and to read out arbitrary selected text in
/// general. A pure function of `&[String]`, not `&TextArea`, for the same
/// testability reason as `find_all_matches`.
pub(crate) fn selection_text(
    lines: &[String],
    start: (usize, usize),
    end: (usize, usize),
) -> String {
    let (sr, sc) = start;
    let (er, ec) = end;
    if sr >= lines.len() {
        return String::new();
    }
    if sr == er {
        let chars: Vec<char> = lines[sr].chars().collect();
        let sc = sc.min(chars.len());
        let ec = ec.min(chars.len());
        return chars[sc..ec].iter().collect();
    }
    let last_row = er.min(lines.len().saturating_sub(1));
    let mut out = String::new();
    for (row, line) in lines.iter().enumerate().take(last_row + 1).skip(sr) {
        let chars: Vec<char> = line.chars().collect();
        if row == sr {
            let sc = sc.min(chars.len());
            out.extend(&chars[sc..]);
        } else if row == er {
            let ec = ec.min(chars.len());
            out.extend(&chars[..ec]);
        } else {
            out.push_str(line);
        }
        if row != er {
            out.push('\n');
        }
    }
    out
}

/// Smallest vertical scroll adjustment that keeps `cursor` inside a
/// `height`-row-tall viewport currently topped at `prev_top` — scrolls up
/// the instant the cursor moves above it, or down the instant it moves
/// past the bottom, and otherwise leaves the viewport exactly where it
/// was (no re-centering on every keystroke).
fn next_scroll_top(prev_top: u16, cursor: u16, height: u16) -> u16 {
    if height == 0 {
        return 0;
    }
    if cursor < prev_top {
        cursor
    } else if prev_top + height <= cursor {
        cursor + 1 - height
    } else {
        prev_top
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn wraps_on_word_boundary() {
        let line = chars("hello world foo");
        let ranges = wrap_line(&line, 11);
        assert_eq!(ranges, vec![(0, 11), (12, 15)]);
    }

    #[test]
    fn hard_breaks_a_single_long_word() {
        let line = chars("abcdefghijklmnopqrstu"); // 21 chars, no spaces
        let ranges = wrap_line(&line, 5);
        assert_eq!(ranges, vec![(0, 5), (5, 10), (10, 15), (15, 20), (20, 21)]);
    }

    #[test]
    fn empty_line_yields_one_empty_range() {
        assert_eq!(wrap_line(&[], 10), vec![(0, 0)]);
    }

    #[test]
    fn locate_maps_cursor_after_a_dropped_space() {
        // "hello world foo" wrapped at 11 -> [(0,11), (12,15)]; col 11 is
        // the dropped space itself, so it belongs to the earlier segment.
        let ranges = vec![(0, 11), (12, 15)];
        assert_eq!(locate_in_wrap(&ranges, 11), (0, 11));
        assert_eq!(locate_in_wrap(&ranges, 12), (1, 0));
    }

    #[test]
    fn locate_maps_cursor_at_a_hard_break_boundary() {
        // No space dropped here, so the boundary value belongs to the
        // *next* segment (it's that segment's first real character).
        let ranges = vec![(0, 5), (5, 10)];
        assert_eq!(locate_in_wrap(&ranges, 5), (1, 0));
    }

    #[test]
    fn scroll_follows_cursor_in_both_directions() {
        assert_eq!(next_scroll_top(0, 3, 5), 0); // cursor already visible
        assert_eq!(next_scroll_top(0, 7, 5), 3); // cursor below viewport
        assert_eq!(next_scroll_top(4, 1, 5), 1); // cursor above viewport
    }

    #[test]
    fn word_range_selects_the_word_under_the_cursor() {
        let line = chars("hello world");
        assert_eq!(word_range(&line, 2), (0, 5)); // inside "hello"
        assert_eq!(word_range(&line, 0), (0, 5)); // at its start
        assert_eq!(word_range(&line, 6), (6, 11)); // inside "world"
    }

    #[test]
    fn word_range_stops_at_punctuation() {
        let line = chars("foo.bar");
        assert_eq!(word_range(&line, 1), (0, 3)); // "foo"
        assert_eq!(word_range(&line, 3), (3, 4)); // the "." itself
        assert_eq!(word_range(&line, 5), (4, 7)); // "bar"
    }

    #[test]
    fn word_range_on_whitespace_selects_just_that_cell() {
        let line = chars("a b");
        assert_eq!(word_range(&line, 1), (1, 2));
    }

    #[test]
    fn word_range_clamps_past_the_last_character() {
        let line = chars("hi");
        assert_eq!(word_range(&line, 5), (0, 2));
    }

    #[test]
    fn word_range_on_empty_line() {
        assert_eq!(word_range(&[], 0), (0, 0));
    }

    #[test]
    fn position_at_maps_a_click_back_to_the_clicked_character() {
        let editor = InlineEditor::from_contents("hello\nworld foo");
        let area = Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 5,
        };
        // Render once so `scroll_top`/wrap state is established, same
        // precondition `cursor_screen_row()` already documents.
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(20, 5))
            .expect("test backend");
        terminal
            .draw(|frame| {
                editor.render(
                    frame,
                    area,
                    RenderOptions {
                        line_numbers: false,
                        gutter_style: Style::default(),
                        secondary_cursor_style: Style::default(),
                        secondary_cursors: &[],
                        typewriter_scroll: false,
                    },
                )
            })
            .unwrap();

        // Row 0 is "hello", clicking column 3 lands on the 'l' at index 3.
        assert_eq!(editor.position_at(area, false, 3, 0), Some((0, 3)));
        // Row 1 is "world foo", clicking column 6 lands on 'f' at index 6.
        assert_eq!(editor.position_at(area, false, 6, 1), Some((1, 6)));
        // Outside the area entirely.
        assert_eq!(editor.position_at(area, false, 30, 0), None);
    }

    #[test]
    fn position_at_accounts_for_the_line_number_gutter() {
        let editor = InlineEditor::from_contents("hello");
        let area = Rect {
            x: 0,
            y: 0,
            width: 20,
            height: 5,
        };
        let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(20, 5))
            .expect("test backend");
        terminal
            .draw(|frame| {
                editor.render(
                    frame,
                    area,
                    RenderOptions {
                        line_numbers: true,
                        gutter_style: Style::default(),
                        secondary_cursor_style: Style::default(),
                        secondary_cursors: &[],
                        typewriter_scroll: false,
                    },
                )
            })
            .unwrap();
        // Gutter for a 1-line file is "1 " (2 columns) — a click landing
        // in the gutter itself clamps to column 0 of the actual text.
        assert_eq!(editor.position_at(area, true, 0, 0), Some((0, 0)));
        assert_eq!(editor.position_at(area, true, 2, 0), Some((0, 0)));
    }

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn find_all_matches_is_case_insensitive_and_non_overlapping() {
        let doc = lines(&["Hello hello", "say HELLO again"]);
        let matches = find_all_matches(&doc, "hello");
        assert_eq!(matches, vec![(0, 0, 5), (0, 6, 11), (1, 4, 9)]);
    }

    #[test]
    fn find_all_matches_returns_nothing_for_an_empty_query() {
        assert_eq!(find_all_matches(&lines(&["anything"]), ""), Vec::new());
    }

    #[test]
    fn next_match_wraps_around_in_both_directions() {
        let matches = vec![(0, 0, 3), (1, 2, 5), (2, 0, 3)];
        // Forward from the last match wraps to the first.
        assert_eq!(next_match(&matches, (2, 0), false), Some((0, 0, 3)));
        // Forward from before the first match lands on the first.
        assert_eq!(next_match(&matches, (0, 0), false), Some((1, 2, 5)));
        // Backward from the first match wraps to the last.
        assert_eq!(next_match(&matches, (0, 0), true), Some((2, 0, 3)));
        // Backward from after the last match lands on the last.
        assert_eq!(next_match(&matches, (2, 3), true), Some((2, 0, 3)));
    }

    #[test]
    fn next_match_none_when_there_are_no_matches() {
        assert_eq!(next_match(&[], (0, 0), false), None);
    }

    #[test]
    fn selection_text_extracts_a_single_line_range() {
        let doc = lines(&["hello world"]);
        assert_eq!(selection_text(&doc, (0, 6), (0, 11)), "world");
    }

    #[test]
    fn selection_text_extracts_across_multiple_lines() {
        let doc = lines(&["hello world", "second line", "third"]);
        assert_eq!(
            selection_text(&doc, (0, 6), (2, 3)),
            "world\nsecond line\nthi"
        );
    }
}

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Focus, Mode};
use crate::icons;
use crate::render::hex_to_color;

/// Whitespace-separated word count — good enough for an estimate next to
/// the character count, not a publishing-grade word counter (doesn't try
/// to special-case punctuation-only "words" or CJK text without spaces).
fn word_count(body: &str) -> usize {
    body.split_whitespace().count()
}

/// Rounded up to the nearest minute at the common 200wpm estimate (the
/// same figure Medium/most reading-time plugins use), with a floor of 1
/// for any non-empty note — "0 min read" reads as broken, not as "very
/// short".
fn reading_time_minutes(words: usize) -> usize {
    if words == 0 {
        return 0;
    }
    words.div_ceil(200).max(1)
}

/// Only worth announcing when it's not the default — a "NORMAL" label on
/// every frame is noise, but INSERT/EDIT/VISUAL are worth flagging.
fn mode_label(mode: Mode) -> Option<&'static str> {
    match mode {
        Mode::Normal => None,
        Mode::Insert => Some("INSERT"),
        Mode::Edit => Some("EDIT"),
        Mode::Visual => Some("VISUAL"),
    }
}

/// Frames for the sync-in-progress spinner — advanced once per `run()`
/// iteration (`App::spinner_frame`) while `App::sync_in_flight` is set, so
/// it animates at the same ~10Hz cadence as the render loop itself.
const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Shortens `text` to at most `max_chars`, marking the cut with `…` — for
/// fitting the status message into whatever footer space is actually left
/// on narrow/small terminals instead of overflowing into the right-aligned
/// help/version text.
fn truncate_to(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    let mut truncated: String = text.chars().take(max_chars - 1).collect();
    truncated.push('…');
    truncated
}

/// URL opened by clicking the footer's Buy Me a Coffee segment.
pub const COFFEE_URL: &str = "https://buymeacoffee.com/sazarcode";

/// Builds the footer's right-aligned text and the char-column range within
/// it occupied by the coffee segment — factored out so `render` and
/// `coffee_hit_at` can never disagree about where the clickable area
/// actually is, the same reasoning `git_status_color`/`git_status_suffix`
/// being shared between the footer and the drawer was already built on.
fn right_text(show_coffee_link: bool) -> (String, std::ops::Range<usize>) {
    let coffee = if show_coffee_link {
        format!("{}Support", icons::COFFEE)
    } else {
        String::new()
    };
    let prefix = if coffee.is_empty() { "" } else { "  " };
    let suffix = format!(
        "   {}? help   v{}  ",
        icons::KEYBOARD,
        env!("CARGO_PKG_VERSION")
    );
    let text = format!("{prefix}{coffee}{suffix}");
    let start = prefix.chars().count();
    let end = start + coffee.chars().count();
    (text, start..end)
}

/// Hit-tests a mouse click against the footer's coffee segment, replaying
/// the same right-alignment math ratatui applies when rendering
/// `right_text()`'s output into `area` — a plain function of coordinates,
/// not `&App`, so it's unit-testable the same way `panel_drawer::drawer_hit_at`
/// is. Always misses when `show_coffee_link` is off, since the segment's
/// range then collapses to zero width.
pub fn coffee_hit_at(area: Rect, column: u16, row: u16, show_coffee_link: bool) -> bool {
    if row != area.y {
        return false;
    }
    let (text, range) = right_text(show_coffee_link);
    let content_width = text.chars().count() as u16;
    if content_width > area.width {
        return false; // clipped from the left, same as ratatui would render it
    }
    let text_start = area.x + area.width - content_width;
    let hit_start = text_start + range.start as u16;
    let hit_end = text_start + range.end as u16;
    column >= hit_start && column < hit_end
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);
    let accent = hex_to_color(&app.theme.accent);
    let plain = Style::default();
    let sep = Span::styled(" │ ", plain.fg(muted));

    let mut spans = Vec::new();

    if let Some(label) = mode_label(app.mode) {
        let count = app.visual_selection_count();
        let text = if count > 0 {
            format!("{label} ({count} selected) ")
        } else {
            format!("{label} ")
        };
        spans.push(Span::styled(
            text,
            plain.fg(accent).add_modifier(Modifier::BOLD),
        ));
        spans.push(sep.clone());
    }

    let notebook_name = app
        .selected_notebook()
        .map(|nb| nb.name.as_str())
        .unwrap_or("-");
    spans.push(Span::styled(
        format!("{}{notebook_name}", icons::NOTEBOOK),
        plain.fg(fg),
    ));

    // Contextual metadata (char/word count, reading time, note-count
    // breakdown, revision count) — skipped entirely in compact mode,
    // leaving just the essentials (notebook, git status, editor mode).
    if !app.config.general.compact_footer {
        spans.push(sep.clone());

        // Character count of the note actually being read (Notes/Preview,
        // something selected), otherwise how many notes are in view (e.g.
        // while still browsing NOTEBOOKS).
        let meta = match app.selected_note() {
            Some(note) if matches!(app.focus, Focus::Notes | Focus::Preview) => {
                let words = word_count(&note.body);
                format!(
                    "{}{} chars · {words} words · {} min read",
                    icons::NOTE,
                    note.body.chars().count(),
                    reading_time_minutes(words)
                )
            }
            _ => format!("{}{} notes", icons::NOTE, app.notes.len()),
        };
        spans.push(Span::styled(meta, plain.fg(fg)));

        // Note version history — how many commits have touched this
        // specific note, only while actually reading one in PREVIEW (not
        // while just browsing NOTES, where it'd compete with the char/note
        // count above).
        if app.focus == Focus::Preview {
            if let Some(count) = app.note_revision_count() {
                spans.push(sep.clone());
                spans.push(Span::styled(
                    format!("{}{count} changes", icons::HISTORY),
                    plain.fg(muted),
                ));
            }
        }
    }

    // While a sync/push/pull is running in the background (`spawn_git_op`),
    // this replaces the normal git-status segment rather than sitting next
    // to it — the point is to make it obvious *something's* happening
    // instead of the UI just looking idle for however long the network
    // call takes, not to duplicate information that's about to be stale
    // the moment the result comes back anyway.
    if let Some(label) = &app.sync_in_flight {
        spans.push(sep.clone());
        let frame = SPINNER_FRAMES[app.spinner_frame % SPINNER_FRAMES.len()];
        spans.push(Span::styled(
            format!("{frame} syncing '{label}'…"),
            plain.fg(accent),
        ));
    } else if app.git_status.is_repo {
        spans.push(sep.clone());
        let gs = &app.git_status;
        let branch = gs.branch.as_deref().unwrap_or("?");
        let extras = crate::render::git_status_suffix(gs);
        let color = crate::render::git_status_color(&app.theme, gs);
        spans.push(Span::styled(
            format!("{}{branch}{extras}", icons::GIT),
            plain.fg(color),
        ));
    }

    spans.push(sep.clone());
    let editor_color = if app.config.general.use_favorite_editor {
        accent
    } else {
        muted
    };
    spans.push(Span::styled(
        format!("{}{}", icons::PENCIL, app.editor_status_label()),
        plain.fg(editor_color),
    ));

    if app.leader_pending {
        spans.push(sep.clone());
        spans.push(Span::styled(
            format!("{}leader…", icons::KEYBOARD),
            plain.fg(accent).add_modifier(Modifier::BOLD),
        ));
    }

    let (right, coffee_range) = right_text(app.config.general.show_coffee_link);

    if let Some(status) = &app.status_message {
        // Truncated to whatever room is actually left, so a long message
        // can't push the right-aligned help/version text out of view or
        // overlap it — reserved space is the *actual* right-text length,
        // not a hardcoded guess, so it can't drift out of sync with it.
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        let budget = (area.width as usize)
            .saturating_sub(used)
            .saturating_sub(right.chars().count());
        if budget > 1 {
            spans.push(sep);
            spans.push(Span::styled(truncate_to(status, budget), plain.fg(fg)));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);

    // Right-aligned over the same area — no background is painted anywhere
    // on this bar, so this just draws on top of empty space rather than
    // needing a separately carved-out sub-rect. The coffee segment gets its
    // own styled color (accent) so it reads as clickable; everything else
    // in this line stays muted, same as before.
    let coffee_text: String = right.chars().take(coffee_range.end).collect();
    let coffee_text: String = coffee_text.chars().skip(coffee_range.start).collect();
    let before: String = right.chars().take(coffee_range.start).collect();
    let after: String = right.chars().skip(coffee_range.end).collect();
    let right_line = Line::from(vec![
        Span::styled(before, plain.fg(muted)),
        Span::styled(coffee_text, plain.fg(accent)),
        Span::styled(after, plain.fg(muted)),
    ]);
    frame.render_widget(Paragraph::new(right_line).alignment(Alignment::Right), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coffee_hit_at_lands_inside_the_coffee_segment() {
        let (text, range) = right_text(true);
        let width = text.chars().count() as u16;
        let area = Rect::new(0, 5, width, 1);
        let text_start = area.x; // content exactly fills the area here
        let mid_col = text_start + (range.start as u16 + range.end as u16) / 2;
        assert!(coffee_hit_at(area, mid_col, 5, true));
    }

    #[test]
    fn coffee_hit_at_misses_outside_the_coffee_segment() {
        let (text, _range) = right_text(true);
        let width = text.chars().count() as u16;
        let area = Rect::new(0, 5, width, 1);
        // The very first column is inside the leading "  " padding, before
        // the coffee segment starts.
        assert!(!coffee_hit_at(area, area.x, 5, true));
        // Wrong row entirely.
        assert!(!coffee_hit_at(area, area.x + 3, 6, true));
    }

    #[test]
    fn coffee_hit_at_shifts_with_wider_area_since_alignment_is_right() {
        let (text, range) = right_text(true);
        let width = text.chars().count() as u16;
        // Extra room on the left — right-aligned text starts further right.
        let area = Rect::new(0, 0, width + 10, 1);
        let text_start = area.x + area.width - width;
        let inside_col = text_start + range.start as u16;
        assert!(coffee_hit_at(area, inside_col, 0, true));
        // The same column that hit when area matched `width` exactly now
        // falls in the extra left-hand gap, so it should miss.
        assert!(!coffee_hit_at(area, range.start as u16, 0, true));
    }

    #[test]
    fn coffee_hit_at_always_misses_when_the_link_is_disabled() {
        let area = Rect::new(0, 0, 40, 1);
        for col in area.x..area.x + area.width {
            assert!(!coffee_hit_at(area, col, 0, false));
        }
    }

    #[test]
    fn word_count_splits_on_any_whitespace() {
        assert_eq!(word_count("one two  three\nfour"), 4);
        assert_eq!(word_count(""), 0);
        assert_eq!(word_count("   "), 0);
    }

    #[test]
    fn reading_time_rounds_up_at_200_words_per_minute() {
        assert_eq!(reading_time_minutes(0), 0);
        assert_eq!(reading_time_minutes(1), 1);
        assert_eq!(reading_time_minutes(200), 1);
        assert_eq!(reading_time_minutes(201), 2);
        assert_eq!(reading_time_minutes(500), 3);
    }
}

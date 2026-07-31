//! Multi-cursor editing (`config.editor.multi_cursor`) — Alt+Click adds a
//! cursor, Ctrl+D adds the next occurrence of the current word/selection.
//!
//! `ratatui-textarea` has no native multi-cursor concept at all: a `TextArea`
//! owns exactly one `cursor: (usize, usize)` and one optional selection
//! anchor. Rather than reimplementing insert/delete/newline-splitting
//! ourselves, this module drives the *same* single-cursor `TextArea`
//! through every extra cursor position in turn — for one keystroke, jump
//! its real cursor to a stored position, replay the key (reusing every bit
//! of `ratatui-textarea`'s own editing logic verbatim), read the result back,
//! and move on to the next position — then restores the shared cursor to
//! wherever the primary ended up.
//!
//! Processing order is the one genuinely tricky part, and the first version
//! of this module got it backwards: processing **bottom-to-top** avoids
//! invalidating cursors *not yet processed* (an edit never affects rows
//! above it), but it does the opposite for cursors *already* processed —
//! once a lower cursor's result is recorded, a later (higher, i.e. "more
//! top") edit that inserts/removes rows shifts everything below it,
//! silently invalidating that already-recorded result. Verified by a
//! failing test before this comment existed: two cursors each pressing
//! Enter landed one row off from where they should have.
//!
//! The correct algorithm processes **top-to-bottom, left-to-right**
//! (ascending `(row, col)`) while tracking a running delta: after each
//! cursor's turn, `lines().len()` before vs. after tells us exactly how
//! many rows that edit inserted or removed (no need to special-case which
//! keys change row count), and the resulting column position vs. the
//! column we moved to tells us the column shift for any cursor still
//! waiting on the *same original row*. Every subsequent cursor's stored
//! position is adjusted by the accumulated delta before it's used, so it's
//! always correct by the time its own turn comes — the same idea real
//! multi-cursor editors use, just derived from `input`'s own before/after
//! state instead of reimplementing insert/delete arithmetic by hand.

use ratatui_textarea::{CursorMove, TextArea};

/// One cursor's position plus its own optional selection anchor —
/// `anchor.is_some()` means this cursor currently has an active selection
/// from `anchor` to `pos`, independent of every other cursor's selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CursorState {
    pub(crate) pos: (usize, usize),
    pub(crate) anchor: Option<(usize, usize)>,
}

/// Replays one keystroke at the primary cursor (`textarea`'s own live
/// cursor/selection) and every secondary cursor, top-to-bottom (see the
/// module doc comment), then restores `textarea`'s live cursor back to the
/// primary's own final position — `secondary` is updated in place, in its
/// original order, with each cursor's new position/selection. Returns how
/// many of the cursors (primary included) actually mutated the buffer,
/// i.e. how many real undo-history entries this one keystroke pushed —
/// `ratatui_textarea::TextArea::input`'s own return value already means
/// exactly this ("did this input modify the buffer"), so it's read
/// straight off each per-cursor `input` call rather than inferred.
pub(crate) fn replay_keystroke(
    textarea: &mut TextArea,
    key: crossterm::event::KeyEvent,
    secondary: &mut Vec<CursorState>,
) -> usize {
    let primary = CursorState {
        pos: crate::editor::cursor_tuple(textarea),
        anchor: textarea.selection_range().map(|(start, _)| start),
    };
    // `None` tags the primary; `Some(i)` tags `secondary[i]` — original
    // (pre-edit) positions, kept alongside the tag so results can be
    // written back in the *original* Vec order even though processing
    // order (ascending position) is generally different from it.
    let mut order: Vec<(Option<usize>, (usize, usize))> = secondary
        .iter()
        .enumerate()
        .map(|(i, c)| (Some(i), c.pos))
        .collect();
    order.push((None, primary.pos));
    order.sort_by_key(|&(_, pos)| pos);

    let mut edits = 0usize;
    let mut new_primary = primary;
    let mut new_secondary = secondary.clone();
    // Running shift applied to every not-yet-processed cursor's *stored*
    // position: `row_delta` from any earlier row-count-changing edit,
    // `col_delta` from an earlier edit on that exact same original row
    // (reset the instant an edit changes row count, since column position
    // on a since-split/merged row no longer means what it used to).
    let mut row_delta: isize = 0;
    let mut col_delta_row: Option<usize> = None;
    let mut col_delta: isize = 0;

    for (tag, orig_pos) in order {
        let cursor = match tag {
            Some(i) => secondary[i],
            None => primary,
        };
        let adjusted = adjust(cursor, orig_pos.0, row_delta, col_delta_row, col_delta);

        goto(textarea, adjusted);
        let lines_before = textarea.lines().len();
        let pre_col = textarea.cursor().1;
        let snapshot = textarea.lines().to_vec();
        if textarea.input(key) {
            edits += undo_history_depth(textarea, &snapshot);
        }
        let lines_after = textarea.lines().len();
        let (post_row, post_col) = crate::editor::cursor_tuple(textarea);

        let this_row_delta = lines_after as isize - lines_before as isize;
        if this_row_delta != 0 {
            row_delta += this_row_delta;
            col_delta_row = None;
            col_delta = 0;
        } else {
            col_delta_row = Some(orig_pos.0);
            col_delta += post_col as isize - pre_col as isize;
        }

        let updated = CursorState {
            pos: (post_row, post_col),
            anchor: textarea.selection_range().map(|(start, _)| start),
        };
        match tag {
            Some(i) => new_secondary[i] = updated,
            None => new_primary = updated,
        }
    }
    goto(textarea, new_primary);
    *secondary = new_secondary;
    edits
}

/// How many real `textarea.undo()` calls it takes to fully reverse the
/// edit that was *just* made — measured directly against `before` (the
/// buffer's content immediately prior to that edit) rather than assumed,
/// then immediately redone that many times to restore the just-made edit
/// before returning, so the measurement is otherwise invisible to the
/// caller.
///
/// This exists because a single `TextArea::input` call can silently push
/// *two* separate history entries, not always one — verified directly
/// against the vendored crate: typing a character while a selection is
/// active is "delete the selection, then insert the character" (2
/// entries; one `undo()` only un-inserts the character, leaving the
/// deleted selection still missing), while Backspace over that same kind
/// of selection is just "delete the selection" (1 entry, nothing to
/// insert). Guessing a fixed count per key/selection combination would
/// mean re-deriving `ratatui-textarea`'s own internal history-grouping rules
/// by hand for every key this module might ever replay; measuring it
/// directly is correct regardless of which key or selection shape was
/// involved. Capped at 10 rounds as a safety valve against looping forever
/// if `undo()` ever legitimately can't reach `before` (not expected in
/// practice, but a silent infinite loop would be far worse than an
/// occasionally-undercounted group).
pub(crate) fn undo_history_depth(textarea: &mut TextArea, before: &[String]) -> usize {
    let mut n = 0;
    while textarea.lines() != before && n < 10 {
        if !textarea.undo() {
            break;
        }
        n += 1;
    }
    for _ in 0..n {
        textarea.redo();
    }
    n
}

/// Applies the running row/column delta (see `replay_keystroke`) to one
/// cursor's stored position and, if present, its selection anchor — both
/// are positions in the same original-document coordinate space, so both
/// need the same adjustment.
fn adjust(
    cursor: CursorState,
    orig_row: usize,
    row_delta: isize,
    col_delta_row: Option<usize>,
    col_delta: isize,
) -> CursorState {
    let shift = |(row, col): (usize, usize)| -> (usize, usize) {
        let new_row = (row as isize + row_delta).max(0) as usize;
        let new_col = if col_delta_row == Some(orig_row) {
            (col as isize + col_delta).max(0) as usize
        } else {
            col
        };
        (new_row, new_col)
    };
    CursorState {
        pos: shift(cursor.pos),
        anchor: cursor.anchor.map(shift),
    }
}

/// Moves `textarea`'s live cursor to `cursor.pos`, restoring its selection
/// anchor first when it has one — shared by `replay_keystroke`'s per-cursor
/// loop and its final restore-the-primary step, so the two can't disagree
/// on how a `CursorState` becomes the textarea's actual live state.
fn goto(textarea: &mut TextArea, cursor: CursorState) {
    textarea.cancel_selection();
    if let Some((ar, ac)) = cursor.anchor {
        textarea.move_cursor(CursorMove::Jump(ar as u16, ac as u16));
        textarea.start_selection();
    }
    textarea.move_cursor(CursorMove::Jump(cursor.pos.0 as u16, cursor.pos.1 as u16));
}

/// Alt+Click: adds a plain (no selection) cursor at `pos`, deduplicating
/// against the primary's own position and every existing secondary.
pub(crate) fn add_cursor_at(
    secondary: &mut Vec<CursorState>,
    primary_pos: (usize, usize),
    pos: (usize, usize),
) {
    if pos == primary_pos || secondary.iter().any(|c| c.pos == pos) {
        return;
    }
    secondary.push(CursorState { pos, anchor: None });
}

/// Ctrl+D's "add the next occurrence": pushes a new selecting cursor at
/// `(row, start)..(row, end)` unless an identical one already exists (which
/// only happens once every occurrence in the buffer already has a cursor —
/// the caller reports that as "no more occurrences" rather than looping).
/// Returns whether a new cursor was actually added.
pub(crate) fn add_occurrence(
    secondary: &mut Vec<CursorState>,
    row: usize,
    start: usize,
    end: usize,
) -> bool {
    let cursor = CursorState {
        pos: (row, end),
        anchor: Some((row, start)),
    };
    if secondary.contains(&cursor) {
        return false;
    }
    secondary.push(cursor);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn textarea(lines: &[&str]) -> TextArea<'static> {
        TextArea::new(lines.iter().map(|l| l.to_string()).collect())
    }

    #[test]
    fn replays_a_plain_character_at_every_cursor() {
        let mut ta = textarea(&["cat", "cat", "cat"]);
        ta.move_cursor(CursorMove::Jump(0, 0));
        let mut secondary = vec![
            CursorState {
                pos: (1, 0),
                anchor: None,
            },
            CursorState {
                pos: (2, 0),
                anchor: None,
            },
        ];
        let edits = replay_keystroke(&mut ta, key('X'), &mut secondary);
        assert_eq!(edits, 3);
        assert_eq!(ta.lines(), &["Xcat", "Xcat", "Xcat"]);
        // Every cursor landed right after its own inserted 'X'.
        assert_eq!(ta.cursor(), (0, 1));
        assert_eq!(secondary[0].pos, (1, 1));
        assert_eq!(secondary[1].pos, (2, 1));
    }

    #[test]
    fn typing_over_a_selection_reports_two_edits_per_cursor_and_undoes_atomically() {
        // Real bug caught live: `TextArea::input` pushes *two* history
        // entries when it replaces an active selection (delete, then
        // insert), not one — a naive `edits += 1` per cursor undercounts,
        // and a single Ctrl+U then only half-reverses each cursor,
        // leaving text neither "cat" nor "dog". Every cursor here has a
        // full-word selection (Ctrl+D-style), so typing 'd' must report
        // 2 edits per cursor (6 total) and a matching number of `undo()`
        // calls must fully restore "cat" on every line.
        let mut ta = textarea(&["cat", "cat", "cat"]);
        ta.move_cursor(CursorMove::Jump(0, 0));
        ta.start_selection();
        ta.move_cursor(CursorMove::Jump(0, 3));
        let mut secondary = vec![
            CursorState {
                pos: (1, 3),
                anchor: Some((1, 0)),
            },
            CursorState {
                pos: (2, 3),
                anchor: Some((2, 0)),
            },
        ];
        let edits = replay_keystroke(&mut ta, key('d'), &mut secondary);
        assert_eq!(edits, 6, "2 history entries per cursor (delete + insert)");
        assert_eq!(ta.lines(), &["d", "d", "d"]);

        for _ in 0..edits {
            ta.undo();
        }
        assert_eq!(
            ta.lines(),
            &["cat", "cat", "cat"],
            "exactly `edits` undo() calls must fully restore the original text"
        );
    }

    #[test]
    fn typing_a_whole_word_across_selected_cursors_undoes_fully_across_keystrokes() {
        // Same scenario as the app itself: Ctrl+D-select "cat" on 3 lines,
        // then type "dog" as three *separate* keystrokes (three separate
        // `replay_keystroke` calls, exactly as `handle_edit_key` makes one
        // per `KeyEvent`) — summing every call's own returned edit count
        // and calling `undo()` that many times total must fully restore
        // "cat" on every line, not get stuck partway.
        let mut ta = textarea(&["cat", "cat", "cat"]);
        ta.move_cursor(CursorMove::Jump(0, 0));
        ta.start_selection();
        ta.move_cursor(CursorMove::Jump(0, 3));
        let mut secondary = vec![
            CursorState {
                pos: (1, 3),
                anchor: Some((1, 0)),
            },
            CursorState {
                pos: (2, 3),
                anchor: Some((2, 0)),
            },
        ];
        let mut total_edits = 0usize;
        for c in ['d', 'o', 'g'] {
            total_edits += replay_keystroke(&mut ta, key(c), &mut secondary);
        }
        assert_eq!(ta.lines(), &["dog", "dog", "dog"]);

        for _ in 0..total_edits {
            ta.undo();
        }
        assert_eq!(
            ta.lines(),
            &["cat", "cat", "cat"],
            "summing every keystroke's own edit count must undo the whole word"
        );
    }

    #[test]
    fn enter_shifts_later_rows_without_desyncing_other_cursors() {
        // Primary splits row 0 ("abc" -> "a"/"bc"); the secondary on row 1
        // gets its *own* Enter too (multi-cursor replays the same key at
        // every cursor), splitting "def" -> "d"/"ef" — but it must land on
        // whatever row that split ends up at post-primary-shift (row 3),
        // not the stale pre-shift row 2.
        let mut ta = textarea(&["abc", "def"]);
        ta.move_cursor(CursorMove::Jump(0, 1)); // between 'a' and 'bc'
        let mut secondary = vec![CursorState {
            pos: (1, 1),
            anchor: None,
        }];
        let key_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let edits = replay_keystroke(&mut ta, key_enter, &mut secondary);
        assert_eq!(edits, 2);
        assert_eq!(ta.lines(), &["a", "bc", "d", "ef"]);
        assert_eq!(ta.cursor(), (1, 0));
        assert_eq!(secondary[0].pos, (3, 0));
    }

    #[test]
    fn two_cursors_on_the_same_row_accumulate_column_shift() {
        // Typing 'X' at col 0 pushes everything after it one column to the
        // right — a second cursor on the *same* row at (originally) col 3
        // must land at col 4 to still be typing in the same place it was
        // pointing at before either edit happened.
        let mut ta = textarea(&["abcdef"]);
        ta.move_cursor(CursorMove::Jump(0, 0));
        let mut secondary = vec![CursorState {
            pos: (0, 3),
            anchor: None,
        }];
        let edits = replay_keystroke(&mut ta, key('X'), &mut secondary);
        assert_eq!(edits, 2);
        assert_eq!(ta.lines(), &["XabcXdef"]);
        assert_eq!(ta.cursor(), (0, 1));
        assert_eq!(secondary[0].pos, (0, 5));
    }

    #[test]
    fn navigation_keys_report_zero_edits() {
        let mut ta = textarea(&["hello"]);
        let mut secondary = vec![CursorState {
            pos: (0, 0),
            anchor: None,
        }];
        let right = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        let edits = replay_keystroke(&mut ta, right, &mut secondary);
        assert_eq!(edits, 0);
    }

    #[test]
    fn add_cursor_at_dedupes_against_primary_and_existing() {
        let mut secondary = Vec::new();
        add_cursor_at(&mut secondary, (0, 0), (1, 1));
        add_cursor_at(&mut secondary, (0, 0), (1, 1)); // duplicate
        add_cursor_at(&mut secondary, (0, 0), (0, 0)); // same as primary
        assert_eq!(secondary.len(), 1);
    }

    #[test]
    fn add_occurrence_reports_false_once_already_present() {
        let mut secondary = Vec::new();
        assert!(add_occurrence(&mut secondary, 0, 2, 5));
        assert!(!add_occurrence(&mut secondary, 0, 2, 5));
        assert_eq!(secondary.len(), 1);
    }
}

//! The global tasks modal (leader+`t`): every checkbox task across every
//! notebook in one flat list, each row carrying its own location — an
//! earlier version grouped tasks under per-note header rows instead
//! (`links_panel::LinkRow`-style), but a header scrolls out of view while
//! its tasks are still on screen, so the location lives *on the row*,
//! muted, where it can't be lost. `Enter`/`space` toggles the task in its
//! source file (`shiki_core::tasks::toggle`); `l`/`o` jumps to its note.

use std::path::PathBuf;

use chrono::NaiveDate;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState};
use ratatui::Frame;

use shiki_core::tasks::Task;
use shiki_core::{Note, Notebook};

use crate::app::{centered_rect, App};
use crate::icons;
use crate::render::{hex_to_color, panel_block};

/// One task in the flattened list — every row is selectable (no headers).
#[derive(Debug, Clone)]
pub struct TaskRow {
    /// Which notebook the note belongs to — `note_changed` needs the name
    /// after a toggle, so it's carried here rather than re-derived.
    pub notebook: String,
    pub note_path: PathBuf,
    /// `"{notebook}/{folders…}/{note title}"` — rendered muted next to the
    /// task text so every row says where it lives without a header.
    pub location: String,
    pub task: Task,
}

/// Builds the rows from the same `(Notebook, Note)` pool global search
/// walks (`NotebookStore::all_notes`). Sorted by urgency: dated tasks
/// first in due order (overdue at the very top), undated ones after, in
/// walk order — the sort is stable, so tasks from the same note keep their
/// file order within each bucket. `include_done` off (the default) hides
/// already-checked tasks; a task toggled *while the modal is open* is
/// updated in place rather than rebuilt, so it stays visible either way.
pub fn build(pool: &[(Notebook, Note)], include_done: bool) -> Vec<TaskRow> {
    let mut rows: Vec<TaskRow> = pool
        .iter()
        .flat_map(|(nb, note)| {
            let location = shiki_core::tasks::location_of(nb, note);
            shiki_core::tasks::extract(&note.body)
                .into_iter()
                .filter(|t| include_done || !t.done)
                .map(move |task| TaskRow {
                    notebook: nb.name.clone(),
                    note_path: note.path.clone(),
                    location: location.clone(),
                    task,
                })
        })
        .collect();
    // `Option`'s derived ordering puts `None` first — flip it so undated
    // tasks sink below dated ones instead of floating above the overdue.
    rows.sort_by_key(|r| (r.task.due.is_none(), r.task.due));
    rows
}

/// How many of the visible tasks are still pending — shown in the title so
/// the modal doubles as a "what's left" count without scrolling.
pub fn pending_count(rows: &[TaskRow]) -> usize {
    rows.iter().filter(|r| !r.task.done).count()
}

pub fn render(frame: &mut Frame, frame_area: Rect, app: &App) {
    let height = (frame_area.height * 3 / 4).max(8);
    let popup_area = centered_rect(frame_area, (frame_area.width * 3 / 4).max(50), height);
    frame.render_widget(Clear, popup_area);

    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);
    let success = hex_to_color(&app.theme.success);
    let warning = hex_to_color(&app.theme.warning);
    let error = hex_to_color(&app.theme.error);
    let today = chrono::Local::now().date_naive();

    let items: Vec<ListItem> = app
        .task_rows
        .iter()
        .map(|row| {
            let task = &row.task;
            let (marker, marker_style) = if task.done {
                (
                    format!(" [{}] ", icons::CHECK.bare()),
                    Style::default().fg(success),
                )
            } else {
                (" [ ] ".to_string(), Style::default().fg(fg))
            };
            let text_style = if task.done {
                Style::default()
                    .fg(muted)
                    .add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default().fg(fg)
            };
            let mut spans = vec![
                Span::styled(marker, marker_style),
                Span::styled(task.text.clone(), text_style),
            ];
            if let Some(due) = task.due {
                spans.push(Span::styled(
                    format!("  {}{due}", icons::CALENDAR),
                    Style::default().fg(due_color(due, today, task.done, muted, warning, error)),
                ));
            }
            spans.push(Span::styled(
                format!("  {}{}", icons::NOTE, row.location),
                Style::default().fg(muted),
            ));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = format!(
        " {}Tasks [{} pending]  \u{2014}  enter/space toggle \u{B7} l jump \u{B7} a show done \u{B7} esc close ",
        icons::CHECK,
        pending_count(&app.task_rows),
    );
    let highlight_symbol = format!("{}", icons::ARROW);
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
    state.select(Some(app.task_selected));
    frame.render_stateful_widget(list, popup_area, &mut state);
}

/// Overdue reads as an error, due today as a warning, anything else (or an
/// already-done task's date) stays muted — done tasks' dates aren't
/// actionable no matter how far past they are.
fn due_color(
    due: NaiveDate,
    today: NaiveDate,
    done: bool,
    muted: ratatui::style::Color,
    warning: ratatui::style::Color,
    error: ratatui::style::Color,
) -> ratatui::style::Color {
    if done {
        muted
    } else if due < today {
        error
    } else if due == today {
        warning
    } else {
        muted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiki_core::note::Frontmatter;

    fn pool_entry(nb: &str, rel_path: &str, title: &str, body: &str) -> (Notebook, Note) {
        (
            Notebook::new(nb, PathBuf::from(format!("/tmp/{nb}"))),
            Note::new(
                PathBuf::from(format!("/tmp/{nb}/{rel_path}")),
                Frontmatter::new(title, nb),
                body.to_string(),
            ),
        )
    }

    #[test]
    fn build_flattens_tasks_with_their_location() {
        let pool = vec![
            pool_entry(
                "work",
                "sprint.md",
                "Sprint",
                "- [ ] ship it\n- [x] plan it",
            ),
            pool_entry("home", "chores.md", "Chores", "- [ ] mow lawn"),
            pool_entry("home", "empty.md", "Empty", "no tasks here"),
        ];

        let rows = build(&pool, false);

        // Done tasks hidden by default; the taskless note contributes nothing.
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].task.text, "ship it");
        assert_eq!(rows[0].location, "work/Sprint");
        assert_eq!(rows[1].location, "home/Chores");
        assert_eq!(pending_count(&rows), 2);
    }

    #[test]
    fn build_includes_done_tasks_when_asked() {
        let pool = vec![pool_entry(
            "work",
            "sprint.md",
            "Sprint",
            "- [ ] ship it\n- [x] plan it",
        )];
        let rows = build(&pool, true);
        assert_eq!(rows.len(), 2);
        assert_eq!(pending_count(&rows), 1);
    }

    #[test]
    fn build_sorts_dated_tasks_first_in_due_order() {
        let pool = vec![pool_entry(
            "work",
            "plan.md",
            "Plan",
            "- [ ] no date\n- [ ] later @due(2026-12-01)\n- [ ] soon @due(2026-08-05)",
        )];
        let rows = build(&pool, false);
        assert_eq!(rows[0].task.text, "soon @due(2026-08-05)");
        assert_eq!(rows[1].task.text, "later @due(2026-12-01)");
        assert_eq!(rows[2].task.text, "no date");
    }

    #[test]
    fn location_includes_nested_folders_and_the_human_title() {
        let pool = vec![pool_entry(
            "work",
            "projects/q3/roadmap-v2.md",
            "Roadmap v2",
            "- [ ] draft",
        )];
        let rows = build(&pool, false);
        assert_eq!(rows[0].location, "work/projects/q3/Roadmap v2");
    }
}

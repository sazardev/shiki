//! The git dashboard (`Action::ShowGitDash`, `G` in NOTEBOOKS focus) —
//! every notebook's sync state in plain language, one line each, plus its
//! latest commits underneath. The point is translation, not new git
//! machinery: the same `GitStatus` the drawer badges show (`+2 ↑1 ↓3`)
//! phrased so a person who has never opened a terminal git prompt knows
//! both what's going on and which key fixes it ("2 uncommitted · 3 on
//! remote — s saves, p pulls"). Read-only: nothing here mutates state.
//!
//! Same "flat rows, some of them non-selectable" shape as `links_panel`
//! (`LinkRow`) and `tree::TreeRow`, so selection skips commit rows with
//! the exact same index-mapping pattern.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, ListItem, ListState};
use ratatui::Frame;

use crate::app::{centered_rect, App};
use crate::icons;
use crate::render::{hex_to_color, panel_block};
use shiki_core::git::{FileRevision, GitStatus};

/// Everything the dashboard needs to know about one notebook — gathered
/// in one pass by `open_git_dash` (all local-disk reads, no network) so
/// `build_rows` stays a pure function over plain data.
#[derive(Debug, Clone)]
pub struct NotebookGitState {
    pub name: String,
    pub status: GitStatus,
    /// A pull came back conflicted and hasn't been resolved yet — the one
    /// state where "sync again" is the *wrong* next move.
    pub merging: bool,
    /// Up to a few most recent commits (newest first), shown under the
    /// notebook's status line.
    pub commits: Vec<FileRevision>,
}

/// How worried the reader should be about a notebook's status line —
/// drives the color only; the words carry the meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Nothing to do: committed and pushed.
    Ok,
    /// Something pending, with a key that fixes it.
    Attention,
    /// Needs a human decision (conflicts, unreadable repo).
    Problem,
}

/// One row in the dashboard list: a notebook's status line (the only
/// selectable kind) or one of its recent commits (display only).
#[derive(Debug, Clone)]
pub enum DashRow {
    Notebook {
        name: String,
        line: String,
        tone: Tone,
    },
    Commit {
        short: String,
        date: String,
        message: String,
    },
}

/// The human sentence for one notebook's `GitStatus` — what's pending and
/// which key fixes it, or "all synced". Pure so tests can cover every
/// branch without an `App`.
pub fn status_line(status: &GitStatus, merging: bool) -> (String, Tone) {
    if !status.is_repo {
        return ("not a git repository yet".to_string(), Tone::Attention);
    }
    if merging {
        return (
            "merge in progress \u{2014} resolve conflicts first".to_string(),
            Tone::Problem,
        );
    }
    if let Some(error) = &status.status_error {
        return (format!("status unavailable ({error})"), Tone::Problem);
    }

    let mut parts = Vec::new();
    let mut hints = Vec::new();
    if status.dirty_count > 0 {
        parts.push(format!("{} uncommitted", status.dirty_count));
        hints.push("s saves");
    }
    if status.behind > 0 {
        parts.push(format!("{} on remote", status.behind));
        hints.push("p pulls");
    }
    if status.ahead > 0 {
        parts.push(format!("{} unpushed", status.ahead));
        hints.push("u pushes");
    }
    if parts.is_empty() {
        return ("all synced".to_string(), Tone::Ok);
    }
    (
        format!("{} \u{2014} {}", parts.join(" \u{B7} "), hints.join(", ")),
        Tone::Attention,
    )
}

/// Flattens every notebook into rows: the status line, then up to three
/// recent commits under it. A notebook row is always present even for an
/// empty brand-new notebook — "no commits yet" is information too.
pub fn build_rows(states: &[NotebookGitState]) -> Vec<DashRow> {
    let mut rows = Vec::new();
    for state in states {
        let (line, tone) = status_line(&state.status, state.merging);
        rows.push(DashRow::Notebook {
            name: state.name.clone(),
            line,
            tone,
        });
        for rev in state.commits.iter().take(3) {
            rows.push(DashRow::Commit {
                short: rev.commit_id.chars().take(7).collect(),
                date: rev.date.format("%m-%d %H:%M").to_string(),
                message: rev.message.clone(),
            });
        }
    }
    rows
}

/// How many rows are selectable (notebook lines) — the bound for a
/// `selected` index into the list.
pub fn selectable_count(rows: &[DashRow]) -> usize {
    rows.iter()
        .filter(|r| matches!(r, DashRow::Notebook { .. }))
        .count()
}

/// The row index (into `rows`, commits included) of the `selected`-th
/// notebook row — what `ListState::select` needs to highlight the right
/// visual row.
pub fn selected_row(rows: &[DashRow], selected: usize) -> Option<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, DashRow::Notebook { .. }))
        .nth(selected)
        .map(|(i, _)| i)
}

pub fn render(frame: &mut Frame, frame_area: ratatui::layout::Rect, app: &App) {
    let height = (frame_area.height * 3 / 4).max(8);
    let popup_area = centered_rect(frame_area, (frame_area.width * 3 / 4).max(60), height);
    frame.render_widget(Clear, popup_area);

    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);
    let success = hex_to_color(&app.theme.success);
    let warning = hex_to_color(&app.theme.warning);
    let error = hex_to_color(&app.theme.error);

    let items: Vec<ListItem> = app
        .git_dash_rows
        .iter()
        .map(|row| match row {
            DashRow::Notebook { name, line, tone } => {
                let tone_color = match tone {
                    Tone::Ok => success,
                    Tone::Attention => warning,
                    Tone::Problem => error,
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        name.clone(),
                        Style::default().fg(fg).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  ", Style::default().fg(muted)),
                    Span::styled(line.clone(), Style::default().fg(tone_color)),
                ]))
            }
            DashRow::Commit {
                short,
                date,
                message,
            } => ListItem::new(Line::from(Span::styled(
                format!("   {short} {date}  {message}"),
                Style::default().fg(muted),
            ))),
        })
        .collect();

    let highlight_symbol = format!("{}", icons::ARROW);
    let title = format!(
        " {}Git  [{}]  \u{2014}  j/k navigate \u{B7} esc/q close ",
        icons::GIT,
        selectable_count(&app.git_dash_rows)
    );
    let list = ratatui::widgets::List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    if selectable_count(&app.git_dash_rows) > 0 {
        state.select(selected_row(&app.git_dash_rows, app.git_dash_selected));
    }
    frame.render_stateful_widget(list, popup_area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(dirty: usize, ahead: usize, behind: usize) -> GitStatus {
        GitStatus {
            is_repo: true,
            dirty: dirty > 0,
            dirty_count: dirty,
            branch: Some("main".into()),
            ahead,
            behind,
            status_error: None,
        }
    }

    #[test]
    fn clean_and_synced_reads_as_all_synced() {
        let (line, tone) = status_line(&status(0, 0, 0), false);
        assert_eq!(line, "all synced");
        assert_eq!(tone, Tone::Ok);
    }

    #[test]
    fn dirty_names_the_count_and_the_fixing_key() {
        let (line, tone) = status_line(&status(3, 0, 0), false);
        assert_eq!(line, "3 uncommitted \u{2014} s saves");
        assert_eq!(tone, Tone::Attention);
    }

    #[test]
    fn combined_states_join_parts_and_hints() {
        let (line, tone) = status_line(&status(2, 1, 4), false);
        assert_eq!(
            line,
            "2 uncommitted \u{B7} 4 on remote \u{B7} 1 unpushed \u{2014} s saves, p pulls, u pushes"
        );
        assert_eq!(tone, Tone::Attention);
    }

    #[test]
    fn merge_in_progress_beats_every_pending_change() {
        let (line, tone) = status_line(&status(5, 2, 2), true);
        assert_eq!(line, "merge in progress \u{2014} resolve conflicts first");
        assert_eq!(tone, Tone::Problem);
    }

    #[test]
    fn non_repos_and_errors_are_reported_without_keys() {
        let mut broken = status(0, 0, 0);
        broken.is_repo = false;
        let (line, _tone) = status_line(&broken, false);
        assert_eq!(line, "not a git repository yet");

        let mut errored = status(0, 0, 0);
        errored.status_error = Some("boom".into());
        let (line, tone) = status_line(&errored, false);
        assert_eq!(line, "status unavailable (boom)");
        assert_eq!(tone, Tone::Problem);
    }

    #[test]
    fn build_rows_interleaves_commits_under_each_notebook() {
        let states = vec![
            NotebookGitState {
                name: "personal".into(),
                status: status(0, 0, 0),
                merging: false,
                commits: vec![],
            },
            NotebookGitState {
                name: "work".into(),
                status: status(1, 0, 0),
                merging: false,
                commits: vec![
                    FileRevision {
                        commit_id: "abcdef1234567890".into(),
                        date: chrono::Local::now(),
                        message: "shiki: added (Ideas.md)".into(),
                    },
                    FileRevision {
                        commit_id: "1234567890abcdef".into(),
                        date: chrono::Local::now(),
                        message: "shiki: updated (Roadmap.md)".into(),
                    },
                ],
            },
        ];

        let rows = build_rows(&states);
        assert!(
            matches!(&rows[0], DashRow::Notebook { name, line, tone: Tone::Ok }
            if name == "personal" && line == "all synced")
        );
        assert!(matches!(&rows[1], DashRow::Notebook { .. }));
        assert!(matches!(&rows[2], DashRow::Commit { short, message, .. }
            if short == "abcdef1" && message == "shiki: added (Ideas.md)"));
        assert!(matches!(&rows[3], DashRow::Commit { short, .. } if short == "1234567"));
        // Selection counts notebooks only, and maps past their commits.
        assert_eq!(selectable_count(&rows), 2);
        assert_eq!(selected_row(&rows, 0), Some(0));
        assert_eq!(selected_row(&rows, 1), Some(1));
        assert_eq!(selected_row(&rows, 2), None);
    }

    #[test]
    fn build_rows_caps_commits_at_three() {
        let commits: Vec<FileRevision> = (0..7)
            .map(|i| FileRevision {
                commit_id: format!("{i:040}"),
                date: chrono::Local::now(),
                message: format!("commit {i}"),
            })
            .collect();
        let states = vec![NotebookGitState {
            name: "big".into(),
            status: status(0, 0, 0),
            merging: false,
            commits,
        }];
        let rows = build_rows(&states);
        assert_eq!(rows.len(), 4, "1 notebook line + at most 3 commits");
    }
}

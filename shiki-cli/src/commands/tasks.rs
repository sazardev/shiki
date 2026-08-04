//! `shiki tasks` — the TUI's global tasks view (leader+`t`), scriptable.
//! Same source of truth (`NotebookStore::all_notes` + `tasks::extract`),
//! same urgency sort; `--json` and `--count` exist specifically for status
//! bars (waybar/polybar/tmux) — e.g. `shiki tasks --overdue --count` as a
//! "2 overdue" module — which is the whole point of exposing this outside
//! the TUI.

use std::io::IsTerminal;

use anyhow::Result;
use chrono::NaiveDate;
use shiki_core::tasks::Task;
use shiki_core::NotebookStore;

pub struct Filters {
    /// Only tasks due strictly before today (implies pending).
    pub overdue: bool,
    /// Only tasks due exactly today (implies pending). Combinable with
    /// `overdue` — together they mean "due today or earlier".
    pub today: bool,
    /// Include already-done tasks (off by default — pending only).
    pub all: bool,
}

struct Row {
    task: Task,
    location: String,
    notebook: String,
    note_title: String,
    path: std::path::PathBuf,
}

pub fn run(
    store: &NotebookStore,
    notebook: Option<&str>,
    filters: &Filters,
    json: bool,
    count: bool,
) -> Result<()> {
    let today = chrono::Local::now().date_naive();
    let pool = store.all_notes()?;
    let mut rows: Vec<Row> = pool
        .iter()
        .filter(|(nb, _)| notebook.is_none_or(|wanted| nb.name == wanted))
        .flat_map(|(nb, note)| {
            let location = shiki_core::tasks::location_of(nb, note);
            shiki_core::tasks::extract(&note.body)
                .into_iter()
                .map(move |task| Row {
                    task,
                    location: location.clone(),
                    notebook: nb.name.clone(),
                    note_title: note.frontmatter.title.clone(),
                    path: note.path.clone(),
                })
        })
        .filter(|r| keep(r, filters, today))
        .collect();
    rows.sort_by_key(|r| (r.task.due.is_none(), r.task.due));

    if count {
        println!("{}", rows.len());
        return Ok(());
    }
    if json {
        let items: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "text": r.task.text,
                    "done": r.task.done,
                    "due": r.task.due.map(|d| d.to_string()),
                    "overdue": is_overdue(&r.task, today),
                    "notebook": r.notebook,
                    "note": r.note_title,
                    "location": r.location,
                    "path": r.path,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("(no matching tasks)");
        return Ok(());
    }
    let color = std::io::stdout().is_terminal();
    for r in &rows {
        println!("{}", format_row(r, today, color));
    }
    Ok(())
}

fn keep(row: &Row, filters: &Filters, today: NaiveDate) -> bool {
    if !filters.all && row.task.done {
        return false;
    }
    match (filters.overdue, filters.today) {
        (false, false) => true,
        (o, t) => row
            .task
            .due
            .is_some_and(|d| (o && d < today) || (t && d == today)),
    }
}

fn is_overdue(task: &Task, today: NaiveDate) -> bool {
    !task.done && task.due.is_some_and(|d| d < today)
}

fn format_row(row: &Row, today: NaiveDate, color: bool) -> String {
    const RED: &str = "\x1b[31m";
    const YELLOW: &str = "\x1b[33m";
    const DIM: &str = "\x1b[2m";
    const RESET: &str = "\x1b[0m";
    let marker = if row.task.done { "[x]" } else { "[ ]" };
    let due = match row.task.due {
        Some(d) if color => {
            let paint = if row.task.done {
                DIM
            } else if d < today {
                RED
            } else if d == today {
                YELLOW
            } else {
                DIM
            };
            format!("  {paint}{d}{RESET}")
        }
        Some(d) => format!("  {d}"),
        None => String::new(),
    };
    let location = if color {
        format!("  {DIM}{}{RESET}", row.location)
    } else {
        format!("  {}", row.location)
    };
    format!("{marker} {}{due}{location}", row.task.text)
}

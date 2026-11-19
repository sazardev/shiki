//! MCP prompts: server-rendered starting points that already contain the
//! notes/tasks the agent needs, so a client can offer "weekly review" or
//! "inbox triage" as a one-click action instead of asking the model to
//! discover and page through everything first.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{PromptMessage, Role};
use rmcp::{prompt, prompt_router, ErrorData};
use serde::Deserialize;

use crate::helpers::{get_and_unlock, tool_err};
use crate::Shiki;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WeeklyReviewArgs {
    /// Notebook to review. Omit to review every notebook.
    pub notebook: Option<String>,
    /// How many days of daily notes to include (default 7, capped at 90).
    pub days: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InboxTriageArgs {
    /// Notebook to triage. Omit to look at every notebook.
    pub notebook: Option<String>,
}

fn preview(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let mut out: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        out.push('\u{2026}');
    }
    out
}

fn notebook_matches(
    nb: &shiki_core::Notebook,
    wanted: Option<&str>,
    hidden: &std::collections::HashSet<String>,
) -> bool {
    !hidden.contains(&nb.name) && wanted.is_none_or(|name| nb.name == name)
}

fn ensure_requested_notebook(shiki: &Shiki, wanted: Option<&str>) -> Result<(), ErrorData> {
    if let Some(name) = wanted {
        shiki_core::notebook::get_notebook(&shiki.store, name).map_err(tool_err)?;
    }
    Ok(())
}

/// Every note in every notebook, with each notebook unlocked exactly the way
/// the tools unlock it — otherwise a prompt would silently fail on an
/// encrypted notebook even when `SHIKI_PASSPHRASE` is set.
fn unlocked_pool(
    shiki: &Shiki,
) -> Result<Vec<(shiki_core::Notebook, shiki_core::Note)>, ErrorData> {
    let hidden = hidden_notebooks(shiki);
    let mut pool = Vec::new();
    for nb in shiki.store.list().map_err(tool_err)? {
        if hidden.contains(&nb.name) {
            continue;
        }
        let nb = get_and_unlock(&shiki.store, &shiki.config(), &nb.name)?;
        for note in nb.all_notes_recursive().map_err(tool_err)? {
            pool.push((nb.clone(), note));
        }
    }
    Ok(pool)
}

fn hidden_notebooks(shiki: &Shiki) -> std::collections::HashSet<String> {
    shiki
        .config()
        .notebooks
        .iter()
        .filter(|(_, over)| over.hidden)
        .map(|(name, _)| name.clone())
        .collect()
}

#[prompt_router(vis = "pub")]
impl Shiki {
    #[prompt(
        name = "weekly_review",
        description = "Review recent daily notes and open tasks, then suggest what to reschedule, finish or drop."
    )]
    pub(crate) fn weekly_review(
        &self,
        Parameters(args): Parameters<WeeklyReviewArgs>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        ensure_requested_notebook(self, args.notebook.as_deref())?;
        let days = args.days.unwrap_or(7).clamp(1, 90) as i64;
        let today = chrono::Local::now().date_naive();
        let cutoff = today - chrono::Duration::days(days);
        let hidden = hidden_notebooks(self);
        let pool = unlocked_pool(self)?;

        let mut tasks: Vec<(String, shiki_core::tasks::Task)> = pool
            .iter()
            .filter(|(nb, _)| notebook_matches(nb, args.notebook.as_deref(), &hidden))
            .flat_map(|(nb, note)| {
                let location = shiki_core::tasks::location_of(nb, note);
                shiki_core::tasks::extract(&note.body)
                    .into_iter()
                    .filter(|task| !task.done)
                    .map(move |task| (location.clone(), task))
            })
            .collect();
        tasks.sort_by_key(|(_, task)| (task.due.is_none(), task.due));
        tasks.truncate(50);

        let mut dailies: Vec<(&shiki_core::Notebook, &shiki_core::Note)> = pool
            .iter()
            .filter(|(nb, note)| {
                notebook_matches(nb, args.notebook.as_deref(), &hidden)
                    && note.frontmatter.date >= cutoff
                    && (note.frontmatter.title.ends_with("Daily")
                        || note
                            .path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.ends_with("-daily.md")))
            })
            .map(|(nb, note)| (nb, note))
            .collect();
        dailies.sort_by_key(|(_, note)| std::cmp::Reverse(note.frontmatter.date));
        dailies.truncate(7);

        let mut text = format!(
            "# Weekly review\n\nToday is {today}. Review the last {days} day(s) of shiki notes \
             and help me plan the next week.\n\n## Open tasks ({})\n\n",
            tasks.len()
        );
        if tasks.is_empty() {
            text.push_str("_No open tasks._\n");
        } else {
            for (location, task) in &tasks {
                text.push_str(&format!(
                    "- [ ] {} \u{2014} {} \u{2014} {}{}\n",
                    task.text,
                    location,
                    task.due
                        .map(|due| format!("due {due}"))
                        .unwrap_or_else(|| "no due date".to_string()),
                    if task.due.is_some_and(|due| due < today) {
                        " (overdue)"
                    } else {
                        ""
                    }
                ));
            }
        }
        text.push_str("\n## Recent daily notes\n\n");
        if dailies.is_empty() {
            text.push_str("_No daily notes in this window._\n");
        } else {
            for (nb, note) in &dailies {
                text.push_str(&format!(
                    "### {} \u{2014} {}\n\n{}\n\n",
                    note.frontmatter.date,
                    nb.name,
                    preview(&note.body, 2_000)
                ));
            }
        }
        text.push_str(
            "Summarize the week, call out overdue tasks and unanswered threads, then propose a \
             short prioritized plan. Ask before changing anything; if I approve, use the shiki \
             tools (tag_note/move_note/edit_note) and run sync_notebook afterwards.",
        );
        Ok(vec![PromptMessage::new_text(Role::User, text)])
    }

    #[prompt(
        name = "inbox_triage",
        description = "Find quick-capture notes and untagged notes, then propose tags, folders and next actions."
    )]
    pub(crate) fn inbox_triage(
        &self,
        Parameters(args): Parameters<InboxTriageArgs>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        ensure_requested_notebook(self, args.notebook.as_deref())?;
        let hidden = hidden_notebooks(self);
        let pool = unlocked_pool(self)?;
        let mut captures: Vec<(&shiki_core::Notebook, &shiki_core::Note)> = pool
            .iter()
            .filter(|(nb, note)| {
                notebook_matches(nb, args.notebook.as_deref(), &hidden)
                    && (note.frontmatter.title.starts_with("Capture ")
                        || note
                            .frontmatter
                            .tags
                            .iter()
                            .any(|tag| tag.eq_ignore_ascii_case("inbox")))
            })
            .map(|(nb, note)| (nb, note))
            .collect();
        captures.sort_by_key(|(_, note)| std::cmp::Reverse(note.frontmatter.date));
        captures.truncate(50);

        let mut text = format!(
            "# Inbox triage\n\nThere are {} quick-capture/inbox note(s) to triage.\n\n",
            captures.len()
        );
        if captures.is_empty() {
            text.push_str("_The inbox is empty._\n");
        } else {
            for (nb, note) in &captures {
                text.push_str(&format!(
                    "## {} \u{2014} {}\n\n- path: `{}`\n- tags: {}\n- date: {}\n\n{}\n\n",
                    nb.name,
                    note.frontmatter.title,
                    note.path
                        .strip_prefix(&nb.path)
                        .unwrap_or(&note.path)
                        .display(),
                    if note.frontmatter.tags.is_empty() {
                        "(none)".to_string()
                    } else {
                        note.frontmatter.tags.join(", ")
                    },
                    note.frontmatter.date,
                    preview(&note.body, 600)
                ));
            }
        }
        text.push_str(
            "For each note, propose a destination folder, 1-3 tags and a concrete next action. \
             Group similar notes and suggest merges. Don't delete anything: present the plan and \
             wait for my confirmation before applying it. Use dry_run first for rename/move/delete \
             operations, and call sync_notebook after a batch of changes.",
        );
        Ok(vec![PromptMessage::new_text(Role::User, text)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_keeps_short_text_and_truncates_long_text_on_a_char_boundary() {
        assert_eq!(preview("short", 10), "short");
        assert_eq!(preview("caf\u{e9} con leche", 4), "caf\u{e9}\u{2026}");
    }
}

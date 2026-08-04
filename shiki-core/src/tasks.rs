//! Checkbox tasks (`- [ ]` / `- [x]`) extracted from note bodies, with an
//! optional `@due(YYYY-MM-DD)` tag — the domain half of the TUI's global
//! tasks view. `extract` is a pure function of a body string (unit-testable
//! without any filesystem), and `toggle` is the one write path: it flips a
//! single checkbox in the note's file on disk, leaving every other byte
//! untouched, so the edit flows through the exact same git/auto-sync
//! machinery as any other note change.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use chrono::NaiveDate;
use regex::Regex;

use crate::{Error, Result};

/// One checkbox line found in a note's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// The exact line as it appears in the body — what `toggle` matches on,
    /// so the toggle still finds the right line even if other lines were
    /// added/removed since extraction shifted every index.
    pub raw_line: String,
    /// 0-based index among body lines *identical* to `raw_line` — two
    /// word-for-word duplicate tasks in one note stay distinguishable, so
    /// toggling the second one can't flip the first.
    pub occurrence: usize,
    pub done: bool,
    /// The task's own text, checkbox marker stripped (`@due(...)` kept as
    /// typed — it's part of what the user wrote, not metadata to hide).
    pub text: String,
    /// Parsed from a `@due(YYYY-MM-DD)` tag anywhere in the text, if present
    /// and well-formed — a malformed date is just text, not an error.
    pub due: Option<NaiveDate>,
}

fn task_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\s*[-*] \[)([ xX])(\] +)(.*)$").unwrap())
}

fn due_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"@due\((\d{4}-\d{2}-\d{2})\)").unwrap())
}

/// Every checkbox task in `body`, in the order they appear.
pub fn extract(body: &str) -> Vec<Task> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    body.lines()
        .filter_map(|line| {
            let caps = task_line_re().captures(line)?;
            let occurrence = {
                let n = seen.entry(line).or_insert(0);
                let current = *n;
                *n += 1;
                current
            };
            let text = caps[4].trim_end().to_string();
            let due = due_re()
                .captures(&text)
                .and_then(|c| NaiveDate::parse_from_str(&c[1], "%Y-%m-%d").ok());
            Some(Task {
                raw_line: line.to_string(),
                occurrence,
                done: &caps[2] != " ",
                text,
                due,
            })
        })
        .collect()
}

/// What `toggle` did: the task's new state plus its new address (`raw_line`
/// changed on disk — the checkbox flipped — and with it, potentially, its
/// occurrence index among now-identical lines). A caller keeping a `Task`
/// alive across the toggle must adopt all three or the *next* toggle of the
/// same row could match a different, coincidentally-identical line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toggled {
    pub done: bool,
    pub raw_line: String,
    pub occurrence: usize,
}

/// Flips one task's checkbox in the file at `path` and writes it back.
/// The target line is matched by exact content (`raw_line`) plus
/// `occurrence` among identical lines — never by stored index, which the
/// file drifting since extraction would invalidate. Lines inside the
/// leading `---` frontmatter block are never candidates, so YAML that
/// happens to look like a checkbox can't shift the count.
pub fn toggle(path: &Path, raw_line: &str, occurrence: usize) -> Result<Toggled> {
    let contents = std::fs::read_to_string(path)?;
    let mut lines: Vec<String> = contents.split('\n').map(str::to_string).collect();

    // Skip the frontmatter block, if any — mirrors `Note::split`'s
    // delimiter format (`---` opener on the first line, `---` closer).
    let mut start = 0;
    if lines.first().map(|l| l.trim_end_matches('\r')) == Some("---") {
        if let Some(close) = lines[1..]
            .iter()
            .position(|l| l.trim_end_matches('\r') == "---")
        {
            start = close + 2;
        }
    }

    let target = lines[start..]
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_end_matches('\r') == raw_line)
        .nth(occurrence)
        .map(|(i, _)| start + i);
    let Some(idx) = target else {
        return Err(Error::TaskNotFound(path.display().to_string()));
    };

    let line = &lines[idx];
    let caps = task_line_re()
        .captures(line.trim_end_matches('\r'))
        .ok_or_else(|| Error::TaskNotFound(path.display().to_string()))?;
    let new_done = &caps[2] == " ";
    let marker = if new_done { "x" } else { " " };
    let new_raw = format!("{}{}{}{}", &caps[1], marker, &caps[3], &caps[4]);
    let crlf = if line.ends_with('\r') { "\r" } else { "" };
    lines[idx] = format!("{new_raw}{crlf}");

    // The flipped line's occurrence among lines *now identical to it* —
    // counted the same way `extract` numbers them (top-down, frontmatter
    // excluded), so the returned address stays valid for a second toggle.
    let new_occurrence = lines[start..idx]
        .iter()
        .filter(|l| l.trim_end_matches('\r') == new_raw)
        .count();

    std::fs::write(path, lines.join("\n"))?;
    Ok(Toggled {
        done: new_done,
        raw_line: new_raw,
        occurrence: new_occurrence,
    })
}

fn any_due_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Any `@due(...)` regardless of content — `normalize_due_tags` decides
    // per capture whether the inside is already ISO, a known relative
    // spec, or junk to leave untouched.
    RE.get_or_init(|| Regex::new(r"@due\(([^)]*)\)").unwrap())
}

/// Parses a relative due spec against `today`: `today`, `tomorrow`/`tom`,
/// `+N`/`+Nd` (days), `+Nw` (weeks), or a weekday name (`mon`/`monday`, …,
/// meaning the *next* such weekday, never today). Case-insensitive. An
/// already-ISO date or anything unrecognized returns `None` — the caller
/// leaves those as-is rather than guessing.
pub fn parse_relative_due(spec: &str, today: NaiveDate) -> Option<NaiveDate> {
    let spec = spec.trim().to_ascii_lowercase();
    if spec.is_empty() {
        return None;
    }
    match spec.as_str() {
        "today" => return Some(today),
        "tomorrow" | "tom" => return today.succ_opt(),
        _ => {}
    }
    if let Some(rest) = spec.strip_prefix('+') {
        let (num, unit) = match rest.strip_suffix('d') {
            Some(n) => (n, 1i64),
            None => match rest.strip_suffix('w') {
                Some(n) => (n, 7i64),
                None => (rest, 1i64),
            },
        };
        let n: i64 = num.parse().ok()?;
        return today.checked_add_signed(chrono::Duration::days(n * unit));
    }
    if let Ok(weekday) = spec.parse::<chrono::Weekday>() {
        let today_num = chrono::Datelike::weekday(&today).num_days_from_monday() as i64;
        let target = weekday.num_days_from_monday() as i64;
        let ahead = (target - today_num).rem_euclid(7);
        let ahead = if ahead == 0 { 7 } else { ahead };
        return today.checked_add_signed(chrono::Duration::days(ahead));
    }
    None
}

/// Rewrites every relative `@due(...)` in `body` to its resolved
/// `@due(YYYY-MM-DD)` form — `Some(new body)` if anything changed, `None`
/// if there was nothing to normalize. ISO dates and unrecognized specs are
/// left byte-for-byte untouched: normalization only ever *resolves*, it
/// never invents or deletes. Callers run this at save time (a relative
/// spec is relative to the day it was written, so it must be pinned then,
/// not re-interpreted at every later read).
pub fn normalize_due_tags(body: &str, today: NaiveDate) -> Option<String> {
    let mut changed = false;
    let result = any_due_re().replace_all(body, |caps: &regex::Captures| {
        let spec = &caps[1];
        if NaiveDate::parse_from_str(spec.trim(), "%Y-%m-%d").is_ok() {
            return caps[0].to_string();
        }
        match parse_relative_due(spec, today) {
            Some(date) => {
                changed = true;
                format!("@due({date})")
            }
            None => caps[0].to_string(),
        }
    });
    changed.then(|| result.into_owned())
}

/// `"{notebook}/{folders…}/{title}"` — where a task lives, shared by the
/// TUI's tasks modal and `shiki tasks` so the two can't describe the same
/// task differently. Folders come from the note's path relative to its
/// notebook; the note itself goes by its human title rather than its
/// slugged file stem, matching what the NOTES panel shows.
pub fn location_of(nb: &crate::Notebook, note: &crate::Note) -> String {
    let mut parts = vec![nb.name.clone()];
    if let Ok(rel) = note.path.strip_prefix(&nb.path) {
        if let Some(parent) = rel.parent() {
            parts.extend(
                parent
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned()),
            );
        }
    }
    parts.push(note.frontmatter.title.clone());
    parts.join("/")
}

/// The "what's on for today" section injected into a freshly created daily
/// note: every *pending* task due today or overdue, across the whole pool,
/// sorted by due date (most overdue first). Deliberately plain bullets with
/// a `[[wikilink]]` back to the source note, **not** checkboxes — a
/// checkbox copy would show up in the tasks view as a second, independent
/// task, and checking the copy wouldn't complete the original. `None` when
/// nothing is due, so quiet days don't get an empty section.
pub fn agenda_section(pool: &[(crate::Notebook, crate::Note)], today: NaiveDate) -> Option<String> {
    let mut due: Vec<(&NaiveDate, &Task, &crate::Note)> = Vec::new();
    let extracted: Vec<(Vec<Task>, &crate::Note)> = pool
        .iter()
        .map(|(_, note)| (extract(&note.body), note))
        .collect();
    for (tasks, note) in &extracted {
        for task in tasks {
            if let Some(d) = &task.due {
                if !task.done && *d <= today {
                    due.push((d, task, note));
                }
            }
        }
    }
    if due.is_empty() {
        return None;
    }
    due.sort_by_key(|(d, _, _)| **d);
    let mut out = String::from("## Due today\n\n");
    for (d, task, note) in due {
        let overdue = if *d < today { " (overdue)" } else { "" };
        out.push_str(&format!(
            "- {}{overdue} \u{2192} [[{}]]\n",
            task.text, note.frontmatter.title
        ));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_finds_pending_and_done_tasks() {
        let body = "# Plan\n\n- [ ] buy milk\n- [x] call mom\n* [X] star style\nnot a task";
        let tasks = extract(body);
        assert_eq!(tasks.len(), 3);
        assert!(!tasks[0].done);
        assert_eq!(tasks[0].text, "buy milk");
        assert!(tasks[1].done);
        assert!(tasks[2].done);
    }

    #[test]
    fn extract_ignores_lines_that_only_look_similar() {
        // No space after the marker, no marker at all, checkbox mid-line.
        let body = "-[ ] no space\n[ ] bare\ntext - [ ] mid-line";
        assert!(extract(body).is_empty());
    }

    #[test]
    fn extract_parses_due_tag_and_keeps_it_in_text() {
        let body = "- [ ] pay rent @due(2026-08-10)\n- [ ] bad date @due(2026-13-99)";
        let tasks = extract(body);
        assert_eq!(
            tasks[0].due,
            Some(NaiveDate::from_ymd_opt(2026, 8, 10).unwrap())
        );
        assert!(tasks[0].text.contains("@due(2026-08-10)"));
        assert_eq!(tasks[1].due, None);
    }

    #[test]
    fn extract_numbers_identical_lines_by_occurrence() {
        let body = "- [ ] repeat\n- [ ] repeat\n- [ ] other";
        let tasks = extract(body);
        assert_eq!(tasks[0].occurrence, 0);
        assert_eq!(tasks[1].occurrence, 1);
        assert_eq!(tasks[2].occurrence, 0);
    }

    #[test]
    fn toggle_flips_only_the_addressed_occurrence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "---\ntitle: t\n---\n\n- [ ] repeat\n- [ ] repeat\n").unwrap();

        let toggled = toggle(&path, "- [ ] repeat", 1).unwrap();
        assert!(toggled.done);
        assert_eq!(toggled.raw_line, "- [x] repeat");
        assert_eq!(toggled.occurrence, 0);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "---\ntitle: t\n---\n\n- [ ] repeat\n- [x] repeat\n"
        );
    }

    #[test]
    fn toggle_unchecks_a_done_task() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "- [x] call mom\n").unwrap();
        assert!(!toggle(&path, "- [x] call mom", 0).unwrap().done);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "- [ ] call mom\n");
    }

    #[test]
    fn toggle_returns_the_new_address_even_among_identical_done_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        // A done twin already sits above — after toggling, the flipped
        // line becomes the *second* `- [x] repeat`, and the returned
        // occurrence must say so or a follow-up untoggle would flip the
        // wrong twin.
        std::fs::write(&path, "- [x] repeat\n- [ ] repeat\n").unwrap();
        let toggled = toggle(&path, "- [ ] repeat", 0).unwrap();
        assert_eq!(toggled.raw_line, "- [x] repeat");
        assert_eq!(toggled.occurrence, 1);
    }

    #[test]
    fn toggle_skips_checkbox_lookalikes_inside_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        // The frontmatter contains a line byte-identical to the task —
        // occurrence 0 must still resolve to the body's line, not YAML's.
        std::fs::write(&path, "---\n- [ ] task\n---\n- [ ] task\n").unwrap();
        toggle(&path, "- [ ] task", 0).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "---\n- [ ] task\n---\n- [x] task\n"
        );
    }

    #[test]
    fn parse_relative_due_handles_every_supported_form() {
        // 2026-08-04 is a Tuesday.
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        assert_eq!(parse_relative_due("today", today), Some(today));
        assert_eq!(parse_relative_due("tomorrow", today), Some(d(2026, 8, 5)));
        assert_eq!(parse_relative_due("tom", today), Some(d(2026, 8, 5)));
        assert_eq!(parse_relative_due("+3", today), Some(d(2026, 8, 7)));
        assert_eq!(parse_relative_due("+3d", today), Some(d(2026, 8, 7)));
        assert_eq!(parse_relative_due("+2w", today), Some(d(2026, 8, 18)));
        assert_eq!(parse_relative_due("fri", today), Some(d(2026, 8, 7)));
        assert_eq!(parse_relative_due("Monday", today), Some(d(2026, 8, 10)));
        // "The next such weekday" is never today — tue on a Tuesday is +7.
        assert_eq!(parse_relative_due("tue", today), Some(d(2026, 8, 11)));
        assert_eq!(parse_relative_due("2026-09-01", today), None);
        assert_eq!(parse_relative_due("garbage", today), None);
    }

    #[test]
    fn normalize_due_tags_rewrites_relative_and_leaves_iso_and_junk() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let body = "- [ ] a @due(tomorrow)\n- [ ] b @due(2026-09-01)\n- [ ] c @due(???)";
        let normalized = normalize_due_tags(body, today).unwrap();
        assert_eq!(
            normalized,
            "- [ ] a @due(2026-08-05)\n- [ ] b @due(2026-09-01)\n- [ ] c @due(???)"
        );
        // Nothing relative left — a second pass has nothing to do.
        assert_eq!(normalize_due_tags(&normalized, today), None);
    }

    #[test]
    fn agenda_section_lists_due_and_overdue_pending_tasks_as_plain_bullets() {
        use crate::note::Frontmatter;
        use crate::{Note, Notebook};
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let entry = |title: &str, body: &str| {
            (
                Notebook::new("nb", std::path::PathBuf::from("/tmp/nb")),
                Note::new(
                    std::path::PathBuf::from(format!("/tmp/nb/{title}.md")),
                    Frontmatter::new(title, "nb"),
                    body.to_string(),
                ),
            )
        };
        let pool = vec![
            entry(
                "Bills",
                "- [ ] pay rent @due(2026-08-01)\n- [x] done one @due(2026-08-01)",
            ),
            entry(
                "Plan",
                "- [ ] standup @due(2026-08-04)\n- [ ] later @due(2026-12-01)",
            ),
        ];

        let section = agenda_section(&pool, today).unwrap();

        // Overdue first, plain bullets (no `- [ ]`), wikilink back to the
        // source; done and future-dated tasks excluded.
        assert_eq!(
            section,
            "## Due today\n\n- pay rent @due(2026-08-01) (overdue) \u{2192} [[Bills]]\n- standup @due(2026-08-04) \u{2192} [[Plan]]\n"
        );
    }

    #[test]
    fn agenda_section_is_none_when_nothing_is_due() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        assert_eq!(agenda_section(&[], today), None);
    }

    #[test]
    fn toggle_errors_when_the_line_no_longer_exists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "- [ ] something else\n").unwrap();
        assert!(matches!(
            toggle(&path, "- [ ] vanished task", 0),
            Err(Error::TaskNotFound(_))
        ));
    }
}

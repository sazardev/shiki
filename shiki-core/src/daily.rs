use std::collections::HashMap;
use std::path::Path;

use chrono::NaiveDate;

use crate::note::{Frontmatter, Note};
use crate::notebook::Notebook;
use crate::templates::Template;
use crate::Result;

/// File name of the daily note for a given date: `YYYY-MM-DD-daily.md`.
pub fn daily_note_path(notebook: &Notebook, date: NaiveDate) -> std::path::PathBuf {
    notebook
        .path
        .join(format!("{}-daily.md", date.format("%Y-%m-%d")))
}

/// Creates (or opens, if it already exists) today's daily note in the given
/// notebook. `template_name` is `general.daily_template` from config (by
/// filename, without `.md`) — callers pass the configured value through
/// rather than this function hardcoding `"daily"`, so customizing that
/// setting (already editable in the Settings GENERAL tab) actually takes
/// effect instead of being silently ignored. `agenda` (see
/// `tasks::agenda_section` — today's due/overdue tasks) is appended after
/// the template **only on creation**: an already-existing daily is opened
/// untouched, so reopening it later in the day never duplicates the
/// section or clobbers edits made to it.
pub fn create_or_open(
    notebook: &Notebook,
    date: NaiveDate,
    templates_dir: &Path,
    template_name: &str,
    agenda: Option<&str>,
) -> Result<Note> {
    let path = daily_note_path(notebook, date);
    if path.exists() {
        return Note::from_file_in_notebook(&path, &notebook.name);
    }

    let mut body = match Template::load(templates_dir, template_name) {
        Ok(template) => {
            let mut vars = HashMap::new();
            vars.insert("date", date.format("%Y-%m-%d").to_string());
            template.render(&vars)
        }
        Err(_) => format!(
            "# {}\n\n## Tasks\n\n- [ ] \n\n## Notes\n\n",
            date.format("%Y-%m-%d")
        ),
    };
    if let Some(agenda) = agenda {
        if !body.ends_with('\n') {
            body.push('\n');
        }
        body.push('\n');
        body.push_str(agenda);
    }

    let mut frontmatter =
        Frontmatter::new(format!("{} Daily", date.format("%Y-%m-%d")), &notebook.name);
    frontmatter.date = date;
    frontmatter.template = Some(template_name.to_string());

    let note = Note::new(path, frontmatter, body);
    note.save()?;
    Ok(note)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_notebook(root: &Path, name: &str) -> Notebook {
        let path = root.join(name);
        std::fs::create_dir_all(&path).unwrap();
        Notebook::new(name, path)
    }

    #[test]
    fn daily_note_path_is_date_stamped_at_the_notebook_root() {
        let tmp = tempfile::tempdir().unwrap();
        let nb = test_notebook(tmp.path(), "personal");
        let date = NaiveDate::from_ymd_opt(2024, 3, 7).unwrap();

        let path = daily_note_path(&nb, date);

        assert_eq!(path, nb.path.join("2024-03-07-daily.md"));
    }

    #[test]
    fn create_or_open_creates_a_new_note_with_fallback_body_when_no_template_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let nb = test_notebook(tmp.path(), "personal");
        let templates_dir = tmp.path().join("templates"); // deliberately never created
        let date = NaiveDate::from_ymd_opt(2024, 3, 7).unwrap();

        let note = create_or_open(&nb, date, &templates_dir, "daily", None).unwrap();

        assert_eq!(note.frontmatter.title, "2024-03-07 Daily");
        assert_eq!(note.frontmatter.date, date);
        assert_eq!(note.frontmatter.template.as_deref(), Some("daily"));
        assert!(note.body.contains("2024-03-07"));
        assert!(note.path.exists());
    }

    #[test]
    fn create_or_open_reopens_the_existing_note_instead_of_overwriting_it() {
        let tmp = tempfile::tempdir().unwrap();
        let nb = test_notebook(tmp.path(), "personal");
        let templates_dir = tmp.path().join("templates");
        let date = NaiveDate::from_ymd_opt(2024, 3, 7).unwrap();

        let first = create_or_open(&nb, date, &templates_dir, "daily", None).unwrap();
        std::fs::write(
            &first.path,
            format!(
                "{}custom edit",
                "---\ntitle: 2024-03-07 Daily\ndate: 2024-03-07\nnotebook: personal\n---\n\n"
            ),
        )
        .unwrap();

        let second = create_or_open(&nb, date, &templates_dir, "daily", None).unwrap();

        assert_eq!(second.body, "custom edit");
    }

    #[test]
    fn create_or_open_appends_the_agenda_only_on_creation() {
        let tmp = tempfile::tempdir().unwrap();
        let nb = test_notebook(tmp.path(), "personal");
        let templates_dir = tmp.path().join("templates");
        let date = NaiveDate::from_ymd_opt(2024, 3, 7).unwrap();
        let agenda = "## Due today\n\n- pay rent \u{2192} [[Bills]]\n";

        let first = create_or_open(&nb, date, &templates_dir, "daily", Some(agenda)).unwrap();
        assert!(first.body.ends_with(agenda));

        // Reopening with a (different) agenda must not touch the file.
        let second =
            create_or_open(&nb, date, &templates_dir, "daily", Some("## Other\n")).unwrap();
        assert_eq!(second.body, first.body);
    }
}

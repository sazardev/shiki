//! The shared capture pipeline — one implementation of "turn text + flags
//! into a note (or a daily bullet)" for all three capture clients: the CLI
//! (`shiki capture`, both the daemon request and the direct-write fallback),
//! the in-TUI daemon (`shiki-tui/src/capture.rs`) and the browser
//! extension's native host (`shiki-native-host`). These were three
//! near-identical copies before; keeping the note-creating logic here means
//! a fix (like the port-file parsing bug the native host had) can't land in
//! one client and silently miss another.
//!
//! Notably absent: encryption handling. Each caller resolves its own
//! `Notebook::crypto` (the TUI caches unlocked passphrases per session, the
//! CLI prompts interactively, the native host can only refuse) and passes a
//! `Notebook` that already carries it — the functions here only ever
//! `save_with_crypto(nb.crypto.as_ref())`.

use std::path::{Path, PathBuf};

use chrono::Local;

use crate::notebook::validate_relative_path;
use crate::{LastCapture, Notebook, Result};

/// Appends the `Source: [title](url)` provenance footer, unless the text
/// already contains the URL. Used by every capture route so a clipped link
/// gets the same attribution regardless of which client captured it.
pub fn with_source(text: &str, url: Option<&str>, title: Option<&str>) -> String {
    let Some(url) = url.filter(|s| !s.is_empty()) else {
        return text.to_string();
    };
    if text.contains(url) {
        return text.to_string();
    }
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        format!("{text}\n\nSource: [{title}]({url})")
    } else {
        format!("{text}\n\nSource: {url}")
    }
}

/// Builds a `CAPTURE` request — see `shiki-tui/src/capture.rs`'s module doc
/// comment for the exact wire format. Header lines are only emitted when
/// actually needed, so the common case (plain capture, no flags) stays a
/// minimal two-line request. `notebook`/`folder` are only sent when the
/// caller explicitly passed them — omitting them lets the daemon apply its
/// own content-prefix routing/`default_notebook` fallback instead.
#[allow(clippy::too_many_arguments)]
pub fn build_capture_request(
    text: &str,
    daily: bool,
    tags: &[String],
    notebook: Option<&str>,
    folder: Option<&str>,
    template: Option<&str>,
    url: Option<&str>,
    title: Option<&str>,
    source: Option<&str>,
) -> String {
    let mut req = String::from("CAPTURE\n");
    if daily {
        req.push_str("daily=1\n");
    }
    if !tags.is_empty() {
        req.push_str(&format!("tags={}\n", tags.join(",")));
    }
    if let Some(notebook) = notebook {
        req.push_str(&format!("notebook={notebook}\n"));
    }
    if let Some(folder) = folder {
        req.push_str(&format!("folder={folder}\n"));
    }
    if let Some(template) = template {
        req.push_str(&format!("template={template}\n"));
    }
    if let Some(url) = url {
        req.push_str(&format!("url={url}\n"));
    }
    if let Some(title) = title {
        req.push_str(&format!("title={title}\n"));
    }
    if let Some(source) = source {
        req.push_str(&format!("source={source}\n"));
    }
    req.push('\n');
    req.push_str(text);
    req
}

/// The capture daemon writes its port file as `"{port} {pid}\n"`
/// (`shiki-tui/src/capture.rs::write_port_file`) — so the port is the
/// *first* whitespace-separated token, not the whole file. Parsing the
/// trimmed file as a single number (which the native host used to do)
/// fails on the real two-token format and silently loses daemon
/// connectivity.
pub fn parse_port_file(contents: &str) -> Option<u16> {
    contents.split_whitespace().next()?.parse().ok()
}

/// The pid half of that same `"{port} {pid}\n"` file, used to tell a live
/// daemon from a stale port file left by a crashed process.
pub fn parse_pid(contents: &str) -> Option<u32> {
    contents.split_whitespace().nth(1)?.parse().ok()
}

/// Creates a plain capture note titled `Capture YYYY-MM-DD HH:MM`, inside
/// `folder` when one is given (validated like any other relative path).
/// An empty `folder` string means "no folder", the same as `None` — the
/// native host used to normalize that itself while the CLI/TUI rejected
/// it, so the shared version normalizes once for everyone.
pub fn capture_into_new_note(
    nb: &Notebook,
    text: &str,
    tags: &[String],
    folder: Option<&str>,
) -> Result<(PathBuf, LastCapture)> {
    let title = format!("Capture {}", Local::now().format("%Y-%m-%d %H:%M"));
    let mut note = match folder.filter(|f| !f.is_empty()) {
        Some(folder) => {
            let relative = validate_relative_path(folder)?;
            nb.create_note_in(&relative, &title, text)?
        }
        None => nb.create_note(&title, text)?,
    };
    if !tags.is_empty() {
        note.frontmatter.tags = tags.to_vec();
        note.save_with_crypto(nb.crypto.as_ref())?;
    }
    let record = LastCapture::Note {
        notebook: nb.name.clone(),
        path: note.path.display().to_string(),
    };
    Ok((note.path, record))
}

/// Like [`capture_into_new_note`], but renders `text` through a note
/// template first. The template gets `title`/`date`/`body`/`notebook` as
/// substitution variables; when the rendered result doesn't already contain
/// the captured text (the template has no `{{body}}`), the text is appended
/// below it so a capture can never be silently dropped by a template.
pub fn capture_into_templated(
    nb: &Notebook,
    text: &str,
    tags: &[String],
    folder: Option<&str>,
    template_name: &str,
    templates_dir: &Path,
) -> Result<(PathBuf, LastCapture)> {
    let tmpl = crate::Template::load(templates_dir, template_name)
        .map_err(|_| crate::Error::TemplateNotFound(template_name.to_string()))?;
    let title = format!("Capture {}", Local::now().format("%Y-%m-%d %H:%M"));
    let mut vars = std::collections::HashMap::new();
    vars.insert("title", title.clone());
    vars.insert("date", Local::now().format("%Y-%m-%d").to_string());
    vars.insert("body", text.to_string());
    vars.insert("notebook", nb.name.clone());
    let rendered = tmpl.render(&vars);
    let body = if rendered.contains(text) {
        rendered
    } else {
        format!("{rendered}\n{text}\n")
    };
    let mut note = match folder.filter(|f| !f.is_empty()) {
        Some(folder) => {
            let relative = validate_relative_path(folder)?;
            nb.create_note_in(&relative, &title, body)?
        }
        None => nb.create_note(&title, body)?,
    };
    if !tags.is_empty() {
        note.frontmatter.tags = tags.to_vec();
        note.save_with_crypto(nb.crypto.as_ref())?;
    }
    let record = LastCapture::Note {
        notebook: nb.name.clone(),
        path: note.path.display().to_string(),
    };
    Ok((note.path, record))
}

/// Opens (or creates) today's daily note and appends `- {text}` as one
/// bullet, recording the exact appended string so `--undo` can strip it
/// back off verbatim. `agenda` is the optional "## Due today" section
/// content, only ever used when the daily is being created (see
/// `daily::create_or_open`'s own contract) — callers build it from
/// `tasks::agenda_section` when `general.daily_agenda` is on.
pub fn capture_into_daily(
    nb: &Notebook,
    text: &str,
    templates_dir: &Path,
    daily_template: &str,
    agenda: Option<&str>,
) -> Result<(PathBuf, LastCapture)> {
    let today = Local::now().date_naive();
    let mut note = crate::daily::create_or_open(nb, today, templates_dir, daily_template, agenda)?;
    if !note.body.ends_with('\n') {
        note.body.push('\n');
    }
    let appended = format!("- {text}\n");
    note.body.push_str(&appended);
    note.save_with_crypto(nb.crypto.as_ref())?;
    let record = LastCapture::DailyAppend {
        notebook: nb.name.clone(),
        path: note.path.display().to_string(),
        appended,
    };
    Ok((note.path, record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NotebookStore;

    #[test]
    fn with_source_appends_url_and_title() {
        assert_eq!(
            with_source("idea", Some("https://x.com"), Some("X")),
            "idea\n\nSource: [X](https://x.com)"
        );
        assert_eq!(
            with_source("idea", Some("https://x.com"), None),
            "idea\n\nSource: https://x.com"
        );
        // Already-present URL / empty values leave the text untouched.
        assert_eq!(
            with_source("see https://x.com", Some("https://x.com"), Some("X")),
            "see https://x.com"
        );
        assert_eq!(with_source("idea", Some(""), Some("X")), "idea");
        assert_eq!(with_source("idea", None, None), "idea");
    }

    #[test]
    fn build_request_minimal_has_no_headers() {
        assert_eq!(
            build_capture_request("hello", false, &[], None, None, None, None, None, None),
            "CAPTURE\n\nhello"
        );
    }

    #[test]
    fn build_request_emits_only_the_given_headers() {
        let req = build_capture_request(
            "hello",
            true,
            &["a".to_string(), "b".to_string()],
            Some("work"),
            Some("meetings"),
            Some("meeting"),
            Some("https://x.com"),
            Some("X"),
            Some("voice"),
        );
        assert_eq!(
            req,
            "CAPTURE\ndaily=1\ntags=a,b\nnotebook=work\nfolder=meetings\ntemplate=meeting\n\
             url=https://x.com\ntitle=X\nsource=voice\n\nhello"
        );
    }

    /// The daemon writes `"{port} {pid}\n"` — the parser has to read the
    /// port from that two-token format, not expect a bare number (the bug
    /// the native host's own copy had).
    #[test]
    fn parse_port_file_reads_the_real_two_token_format() {
        assert_eq!(parse_port_file("54321 12345\n"), Some(54321));
        assert_eq!(parse_port_file("54321\n"), Some(54321));
        assert_eq!(parse_port_file("  54321  "), Some(54321));
        assert_eq!(parse_port_file(""), None);
        assert_eq!(parse_port_file("not-a-port 12345"), None);
        assert_eq!(parse_port_file("-1"), None);
    }

    #[test]
    fn parse_pid_reads_the_second_token() {
        assert_eq!(parse_pid("54321 12345"), Some(12345));
        assert_eq!(parse_pid("54321 12345\n"), Some(12345));
        assert_eq!(parse_pid("54321"), None);
        assert_eq!(parse_pid("54321 not-a-pid"), None);
    }

    #[test]
    fn capture_into_new_note_writes_a_titled_note_with_tags() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().to_path_buf());
        let nb = store.create("personal").unwrap();
        let tags = vec!["idea".to_string()];
        let (path, record) = capture_into_new_note(&nb, "quick thought", &tags, None).unwrap();
        assert!(path.is_file());
        let saved = crate::Note::from_file(&path).unwrap();
        assert!(saved.frontmatter.title.starts_with("Capture "));
        assert_eq!(saved.body.trim(), "quick thought");
        assert_eq!(saved.frontmatter.tags, tags);
        assert_eq!(
            record,
            LastCapture::Note {
                notebook: "personal".into(),
                path: path.display().to_string(),
            }
        );
    }

    #[test]
    fn capture_into_new_note_treats_an_empty_folder_as_none() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().to_path_buf());
        let nb = store.create("personal").unwrap();
        let (path, _) = capture_into_new_note(&nb, "root note", &[], Some("")).unwrap();
        assert_eq!(path.parent().unwrap(), nb.path);
    }

    #[test]
    fn capture_into_new_note_rejects_a_traversal_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().to_path_buf());
        let nb = store.create("personal").unwrap();
        assert!(capture_into_new_note(&nb, "nope", &[], Some("../escape")).is_err());
    }
}

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{Context, Result};
use shiki_config::{Config, LastCapture};
use shiki_core::NotebookStore;

use super::unlock_if_encrypted;

/// A parsed one-line reply from the capture daemon (`shiki-tui/src/
/// capture.rs`) — `OK <payload>` or `ERR <message>`. What `<payload>`
/// means depends on which request was sent: a note path for `CAPTURE`/
/// `UNDO`, or `enabled`/`disabled` for `PING`.
enum DaemonResponse {
    Ok(String),
    Err(String),
}

fn parse_port_file(contents: &str) -> Option<u16> {
    contents.trim().parse().ok()
}

fn parse_response_line(line: &str) -> Option<DaemonResponse> {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(rest) = line.strip_prefix("OK ") {
        return Some(DaemonResponse::Ok(rest.to_string()));
    }
    line.strip_prefix("ERR ")
        .map(|rest| DaemonResponse::Err(rest.to_string()))
}

/// Builds a `CAPTURE` request — see `shiki-tui/src/capture.rs`'s module doc
/// comment for the exact wire format. Header lines are only emitted when
/// actually needed, so the common case (plain capture, no flags) stays a
/// minimal two-line request. `notebook` is only sent when the caller
/// explicitly passed `-n` — omitting it lets the daemon apply its own
/// content-prefix routing/`default_notebook` fallback instead.
fn build_capture_request(
    text: &str,
    daily: bool,
    tags: &[String],
    notebook: Option<&str>,
    folder: Option<&str>,
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
    req.push('\n');
    req.push_str(text);
    req
}

/// Captures a quick note. Tries a running TUI's capture daemon first — if
/// one answers, its response is authoritative (an explicit `locked: ...`
/// refusal is reported as an error rather than silently bypassed by
/// falling back to a direct write, which could otherwise duplicate work or
/// confuse the user about which passphrase prompt they're about to hit).
/// Anything else (no daemon running, the daemon replying "disabled", a
/// connection timeout) falls back to writing straight to disk — the same
/// path `shiki new`/`shiki daily` already use, including the interactive
/// passphrase prompt for an encrypted notebook, since a human is sitting
/// at this terminal in that case.
#[allow(clippy::too_many_arguments)]
pub fn run(
    store: &NotebookStore,
    config: &Config,
    notebook: Option<String>,
    text: Option<String>,
    tags: &[String],
    daily: bool,
    json: bool,
    check: bool,
    undo: bool,
    folder: Option<String>,
) -> Result<()> {
    if check {
        return run_check(json);
    }
    if undo {
        return run_undo(json);
    }

    let text = match text {
        Some(t) => t,
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("failed to read capture text from stdin")?;
            buf
        }
    };

    let request = build_capture_request(&text, daily, tags, notebook.as_deref(), folder.as_deref());
    match try_daemon(&request) {
        Some(DaemonResponse::Ok(path)) => {
            print_capture_result(&path, true, daily, json);
            return Ok(());
        }
        Some(DaemonResponse::Err(msg)) if msg.starts_with("locked:") => {
            anyhow::bail!("{msg}");
        }
        Some(DaemonResponse::Err(_)) | None => {
            // Daemon disabled/unreachable/refused for a non-lock reason —
            // fall through to the direct-write path below.
        }
    }

    let existing_notebooks: Vec<String> = store
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|nb| nb.name)
        .collect();
    let (target, text) = match notebook.as_deref() {
        Some(name) => (name.to_string(), text.as_str()),
        None => shiki_core::notebook::route_by_prefix(&text, &existing_notebooks)
            .unwrap_or_else(|| (config.general.default_notebook.clone(), &text)),
    };

    let nb = match store.get(&target) {
        Ok(nb) => nb,
        Err(_) => store
            .create(&target)
            .with_context(|| format!("could not create notebook '{target}'"))?,
    };
    let nb = unlock_if_encrypted(config, nb)?;

    let (path, record) = if daily {
        capture_into_daily(store, config, &nb, text)?
    } else {
        capture_into_new_note(&nb, text, tags, folder.as_deref())?
    };
    if let Ok(record_path) = Config::default_last_capture_path() {
        let _ = record.save(&record_path);
    }
    print_capture_result(&path.display().to_string(), false, daily, json);
    Ok(())
}

fn capture_into_new_note(
    nb: &shiki_core::Notebook,
    text: &str,
    tags: &[String],
    folder: Option<&str>,
) -> Result<(std::path::PathBuf, LastCapture)> {
    let title = format!("Capture {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
    let mut note = match folder {
        Some(folder) => {
            let relative = shiki_core::notebook::validate_relative_path(folder)?;
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

fn capture_into_daily(
    store: &NotebookStore,
    config: &Config,
    nb: &shiki_core::Notebook,
    text: &str,
) -> Result<(std::path::PathBuf, LastCapture)> {
    let today = chrono::Local::now().date_naive();
    let templates_dir = Config::default_templates_dir()?;
    let agenda = config
        .general
        .daily_agenda
        .then(|| {
            store
                .all_notes()
                .ok()
                .and_then(|pool| shiki_core::tasks::agenda_section(&pool, today))
        })
        .flatten();
    let mut note = shiki_core::daily::create_or_open(
        nb,
        today,
        &templates_dir,
        &config.general.daily_template,
        agenda.as_deref(),
    )?;
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

fn format_capture_result(path: &str, via_daemon: bool, daily: bool, json: bool) -> String {
    if json {
        format!(r#"{{"path": {path:?}, "daemon": {via_daemon}, "daily": {daily}}}"#)
    } else if via_daemon {
        format!("captured (daemon): {path}")
    } else {
        format!("captured: {path}")
    }
}

fn print_capture_result(path: &str, via_daemon: bool, daily: bool, json: bool) {
    println!("{}", format_capture_result(path, via_daemon, daily, json));
}

/// `shiki capture --undo`: reverses the single most recent capture,
/// wherever it landed. Tries the daemon first (`UNDO`) so a live TUI
/// refreshes immediately if the reverted note was on screen; falls back to
/// a standalone undo (same `LastCapture` record, shared with the daemon)
/// when nothing answers.
fn run_undo(json: bool) -> Result<()> {
    match try_daemon("UNDO\n\n") {
        Some(DaemonResponse::Ok(path)) => {
            print_undo_result(&path, true, json);
            return Ok(());
        }
        Some(DaemonResponse::Err(msg)) if msg.starts_with("locked:") => {
            anyhow::bail!("{msg}");
        }
        Some(DaemonResponse::Err(_)) | None => {
            // Unreachable, disabled, or a non-lock error (e.g. "nothing to
            // undo") — falls through to the standalone path, which reads
            // the exact same shared record and reports the same outcome.
        }
    }
    run_undo_standalone(json)
}

fn run_undo_standalone(json: bool) -> Result<()> {
    let record_path = Config::default_last_capture_path()?;
    let Some(record) = LastCapture::load(&record_path) else {
        if json {
            println!(r#"{{"undone": false, "error": "nothing to undo"}}"#);
        } else {
            println!("nothing to undo");
        }
        anyhow::bail!("nothing to undo");
    };

    let (notebook, path) = match &record {
        LastCapture::Note { notebook, path } => (notebook.clone(), path.clone()),
        LastCapture::DailyAppend { notebook, path, .. } => (notebook.clone(), path.clone()),
    };

    let config_path = Config::default_path()?;
    let config = Config::load_or_init(&config_path)?;
    let data_dir = match config.general.data_dir.as_ref() {
        Some(dir) => std::path::PathBuf::from(dir),
        None => Config::default_data_dir()?,
    };
    let store = NotebookStore::new_with_custom_paths(data_dir, config.notebook_custom_paths());
    let nb = store
        .get(&notebook)
        .with_context(|| format!("notebook '{notebook}' not found"))?;

    match &record {
        LastCapture::Note { path, .. } => {
            let trash_dir = Config::default_trash_dir()?.join(&notebook);
            let suffix = chrono::Local::now().format("%Y%m%d%H%M%S%3f").to_string();
            shiki_core::trash::move_to_trash(std::path::Path::new(path), &trash_dir, &suffix)
                .with_context(|| format!("could not move '{path}' to trash"))?;
        }
        LastCapture::DailyAppend { path, appended, .. } => {
            let nb = unlock_if_encrypted(&config, nb)?;
            let mut note = shiki_core::Note::from_file_in_notebook_with_crypto(
                std::path::Path::new(path),
                &notebook,
                nb.crypto.as_ref(),
            )
            .with_context(|| format!("could not read '{path}'"))?;
            if !note.body.ends_with(appended.as_str()) {
                anyhow::bail!("the daily note has changed since that capture — not undoing");
            }
            let new_len = note.body.len() - appended.len();
            note.body.truncate(new_len);
            note.save_with_crypto(nb.crypto.as_ref())?;
        }
    }

    LastCapture::clear(&record_path);
    print_undo_result(&path, false, json);
    Ok(())
}

fn format_undo_result(path: &str, via_daemon: bool, json: bool) -> String {
    if json {
        format!(r#"{{"undone": true, "path": {path:?}, "daemon": {via_daemon}}}"#)
    } else if via_daemon {
        format!("undone (daemon): {path}")
    } else {
        format!("undone: {path}")
    }
}

fn print_undo_result(path: &str, via_daemon: bool, json: bool) {
    println!("{}", format_undo_result(path, via_daemon, json));
}

/// `shiki capture --check`: reports whether a capture daemon is reachable
/// right now, without capturing anything or touching stdin — meant for a
/// script/status-bar module to decide what to show (e.g. "capture: on" in
/// waybar) before committing to an actual capture. Exits non-zero when
/// unreachable, so `shiki capture --check && ...` composes naturally.
fn run_check(json: bool) -> Result<()> {
    let response = try_daemon("PING\n\n");
    let (reachable, enabled) = match response {
        Some(DaemonResponse::Ok(status)) => (true, status == "enabled"),
        _ => (false, false),
    };
    println!("{}", format_check_result(reachable, enabled, json));
    if !reachable {
        anyhow::bail!("capture daemon not reachable");
    }
    Ok(())
}

fn format_check_result(reachable: bool, enabled: bool, json: bool) -> String {
    if json {
        format!(r#"{{"reachable": {reachable}, "enabled": {enabled}}}"#)
    } else if reachable {
        format!(
            "daemon: reachable ({})",
            if enabled { "enabled" } else { "disabled" }
        )
    } else {
        "daemon: not reachable".to_string()
    }
}

/// `None` means "nothing to connect to" (no port file, unparsable
/// content, connection refused/timed out) — the caller treats that
/// identically to "daemon disabled". `Some` means the daemon actually
/// answered and its response must be respected.
fn try_daemon(request: &str) -> Option<DaemonResponse> {
    let port_path = Config::default_capture_port_path().ok()?;
    let contents = std::fs::read_to_string(port_path).ok()?;
    let port = parse_port_file(&contents)?;

    let mut stream =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300))
            .ok()?;
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    stream.write_all(request.as_bytes()).ok()?;
    let _ = stream.shutdown(std::net::Shutdown::Write);

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    parse_response_line(&response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_port_file_accepts_valid_content() {
        assert_eq!(parse_port_file("54321"), Some(54321));
        assert_eq!(parse_port_file("54321\n"), Some(54321));
        assert_eq!(parse_port_file("  54321  "), Some(54321));
    }

    #[test]
    fn parse_port_file_rejects_malformed_content() {
        assert_eq!(parse_port_file(""), None);
        assert_eq!(parse_port_file("not-a-port"), None);
        assert_eq!(parse_port_file("-1"), None);
    }

    #[test]
    fn parse_response_line_recognizes_ok_and_err() {
        match parse_response_line("OK /tmp/note.md\n") {
            Some(DaemonResponse::Ok(path)) => assert_eq!(path, "/tmp/note.md"),
            _ => panic!("expected Ok"),
        }
        match parse_response_line("ERR locked: notebook 'personal' is locked\n") {
            Some(DaemonResponse::Err(msg)) => assert!(msg.starts_with("locked:")),
            _ => panic!("expected Err"),
        }
    }

    #[test]
    fn parse_response_line_rejects_unrecognized_content() {
        assert!(parse_response_line("garbage").is_none());
        assert!(parse_response_line("").is_none());
    }

    #[test]
    fn build_capture_request_omits_absent_headers() {
        assert_eq!(
            build_capture_request("buy milk", false, &[], None, None),
            "CAPTURE\n\nbuy milk"
        );
    }

    #[test]
    fn build_capture_request_includes_every_header_when_given() {
        let tags = vec!["work".to_string(), "idea".to_string()];
        assert_eq!(
            build_capture_request("buy milk", true, &tags, Some("work"), Some("work/meetings")),
            "CAPTURE\ndaily=1\ntags=work,idea\nnotebook=work\nfolder=work/meetings\n\nbuy milk"
        );
    }

    #[test]
    fn format_capture_result_plain_text_distinguishes_daemon_from_fallback() {
        assert_eq!(
            format_capture_result("/tmp/note.md", true, false, false),
            "captured (daemon): /tmp/note.md"
        );
        assert_eq!(
            format_capture_result("/tmp/note.md", false, false, false),
            "captured: /tmp/note.md"
        );
    }

    #[test]
    fn format_capture_result_json_shape_is_stable() {
        assert_eq!(
            format_capture_result("/tmp/note.md", true, true, true),
            r#"{"path": "/tmp/note.md", "daemon": true, "daily": true}"#
        );
    }

    #[test]
    fn format_undo_result_distinguishes_daemon_from_standalone_and_json() {
        assert_eq!(
            format_undo_result("/tmp/note.md", true, false),
            "undone (daemon): /tmp/note.md"
        );
        assert_eq!(
            format_undo_result("/tmp/note.md", false, false),
            "undone: /tmp/note.md"
        );
        assert_eq!(
            format_undo_result("/tmp/note.md", false, true),
            r#"{"undone": true, "path": "/tmp/note.md", "daemon": false}"#
        );
    }

    #[test]
    fn format_check_result_matches_reachability_and_enabled_state() {
        assert_eq!(
            format_check_result(true, true, false),
            "daemon: reachable (enabled)"
        );
        assert_eq!(
            format_check_result(true, false, false),
            "daemon: reachable (disabled)"
        );
        assert_eq!(
            format_check_result(false, false, false),
            "daemon: not reachable"
        );
        assert_eq!(
            format_check_result(true, false, true),
            r#"{"reachable": true, "enabled": false}"#
        );
    }
}

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{Context, Result};
use shiki_config::Config;
use shiki_core::capture::{
    build_capture_request, capture_into_daily, capture_into_new_note, capture_into_templated,
    parse_pid, parse_port_file, with_source,
};
use shiki_core::process::is_pid_alive;
use shiki_core::{LastCapture, NotebookStore};

use super::unlock_if_encrypted;

/// A parsed one-line reply from the capture daemon (`shiki-tui/src/
/// capture.rs`) — `OK <payload>` or `ERR <message>`. What `<payload>`
/// means depends on which request was sent: a note path for `CAPTURE`/
/// `UNDO`, or `enabled`/`disabled` for `PING`.
pub(crate) enum DaemonResponse {
    Ok(String),
    Err(String),
}

fn parse_response_line(line: &str) -> Option<DaemonResponse> {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(rest) = line.strip_prefix("OK ") {
        return Some(DaemonResponse::Ok(rest.to_string()));
    }
    line.strip_prefix("ERR ")
        .map(|rest| DaemonResponse::Err(rest.to_string()))
}

fn read_clipboard_text() -> Result<String> {
    let mut cb = arboard::Clipboard::new().context("could not open clipboard (no display?)")?;
    let text = cb.get_text().context("clipboard has no text")?;
    if text.trim().is_empty() {
        anyhow::bail!("clipboard is empty");
    }
    Ok(text)
}

fn validate_no_newline(value: &str, field: &str) -> Result<()> {
    if value.contains('\n') || value.contains('\r') {
        anyhow::bail!("{field} must not contain newline");
    }
    Ok(())
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
    template: Option<String>,
    url: Option<String>,
    title: Option<String>,
    clip: bool,
    source: Option<String>,
    voice: bool,
    seconds: u32,
    model: &str,
) -> Result<()> {
    if check {
        return run_check(json);
    }
    if undo {
        return run_undo(json);
    }

    // Guard against header injection — daemon protocol is \n-delimited
    if let Some(t) = &template {
        validate_no_newline(t, "template")?;
        if t.contains('/') || t.contains('\\') || t.contains('.') {
            anyhow::bail!("invalid template: must be a plain name");
        }
    }
    if let Some(u) = &url {
        validate_no_newline(u, "url")?;
    }
    if let Some(t) = &title {
        validate_no_newline(t, "title")?;
    }
    if let Some(s) = &source {
        validate_no_newline(s, "source")?;
    }
    if let Some(nb) = &notebook {
        validate_no_newline(nb, "notebook")?;
    }
    if let Some(f) = &folder {
        validate_no_newline(f, "folder")?;
    }
    for t in tags {
        validate_no_newline(t, "tag")?;
    }

    // `--voice`: record + transcribe locally first, then treat the
    // transcript as the capture text (source defaults to "voice").
    let voice_text = if voice {
        if text.is_some() || clip {
            anyhow::bail!("--voice cannot be combined with a positional text argument or --clip");
        }
        Some(shiki_core::voice::capture_transcript(
            &store.root.join("bin"),
            seconds,
            model,
        )?)
    } else {
        None
    };
    let source = source.or_else(|| voice.then(|| "voice".to_string()));

    let raw_text = if let Some(vt) = voice_text {
        vt
    } else if clip {
        read_clipboard_text()?
    } else {
        match text {
            Some(t) => t,
            None => {
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .context("failed to read capture text from stdin")?;
                buf
            }
        }
    };

    let request = build_capture_request(
        &raw_text,
        daily,
        tags,
        notebook.as_deref(),
        folder.as_deref(),
        template.as_deref(),
        url.as_deref(),
        title.as_deref(),
        source.as_deref(),
    );
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

    let (path, daily) = perform_direct_capture(
        store, config, notebook, &raw_text, tags, daily, folder, template, url, title, true,
    )?;
    print_capture_result(&path.display().to_string(), false, daily, json);
    Ok(())
}

/// The direct-disk fallback for `shiki capture` — the exact same
/// note-creation path the daemon-less path always took, factored out so
/// the standalone `shiki daemon` (headless) handler reuses it instead of
/// duplicating it. Resolves the target notebook (explicit `-n` >
/// content-prefix routing > `default_notebook`), appends the `Source:`
/// footer, and writes through `capture_into_daily`/`capture_into_templated`/
/// `capture_into_new_note`. `interactive` controls the encrypted-notebook
/// case: `true` (a human at a terminal) prompts for the passphrase, `false`
/// (a background daemon) replies `locked:` instead.
#[allow(clippy::too_many_arguments)]
pub(crate) fn perform_direct_capture(
    store: &NotebookStore,
    config: &Config,
    notebook: Option<String>,
    raw_text: &str,
    tags: &[String],
    daily: bool,
    folder: Option<String>,
    template: Option<String>,
    url: Option<String>,
    title: Option<String>,
    interactive: bool,
) -> Result<(std::path::PathBuf, bool)> {
    let existing_notebooks: Vec<String> = store
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|nb| nb.name)
        .collect();
    let (target, body_owned) = match notebook.as_deref() {
        Some(name) => (name.to_string(), raw_text.to_string()),
        None => shiki_core::notebook::route_by_prefix(raw_text, &existing_notebooks)
            .map(|(n, t)| (n, t.to_string()))
            .unwrap_or_else(|| {
                (
                    config.general.default_notebook.clone(),
                    raw_text.to_string(),
                )
            }),
    };
    let text = with_source(&body_owned, url.as_deref(), title.as_deref());

    let nb = match store.get(&target) {
        Ok(nb) => nb,
        Err(_) => store
            .create(&target)
            .with_context(|| format!("could not create notebook '{target}'"))?,
    };
    let nb = if config.encrypt_for(&target) {
        if !interactive {
            anyhow::bail!(
                "locked: notebook '{target}' is encrypted and locked \u{2014} unlock it in the TUI \
                 or run `shiki capture` from a terminal instead"
            );
        }
        unlock_if_encrypted(config, nb)?
    } else {
        nb
    };

    let templates_dir = Config::default_templates_dir()?;
    let (path, record) = if daily {
        let today = chrono::Local::now().date_naive();
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
        capture_into_daily(
            &nb,
            &text,
            &templates_dir,
            &config.general.daily_template,
            agenda.as_deref(),
        )?
    } else if let Some(tmpl) = template.as_deref().filter(|s| !s.is_empty()) {
        capture_into_templated(&nb, &text, tags, folder.as_deref(), tmpl, &templates_dir)?
    } else {
        capture_into_new_note(&nb, &text, tags, folder.as_deref())?
    };
    if let Ok(record_path) = Config::default_last_capture_path() {
        let _ = record.save(&record_path);
    }
    Ok((path, daily))
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
    match perform_direct_undo(true) {
        Ok(path) => {
            print_undo_result(&path.display().to_string(), false, json);
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("nothing to undo") {
                if json {
                    println!(r#"{{"undone": false, "error": "nothing to undo"}}"#);
                } else {
                    println!("nothing to undo");
                }
            }
            Err(e)
        }
    }
}

/// Reverses the single most recent capture directly (no daemon): moves a
/// plain note to trash, or strips the bullet back off a `--daily` append —
/// the shared implementation used by `shiki capture --undo`'s fallback and
/// the headless `shiki daemon`. `interactive` controls the encrypted-
/// notebook case like `perform_direct_capture`: a terminal prompts for the
/// passphrase, a background daemon returns `locked:` instead.
pub(crate) fn perform_direct_undo(interactive: bool) -> Result<std::path::PathBuf> {
    let record_path = Config::default_last_capture_path()?;
    let Some(record) = LastCapture::load(&record_path) else {
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
    let mut store = NotebookStore::new_with_custom_paths(data_dir, config.notebook_custom_paths());
    store.extra_extensions = config.general.note_extra_extensions.clone();
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
            let encrypted = config.encrypt_for(&notebook);
            let nb = if encrypted {
                if !interactive {
                    anyhow::bail!(
                        "locked: notebook '{notebook}' is encrypted and locked \u{2014} unlock it \
                         in the TUI or run `shiki capture` from a terminal instead"
                    );
                }
                unlock_if_encrypted(&config, nb)?
            } else {
                nb
            };
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
    Ok(std::path::PathBuf::from(path))
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
/// answered and its response must be respected. `pub(crate)` so `shiki
/// doctor` can report daemon reachability the same way `--check` does.
pub(crate) fn try_daemon(request: &str) -> Option<DaemonResponse> {
    let port_path = Config::default_capture_port_path().ok()?;
    let contents = std::fs::read_to_string(&port_path).ok()?;
    let port = parse_port_file(&contents)?;
    // Stale-file detection: if the port file also records a pid and that
    // pid is no longer alive, the daemon crashed without cleaning up.
    // Remove the stale file so the next `shiki capture` falls through to
    // the direct-write path immediately instead of hanging on a dead port.
    if let Some(pid) = parse_pid(&contents) {
        if !is_pid_alive(pid) {
            let _ = std::fs::remove_file(&port_path);
            return None;
        }
    }

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

/// Returns `Some(true)` if the port file exists but its pid is stale,
/// `Some(false)` if it exists and is not stale (or has no pid), `None`
/// if the file doesn't exist or is unreadable. Used by `shiki doctor`
/// to distinguish "stale" from "not running".
pub(crate) fn is_port_file_stale() -> Option<bool> {
    let port_path = Config::default_capture_port_path().ok()?;
    let contents = std::fs::read_to_string(&port_path).ok()?;
    let pid = parse_pid(&contents)?;
    Some(!is_pid_alive(pid))
}

#[cfg(test)]
mod tests {
    use super::*;

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

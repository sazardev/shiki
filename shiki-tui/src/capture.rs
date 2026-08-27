//! Quick-capture daemon (`general.enable_capture_daemon`) — lets an
//! external `shiki capture "text"` invocation (`shiki-cli/src/commands/
//! capture.rs`) land in an already-running TUI instance live, instead of
//! only writing to disk unnoticed. On by default; `shiki capture` itself
//! always works with or without this running (see the CLI side's direct-
//! write fallback) — this module only covers the "TUI is open and wants to
//! know immediately" path.
//!
//! Transport is a plain TCP loopback socket (`127.0.0.1`, an OS-assigned
//! ephemeral port), not a Unix domain socket, specifically so the exact
//! same implementation works on Windows too — shiki ships real Windows
//! binaries (see `release.yml`), and a Unix-socket-only daemon would leave
//! that platform without the feature entirely. The bound port is recorded
//! in `Config::default_capture_port_path()`, rewritten on every daemon
//! start, so a dead process never leaves a stale port number for the next
//! `shiki capture` invocation to hang against.
//!
//! Wire protocol: one TCP connection per request, client writes the whole
//! request then shuts down its write half, server reads to EOF, replies
//! with exactly one line, closes. Three request kinds, all plain text (no
//! JSON — the payload shapes are too simple to justify it):
//! - `PING\n\n` — a reachability/health check (`shiki capture --check`);
//!   answered directly by the accept-loop thread from the `enabled` flag
//!   it already holds, without touching `App` at all. Reply is always
//!   `OK enabled`/`OK disabled`, never `ERR` — being asked "are you there"
//!   isn't itself an error state.
//! - `UNDO\n\n` — reverses the single most recent capture (`shiki capture
//!   --undo`), whichever kind it was and however it was made (daemon or
//!   the CLI's own standalone fallback both write to the same
//!   `LastCapture` record).
//! - `CAPTURE\n<headers>\n\n<body>` — `<headers>` is zero or more
//!   `key=value` lines (`daily=1`, `tags=a,b,c`, `notebook=work`,
//!   `folder=work/meetings`, `template=meeting`, `url=https://...`,
//!   `title=Page Title`, `source=browser|clip|voice|pipe|rofi`), ending at
//!   the first blank line; `<body>` is the raw capture text, not line-
//!   delimited (it may itself contain blank lines/newlines) since it's
//!   simply "everything after the header block's terminating blank line."

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use shiki_config::{Config, LastCapture};

use crate::app::App;

/// What the listener thread asks the main thread to do — bundled with a
/// one-shot reply channel, since the listener thread is still holding the
/// client connection open, waiting to write the result back, and the
/// existing git-sync/self-update background-thread channels are fire-and-
/// forget with no "reply to this specific caller" concept.
pub struct CaptureRequest {
    pub kind: RequestKind,
    pub reply_tx: Sender<CaptureReply>,
}

pub enum RequestKind {
    Capture {
        text: String,
        daily: bool,
        tags: Vec<String>,
        /// Explicit `-n <notebook>` from the client, if any — takes
        /// priority over content-prefix routing and `default_notebook`.
        notebook: Option<String>,
        /// `--folder <path>` from the client, if any — a note is created
        /// inside this subfolder instead of the notebook's root. Ignored
        /// when `daily` is set (a daily note's path is always fixed).
        folder: Option<String>,
        /// Template name (`--template <name>`), if any. Ignored for `--daily`.
        template: Option<String>,
        /// Source URL, if any — appended as `Source: [title](url)` when present.
        url: Option<String>,
        /// Page title for `url`, if any.
        title: Option<String>,
        /// Origin marker (`browser|clip|voice|pipe|rofi`) — for logging only.
        source: Option<String>,
    },
    Undo,
}

pub enum CaptureReply {
    Ok(PathBuf),
    Err(String),
}

/// Held by `App` once the daemon has been spawned at least once this
/// session. The listener thread is never torn down — see
/// `App::set_capture_daemon_enabled` for why toggling this off just flips
/// the flag instead.
pub struct CaptureDaemonHandle {
    pub enabled: Arc<AtomicBool>,
}

/// A parsed request off the wire — see the module doc comment for the
/// exact text format. Pure/testable independent of any real socket.
enum ParsedRequest {
    Ping,
    Undo,
    Capture {
        daily: bool,
        tags: Vec<String>,
        notebook: Option<String>,
        folder: Option<String>,
        template: Option<String>,
        url: Option<String>,
        title: Option<String>,
        source: Option<String>,
        text: String,
    },
    /// Anything not starting with a recognized command word.
    Invalid,
}

/// Headers end at the first blank line; everything after it is the body
/// verbatim (including any further blank lines inside it) — so this can't
/// just be `raw.lines()`, which would also split the body on its own
/// internal newlines.
fn parse_request(raw: &str) -> ParsedRequest {
    let (header_block, body) = raw
        .split_once("\n\n")
        .unwrap_or((raw.trim_end_matches('\n'), ""));
    let mut lines = header_block.lines();
    match lines.next().unwrap_or("").trim() {
        "PING" => ParsedRequest::Ping,
        "UNDO" => ParsedRequest::Undo,
        "CAPTURE" => {
            let mut daily = false;
            let mut tags = Vec::new();
            let mut notebook = None;
            let mut folder = None;
            let mut template = None;
            let mut url = None;
            let mut title = None;
            let mut source = None;
            for line in lines {
                if let Some(v) = line.strip_prefix("daily=") {
                    daily = matches!(v.trim(), "1" | "true");
                } else if let Some(v) = line.strip_prefix("tags=") {
                    tags = v
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                } else if let Some(v) = line.strip_prefix("notebook=") {
                    notebook = Some(v.trim().to_string()).filter(|s| !s.is_empty());
                } else if let Some(v) = line.strip_prefix("folder=") {
                    folder = Some(v.trim().to_string()).filter(|s| !s.is_empty());
                } else if let Some(v) = line.strip_prefix("template=") {
                    template = Some(v.trim().to_string()).filter(|s| !s.is_empty());
                } else if let Some(v) = line.strip_prefix("url=") {
                    url = Some(v.trim().to_string()).filter(|s| !s.is_empty());
                } else if let Some(v) = line.strip_prefix("title=") {
                    title = Some(v.trim().to_string()).filter(|s| !s.is_empty());
                } else if let Some(v) = line.strip_prefix("source=") {
                    source = Some(v.trim().to_string()).filter(|s| !s.is_empty());
                }
            }
            ParsedRequest::Capture {
                daily,
                tags,
                notebook,
                folder,
                template,
                url,
                title,
                source,
                text: body.to_string(),
            }
        }
        _ => ParsedRequest::Invalid,
    }
}

/// Binds an ephemeral loopback port, records it, and spawns the accept-loop
/// thread. Returns as soon as the port is known and written to disk, so a
/// capture issued moments later reliably finds it. Shared by the TUI's
/// in-app daemon and the standalone `shiki daemon` (headless) process.
pub fn spawn_capture_daemon(
    capture_tx: Sender<CaptureRequest>,
) -> anyhow::Result<CaptureDaemonHandle> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    write_port_file(port)?;

    let enabled = Arc::new(AtomicBool::new(true));
    let thread_enabled = Arc::clone(&enabled);
    std::thread::spawn(move || accept_loop(listener, capture_tx, thread_enabled));

    Ok(CaptureDaemonHandle { enabled })
}

fn write_port_file(port: u16) -> anyhow::Result<()> {
    let path = Config::default_capture_port_path()?;
    let pid = std::process::id();
    std::fs::write(&path, format!("{port} {pid}\n"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Best-effort: a stricter mode just means only this user's other
        // processes can read the port number, not a functional requirement
        // (the port itself has no auth beyond "connects from localhost").
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Whether `pid` is still a live process.
///
/// On Unix, `kill(pid,0)` with signal 0 does not send a signal but
/// performs the usual permission checks — 0 means the process exists
/// (or we lack permission to signal it, which still means it exists),
/// `ESRCH` means no such process. On Windows, where `libc::kill` is not
/// available and `kill -0` semantics differ, we conservatively return
/// `true` (treat the port file as not stale) — stale detection there
/// falls back to the TCP connect timeout, which already cleans up the
/// user-visible symptom, just not the file itself immediately.
#[allow(dead_code)]
fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        if pid == 0 || pid > i32::MAX as u32 {
            return false;
        }
        // SAFETY: `kill` is async-signal-safe and we pass a valid pid + 0.
        let ret = unsafe { libc::kill(pid as i32, 0) };
        if ret == 0 {
            return true;
        }
        // `kill` failed — check errno. ESRCH = no such process → stale.
        // EPERM = process exists but we can't signal it → alive.
        let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        // libc::ESRCH is 3 on Linux, 3 on macOS — use raw value to avoid
        // needing `errno` crate, but prefer `libc::ESRCH` when available.
        #[allow(unused_variables)]
        let esrch = {
            #[cfg(any(target_os = "linux", target_os = "android"))]
            {
                3
            }
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            {
                3
            }
            #[cfg(not(any(
                target_os = "linux",
                target_os = "android",
                target_os = "macos",
                target_os = "ios"
            )))]
            {
                3
            }
        };
        errno != esrch
    }
    #[cfg(windows)]
    {
        let _ = pid;
        true
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        true
    }
}

fn accept_loop(
    listener: TcpListener,
    capture_tx: Sender<CaptureRequest>,
    enabled: Arc<AtomicBool>,
) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut raw = String::new();
        if stream.read_to_string(&mut raw).is_err() {
            let _ = stream.write_all(b"ERR could not read request\n");
            continue;
        }

        match parse_request(&raw) {
            ParsedRequest::Ping => {
                // Answered directly, no round trip through `App` — a health
                // check should be as fast as the connection itself, and
                // doesn't need anything `App` holds beyond this flag, which
                // the accept-loop thread already has its own clone of.
                let status = if enabled.load(Ordering::Relaxed) {
                    "enabled"
                } else {
                    "disabled"
                };
                let _ = stream.write_all(format!("OK {status}\n").as_bytes());
            }
            ParsedRequest::Undo => {
                if !enabled.load(Ordering::Relaxed) {
                    let _ = stream.write_all(disabled_response().as_bytes());
                    continue;
                }
                dispatch(&mut stream, &capture_tx, RequestKind::Undo);
            }
            ParsedRequest::Capture {
                daily,
                tags,
                notebook,
                folder,
                template,
                url,
                title,
                source,
                text,
            } => {
                if !enabled.load(Ordering::Relaxed) {
                    let _ = stream.write_all(disabled_response().as_bytes());
                    continue;
                }
                dispatch(
                    &mut stream,
                    &capture_tx,
                    RequestKind::Capture {
                        text,
                        daily,
                        tags,
                        notebook,
                        folder,
                        template,
                        url,
                        title,
                        source,
                    },
                );
            }
            ParsedRequest::Invalid => {
                let _ = stream.write_all(b"ERR unrecognized request\n");
            }
        }
    }
}

/// Sends `kind` to the main thread and writes its reply back to `stream` —
/// shared by the `CAPTURE`/`UNDO` branches of `accept_loop`, which differ
/// only in what they send, not in how the round trip works.
fn dispatch(
    stream: &mut std::net::TcpStream,
    capture_tx: &Sender<CaptureRequest>,
    kind: RequestKind,
) {
    let (reply_tx, reply_rx) = std::sync::mpsc::channel();
    if capture_tx.send(CaptureRequest { kind, reply_tx }).is_err() {
        // Main thread is gone (process shutting down) — nothing left to
        // reply to correctly, but still close the connection cleanly.
        let _ = stream.write_all(b"ERR shiki is shutting down\n");
        return;
    }
    let response = match reply_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(reply) => response_line(&reply),
        Err(_) => "ERR timed out waiting for shiki to respond\n".to_string(),
    };
    let _ = stream.write_all(response.as_bytes());
}

fn disabled_response() -> String {
    "ERR daemon disabled\n".to_string()
}

fn response_line(reply: &CaptureReply) -> String {
    match reply {
        CaptureReply::Ok(path) => format!("OK {}\n", path.display()),
        CaptureReply::Err(msg) => format!("ERR {msg}\n"),
    }
}

/// Runs on the main thread (`App::poll_capture_channel`) — dispatches to
/// `perform_capture`/`perform_undo` depending on what the listener thread
/// asked for.
pub(crate) fn handle_request(app: &mut App, request: &RequestKind) -> CaptureReply {
    match request {
        RequestKind::Capture {
            text,
            daily,
            tags,
            notebook,
            folder,
            template,
            url,
            title,
            source,
        } => perform_capture(
            app,
            text,
            *daily,
            tags,
            notebook.as_deref(),
            folder.as_deref(),
            template.as_deref(),
            url.as_deref(),
            title.as_deref(),
            source.as_deref(),
        ),
        RequestKind::Undo => perform_undo(app),
    }
}

/// Resolves which notebook a capture with no explicit `-n` targets:
/// content-prefix routing (`"work: call Ana"`) first, then
/// `general.default_notebook`. An explicit override always wins outright
/// and never reaches this function at all — see the call site.
fn resolve_notebook_and_text<'a>(
    text: &'a str,
    explicit: Option<&str>,
    default_notebook: &str,
    existing_notebooks: &[String],
) -> (String, &'a str) {
    if let Some(name) = explicit {
        return (name.to_string(), text);
    }
    shiki_core::notebook::route_by_prefix(text, existing_notebooks)
        .unwrap_or_else(|| (default_notebook.to_string(), text))
}

/// Reuses the exact note-creation/daily-note path every other capture
/// route in this codebase already uses — this is what makes cache/panel
/// refresh and auto-sync counting "just work" for a capture that arrived
/// from an external process. Every outcome is also funneled through
/// `App::set_status`, so a capture that happened while the TUI was
/// unattended still leaves a trace in the logs modal (`leader` then `l`),
/// not just a footer message nobody was there to read.
fn with_source(text: &str, url: Option<&str>, title: Option<&str>) -> String {
    let Some(url) = url else {
        return text.to_string();
    };
    if url.is_empty() || text.contains(url) {
        return text.to_string();
    }
    if let Some(title) = title.filter(|t| !t.is_empty()) {
        format!("{text}\n\nSource: [{title}]({url})")
    } else {
        format!("{text}\n\nSource: {url}")
    }
}

#[allow(clippy::too_many_arguments)]
fn perform_capture(
    app: &mut App,
    text: &str,
    daily: bool,
    tags: &[String],
    explicit_notebook: Option<&str>,
    folder: Option<&str>,
    template: Option<&str>,
    url: Option<&str>,
    title: Option<&str>,
    source: Option<&str>,
) -> CaptureReply {
    let text = text.trim();
    if text.is_empty() {
        let reply = CaptureReply::Err("empty capture text".into());
        app.set_status("capture failed: empty text".into());
        return reply;
    }
    // Unify source-URL appending here so every client (CLI, native host,
    // rofi/waybar pipes) gets the same provenance footer, not just the
    // browser extension's own `handle_capture` path.
    let text_owned = with_source(text, url, title);
    let text = text_owned.as_str();

    let existing_notebooks: Vec<String> = app.notebooks.iter().map(|nb| nb.name.clone()).collect();
    let (name, text) = resolve_notebook_and_text(
        text,
        explicit_notebook,
        &app.config.general.default_notebook.clone(),
        &existing_notebooks,
    );

    let nb = match app.store.get(&name) {
        Ok(nb) => nb,
        Err(_) => match app.store.create(&name) {
            Ok(nb) => nb,
            Err(e) => {
                let msg = format!("could not create notebook '{name}': {e}");
                app.set_status(format!("capture failed: {msg}"));
                return CaptureReply::Err(msg);
            }
        },
    };

    let encrypted = app.config.encrypt_for(&name);
    let crypto = app.resolved_notebook_crypto(&name);
    if encrypted && crypto.is_none() {
        let msg = format!(
            "locked: notebook '{name}' is encrypted and locked in this session — \
             unlock it in the TUI or run `shiki capture` from a terminal instead"
        );
        app.set_status(format!("capture failed: notebook '{name}' is locked"));
        return CaptureReply::Err(msg);
    }
    let nb = nb.with_crypto(crypto);

    let result = if daily {
        capture_into_daily(app, &nb, text)
    } else if let Some(tmpl) = template.filter(|s| !s.is_empty()) {
        capture_into_templated(&nb, text, tags, folder, tmpl, source)
    } else {
        capture_into_new_note(&nb, text, tags, folder)
    };

    let (path, record) = match result {
        Ok(outcome) => outcome,
        Err(e) => {
            let msg = format!("could not save capture: {e}");
            app.set_status(format!("capture failed: {msg}"));
            return CaptureReply::Err(msg);
        }
    };
    if let Ok(record_path) = Config::default_last_capture_path() {
        let _ = record.save(&record_path);
    }

    // A daily-note append always lands at the notebook's root, regardless
    // of `folder` (which only applies to a brand-new note); only refresh
    // the live panel when that's genuinely what's on screen, so a capture
    // elsewhere doesn't stomp on whatever the user's currently browsing.
    let target_relative = if daily { "" } else { folder.unwrap_or("") };
    let viewing_target = app.selected_notebook().is_some_and(|sel| sel.name == name)
        && app.notes_path.join("/") == target_relative;
    if viewing_target {
        app.refresh_notes_preserve_selection();
    }
    app.note_changed(&name);
    app.set_status(format!("captured: {}", path.display()));

    CaptureReply::Ok(path)
}

fn capture_into_templated(
    nb: &shiki_core::Notebook,
    text: &str,
    tags: &[String],
    folder: Option<&str>,
    template_name: &str,
    _source: Option<&str>,
) -> shiki_core::Result<(PathBuf, LastCapture)> {
    let templates_dir = Config::default_templates_dir().unwrap_or_default();
    let tmpl = shiki_core::Template::load(&templates_dir, template_name)
        .map_err(|_| shiki_core::Error::TemplateNotFound(template_name.to_string()))?;
    let title = format!("Capture {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
    let mut vars = std::collections::HashMap::new();
    vars.insert("title", title.clone());
    vars.insert("date", chrono::Local::now().format("%Y-%m-%d").to_string());
    vars.insert("body", text.to_string());
    // Also expose notebook name for templates that want it
    vars.insert("notebook", nb.name.clone());
    let rendered = tmpl.render(&vars);
    let body = if rendered.contains(text) {
        rendered
    } else {
        format!("{rendered}\n{text}\n")
    };
    let mut note = match folder {
        Some(folder) if !folder.is_empty() => {
            let relative = shiki_core::notebook::validate_relative_path(folder)?;
            nb.create_note_in(&relative, &title, body)?
        }
        _ => nb.create_note(&title, body)?,
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

fn capture_into_new_note(
    nb: &shiki_core::Notebook,
    text: &str,
    tags: &[String],
    folder: Option<&str>,
) -> shiki_core::Result<(PathBuf, LastCapture)> {
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
    app: &App,
    nb: &shiki_core::Notebook,
    text: &str,
) -> shiki_core::Result<(PathBuf, LastCapture)> {
    let today = chrono::Local::now().date_naive();
    let templates_dir = Config::default_templates_dir().unwrap_or_default();
    let agenda = if app.config.general.daily_agenda {
        app.store
            .all_notes()
            .ok()
            .and_then(|pool| shiki_core::tasks::agenda_section(&pool, today))
    } else {
        None
    };
    let mut note = shiki_core::daily::create_or_open(
        nb,
        today,
        &templates_dir,
        &app.config.general.daily_template,
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

/// Reverses the single most recent capture — see `LastCapture`'s own doc
/// comment for why this is a one-slot record, not a stack.
fn perform_undo(app: &mut App) -> CaptureReply {
    let record_path = match Config::default_last_capture_path() {
        Ok(p) => p,
        Err(e) => return CaptureReply::Err(format!("could not resolve state path: {e}")),
    };
    let Some(record) = LastCapture::load(&record_path) else {
        return CaptureReply::Err("nothing to undo".into());
    };

    let (notebook, path, outcome) = match &record {
        LastCapture::Note { notebook, path } => {
            let outcome = undo_note(app, notebook, path);
            (notebook.clone(), path.clone(), outcome)
        }
        LastCapture::DailyAppend {
            notebook,
            path,
            appended,
        } => {
            let outcome = undo_daily_append(app, notebook, path, appended);
            (notebook.clone(), path.clone(), outcome)
        }
    };

    let path = match outcome {
        Ok(()) => path,
        Err(e) => {
            app.set_status(format!("undo failed: {e}"));
            return CaptureReply::Err(e);
        }
    };

    LastCapture::clear(&record_path);
    let viewing_target = app
        .selected_notebook()
        .is_some_and(|sel| sel.name == notebook);
    if viewing_target {
        app.refresh_notes_preserve_selection();
    }
    app.note_changed(&notebook);
    app.set_status(format!("undone: {path}"));
    CaptureReply::Ok(PathBuf::from(path))
}

fn undo_note(app: &App, notebook: &str, path: &str) -> Result<(), String> {
    let Some(trash_root) = app.trash_root.as_ref() else {
        return Err("trash directory unavailable".into());
    };
    let trash_dir = trash_root.join(notebook);
    let suffix = chrono::Local::now().format("%Y%m%d%H%M%S%3f").to_string();
    shiki_core::trash::move_to_trash(std::path::Path::new(path), &trash_dir, &suffix)
        .map(|_| ())
        .map_err(|e| format!("could not move '{path}' to trash: {e}"))
}

fn undo_daily_append(app: &App, notebook: &str, path: &str, appended: &str) -> Result<(), String> {
    let encrypted = app.config.encrypt_for(notebook);
    let crypto = app.resolved_notebook_crypto(notebook);
    if encrypted && crypto.is_none() {
        return Err(format!(
            "locked: notebook '{notebook}' is encrypted and locked in this session"
        ));
    }
    let mut note = shiki_core::Note::from_file_in_notebook_with_crypto(
        std::path::Path::new(path),
        notebook,
        crypto.as_ref(),
    )
    .map_err(|e| format!("could not read '{path}': {e}"))?;
    if !note.body.ends_with(appended) {
        return Err("the daily note has changed since that capture — not undoing".into());
    }
    let new_len = note.body.len() - appended.len();
    note.body.truncate(new_len);
    note.save_with_crypto(crypto.as_ref())
        .map_err(|e| format!("could not save '{path}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_line_formats_ok_and_err() {
        assert_eq!(
            response_line(&CaptureReply::Ok(PathBuf::from("/tmp/note.md"))),
            "OK /tmp/note.md\n"
        );
        assert_eq!(
            response_line(&CaptureReply::Err("oops".into())),
            "ERR oops\n"
        );
    }

    #[test]
    fn disabled_response_is_a_clear_err_line() {
        assert_eq!(disabled_response(), "ERR daemon disabled\n");
    }

    #[test]
    fn parse_request_recognizes_ping_and_undo() {
        assert!(matches!(parse_request("PING\n\n"), ParsedRequest::Ping));
        assert!(matches!(parse_request("PING"), ParsedRequest::Ping));
        assert!(matches!(parse_request("UNDO\n\n"), ParsedRequest::Undo));
        assert!(matches!(parse_request("UNDO"), ParsedRequest::Undo));
    }

    #[test]
    fn parse_request_parses_plain_capture_with_no_headers() {
        match parse_request("CAPTURE\n\nbuy milk") {
            ParsedRequest::Capture {
                daily,
                tags,
                notebook,
                folder,
                template,
                url,
                title,
                source,
                text,
            } => {
                assert!(!daily);
                assert!(tags.is_empty());
                assert!(notebook.is_none());
                assert!(folder.is_none());
                assert!(template.is_none());
                assert!(url.is_none());
                assert!(title.is_none());
                assert!(source.is_none());
                assert_eq!(text, "buy milk");
            }
            _ => panic!("expected Capture"),
        }
    }

    #[test]
    fn parse_request_parses_all_headers() {
        match parse_request(
            "CAPTURE\ndaily=1\ntags=work,idea\nnotebook=work\nfolder=work/meetings\ntemplate=meeting\nurl=https://example.com\ntitle=Example\nsource=browser\n\nbuy milk",
        ) {
            ParsedRequest::Capture {
                daily,
                tags,
                notebook,
                folder,
                template,
                url,
                title,
                source,
                text,
            } => {
                assert!(daily);
                assert_eq!(tags, vec!["work".to_string(), "idea".to_string()]);
                assert_eq!(notebook.as_deref(), Some("work"));
                assert_eq!(folder.as_deref(), Some("work/meetings"));
                assert_eq!(template.as_deref(), Some("meeting"));
                assert_eq!(url.as_deref(), Some("https://example.com"));
                assert_eq!(title.as_deref(), Some("Example"));
                assert_eq!(source.as_deref(), Some("browser"));
                assert_eq!(text, "buy milk");
            }
            _ => panic!("expected Capture"),
        }
    }

    #[test]
    fn with_source_appends_url_and_title() {
        assert_eq!(
            with_source("hello", Some("https://x.com"), Some("X")),
            "hello\n\nSource: [X](https://x.com)"
        );
        assert_eq!(
            with_source("hello", Some("https://x.com"), None),
            "hello\n\nSource: https://x.com"
        );
        // Already contains url -> no duplicate
        assert_eq!(
            with_source("hello https://x.com world", Some("https://x.com"), None),
            "hello https://x.com world"
        );
        assert_eq!(with_source("hello", None, None), "hello");
    }

    #[test]
    fn parse_request_body_may_contain_blank_lines() {
        match parse_request("CAPTURE\n\nline one\n\nline two") {
            ParsedRequest::Capture { text, .. } => {
                assert_eq!(text, "line one\n\nline two");
            }
            _ => panic!("expected Capture"),
        }
    }

    #[test]
    fn parse_request_rejects_unrecognized_command() {
        assert!(matches!(
            parse_request("GARBAGE\n\nsomething"),
            ParsedRequest::Invalid
        ));
        assert!(matches!(parse_request(""), ParsedRequest::Invalid));
    }

    #[test]
    fn resolve_notebook_and_text_prefers_explicit_override() {
        let existing = vec!["work".to_string()];
        let (name, text) =
            resolve_notebook_and_text("work: call Ana", Some("personal"), "personal", &existing);
        assert_eq!(name, "personal");
        assert_eq!(text, "work: call Ana");
    }

    #[test]
    fn resolve_notebook_and_text_routes_by_prefix_when_no_override() {
        let existing = vec!["work".to_string()];
        let (name, text) = resolve_notebook_and_text("work: call Ana", None, "personal", &existing);
        assert_eq!(name, "work");
        assert_eq!(text, "call Ana");
    }

    #[test]
    fn resolve_notebook_and_text_falls_back_to_default() {
        let existing = vec!["work".to_string()];
        let (name, text) = resolve_notebook_and_text("just an idea", None, "personal", &existing);
        assert_eq!(name, "personal");
        assert_eq!(text, "just an idea");
    }
}

//! Quick-capture daemon (`general.enable_capture_daemon`) — lets an
//! external `shiki capture "text"` invocation (`shiki-cli/src/commands/
//! capture.rs`) land in an already-running TUI instance live, instead of
//! only writing to disk unnoticed. Off by default; `shiki capture` itself
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
//! Wire protocol is deliberately the simplest thing that works: one TCP
//! connection per capture, client writes the raw capture text (not line-
//! delimited — the text itself may contain newlines) then shuts down its
//! write half, server reads to EOF, replies with exactly one line (`OK
//! <path>` or `ERR <message>`), and closes.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

use shiki_config::Config;

use crate::app::App;

/// Sent from the listener thread to the main thread over `App::capture_tx`/
/// `capture_rx` — the listener thread never touches `App`/`NotebookStore`
/// itself, it's a dumb pipe; all real work happens on the main thread
/// inside `perform_capture`, reusing the exact same note-creation/refresh
/// path every other capture route already uses.
pub(crate) struct CaptureRequest {
    pub text: String,
    /// One-shot reply channel bundled per-request, since the existing
    /// git-sync/self-update background-thread channels are fire-and-forget
    /// with no "reply to this specific caller" concept — a capture needs
    /// one, since the listener thread is still holding the client
    /// connection open, waiting to write the result back.
    pub reply_tx: Sender<CaptureReply>,
}

pub(crate) enum CaptureReply {
    Ok(PathBuf),
    Err(String),
}

/// Held by `App` once the daemon has been spawned at least once this
/// session. The listener thread is never torn down — see
/// `App::set_capture_daemon_enabled` for why toggling this off just flips
/// the flag instead.
pub(crate) struct CaptureDaemonHandle {
    pub enabled: Arc<AtomicBool>,
}

/// Binds an ephemeral loopback port, records it, and spawns the accept-loop
/// thread. Returns as soon as the port is known and written to disk, so a
/// capture issued moments later reliably finds it.
pub(crate) fn spawn_capture_daemon(
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
    std::fs::write(&path, port.to_string())?;
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

fn accept_loop(
    listener: TcpListener,
    capture_tx: Sender<CaptureRequest>,
    enabled: Arc<AtomicBool>,
) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        if !enabled.load(Ordering::Relaxed) {
            let _ = stream.write_all(disabled_response().as_bytes());
            continue;
        }
        let mut text = String::new();
        if stream.read_to_string(&mut text).is_err() {
            let _ = stream.write_all(b"ERR could not read request\n");
            continue;
        }

        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        if capture_tx.send(CaptureRequest { text, reply_tx }).is_err() {
            // Main thread is gone (process shutting down) — nothing left
            // to reply to correctly, but still close the connection cleanly.
            let _ = stream.write_all(b"ERR shiki is shutting down\n");
            continue;
        }
        let response = match reply_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(reply) => response_line(&reply),
            Err(_) => "ERR timed out waiting for shiki to respond\n".to_string(),
        };
        let _ = stream.write_all(response.as_bytes());
    }
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

/// Runs on the main thread (`App::poll_capture_channel`), reusing the exact
/// note-creation path every other capture route in this codebase already
/// uses — this is what makes cache/panel refresh and auto-sync counting
/// "just work" for a capture that arrived from an external process.
pub(crate) fn perform_capture(app: &mut App, text: &str) -> CaptureReply {
    let text = text.trim();
    if text.is_empty() {
        return CaptureReply::Err("empty capture text".into());
    }

    let name = app.config.general.default_notebook.clone();
    let nb = match app.store.get(&name) {
        Ok(nb) => nb,
        Err(_) => match app.store.create(&name) {
            Ok(nb) => nb,
            Err(e) => return CaptureReply::Err(format!("could not create notebook '{name}': {e}")),
        },
    };

    let encrypted = app.config.encrypt_for(&name);
    let crypto = app.resolved_notebook_crypto(&name);
    if encrypted && crypto.is_none() {
        return CaptureReply::Err(format!(
            "locked: notebook '{name}' is encrypted and locked in this session — \
             unlock it in the TUI or run `shiki capture` from a terminal instead"
        ));
    }
    let nb = nb.with_crypto(crypto);

    let title = format!("Capture {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
    let note = match nb.create_note(&title, text) {
        Ok(note) => note,
        Err(e) => return CaptureReply::Err(format!("could not create note: {e}")),
    };

    // Captures always land at the notebook's root — only refresh the live
    // panel when that's genuinely what's on screen, so a capture into a
    // different notebook (or the same notebook but a different folder)
    // doesn't stomp on whatever the user's currently browsing.
    let viewing_target_root =
        app.selected_notebook().is_some_and(|sel| sel.name == name) && app.notes_path.is_empty();
    if viewing_target_root {
        app.refresh_notes_preserve_selection();
    }
    app.note_changed(&name);

    CaptureReply::Ok(note.path)
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
}

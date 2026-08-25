//! `shiki daemon` — the capture daemon run standalone, headless, with no
//! TUI. It binds the exact same loopback port + wire protocol as the
//! in-TUI daemon (`shiki-tui/src/capture.rs`), so `shiki capture` can't
//! tell the difference: captures land on disk, `--undo` works, `--check`
//! answers — and every request is recorded to the shared `shiki.log`
//! (the same log the TUI's `leader+l` modal reads) so a capture that
//! happened with nobody looking still leaves a trace.
//!
//! Registered as a systemd user service it makes shiki's capture path
//! available even with no TUI open at all:
//!
//! ```ini
//! # ~/.config/systemd/user/shiki-daemon.service
//! [Service]
//! ExecStart=%h/.cargo/bin/shiki daemon
//! Restart=always
//! ```
//!
//! The transport lives in `shiki-tui` (which `shiki-cli` already depends
//! on); the headless handler here reuses the exact direct-disk write
//! helpers `shiki capture`'s fallback uses (`perform_direct_capture` /
//! `perform_direct_undo`), with `interactive = false` so an encrypted
//! locked notebook replies `locked:` instead of prompting a background
//! process for a passphrase it can't type back.

use std::io::Write;

use anyhow::Result;
use shiki_config::Config;
use shiki_core::NotebookStore;
use shiki_tui::capture::{spawn_capture_daemon, CaptureReply, CaptureRequest, RequestKind};

use super::capture::{perform_direct_capture, perform_direct_undo};

pub fn run(store: &NotebookStore, config: &Config) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _handle = spawn_capture_daemon(tx)?;

    let port = Config::default_capture_port_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "?".to_string());
    eprintln!("shiki daemon: capture daemon listening on 127.0.0.1:{port} \u{2014} Ctrl+C to stop");

    // `rx.recv()` blocks; it only returns `Err` if every sender is gone,
    // which can't happen while the accept-loop thread owns its clone. So
    // this loop runs until the process is killed (systemd restarts it).
    for request in rx {
        let CaptureRequest { kind, reply_tx } = request;
        let reply = handle_request(kind, store, config);
        let _ = reply_tx.send(reply);
    }
    Ok(())
}

fn handle_request(kind: RequestKind, store: &NotebookStore, config: &Config) -> CaptureReply {
    match kind {
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
        } => {
            let src = source.as_deref().unwrap_or("daemon");
            match perform_direct_capture(
                store, config, notebook, &text, &tags, daily, folder, template, url, title, false,
            ) {
                Ok((path, _)) => {
                    log_line(&format!("captured ({src}): {}", path.display()));
                    CaptureReply::Ok(path)
                }
                Err(e) => {
                    log_line(&format!("capture failed: {e}"));
                    CaptureReply::Err(e.to_string())
                }
            }
        }
        RequestKind::Undo => match perform_direct_undo(false) {
            Ok(path) => {
                log_line(&format!("undone: {}", path.display()));
                CaptureReply::Ok(path)
            }
            Err(e) => {
                log_line(&format!("undo failed: {e}"));
                CaptureReply::Err(e.to_string())
            }
        },
    }
}

/// Appends `msg` to the shared `shiki.log` — the same file the TUI's logs
/// modal (`leader+l`) and status history read, so a headless capture is
/// never invisible to a later TUI session. Best-effort: a logging failure
/// must never take a capture down.
fn log_line(msg: &str) {
    let Ok(path) = Config::default_log_path() else {
        return;
    };
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{ts} [daemon] {msg}");
    }
}

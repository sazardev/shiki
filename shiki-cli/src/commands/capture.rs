use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use anyhow::{Context, Result};
use shiki_config::Config;
use shiki_core::NotebookStore;

use super::unlock_if_encrypted;

/// A parsed one-line reply from the capture daemon (`shiki-tui/src/
/// capture.rs`) — `OK <path>` or `ERR <message>`.
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

/// Captures a quick note. Tries a running TUI's capture daemon first — if
/// one answers, its response is authoritative (an explicit `locked: ...`
/// refusal is reported as an error rather than silently bypassed by
/// falling back to a direct write, which could otherwise duplicate work or
/// confuse the user about which passphrase prompt they're about to hit).
/// Anything else (no daemon running, the daemon replying "disabled", a
/// connection timeout) falls back to writing straight to disk — the same
/// path `shiki new` already uses, including the interactive passphrase
/// prompt for an encrypted notebook, since a human is sitting at this
/// terminal in that case.
pub fn run(store: &NotebookStore, config: &Config, notebook: &str, text: &str) -> Result<()> {
    match try_daemon(text) {
        Some(DaemonResponse::Ok(path)) => {
            println!("captured (daemon): {path}");
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

    let nb = match store.get(notebook) {
        Ok(nb) => nb,
        Err(_) => store
            .create(notebook)
            .with_context(|| format!("could not create notebook '{notebook}'"))?,
    };
    let nb = unlock_if_encrypted(config, nb)?;
    let title = format!("Capture {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
    let note = nb.create_note(&title, text)?;
    println!("captured: {}", note.path.display());
    Ok(())
}

/// `None` means "nothing to connect to" (no port file, unparsable
/// content, connection refused/timed out) — the caller treats that
/// identically to "daemon disabled". `Some` means the daemon actually
/// answered and its response must be respected.
fn try_daemon(text: &str) -> Option<DaemonResponse> {
    let port_path = Config::default_capture_port_path().ok()?;
    let contents = std::fs::read_to_string(port_path).ok()?;
    let port = parse_port_file(&contents)?;

    let mut stream =
        TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300))
            .ok()?;
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    stream.write_all(text.as_bytes()).ok()?;
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
}

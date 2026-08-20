//! Native messaging host for Shiki browser extension.
//!
//! Chrome/Firefox -> stdin (4-byte LE length + JSON) -> this binary -> shiki
//! - Reads notebook list / folders via `shiki-config` + `shiki-core`
//! - For `capture`, tries the TUI daemon first (TCP 127.0.0.1 + port file),
//!   falling back to direct disk write — same logic as `shiki-cli/src/commands/capture.rs`.
//!
//! Install manifest: `browser-extension/host/com.shiki.native.json`
//! See `browser-extension/README.md` for install instructions.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use shiki_config::{Config, LastCapture};
use shiki_core::NotebookStore;

// ── Native messaging framing ───────────────────────────────────────────────

fn read_message() -> anyhow::Result<Option<serde_json::Value>> {
    let mut len_buf = [0u8; 4];
    match std::io::stdin().read_exact(&mut len_buf) {
        Ok(()) => {},
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 {
        return Ok(None);
    }
    // Guard against insane size (browser caps at 1MB per spec)
    if len > 1024 * 1024 {
        anyhow::bail!("message too large: {len} bytes");
    }
    let mut buf = vec![0u8; len];
    std::io::stdin().read_exact(&mut buf)?;
    let val = serde_json::from_slice(&buf)?;
    Ok(Some(val))
}

fn write_message(val: &serde_json::Value) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(val)?;
    let len = (bytes.len() as u32).to_le_bytes();
    let mut out = std::io::stdout();
    out.write_all(&len)?;
    out.write_all(&bytes)?;
    out.flush()?;
    Ok(())
}

// ── Request / Response types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Request {
    action: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    notebook: Option<String>,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    daily: Option<bool>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    ok: bool,
    error: String,
}

#[derive(Debug, Serialize)]
struct PingResponse {
    ok: bool,
    daemon: DaemonStatus,
    config: Option<ConfigInfo>,
}

#[derive(Debug, Serialize)]
struct DaemonStatus {
    reachable: bool,
    enabled: bool,
}

#[derive(Debug, Serialize)]
struct ConfigInfo {
    default_notebook: String,
    data_dir: String,
    config_path: String,
}

#[derive(Debug, Serialize)]
struct NotebooksResponse {
    ok: bool,
    notebooks: Vec<NotebookInfo>,
    default_notebook: String,
}

#[derive(Debug, Serialize)]
struct NotebookInfo {
    name: String,
    path: String,
    is_encrypted: bool,
}

#[derive(Debug, Serialize)]
struct FoldersResponse {
    ok: bool,
    folders: Vec<String>,
    notebook: String,
}

#[derive(Debug, Serialize)]
struct CaptureResponse {
    ok: bool,
    path: String,
    via_daemon: bool,
    daily: bool,
    notebook: String,
}

// ── Daemon client (mirrors shiki-cli/src/commands/capture.rs) ─────────────

fn parse_port_file(contents: &str) -> Option<u16> {
    contents.trim().parse().ok()
}

#[derive(Debug)]
enum DaemonResponse {
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
    if let Some(nb) = notebook {
        req.push_str(&format!("notebook={nb}\n"));
    }
    if let Some(f) = folder {
        req.push_str(&format!("folder={f}\n"));
    }
    req.push('\n');
    req.push_str(text);
    req
}

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

fn check_daemon() -> DaemonStatus {
    match try_daemon("PING\n\n") {
        Some(DaemonResponse::Ok(status)) => DaemonStatus {
            reachable: true,
            enabled: status == "enabled",
        },
        _ => DaemonStatus {
            reachable: false,
            enabled: false,
        },
    }
}

// ── Config / Store helpers ─────────────────────────────────────────────────

fn load_config_and_store() -> anyhow::Result<(Config, NotebookStore)> {
    let config_path = Config::default_path()?;
    let config = Config::load_or_init(&config_path)?;
    let data_dir = match config.general.data_dir.as_ref() {
        Some(dir) => PathBuf::from(dir),
        None => Config::default_data_dir()?,
    };
    let store = NotebookStore::new_with_custom_paths(data_dir, config.notebook_custom_paths());
    Ok((config, store))
}

fn collect_folders(notebook: &shiki_core::Notebook, relative: &Path, out: &mut Vec<String>) {
    if let Ok((folders, _)) = notebook.list_dir(relative) {
        for folder in folders {
            let rel = if relative.as_os_str().is_empty() {
                PathBuf::from(&folder)
            } else {
                relative.join(&folder)
            };
            out.push(rel.display().to_string());
            collect_folders(notebook, &rel, out);
        }
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────────

fn handle_ping() -> anyhow::Result<serde_json::Value> {
    let daemon = check_daemon();
    let config_info = match load_config_and_store() {
        Ok((config, _)) => {
            let data_dir = config
                .general
                .data_dir
                .clone()
                .unwrap_or_else(|| {
                    Config::default_data_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                });
            let config_path = Config::default_path()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            Some(ConfigInfo {
                default_notebook: config.general.default_notebook.clone(),
                data_dir,
                config_path,
            })
        }
        Err(_) => None,
    };
    Ok(serde_json::to_value(PingResponse {
        ok: true,
        daemon,
        config: config_info,
    })?)
}

fn handle_list_notebooks() -> anyhow::Result<serde_json::Value> {
    let (config, store) = load_config_and_store()?;
    let notebooks = store.list().unwrap_or_default();
    let infos: Vec<NotebookInfo> = notebooks
        .into_iter()
        .map(|nb| {
            let is_encrypted = config.encrypt_for(&nb.name);
            NotebookInfo {
                name: nb.name.clone(),
                path: nb.path.display().to_string(),
                is_encrypted,
            }
        })
        .collect();
    Ok(serde_json::to_value(NotebooksResponse {
        ok: true,
        notebooks: infos,
        default_notebook: config.general.default_notebook.clone(),
    })?)
}

fn handle_list_folders(notebook_name: &str) -> anyhow::Result<serde_json::Value> {
    let (_, store) = load_config_and_store()?;
    let nb = store
        .get(notebook_name)
        .map_err(|e| anyhow::anyhow!("notebook '{notebook_name}' not found: {e}"))?;
    let mut folders = vec!["".to_string()]; // root
    let mut nested = Vec::new();
    collect_folders(&nb, Path::new(""), &mut nested);
    folders.extend(nested);
    folders.sort();
    Ok(serde_json::to_value(FoldersResponse {
        ok: true,
        folders,
        notebook: notebook_name.to_string(),
    })?)
}

fn handle_capture(req: &Request) -> anyhow::Result<serde_json::Value> {
    let text = req.text.clone().unwrap_or_default();
    // Allow caller to pass url/title for richer capture
    let mut full_text = text.clone();
    if let Some(url) = &req.url {
        if !url.is_empty() && !full_text.contains(url) {
            if full_text.is_empty() {
                full_text = url.clone();
            } else {
                // Append source URL as markdown link if title available
                if let Some(title) = &req.title {
                    if !title.is_empty() {
                        full_text = format!("{full_text}\n\nSource: [{title}]({url})");
                    } else {
                        full_text = format!("{full_text}\n\nSource: {url}");
                    }
                } else {
                    full_text = format!("{full_text}\n\nSource: {url}");
                }
            }
        }
    }
    let text = full_text.trim().to_string();
    if text.is_empty() {
        anyhow::bail!("empty capture text");
    }
    let daily = req.daily.unwrap_or(false);
    let tags = req.tags.clone().unwrap_or_default();
    let notebook = req.notebook.clone();
    let folder = req.folder.clone();

    // Try daemon first
    let daemon_req = build_capture_request(&text, daily, &tags, notebook.as_deref(), folder.as_deref());
    match try_daemon(&daemon_req) {
        Some(DaemonResponse::Ok(path)) => {
            // Record notebook name for response — try to extract from path or use default
            let nb_name = notebook.clone().unwrap_or_else(|| {
                // Fallback: try to infer from path
                Path::new(&path)
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "personal".to_string())
            });
            return Ok(serde_json::to_value(CaptureResponse {
                ok: true,
                path,
                via_daemon: true,
                daily,
                notebook: nb_name,
            })?);
        }
        Some(DaemonResponse::Err(msg)) if msg.starts_with("locked:") => {
            anyhow::bail!("{msg}");
        }
        _ => {
            // Fall through to direct write
        }
    }

    // Direct disk write (same as shiki-cli fallback)
    let (config, store) = load_config_and_store()?;
    let existing_notebooks: Vec<String> = store
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|nb| nb.name)
        .collect();
    let (target, text_ref) = match notebook.as_deref() {
        Some(name) => (name.to_string(), text.as_str()),
        None => shiki_core::notebook::route_by_prefix(&text, &existing_notebooks)
            .map(|(n, t)| (n, t))
            .unwrap_or((config.general.default_notebook.clone(), text.as_str())),
    };
    // Need owned string if route_by_prefix trimmed
    let text_owned;
    let text_final = if text_ref.as_ptr() == text.as_str().as_ptr() && text_ref.len() == text.len() {
        text_ref
    } else {
        text_owned = text_ref.to_string();
        &*Box::leak(text_owned.into_boxed_str())
    };

    let nb = match store.get(&target) {
        Ok(nb) => nb,
        Err(_) => store
            .create(&target)
            .map_err(|e| anyhow::anyhow!("could not create notebook '{target}': {e}"))?,
    };
    // Handle encryption — host can't prompt for passphrase, so fail clearly
    let encrypted = config.encrypt_for(&target);
    if encrypted {
        anyhow::bail!(
            "notebook '{target}' is encrypted and locked — unlock it in the TUI or run `shiki capture` from a terminal first"
        );
    }

    let (path, record) = if daily {
        capture_into_daily(&store, &config, &nb, text_final)?
    } else {
        capture_into_new_note(&nb, text_final, &tags, folder.as_deref())?
    };
    if let Ok(record_path) = Config::default_last_capture_path() {
        let _ = record.save(&record_path);
    }
    Ok(serde_json::to_value(CaptureResponse {
        ok: true,
        path: path.display().to_string(),
        via_daemon: false,
        daily,
        notebook: target,
    })?)
}

fn capture_into_new_note(
    nb: &shiki_core::Notebook,
    text: &str,
    tags: &[String],
    folder: Option<&str>,
) -> anyhow::Result<(PathBuf, LastCapture)> {
    let title = format!("Capture {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
    let mut note = match folder {
        Some(folder) if !folder.is_empty() => {
            let relative = shiki_core::notebook::validate_relative_path(folder)?;
            nb.create_note_in(&relative, &title, text)?
        }
        _ => nb.create_note(&title, text)?,
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
) -> anyhow::Result<(PathBuf, LastCapture)> {
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

fn dispatch(req: Request) -> serde_json::Value {
    let result: anyhow::Result<serde_json::Value> = match req.action.as_str() {
        "ping" | "check_daemon" => handle_ping(),
        "list_notebooks" => handle_list_notebooks(),
        "list_folders" => {
            let nb = req.notebook.clone().unwrap_or_default();
            if nb.is_empty() {
                Err(anyhow::anyhow!("notebook required for list_folders"))
            } else {
                handle_list_folders(&nb)
            }
        }
        "capture" => handle_capture(&req),
        other => Err(anyhow::anyhow!("unknown action: {other}")),
    };
    match result {
        Ok(v) => v,
        Err(e) => serde_json::to_value(ErrorResponse {
            ok: false,
            error: e.to_string(),
        })
        .unwrap_or_else(|_| serde_json::json!({"ok": false, "error": "serialization failed"})),
    }
}

fn main() -> anyhow::Result<()> {
    // Native messaging hosts must not write to stdout/stderr except framed JSON to stdout.
    // Redirect logs to a file if needed; keep stderr for crash diagnostics but avoid polluting stdout.
    loop {
        let msg = read_message()?;
        let Some(val) = msg else {
            break; // EOF — browser closed port
        };
        let req: Result<Request, _> = serde_json::from_value(val);
        let response = match req {
            Ok(r) => dispatch(r),
            Err(e) => serde_json::to_value(ErrorResponse {
                ok: false,
                error: format!("invalid request: {e}"),
            })
            .unwrap(),
        };
        if let Err(e) = write_message(&response) {
            eprintln!("shiki-native-host: failed to write response: {e}");
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_minimal() {
        assert_eq!(
            build_capture_request("hello", false, &[], None, None),
            "CAPTURE\n\nhello"
        );
    }

    #[test]
    fn build_request_with_all_headers() {
        let tags = vec!["work".into(), "idea".into()];
        assert_eq!(
            build_capture_request("hi", true, &tags, Some("work"), Some("a/b")),
            "CAPTURE\ndaily=1\ntags=work,idea\nnotebook=work\nfolder=a/b\n\nhi"
        );
    }

    #[test]
    fn parse_port_ok() {
        assert_eq!(parse_port_file("12345\n"), Some(12345));
    }

    #[test]
    fn parse_response_ok() {
        match parse_response_line("OK /tmp/x.md\n").unwrap() {
            DaemonResponse::Ok(p) => assert_eq!(p, "/tmp/x.md"),
            _ => panic!(),
        }
    }
}

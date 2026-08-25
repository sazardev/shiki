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
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 {
        return Ok(None);
    }
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
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    name: Option<String>,
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

#[derive(Debug, Serialize)]
struct TagsResponse {
    ok: bool,
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TemplatesResponse {
    ok: bool,
    templates: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SearchResponse {
    ok: bool,
    hits: Vec<SearchHit>,
    query: String,
}

#[derive(Debug, Serialize)]
struct SearchHit {
    title: String,
    path: String,
    notebook: String,
    preview: String,
    score: u32,
}

#[derive(Debug, Serialize)]
struct RecentResponse {
    ok: bool,
    notes: Vec<RecentNote>,
}

#[derive(Debug, Serialize)]
struct RecentNote {
    title: String,
    path: String,
    notebook: String,
    relative: String,
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
struct UndoResponse {
    ok: bool,
    path: String,
    via_daemon: bool,
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

#[allow(clippy::too_many_arguments)]
fn build_capture_request(
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
    if let Some(nb) = notebook {
        req.push_str(&format!("notebook={nb}\n"));
    }
    if let Some(f) = folder {
        req.push_str(&format!("folder={f}\n"));
    }
    if let Some(tmpl) = template {
        req.push_str(&format!("template={tmpl}\n"));
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

fn with_source(text: &str, url: Option<&str>, title: Option<&str>) -> String {
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

fn collect_folders_inner(
    notebook: &shiki_core::Notebook,
    relative: &Path,
    out: &mut Vec<String>,
    visited: &mut std::collections::HashSet<PathBuf>,
) {
    let dir = notebook.path.join(relative);
    if let Ok(real) = dir.canonicalize() {
        if !visited.insert(real) {
            return;
        }
    }
    if let Ok((folders, _)) = notebook.list_dir(relative) {
        for folder in folders {
            let rel = if relative.as_os_str().is_empty() {
                PathBuf::from(&folder)
            } else {
                relative.join(&folder)
            };
            out.push(rel.display().to_string());
            collect_folders_inner(notebook, &rel, out, visited);
        }
    }
}

fn collect_folders(notebook: &shiki_core::Notebook, relative: &Path, out: &mut Vec<String>) {
    let mut visited = std::collections::HashSet::new();
    collect_folders_inner(notebook, relative, out, &mut visited);
}

// ── Handlers ─────────────────────────────────────────────────────────────────

fn handle_ping() -> anyhow::Result<serde_json::Value> {
    let daemon = check_daemon();
    let config_info = match load_config_and_store() {
        Ok((config, _)) => {
            let data_dir = config.general.data_dir.clone().unwrap_or_else(|| {
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
    // Same filter the TUI applies: notebooks untracked via "keep files,
    // just untrack" ([notebooks.<name>] hidden = true) must not show up in
    // the extension's notebook picker either — untracking from the TUI
    // would otherwise leave them capturable from the browser.
    let notebooks: Vec<_> = store
        .list()
        .unwrap_or_default()
        .into_iter()
        .filter(|nb| {
            !config
                .notebooks
                .get(&nb.name)
                .is_some_and(|over| over.hidden)
        })
        .collect();
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
    let mut folders = vec!["".to_string()];
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

fn handle_list_tags() -> anyhow::Result<serde_json::Value> {
    let (_, store) = load_config_and_store()?;
    let pool = store.all_notes().unwrap_or_default();
    let tags = shiki_core::tags::all_tags(&pool);
    Ok(serde_json::to_value(TagsResponse { ok: true, tags })?)
}

fn handle_list_templates() -> anyhow::Result<serde_json::Value> {
    let dir = Config::default_templates_dir()?;
    shiki_core::templates::ensure_defaults(&dir).ok();
    let mut templates = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) == Some("md") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    templates.push(stem.to_string());
                }
            }
        }
    }
    templates.sort();
    Ok(serde_json::to_value(TemplatesResponse {
        ok: true,
        templates,
    })?)
}

fn handle_search(query: &str, limit: usize) -> anyhow::Result<serde_json::Value> {
    let (_, store) = load_config_and_store()?;
    let pool = store.all_notes().unwrap_or_default();
    if query.trim().is_empty() {
        return Ok(serde_json::to_value(SearchResponse {
            ok: true,
            hits: vec![],
            query: query.to_string(),
        })?);
    }
    let notes: Vec<shiki_core::Note> = pool.iter().map(|(_, n)| n.clone()).collect();
    let mut engine = shiki_core::SearchEngine::new();
    // Search titles and also body preview? For popup, search title + body snippet
    let haystacks: Vec<String> = notes
        .iter()
        .map(|n| {
            format!(
                "{} {}",
                n.frontmatter.title,
                n.body.chars().take(200).collect::<String>()
            )
        })
        .collect();
    let hay_refs: Vec<&str> = haystacks.iter().map(|s| s.as_str()).collect();
    let hits = engine.search_text(query, &hay_refs);
    let mut out = Vec::new();
    for h in hits.into_iter().take(limit) {
        if let Some((nb, note)) = pool.get(h.index) {
            out.push(SearchHit {
                title: note.frontmatter.title.clone(),
                path: note.path.display().to_string(),
                notebook: nb.name.clone(),
                preview: note
                    .body
                    .chars()
                    .take(120)
                    .collect::<String>()
                    .replace('\n', " "),
                score: h.score,
            });
        }
    }
    Ok(serde_json::to_value(SearchResponse {
        ok: true,
        hits: out,
        query: query.to_string(),
    })?)
}

fn handle_recent(limit: usize) -> anyhow::Result<serde_json::Value> {
    let (_, store) = load_config_and_store()?;
    let pool = store.all_notes().unwrap_or_default();
    // Sort by file mtime or frontmatter date descending
    let mut items: Vec<(
        std::time::SystemTime,
        shiki_core::Notebook,
        shiki_core::Note,
    )> = Vec::new();
    for (nb, note) in pool {
        let mtime = std::fs::metadata(&note.path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        items.push((mtime, nb, note));
    }
    items.sort_by_key(|b| std::cmp::Reverse(b.0));
    let notes: Vec<RecentNote> = items
        .into_iter()
        .take(limit)
        .map(|(_, nb, note)| {
            let rel = note
                .path
                .strip_prefix(&nb.path)
                .unwrap_or(&note.path)
                .display()
                .to_string();
            RecentNote {
                title: note.frontmatter.title.clone(),
                path: note.path.display().to_string(),
                notebook: nb.name.clone(),
                relative: rel,
                tags: note.frontmatter.tags.clone(),
            }
        })
        .collect();
    Ok(serde_json::to_value(RecentResponse { ok: true, notes })?)
}

fn handle_create_folder(notebook: &str, folder: &str) -> anyhow::Result<serde_json::Value> {
    let (_, store) = load_config_and_store()?;
    let nb = store
        .get(notebook)
        .map_err(|e| anyhow::anyhow!("notebook '{notebook}' not found: {e}"))?;
    let rel = shiki_core::notebook::validate_relative_path(folder)?;
    // Create nested path step by step
    let mut cur = PathBuf::new();
    for comp in rel.components() {
        cur.push(comp);
        nb.create_folder_in(
            cur.parent().unwrap_or(Path::new("")),
            &comp.as_os_str().to_string_lossy(),
        )?;
    }
    Ok(serde_json::json!({"ok": true, "path": nb.path.join(rel).display().to_string()}))
}

fn handle_undo() -> anyhow::Result<serde_json::Value> {
    // Try daemon first
    if let Some(resp) = try_daemon("UNDO\n\n") {
        match resp {
            DaemonResponse::Ok(path) => {
                return Ok(serde_json::to_value(UndoResponse {
                    ok: true,
                    path,
                    via_daemon: true,
                })?);
            }
            DaemonResponse::Err(msg) if msg.starts_with("locked:") => {
                anyhow::bail!(msg);
            }
            _ => {}
        }
    }
    // Fallback standalone
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
        Some(dir) => PathBuf::from(dir),
        None => Config::default_data_dir()?,
    };
    let store = NotebookStore::new_with_custom_paths(data_dir, config.notebook_custom_paths());
    let _nb = store
        .get(&notebook)
        .map_err(|e| anyhow::anyhow!("notebook '{notebook}' not found: {e}"))?;
    match &record {
        LastCapture::Note { path, .. } => {
            let trash_dir = Config::default_trash_dir()?.join(&notebook);
            let suffix = chrono::Local::now().format("%Y%m%d%H%M%S%3f").to_string();
            shiki_core::trash::move_to_trash(Path::new(path), &trash_dir, &suffix)
                .map_err(|e| anyhow::anyhow!("could not move '{path}' to trash: {e}"))?;
        }
        LastCapture::DailyAppend { path, appended, .. } => {
            // Need crypto handling — fail if encrypted locked
            let encrypted = config.encrypt_for(&notebook);
            if encrypted {
                anyhow::bail!("notebook '{notebook}' is encrypted and locked");
            }
            let mut note = shiki_core::Note::from_file_in_notebook_with_crypto(
                Path::new(path),
                &notebook,
                None,
            )
            .map_err(|e| anyhow::anyhow!("could not read '{path}': {e}"))?;
            if !note.body.ends_with(appended.as_str()) {
                anyhow::bail!("the daily note has changed since that capture — not undoing");
            }
            let new_len = note.body.len() - appended.len();
            note.body.truncate(new_len);
            note.save_with_crypto(None)
                .map_err(|e| anyhow::anyhow!("could not save '{path}': {e}"))?;
        }
    }
    LastCapture::clear(&record_path);
    Ok(serde_json::to_value(UndoResponse {
        ok: true,
        path,
        via_daemon: false,
    })?)
}

fn handle_capture(req: &Request) -> anyhow::Result<serde_json::Value> {
    let mut text = req.text.clone().unwrap_or_default();
    // If text is empty but url is given (browser clip with no selection), use url as body
    // and let with_source handle provenance — keep raw text separate so daemon
    // appends Source consistently.
    if text.trim().is_empty() {
        if let Some(u) = &req.url {
            if !u.trim().is_empty() {
                text = u.clone();
            }
        }
    }
    let text = text.trim().to_string();
    if text.is_empty() {
        anyhow::bail!("empty capture text");
    }
    let daily = req.daily.unwrap_or(false);
    let tags = req.tags.clone().unwrap_or_default();
    let notebook = req.notebook.clone();
    let folder = req.folder.clone();
    let template = req.template.clone();
    let url = req.url.clone();
    let title = req.title.clone();
    // Native host doesn't set source header itself; browser popup can pass it later
    let source: Option<String> = None;

    // Header injection guard: daemon protocol is \n-delimited
    for t in &tags {
        if t.contains('\n') || t.contains('\r') {
            anyhow::bail!("invalid tag: must not contain newline");
        }
    }
    if let Some(nb) = &notebook {
        if nb.contains('\n') || nb.contains('\r') {
            anyhow::bail!("invalid notebook: must not contain newline");
        }
    }
    if let Some(f) = &folder {
        if f.contains('\n') || f.contains('\r') {
            anyhow::bail!("invalid folder: must not contain newline");
        }
    }
    if let Some(t) = &template {
        if t.contains('\n')
            || t.contains('\r')
            || t.contains('/')
            || t.contains('\\')
            || t.contains('.')
        {
            anyhow::bail!("invalid template");
        }
        // also validate via same rule as notebook name
        if t.contains('/') || t.contains('\\') {
            anyhow::bail!("invalid template");
        }
    }
    if let Some(u) = &url {
        if u.contains('\n') || u.contains('\r') {
            anyhow::bail!("invalid url: must not contain newline");
        }
    }
    if let Some(t) = &title {
        if t.contains('\n') || t.contains('\r') {
            anyhow::bail!("invalid title: must not contain newline");
        }
    }
    if let Some(s) = &source {
        if s.contains('\n') || s.contains('\r') {
            anyhow::bail!("invalid source: must not contain newline");
        }
    }

    let daemon_req = build_capture_request(
        &text,
        daily,
        &tags,
        notebook.as_deref(),
        folder.as_deref(),
        template.as_deref(),
        url.as_deref(),
        title.as_deref(),
        source.as_deref(),
    );
    match try_daemon(&daemon_req) {
        Some(DaemonResponse::Ok(path)) => {
            let nb_name = notebook.clone().unwrap_or_else(|| {
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
        _ => {}
    }

    let (config, store) = load_config_and_store()?;
    // Fallback path must produce the same final body the daemon does, so
    // apply the Source footer here too before routing/templating.
    let final_text = with_source(&text, url.as_deref(), title.as_deref());
    let existing_notebooks: Vec<String> = store
        .list()
        .unwrap_or_default()
        .into_iter()
        .map(|nb| nb.name)
        .collect();
    let (target, text_final) = match notebook.as_deref() {
        Some(name) => (name.to_string(), final_text.clone()),
        None => {
            if let Some((n, t)) =
                shiki_core::notebook::route_by_prefix(&final_text, &existing_notebooks)
            {
                (n, t.to_string())
            } else {
                (config.general.default_notebook.clone(), final_text.clone())
            }
        }
    };

    let nb = match store.get(&target) {
        Ok(nb) => nb,
        Err(_) => store
            .create(&target)
            .map_err(|e| anyhow::anyhow!("could not create notebook '{target}': {e}"))?,
    };
    let encrypted = config.encrypt_for(&target);
    if encrypted {
        anyhow::bail!(
            "notebook '{target}' is encrypted and locked — unlock it in the TUI or run `shiki capture` from a terminal first"
        );
    }

    // If template is requested, create note via template rendering (for non-daily)
    let (path, record) = if daily {
        capture_into_daily(&store, &config, &nb, &text_final)?
    } else if let Some(tmpl_name) = template.filter(|s| !s.is_empty()) {
        capture_into_templated(&nb, &text_final, &tags, folder.as_deref(), &tmpl_name)?
    } else {
        capture_into_new_note(&nb, &text_final, &tags, folder.as_deref())?
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

fn capture_into_templated(
    nb: &shiki_core::Notebook,
    text: &str,
    tags: &[String],
    folder: Option<&str>,
    template_name: &str,
) -> anyhow::Result<(PathBuf, LastCapture)> {
    let templates_dir = Config::default_templates_dir()?;
    let tmpl = shiki_core::Template::load(&templates_dir, template_name)
        .map_err(|e| anyhow::anyhow!("template '{template_name}' not found: {e}"))?;
    let mut vars = std::collections::HashMap::new();
    let title = format!("Capture {}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
    vars.insert("title", title.clone());
    vars.insert("date", chrono::Local::now().format("%Y-%m-%d").to_string());
    vars.insert("body", text.to_string());
    let rendered = tmpl.render(&vars);
    // Template already contains title/date, append body if not present
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
        "list_tags" => handle_list_tags(),
        "list_templates" => handle_list_templates(),
        "search" => {
            let q = req
                .query
                .clone()
                .unwrap_or_else(|| req.text.clone().unwrap_or_default());
            let mut limit = req.limit.unwrap_or(8);
            if limit > 50 {
                limit = 50;
            }
            let q = q.chars().take(500).collect::<String>();
            handle_search(&q, limit)
        }
        "recent" => {
            let mut limit = req.limit.unwrap_or(8);
            if limit > 50 {
                limit = 50;
            }
            handle_recent(limit)
        }
        "create_folder" => {
            let nb = req.notebook.clone().unwrap_or_default();
            let folder = req.folder.clone().or(req.name.clone()).unwrap_or_default();
            if nb.is_empty() || folder.is_empty() {
                Err(anyhow::anyhow!("notebook and folder/name required"))
            } else {
                handle_create_folder(&nb, &folder)
            }
        }
        "capture" => handle_capture(&req),
        "undo" => handle_undo(),
        "open_note" => {
            let path = req.text.clone().or(req.query.clone()).unwrap_or_default();
            if path.is_empty() {
                Err(anyhow::anyhow!("path required"))
            } else {
                let p = Path::new(&path);
                if !p.exists() {
                    Err(anyhow::anyhow!("file not found: {path}"))
                } else {
                    match load_config_and_store() {
                        Ok((_, store)) => {
                            let data_dir = store.root.canonicalize().unwrap_or(store.root.clone());
                            let p_real = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
                            let mut allowed = p_real.starts_with(&data_dir);
                            if !allowed {
                                for cpath in store.custom_paths.values() {
                                    if let Ok(c) = cpath.canonicalize() {
                                        if p_real.starts_with(&c) {
                                            allowed = true;
                                            break;
                                        }
                                    } else if p_real.starts_with(cpath) {
                                        allowed = true;
                                        break;
                                    }
                                }
                            }
                            if !allowed {
                                if let Ok(notes) = store.all_notes() {
                                    for (_, note) in notes {
                                        if let Ok(r) = note.path.canonicalize() {
                                            if r == p_real {
                                                allowed = true;
                                                break;
                                            }
                                        } else if note.path == p_real {
                                            allowed = true;
                                            break;
                                        }
                                    }
                                }
                            }
                            if !allowed {
                                Err(anyhow::anyhow!("path not inside a notebook: {path}"))
                            } else {
                                #[cfg(target_os = "linux")]
                                let _ = std::process::Command::new("xdg-open").arg(p).spawn();
                                #[cfg(target_os = "macos")]
                                let _ = std::process::Command::new("open").arg(p).spawn();
                                #[cfg(target_os = "windows")]
                                let _ = std::process::Command::new("cmd")
                                    .args(["/C", "start", "", &path])
                                    .spawn();
                                Ok(serde_json::json!({"ok": true, "path": path}))
                            }
                        }
                        Err(e) => Err(anyhow::anyhow!("could not load store: {e}")),
                    }
                }
            }
        }
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
    loop {
        let msg = read_message()?;
        let Some(val) = msg else {
            break;
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
            build_capture_request("hello", false, &[], None, None, None, None, None, None),
            "CAPTURE\n\nhello"
        );
    }

    #[test]
    fn build_request_with_all_headers() {
        let tags = vec!["work".into(), "idea".into()];
        assert_eq!(
            build_capture_request(
                "hi",
                true,
                &tags,
                Some("work"),
                Some("a/b"),
                Some("meeting"),
                Some("https://example.com"),
                Some("Example"),
                Some("browser")
            ),
            "CAPTURE\ndaily=1\ntags=work,idea\nnotebook=work\nfolder=a/b\ntemplate=meeting\nurl=https://example.com\ntitle=Example\nsource=browser\n\nhi"
        );
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
        assert_eq!(
            with_source("hello https://x.com world", Some("https://x.com"), None),
            "hello https://x.com world"
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

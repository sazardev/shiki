//! Note operations for the desktop UI — the functional core that makes the
//! GUI usable, mirroring what the TUI's middle/right panes do with the same
//! `shiki-core` primitives: list, read, save, create/rename/delete, daily
//! notes, fuzzy search, markdown rendering and git status/commit.
//!
//! Every path crossing the IPC boundary is *relative to the notebook root*
//! (what `Notebook::list_notes` returns) — the webview never sees absolute
//! filesystem paths except in rendered HTML image `src`s, which go through
//! Tauri's asset protocol for display.

use std::path::Path;

use serde::Serialize;
use shiki_config::Config;
use shiki_core::git;
use shiki_core::note::{Frontmatter, Note};
use shiki_core::search::SearchEngine;
use shiki_core::{daily, templates};

use crate::commands::AppState;

fn get_notebook(state: &AppState, name: &str) -> Result<shiki_core::Notebook, String> {
    state.store().get(name).map_err(|e| e.to_string())
}

/// Frontmatter date formatted as `YYYY-MM-DD` for sorting/display.
fn iso_date(d: chrono::NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

#[derive(Debug, Serialize)]
pub struct NoteInfo {
    /// Path relative to the notebook root, e.g. `notes/foo.md`.
    pub path: String,
    pub title: String,
    pub date: String,
    pub tags: Vec<String>,
    /// File mtime as an ISO timestamp — lets the UI sort by "recently
    /// modified" without touching git.
    pub modified: String,
}

fn note_info(note: &Note) -> NoteInfo {
    NoteInfo {
        path: note.path.to_string_lossy().into_owned(),
        title: note.frontmatter.title.clone(),
        date: iso_date(note.frontmatter.date),
        tags: note.frontmatter.tags.clone(),
        modified: std::fs::metadata(&note.path)
            .and_then(|m| m.modified())
            .map(|t| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default(),
    }
}

#[tauri::command]
pub fn list_notes(
    state: tauri::State<'_, AppState>,
    notebook: String,
) -> Result<Vec<NoteInfo>, String> {
    let nb = get_notebook(&state, &notebook)?;
    let mut notes = nb.list_notes().map_err(|e| e.to_string())?;
    notes.sort_by_key(|n| std::cmp::Reverse(n.frontmatter.date));
    Ok(notes.iter().map(note_info).collect())
}

#[derive(Debug, Serialize)]
pub struct NoteContent {
    pub content: String,
    pub frontmatter: Frontmatter,
}

#[tauri::command]
pub fn read_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
) -> Result<NoteContent, String> {
    let nb = get_notebook(&state, &notebook)?;
    if nb.crypto.is_some() {
        return Err("encrypted notebooks are not editable from the desktop app yet".into());
    }
    let note =
        Note::from_file_in_notebook(&nb.path.join(&path), &nb.name).map_err(|e| e.to_string())?;
    Ok(NoteContent {
        content: note.to_file_contents().map_err(|e| e.to_string())?,
        frontmatter: note.frontmatter,
    })
}

#[tauri::command]
pub fn save_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
    content: String,
) -> Result<(), String> {
    let nb = get_notebook(&state, &notebook)?;
    if nb.crypto.is_some() {
        return Err("encrypted notebooks are not editable from the desktop app yet".into());
    }
    std::fs::write(nb.path.join(&path), content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    title: String,
    template: Option<String>,
    folder: Option<String>,
) -> Result<NoteInfo, String> {
    let nb = get_notebook(&state, &notebook)?;
    let body = match template {
        Some(name) => {
            let dir = Config::default_templates_dir().map_err(|e| e.to_string())?;
            let tpl = templates::Template::load(&dir, &name).map_err(|e| e.to_string())?;
            let mut vars = std::collections::HashMap::new();
            vars.insert("title", title.clone());
            vars.insert("date", chrono::Local::now().format("%Y-%m-%d").to_string());
            tpl.render(&vars)
        }
        None => String::new(),
    };
    let rel = folder
        .as_deref()
        .map(Path::new)
        .unwrap_or_else(|| Path::new(""));
    let note = nb
        .create_note_in(rel, &title, body)
        .map_err(|e| e.to_string())?;
    Ok(note_info(&note))
}

#[tauri::command]
pub fn rename_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
    new_title: String,
) -> Result<(), String> {
    let nb = get_notebook(&state, &notebook)?;
    // `rename_note_at`/`delete_note_at` (unlike `delete_folder_at`) expect
    // an already-absolute path — passing the bare relative path here was a
    // real bug: it resolved (or failed to) relative to the process's CWD,
    // not the notebook root, the same way `read_note`/`save_note` already
    // correctly join `nb.path` first.
    nb.rename_note_at(&nb.path.join(&path), &new_title)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Moves the note to the trash instead of hard-deleting it outright — same
/// safety net the TUI's `d` has (`key_handlers.rs::trash_path`), a
/// best-effort one: if the trash directory can't be resolved or the move
/// itself fails, this falls back to an actual delete rather than blocking
/// a delete the user already confirmed. Returns the trash path so the
/// caller can offer undo without the backend needing its own "last
/// deleted" session state.
#[tauri::command]
pub fn delete_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
) -> Result<Option<String>, String> {
    let nb = get_notebook(&state, &notebook)?;
    let abs = nb.path.join(&path);
    if let Ok(trash_root) = Config::default_trash_dir() {
        let trash_dir = trash_root.join(&nb.name);
        let suffix = chrono::Local::now().format("%Y%m%d%H%M%S%3f").to_string();
        if let Ok(trash_path) = shiki_core::trash::move_to_trash(&abs, &trash_dir, &suffix) {
            return Ok(Some(trash_path.to_string_lossy().into_owned()));
        }
    }
    nb.delete_note_at(&abs).map_err(|e| e.to_string())?;
    Ok(None)
}

#[tauri::command]
pub fn undo_delete_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
    trash_path: String,
) -> Result<(), String> {
    let nb = get_notebook(&state, &notebook)?;
    let original = nb.path.join(&path);
    shiki_core::trash::restore(Path::new(&trash_path), &original).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_folder(
    state: tauri::State<'_, AppState>,
    notebook: String,
    name: String,
) -> Result<(), String> {
    let nb = get_notebook(&state, &notebook)?;
    nb.create_folder_in(Path::new(""), &name)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Same destination-parsing convention as `move_note`, but leaves the
/// source in place — `Mode::Visual`'s batch copy (`y`) needs this, unlike
/// the single-item move prompt which always deletes the source.
#[tauri::command]
pub fn copy_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
    dest: String,
) -> Result<NoteInfo, String> {
    let nb = get_notebook(&state, &notebook)?;
    let mut parts = dest.splitn(2, '/');
    let dest_notebook_name = parts.next().unwrap_or("").trim();
    let dest_relative = parts.next().unwrap_or("").trim();
    if dest_notebook_name.is_empty() {
        return Err("destination notebook name is required".into());
    }
    let dest_nb = get_notebook(&state, dest_notebook_name)?;
    let copied = nb
        .copy_note_to(&nb.path.join(&path), &dest_nb, Path::new(dest_relative))
        .map_err(|e| e.to_string())?;
    Ok(note_info(&copied))
}

/// Moves a note to `notebook/relative/folder/path` — `dest` is parsed the
/// same shape the TUI's move prompt prefills (`{notebook}/{breadcrumb}`):
/// the first segment names the destination notebook (must already exist —
/// a typo shouldn't silently create one), everything after it is the
/// relative folder within it (auto-created, same as note/folder creation
/// already does).
#[tauri::command]
pub fn move_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
    dest: String,
) -> Result<NoteInfo, String> {
    let nb = get_notebook(&state, &notebook)?;
    let mut parts = dest.splitn(2, '/');
    let dest_notebook_name = parts.next().unwrap_or("").trim();
    let dest_relative = parts.next().unwrap_or("").trim();
    if dest_notebook_name.is_empty() {
        return Err("destination notebook name is required".into());
    }
    let dest_nb = get_notebook(&state, dest_notebook_name)?;
    let moved = nb
        .move_note_to(&nb.path.join(&path), &dest_nb, Path::new(dest_relative))
        .map_err(|e| e.to_string())?;
    Ok(note_info(&moved))
}

#[tauri::command]
pub fn create_notebook(state: tauri::State<'_, AppState>, name: String) -> Result<(), String> {
    state.store().create(&name).map_err(|e| e.to_string())?;
    Ok(())
}

/// Notebook name derived from a git URL's repo name — the last path
/// segment, minus a trailing `.git`. Handles both `.../owner/repo` (split on
/// `/`) and `git@host:owner/repo.git` (split on `:` for the host separator,
/// `/` for the owner/repo one) since splitting on either character and
/// taking the last piece lands on `repo[.git]` either way.
fn notebook_name_from_git_url(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    (!name.is_empty()).then(|| name.to_string())
}

#[derive(Debug, Serialize)]
pub struct CloneResult {
    pub name: String,
    pub message: String,
}

/// New-notebook fast path for pasting a git URL directly: derives the
/// notebook name from the repo name, creates it under the default data
/// dir, points its remote at the URL, and pulls right away — mirrors
/// shiki-tui's `App::create_notebook_from_url`. A failure to set the remote
/// or pull is reported in `message` rather than as an `Err`, since the
/// notebook was still genuinely created at that point (same "don't fail the
/// whole operation over a partial step" reasoning `App::create_notebook_from_url`
/// already uses) — the frontend should still refresh its notebook list either way.
#[tauri::command]
pub fn create_notebook_from_url(
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<CloneResult, String> {
    let redacted = git::redact_credentials(&url);
    let name = notebook_name_from_git_url(&url)
        .ok_or_else(|| format!("could not derive a notebook name from '{redacted}'"))?;
    let nb = state.store().create(&name).map_err(|e| e.to_string())?;
    if let Err(e) = git::set_remote(&nb.path, &url) {
        return Ok(CloneResult {
            name,
            message: format!("created but could not set remote: {e}"),
        });
    }
    let cfg = state.config();
    let message = match git::pull(&nb.path, &cfg.git.remote, &cfg.git.branch) {
        Ok(outcome) => {
            let branch = outcome.branch();
            if branch == cfg.git.branch {
                format!("cloned from {redacted}")
            } else {
                format!("cloned from {redacted} (branch '{branch}')")
            }
        }
        Err(e) => format!("created and set remote, but pull failed: {e}"),
    };
    Ok(CloneResult { name, message })
}

#[derive(Debug, Serialize)]
#[serde(tag = "status")]
pub enum AdoptFolderResult {
    Adopted {
        name: String,
    },
    /// The folder has no `.git` yet — the frontend should confirm with the
    /// user before calling this command again with `init_git_if_missing:
    /// true`, mirroring shiki-tui's confirm dialog for the same case
    /// (`App::adopt_notebook_from_path`).
    NeedsGitInitConfirm {
        name: String,
    },
}

/// New-notebook fast path for pointing at an existing directory on disk —
/// mirrors shiki-tui's `App::adopt_notebook_from_path` +
/// `App::finish_notebook_adopt`, except the path always arrives already
/// resolved (the frontend gets it from a native folder-picker dialog, so
/// there's no `~`/`./` shorthand to expand here).
#[tauri::command]
pub fn adopt_notebook_folder(
    state: tauri::State<'_, AppState>,
    path: String,
    init_git_if_missing: bool,
) -> Result<AdoptFolderResult, String> {
    let path = std::path::PathBuf::from(path);
    if !path.is_dir() {
        return Err(format!("'{}' is not a directory", path.display()));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| format!("could not derive a notebook name from '{}'", path.display()))?;
    if state.store().get(&name).is_ok() {
        return Err(format!("notebook '{name}' already exists"));
    }
    if !path.join(".git").is_dir() {
        if !init_git_if_missing {
            return Ok(AdoptFolderResult::NeedsGitInitConfirm { name });
        }
        git::init_repo(&path).map_err(|e| e.to_string())?;
    }
    state.register_custom_path(name.clone(), path)?;
    Ok(AdoptFolderResult::Adopted { name })
}

#[tauri::command]
pub fn daily_note(state: tauri::State<'_, AppState>, notebook: String) -> Result<NoteInfo, String> {
    let nb = get_notebook(&state, &notebook)?;
    let templates_dir = Config::default_templates_dir().map_err(|e| e.to_string())?;
    let template_name = state.config().general.daily_template.clone();
    let date = chrono::Local::now().date_naive();
    let note = daily::create_or_open(&nb, date, &templates_dir, &template_name, None)
        .map_err(|e| e.to_string())?;
    Ok(note_info(&note))
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub notebook: String,
    pub path: String,
    pub title: String,
    pub score: u32,
}

#[tauri::command]
pub fn search_notes(
    state: tauri::State<'_, AppState>,
    query: String,
    notebook: Option<String>,
) -> Result<Vec<SearchResult>, String> {
    let mut engine = SearchEngine::new();
    let mut out = Vec::new();
    let notebooks: Vec<shiki_core::Notebook> = match notebook {
        Some(name) => vec![get_notebook(&state, &name)?],
        None => state.store().list().map_err(|e| e.to_string())?,
    };
    for nb in notebooks {
        let notes = nb.list_notes().map_err(|e| e.to_string())?;
        for hit in engine.search(&query, &notes) {
            let note = &notes[hit.index];
            out.push(SearchResult {
                notebook: nb.name.clone(),
                path: note.path.to_string_lossy().into_owned(),
                title: note.frontmatter.title.clone(),
                score: hit.score,
            });
        }
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.score));
    Ok(out)
}

/// Markdown rendered to HTML server-side (comrak, same parser the TUI's
/// PREVIEW pane uses) plus the notebook root so the frontend can resolve
/// relative image paths through Tauri's asset protocol.
#[derive(Debug, Serialize)]
pub struct RenderedNote {
    pub html: String,
    /// Absolute path of the notebook directory — the prefix for every
    /// relative image `src` the frontend turns into `convertFileSrc(...)`.
    pub root: String,
}

#[tauri::command]
pub fn render_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
) -> Result<RenderedNote, String> {
    let nb = get_notebook(&state, &notebook)?;
    let note =
        Note::from_file_in_notebook(&nb.path.join(&path), &nb.name).map_err(|e| e.to_string())?;
    let raw = note.to_file_contents().map_err(|e| e.to_string())?;
    let html = render_markdown(&raw, &nb.path);
    Ok(RenderedNote {
        html,
        root: nb.path.to_string_lossy().into_owned(),
    })
}

/// Renders markdown to HTML with the same extension set the TUI preview
/// enables (tables, task lists, strikethrough, autolinks, wikilinks, raw
/// HTML for `<details>` folding), then rewrites relative image `src`s to
/// absolute paths for the asset protocol.
fn render_markdown(md: &str, notebook_root: &Path) -> String {
    let mut opts = comrak::Options::default();
    opts.extension.strikethrough = true;
    opts.extension.table = true;
    opts.extension.autolink = true;
    opts.extension.tasklist = true;
    opts.extension.wikilinks_title_after_pipe = true;
    opts.render.r#unsafe = true;
    let html = comrak::markdown_to_html(md, &opts);

    // Absolute-ize relative image srcs so the webview can load them via
    // convertFileSrc. http(s)/data/anchor/absolute are left alone.
    let re = regex::Regex::new(r##"src="([^"]+)""##).expect("static regex");
    re.replace_all(&html, |caps: &regex::Captures| {
        let target = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        if target.starts_with("http")
            || target.starts_with("data:")
            || target.starts_with('/')
            || Path::new(target).is_absolute()
        {
            caps[0].to_string()
        } else {
            let joined = notebook_root.join(target);
            format!(r##"src="{}""##, joined.to_string_lossy())
        }
    })
    .into_owned()
}

/// Mirrors the fields shiki-tui's footer actually renders
/// (`status_bar.rs::render`'s git segment) — branch/ahead/behind, not just
/// a dirty flag, so the desktop footer can show the same
/// `branch +dirty ↑ahead ↓behind` shape instead of a flattened "dirty or
/// not" summary.
#[derive(Debug, Serialize)]
pub struct GitStatus {
    pub dirty: bool,
    pub changed: usize,
    pub branch: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    /// First remote URL, if any — shown in the footer as the sync target.
    pub remote: Option<String>,
}

#[tauri::command]
pub fn git_status(
    state: tauri::State<'_, AppState>,
    notebook: String,
) -> Result<GitStatus, String> {
    let nb = get_notebook(&state, &notebook)?;
    let gs = git::status(&nb.path, &state.config().git.remote);
    let remote = git2::Repository::open(&nb.path).ok().and_then(|repo| {
        let r = repo.find_remote("origin").ok();
        r.and_then(|r| r.url().ok().map(|u| u.to_string()))
    });
    Ok(GitStatus {
        dirty: gs.dirty,
        changed: gs.dirty_count,
        branch: gs.branch,
        ahead: gs.ahead,
        behind: gs.behind,
        remote,
    })
}

#[tauri::command]
pub fn git_commit(
    state: tauri::State<'_, AppState>,
    notebook: String,
    message: String,
) -> Result<String, String> {
    let nb = get_notebook(&state, &notebook)?;
    let committed = git::commit_all(&nb.path, &message).map_err(|e| e.to_string())?;
    Ok(if committed {
        "committed".into()
    } else {
        "nothing to commit".into()
    })
}

/// Describes a `git::PullOutcome` in one line for a status message —
/// conflicts are surfaced as an error rather than silently landed, since
/// the desktop app has no merge-conflict resolver UI yet (unlike the TUI's
/// dedicated modal): a caller sees a clear "resolve in the TUI/CLI for now"
/// instead of a half-merged working tree with no way to act on it here.
#[tauri::command]
pub fn pull_notebook(
    state: tauri::State<'_, AppState>,
    notebook: String,
) -> Result<String, String> {
    let nb = get_notebook(&state, &notebook)?;
    let cfg = state.config();
    let outcome =
        git::pull(&nb.path, &cfg.git.remote, &cfg.git.branch).map_err(|e| e.to_string())?;
    match outcome {
        git::PullOutcome::FastForwarded { branch } => Ok(format!("pulled — {branch} fast-forwarded")),
        git::PullOutcome::UpToDate { branch } => Ok(format!("{branch} already up to date")),
        git::PullOutcome::NewRepo { branch } => Ok(format!("pulled — {branch} created")),
        git::PullOutcome::MergedClean { branch } => Ok(format!("pulled — {branch} merged cleanly")),
        git::PullOutcome::ConflictsPending { branch, files } => Err(format!(
            "{branch}: pull produced {} conflicting file(s) — resolve in the TUI or CLI, the desktop app can't yet",
            files.len()
        )),
    }
}

#[derive(Debug, Serialize)]
pub struct PullAllResult {
    pub notebook: String,
    pub ok: bool,
    pub message: String,
}

#[tauri::command]
pub fn pull_all_notebooks(state: tauri::State<'_, AppState>) -> Result<Vec<PullAllResult>, String> {
    let cfg = state.config();
    let remote = cfg.git.remote.clone();
    let branch = cfg.git.branch.clone();
    let mut out = Vec::new();
    for nb in state.store().list().map_err(|e| e.to_string())? {
        let result = git::pull(&nb.path, &remote, &branch);
        out.push(match result {
            Ok(git::PullOutcome::ConflictsPending { files, .. }) => PullAllResult {
                notebook: nb.name,
                ok: false,
                message: format!("{} conflicting file(s)", files.len()),
            },
            Ok(_) => PullAllResult {
                notebook: nb.name,
                ok: true,
                message: "ok".to_string(),
            },
            Err(e) => PullAllResult {
                notebook: nb.name,
                ok: false,
                message: e.to_string(),
            },
        });
    }
    Ok(out)
}

/// One row of the global tasks view — mirrors `shiki_core::tasks::Task`
/// plus the note it lives in (`notebook`/`path`/`location`), the same
/// flattening the TUI's `panel_tasks::TaskRow` does (see CLAUDE.md: "a flat
/// list, not the header-interspersed shape links/tree use").
#[derive(Debug, Serialize)]
pub struct TaskRow {
    pub notebook: String,
    pub path: String,
    pub location: String,
    pub raw_line: String,
    pub occurrence: usize,
    pub done: bool,
    pub text: String,
    pub due: Option<String>,
}

#[tauri::command]
pub fn list_tasks(state: tauri::State<'_, AppState>) -> Result<Vec<TaskRow>, String> {
    let mut out = Vec::new();
    for nb in state.store().list().map_err(|e| e.to_string())? {
        let notes = nb.all_notes_recursive().map_err(|e| e.to_string())?;
        for note in &notes {
            let location = shiki_core::tasks::location_of(&nb, note);
            for t in shiki_core::tasks::extract(&note.body) {
                out.push(TaskRow {
                    notebook: nb.name.clone(),
                    path: note.path.to_string_lossy().into_owned(),
                    location: location.clone(),
                    raw_line: t.raw_line,
                    occurrence: t.occurrence,
                    done: t.done,
                    text: t.text,
                    due: t.due.map(|d| d.format("%Y-%m-%d").to_string()),
                });
            }
        }
    }
    Ok(out)
}

#[tauri::command]
pub fn toggle_task(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
    raw_line: String,
    occurrence: usize,
) -> Result<(), String> {
    let nb = get_notebook(&state, &notebook)?;
    shiki_core::tasks::toggle(&nb.path.join(&path), &raw_line, occurrence)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct LinkNote {
    pub path: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct LinksInfo {
    pub outgoing: Vec<LinkNote>,
    pub backlinks: Vec<LinkNote>,
    pub mentions: Vec<LinkNote>,
}

/// The selected note's outgoing wikilinks, backlinks, and unlinked
/// mentions — scoped to the current notebook's root-level notes (matching
/// what `list_notes` already shows in NOTES), not the TUI's full
/// cross-notebook fallback (`resolve_one_global`) — a smaller but honest
/// subset rather than a half-built cross-notebook resolver.
#[tauri::command]
pub fn get_links(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
) -> Result<LinksInfo, String> {
    let nb = get_notebook(&state, &notebook)?;
    let target =
        Note::from_file_in_notebook(&nb.path.join(&path), &nb.name).map_err(|e| e.to_string())?;
    let all = nb.list_notes().map_err(|e| e.to_string())?;

    let outgoing = shiki_core::wikilinks::extract(&target.body)
        .into_iter()
        .filter_map(|link| {
            shiki_core::wikilinks::resolve_one(&link, &all).and_then(|p| {
                all.iter().find(|n| n.path == p).map(|n| LinkNote {
                    path: n.path.to_string_lossy().into_owned(),
                    title: n.frontmatter.title.clone(),
                })
            })
        })
        .collect();

    let backlinks = shiki_core::wikilinks::backlinks(&target.path, &all)
        .into_iter()
        .map(|n| LinkNote {
            path: n.path.to_string_lossy().into_owned(),
            title: n.frontmatter.title.clone(),
        })
        .collect();

    let mentions = shiki_core::wikilinks::unlinked_mentions(&target, &all)
        .into_iter()
        .map(|n| LinkNote {
            path: n.path.to_string_lossy().into_owned(),
            title: n.frontmatter.title.clone(),
        })
        .collect();

    Ok(LinksInfo {
        outgoing,
        backlinks,
        mentions,
    })
}

#[derive(Debug, Serialize)]
pub struct RevisionInfo {
    pub commit_id: String,
    pub date: String,
    pub message: String,
}

#[tauri::command]
pub fn note_history(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
) -> Result<Vec<RevisionInfo>, String> {
    let nb = get_notebook(&state, &notebook)?;
    let history = git::file_history(&nb.path, Path::new(&path)).map_err(|e| e.to_string())?;
    Ok(history
        .into_iter()
        .map(|r| RevisionInfo {
            commit_id: r.commit_id,
            date: r.date.format("%Y-%m-%d %H:%M").to_string(),
            message: r.message,
        })
        .collect())
}

/// Reverts the note's *working tree* to a past revision — not a commit by
/// itself (same as the TUI: `revert_file_to` never commits), so the
/// reverted content flows through the normal save/sync path the caller
/// already has, rather than needing a separate "revert commit" code path.
#[tauri::command]
pub fn revert_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
    commit_id: String,
) -> Result<(), String> {
    let nb = get_notebook(&state, &notebook)?;
    git::revert_file_to(&nb.path, &commit_id, Path::new(&path)).map_err(|e| e.to_string())
}

/// Spawns `editor` on the note's file and doesn't wait for it — the
/// desktop app has no terminal to suspend the way the TUI does
/// (`suspend_and_edit`), so this is fire-and-forget: the user edits in
/// their own external window and comes back on their own; re-selecting the
/// note re-reads it from disk (`select_note`/`render_note` always read
/// fresh), so whatever they saved externally shows up the next time it's
/// viewed with no extra plumbing needed.
fn spawn_editor(editor: &str, path: &std::path::Path) -> Result<(), String> {
    shiki_core::editor::command_for(editor, path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_external_editor(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
) -> Result<(), String> {
    let nb = get_notebook(&state, &notebook)?;
    spawn_editor(&state.config().general.editor, &nb.path.join(&path))
}

#[tauri::command]
pub fn open_favorite_editor(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
) -> Result<(), String> {
    let nb = get_notebook(&state, &notebook)?;
    let editor = shiki_core::editor::detect_favorite_editor()
        .unwrap_or_else(|| state.config().general.editor.clone());
    spawn_editor(&editor, &nb.path.join(&path))
}

#[derive(Debug, Serialize)]
pub struct DiffLineInfo {
    pub origin: char,
    pub content: String,
}

/// The selected note's pending (uncommitted) changes, working tree vs
/// `HEAD` — an empty result means the note has nothing pending, which the
/// caller treats as "fall back to history" (same as the TUI's `d`: "shows
/// the selected note's pending changes... when the note has nothing
/// uncommitted it opens the version history instead").
#[tauri::command]
pub fn working_diff(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
) -> Result<Vec<DiffLineInfo>, String> {
    let nb = get_notebook(&state, &notebook)?;
    let lines = git::working_tree_diff(&nb.path, Path::new(&path)).map_err(|e| e.to_string())?;
    Ok(lines
        .into_iter()
        .map(|l| DiffLineInfo {
            origin: l.origin,
            content: l.content,
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct TreeNote {
    pub path: String,
    pub title: String,
    /// Folder breadcrumb relative to the notebook root — empty string for
    /// a root-level note, otherwise e.g. `"projects/2026"`.
    pub folder: String,
}

/// Every note in the notebook at any depth — the desktop equivalent of the
/// TUI's tree view. NOTES itself only ever lists the current folder's root
/// (there's no folder-drilling UI in the desktop app yet), so this is the
/// one place a nested note becomes reachable at all without already
/// knowing its path.
#[tauri::command]
pub fn notebook_tree(
    state: tauri::State<'_, AppState>,
    notebook: String,
) -> Result<Vec<TreeNote>, String> {
    let nb = get_notebook(&state, &notebook)?;
    let mut notes = nb.all_notes_recursive().map_err(|e| e.to_string())?;
    notes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(notes
        .iter()
        .map(|n| {
            let folder = n
                .path
                .strip_prefix(&nb.path)
                .ok()
                .and_then(|rel| rel.parent())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            TreeNote {
                path: n.path.to_string_lossy().into_owned(),
                title: n.frontmatter.title.clone(),
                folder,
            }
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct QueryRowInfo {
    pub location: String,
    pub notebook: String,
    pub note_title: String,
    pub path: String,
    pub fields: std::collections::BTreeMap<String, String>,
}

/// Dataview-style `where ... sort ...` filter over every notebook's notes
/// (`shiki_core::query`) — same DSL and pool-loaded-once-per-open,
/// re-run-per-keystroke shape the TUI's query modal uses. `fields` is
/// flattened from `serde_yaml::Mapping` to a plain string map — simpler to
/// render in a table than round-tripping YAML scalar types through IPC.
#[tauri::command]
pub fn run_note_query(
    state: tauri::State<'_, AppState>,
    query: String,
    notebook: Option<String>,
) -> Result<Vec<QueryRowInfo>, String> {
    let q = shiki_core::query::parse(&query).map_err(|e| e.to_string())?;
    let pool = state.store().all_notes().map_err(|e| e.to_string())?;
    let today = chrono::Local::now().date_naive();
    let rows = shiki_core::query::run_query(&pool, &q, notebook.as_deref(), today);
    Ok(rows
        .into_iter()
        .map(|r| {
            let fields = r
                .fields
                .iter()
                .filter_map(|(k, v)| {
                    let key = k.as_str()?.to_string();
                    let value = match v {
                        serde_yaml::Value::String(s) => s.clone(),
                        other => serde_yaml::to_string(other)
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                    };
                    Some((key, value))
                })
                .collect();
            QueryRowInfo {
                location: r.location,
                notebook: r.notebook,
                note_title: r.note_title,
                path: r.path.to_string_lossy().into_owned(),
                fields,
            }
        })
        .collect())
}

/// Exports the notebook (recursively, sorted date-then-title — same as
/// `shiki export`) into `{data_dir}/exports/{notebook}.{html,md}`, opened
/// immediately via the OS's default handler afterward (`shiki_core::browser::open_url`,
/// the same primitive the footer's coffee link already uses). A dedicated
/// `exports/` directory rather than writing into the notebook's own
/// (git-tracked) folder — an export bundle isn't a note and shouldn't end
/// up committed by the next auto-sync.
#[tauri::command]
pub fn export_notebook(
    state: tauri::State<'_, AppState>,
    notebook: String,
    format: String,
) -> Result<String, String> {
    let nb = get_notebook(&state, &notebook)?;
    let mut notes = nb.all_notes_recursive().map_err(|e| e.to_string())?;
    notes.sort_by(|a, b| {
        a.frontmatter
            .date
            .cmp(&b.frontmatter.date)
            .then_with(|| a.frontmatter.title.cmp(&b.frontmatter.title))
    });

    let (fmt, ext) = match format.as_str() {
        "html" => (shiki_core::export::Format::Html, "html"),
        "md" => (shiki_core::export::Format::Md, "md"),
        other => return Err(format!("unknown export format '{other}'")),
    };
    let content = shiki_core::export::render(&notebook, &notes, fmt);

    let data_dir = Config::default_data_dir().map_err(|e| e.to_string())?;
    let out_dir = data_dir.join("exports");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out_path = out_dir.join(format!("{notebook}.{ext}"));
    std::fs::write(&out_path, content).map_err(|e| e.to_string())?;

    let _ = shiki_core::browser::open_url(&out_path.to_string_lossy());
    Ok(out_path.to_string_lossy().into_owned())
}

/// Renders the notebook to a themed PDF via `pretty-pdf` (downloaded/cached
/// automatically on first use — `shiki_core::publish::publish` handles
/// that, same as the TUI's `leader+P`), written to
/// `{data_dir}/exports/{notebook}.pdf` and opened immediately. This can
/// genuinely take a few seconds (fetching the renderer binary on a cold
/// cache) — the caller should show it as in-flight, not assume it returns
/// instantly the way `export_notebook` does.
#[tauri::command]
pub fn publish_notebook(
    state: tauri::State<'_, AppState>,
    notebook: String,
) -> Result<String, String> {
    let nb = get_notebook(&state, &notebook)?;
    let mut notes = nb.all_notes_recursive().map_err(|e| e.to_string())?;
    notes.sort_by(|a, b| {
        a.frontmatter
            .date
            .cmp(&b.frontmatter.date)
            .then_with(|| a.frontmatter.title.cmp(&b.frontmatter.title))
    });

    let theme = state.config().export.pdf_theme.clone();
    let out_dir = state.store().root.join("exports");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out_path = out_dir.join(format!("{notebook}.pdf"));
    let cache_dir = state.store().root.join("bin");

    shiki_core::publish::publish(&notes, &theme, &cache_dir, &out_path)
        .map_err(|e| e.to_string())?;
    let _ = shiki_core::browser::open_url(&out_path.to_string_lossy());
    Ok(out_path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn set_notebook_remote(
    state: tauri::State<'_, AppState>,
    notebook: String,
    url: String,
) -> Result<(), String> {
    let nb = get_notebook(&state, &notebook)?;
    git::set_remote(&nb.path, &url).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_markdown_produces_html() {
        let html = render_markdown(
            "# Hola\n\n- [x] done\n- todo\n\n[[wikilink]]",
            Path::new("/tmp/nb"),
        );
        assert!(html.contains("<h1"));
        assert!(html.contains("Hola"));
        assert!(html.contains("wikilink"));
        assert!(html.contains("<li><input type=\"checkbox\""));
        assert!(html.contains("checked=\"\""));
        assert!(html.contains("disabled=\"\""));
    }

    #[test]
    fn render_markdown_absolutizes_relative_images() {
        let html = render_markdown(
            "![pic](attachments/x.png)\n\n![web](https://ex.com/a.png)",
            Path::new("/tmp/nb"),
        );
        // Path separators: the render joins the relative path as one string
        // (forward slashes preserved on every platform), so assert against
        // the same join.
        let expected = Path::new("/tmp/nb").join("attachments/x.png");
        assert!(
            html.contains(&format!(r##"src="{}""##, expected.to_string_lossy())),
            "expected {expected:?} in: {html}"
        );
        assert!(html.contains(r#"src="https://ex.com/a.png""#));
    }
}

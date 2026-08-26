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
    state.store.get(name).map_err(|e| e.to_string())
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
    nb.rename_note_at(Path::new(&path), &new_title)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_note(
    state: tauri::State<'_, AppState>,
    notebook: String,
    path: String,
) -> Result<(), String> {
    let nb = get_notebook(&state, &notebook)?;
    nb.delete_note_at(Path::new(&path))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn create_notebook(state: tauri::State<'_, AppState>, name: String) -> Result<(), String> {
    state.store.create(&name).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn daily_note(state: tauri::State<'_, AppState>, notebook: String) -> Result<NoteInfo, String> {
    let nb = get_notebook(&state, &notebook)?;
    let templates_dir = Config::default_templates_dir().map_err(|e| e.to_string())?;
    let template_name = state.config.general.daily_template.clone();
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
        None => state.store.list().map_err(|e| e.to_string())?,
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

#[derive(Debug, Serialize)]
pub struct GitStatus {
    pub dirty: bool,
    pub changed: usize,
    /// First remote URL, if any — shown in the footer as the sync target.
    pub remote: Option<String>,
}

#[tauri::command]
pub fn git_status(
    state: tauri::State<'_, AppState>,
    notebook: String,
) -> Result<GitStatus, String> {
    let nb = get_notebook(&state, &notebook)?;
    let statuses = git::file_statuses(&nb.path).map_err(|e| e.to_string())?;
    let remote = git2::Repository::open(&nb.path).ok().and_then(|repo| {
        let r = repo.find_remote("origin").ok();
        r.and_then(|r| r.url().ok().map(|u| u.to_string()))
    });
    Ok(GitStatus {
        dirty: !statuses.is_empty(),
        changed: statuses.len(),
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

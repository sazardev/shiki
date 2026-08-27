//! Saving a pasted clipboard image as a note attachment. Unlike
//! `shiki-tui/src/attachments.rs` (which decodes `arboard::ImageData` and
//! re-encodes it as PNG), the browser's Clipboard API already hands the
//! frontend pre-encoded PNG bytes — so this is a plain byte-write, no image
//! crate needed, and `shiki-tui` (a separate binary's dependency, not
//! `shiki-desktop`'s) is never imported.

use std::path::{Path, PathBuf};

use crate::commands::AppState;

fn unique_file(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.png"));
    if !first.exists() {
        return first;
    }
    for n in 2.. {
        let candidate = dir.join(format!("{stem}-{n}.png"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[derive(Debug, serde::Serialize)]
pub struct PastedImage {
    pub path: String,
    pub markdown_link: String,
}

#[tauri::command]
pub fn save_pasted_image(
    state: tauri::State<'_, AppState>,
    notebook: String,
    bytes: Vec<u8>,
) -> Result<PastedImage, String> {
    let nb = state.store().get(&notebook).map_err(|e| e.to_string())?;
    let cfg = state.config();
    let dir_name = cfg.general.attachments_dir.trim();
    let dir_name = if dir_name.is_empty() {
        shiki_config::config::default_attachments_dir()
    } else {
        dir_name.to_string()
    };
    let dir = nb.path.join(&dir_name);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let stem = format!("pasted-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let file = unique_file(&dir, &stem);
    std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;

    let file_name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(PastedImage {
        path: file.to_string_lossy().into_owned(),
        markdown_link: format!("![{stem}]({dir_name}/{file_name})"),
    })
}

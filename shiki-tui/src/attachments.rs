//! Saving clipboard images as note attachments: `Ctrl+V` in the editor
//! with an image on the clipboard writes it into the notebook's configured
//! attachments folder (`[general] attachments_dir`, resolved against the
//! notebook root) as a PNG and returns the markdown link to insert. The
//! preview already resolves image paths through note-folder → notebook
//! root → data-dir, so the written link renders everywhere in the notebook
//! with no extra bookkeeping.

use std::path::{Path, PathBuf};

/// Saves `image` (RGBA, straight from arboard) as
/// `<notebook>/<attachments_dir>/<pasted-TIMESTAMP>.png`, creating the
/// folder when needed and appending `-2`/`-3`/… on a same-second name
/// collision instead of overwriting. Returns `(absolute file path,
/// markdown link text)` — the link is relative to the *notebook root*, so
/// it resolves from any note depth via the preview's base-dir chain.
pub fn save_image(
    notebook_root: &Path,
    attachments_dir: &str,
    image: &arboard::ImageData<'_>,
) -> shiki_core::Result<(PathBuf, String)> {
    let dir_name = if attachments_dir.trim().is_empty() {
        shiki_config::config::default_attachments_dir()
    } else {
        attachments_dir.trim().to_string()
    };
    let dir = notebook_root.join(dir_name);
    std::fs::create_dir_all(&dir)?;

    let stem = format!("pasted-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let file = unique_file(&dir, &stem);

    encode_png(&file, image)?;

    let file_name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok((file, format!("![{stem}](attachments/{file_name})")))
}

/// `stem.png`, or `stem-2.png`/`stem-3.png`/… past the first collision.
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

/// RGBA8 → PNG on disk. png-crate errors funnel through `Io` via
/// `Error::other` so the caller sees one message either way.
fn encode_png(path: &Path, image: &arboard::ImageData<'_>) -> shiki_core::Result<()> {
    let png_err = |e: png::EncodingError| std::io::Error::other(e.to_string());
    let file = std::fs::File::create(path)?;
    let buf = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(buf, image.width as u32, image.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(png_err)?;
    writer.write_image_data(&image.bytes).map_err(png_err)?;
    writer.finish().map_err(png_err)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_image() -> arboard::ImageData<'static> {
        arboard::ImageData {
            width: 2,
            height: 2,
            bytes: std::borrow::Cow::Owned(vec![10u8; 2 * 2 * 4]),
        }
    }

    #[test]
    fn saves_png_and_returns_notebook_relative_link() {
        let tmp = tempfile::tempdir().unwrap();

        let (path, link) = save_image(tmp.path(), "attachments", &tiny_image()).unwrap();

        assert!(path.is_file());
        assert!(path.starts_with(tmp.path().join("attachments")));
        assert_eq!(path.extension().unwrap(), "png");
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        assert!(stem.starts_with("pasted-"), "{stem}");
        // Link text carries the file name, relative to the notebook root.
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(link, format!("![{stem}](attachments/{name})"));
    }

    #[test]
    fn colliding_names_get_a_numeric_suffix_not_an_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("pasted.png"), b"occupied").unwrap();

        let second = unique_file(tmp.path(), "pasted");

        assert_eq!(second, tmp.path().join("pasted-2.png"));
        // And the chain keeps going.
        std::fs::write(&second, b"occupied too").unwrap();
        assert_eq!(
            unique_file(tmp.path(), "pasted"),
            tmp.path().join("pasted-3.png")
        );
    }

    #[test]
    fn empty_dir_name_falls_back_to_attachments() {
        let tmp = tempfile::tempdir().unwrap();
        let (path, _) = save_image(tmp.path(), "   ", &tiny_image()).unwrap();
        assert!(path.starts_with(tmp.path().join("attachments")));
    }
}

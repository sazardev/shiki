//! A pluggable filesystem-access capability — `LocalFs` just calls the
//! `std::fs` functions of the same name; a future non-native consumer (no
//! local disk — a browser/WASM context, a remote-storage-backed notebook)
//! can implement this instead. Unlike `vcs::VcsPort` (bottlenecked through
//! exactly two call sites in `NotebookStore`), plain filesystem access is
//! spread across many functions in several files with no existing single
//! seam — see each file's own doc comments for how each one adopts this:
//! `Notebook`/`NotebookStore` (notebook.rs) and `Note` (note.rs) carry an
//! injected `fs` field, same shape as `vcs`/`crypto`, so their own public
//! method signatures don't change at all. Free functions with no `&self` to
//! carry a field on (`trash.rs`, `wikilinks.rs`, `tasks.rs`,
//! `templates.rs`, `last_capture.rs`) instead grow a `_with_fs` twin next to
//! the original — the original keeps its exact signature, unconditionally
//! calling `LocalFs`, so every existing caller in `shiki-tui`/`shiki-cli`/
//! `shiki-desktop`/`shiki-native-host` needs zero changes.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub trait FileStore: Send + Sync {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String>;
    fn write(&self, path: &Path, contents: &[u8]) -> std::io::Result<()>;
    /// Every direct child's full path — files and directories alike,
    /// callers that need to tell them apart use `is_dir` per entry, the
    /// same shape `std::fs::read_dir` + `DirEntry::path`/`is_dir` already has.
    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<PathBuf>>;
    fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;
    fn remove_file(&self, path: &Path) -> std::io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn modified(&self, path: &Path) -> std::io::Result<SystemTime>;
}

pub struct LocalFs;

impl FileStore for LocalFs {
    fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&self, path: &Path, contents: &[u8]) -> std::io::Result<()> {
        std::fs::write(path, contents)
    }

    fn read_dir(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
        Ok(std::fs::read_dir(path)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect())
    }

    fn create_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
    }

    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
        std::fs::remove_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::rename(from, to)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn modified(&self, path: &Path) -> std::io::Result<SystemTime> {
        std::fs::metadata(path)?.modified()
    }
}

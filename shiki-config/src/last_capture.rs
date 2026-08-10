use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::Result;

/// Persisted record of the most recent `shiki capture` (either kind — a
/// new note, or a `--daily` bullet append), backing `shiki capture --undo`.
/// Written by whichever path actually performed the capture (the in-TUI
/// daemon, or the CLI's standalone direct-write fallback — see
/// CLAUDE.md's Quick capture section) to the *same* file
/// (`Config::default_last_capture_path`), so `--undo` works identically
/// regardless of which path did the original capture: it just reads
/// whatever's here. A single slot, not a stack — undoing only ever
/// reverses the single most recent capture, same simplicity level as the
/// TUI's own `leader+u` (undo delete), which is also one level deep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LastCapture {
    /// A whole new note was created — undoing moves it to trash (same
    /// mechanism as a normal note delete, so it's still recoverable by
    /// hand even after undoing).
    Note { notebook: String, path: String },
    /// Text was appended as a bullet to an existing/just-created daily
    /// note — undoing strips exactly `appended` off the end of the body,
    /// and only if the body still ends with it verbatim (an edit made to
    /// the daily note in between means undo refuses rather than risking
    /// removing content the user actually meant to keep).
    DailyAppend {
        notebook: String,
        path: String,
        appended: String,
    },
}

impl LastCapture {
    /// Reads the last-capture record, if any. Any failure (missing file,
    /// unreadable, malformed TOML) is treated as "nothing to undo" rather
    /// than a real error — a corrupt/missing record just means undo has
    /// nothing to do, not that something is broken.
    pub fn load(path: &Path) -> Option<Self> {
        let contents = std::fs::read_to_string(path).ok()?;
        toml::from_str(&contents).ok()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Removes the record after a successful undo, so a second `--undo`
    /// reports "nothing to undo" instead of reapplying the same undo (which
    /// would be a no-op for `Note` — the file's already gone — but would
    /// otherwise re-attempt stripping `appended` a second time for
    /// `DailyAppend`, harmlessly failing the "body still ends with it"
    /// check, but with a confusing error rather than a clean "nothing to
    /// undo"). Best-effort: a failed removal doesn't fail the undo that
    /// already succeeded.
    pub fn clear(path: &Path) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "shiki-last-capture-test-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn save_then_load_round_trips_note_kind() {
        let dir = temp_path("note");
        let path = dir.join("last-capture.toml");
        let record = LastCapture::Note {
            notebook: "personal".into(),
            path: "/tmp/capture-1.md".into(),
        };
        record.save(&path).expect("save must succeed");
        assert_eq!(LastCapture::load(&path), Some(record));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_then_load_round_trips_daily_append_kind() {
        let dir = temp_path("daily");
        let path = dir.join("last-capture.toml");
        let record = LastCapture::DailyAppend {
            notebook: "personal".into(),
            path: "/tmp/2026-08-10-daily.md".into(),
            appended: "- buy milk\n".into(),
        };
        record.save(&path).expect("save must succeed");
        assert_eq!(LastCapture::load(&path), Some(record));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_for_a_missing_file() {
        assert!(LastCapture::load(Path::new("/nonexistent/shiki-last-capture.toml")).is_none());
    }

    #[test]
    fn clear_removes_the_record() {
        let dir = temp_path("clear");
        let path = dir.join("last-capture.toml");
        LastCapture::Note {
            notebook: "personal".into(),
            path: "/tmp/x.md".into(),
        }
        .save(&path)
        .unwrap();
        LastCapture::clear(&path);
        assert!(LastCapture::load(&path).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}

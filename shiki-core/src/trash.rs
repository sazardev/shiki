//! Delete-with-undo: a note or folder removed from a notebook is moved into
//! a trash directory instead of being permanently deleted, so a single
//! "undo" can put it right back. Not a full undo/redo history — only the
//! most recent delete is restorable (the caller, `shiki-tui`'s `App`, keeps
//! at most one delete operation's worth of trash entries in memory); older
//! trashed items simply stay on disk, unreachable from the undo keybinding
//! but not actually gone.

use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// Moves `source` (a note file or a whole folder, with everything inside
/// it) into `trash_root`, naming it `{unique_suffix}-{original file name}`
/// so a batch delete's same-named items across different folders can't
/// collide with each other in the trash. Returns the path it now lives at,
/// for `restore` to move back later.
pub fn move_to_trash(source: &Path, trash_root: &Path, unique_suffix: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(trash_root)?;
    let name = source
        .file_name()
        .ok_or_else(|| Error::NoteNotFound(source.display().to_string()))?;
    let dest = trash_root.join(format!("{unique_suffix}-{}", name.to_string_lossy()));
    std::fs::rename(source, &dest)?;
    Ok(dest)
}

/// Moves a previously-trashed item back to `original_path`, recreating any
/// parent directories that no longer exist (e.g. the folder it used to live
/// in was itself deleted or renamed in the meantime).
pub fn restore(trash_path: &Path, original_path: &Path) -> Result<()> {
    if let Some(parent) = original_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(trash_path, original_path)?;
    Ok(())
}

/// Permanently removes every trashed item older than `days`, across every
/// notebook's subdirectory of `trash_root` — called once at startup
/// (`config.general.trash_retention_days`, `0` meaning "never", the
/// default) rather than on a timer, since the TUI has no background
/// scheduler and a note deleted 5 minutes ago obviously isn't due for
/// purging regardless of how long the app then stays open.
///
/// Age comes from the `{unix_millis}-{original name}` prefix `move_to_trash`
/// already names every entry with, not filesystem mtime — a trashed item's
/// mtime is whatever it was before the move (rename preserves it on most
/// filesystems), which would read as "already old" the instant something
/// old gets deleted, not from when it was actually trashed.
///
/// Best-effort: an unreadable trash root, a sibling entry that isn't
/// actually a notebook subdirectory, or a single item that fails to
/// remove are all silently skipped rather than aborting the whole sweep —
/// this runs unconditionally on every startup, so it can't be allowed to
/// block launch over a single stray/permission-denied file. Returns how
/// many items were actually removed, for an optional status message.
pub fn purge_older_than(trash_root: &Path, days: u32) -> usize {
    if days == 0 {
        return 0;
    }
    let cutoff_millis = chrono::Local::now().timestamp_millis() - (days as i64) * 86_400_000;
    let Ok(notebook_dirs) = std::fs::read_dir(trash_root) else {
        return 0;
    };
    let mut removed = 0;
    for notebook_dir in notebook_dirs.flatten() {
        let Ok(entries) = std::fs::read_dir(notebook_dir.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Some((millis_str, _)) = name.split_once('-') else {
                continue;
            };
            let Ok(millis) = millis_str.parse::<i64>() else {
                continue;
            };
            if millis > cutoff_millis {
                continue;
            }
            let path = entry.path();
            let result = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            if result.is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_to_trash_then_restore_round_trips_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let original = tmp.path().join("notebook/note.md");
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::write(&original, "body").unwrap();
        let trash_root = tmp.path().join("trash/notebook");

        let trashed = move_to_trash(&original, &trash_root, "1700000000-0").unwrap();

        assert!(!original.exists());
        assert!(trashed.exists());
        assert_eq!(std::fs::read_to_string(&trashed).unwrap(), "body");

        restore(&trashed, &original).unwrap();

        assert!(original.exists());
        assert!(!trashed.exists());
        assert_eq!(std::fs::read_to_string(&original).unwrap(), "body");
    }

    #[test]
    fn move_to_trash_moves_a_whole_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let original = tmp.path().join("notebook/scratch");
        std::fs::create_dir_all(&original).unwrap();
        std::fs::write(original.join("note.md"), "x").unwrap();
        let trash_root = tmp.path().join("trash/notebook");

        let trashed = move_to_trash(&original, &trash_root, "1700000000-0").unwrap();

        assert!(!original.exists());
        assert!(trashed.join("note.md").exists());
    }

    #[test]
    fn restore_recreates_missing_parent_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let original = tmp.path().join("notebook/folder-that-got-removed/note.md");
        let trash_root = tmp.path().join("trash/notebook");
        std::fs::create_dir_all(&trash_root).unwrap();
        let trashed = trash_root.join("1700000000-0-note.md");
        std::fs::write(&trashed, "body").unwrap();

        restore(&trashed, &original).unwrap();

        assert_eq!(std::fs::read_to_string(&original).unwrap(), "body");
    }

    #[test]
    fn purge_older_than_removes_only_entries_past_the_cutoff() {
        let tmp = tempfile::tempdir().unwrap();
        let trash_root = tmp.path().join("trash");
        let nb_dir = trash_root.join("notebook");
        std::fs::create_dir_all(&nb_dir).unwrap();

        let now = chrono::Local::now().timestamp_millis();
        let old_millis = now - 10 * 86_400_000; // 10 days ago
        let recent_millis = now - 86_400_000; // 1 day ago

        let old_entry = nb_dir.join(format!("{old_millis}-old.md"));
        let recent_entry = nb_dir.join(format!("{recent_millis}-recent.md"));
        std::fs::write(&old_entry, "old").unwrap();
        std::fs::write(&recent_entry, "recent").unwrap();

        let removed = purge_older_than(&trash_root, 5);

        assert_eq!(removed, 1);
        assert!(!old_entry.exists());
        assert!(recent_entry.exists());
    }

    #[test]
    fn purge_older_than_is_a_no_op_when_retention_is_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let trash_root = tmp.path().join("trash");
        let nb_dir = trash_root.join("notebook");
        std::fs::create_dir_all(&nb_dir).unwrap();
        let ancient = nb_dir.join("0-ancient.md");
        std::fs::write(&ancient, "x").unwrap();

        assert_eq!(purge_older_than(&trash_root, 0), 0);
        assert!(ancient.exists());
    }

    #[test]
    fn distinct_unique_suffixes_avoid_collisions_for_same_named_items() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("nb/a/note.md");
        let b = tmp.path().join("nb/b/note.md");
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let trash_root = tmp.path().join("trash/nb");

        let trashed_a = move_to_trash(&a, &trash_root, "1700000000-0").unwrap();
        let trashed_b = move_to_trash(&b, &trash_root, "1700000000-1").unwrap();

        assert_ne!(trashed_a, trashed_b);
        assert_eq!(std::fs::read_to_string(&trashed_a).unwrap(), "a");
        assert_eq!(std::fs::read_to_string(&trashed_b).unwrap(), "b");
    }
}

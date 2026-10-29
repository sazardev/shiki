use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};

/// Watches one notebook directory so edits made outside shiki (another
/// editor, a `git pull` from a shell, a sync tool) show up without a manual
/// reload. Events are collapsed to a bare "something changed" signal — the
/// app re-lists the directory itself, so the payload would be redundant.
pub struct FsWatcher {
    // Held only to keep the OS watch alive; dropping it stops the events.
    _watcher: RecommendedWatcher,
    rx: Receiver<()>,
    pub path: PathBuf,
}

impl FsWatcher {
    /// `None` when the watch can't be set up (path vanished, inotify limit
    /// reached) — live reload is a convenience, never worth failing over.
    pub fn new(path: &Path) -> Option<Self> {
        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if event.paths.iter().any(|p| is_relevant(p)) {
                    let _ = tx.send(());
                }
            }
        })
        .ok()?;
        watcher.watch(path, RecursiveMode::Recursive).ok()?;
        Some(Self {
            _watcher: watcher,
            rx,
            path: path.to_path_buf(),
        })
    }

    /// Drains every pending signal; true if at least one arrived.
    pub fn take_changed(&self) -> bool {
        let mut changed = false;
        while self.rx.try_recv().is_ok() {
            changed = true;
        }
        changed
    }
}

/// Anything under `.git` is ignored: commits/fetches churn it constantly and
/// the app already refreshes git state itself after its own operations.
fn is_relevant(path: &Path) -> bool {
    !path
        .components()
        .any(|c| matches!(c, Component::Normal(name) if name == ".git"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_internals_are_ignored() {
        assert!(!is_relevant(Path::new("/nb/.git/index")));
        assert!(!is_relevant(Path::new("/nb/.git")));
        assert!(is_relevant(Path::new("/nb/notes/a.md")));
        assert!(is_relevant(Path::new("/nb/.gitignore")));
    }
}

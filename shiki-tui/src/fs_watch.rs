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
                if is_change_event(&event) {
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

/// Whether an event represents a real change worth reloading for.
///
/// `notify`'s inotify backend watches `OPEN` as well as content/metadata
/// changes, and shiki itself opens every note when it lists a directory. Left
/// unfiltered, that is a feedback loop: listing notes fires `Open` events →
/// reload → listing again fires more → reload…, with the 300 ms debounce
/// turning it into a visible flicker in PREVIEW (the cache is dropped on every
/// reload). Read-side access events are non-mutating by notify's own
/// definition, so ignoring them cannot hide a real external edit —
/// `Close(Write)`, `Modify` and `Create` all still count.
fn is_change_event(event: &Event) -> bool {
    use notify::event::{AccessKind, AccessMode, EventKind};

    let read_access = matches!(
        event.kind,
        EventKind::Access(
            AccessKind::Open(_)
                | AccessKind::Read
                | AccessKind::Close(AccessMode::Read)
                | AccessKind::Any
                | AccessKind::Other
        )
    );
    !read_access && event.paths.iter().any(|p| is_relevant(p))
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

    #[test]
    fn access_events_from_our_own_reads_are_ignored() {
        use notify::event::{AccessKind, AccessMode, EventKind, ModifyKind};

        let event = |kind| Event {
            kind,
            paths: vec![PathBuf::from("/nb/notes/a.md")],
            attrs: Default::default(),
        };
        // Opening/reading a note for the NOTES list must not schedule another
        // reload — that was the feedback loop behind the PREVIEW flicker.
        assert!(!is_change_event(&event(EventKind::Access(
            AccessKind::Open(AccessMode::Read)
        ))));
        assert!(!is_change_event(&event(EventKind::Access(
            AccessKind::Open(AccessMode::Any)
        ))));
        assert!(!is_change_event(&event(EventKind::Access(
            AccessKind::Close(AccessMode::Read)
        ))));
        // A real external edit still counts, including the close-after-write
        // signal editors produce (notify maps it to Access, not Modify).
        assert!(is_change_event(&event(EventKind::Access(
            AccessKind::Close(AccessMode::Write)
        ))));
        assert!(is_change_event(&event(EventKind::Modify(
            ModifyKind::Data(notify::event::DataChange::Any)
        ))));
        assert!(is_change_event(&event(EventKind::Create(
            notify::event::CreateKind::File
        ))));
    }
}

use crate::app::App;
use shiki_config::Config;
use shiki_core::{Notebook, NotebookStore};

/// Every notebook on disk except the ones untracked via "keep files, just
/// untrack" ([`shiki_config::config::NotebookGitOverride::hidden`]) — the
/// one filter both startup (`App::new`) and `App::reload_notebooks` must go
/// through, so a notebook untracked mid-session stays untracked after a
/// restart too (startup used to skip this filter entirely, which made an
/// untracked notebook silently come back on every relaunch).
pub(crate) fn visible_notebooks(store: &NotebookStore, config: &Config) -> Vec<Notebook> {
    store
        .list()
        .unwrap_or_default()
        .into_iter()
        .filter(|nb| {
            !config
                .notebooks
                .get(&nb.name)
                .is_some_and(|over| over.hidden)
        })
        .collect()
}

/// One in-flight git operation's eventual result, sent back over
/// `App::sync_rx` from the background thread `spawn_git_op` starts. `kind`
/// tells `apply_git_op_result` which follow-up state to refresh (which
/// notebook's `GitStatus`, whether to `reload_notes`, …) — the actual git
/// work already ran on the background thread by the time this exists, this
/// is purely "what should the main thread do with the result."
pub(crate) struct GitOpResult {
    pub(crate) kind: GitOpKind,
    pub(crate) message: String,
    /// Set only when `kind` is `Pull` and the pull's outcome was
    /// `ConflictsPending` — `(notebook, conflicted files, branch)`, the
    /// branch carried through so the eventual merge commit message can
    /// name it. `apply_git_op_result` opens the conflict resolver modal
    /// when this is `Some` and the notebook is still the selected one.
    pub(crate) conflict: Option<(String, Vec<std::path::PathBuf>, String)>,
}

pub(crate) enum GitOpKind {
    /// Commit (+ maybe push) for one notebook — manual `s`/`u`, or the
    /// automatic every-N-changes trigger.
    Sync { notebook: String },
    /// Pull for one notebook — manual `p`.
    Pull { notebook: String },
    /// Pull for every notebook that has a remote — manual `P`.
    PullAll,
    /// Render one notebook to PDF (`shiki_core::publish`) — leader+`P`.
    /// Unlike `Sync`/`Pull`, a publish never changes note content or git
    /// status, so `apply_git_op_result`'s arm for this is a no-op beyond the
    /// status message itself.
    Publish,
}

impl App {
    pub(crate) fn reload_notebooks(&mut self) {
        // A notebook "deleted" via the keep-files/just-untrack choice (see
        // `App::handle_delete_notebook_confirm_key`) still fully exists on
        // disk and in `self.store` — it's excluded by `visible_notebooks`
        // (the same filter startup uses), so it stops showing up anywhere
        // the notebook list is used, without needing its own invalidation
        // logic anywhere else.
        let visible = visible_notebooks(&self.store, &self.config);
        self.notebooks = visible
            .into_iter()
            // Attaches whatever passphrase is cached for an encrypted
            // notebook (`resolved_notebook_crypto` — `None` if it isn't
            // encrypted, or if nothing's been typed in yet this session).
            .map(|nb| {
                let crypto = self.resolved_notebook_crypto(&nb.name);
                nb.with_crypto(crypto)
            })
            .collect();
        if self.notebooks.is_empty() {
            self.selected_notebook = 0;
        } else {
            self.selected_notebook = self.selected_notebook.min(self.notebooks.len() - 1);
        }
        self.notes_path.clear();
        self.reload_notes();
    }

    /// Shared by `reload_notes`/`refresh_notes_preserve_selection`: lists
    /// `relative` within the selected notebook, and — unlike the plain
    /// `.ok()`/`unwrap_or_default()` this used to be — actually
    /// distinguishes "an encrypted notebook with no/wrong passphrase
    /// cached" from every other failure (missing notebook, I/O error),
    /// since only that one case has a UI response worth taking
    /// (`maybe_prompt_for_notebook_passphrase`) rather than just showing an
    /// empty list.
    fn list_dir_or_prompt_passphrase(
        &mut self,
        relative: &std::path::Path,
    ) -> (Vec<String>, Vec<shiki_core::Note>) {
        let Some(nb) = self.selected_notebook().cloned() else {
            return (Vec::new(), Vec::new());
        };
        match nb.list_dir(relative) {
            Ok(result) => result,
            Err(shiki_core::Error::Encryption(_)) => {
                self.maybe_prompt_for_notebook_passphrase();
                (Vec::new(), Vec::new())
            }
            Err(_) => (Vec::new(), Vec::new()),
        }
    }

    /// Re-lists the current path (`notes_path`) within the selected
    /// notebook and resets the selection to the top — for a notebook switch
    /// or a folder change, where "resume where you were" doesn't apply.
    pub(crate) fn reload_notes(&mut self) {
        let relative = self.notes_relative_path();
        let (folders, notes) = self.list_dir_or_prompt_passphrase(&relative);
        self.folders = folders;
        self.notes = notes;
        self.apply_sort();
        self.selected_note = 0;
        self.preview_scroll = 0;
        self.folder_preview_cache = None;
        self.note_preview_cache = None;
        self.tag_index_cache = None;
        self.refresh_git_status();
    }

    /// Like `reload_notes`, but keeps the same note selected (by slug) instead
    /// of resetting to the top — used after an in-place edit rather than a
    /// notebook/folder switch, so the cursor doesn't jump around underneath you.
    pub(crate) fn refresh_notes_preserve_selection(&mut self) {
        let stem = self.selected_note().map(|n| n.file_stem());
        let relative = self.notes_relative_path();
        let (folders, notes) = self.list_dir_or_prompt_passphrase(&relative);
        self.folders = folders;
        self.notes = notes;
        self.apply_sort();
        if let Some(stem) = stem {
            if let Some(idx) = self.notes.iter().position(|n| n.file_stem() == stem) {
                self.selected_note = self.folders.len() + idx;
            }
        }
        self.folder_preview_cache = None;
        // Also covers the case this cache exists for: same note path, body
        // changed underneath it (revert, external edit, inline edit save —
        // every caller of this function). The colors-in-the-key check alone
        // wouldn't catch that, since neither the path nor the theme changed.
        self.note_preview_cache = None;
        self.tag_index_cache = None;
        self.refresh_git_status();
    }

    pub(crate) fn refresh_git_status(&mut self) {
        self.git_status = self
            .selected_notebook()
            .map(|nb| shiki_core::git::status(&nb.path, &self.config.git.remote))
            .unwrap_or_default();
        self.merge_active = self
            .selected_notebook()
            .is_some_and(|nb| shiki_core::git::merge_in_progress(&nb.path));
        self.note_statuses = self
            .selected_notebook()
            .and_then(|nb| shiki_core::git::file_statuses(&nb.path).ok())
            .unwrap_or_default();
        if self.show_drawer {
            self.refresh_drawer_statuses();
        }
    }

    /// Populates `drawer_statuses` for every notebook, not just the
    /// selected one — called when the drawer opens and again whenever
    /// `refresh_git_status` runs while it's open, since any sync/push/pull
    /// can change another notebook's status too (e.g. `pull_all_notebooks`).
    /// `shiki_core::git::status` is local-only (no network), cheap enough
    /// to redo for every notebook on these discrete events rather than
    /// needing its own per-notebook cache.
    pub(crate) fn refresh_drawer_statuses(&mut self) {
        self.drawer_statuses = self
            .notebooks
            .iter()
            .map(|nb| {
                (
                    nb.name.clone(),
                    shiki_core::git::status(&nb.path, &self.config.git.remote),
                )
            })
            .collect();
    }

    pub(crate) fn sync_notebook(&mut self) {
        let Some(nb) = self.selected_notebook().cloned() else {
            self.set_status("no notebook selected".into());
            return;
        };
        let auto_push = self.config.sync_for(&nb.name).auto_push;
        let commit_prefix = self.config.git.commit_prefix.clone();
        let remote = self.config.git.remote.clone();
        let (nb_name, nb_path) = (nb.name.clone(), nb.path.clone());
        self.spawn_git_op(nb_name.clone(), move || {
            let message =
                Self::run_sync_blocking(&nb_path, &commit_prefix, false, auto_push, &remote);
            GitOpResult {
                kind: GitOpKind::Sync { notebook: nb_name },
                message,
                conflict: None,
            }
        });
    }

    /// Commits (message auto-built from the diff, naming the actual files
    /// when there are only a few, e.g. "shiki: added (First note.md)") and
    /// pushes if `force_push` is set or `auto_push` is on. Shared by manual
    /// `s` (`force_push: false` — respects the configured policy), manual
    /// `u` (`force_push: true` — always pushes right now regardless of
    /// policy), and the automatic every-N-changes trigger (`note_changed`,
    /// `force_push: false`). Every step is reported explicitly (commit
    /// outcome, then push outcome including remote-side verification)
    /// rather than a terse "done" — push failures (no internet, auth,
    /// rejected by the remote, etc.) are surfaced, never panic: the commit
    /// already succeeded either way, so nothing pending is lost, and the
    /// next attempt just retries the push.
    ///
    /// Takes only owned data, no `&self`/`&App` — this runs on a background
    /// thread (`spawn_git_op`), so it can't borrow from the `App` that
    /// spawned it. Each call site resolves `Config::sync_for`'s
    /// `auto_push` on the main thread first and passes the plain `bool`.
    fn run_sync_blocking(
        nb_path: &std::path::Path,
        commit_prefix: &str,
        force_push: bool,
        auto_push: bool,
        remote: &str,
    ) -> String {
        let summary =
            shiki_core::git::diff_summary(nb_path).unwrap_or_else(|_| "changes".to_string());
        let message = format!("{commit_prefix}{summary}");
        let mut parts = Vec::new();
        match shiki_core::git::commit_all(nb_path, &message) {
            Ok(true) => parts.push(format!("committed: {summary}")),
            Ok(false) => parts.push("no changes to commit".to_string()),
            Err(e) => parts.push(format!("commit error: {e}")),
        }
        if force_push || auto_push {
            if shiki_core::git::remote_url(nb_path).is_none() {
                parts.push("no remote configured (press R, then s)".to_string());
            } else {
                match shiki_core::git::push(nb_path, remote) {
                    Ok(()) => parts.push("pushed and confirmed by remote".to_string()),
                    Err(e) => parts.push(format!("push error: {e}")),
                }
            }
        }
        parts.join("; ")
    }

    /// Commits (same as `s`) and always pushes, regardless of the resolved
    /// `auto_push` policy — the explicit "sync right now" override, for
    /// pushing without waiting on `auto_sync`'s threshold or turning
    /// `auto_push` on globally.
    pub(crate) fn push_notebook(&mut self) {
        let Some(nb) = self.selected_notebook().cloned() else {
            self.set_status("no notebook selected".into());
            return;
        };
        let commit_prefix = self.config.git.commit_prefix.clone();
        let remote = self.config.git.remote.clone();
        let (nb_name, nb_path) = (nb.name.clone(), nb.path.clone());
        self.spawn_git_op(nb_name.clone(), move || {
            let message = Self::run_sync_blocking(&nb_path, &commit_prefix, true, false, &remote);
            GitOpResult {
                kind: GitOpKind::Sync {
                    notebook: nb_name.clone(),
                },
                message: format!("'{nb_name}': {message}"),
                conflict: None,
            }
        });
    }

    /// Call after any note create/edit/rename/delete/move: bumps
    /// `notebook_name`'s pending-change count and, if `auto_sync` is on for
    /// it (`Config::sync_for`) and the count reaches `auto_sync_every`,
    /// syncs immediately and resets the counter. A no-op notebook whose
    /// policy has `auto_sync` off, so this is cheap to call unconditionally.
    pub(crate) fn note_changed(&mut self, notebook_name: &str) {
        let sync = self.config.sync_for(notebook_name);
        if !sync.auto_sync {
            return;
        }
        let count = self
            .pending_changes
            .entry(notebook_name.to_string())
            .or_insert(0);
        *count += 1;
        let reached = *count >= sync.auto_sync_every.max(1);
        if !reached {
            return;
        }
        self.pending_changes.insert(notebook_name.to_string(), 0);

        let Some(nb) = self
            .notebooks
            .iter()
            .find(|nb| nb.name == notebook_name)
            .cloned()
        else {
            return;
        };
        let commit_prefix = self.config.git.commit_prefix.clone();
        let remote = self.config.git.remote.clone();
        let auto_push = sync.auto_push;
        let (nb_name, nb_path) = (nb.name.clone(), nb.path.clone());
        self.spawn_git_op(nb_name.clone(), move || {
            let message =
                Self::run_sync_blocking(&nb_path, &commit_prefix, false, auto_push, &remote);
            GitOpResult {
                kind: GitOpKind::Sync {
                    notebook: nb_name.clone(),
                },
                message: format!("auto-sync '{nb_name}': {message}"),
                conflict: None,
            }
        });
    }

    pub(crate) fn pull_notebook(&mut self) {
        let Some(nb) = self.selected_notebook().cloned() else {
            self.set_status("no notebook selected".into());
            return;
        };
        // A previous pull on this notebook already left it mid-merge
        // (conflicts unresolved, or the resolver was just closed with
        // `Esc` rather than finished/aborted) — `p` reopens the resolver
        // instead of attempting a second pull on top of an unfinished one,
        // so closing the modal is never a dead end.
        if shiki_core::git::merge_in_progress(&nb.path) {
            self.reopen_conflicts_if_merging();
            return;
        }
        // Check upfront rather than letting git2 fail with a generic
        // "remote 'origin' does not exist" — that error doesn't say which
        // notebook it's about, and is easy to hit by accident: `p` pulls
        // whichever notebook is currently selected, which after switching
        // notebooks or relaunching may not be the one a remote was set on.
        if shiki_core::git::remote_url(&nb.path).is_none() {
            self.set_status(format!(
                "'{}' has no remote configured — press R to set one, then p to pull",
                nb.name
            ));
            return;
        }
        let remote = self.config.git.remote.clone();
        let configured_branch = self.config.git.branch.clone();
        let (nb_name, nb_path) = (nb.name.clone(), nb.path.clone());
        self.spawn_git_op(nb_name.clone(), move || {
            use shiki_core::git::PullOutcome;
            let mut conflict = None;
            let message = match shiki_core::git::pull(&nb_path, &remote, &configured_branch) {
                Ok(PullOutcome::FastForwarded { branch }) if branch == configured_branch => {
                    format!("pulled '{nb_name}'")
                }
                Ok(PullOutcome::FastForwarded { branch }) => format!(
                    "pulled '{nb_name}' (remote's default branch is '{branch}', not '{configured_branch}')"
                ),
                Ok(PullOutcome::UpToDate { .. }) => format!("'{nb_name}' already up to date"),
                Ok(PullOutcome::NewRepo { .. }) => format!("pulled '{nb_name}'"),
                Ok(PullOutcome::MergedClean { .. }) => {
                    format!("'{nb_name}': merged cleanly, no conflicts")
                }
                Ok(PullOutcome::ConflictsPending { branch, files }) => {
                    let n = files.len();
                    conflict = Some((nb_name.clone(), files, branch));
                    format!(
                        "'{nb_name}': pull has {n} conflicting file{} — resolve them",
                        if n == 1 { "" } else { "s" }
                    )
                }
                Err(e) => format!("pull error ('{nb_name}'): {e}"),
            };
            GitOpResult {
                kind: GitOpKind::Pull { notebook: nb_name },
                message,
                conflict,
            }
        });
    }

    /// Pulls every notebook that has a remote configured. Unlike
    /// `pull_notebook`, this never auto-opens the conflict resolver — with
    /// several notebooks in flight at once there's no single "the" notebook
    /// to show a modal for. A notebook that comes back `ConflictsPending`
    /// is counted separately and named in the summary message; resolving it
    /// is a manual follow-up (select that notebook, press `p` again — the
    /// second pull re-attempts the same merge and reports the conflict the
    /// normal single-notebook way).
    pub(crate) fn pull_all_notebooks(&mut self) {
        let remote = self.config.git.remote.clone();
        let branch = self.config.git.branch.clone();
        let notebooks = self.notebooks.clone();
        self.spawn_git_op("all notebooks".to_string(), move || {
            use shiki_core::git::PullOutcome;
            let (mut ok, mut failed, mut conflicted) = (0u32, 0u32, Vec::new());
            for nb in notebooks {
                match shiki_core::git::pull(&nb.path, &remote, &branch) {
                    Ok(PullOutcome::ConflictsPending { .. }) => conflicted.push(nb.name.clone()),
                    Ok(_) => ok += 1,
                    Err(_) => failed += 1,
                }
            }
            let message = if conflicted.is_empty() {
                format!("pull all: {ok} ok, {failed} failed")
            } else {
                format!(
                    "pull all: {ok} ok, {failed} failed, needs conflict resolution: {}",
                    conflicted.join(", ")
                )
            };
            GitOpResult {
                kind: GitOpKind::PullAll,
                message,
                conflict: None,
            }
        });
    }

    /// The directory a PDF is saved into absent an explicit path — respects
    /// `[export].export_dir` when set, otherwise `{data_dir}/exports`
    /// (deliberately not inside the notebook's own git-tracked directory,
    /// so the rendered PDF never shows up as a stray untracked file for
    /// auto-sync to pick up).
    pub(crate) fn resolved_export_dir(&self) -> std::path::PathBuf {
        let configured = self.config.export.export_dir.trim();
        if configured.is_empty() {
            self.store.root.join("exports")
        } else {
            std::path::PathBuf::from(configured)
        }
    }

    /// leader+`P` — renders the selected notebook to a themed PDF via
    /// `pretty-pdf` (downloaded/cached automatically on first use, see
    /// `shiki_core::publish::ensure_binary`) and opens the result. Reuses
    /// `spawn_git_op`'s single-job-at-a-time guard and footer spinner rather
    /// than inventing a second "something's running" indicator — a publish
    /// and a sync are both "one background job at a time" from the user's
    /// point of view. When `[export].ask_export_path` is on, prompts for the
    /// exact save path first (`App::start_publish_path_prompt`) instead of
    /// silently resolving one here.
    pub(crate) fn publish_notebook(&mut self) {
        if self.config.export.ask_export_path {
            self.start_publish_path_prompt();
            return;
        }
        let Some(nb) = self.selected_notebook().cloned() else {
            self.set_status("no notebook selected".into());
            return;
        };
        let out = self.resolved_export_dir().join(format!("{}.pdf", nb.name));
        self.publish_notebook_to(nb, out);
    }

    /// Shared tail of `publish_notebook` and the `PendingInput::PublishPath`
    /// confirm handler — both end up here once the output path is decided,
    /// one way (silent default) or the other (typed prompt).
    pub(crate) fn publish_notebook_to(
        &mut self,
        nb: shiki_core::Notebook,
        out: std::path::PathBuf,
    ) {
        let mut notes = match nb.all_notes_recursive() {
            Ok(notes) => notes,
            Err(e) => {
                self.set_status(format!("publish error: {e}"));
                return;
            }
        };
        notes.sort_by(|a, b| {
            a.frontmatter
                .date
                .cmp(&b.frontmatter.date)
                .then_with(|| a.frontmatter.title.cmp(&b.frontmatter.title))
        });
        let theme = self.config.export.pdf_theme.clone();
        let cache_dir = self.store.root.join("bin");
        let nb_name = nb.name.clone();
        self.spawn_git_op(nb_name.clone(), move || {
            let message = match shiki_core::publish::publish(&notes, &theme, &cache_dir, &out) {
                Ok(()) => {
                    // Fire-and-forget, same as the footer's Buy Me a Coffee
                    // link — a failed open (headless SSH, no GUI) is still a
                    // successful publish, just one the user has to find the
                    // file for themselves via the path in this message.
                    let _ = shiki_core::browser::open_url(&out.to_string_lossy());
                    format!("published '{nb_name}' to {}", out.display())
                }
                Err(e) => format!("publish error ('{nb_name}'): {e}"),
            };
            GitOpResult {
                kind: GitOpKind::Publish,
                message,
                conflict: None,
            }
        });
    }

    /// Spawns `op` on a background thread and sends its result back over
    /// `sync_rx`, so the caller (any of the git actions above) never blocks
    /// the render loop on a network call — same `std::thread` + `mpsc`
    /// shape as the self-updater's `open_update_check`. Only one operation
    /// runs at a time, globally (same simplicity level the self-updater
    /// already has, not per-notebook concurrent tracking) — a second
    /// request while one's in flight is reported and dropped rather than
    /// queued, since the first one will pick up anything pending anyway.
    fn spawn_git_op(&mut self, label: String, op: impl FnOnce() -> GitOpResult + Send + 'static) {
        if self.sync_in_flight.is_some() {
            self.set_status("a sync is already running, try again in a moment".into());
            return;
        }
        self.sync_in_flight = Some(label);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(op());
        });
        self.sync_rx = Some(rx);
    }

    /// Non-blocking: called once per `run()` loop iteration, same spot and
    /// pattern as `poll_update_channel`.
    pub(crate) fn poll_sync_channel(&mut self) {
        let Some(rx) = &self.sync_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                self.sync_in_flight = None;
                self.sync_rx = None;
                self.apply_git_op_result(result);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.sync_in_flight = None;
                self.sync_rx = None;
            }
        }
    }

    /// What each call site's synchronous code used to do immediately after
    /// `run_sync`/`pull` returned — now deferred until the result actually
    /// arrives over `sync_rx`. The "still selected?" check on `Sync`/`Pull`
    /// is a real correctness addition, not just carried over: the operation
    /// runs in the background now, so the user could have switched to a
    /// different notebook while it was in flight, and this must not stomp
    /// on `git_status`/`notes` for whatever's selected *now* with a result
    /// about a notebook that isn't selected anymore.
    fn apply_git_op_result(&mut self, result: GitOpResult) {
        self.set_status(result.message);
        match result.kind {
            GitOpKind::Sync { notebook } => {
                self.pending_changes.insert(notebook.clone(), 0);
                // A new commit may have changed the currently-previewed
                // note's revision count — force the footer's cache to
                // recompute instead of showing a stale number.
                self.history_count_cache = None;
                if self.selected_notebook().map(|n| n.name.as_str()) == Some(notebook.as_str()) {
                    self.refresh_git_status();
                }
            }
            GitOpKind::Pull { notebook } => {
                if self.selected_notebook().map(|n| n.name.as_str()) == Some(notebook.as_str()) {
                    // Same fix as `PullAll` below: re-list from disk without
                    // resetting the selection/scroll the user may have since
                    // moved to a different note/folder *within this same
                    // notebook* while the pull was in flight.
                    self.refresh_notes_preserve_selection();
                    if let Some((conflict_notebook, files, branch)) = result.conflict {
                        self.conflict_notebook = conflict_notebook;
                        self.conflict_files = files;
                        self.conflict_branch = branch;
                        self.conflict_selected = 0;
                        self.conflict_viewing = None;
                        self.show_conflicts = true;
                    }
                }
            }
            // Unlike `Sync`/`Pull`, this doesn't know which specific
            // notebook(s) actually changed (just an aggregate ok/failed
            // count) — so it can't skip refreshing outright the way those
            // two do when the background op wasn't about the currently
            // selected notebook. But it still shouldn't *reset* the
            // selection: `reload_notes` always jumps back to index 0 and
            // clears scroll position, which used to fire unconditionally
            // here even when the user had since navigated to a different
            // note/folder entirely while the pull was in flight.
            // `refresh_notes_preserve_selection` re-lists from disk the same
            // way but keeps the same note selected (by slug) instead.
            GitOpKind::PullAll => self.refresh_notes_preserve_selection(),
            // Nothing to refresh — publishing never touches note content or
            // git status, the status message set above is the whole result.
            GitOpKind::Publish => {}
        }
        if self.show_drawer {
            self.refresh_drawer_statuses();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::visible_notebooks;
    use shiki_config::config::NotebookGitOverride;
    use shiki_config::Config;
    use shiki_core::NotebookStore;

    #[test]
    fn visible_notebooks_excludes_hidden_but_keeps_the_rest() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().to_path_buf());
        store.create("personal").unwrap();
        store.create("work").unwrap();

        // No overrides at all — everything on disk shows up.
        let config = Config::default();
        let names: Vec<String> = visible_notebooks(&store, &config)
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(names, vec!["personal".to_string(), "work".to_string()]);

        // Untracking "work" ("keep files, just untrack") hides exactly that
        // one — the filter startup and reload_notebooks must agree on.
        let mut config = Config::default();
        config.notebooks.insert(
            "work".to_string(),
            NotebookGitOverride {
                hidden: true,
                ..Default::default()
            },
        );
        let names: Vec<String> = visible_notebooks(&store, &config)
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(names, vec!["personal".to_string()]);
    }

    #[test]
    fn visible_notebooks_treats_an_override_without_hidden_as_visible() {
        // A per-notebook override that only tunes sync policy (the common
        // [notebooks.<name>] table) must not hide the notebook.
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().to_path_buf());
        store.create("personal").unwrap();

        let mut config = Config::default();
        config.notebooks.insert(
            "personal".to_string(),
            NotebookGitOverride {
                auto_push: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(visible_notebooks(&store, &config).len(), 1);
    }
}

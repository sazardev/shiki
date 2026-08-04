use crate::app::App;

/// One in-flight git operation's eventual result, sent back over
/// `App::sync_rx` from the background thread `spawn_git_op` starts. `kind`
/// tells `apply_git_op_result` which follow-up state to refresh (which
/// notebook's `GitStatus`, whether to `reload_notes`, …) — the actual git
/// work already ran on the background thread by the time this exists, this
/// is purely "what should the main thread do with the result."
pub(crate) struct GitOpResult {
    pub(crate) kind: GitOpKind,
    pub(crate) message: String,
}

pub(crate) enum GitOpKind {
    /// Commit (+ maybe push) for one notebook — manual `s`/`u`, or the
    /// automatic every-N-changes trigger.
    Sync { notebook: String },
    /// Pull for one notebook — manual `p`.
    Pull { notebook: String },
    /// Pull for every notebook that has a remote — manual `P`.
    PullAll,
}

impl App {
    pub(crate) fn reload_notebooks(&mut self) {
        // A notebook "deleted" via the keep-files/just-untrack choice (see
        // `App::handle_delete_notebook_confirm_key`) still fully exists on
        // disk and in `self.store` — it's excluded here, and only here, so
        // it stops showing up anywhere the notebook list is used, without
        // needing its own invalidation logic anywhere else.
        self.notebooks = self
            .store
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|nb| {
                !self
                    .config
                    .notebooks
                    .get(&nb.name)
                    .is_some_and(|over| over.hidden)
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

    /// Re-lists the current path (`notes_path`) within the selected
    /// notebook and resets the selection to the top — for a notebook switch
    /// or a folder change, where "resume where you were" doesn't apply.
    pub(crate) fn reload_notes(&mut self) {
        let relative = self.notes_relative_path();
        let (folders, notes) = self
            .selected_notebook()
            .and_then(|nb| nb.list_dir(&relative).ok())
            .unwrap_or_default();
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
        let (folders, notes) = self
            .selected_notebook()
            .and_then(|nb| nb.list_dir(&relative).ok())
            .unwrap_or_default();
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
            }
        });
    }

    pub(crate) fn pull_notebook(&mut self) {
        let Some(nb) = self.selected_notebook().cloned() else {
            self.set_status("no notebook selected".into());
            return;
        };
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
            let message = match shiki_core::git::pull(&nb_path, &remote, &configured_branch) {
                Ok(actual_branch) if actual_branch == configured_branch => {
                    format!("pulled '{nb_name}'")
                }
                Ok(actual_branch) => format!(
                    "pulled '{nb_name}' (remote's default branch is '{actual_branch}', not '{configured_branch}')"
                ),
                Err(e) => format!("pull error ('{nb_name}'): {e}"),
            };
            GitOpResult {
                kind: GitOpKind::Pull { notebook: nb_name },
                message,
            }
        });
    }

    pub(crate) fn pull_all_notebooks(&mut self) {
        let remote = self.config.git.remote.clone();
        let branch = self.config.git.branch.clone();
        let notebooks = self.notebooks.clone();
        self.spawn_git_op("all notebooks".to_string(), move || {
            let (mut ok, mut failed) = (0u32, 0u32);
            for nb in notebooks {
                match shiki_core::git::pull(&nb.path, &remote, &branch) {
                    Ok(_) => ok += 1,
                    Err(_) => failed += 1,
                }
            }
            GitOpResult {
                kind: GitOpKind::PullAll,
                message: format!("pull all: {ok} ok, {failed} failed"),
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
        }
        if self.show_drawer {
            self.refresh_drawer_statuses();
        }
    }
}

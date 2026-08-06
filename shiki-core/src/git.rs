use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use git2::{Cred, CredentialType, IndexAddOption, RemoteCallbacks, Repository, Signature};

use crate::{Error, Result};

/// Per-file git status, for coloring individual rows in the NOTES list —
/// coarser aggregates (`GitStatus::dirty_count`, `diff_summary`) already
/// exist for the footer/commit-message use cases, this is the same
/// `repo.statuses(None)` walk bucketed per path instead of into one summary
/// string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileGitStatus {
    New,
    Modified,
    Deleted,
    Renamed,
}

/// Maps every changed file in the notebook at `path` to its status, keyed by
/// *absolute* path (`path.join(relative)`) so it matches `Note::path`
/// directly — no relative-vs-absolute conversion needed at the call site.
pub fn file_statuses(path: &Path) -> Result<HashMap<PathBuf, FileGitStatus>> {
    let repo = Repository::open(path)?;
    let statuses = repo.statuses(None)?;
    let mut map = HashMap::new();
    for entry in statuses.iter() {
        let s = entry.status();
        let Some(relative) = entry.path().ok() else {
            continue;
        };
        let status = if s.intersects(git2::Status::WT_NEW | git2::Status::INDEX_NEW) {
            FileGitStatus::New
        } else if s.intersects(git2::Status::WT_DELETED | git2::Status::INDEX_DELETED) {
            FileGitStatus::Deleted
        } else if s.intersects(git2::Status::WT_RENAMED | git2::Status::INDEX_RENAMED) {
            FileGitStatus::Renamed
        } else if s.intersects(git2::Status::WT_MODIFIED | git2::Status::INDEX_MODIFIED) {
            FileGitStatus::Modified
        } else {
            continue;
        };
        map.insert(path.join(relative), status);
    }
    Ok(map)
}

/// Builds a credentials callback that actually works for more than SSH.
///
/// The previous version unconditionally called `Cred::ssh_key_from_agent`,
/// which is meaningless for an `https://` remote — libgit2 would report that
/// as a generic "authentication required but no callback" failure, not a
/// clear "wrong credential type" error. This tries, in order: the SSH agent
/// (only when the server actually offered `SSH_KEY`), then the *system*
/// git credential helper (`Cred::credential_helper`) — which reuses whatever
/// the user's own `git`/`gh` already has stored (macOS Keychain, Windows
/// Credential Manager, libsecret, a cached PAT, …), so if plain `git clone`
/// works in their shell without prompting, this does too — then finally
/// anonymous access (works for public repos over HTTPS). A capped attempt
/// counter avoids looping forever if the server keeps rejecting every kind.
fn build_callbacks<'a>() -> RemoteCallbacks<'a> {
    let attempts = Cell::new(0u32);
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |url, username_from_url, allowed| {
        let tries = attempts.get();
        attempts.set(tries + 1);
        if tries >= 5 {
            return Err(git2::Error::from_str(
                "too many failed authentication attempts",
            ));
        }

        if allowed.contains(CredentialType::SSH_KEY) {
            if let Ok(cred) = Cred::ssh_key_from_agent(username_from_url.unwrap_or("git")) {
                return Ok(cred);
            }
        }
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT)
            || allowed.contains(CredentialType::DEFAULT)
        {
            if let Ok(config) = git2::Config::open_default() {
                if let Ok(cred) = Cred::credential_helper(&config, url, username_from_url) {
                    return Ok(cred);
                }
            }
        }
        if allowed.contains(CredentialType::USERNAME) {
            if let Some(user) = username_from_url {
                return Cred::username(user);
            }
        }
        Cred::default()
    });
    callbacks
}

/// Sync status of a notebook.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitStatus {
    pub is_repo: bool,
    pub dirty: bool,
    /// How many files have uncommitted changes (working tree + index).
    pub dirty_count: usize,
    /// Current branch (`HEAD`'s shorthand name), if `HEAD` points at one.
    pub branch: Option<String>,
    /// Local commits not yet pushed, relative to `refs/remotes/{remote}/{branch}`.
    pub ahead: usize,
    /// Remote commits not yet pulled in, relative to the same ref. Only
    /// reflects what was fetched at the last `pull`/`pull_all` — computing
    /// this doesn't itself talk to the network.
    pub behind: usize,
    /// Set when `repo.statuses(None)` itself failed (locked index,
    /// permission issue, corrupted repo) — `dirty_count` falls back to `0`
    /// in that case (see `status()`), which used to be indistinguishable
    /// from a genuinely clean notebook. `None` means the check actually
    /// succeeded, regardless of what it found.
    pub status_error: Option<String>,
}

/// Initializes a git repo at `path` if one doesn't already exist.
pub fn init_repo(path: &Path) -> Result<Repository> {
    Ok(match Repository::open(path) {
        Ok(repo) => repo,
        Err(_) => Repository::init(path)?,
    })
}

/// Stages all changes and creates a commit. Returns `false` if there was nothing to commit.
pub fn commit_all(path: &Path, message: &str) -> Result<bool> {
    let repo = init_repo(path)?;
    let mut index = repo.index()?;
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
    index.write()?;

    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;

    if let Ok(head) = repo.head() {
        if let Ok(parent_commit) = head.peel_to_commit() {
            if parent_commit.tree_id() == tree_id {
                return Ok(false); // nothing to commit
            }
        }
    }

    let signature = repo
        .signature()
        .unwrap_or_else(|_| Signature::now("shiki", "shiki@localhost").unwrap());

    let parents: Vec<_> = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .into_iter()
        .collect();
    let parent_refs: Vec<&_> = parents.iter().collect();

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parent_refs,
    )?;
    Ok(true)
}

/// A short, human-readable summary of the working tree's pending changes,
/// for building an automatic commit message and for reporting exactly what
/// happened instead of a bare count. Names the actual files when there are
/// only a few of them (`"added (First note.md)"`), falls back to a plain
/// count for a big batch so the message doesn't become unreadable
/// (`"12 updated"`). `"no changes"` if nothing is pending (the caller won't
/// normally get this far, since `commit_all` already no-ops in that case,
/// but it's a sane fallback).
pub fn diff_summary(path: &Path) -> Result<String> {
    let repo = Repository::open(path)?;
    let statuses = repo.statuses(None)?;
    let mut added = Vec::new();
    let mut updated = Vec::new();
    let mut deleted = Vec::new();
    let mut renamed = Vec::new();
    for entry in statuses.iter() {
        let s = entry.status();
        let file = entry.path().unwrap_or("?").to_string();
        if s.intersects(git2::Status::WT_NEW | git2::Status::INDEX_NEW) {
            added.push(file);
        } else if s.intersects(git2::Status::WT_DELETED | git2::Status::INDEX_DELETED) {
            deleted.push(file);
        } else if s.intersects(git2::Status::WT_RENAMED | git2::Status::INDEX_RENAMED) {
            renamed.push(file);
        } else if s.intersects(git2::Status::WT_MODIFIED | git2::Status::INDEX_MODIFIED) {
            updated.push(file);
        }
    }

    fn describe(label: &str, files: &[String]) -> Option<String> {
        match files.len() {
            0 => None,
            1..=3 => Some(format!("{label} ({})", files.join(", "))),
            n => Some(format!("{n} {label}")),
        }
    }

    let parts: Vec<String> = [
        describe("added", &added),
        describe("updated", &updated),
        describe("renamed", &renamed),
        describe("deleted", &deleted),
    ]
    .into_iter()
    .flatten()
    .collect();

    Ok(if parts.is_empty() {
        "no changes".to_string()
    } else {
        parts.join(", ")
    })
}

/// Pushes the current branch (whatever `HEAD` actually points at) to
/// `remote`, creating/updating a same-named branch there. Authenticates via
/// SSH agent (for `git@…` remotes) or the system git credential store (for
/// `https://…` remotes).
///
/// Deliberately does *not* take a configured branch name: `pull` already
/// falls back to whatever branch the remote actually uses (e.g. `master`
/// instead of the configured `main`), so the local repo's real branch can
/// differ from `config.git.branch` — pushing that fixed name unconditionally
/// used to fail with "src refspec 'refs/heads/main' does not match any
/// existing object" the moment it didn't match reality.
///
/// Actually verifies the push landed, rather than trusting `Remote::push`'s
/// `Ok(())` at face value: libgit2 reports the network round-trip
/// succeeding even when the *server* rejected the specific ref update (e.g.
/// non-fast-forward, a protected branch, a rejecting hook) — that only
/// surfaces through the `push_update_reference` callback, which is
/// registered here and turned into a real `Err` if the server sent back a
/// rejection status.
pub fn push(path: &Path, remote: &str) -> Result<()> {
    let repo = Repository::open(path)?;
    let branch = repo
        .head()?
        .shorthand()
        .map_err(|_| Error::Git(git2::Error::from_str("HEAD is not on a branch")))?
        .to_string();
    let mut remote_ref = repo.find_remote(remote)?;

    let rejected = std::cell::RefCell::new(None::<String>);
    let mut callbacks = build_callbacks();
    callbacks.push_update_reference(|refname, status| {
        if let Some(message) = status {
            *rejected.borrow_mut() = Some(format!("{refname}: {message}"));
        }
        Ok(())
    });

    let mut opts = git2::PushOptions::new();
    opts.remote_callbacks(callbacks);
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    let push_result = remote_ref.push(&[&refspec], Some(&mut opts));
    drop(opts);
    push_result?;

    if let Some(reason) = rejected.into_inner() {
        return Err(Error::Git(git2::Error::from_str(&format!(
            "push rejected by remote: {reason}"
        ))));
    }
    Ok(())
}

/// What `pull` actually did. Every variant carries the branch name that was
/// pulled — it can differ from the `branch` argument (see `pull`'s fallback
/// logic), so callers should report it rather than assuming the configured
/// name was used.
#[derive(Debug, Clone)]
pub enum PullOutcome {
    /// The local branch had no unpushed-relative-to-remote commits of its
    /// own, so it was simply moved forward to the fetched commit.
    FastForwarded { branch: String },
    /// Nothing to do — the local branch already contains everything fetched.
    UpToDate { branch: String },
    /// No local branch existed yet (a brand-new/empty notebook, or one just
    /// pointed at an existing remote for the first time) — it was created
    /// pointing straight at the fetched commit, the same initial checkout
    /// `git clone` would do. Can't conflict: there was nothing local to
    /// diverge from.
    NewRepo { branch: String },
    /// Local and remote history had diverged, and merging them landed
    /// conflicting changes to the same file(s) — `conflicted_files`,
    /// `conflict_sides`/`conflict_diff`, `resolve_conflict`, and finally
    /// `finish_merge` (or `abort_merge`) are how a caller works through
    /// this. The working tree already has the conflict reflected in it
    /// (via `repo.merge`), and `merge_in_progress` is true until resolved.
    ConflictsPending { branch: String, files: Vec<PathBuf> },
    /// Local and remote history had diverged, but the merge produced no
    /// conflicts (different files, or non-overlapping regions of the same
    /// file) — already finalized as a normal two-parent merge commit, no
    /// further action needed from the caller.
    MergedClean { branch: String },
}

impl PullOutcome {
    pub fn branch(&self) -> &str {
        match self {
            PullOutcome::FastForwarded { branch }
            | PullOutcome::UpToDate { branch }
            | PullOutcome::NewRepo { branch }
            | PullOutcome::ConflictsPending { branch, .. }
            | PullOutcome::MergedClean { branch } => branch,
        }
    }
}

/// Every path with an unresolved conflict in `index`, in the order
/// `index.conflicts()` yields them, deduplicated — an add/add or
/// modify/modify conflict can otherwise report the same path via more than
/// one side (`ancestor`/`our`/`their`).
fn conflict_paths_in_index(index: &git2::Index) -> Result<Vec<PathBuf>> {
    let mut seen = std::collections::HashSet::new();
    let mut files = Vec::new();
    for conflict in index.conflicts()? {
        let conflict = conflict?;
        let entry = conflict
            .ancestor
            .as_ref()
            .or(conflict.our.as_ref())
            .or(conflict.their.as_ref());
        if let Some(entry) = entry {
            let path = PathBuf::from(String::from_utf8_lossy(&entry.path).into_owned());
            if seen.insert(path.clone()) {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn find_conflict_entry(index: &git2::Index, file_relative: &Path) -> Result<git2::IndexConflict> {
    for conflict in index.conflicts()? {
        let conflict = conflict?;
        let matches = [&conflict.ancestor, &conflict.our, &conflict.their]
            .iter()
            .filter_map(|e| e.as_ref())
            .any(|e| Path::new(&String::from_utf8_lossy(&e.path).into_owned()) == file_relative);
        if matches {
            return Ok(conflict);
        }
    }
    Err(Error::Git(git2::Error::from_str(&format!(
        "no conflict entry for {}",
        file_relative.display()
    ))))
}

/// Finalizes an in-progress merge as a normal two-parent commit and clears
/// merge state (`repo.cleanup_state()`, which removes `.git/MERGE_HEAD`) —
/// the generalization of `commit_all`'s zero-or-one-parent commit to
/// exactly two, since a merge commit's second parent is `their` side.
/// Requires the index to have no remaining conflicts; callers check that
/// first (`pull`'s clean-merge branch never has any by construction, since
/// it's only reached when `index.has_conflicts()` is false; `finish_merge`
/// checks explicitly since a caller could call it too early).
fn finalize_merge_commit(repo: &Repository, their: &git2::Oid, message: &str) -> Result<()> {
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let signature = repo
        .signature()
        .unwrap_or_else(|_| Signature::now("shiki", "shiki@localhost").unwrap());
    let our_commit = repo.head()?.peel_to_commit()?;
    let their_commit = repo.find_commit(*their)?;
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&our_commit, &their_commit],
    )?;
    repo.cleanup_state()?;
    Ok(())
}

/// Every currently-conflicted file in the notebook at `path`, relative
/// paths — empty (not an error) when there's no merge in progress at all.
pub fn conflicted_files(path: &Path) -> Result<Vec<PathBuf>> {
    let repo = Repository::open(path)?;
    let index = repo.index()?;
    conflict_paths_in_index(&index)
}

/// Whether `path` has an in-progress merge (`.git/MERGE_HEAD` present) —
/// checked via `repo.state()` rather than reading that file directly, since
/// libgit2 already tracks and exposes this. A notebook stays in this state
/// from the moment `pull` reports `ConflictsPending` until `finish_merge` or
/// `abort_merge` resolves it, even across shiki restarts.
pub fn merge_in_progress(path: &Path) -> bool {
    Repository::open(path)
        .map(|repo| repo.state() == git2::RepositoryState::Merge)
        .unwrap_or(false)
}

/// The three sides of one conflicted file — `base` (the common ancestor),
/// `ours` (the local side), `theirs` (the fetched/remote side). Any of the
/// three can be `None`: an add/add conflict has no `base`; a modify/delete
/// conflict is missing whichever side deleted the file.
#[derive(Debug, Clone, Default)]
pub struct ConflictSides {
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
}

/// Reads the full text content of every present side of `file_relative`'s
/// conflict, for a "here's what changed" view before picking a resolution.
pub fn conflict_sides(path: &Path, file_relative: &Path) -> Result<ConflictSides> {
    let repo = Repository::open(path)?;
    let index = repo.index()?;
    let conflict = find_conflict_entry(&index, file_relative)?;
    let blob_text = |entry: &Option<git2::IndexEntry>| -> Option<String> {
        let entry = entry.as_ref()?;
        let blob = repo.find_blob(entry.id).ok()?;
        Some(String::from_utf8_lossy(blob.content()).into_owned())
    };
    Ok(ConflictSides {
        base: blob_text(&conflict.ancestor),
        ours: blob_text(&conflict.our),
        theirs: blob_text(&conflict.their),
    })
}

/// A unified diff of one conflict side against the common ancestor (`base`
/// vs `ours`, and `base` vs `theirs`) — the same real-libgit2-diff approach
/// `diff_file_at` uses for note history, just computed from in-memory blobs
/// (`git2::Patch::from_blobs`) instead of two tree entries, since a
/// conflict's sides live only in the index, not as commits of their own. A
/// missing side (see `ConflictSides`) is treated as an empty blob, so an
/// add/add conflict's `ours` diff reads as "the whole file is new," the
/// same convention `diff_file_at` already uses for a file's very first
/// commit.
pub fn conflict_diff(path: &Path, file_relative: &Path) -> Result<(Vec<DiffLine>, Vec<DiffLine>)> {
    let repo = Repository::open(path)?;
    let index = repo.index()?;
    let conflict = find_conflict_entry(&index, file_relative)?;

    let empty_oid = repo.blob(b"")?;
    let blob_for = |entry: &Option<git2::IndexEntry>| -> Result<git2::Blob> {
        let oid = entry.as_ref().map(|e| e.id).unwrap_or(empty_oid);
        Ok(repo.find_blob(oid)?)
    };
    let base_blob = blob_for(&conflict.ancestor)?;
    let our_blob = blob_for(&conflict.our)?;
    let their_blob = blob_for(&conflict.their)?;

    let lines_of = |patch: &mut git2::Patch| -> Result<Vec<DiffLine>> {
        let mut lines = Vec::new();
        patch.print(&mut |_delta, _hunk, line| {
            if matches!(line.origin(), '+' | '-' | ' ') {
                lines.push(DiffLine {
                    origin: line.origin(),
                    content: String::from_utf8_lossy(line.content())
                        .trim_end_matches('\n')
                        .to_string(),
                });
            }
            true
        })?;
        Ok(lines)
    };

    let mut ours_patch = git2::Patch::from_blobs(&base_blob, None, &our_blob, None, None)?;
    let mut theirs_patch = git2::Patch::from_blobs(&base_blob, None, &their_blob, None, None)?;
    Ok((lines_of(&mut ours_patch)?, lines_of(&mut theirs_patch)?))
}

/// Resolves one conflicted file to `content`: writes it to the working
/// tree, then re-stages it, which clears every one of its conflict entries
/// from the index (`remove_path` first, since `add_path` alone doesn't
/// otherwise drop the now-stale ancestor/our/their stage entries). Doesn't
/// finalize the merge by itself — `finish_merge` is the explicit next step,
/// once every conflicted file has gone through this.
pub fn resolve_conflict(path: &Path, file_relative: &Path, content: &str) -> Result<()> {
    std::fs::write(path.join(file_relative), content)?;
    let repo = Repository::open(path)?;
    let mut index = repo.index()?;
    index.remove_path(file_relative)?;
    index.add_path(file_relative)?;
    index.write()?;
    Ok(())
}

/// Commits the now-fully-resolved merge as a two-parent commit and clears
/// merge state. Errors if any conflict remains — a caller shouldn't reach
/// this until `conflicted_files` is empty, but this checks explicitly
/// rather than trusting that. Reads the incoming side's commit off
/// `MERGE_HEAD` (set by `repo.merge` and still present at this point, unlike
/// during `pull` itself, where the `AnnotatedCommit` from the fetch is used
/// directly instead of round-tripping through that ref).
pub fn finish_merge(path: &Path, message: &str) -> Result<()> {
    let repo = Repository::open(path)?;
    let index = repo.index()?;
    if index.has_conflicts() {
        return Err(Error::Git(git2::Error::from_str(
            "cannot finish merge: unresolved conflicts remain",
        )));
    }
    let merge_head = repo.find_reference("MERGE_HEAD").map_err(|_| {
        Error::Git(git2::Error::from_str(
            "no merge in progress (MERGE_HEAD not found)",
        ))
    })?;
    let their_oid = merge_head
        .target()
        .ok_or_else(|| Error::Git(git2::Error::from_str("MERGE_HEAD has no target")))?;
    finalize_merge_commit(&repo, &their_oid, message)
}

/// Discards an in-progress merge entirely: resets the index and working
/// tree back to `HEAD` (before the merge touched anything) and clears merge
/// state. The fetched commits themselves stay in the object database (they
/// were already fetched into `refs/remotes/{remote}/*`), so a later `pull`
/// can attempt the merge again without re-fetching.
pub fn abort_merge(path: &Path) -> Result<()> {
    let repo = Repository::open(path)?;
    let head_commit = repo.head()?.peel_to_commit()?;
    repo.reset(head_commit.as_object(), git2::ResetType::Hard, None)?;
    repo.cleanup_state()?;
    Ok(())
}

/// Pull (fetch + merge) from `remote`, preferring `branch`. Only ever
/// fast-forwards or merges — never discards local commits. See
/// `PullOutcome` for what each possible result means.
pub fn pull(path: &Path, remote: &str, branch: &str) -> Result<PullOutcome> {
    let repo = Repository::open(path)?;
    let mut remote_ref = repo.find_remote(remote)?;
    let mut opts = git2::FetchOptions::new();
    opts.remote_callbacks(build_callbacks());
    // Tags aren't needed for a note-taking pull, and following them adds
    // extra lines to FETCH_HEAD, making the FETCH_HEAD-corruption issue
    // below more likely to trigger.
    opts.download_tags(git2::AutotagOption::None);

    // Fetch every branch the remote has — via the standard
    // `+refs/heads/*:refs/remotes/{remote}/*` refspec `repo.remote()` set up
    // when the remote was created — rather than a single hardcoded branch
    // name. A repo whose default branch isn't `main` (older repos default to
    // `master`, or the owner just named it something else) would otherwise
    // fail outright since `refs/heads/{branch}` wouldn't exist upstream.
    remote_ref.fetch(&[] as &[&str], Some(&mut opts), None)?;

    let prefix = format!("refs/remotes/{remote}/");
    let mut available = Vec::new();
    for reference in repo.references_glob(&format!("{prefix}*"))?.flatten() {
        if let Ok(name) = reference.name() {
            if let Some(b) = name.strip_prefix(&prefix) {
                available.push(b.to_string());
            }
        }
    }

    // Prefer the configured branch; if it's not there, fall back to the
    // remote's one branch (the common single-branch-with-a-different-name
    // case). Ambiguous (multiple branches, none matching) is an error rather
    // than a guess.
    let resolved_branch = if available.iter().any(|b| b == branch) {
        branch.to_string()
    } else if let [only] = available.as_slice() {
        only.clone()
    } else if available.is_empty() {
        return Err(Error::Git(git2::Error::from_str(
            "remote has no branches to pull (is it empty?)",
        )));
    } else {
        return Err(Error::Git(git2::Error::from_str(&format!(
            "branch '{branch}' not found on remote; available: {}",
            available.join(", ")
        ))));
    };

    // Reading the commit id back off the remote-tracking ref we just fetched
    // into, instead of FETCH_HEAD: FETCH_HEAD's on-disk format has extra
    // "branch '...' of '...'" annotation text after the commit id (and can
    // span multiple lines) — git2's loose-reference parser doesn't expect
    // that, so `repo.find_reference("FETCH_HEAD")` can fail with "corrupted
    // loose reference file: FETCH_HEAD" even when the fetch itself
    // succeeded. A plain ref (just a commit id, no annotation) doesn't have
    // this problem.
    let tracking_ref = format!("{prefix}{resolved_branch}");
    let fetched_oid = repo.refname_to_id(&tracking_ref)?;
    let fetch_commit = repo.find_annotated_commit(fetched_oid)?;
    let refname = format!("refs/heads/{resolved_branch}");

    let outcome = match repo.find_reference(&refname) {
        // Local branch exists — fast-forward when possible; otherwise
        // (history diverged) attempt a real merge instead of silently
        // doing nothing. This used to just skip the `if` and return success
        // regardless — the local ref never advanced, but `pull` reported
        // as if it had. `repo.merge` populates the index (and, for a real
        // conflict, the working tree's conflict markers) the same way `git
        // merge` itself would; `repo.state()` becomes `Merge` until
        // `finish_merge`/`abort_merge` clears it.
        Ok(mut reference) => {
            let analysis = repo.merge_analysis(&[&fetch_commit])?;
            if analysis.0.is_fast_forward() {
                reference.set_target(fetch_commit.id(), "shiki: fast-forward")?;
                repo.set_head(&refname)?;
                repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
                Ok(PullOutcome::FastForwarded {
                    branch: resolved_branch,
                })
            } else if analysis.0.is_up_to_date() {
                Ok(PullOutcome::UpToDate {
                    branch: resolved_branch,
                })
            } else {
                repo.merge(&[&fetch_commit], None, None)?;
                let index = repo.index()?;
                if index.has_conflicts() {
                    let files = conflict_paths_in_index(&index)?;
                    Ok(PullOutcome::ConflictsPending {
                        branch: resolved_branch,
                        files,
                    })
                } else {
                    finalize_merge_commit(
                        &repo,
                        &fetch_commit.id(),
                        &format!("shiki: merge '{resolved_branch}'"),
                    )?;
                    Ok(PullOutcome::MergedClean {
                        branch: resolved_branch,
                    })
                }
            }
        }
        // No local branch yet — this is the first pull into a brand-new
        // notebook (e.g. importing an existing repo as its remote), so there's
        // nothing to fast-forward against. Point the branch straight at what
        // was fetched, the same initial checkout `git clone` would do.
        Err(_) => {
            repo.reference(&refname, fetch_commit.id(), true, "shiki: initial pull")?;
            repo.set_head(&refname)?;
            repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
            Ok(PullOutcome::NewRepo {
                branch: resolved_branch,
            })
        }
    };
    outcome
}

/// Sets (creating or replacing) the notebook's `origin` remote. `url` can be
/// a normal git URL (`https://…`, `git@…`) or a local path/`file://` URL —
/// git treats a local path remote the same as any other for fetch/pull.
/// Strips the userinfo portion (`user[:password]@` in `scheme://user@host/…`,
/// or the whole `user@` in scp-style `user@host:path`) from a git URL —
/// called before any remote URL is ever interpolated into a status message
/// in `shiki-tui`, since those land in `log_history` (viewable in the logs
/// modal and copyable to the clipboard, and now persisted to disk).
/// GitHub/GitLab personal-access-token URLs commonly embed the token as
/// bare userinfo (`https://ghp_xxx@github.com/…`, no `:password` at all),
/// so this redacts *any* userinfo present, not just ones with a `:` in
/// them — a plain SSH `git@host:path` loses its (non-secret) "git" prefix
/// this way too, which is an acceptable trade for never accidentally
/// leaving a real token unredacted.
/// Redacts userinfo (`user[:password]@`) from a `scheme://...` URL, or the
/// whole `user@` from an scp-style `user@host:path` remote — never a bare
/// `@` search over the *entire* string. A self-hosted remote can legitimately
/// have an `@` in its path (e.g. `https://git.example.com/repos/notes@backup.git`);
/// searching the whole string for the first `@` would treat that as
/// userinfo and redact straight through the real host/path, not just hide a
/// credential. The userinfo `@`, if any, only ever appears in the authority
/// component — between `://` and the first `/` that starts the path — so
/// only that slice is searched.
pub fn redact_credentials(url: &str) -> String {
    match url.find("://") {
        Some(scheme_end) => {
            let authority_start = scheme_end + 3;
            let authority_end = url[authority_start..]
                .find('/')
                .map(|i| authority_start + i)
                .unwrap_or(url.len());
            match url[authority_start..authority_end].find('@') {
                Some(rel_at) => {
                    let at_idx = authority_start + rel_at;
                    format!("{}***@{}", &url[..authority_start], &url[at_idx + 1..])
                }
                None => url.to_string(),
            }
        }
        None => match url.find('@') {
            Some(at_idx) => format!("***@{}", &url[at_idx + 1..]), // scp-style, no scheme
            None => url.to_string(),
        },
    }
}

pub fn set_remote(path: &Path, url: &str) -> Result<()> {
    let repo = init_repo(path)?;
    if repo.find_remote("origin").is_ok() {
        repo.remote_set_url("origin", url)?;
    } else {
        repo.remote("origin", url)?;
    }
    Ok(())
}

/// The notebook's configured `origin` URL, if any.
pub fn remote_url(path: &Path) -> Option<String> {
    let repo = Repository::open(path).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    remote.url().ok().map(String::from)
}

/// One commit that changed a specific file — the note's real version
/// history, since every note lives in a git repo already; no separate
/// versioning system needed.
#[derive(Debug, Clone)]
pub struct FileRevision {
    /// Full 40-hex commit id — `show_file_at`/`revert_file_to` need the
    /// exact id, not an abbreviation; truncate for display, not for lookup.
    pub commit_id: String,
    pub date: chrono::DateTime<chrono::Local>,
    pub message: String,
}

/// Every commit that changed `file_relative` (relative to the notebook
/// root at `repo_path`), newest first. Walks history from `HEAD` comparing
/// each commit's blob at that path against its first parent's — a commit
/// is included only when that blob actually differs (or the file didn't
/// exist yet in the parent), same idea as `git log -- <path>`. Empty (not
/// an error) for a brand-new repo with no commits yet, or a file that was
/// never committed.
pub fn file_history(repo_path: &Path, file_relative: &Path) -> Result<Vec<FileRevision>> {
    let repo = Repository::open(repo_path)?;
    if repo.head().is_err() {
        return Ok(Vec::new());
    }

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(git2::Sort::TIME)?;

    let mut revisions = Vec::new();
    for oid in revwalk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;
        let entry_id = tree.get_path(file_relative).ok().map(|e| e.id());
        let Some(entry_id) = entry_id else {
            continue;
        };

        let parent_entry_id = commit
            .parent(0)
            .ok()
            .and_then(|parent| parent.tree().ok())
            .and_then(|tree| tree.get_path(file_relative).ok())
            .map(|e| e.id());

        if parent_entry_id == Some(entry_id) {
            continue; // unchanged at this path since the previous commit
        }

        let time = commit.time();
        let date = chrono::DateTime::from_timestamp(time.seconds(), 0)
            .unwrap_or_default()
            .with_timezone(&chrono::Local);
        revisions.push(FileRevision {
            commit_id: oid.to_string(),
            date,
            message: commit.summary().ok().flatten().unwrap_or("").to_string(),
        });
    }
    Ok(revisions)
}

/// `file_relative`'s full content as it was in `commit_id` — for viewing an
/// old revision, or as the source for `revert_file_to`.
pub fn show_file_at(repo_path: &Path, commit_id: &str, file_relative: &Path) -> Result<String> {
    let repo = Repository::open(repo_path)?;
    let oid = git2::Oid::from_str(commit_id)?;
    let commit = repo.find_commit(oid)?;
    let tree = commit.tree()?;
    let entry = tree.get_path(file_relative)?;
    let blob = repo.find_blob(entry.id())?;
    Ok(String::from_utf8_lossy(blob.content()).into_owned())
}

/// One line of a unified diff for a single file — `origin` is `'+'`
/// (added), `'-'` (removed), or `' '` (unchanged context line); nothing
/// else is ever produced, since `diff_file_at` only keeps those three
/// origins from libgit2's own patch output (file/hunk headers are
/// filtered out — the caller already knows which commit and file this is).
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub origin: char,
    pub content: String,
}

/// A unified diff of `file_relative` between `commit_id` and its first
/// parent — "what did this commit actually change here," the same thing
/// `git log -p -- <path>` would show for that one commit. If `commit_id`
/// has no parent (the repo's very first commit), the file is diffed
/// against an empty tree, so every line comes back as an addition — which
/// is the correct answer, not a special case: the whole file really is
/// new at that point. Real diff computation (via libgit2's own
/// `Repository::diff_tree_to_tree`, not a hand-rolled line algorithm), so
/// it's the same result `git diff` itself would produce, including its
/// line-matching heuristics.
pub fn diff_file_at(
    repo_path: &Path,
    commit_id: &str,
    file_relative: &Path,
) -> Result<Vec<DiffLine>> {
    let repo = Repository::open(repo_path)?;
    let oid = git2::Oid::from_str(commit_id)?;
    let commit = repo.find_commit(oid)?;
    let new_tree = commit.tree()?;
    let old_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

    let mut opts = git2::DiffOptions::new();
    opts.pathspec(file_relative);
    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;

    let mut lines = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        if matches!(line.origin(), '+' | '-' | ' ') {
            lines.push(DiffLine {
                origin: line.origin(),
                content: String::from_utf8_lossy(line.content())
                    .trim_end_matches('\n')
                    .to_string(),
            });
        }
        true
    })?;
    Ok(lines)
}

/// Overwrites the current working copy of `file_relative` with its content
/// from `commit_id` — a full-file revert (frontmatter included, since
/// that's what's actually stored in the blob), not just the body text.
/// Doesn't commit by itself: the reverted file just shows up as a normal
/// pending change, picked up by the usual sync flow (manual `s`/`u`, or
/// `auto_sync`) like any other edit.
pub fn revert_file_to(repo_path: &Path, commit_id: &str, file_relative: &Path) -> Result<()> {
    let content = show_file_at(repo_path, commit_id, file_relative)?;
    std::fs::write(repo_path.join(file_relative), content)?;
    Ok(())
}

/// Quick status: current branch, uncommitted changes, and how far ahead/
/// behind `refs/remotes/{remote}/{branch}` local `HEAD` is (from the last
/// fetch — this doesn't talk to the network itself).
pub fn status(path: &Path, remote: &str) -> GitStatus {
    let repo = match Repository::open(path) {
        Ok(r) => r,
        Err(_) => return GitStatus::default(),
    };
    let (dirty_count, status_error) = match repo.statuses(None) {
        Ok(statuses) => (statuses.len(), None),
        Err(e) => (0, Some(e.to_string())),
    };
    let branch = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().ok().map(str::to_string));
    let (ahead, behind) = branch
        .as_deref()
        .and_then(|b| {
            let local = repo.refname_to_id(&format!("refs/heads/{b}")).ok()?;
            let upstream = repo
                .refname_to_id(&format!("refs/remotes/{remote}/{b}"))
                .ok()?;
            repo.graph_ahead_behind(local, upstream).ok()
        })
        .unwrap_or((0, 0));
    GitStatus {
        is_repo: true,
        dirty: dirty_count > 0,
        dirty_count,
        branch,
        ahead,
        behind,
        status_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_https_token_as_bare_userinfo() {
        assert_eq!(
            redact_credentials("https://ghp_secrettoken@github.com/user/repo.git"),
            "https://***@github.com/user/repo.git"
        );
    }

    #[test]
    fn redacts_https_user_and_password() {
        assert_eq!(
            redact_credentials("https://user:hunter2@example.com/repo.git"),
            "https://***@example.com/repo.git"
        );
    }

    #[test]
    fn redacts_scp_style_ssh_url() {
        assert_eq!(
            redact_credentials("git@github.com:user/repo.git"),
            "***@github.com:user/repo.git"
        );
    }

    #[test]
    fn leaves_url_without_userinfo_unchanged() {
        assert_eq!(
            redact_credentials("https://github.com/user/repo.git"),
            "https://github.com/user/repo.git"
        );
        assert_eq!(redact_credentials(""), "");
    }

    #[test]
    fn leaves_local_path_unchanged() {
        assert_eq!(
            redact_credentials("/home/omar/bare-repos/notes.git"),
            "/home/omar/bare-repos/notes.git"
        );
    }

    #[test]
    fn leaves_a_legitimate_at_sign_in_the_path_untouched() {
        // The '@' here is part of the repo path, not userinfo — it comes
        // after the first '/' that starts the path, past the authority
        // component entirely.
        assert_eq!(
            redact_credentials("https://git.example.com/repos/notes@backup.git"),
            "https://git.example.com/repos/notes@backup.git"
        );
    }

    #[test]
    fn diff_file_at_reports_the_changed_line_as_removed_and_added() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let file = path.join("note.md");

        std::fs::write(&file, "line one\nline two\nline three\n").unwrap();
        commit_all(path, "first").unwrap();
        std::fs::write(&file, "line one\nline TWO changed\nline three\n").unwrap();
        commit_all(path, "second").unwrap();

        let relative = std::path::Path::new("note.md");
        let history = file_history(path, relative).unwrap();
        assert_eq!(history.len(), 2, "both commits touched note.md");
        let latest = &history[0].commit_id; // newest first

        let diff = diff_file_at(path, latest, relative).unwrap();
        let removed: Vec<&str> = diff
            .iter()
            .filter(|l| l.origin == '-')
            .map(|l| l.content.as_str())
            .collect();
        let added: Vec<&str> = diff
            .iter()
            .filter(|l| l.origin == '+')
            .map(|l| l.content.as_str())
            .collect();
        assert_eq!(removed, vec!["line two"]);
        assert_eq!(added, vec!["line TWO changed"]);
        // The untouched lines still show up as context, not as churn.
        assert!(diff
            .iter()
            .any(|l| l.origin == ' ' && l.content == "line one"));
    }

    #[test]
    fn diff_file_at_treats_the_first_commit_as_entirely_added() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let file = path.join("note.md");
        std::fs::write(&file, "brand new\n").unwrap();
        commit_all(path, "first").unwrap();

        let relative = std::path::Path::new("note.md");
        let history = file_history(path, relative).unwrap();
        let diff = diff_file_at(path, &history[0].commit_id, relative).unwrap();

        assert_eq!(diff.len(), 1);
        assert_eq!(diff[0].origin, '+');
        assert_eq!(diff[0].content, "brand new");
    }

    /// `Repository::init` alone leaves the initial branch name up to
    /// whatever `init.defaultBranch` happens to be configured to on the
    /// machine running the test — pinning it to "main" via
    /// `RepositoryInitOptions` keeps these tests deterministic regardless
    /// of that config.
    fn init_with_main(path: &Path) {
        let mut opts = git2::RepositoryInitOptions::new();
        opts.initial_head("main");
        Repository::init_opts(path, &opts).unwrap();
    }

    /// Sets up a genuine diverged-history scenario: a bare "origin", two
    /// working clones (`a`/`b`) both starting from the same pushed commit,
    /// then each committing a conflicting edit to the same line of
    /// `note.md` — `a` pushes first, `b` doesn't (a non-fast-forward push
    /// would just be rejected). Returns the two clone directories; `b` is
    /// the one every test then calls `pull` against.
    fn diverge_on_the_same_line(root: &Path) -> (PathBuf, PathBuf) {
        let bare_path = root.join("bare.git");
        Repository::init_bare(&bare_path).unwrap();
        let bare_url = bare_path.to_string_lossy().into_owned();

        let dir_a = root.join("a");
        let dir_b = root.join("b");
        init_with_main(&dir_a);
        init_with_main(&dir_b);
        set_remote(&dir_a, &bare_url).unwrap();
        set_remote(&dir_b, &bare_url).unwrap();

        std::fs::write(dir_a.join("note.md"), "line one\nline two\nline three\n").unwrap();
        commit_all(&dir_a, "first").unwrap();
        push(&dir_a, "origin").unwrap();

        // b's first pull: its local "main" is unborn (no commits yet), so
        // there's nothing to fast-forward against — it just adopts what
        // was fetched, same as a fresh `git clone`.
        let first_pull = pull(&dir_b, "origin", "main").unwrap();
        assert!(matches!(first_pull, PullOutcome::NewRepo { .. }));

        std::fs::write(
            dir_a.join("note.md"),
            "line one\nA changed this\nline three\n",
        )
        .unwrap();
        commit_all(&dir_a, "a-edit").unwrap();
        push(&dir_a, "origin").unwrap();

        std::fs::write(
            dir_b.join("note.md"),
            "line one\nB changed this\nline three\n",
        )
        .unwrap();
        commit_all(&dir_b, "b-edit").unwrap();

        (dir_a, dir_b)
    }

    #[test]
    fn pull_reports_conflicts_when_the_same_line_diverges() {
        let root = tempfile::tempdir().unwrap();
        let (_dir_a, dir_b) = diverge_on_the_same_line(root.path());

        let outcome = pull(&dir_b, "origin", "main").unwrap();
        let files = match outcome {
            PullOutcome::ConflictsPending { files, branch } => {
                assert_eq!(branch, "main");
                files
            }
            other => panic!("expected ConflictsPending, got {other:?}"),
        };
        assert_eq!(files, vec![PathBuf::from("note.md")]);
        assert!(merge_in_progress(&dir_b));
        assert_eq!(
            conflicted_files(&dir_b).unwrap(),
            vec![PathBuf::from("note.md")]
        );

        let sides = conflict_sides(&dir_b, Path::new("note.md")).unwrap();
        assert!(sides.base.unwrap().contains("line two"));
        assert!(sides.ours.unwrap().contains("B changed this"));
        assert!(sides.theirs.unwrap().contains("A changed this"));

        let (ours_diff, theirs_diff) = conflict_diff(&dir_b, Path::new("note.md")).unwrap();
        assert!(ours_diff
            .iter()
            .any(|l| l.origin == '+' && l.content == "B changed this"));
        assert!(theirs_diff
            .iter()
            .any(|l| l.origin == '+' && l.content == "A changed this"));
    }

    #[test]
    fn resolving_and_finishing_a_merge_produces_a_two_parent_commit() {
        let root = tempfile::tempdir().unwrap();
        let (_dir_a, dir_b) = diverge_on_the_same_line(root.path());
        pull(&dir_b, "origin", "main").unwrap();

        resolve_conflict(
            &dir_b,
            Path::new("note.md"),
            "line one\nresolved together\nline three\n",
        )
        .unwrap();
        assert!(conflicted_files(&dir_b).unwrap().is_empty());
        // Not finalized yet — still mid-merge until finish_merge runs.
        assert!(merge_in_progress(&dir_b));

        finish_merge(&dir_b, "shiki: merge 'main'").unwrap();

        assert!(!merge_in_progress(&dir_b));
        let repo_b = Repository::open(&dir_b).unwrap();
        let head_commit = repo_b.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head_commit.parent_count(), 2);
        assert_eq!(
            std::fs::read_to_string(dir_b.join("note.md")).unwrap(),
            "line one\nresolved together\nline three\n"
        );
    }

    #[test]
    fn finish_merge_errors_while_conflicts_remain() {
        let root = tempfile::tempdir().unwrap();
        let (_dir_a, dir_b) = diverge_on_the_same_line(root.path());
        pull(&dir_b, "origin", "main").unwrap();

        assert!(finish_merge(&dir_b, "shiki: merge 'main'").is_err());
        assert!(merge_in_progress(&dir_b));
    }

    #[test]
    fn abort_merge_restores_the_pre_merge_working_tree() {
        let root = tempfile::tempdir().unwrap();
        let (_dir_a, dir_b) = diverge_on_the_same_line(root.path());
        pull(&dir_b, "origin", "main").unwrap();

        abort_merge(&dir_b).unwrap();

        assert!(!merge_in_progress(&dir_b));
        assert!(conflicted_files(&dir_b).unwrap().is_empty());
        assert_eq!(
            std::fs::read_to_string(dir_b.join("note.md")).unwrap(),
            "line one\nB changed this\nline three\n"
        );
    }

    #[test]
    fn pull_merges_cleanly_when_edits_touch_different_files() {
        let root = tempfile::tempdir().unwrap();
        let bare_path = root.path().join("bare.git");
        Repository::init_bare(&bare_path).unwrap();
        let bare_url = bare_path.to_string_lossy().into_owned();

        let dir_a = root.path().join("a");
        let dir_b = root.path().join("b");
        init_with_main(&dir_a);
        init_with_main(&dir_b);
        set_remote(&dir_a, &bare_url).unwrap();
        set_remote(&dir_b, &bare_url).unwrap();

        std::fs::write(dir_a.join("one.md"), "one\n").unwrap();
        std::fs::write(dir_a.join("two.md"), "two\n").unwrap();
        commit_all(&dir_a, "first").unwrap();
        push(&dir_a, "origin").unwrap();
        pull(&dir_b, "origin", "main").unwrap();

        std::fs::write(dir_a.join("one.md"), "one, edited by a\n").unwrap();
        commit_all(&dir_a, "a-edit").unwrap();
        push(&dir_a, "origin").unwrap();

        std::fs::write(dir_b.join("two.md"), "two, edited by b\n").unwrap();
        commit_all(&dir_b, "b-edit").unwrap();

        let outcome = pull(&dir_b, "origin", "main").unwrap();
        assert!(matches!(outcome, PullOutcome::MergedClean { .. }));
        assert!(!merge_in_progress(&dir_b));

        let repo_b = Repository::open(&dir_b).unwrap();
        let head_commit = repo_b.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head_commit.parent_count(), 2);
        assert_eq!(
            std::fs::read_to_string(dir_b.join("one.md")).unwrap(),
            "one, edited by a\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir_b.join("two.md")).unwrap(),
            "two, edited by b\n"
        );
    }
}

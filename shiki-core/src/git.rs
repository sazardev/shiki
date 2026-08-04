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

/// Pull (fetch + fast-forward merge) from `remote`, preferring `branch`.
/// Returns the branch name actually pulled — it can differ from `branch`
/// (see the fallback below), so callers should report it rather than
/// assuming the configured name was used.
pub fn pull(path: &Path, remote: &str, branch: &str) -> Result<String> {
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

    match repo.find_reference(&refname) {
        // Local branch exists — only fast-forward, never discard local commits.
        Ok(mut reference) => {
            let analysis = repo.merge_analysis(&[&fetch_commit])?;
            if analysis.0.is_fast_forward() {
                reference.set_target(fetch_commit.id(), "shiki: fast-forward")?;
                repo.set_head(&refname)?;
                repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
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
        }
    }
    Ok(resolved_branch)
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
}

//! Tiny process-environment helpers shared by anything that needs to know
//! whether an external binary is available, without executing it.

/// Whether `bin` exists somewhere on `$PATH` — a plain lookup, deliberately
/// not executing it (a `--version` probe could hang or have side effects for
/// an arbitrary configured/external binary). Shared by `shiki doctor` and
/// `publish::ensure_binary` so there's exactly one `$PATH` scan implementation,
/// not two copies that could drift.
pub fn on_path(bin: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| dir.join(bin).is_file())
}

/// Expands a leading `~` (or `~/...`) to the user's home directory; anything
/// else — including a plain `/absolute` or `./relative` path — is returned
/// unchanged for the caller to resolve against the current directory
/// itself. A `~` that can't be resolved (no home directory at all) also
/// comes back unchanged, so a caller can decide whether that's an error or
/// just something to pass along.
#[cfg(feature = "home-dir-expand")]
pub fn expand_home(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
            let rest = rest.strip_prefix('/').unwrap_or(rest);
            return if rest.is_empty() {
                home
            } else {
                home.join(rest)
            };
        }
    }
    std::path::PathBuf::from(path)
}

/// Without the `home-dir-expand` feature (default-on for every existing
/// consumer — see `shiki-core/Cargo.toml`), there's no `directories`
/// dependency at all, so a leading `~` is left untouched rather than
/// resolved — same "return unchanged" contract this function already uses
/// for an unresolvable home directory.
#[cfg(not(feature = "home-dir-expand"))]
pub fn expand_home(path: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(path)
}

/// Whether `pid` is still a live process.
///
/// On Unix, `kill(pid, 0)` sends no signal but performs the usual permission
/// checks: success or a permission error both mean the process exists (we
/// may just not be allowed to signal it), while `ESRCH` means no such
/// process. On Windows there's no cheap equivalent, so this conservatively
/// reports `true` — a stale capture-daemon port file is then caught by the
/// TCP connect timeout instead (the user-visible symptom is still handled,
/// just not the file cleanup done here on Unix).
pub fn is_pid_alive(pid: u32) -> bool {
    #[cfg(all(unix, feature = "unix-process-check"))]
    {
        if pid == 0 || pid > i32::MAX as u32 {
            return false;
        }
        // SAFETY: signal 0 performs no signal delivery, and `pid` was
        // range-checked above.
        let ret = unsafe { libc::kill(pid as i32, 0) };
        if ret == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    // Without the `unix-process-check` feature (default-on for every
    // existing consumer — see `shiki-core/Cargo.toml`), there's no `libc`
    // dependency at all, so this falls back to the same conservative
    // "assume alive" `true` Windows already uses above.
    #[cfg(all(unix, not(feature = "unix-process-check")))]
    {
        let _ = pid;
        true
    }
    #[cfg(windows)]
    {
        let _ = pid;
        true
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_is_alive() {
        assert!(is_pid_alive(std::process::id()));
    }

    #[test]
    fn expand_home_leaves_non_tilde_paths_untouched() {
        assert_eq!(
            expand_home("/abs/path"),
            std::path::PathBuf::from("/abs/path")
        );
        assert_eq!(
            expand_home("./relative"),
            std::path::PathBuf::from("./relative")
        );
    }

    #[cfg(feature = "home-dir-expand")]
    #[test]
    fn expand_home_expands_a_tilde_when_a_home_exists() {
        // The sandbox/CI always has a home directory, so this asserts the
        // expansion path; the no-home fallback is the untouched-path branch
        // covered above.
        if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) {
            assert_eq!(expand_home("~"), home);
            assert_eq!(expand_home("~/notes"), home.join("notes"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn nonexistent_pid_is_not_alive() {
        // u32::MAX is not a valid pid on any real system.
        assert!(!is_pid_alive(u32::MAX));
    }
}

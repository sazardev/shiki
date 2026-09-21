//! Tiny process-environment helpers shared by anything that needs to know
//! whether an external binary is available, without executing it — plus
//! `run_with_timeout`, for the cases (voice recording, the new-notebook
//! wizard's `gh` preflight checks) that *do* need to actually run one, safely.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

/// Outcome of one `run_with_timeout` call: whether the command succeeded,
/// plus its stderr (collected rather than inherited, so a caller can surface
/// *why* only if it actually needs to).
pub struct CommandOutcome {
    pub success: bool,
    pub stderr: String,
}

/// Runs `command`, killing it if it hasn't exited within `timeout` — std-only,
/// so a command that hangs (a recorder opening a missing audio device, `gh`
/// waiting on a stalled network call) fails fast instead of wedging whatever
/// called it. Originally `shiki-core/src/voice.rs`'s own private helper;
/// promoted here once the new-notebook wizard's `gh auth status`/`gh repo
/// view` preflight checks needed the exact same "run a real external command
/// safely" primitive — one implementation, not two copies that could drift.
pub fn run_with_timeout(command: &mut Command, timeout: Duration) -> CommandOutcome {
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return CommandOutcome {
                success: false,
                stderr: format!("could not spawn: {e}"),
            };
        }
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut buf = String::new();
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_string(&mut buf);
                }
                return CommandOutcome {
                    success: status.success(),
                    stderr: buf,
                };
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return CommandOutcome {
                        success: false,
                        stderr: format!("timed out after {}s", timeout.as_secs()),
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return CommandOutcome {
                    success: false,
                    stderr: format!("{e}"),
                };
            }
        }
    }
}

/// Whether an SSH key/agent is available at all — checked before offering
/// the new-notebook wizard's SSH source kind, purely advisory (still lets
/// the user proceed either way, just warns if nothing was found). Checks
/// `SSH_AUTH_SOCK` (an agent is loaded — the codepath `git.rs::build_callbacks`
/// actually tries first, via `Cred::ssh_key_from_agent`) or a handful of the
/// most common default key file names under `~/.ssh`, since a key can exist
/// without an agent running (git2 falls back to the credential helper, which
/// only helps for HTTPS remotes, not these SSH-file-only ones).
#[cfg(feature = "home-dir-expand")]
pub fn ssh_agent_or_key_available() -> bool {
    if std::env::var_os("SSH_AUTH_SOCK").is_some() {
        return true;
    }
    let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()) else {
        return false;
    };
    let ssh_dir = home.join(".ssh");
    ["id_ed25519", "id_rsa", "id_ecdsa"]
        .iter()
        .any(|name| ssh_dir.join(name).is_file())
}

/// Without `home-dir-expand` there's no `directories` dependency to resolve a
/// home directory — falls back to just the agent-socket check, same
/// "degrade gracefully, don't add a new dependency" contract `expand_home`
/// already follows below.
#[cfg(not(feature = "home-dir-expand"))]
pub fn ssh_agent_or_key_available() -> bool {
    std::env::var_os("SSH_AUTH_SOCK").is_some()
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

    #[cfg(unix)]
    #[test]
    fn run_with_timeout_reports_the_real_exit_status() {
        let ok = run_with_timeout(&mut Command::new("true"), Duration::from_secs(5));
        assert!(ok.success);
        let fail = run_with_timeout(&mut Command::new("false"), Duration::from_secs(5));
        assert!(!fail.success);
    }

    #[cfg(unix)]
    #[test]
    fn run_with_timeout_kills_a_hanging_command() {
        let res = run_with_timeout(Command::new("sleep").arg("5"), Duration::from_millis(200));
        assert!(!res.success);
        assert!(res.stderr.contains("timed out"));
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

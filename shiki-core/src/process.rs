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

pub mod capture;
pub mod config;
pub mod daemon;
pub mod daily;
pub mod delete;
pub mod diff;
pub mod doctor;
pub mod edit;
pub mod export;
pub mod extension;
pub mod field;
pub mod folder;
pub mod graph;
pub mod import;
pub mod index;
pub mod list;
pub mod log;
pub mod r#move;
pub mod new;
pub mod notebook;
pub mod publish;
pub mod query;
pub mod rename;
pub mod search;
pub mod show;
pub mod sync;
pub mod tag;
pub mod tasks;
pub mod theme;

use std::io::IsTerminal;

use anyhow::{Context, Result};
use shiki_core::Notebook;

/// `find_note`/`get_notebook` and the pagination helpers below are plain
/// `shiki-core` functions (`shiki_core::notebook`/`shiki_core::pagination`)
/// re-exported here — they moved down out of this file so the MCP server
/// (`shiki-mcp`, which depends on `shiki-core` directly, not on
/// `shiki-cli`) can share the exact same implementations instead of
/// carrying parallel copies. Every existing `use super::{find_note, ...}`
/// import elsewhere in this crate keeps working unchanged.
pub use shiki_core::notebook::{find_note, get_notebook};
pub use shiki_core::pagination::{effective_limit, page_footer, page_json, paginate};

/// Attaches this session's passphrase to `nb` if it's configured as
/// encrypted, never cached or stored anywhere beyond the lifetime of this
/// one CLI invocation. A plaintext notebook passes through untouched, no
/// prompt at all. Three sources, in order:
/// 1. `SHIKI_PASSPHRASE` — for non-interactive/scripted use (an agent, a
///    cron job, `shiki daemon`); if set, it always wins, no prompt.
/// 2. An interactive hidden-input prompt (`rpassword`), only when stdin is
///    actually a TTY — same as before this function existed.
/// 3. Otherwise (no env var, no TTY — e.g. piped/redirected stdin under a
///    script) a clear error instead of `rpassword` blocking forever on a
///    non-interactive stream, which previously hung rather than failed.
pub fn unlock_if_encrypted(config: &shiki_config::Config, nb: Notebook) -> Result<Notebook> {
    if !config.encrypt_for(&nb.name) {
        return Ok(nb);
    }
    let passphrase = if let Ok(env) = std::env::var("SHIKI_PASSPHRASE") {
        env
    } else if std::io::stdin().is_terminal() {
        rpassword::prompt_password(format!("Passphrase for '{}': ", nb.name))?
    } else {
        anyhow::bail!(
            "notebook '{}' is encrypted \u{2014} set SHIKI_PASSPHRASE or run interactively",
            nb.name
        );
    };
    Ok(nb.with_crypto(Some(shiki_core::crypto::NotebookCrypto::new(passphrase))))
}

/// Opens `path` with the configured external editor, waiting for it to finish.
pub fn open_in_editor(editor: &str, path: &std::path::Path) -> Result<()> {
    let status = shiki_core::editor::command_for(editor, path)
        .status()
        .with_context(|| format!("could not run editor '{editor}'"))?;
    if !status.success() {
        anyhow::bail!("'{editor}' exited with an error");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // `unlock_if_encrypted` reads a real process-global env var, so tests
    // touching it are serialized against each other the same way
    // `shiki-config`'s own `XDG_CONFIG_HOME`/`XDG_DATA_HOME` tests already
    // are (`shiki-config/src/config.rs`'s `ENV_LOCK`) — otherwise two tests
    // running in parallel on separate threads could race on the same
    // variable. Only the TTY branch is untested here: there's no seam to
    // fake `stdin().is_terminal()` from a unit test, and `cargo test`
    // itself runs with stdin redirected, so that branch never fires under
    // `cargo test` regardless — covered by manual verification instead.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn unlock_if_encrypted_leaves_a_plaintext_notebook_untouched() {
        let config = shiki_config::Config::default();
        let nb = shiki_core::Notebook::new("personal", std::path::PathBuf::from("/tmp/personal"));

        let unlocked = unlock_if_encrypted(&config, nb).unwrap();

        assert!(unlocked.crypto.is_none());
    }

    #[test]
    fn unlock_if_encrypted_uses_the_env_var_without_prompting() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("SHIKI_PASSPHRASE");
        // SAFETY: held under ENV_LOCK; restored before releasing.
        unsafe {
            std::env::set_var("SHIKI_PASSPHRASE", "correct horse battery staple");
        }

        let mut config = shiki_config::Config::default();
        config
            .notebooks
            .entry("work".to_string())
            .or_default()
            .encrypt = true;
        let nb = shiki_core::Notebook::new("work", std::path::PathBuf::from("/tmp/work"));
        let result = unlock_if_encrypted(&config, nb);

        match prev {
            Some(v) => unsafe { std::env::set_var("SHIKI_PASSPHRASE", v) },
            None => unsafe { std::env::remove_var("SHIKI_PASSPHRASE") },
        }

        assert!(result.unwrap().crypto.is_some());
    }

    #[test]
    fn unlock_if_encrypted_without_a_tty_or_env_var_errors_instead_of_hanging() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("SHIKI_PASSPHRASE");
        // SAFETY: held under ENV_LOCK; restored before releasing.
        unsafe {
            std::env::remove_var("SHIKI_PASSPHRASE");
        }

        let mut config = shiki_config::Config::default();
        config
            .notebooks
            .entry("work".to_string())
            .or_default()
            .encrypt = true;
        let nb = shiki_core::Notebook::new("work", std::path::PathBuf::from("/tmp/work"));
        let result = unlock_if_encrypted(&config, nb);

        if let Some(v) = prev {
            unsafe { std::env::set_var("SHIKI_PASSPHRASE", v) };
        }

        // `cargo test`'s stdin is never a TTY, so this always takes the
        // "clear error" branch rather than blocking on a prompt.
        assert!(result.is_err());
    }
}

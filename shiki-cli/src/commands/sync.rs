use anyhow::{Context, Result};
use shiki_config::Config;
use shiki_core::{git, NotebookStore};

pub fn run(store: &NotebookStore, notebook: &str, config: &Config) -> Result<()> {
    let nb = store.get(notebook).with_context(|| {
        format!("notebook '{notebook}' not found \u{2014} see `shiki notebook list`")
    })?;
    let sync = config.sync_for(notebook);
    if config.git.auto_commit {
        // Names the actual changed files in the commit message (falling
        // back to a bare "changes" only if the diff walk itself fails) —
        // same `git::diff_summary` the TUI's `run_sync_blocking` already
        // uses, so `shiki sync` and pressing `s` in the TUI produce the
        // same git history instead of this command always writing a bare
        // "{prefix}sync" regardless of what actually changed.
        let summary = git::diff_summary(&nb.path).unwrap_or_else(|_| "changes".to_string());
        let message = format!("{}{summary}", config.git.commit_prefix);
        let committed = git::commit_all(&nb.path, &message)?;
        if committed {
            println!("commit created in '{notebook}': {summary}");
        } else {
            println!("'{notebook}' has no changes");
        }
    } else {
        println!("auto_commit is disabled; skipping commit for '{notebook}'");
    }
    if sync.auto_push {
        if git::remote_url(&nb.path).is_none() {
            println!(
                "'{notebook}' has no git remote configured \u{2014} set one (`R` in the TUI, or \
                 `git -C <notebook path> remote add origin <url>`), then sync again"
            );
        } else {
            git::push(&nb.path, &config.git.remote)?;
            println!("pushed to {}", config.git.remote);
        }
    }
    Ok(())
}

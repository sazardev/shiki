use anyhow::{Context as _, Result};
use shiki_config::Config;
use shiki_core::crypto::{canary_blob, verify_canary, NotebookCrypto, CANARY_FILE};
use shiki_core::NotebookStore;

pub fn create(store: &NotebookStore, name: &str) -> Result<()> {
    let nb = store.create(name)?;
    println!("notebook created: {}", nb.path.display());
    Ok(())
}

pub fn list(store: &NotebookStore, config: &Config, json: bool, all: bool) -> Result<()> {
    // Notebooks untracked via "keep files, just untrack" ([notebooks.<name>]
    // hidden = true) stay hidden here too, matching what the TUI lists —
    // `--all` is the explicit way to see them (marked), e.g. to find
    // something you untracked and want back.
    let notebooks: Vec<_> = store
        .list()?
        .into_iter()
        .filter(|nb| {
            all || !config
                .notebooks
                .get(&nb.name)
                .is_some_and(|over| over.hidden)
        })
        .collect();
    let label = |nb: &shiki_core::Notebook| -> String {
        if all
            && config
                .notebooks
                .get(&nb.name)
                .is_some_and(|over| over.hidden)
        {
            format!("{} (hidden)", nb.name)
        } else {
            nb.name.clone()
        }
    };

    if json {
        let items: Vec<serde_json::Value> = notebooks
            .iter()
            .map(|nb| match nb.all_notes_recursive() {
                Ok(notes) => serde_json::json!({
                    "name": nb.name,
                    "path": nb.path,
                    "hidden": config.notebooks.get(&nb.name).is_some_and(|over| over.hidden),
                    "note_count": notes.len(),
                    "error": null,
                }),
                Err(e) => serde_json::json!({
                    "name": nb.name,
                    "path": nb.path,
                    "hidden": config.notebooks.get(&nb.name).is_some_and(|over| over.hidden),
                    "note_count": null,
                    "error": e.to_string(),
                }),
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    if notebooks.is_empty() {
        println!("(no notebooks)");
        return Ok(());
    }
    for nb in &notebooks {
        match nb.all_notes_recursive() {
            Ok(notes) => println!("{}  ({} notes)", label(nb), notes.len()),
            // A failed walk (permissions, I/O) is not the same thing as a
            // genuinely empty notebook — `unwrap_or(0)` used to make the two
            // indistinguishable in this output.
            Err(e) => println!("{}  (error reading notes: {e})", label(nb)),
        }
    }
    Ok(())
}

pub fn rename(store: &NotebookStore, old: &str, new: &str) -> Result<()> {
    let nb = store.rename(old, new)?;
    println!("renamed to: {}", nb.path.display());
    Ok(())
}

/// Permanently deletes a notebook and every note in it (`NotebookStore::delete`
/// is a plain `remove_dir_all` — no trash/undo). `yes` must be explicitly
/// passed (`--yes`), mirroring the TUI's own confirm dialog for the same
/// action, rather than deleting on the bare name alone.
pub fn delete(store: &NotebookStore, name: &str, yes: bool) -> Result<()> {
    if !yes {
        anyhow::bail!(
            "this permanently deletes '{name}' and every note in it \u{2014} re-run with --yes \
             to confirm"
        );
    }
    store.delete(name)?;
    println!("deleted: {name}");
    Ok(())
}

/// Enables encryption for an existing (plaintext) notebook: prompts for a
/// passphrase (twice, to catch typos), writes a canary file so a future
/// wrong-passphrase attempt fails immediately rather than after a bulk
/// re-encrypt, re-encrypts every existing note with it, flips
/// `[notebooks.<name>] encrypt = true` in config.toml, and commits the
/// result. The passphrase itself is never written anywhere — the user has
/// to remember it (or record it themselves, out of band); there is no
/// recovery path if it's lost.
pub fn encrypt(store: &NotebookStore, config: &mut Config, name: &str) -> Result<()> {
    if config.encrypt_for(name) {
        anyhow::bail!("'{name}' is already encrypted");
    }
    let nb = store
        .get(name)
        .with_context(|| format!("notebook '{name}' not found"))?;

    let passphrase = rpassword::prompt_password("Passphrase: ")?;
    if passphrase.is_empty() {
        anyhow::bail!("passphrase must not be empty");
    }
    let confirm = rpassword::prompt_password("Confirm passphrase: ")?;
    if passphrase != confirm {
        anyhow::bail!("passphrases did not match");
    }
    let crypto = NotebookCrypto::new(passphrase);

    let canary = canary_blob(&crypto)?;
    std::fs::write(nb.path.join(CANARY_FILE), canary)
        .with_context(|| format!("could not write {CANARY_FILE}"))?;

    let notes = nb
        .all_notes_recursive()
        .context("could not read existing notes (before any were touched)")?;
    for note in &notes {
        note.save_with_crypto(Some(&crypto))
            .with_context(|| format!("could not re-encrypt '{}'", note.path.display()))?;
    }

    config
        .notebooks
        .entry(name.to_string())
        .or_default()
        .encrypt = true;
    config.save(&Config::default_path()?)?;

    if let Err(e) = shiki_core::git::commit_all(&nb.path, "shiki: enable encryption") {
        println!("warning: could not commit the change: {e}");
    }

    println!(
        "'{name}' is now encrypted ({} note{} re-encrypted) — remember this passphrase, \
         it's never stored anywhere and there is no recovery if it's lost",
        notes.len(),
        if notes.len() == 1 { "" } else { "s" }
    );
    Ok(())
}

/// Reverses `encrypt`: verifies the passphrase against the canary first (so
/// a wrong passphrase fails clearly instead of writing corrupted plaintext
/// over every note), decrypts every note back to plain text, removes the
/// canary, flips `encrypt = false`, and commits.
pub fn decrypt(store: &NotebookStore, config: &mut Config, name: &str) -> Result<()> {
    if !config.encrypt_for(name) {
        anyhow::bail!("'{name}' is not encrypted");
    }
    let nb = store
        .get(name)
        .with_context(|| format!("notebook '{name}' not found"))?;

    let passphrase = rpassword::prompt_password("Passphrase: ")?;
    let crypto = NotebookCrypto::new(passphrase);

    let canary_path = nb.path.join(CANARY_FILE);
    let canary = std::fs::read_to_string(&canary_path).with_context(|| {
        format!("missing {CANARY_FILE} — was '{name}' really encrypted by shiki?")
    })?;
    match verify_canary(&crypto, &canary) {
        Ok(true) => {}
        Ok(false) => anyhow::bail!("canary file is corrupted, not just a wrong passphrase"),
        Err(e) => anyhow::bail!("wrong passphrase: {e}"),
    }

    let nb_unlocked = nb.clone().with_crypto(Some(crypto));
    let notes = nb_unlocked
        .all_notes_recursive()
        .context("could not decrypt existing notes")?;
    for note in &notes {
        note.save_with_crypto(None)
            .with_context(|| format!("could not decrypt '{}'", note.path.display()))?;
    }
    std::fs::remove_file(&canary_path).ok();

    if let Some(over) = config.notebooks.get_mut(name) {
        over.encrypt = false;
    }
    config.save(&Config::default_path()?)?;

    if let Err(e) = shiki_core::git::commit_all(&nb.path, "shiki: disable encryption") {
        println!("warning: could not commit the change: {e}");
    }

    println!(
        "'{name}' is now decrypted ({} note{} restored to plain text)",
        notes.len(),
        if notes.len() == 1 { "" } else { "s" }
    );
    Ok(())
}

/// Changes an encrypted notebook's passphrase in one step: verifies the old
/// passphrase against the canary (wrong old passphrase fails before anything
/// is touched), prompts for the new one twice, re-encrypts every note with
/// the new passphrase, and rewrites the canary. Unlike the `decrypt` +
/// `encrypt` two-step, the notes never sit unencrypted on disk in between.
/// `config.toml` doesn't need touching — `encrypt = true` already holds; the
/// passphrase was never stored anywhere, so the only thing that changes is
/// what a future unlock must type.
pub fn rekey(store: &NotebookStore, config: &mut Config, name: &str) -> Result<()> {
    if !config.encrypt_for(name) {
        anyhow::bail!("'{name}' is not encrypted");
    }
    let nb = store
        .get(name)
        .with_context(|| format!("notebook '{name}' not found"))?;

    let old = rpassword::prompt_password("Current passphrase: ")?;
    let old_crypto = NotebookCrypto::new(old);

    let canary_path = nb.path.join(CANARY_FILE);
    let canary = std::fs::read_to_string(&canary_path).with_context(|| {
        format!("missing {CANARY_FILE} — was '{name}' really encrypted by shiki?")
    })?;
    match verify_canary(&old_crypto, &canary) {
        Ok(true) => {}
        Ok(false) => anyhow::bail!("canary file is corrupted, not just a wrong passphrase"),
        Err(e) => anyhow::bail!("wrong passphrase: {e}"),
    }

    let passphrase = rpassword::prompt_password("New passphrase: ")?;
    if passphrase.is_empty() {
        anyhow::bail!("passphrase must not be empty");
    }
    let confirm = rpassword::prompt_password("Confirm new passphrase: ")?;
    if passphrase != confirm {
        anyhow::bail!("passphrases did not match");
    }
    let new_crypto = NotebookCrypto::new(passphrase);

    let old_unlocked = nb.clone().with_crypto(Some(old_crypto));
    let notes = old_unlocked
        .all_notes_recursive()
        .context("could not read existing notes (before any were touched)")?;
    for note in &notes {
        note.save_with_crypto(Some(&new_crypto))
            .with_context(|| format!("could not re-encrypt '{}'", note.path.display()))?;
    }

    std::fs::write(nb.path.join(CANARY_FILE), canary_blob(&new_crypto)?)
        .with_context(|| format!("could not rewrite {CANARY_FILE}"))?;

    if let Err(e) = shiki_core::git::commit_all(&nb.path, "shiki: rekey encryption") {
        println!("warning: could not commit the change: {e}");
    }

    println!(
        "'{name}' re-keyed ({} note{} re-encrypted) — use the new passphrase from now on; \
         any other machine still holding the old one needs it",
        notes.len(),
        if notes.len() == 1 { "" } else { "s" }
    );
    Ok(())
}

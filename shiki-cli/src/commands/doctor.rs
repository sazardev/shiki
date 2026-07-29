//! `shiki doctor` — an environment health check, independent of the normal
//! startup path (`Context::load`) so it still works (and says why) even when
//! the config is broken, which is exactly the situation someone reaching for
//! a "doctor" command is usually in.

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::KeyCode;
use shiki_config::Config;
use shiki_core::NotebookStore;
use shiki_tui::keybindings::parse_key;

struct Report {
    color: bool,
    ok: u32,
    warn: u32,
    fail: u32,
}

impl Report {
    fn new() -> Self {
        Self {
            color: std::io::stdout().is_terminal(),
            ok: 0,
            warn: 0,
            fail: 0,
        }
    }

    fn pass(&mut self, label: &str, detail: impl std::fmt::Display) {
        self.ok += 1;
        self.line("\u{2713}", 32, label, detail);
    }

    fn warn(&mut self, label: &str, detail: impl std::fmt::Display) {
        self.warn += 1;
        self.line("!", 33, label, detail);
    }

    fn fail(&mut self, label: &str, detail: impl std::fmt::Display) {
        self.fail += 1;
        self.line("\u{2717}", 31, label, detail);
    }

    fn line(&self, symbol: &str, color_code: u8, label: &str, detail: impl std::fmt::Display) {
        if self.color {
            println!("\x1b[{color_code}m{symbol}\x1b[0m {label}: {detail}");
        } else {
            println!("{symbol} {label}: {detail}");
        }
    }
}

/// Whether `bin` exists somewhere on `$PATH` — a plain lookup, deliberately
/// not executing it (a `--version` probe could hang or have side effects for
/// an arbitrary user-configured editor command).
fn on_path(bin: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| dir.join(bin).is_file())
}

pub fn run() -> Result<()> {
    let mut r = Report::new();
    println!(
        "shiki {} \u{2014} environment check\n",
        env!("CARGO_PKG_VERSION")
    );

    let config_path = Config::default_path();
    let config = match &config_path {
        Ok(path) if !path.exists() => {
            r.warn(
                "config",
                format!(
                    "{} \u{2014} not created yet, defaults will be used on first run",
                    path.display()
                ),
            );
            None
        }
        Ok(path) => match std::fs::read_to_string(path) {
            Ok(contents) => match Config::parse(&contents) {
                Ok(cfg) => {
                    r.pass("config", path.display());
                    Some(cfg)
                }
                Err(e) => {
                    r.fail(
                        "config",
                        format!("{} \u{2014} invalid TOML: {e}", path.display()),
                    );
                    None
                }
            },
            Err(e) => {
                r.fail(
                    "config",
                    format!("{} \u{2014} could not read: {e}", path.display()),
                );
                None
            }
        },
        Err(e) => {
            r.fail("config", e);
            None
        }
    };
    let config = config.unwrap_or_default();

    let data_dir = config
        .general
        .data_dir
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| Config::default_data_dir().ok());
    let Some(data_dir) = data_dir else {
        r.fail("data dir", "could not determine default data directory");
        return Ok(());
    };
    if data_dir.exists() {
        r.pass("data dir", data_dir.display());
    } else {
        r.warn(
            "data dir",
            format!("{} — not created yet", data_dir.display()),
        );
    }

    match Config::default_templates_dir() {
        Ok(dir) if dir.exists() => r.pass("templates dir", dir.display()),
        Ok(dir) => r.warn(
            "templates dir",
            format!("{} \u{2014} created on first run", dir.display()),
        ),
        Err(e) => r.fail("templates dir", e),
    }

    if on_path("git") {
        r.pass("git", "found on PATH");
    } else {
        r.fail(
            "git",
            "not found on PATH \u{2014} required for notebook sync/pull/push",
        );
    }

    if on_path("gh") {
        r.pass(
            "gh (GitHub CLI)",
            "found \u{2014} used by git's credential helper for HTTPS auth to private repos",
        );
    } else {
        r.warn(
            "gh (GitHub CLI)",
            "not found \u{2014} HTTPS auth to private remotes relies on your system git credential store instead",
        );
    }

    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    if colorterm.contains("truecolor") || colorterm.contains("24bit") {
        r.pass("terminal truecolor", format!("COLORTERM={colorterm}"));
    } else {
        r.warn(
            "terminal truecolor",
            "COLORTERM isn't \"truecolor\"/\"24bit\" \u{2014} themes may render with approximated \
             colors instead of exact hex (inside tmux, also check `terminal-overrides ,*:Tc` or \
             `terminal-features ,*:RGB` in .tmux.conf)",
        );
    }

    let editor_bin = config
        .general
        .editor
        .split_whitespace()
        .next()
        .unwrap_or("");
    if editor_bin.is_empty() {
        r.warn("editor", "no `general.editor` configured");
    } else if on_path(editor_bin) {
        r.pass(
            "editor",
            format!("'{}' found on PATH", config.general.editor),
        );
    } else {
        r.warn(
            "editor",
            format!(
                "'{editor_bin}' not found on PATH \u{2014} check `general.editor` in config.toml"
            ),
        );
    }

    if config.general.use_favorite_editor {
        match shiki_core::editor::detect_favorite_editor() {
            Some(fav) => r.pass("favorite editor", fav),
            None => r.warn(
                "favorite editor",
                "`use_favorite_editor` is on but none could be detected ($VISUAL/$EDITOR/xdg-mime)",
            ),
        }
    }

    let custom_paths = config.notebook_custom_paths();
    let store = NotebookStore::new_with_custom_paths(data_dir.clone(), custom_paths);
    match store.list() {
        Ok(notebooks) if notebooks.is_empty() => r.warn(
            "notebooks",
            "none yet — `shiki notebook create <name>`, or `a` in the TUI",
        ),
        Ok(notebooks) => {
            let with_remote = notebooks
                .iter()
                .filter(|nb| shiki_core::git::remote_url(&nb.path).is_some())
                .count();
            r.pass(
                "notebooks",
                format!(
                    "{} found, {with_remote} with a git remote configured",
                    notebooks.len()
                ),
            );
        }
        Err(e) => r.fail("notebooks", e),
    }

    check_keybinding_health(&config, &mut r);
    check_snippet_health(&config, &mut r);
    check_notebook_path_health(&config, &mut r);

    println!("\n{} ok, {} warning(s), {} failed", r.ok, r.warn, r.fail);
    if r.fail > 0 {
        anyhow::bail!("doctor found {} problem(s) \u{2014} see above", r.fail);
    }
    Ok(())
}

/// Two problems that `KeyMaps::from_config`'s silent `HashMap::insert`
/// (last `bind()` call for a given key wins, with nothing to say so) can
/// never surface on its own, checked per-scope since the same key in
/// *different* scopes is fine (they're independent maps):
///
/// 1. A key string that doesn't parse to anything at all (a typo, an empty
///    string, `"ctrl+x"` — modifiers aren't supported) — that field's
///    action becomes permanently unreachable with no error anywhere else.
/// 2. Two fields in the *same* scope resolving to the same actual
///    `KeyCode` — including non-identical strings that still mean the same
///    key (`"space"` and `" "`), which a plain string comparison would
///    miss. Only one binding survives; which one is whichever `bind()` call
///    happens to run last in `KeyMaps::from_config`'s fixed source order —
///    deliberately not spelled out here (name-only, no expected "winner"),
///    since restating that order here would just be a second copy of it to
///    keep in sync.
fn check_keybinding_health(config: &Config, r: &mut Report) {
    let kb = &config.keybindings;
    let scopes: [(&str, Vec<(&str, &str)>); 4] = [
        (
            "keybindings.global",
            vec![
                ("theme_picker", kb.global.theme_picker.as_str()),
                ("global_search", kb.global.global_search.as_str()),
                ("tags_panel", kb.global.tags_panel.as_str()),
                ("logs", kb.global.logs.as_str()),
                (
                    "toggle_favorite_editor",
                    kb.global.toggle_favorite_editor.as_str(),
                ),
                ("check_update", kb.global.check_update.as_str()),
                ("drawer", kb.global.drawer.as_str()),
                ("undo_delete", kb.global.undo_delete.as_str()),
                ("settings", kb.global.settings.as_str()),
                ("scratchpad", kb.global.scratchpad.as_str()),
            ],
        ),
        (
            "keybindings.notebooks",
            vec![
                ("new", kb.notebooks.new.as_str()),
                ("rename", kb.notebooks.rename.as_str()),
                ("delete", kb.notebooks.delete.as_str()),
                ("sync", kb.notebooks.sync.as_str()),
                ("pull", kb.notebooks.pull.as_str()),
                ("pull_all", kb.notebooks.pull_all.as_str()),
                ("set_remote", kb.notebooks.set_remote.as_str()),
                ("push", kb.notebooks.push.as_str()),
            ],
        ),
        (
            "keybindings.notes",
            vec![
                ("new", kb.notes.new.as_str()),
                ("rename", kb.notes.rename.as_str()),
                ("delete", kb.notes.delete.as_str()),
                ("edit_inline", kb.notes.edit_inline.as_str()),
                ("edit_external", kb.notes.edit_external.as_str()),
                ("search", kb.notes.search.as_str()),
                ("daily_note", kb.notes.daily_note.as_str()),
                ("move_to_notebook", kb.notes.move_to_notebook.as_str()),
                ("sort", kb.notes.sort.as_str()),
                ("tree_view", kb.notes.tree_view.as_str()),
                ("toggle_dates", kb.notes.toggle_dates.as_str()),
                ("new_folder", kb.notes.new_folder.as_str()),
                ("visual", kb.notes.visual.as_str()),
                ("copy_entries", kb.notes.copy_entries.as_str()),
            ],
        ),
        (
            "keybindings.preview",
            vec![
                ("edit_inline", kb.preview.edit_inline.as_str()),
                ("edit_external", kb.preview.edit_external.as_str()),
                ("history", kb.preview.history.as_str()),
                ("links", kb.preview.links.as_str()),
            ],
        ),
    ];

    let mut clean = true;
    for (scope_name, fields) in &scopes {
        for (field, value) in fields {
            if parse_key(value).is_none() {
                clean = false;
                r.warn(
                    "keybindings",
                    format!(
                        "{scope_name}.{field} = \"{value}\" doesn't match any key \u{2014} \
                         that action will never trigger. Valid: a single character, or \
                         enter/tab/esc/space/backspace"
                    ),
                );
            }
        }

        let mut by_code: HashMap<KeyCode, Vec<&str>> = HashMap::new();
        for (field, value) in fields {
            if let Some(code) = parse_key(value) {
                by_code.entry(code).or_default().push(field);
            }
        }
        let mut collisions: Vec<Vec<&str>> = by_code
            .into_values()
            .filter(|fields| fields.len() > 1)
            .collect();
        // `HashMap` iteration order isn't fixed — sort so repeated runs of
        // `doctor` against the same config always print collisions in the
        // same order, not just each individual collision's own field list.
        for fields in &mut collisions {
            fields.sort_unstable();
        }
        collisions.sort();
        for fields in collisions {
            clean = false;
            r.warn(
                "keybindings",
                format!(
                    "{scope_name}: {} are all bound to the same key \u{2014} only one of \
                     them actually works, the others are silently unreachable",
                    fields.join(", ")
                ),
            );
        }
    }

    // `leader`/`quit` are checked as plain (non-scoped) keys in
    // `handle_normal_key`'s match *before* it ever falls through to
    // `resolve_scoped` — so a notebooks/notes/preview binding that happens
    // to equal either one is never reachable there, not just "shadowed"
    // the way two scoped fields sharing a key are. `keybindings.global`
    // itself is exempt: those actions are only ever looked up *after*
    // `leader_pending` is already true, a completely different code path
    // that plain `leader`/`quit` never touches.
    let leader_code = parse_key(&kb.leader);
    let quit_code = parse_key(&kb.quit);
    if let (Some(l), Some(q)) = (leader_code, quit_code) {
        if l == q {
            clean = false;
            r.warn(
                "keybindings",
                format!(
                    "leader (\"{}\") and quit (\"{}\") are the same key \u{2014} quit always \
                     wins, so leader can never actually be pressed",
                    kb.leader, kb.quit
                ),
            );
        }
    }
    for (scope_name, fields) in &scopes[1..] {
        for (field, value) in fields {
            let Some(code) = parse_key(value) else {
                continue;
            };
            if leader_code == Some(code) {
                clean = false;
                r.warn(
                    "keybindings",
                    format!(
                        "{scope_name}.{field} = \"{value}\" is also the leader key \u{2014} \
                         leader always wins, so this action can never trigger"
                    ),
                );
            }
            if quit_code == Some(code) {
                clean = false;
                r.warn(
                    "keybindings",
                    format!(
                        "{scope_name}.{field} = \"{value}\" is also the quit key \u{2014} quit \
                         always wins, so this action can never trigger"
                    ),
                );
            }
        }
    }

    if clean {
        r.pass(
            "keybindings",
            "no unrecognized or colliding keys in any scope",
        );
    }
}

/// A `[snippets.<trigger>]` colliding case-insensitively with *another*
/// custom entry (not a built-in — `slash_menu::all_commands` already
/// handles and reports builtin overrides as intentional) — TOML itself
/// only rejects an exact duplicate key, so `[snippets.H1]` next to
/// `[snippets.h1]` parses fine and silently collapses to just one of them
/// (`all_commands` sorts deterministically so *which* one is at least
/// reproducible, but it's still almost certainly not what was intended).
/// `Config::notebook_custom_paths` silently drops any `[notebooks.<name>]
/// path = "..."` that isn't absolute (see its doc comment for why) — this
/// surfaces that instead of leaving the affected notebook falling back to
/// the default location with no explanation.
fn check_notebook_path_health(config: &Config, r: &mut Report) {
    let relative: Vec<&str> = config
        .notebooks
        .iter()
        .filter_map(|(name, overrides)| {
            let path = overrides.path.as_ref()?;
            (!PathBuf::from(path).is_absolute()).then_some(name.as_str())
        })
        .collect();
    if relative.is_empty() {
        return;
    }
    let mut names = relative;
    names.sort_unstable();
    r.warn(
        "notebook paths",
        format!(
            "non-absolute `path` ignored (falling back to the default location) for: {}",
            names.join(", ")
        ),
    );
}

fn check_snippet_health(config: &Config, r: &mut Report) {
    if config.snippets.is_empty() {
        return;
    }
    let mut by_lower: HashMap<String, Vec<&str>> = HashMap::new();
    for trigger in config.snippets.keys() {
        by_lower
            .entry(trigger.to_lowercase())
            .or_default()
            .push(trigger.as_str());
    }
    let mut collisions: Vec<Vec<&str>> = by_lower
        .into_values()
        .filter(|triggers| triggers.len() > 1)
        .collect();
    for triggers in &mut collisions {
        triggers.sort_unstable();
    }
    collisions.sort();
    if collisions.is_empty() {
        r.pass(
            "snippets",
            format!(
                "{} custom command(s), no trigger collisions",
                config.snippets.len()
            ),
        );
        return;
    }
    for triggers in collisions {
        r.warn(
            "snippets",
            format!(
                "[snippets.{}] all match the same `/`-menu trigger (case-insensitive) \u{2014} \
                 only one applies",
                triggers.join("] / [snippets.")
            ),
        );
    }
}

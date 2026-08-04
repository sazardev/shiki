//! `shiki doctor` — an environment health check, independent of the normal
//! startup path (`Context::load`) so it still works (and says why) even when
//! the config is broken, which is exactly the situation someone reaching for
//! a "doctor" command is usually in.

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::KeyCode;
use shiki_config::config::{NotebookGitOverride, SnippetConfig};
use shiki_config::Config;
use shiki_core::process::on_path;
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

pub fn run() -> Result<()> {
    let mut r = Report::new();
    println!(
        "shiki {} \u{2014} environment check\n",
        env!("CARGO_PKG_VERSION")
    );

    let config_path = Config::default_path();
    let mut raw_contents: Option<String> = None;
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
                    raw_contents = Some(contents);
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
    if data_dir.is_dir() {
        r.pass("data dir", data_dir.display());
    } else if data_dir.exists() {
        r.fail(
            "data dir",
            format!(
                "{} \u{2014} exists but is not a directory (notebook creation will fail)",
                data_dir.display()
            ),
        );
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

    // `pretty-pdf` (go-pretty-pdf, used by `shiki publish`) is fetched
    // automatically the first time it's needed if it's missing from both
    // `$PATH` and shiki's own cache — that's an expected, self-healing
    // state, not a misconfiguration, so this is always a `pass`, never
    // warn/fail.
    let pretty_pdf_cached = data_dir
        .join("bin")
        .join(if cfg!(windows) {
            "pretty-pdf.exe"
        } else {
            "pretty-pdf"
        })
        .is_file();
    if on_path("pretty-pdf") {
        r.pass("pretty-pdf (shiki publish)", "found on PATH");
    } else if pretty_pdf_cached {
        r.pass(
            "pretty-pdf (shiki publish)",
            format!("cached at {}", data_dir.join("bin").display()),
        );
    } else {
        r.pass(
            "pretty-pdf (shiki publish)",
            "not yet downloaded \u{2014} fetched automatically on first `shiki publish`",
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
            check_notebook_path_collisions(&notebooks, &mut r);
            let default_name = &config.general.default_notebook;
            if notebooks.iter().any(|nb| &nb.name == default_name) {
                r.pass("default_notebook", format!("\"{default_name}\" exists"));
            } else {
                r.warn(
                    "default_notebook",
                    format!(
                        "\"{default_name}\" doesn't match any existing notebook \u{2014} commands \
                         that fall back to it (no explicit -n/--notebook) will fail with \
                         \"notebook not found\" until it's created or `general.default_notebook` \
                         is updated"
                    ),
                );
            }
        }
        Err(e) => r.fail("notebooks", e),
    }

    check_keybinding_health(&config, &mut r);
    check_snippet_health(&config, &mut r);
    check_notebook_path_health(&config, &mut r);
    check_theme_health(&config, &mut r);
    if let Some(raw) = &raw_contents {
        check_unknown_config_keys(raw, &mut r);
    }
    check_git_config_health(&config, &mut r);

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
                ("links", kb.global.links.as_str()),
                ("tasks_panel", kb.global.tasks_panel.as_str()),
                ("publish", kb.global.publish.as_str()),
                ("export", kb.global.export.as_str()),
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

/// A typo'd `theme.name` or a malformed hex color override doesn't panic
/// anywhere — `ThemeConfig::resolve()` falls back to `Theme::terminal_default()`
/// for an unknown name, and `shiki-tui::render::hex_to_color` falls back to
/// `Color::Reset` for a bad value — but neither ever tells the user their
/// config value was actually ignored. This surfaces both at the one place a
/// user checking "is my config okay" would look.
/// `Config`'s `#[serde(default)]` fields mean a typo'd key/table anywhere in
/// `config.toml` (`remeber_last_session`, `[keybinding.global]`) is silently
/// ignored rather than rejected — there's no `deny_unknown_fields` here on
/// purpose (that would turn any single typo into a hard failure of the
/// *entire* config, including every field that parsed fine), but a typo
/// still deserves a visible warning somewhere. This diffs the raw parsed
/// TOML against a canonical shape built from `Config::default()` (plus a
/// synthetic single-entry shape for the two dynamic tables,
/// `[notebooks.<name>]`/`[snippets.<trigger>]`, whose own keys are
/// user-chosen names, not part of the schema) — generic over whatever
/// fields `Config`'s structs actually have, so it can't drift out of sync
/// with them the way a hand-maintained list of "known field names" would.
fn check_unknown_config_keys(raw: &str, r: &mut Report) {
    let Ok(raw_value) = toml::from_str::<toml::Value>(raw) else {
        return; // already reported as invalid TOML by the caller
    };
    // `toml::Value::try_from` omits a struct field entirely when it's a
    // `None`-valued `Option<T>` — which `Config::default()` alone would be
    // for every one of these: `general.data_dir`, all 19
    // `ThemeOverrides` slots, `NotebookGitOverride`'s `auto_push`/
    // `auto_sync`/`auto_sync_every`/`path`, and `SnippetConfig.label`. A
    // canonical shape built straight from `Config::default()` would then
    // have *none* of those keys at all, so setting any single one of them
    // — every one a real, documented feature — got flagged as "unrecognized"
    // (caught in review before merge: a config with `theme.bg`,
    // `notebooks.personal.auto_push`, or `snippets.todo.label` set falsely
    // warned on all three). Fixed by explicitly populating every such field
    // with `Some(_)` in the shapes below, so its key actually appears.
    let mut canonical_config = Config::default();
    canonical_config.general.data_dir = Some(String::new());
    canonical_config.theme.overrides =
        shiki_config::config::ThemeOverrides::from_theme(&shiki_config::Theme::terminal_default());
    let Ok(canonical) = toml::Value::try_from(canonical_config) else {
        return;
    };
    let notebook_shape = toml::Value::try_from(NotebookGitOverride {
        auto_push: Some(false),
        auto_sync: Some(false),
        auto_sync_every: Some(1),
        path: Some(String::new()),
        hidden: false,
    })
    .ok();
    let snippet_shape = toml::Value::try_from(SnippetConfig {
        label: Some(String::new()),
        body: String::new(),
    })
    .ok();

    let mut unknown = Vec::new();
    collect_unknown_keys(
        &raw_value,
        &canonical,
        "",
        notebook_shape.as_ref(),
        snippet_shape.as_ref(),
        &mut unknown,
    );

    if unknown.is_empty() {
        r.pass("config keys", "no unrecognized keys found");
        return;
    }
    unknown.sort();
    for key in unknown {
        r.warn(
            "config keys",
            format!(
                "`{key}` in config.toml doesn't match any known setting \u{2014} a typo, or \
                 left over from a different shiki version? It's silently ignored either way"
            ),
        );
    }
}

fn collect_unknown_keys(
    raw: &toml::Value,
    canonical: &toml::Value,
    path: &str,
    notebook_shape: Option<&toml::Value>,
    snippet_shape: Option<&toml::Value>,
    out: &mut Vec<String>,
) {
    let (Some(raw_table), Some(canon_table)) = (raw.as_table(), canonical.as_table()) else {
        return;
    };
    for (key, raw_val) in raw_table {
        let full_path = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        match canon_table.get(key) {
            Some(canon_val) => {
                collect_unknown_keys(
                    raw_val,
                    canon_val,
                    &full_path,
                    notebook_shape,
                    snippet_shape,
                    out,
                );
            }
            None if path == "notebooks" => {
                if let Some(shape) = notebook_shape {
                    collect_unknown_keys(raw_val, shape, &full_path, None, None, out);
                }
            }
            None if path == "snippets" => {
                if let Some(shape) = snippet_shape {
                    collect_unknown_keys(raw_val, shape, &full_path, None, None, out);
                }
            }
            None => out.push(full_path),
        }
    }
}

fn check_theme_health(config: &Config, r: &mut Report) {
    let name = &config.theme.name;
    if name != "default" && shiki_config::themes::by_name(name).is_none() {
        r.warn(
            "theme",
            format!(
                "theme.name = \"{name}\" doesn't match any built-in theme \u{2014} falling back \
                 to \"default\" (terminal colors). Run `shiki theme set <name>` to see valid names"
            ),
        );
    } else {
        r.pass("theme", format!("\"{name}\""));
    }

    let overrides: [(&str, &Option<String>); 19] = [
        ("bg", &config.theme.overrides.bg),
        ("fg", &config.theme.overrides.fg),
        ("accent", &config.theme.overrides.accent),
        ("selection", &config.theme.overrides.selection),
        ("border", &config.theme.overrides.border),
        ("statusbar", &config.theme.overrides.statusbar),
        ("highlight", &config.theme.overrides.highlight),
        ("error", &config.theme.overrides.error),
        ("warning", &config.theme.overrides.warning),
        ("success", &config.theme.overrides.success),
        ("inactive", &config.theme.overrides.inactive),
        ("scrollbar", &config.theme.overrides.scrollbar),
        ("tab_active", &config.theme.overrides.tab_active),
        ("tab_inactive", &config.theme.overrides.tab_inactive),
        ("panel_title", &config.theme.overrides.panel_title),
        ("cursor", &config.theme.overrides.cursor),
        ("link", &config.theme.overrides.link),
        ("tag", &config.theme.overrides.tag),
        ("muted", &config.theme.overrides.muted),
    ];
    let mut bad: Vec<String> = Vec::new();
    for (field, value) in overrides {
        if let Some(v) = value {
            if !is_valid_color_value(v) {
                bad.push(format!("{field} = \"{v}\""));
            }
        }
    }
    if bad.is_empty() {
        return;
    }
    r.warn(
        "theme overrides",
        format!(
            "not a recognized color \u{2014} silently renders as the terminal's reset color \
             instead: {}",
            bad.join(", ")
        ),
    );
}

/// Same set of values `shiki-tui::render::hex_to_color` accepts: a known
/// ANSI name, `"reset"`/empty, or a 6-digit hex string (with or without a
/// leading `#`). Kept independent of `render::hex_to_color` itself (which
/// lives in `shiki-tui` and has no "was this valid" return value, only a
/// color to fall back to) rather than trying to reuse it here.
fn is_valid_color_value(value: &str) -> bool {
    match value.to_ascii_lowercase().as_str() {
        "reset" | "" | "black" | "red" | "green" | "yellow" | "blue" | "magenta" | "cyan"
        | "white" | "gray" | "grey" | "darkgray" | "darkgrey" => return true,
        _ => {}
    }
    let hex = value.trim_start_matches('#');
    hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// `[notebooks.<name>] path = "..."` is validated for being absolute
/// (`Config::notebook_custom_paths`) but nothing checks whether two
/// different notebook names end up pointing at the exact same directory —
/// a copy-pasted override, or one notebook's custom path accidentally
/// matching another's, silently makes both names share a single git repo
/// (writes to one show up as the other). Compares canonicalized paths where
/// possible so a `..`-relative detour or symlink doesn't hide a real
/// collision, falling back to the raw path when canonicalization fails
/// (e.g. the directory doesn't exist yet) rather than skipping the check.
fn check_notebook_path_collisions(notebooks: &[shiki_core::Notebook], r: &mut Report) {
    let mut by_path: HashMap<PathBuf, Vec<&str>> = HashMap::new();
    for nb in notebooks {
        let key = nb.path.canonicalize().unwrap_or_else(|_| nb.path.clone());
        by_path.entry(key).or_default().push(&nb.name);
    }
    let mut collisions: Vec<Vec<&str>> = by_path
        .into_values()
        .filter(|names| names.len() > 1)
        .collect();
    for names in &mut collisions {
        names.sort_unstable();
    }
    collisions.sort();
    for names in collisions {
        r.warn(
            "notebook paths",
            format!(
                "{} all resolve to the same directory on disk \u{2014} they're silently \
                 sharing one git repo",
                names.join(", ")
            ),
        );
    }
}

/// Two purely local `[git]` preconditions doctor can actually verify without
/// touching the network:
///
/// 1. `remote_template` is meant to contain a `{notebook}` placeholder
///    (`App::create_notebook_from_url`'s plain-name path substitutes it in) —
///    a typo'd or missing placeholder means every new notebook silently gets
///    assigned the exact same literal remote URL, which nobody would notice
///    until a push collides.
/// 2. `sign_commits = true` has no effect unless git itself already has a
///    signing key configured (`user.signingkey`, or `gpg.format = ssh`'s
///    `user.signingkey` pointing at an SSH key) — `commit_all`'s signing
///    would otherwise fail on every single commit.
fn check_git_config_health(config: &Config, r: &mut Report) {
    let template = &config.git.remote_template;
    if !template.is_empty() && !template.contains("{notebook}") {
        r.warn(
            "git.remote_template",
            format!(
                "\"{template}\" has no {{notebook}} placeholder \u{2014} every new plain-name \
                 notebook would get assigned this exact same literal remote URL"
            ),
        );
    }

    if config.git.sign_commits {
        let key_configured = std::process::Command::new("git")
            .args(["config", "--get", "user.signingkey"])
            .output()
            .map(|out| out.status.success() && !out.stdout.is_empty())
            .unwrap_or(false);
        if key_configured {
            r.pass("git sign_commits", "user.signingkey is configured");
        } else {
            r.warn(
                "git sign_commits",
                "git.sign_commits is on but `git config user.signingkey` is empty \u{2014} \
                 every commit will fail to sign until it's set",
            );
        }
    }
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

#[cfg(test)]
mod check_unknown_config_keys_tests {
    use super::*;

    #[test]
    fn does_not_flag_legitimate_optional_fields_as_unrecognized() {
        // Every one of these is a real, `Option<T>`-typed field that's
        // `None` in `Config::default()` — a naive canonical shape built
        // straight from `Config::default()` (via `toml::Value::try_from`,
        // which omits `None` fields entirely) flagged all of them as
        // typos the first time this check was written, since setting any
        // one of them meant its key simply didn't exist in the "known"
        // shape to compare against.
        let raw = r##"
[general]
default_notebook = "personal"
data_dir = "/some/custom/path"

[theme]
name = "gruvbox-dark"
bg = "#123456"

[notebooks.personal]
auto_push = true
auto_sync_every = 5
path = "/some/notebook/path"

[snippets.todo]
label = "Todo item"
body = "- [ ] "
"##;
        let mut r = Report::new();
        check_unknown_config_keys(raw, &mut r);
        assert_eq!(
            r.warn, 0,
            "no legitimate field should be flagged as unrecognized"
        );
        assert_eq!(r.ok, 1);
    }

    #[test]
    fn still_flags_a_real_typo() {
        let raw = r#"
[general]
remeber_last_session = true
"#;
        let mut r = Report::new();
        check_unknown_config_keys(raw, &mut r);
        assert_eq!(r.warn, 1);
        assert_eq!(r.ok, 0);
    }

    #[test]
    fn flags_a_nested_notebook_override_typo() {
        let raw = r#"
[notebooks.work]
auto_puush = true
"#;
        let mut r = Report::new();
        check_unknown_config_keys(raw, &mut r);
        assert_eq!(r.warn, 1);
    }
}

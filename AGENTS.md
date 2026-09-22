# AGENTS.md

## Read this first

- **`CLAUDE.md` is the exhaustive source of truth** (layout, keybindings, config schema, git sync, release automation, per-file design rationale). Read the relevant section before any architectural change. `IDEA.md` is the design spec. `AGENTS.md` only covers what you'd otherwise guess wrong.
- This is a Rust Cargo workspace (`shiki-core` → `shiki-config` → `shiki-tui` → `shiki-cli`, strict one-way deps; plus `shiki-native-host` and the Tauri-based `shiki-desktop`, neither of which the terminal crates depend on). Binary name is **`shiki`** (in `shiki-cli/src/main.rs`), launched with no args = TUI, `-- <args>` = CLI subcommands.

## Commands

```sh
cargo check --workspace                          # fast iteration
cargo test --workspace                           # the whole suite
cargo clippy --workspace --all-targets -- -D warnings   # CI gate — the -D warnings matters
cargo fmt --all                                  # then re-clippy
cargo audit                                      # CI runs this; ignore list lives in .cargo/audit.toml
cargo run -p shiki-cli -- --help                 # CLI; no args launches the TUI
```

- CI (`ci.yml`) runs `fmt --check` (ubuntu only), `clippy -D warnings` and `cargo test` on all 3 OSes. Clippy is matrixed because `shiki-core/src/editor.rs` has `#[cfg(target_os = ...)]` blocks.
- Never commit a clippy failure; `-- -D warnings` is enforced in CI.

## Testing

- There are **498 `#[test]`s** (236 in `shiki-core`, 27 `shiki-config`, 196 `shiki-tui`, 33 `shiki-cli`, plus 1 in `shiki-native-host` and 5 in `shiki-desktop`; `shiki-mcp` has none yet — see its own note below). They are all inline `#[cfg(test)]` modules inside source files (no `tests/` dirs, no `#[ignore]`, no fixture setup). `cargo test --workspace` is green.
- To exercise the CLI/TUI without touching real user data, override XDG dirs (used via `directories::ProjectDirs::from("", "", "shiki")`):

```sh
XDG_CONFIG_HOME=/tmp/shiki-test-config XDG_DATA_HOME=/tmp/shiki-test-data cargo run -p shiki-cli -- notebook create personal
```

- For `shiki-tui` logic, prefer pure functions over constructing a full `App` in a test — the pattern is `panel_drawer::drawer_hit_at(notebook_count, area, column, row)` (takes plain numbers, not `&App`).

## Invariants that are easy to break

- **`KeyMaps` matches `KeyCode` only, never the full `KeyEvent`/modifiers.** Shift bindings are plain uppercase chars in `config.toml` — the shipped defaults include `B` (links), `D` (toggle dates), `E` (external edit), `G` (git dash), `H` (history), `M` (metadata), `P` (pull all/publish), `R` (set remote), `T` (tags/tree view) and `U` (check update); making matching modifier-aware silently breaks them.
- **`shiki-config` must stay ratatui-free.** Theme colors are hex strings (`#rrggbb`/ANSI names/`"reset"`); conversion lives only in `shiki-tui/src/render.rs::hex_to_color`.
- **`git2` needs `ssh`, `https`, `vendored-libgit2`, `vendored-openssl`** (root `Cargo.toml`). Dropping the vendored features breaks Windows/aarch64 cross-builds; dropping `ssh`/`https` post-git2-0.21 silently kills remote support and the credential-helper fallback.
- **The terminal crates (`shiki-core`/`shiki-config`/`shiki-tui`/`shiki-cli`) are synchronous + `std::thread`/`mpsc`** — don't introduce async there for new features. (`shiki-desktop` brings its own `tokio` through Tauri; it's the only member that may be async.)
- **`shiki doctor` is dispatched *before* `Context::load()`** in `main.rs` (it must work when `config.toml` is broken). A new subcommand that shouldn't require a working config follows the same pattern.
- **`Cargo.lock` is committed deliberately** (binary crate, reproducible builds). Don't gitignore it.
- **`shiki-mcp` is a real MCP server (workspace member, sibling to `shiki-native-host`/`shiki-desktop`, `rmcp` SDK, stdio transport) — 28 tools calling straight into `shiki-core`, never the `shiki` binary.** It shares `find_note`/`get_notebook`/pagination with `shiki-cli` via `shiki_core::notebook`/`shiki_core::pagination` (promoted there specifically so the two don't carry parallel copies) — put new cross-cutting note-lookup logic there, not in `shiki-cli/src/commands/`. It resolves encrypted-notebook passphrases from `SHIKI_PASSPHRASE` only (no TTY ever, unlike the CLI). It's published to crates.io alongside `shiki-core`/`shiki-config`/`shiki-tui`/`shiki-cli` (`release.yml`'s `publish-crates` loop).
- **`shiki_core::agent_connect` (`shiki agent status/connect/disconnect`) writes to *other tools'* config files** (`.cursor/mcp.json`, `~/.claude.json`, etc.), not shiki's own — every merge touches only the one JSON key it owns (`mcpServers.shiki`/`mcp.shiki`) and refuses outright if the existing file doesn't parse as clean JSON, rather than guessing. Adding a new supported client means adding an `AgentClient` variant + its `config_path`/`entry_value` arms, not a parallel merge function.
- **Every CLI command that resolves a notebook to touch actual note content must call `commands::unlock_if_encrypted` on it first** (`shiki-cli/src/commands/mod.rs`) — `NotebookStore::get`/`get_notebook` never attach crypto on their own. `find_note` takes an already-resolved `&Notebook`, not `(store, name)`, specifically so this can't be skipped. `unlock_if_encrypted` checks `SHIKI_PASSPHRASE` before ever prompting, and errors instead of blocking when stdin isn't a TTY — don't add a new bare `rpassword::prompt_password` call anywhere in `shiki-cli`; a non-interactive agent/script invocation would hang on it.

## Versioning & release

- Single version in `[workspace.package]` (root `Cargo.toml`); all crates inherit via `version.workspace = true`. Bump there + add a `CHANGELOG.md` entry (Keep a Changelog) + update `docs/index.html`'s hardcoded JSON-LD `softwareVersion`.
- **Never `git tag`/push tags by hand.** Include `[PUBLISH]` in the commit message that lands on `main`; `.github/workflows/auto-tag.yml` reads the version from `Cargo.toml`, tags, and triggers `release.yml` (builds → GitHub Release → crates.io → AUR/Scoop manifests). `release.yml` pushes need `secrets.RELEASE_TAG_PAT` (admin PAT) — don't revert those jobs to `GITHUB_TOKEN`; branch protection requires admin.
- **Cut a release with `/deploy`** (command `.opencode/command/deploy.md` → skill `.opencode/skill/deploy/SKILL.md`, verified on v0.9.1): it runs the full runbook — preflight, CHANGELOG, version bump, docs-site sync, image/video scripts, `cargo check/clippy/test/fmt`, commit `[PUBLISH]`, push, CI monitoring, and store-by-store verification — including the three manual fallbacks (workflow_dispatch when release.yml doesn't auto-trigger, manual Homebrew-tap push on the 403, local screenshots/release-pages runs when those jobs get skipped).

## `/docs` (marketing site) sync

- Theme colors in `docs/css/styles.css` and the `THEMES` array in `docs/js/main.js` are copied from `shiki-config/src/themes/*.rs` — update all three together if a palette changes or a theme is added.
- `/screenshots` (repo root) is gitignored; `docs/assets/screenshots/` is not (and is what README.md's `<img>` tags and the site reference).
- `docs/documentation.html` mirrors `IDEA.md` (manually synced — they have drifted before, so check both when editing either).
- `nix/` derivations are drafts validated only by the manual-trigger `nix-package-check.yml` — not a real packaging path.

## Packaging

- `bucket/shiki.json` and `packaging/scoop/shiki.json` are **byte-identical copies** (Scoop bucket install vs. direct-URL install); `release.yml` writes both from one loop. `packaging/aur/` = `shiki-bin` (prebuilt), `packaging/aur-src/` = `shiki` (source build) — both are real AUR packages, don't merge them.

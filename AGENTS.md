# AGENTS.md

## Read this first

- **`CLAUDE.md` is the exhaustive source of truth** (layout, keybindings, config schema, git sync, release automation, per-file design rationale). Read the relevant section before any architectural change. `IDEA.md` is the design spec. `AGENTS.md` only covers what you'd otherwise guess wrong.
- This is a Rust Cargo workspace (`shiki-core` → `shiki-config` → `shiki-tui` → `shiki-cli`, strict one-way deps). Binary name is **`shiki`** (in `shiki-cli/src/main.rs`), launched with no args = TUI, `-- <args>` = CLI subcommands.

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

- There are **~267 `#[test]`s** (134 in `shiki-core`, 17 `shiki-config`, 103 `shiki-tui`, 13 `shiki-cli`) — CLAUDE.md's "almost no tests yet" paragraph is stale. They are all inline `#[cfg(test)]` modules inside source files (no `tests/` dirs, no `#[ignore]`, no fixture setup). `cargo test --workspace` is green.
- To exercise the CLI/TUI without touching real user data, override XDG dirs (used via `directories::ProjectDirs::from("", "", "shiki")`):

```sh
XDG_CONFIG_HOME=/tmp/shiki-test-config XDG_DATA_HOME=/tmp/shiki-test-data cargo run -p shiki-cli -- notebook create personal
```

- For `shiki-tui` logic, prefer pure functions over constructing a full `App` in a test — the pattern is `panel_drawer::drawer_hit_at(notebook_count, area, column, row)` (takes plain numbers, not `&App`).

## Invariants that are easy to break

- **`KeyMaps` matches `KeyCode` only, never the full `KeyEvent`/modifiers.** Shift bindings (`A`, `E`, `T`, `P`, `R`) are plain uppercase chars in `config.toml`; making matching modifier-aware silently breaks them.
- **`shiki-config` must stay ratatui-free.** Theme colors are hex strings (`#rrggbb`/ANSI names/`"reset"`); conversion lives only in `shiki-tui/src/render.rs::hex_to_color`.
- **`git2` needs `ssh`, `https`, `vendored-libgit2`, `vendored-openssl`** (root `Cargo.toml`). Dropping the vendored features breaks Windows/aarch64 cross-builds; dropping `ssh`/`https` post-git2-0.21 silently kills remote support and the credential-helper fallback.
- **`tokio` is a workspace dependency but is not used anywhere** — everything is synchronous + `std::thread`/`mpsc`. Don't introduce async for new features.
- **`shiki doctor` is dispatched *before* `Context::load()`** in `main.rs` (it must work when `config.toml` is broken). A new subcommand that shouldn't require a working config follows the same pattern.
- **`Cargo.lock` is committed deliberately** (binary crate, reproducible builds). Don't gitignore it.

## Versioning & release

- Single version in `[workspace.package]` (root `Cargo.toml`); all crates inherit via `version.workspace = true`. Bump there + add a `CHANGELOG.md` entry (Keep a Changelog) + update `docs/index.html`'s hardcoded JSON-LD `softwareVersion`.
- **Never `git tag`/push tags by hand.** Include `[PUBLISH]` in the commit message that lands on `main`; `.github/workflows/auto-tag.yml` reads the version from `Cargo.toml`, tags, and triggers `release.yml` (builds → GitHub Release → crates.io → AUR/Scoop manifests). `release.yml` pushes need `secrets.RELEASE_TAG_PAT` (admin PAT) — don't revert those jobs to `GITHUB_TOKEN`; branch protection requires admin.

## `/docs` (marketing site) sync

- Theme colors in `docs/css/styles.css` and the `THEMES` array in `docs/js/main.js` are copied from `shiki-config/src/themes/*.rs` — update all three together if a palette changes or a theme is added.
- `/screenshots` (repo root) is gitignored; `docs/assets/screenshots/` is not (and is what README.md's `<img>` tags and the site reference).
- `docs/documentation.html` is copied verbatim from `IDEA.md` — don't let them drift.
- `nix/` derivations are drafts validated only by the manual-trigger `nix-package-check.yml` — not a real packaging path.

## Packaging

- `bucket/shiki.json` and `packaging/scoop/shiki.json` are **byte-identical copies** (Scoop bucket install vs. direct-URL install); `release.yml` writes both from one loop. `packaging/aur/` = `shiki-bin` (prebuilt), `packaging/aur-src/` = `shiki` (source build) — both are real AUR packages, don't merge them.

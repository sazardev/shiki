---
name: deploy
description: >
  Cuts a full shiki release (bump to the next version, CHANGELOG, docs site, image/video scripts,
  CI through GitHub Release, every store, and the marketing site) end-to-end. Use when the user says
  "publica", "haz el release", "cut a release", "release vX.Y.Z", "bump a X.Y.Z", "sube la version",
  "deploy", "publish", or invokes /deploy. Encodes the three non-obvious manual fallbacks discovered
  on the v0.9.1 cut (release.yml not auto-triggering, Homebrew tap 403, screenshots/release-pages
  jobs skipped) so a release never silently half-publishes.
---

# Shiki release runbook (verified on v0.9.1)

The complete flow for publishing a shiki release, in order. Every command below was exercised for
the v0.9.1 cut. Do the steps in sequence and verify each checkpoint before moving on. The three
fallbacks in **Phase 6–8 are part of the real flow**, not exceptional edge cases.

## Phase 0 — preflight

- [ ] Working tree clean, on `main`, up to date: `git status --short`, `git log --oneline -3`.
- [ ] Know the target version (next patch/minor from `[workspace.package] version` in root
      `Cargo.toml`). Default: next patch (`0.9.1` → `0.9.2`).
- [ ] Enumerate what's changed since the last tag:
      `git log --oneline <last-tag>..main` (e.g. `git log --oneline v0.9.1..main`).

## Phase 1 — CHANGELOG

- [ ] Audit the `## [Unreleased]` section against the commits in Phase 0: every user-facing change
      needs a bullet, and every bullet should trace to real code. If unsure, run the
      `docs-coherence` skill first — its report flags exactly the missing entries (this caught the
      bold/italic slash commands, theme-adaptive syntax colors, preview-scroll fix, and autocrlf
      fix missing from the v0.9.1 notes).
- [ ] Rename `## [Unreleased]` to `## [<version>] - <YYYY-MM-DD>` (today's date), and re-add an
      empty `## [Unreleased]` header at the top of the changelog (Keep a Changelog convention).

## Phase 2 — version bump

- [ ] Root `Cargo.toml`: `[workspace.package] version = "X.Y.Z"`.
- [ ] Root `Cargo.toml`: the three `[workspace.dependencies]` `shiki-core`/`shiki-config`/
      `shiki-tui` `version = "X.Y.Z"` (plain `sed -i 's/version = "OLD"/version = "NEW"/' Cargo.toml`
      covers all four in one shot). `Cargo.lock` picks them up on the next `cargo check`.
- [ ] `docs/index.html`: the hardcoded JSON-LD `"softwareVersion": "X.Y.Z"` (the nav version pill
      and download button are fetched live; this one is not — bump by hand).

## Phase 3 — docs site + config prose

- [ ] If a doc-audit found drift, fix the flagged files. Known hotspot from v0.9.1: the built-in
      `/`-menu command count and list appear in **four** places and all must stay in sync —
      `docs/documentation.html` (the `19 built-in commands` prose + the sample config block),
      `IDEA.md` (same two spots), and `shiki-config/src/config.rs` (`section_comment` for
      `[snippets]`). When `slash_menu.rs::builtins()` changes, update all of them (and the count).
- [ ] If `config.rs` gained a `[general]`/other field, add it to that table's `section_comment`
      block (a fresh `shiki config` run surfaces exactly which keys the prose omits).

## Phase 4 — image/video scripts

- [ ] If this release's headline features are visual (PREVIEW rendering, new editor UX, new
      modals), showcase them:
  - `scripts/screenshots.sh`: add a sample note in `setup_sample_data` + a `shot "NN-…"` capture
    in the `wide` branch (same title-jump pattern as the existing shots), so the marketing
    screenshots don't go stale.
  - `scripts/demo-gif.sh`: add a note in `setup_sample_data` (put it in `research/` — the flat,
    index-independent notebook — never in `personal/` where hardcoded phase indices live) + a new
    Phase block before the quit, using a **unique** fuzzy-search query that matches only that note
    (see the "rollup" comment on Phase 20 for why "release day" was wrong).
- [ ] `bash -n` both scripts. If local tooling is available (vhs, xterm, xdotool, Xvfb, ffmpeg,
      ttyd, a Nerd Font), do a real smoke test: run `scripts/demo-gif.sh /tmp/test.gif` and a
      single-theme screenshot run before committing.

## Phase 5 — verify + commit + push

- [ ] `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace`, `cargo fmt --all -- --check` — all green.
- [ ] Commit **with `[PUBLISH]` anywhere in the message** (this is what auto-tag watches for),
      conventional style: `chore: cut release for vX.Y.Z [PUBLISH]`. Push to `main`.
- [ ] Watch `Auto Tag on Publish` run to completion (it reads the version from `Cargo.toml` and
      pushes the `vX.Y.Z` tag via `RELEASE_TAG_PAT`).

## Phase 6 — make sure release.yml actually runs  ⚠️

The tag push *often does not* trigger `release.yml` (confirmed on v0.8.4 and v0.9.1). Check:

```
gh run list --limit 8
```

- [ ] If a `Release` run appears (event `push` or `workflow_dispatch`) within ~1–2 min after
      auto-tag finishes, monitor it (`gh run watch <id>`).
- [ ] **If no Release run appears, dispatch it manually** — this is the documented fallback:
      `gh workflow run release.yml --ref main --field tag=vX.Y.Z`. The `resolve-tag` job makes this
      equivalent to a real tag push.

## Phase 7 — monitor release.yml

Jobs run in this order (some in parallel): `resolve-tag` → 4× `build` → `release` (creates the
GitHub Release + assets) → `publish-crates` + `update-packaging-manifests` (parallel) →
`update-screenshots` → `update-release-pages`.

- [ ] `release` completes → the GitHub Release with 4 assets + SHA256SUMS.txt exists.
- [ ] `publish-crates` → all 4 crates (`shiki-core` → `shiki-config` → `shiki-tui` → `shiki-cli`)
      land on crates.io (verify each: `curl -s -H "User-Agent: …" https://crates.io/api/v1/crates/<crate>`).
- [ ] `update-packaging-manifests`:
  - Scoop manifests (both `bucket/shiki.json` and `packaging/scoop/shiki.json`) updated + committed
    to `main` (they must stay byte-identical — `cmp` them).
  - AUR: `packaging/aur` (shiki-bin) and `packaging/aur-src` (shiki) PKGBUILDs + .SRCINFO updated,
    and **pushed to the AUR repos themselves**.
  - Homebrew tap push — see the ⚠️ below.

### ⚠️ Homebrew tap 403 (expected to fail on v0.9.1-era PAT)

`Push Homebrew tap` step fails with `Permission to sazardev/homebrew-shiki.git denied` when
`RELEASE_TAG_PAT` can't write to the tap repo. This fails the whole job **after** everything else
in it succeeded. Do the tap push manually:

```
cd /tmp && rm -rf hb && git clone https://github.com/sazardev/homebrew-shiki.git hb
cp /home/omar/personal/shiki/packaging/homebrew/shiki.rb hb/Formula/shiki.rb
cd hb && git add Formula/shiki.rb
git -c user.name="github-actions[bot]" -c user.email="github-actions[bot]@users.noreply.github.com" commit -m "Update to vX.Y.Z"
git push origin HEAD:main
```

Verify afterwards that `version "X.Y.Z"` and both sha256s in the formula match the release's
SHA256SUMS.txt.

## Phase 8 — skipped jobs need local runs  ⚠️

Because `update-screenshots` and `update-release-pages` are `needs:`-gated on
`update-packaging-manifests`, when that job fails at the Homebrew step they are **skipped**, leaving
the site stale. Replicate them locally:

- [ ] Regenerate everything:
  `bash scripts/screenshots.sh` (needs xterm/ImageMagick/xdotool/Xvfb/pngquant + Nerd Font),
  `bash scripts/demo-gif.sh` (needs vhs/ttyd/ffmpeg), then copy into `docs/` exactly like
  `release.yml` does:
  - `docs/assets/screenshots/<theme>.png` ← `screenshots/<theme>/wide-01-notebooks.png` (12 themes)
  - `docs/assets/screenshots/gallery/<theme>/` ← every capture per theme
  - `pngquant --quality=80-95 --skip-if-larger --ext .png --force docs/assets/screenshots/gallery/*/*.png`
  - `docs/assets/demo.gif` ← `scripts/demo-gif.sh` output
  - Release page + OG + feed: `python3 scripts/generate_release_pages.py --version X.Y.Z --chromium <path>`
    (commits `docs/changelog/X.Y.Z.html`, `docs/assets/og/X.Y.Z.png`, `docs/sitemap.xml`, `docs/feed.xml`).
- [ ] `git add docs/`, commit `docs: refresh screenshots, demo gif, release page and OG card for
      vX.Y.Z`, `git pull --rebase origin main` (the packaging job's own commit may have landed),
      push.

## Phase 9 — verify every store

- [ ] GitHub Release: `gh release view vX.Y.Z` — 4 assets present; download + `sha256sum -c`
      against SHA256SUMS.txt.
- [ ] crates.io: all four crates `max_stable_version == X.Y.Z`, none yanked.
- [ ] AUR: `shiki-bin` and `shiki` both `X.Y.Z-1` via the AUR RPC; PKGBUILD sha256 for shiki-bin
      matches the release's linux tarball, and `shiki`'s sha256 matches the source tarball
      (`https://github.com/sazardev/shiki/archive/refs/tags/vX.Y.Z.tar.gz`).
- [ ] Homebrew: tap formula `version "X.Y.Z"` + arm/intel sha256 match release.
- [ ] Scoop: both manifests version X.Y.Z, hash matches the Windows zip, `extract_dir` matches the
      zip's top-level folder, and the two files are byte-identical.
- [ ] Site: `pages.yml` deploy green; `https://shiki.sazardev.stream/` `softwareVersion` =
      X.Y.Z; `/changelog/X.Y.Z.html`, `/assets/og/X.Y.Z.png`, `/feed.xml` all HTTP 200.

## Phase 10 — clean up

- [ ] `rm -rf screenshots` (gitignored local output) if a local run created it; confirm
      `git status --short` is clean.
- [ ] Report to the user: version, GitHub Release URL, and the store-by-store verification table,
      plus any manual fallback that was needed (Homebrew tap push, workflow_dispatch, local
      screenshots run) so they know which CI jobs are still red and why.

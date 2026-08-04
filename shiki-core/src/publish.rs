//! Renders a notebook's notes to a themed PDF via `pretty-pdf`
//! (`sazardev/go-pretty-pdf`), a separate Go project shelled out to as an
//! external process — never linked in, same as `editor`/`browser`. The
//! binary itself is fetched automatically the first time it's needed (see
//! `ensure_binary`), so using this feature never requires a manual install
//! step.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{Error, Note, Result};

const REPO_OWNER: &str = "sazardev";
const REPO_NAME: &str = "go-pretty-pdf";
const BIN_NAME: &str = "pretty-pdf";

fn bin_file_name() -> &'static str {
    if cfg!(windows) {
        "pretty-pdf.exe"
    } else {
        "pretty-pdf"
    }
}

/// go-pretty-pdf's release assets are named `go-pretty-pdf_{version}_{os}_{arch}.{ext}`
/// — plain `{os}_{arch}` strings, not Rust target triples — so this maps the
/// current platform to the exact substring `self_update`'s asset matching
/// needs to find (see `ensure_binary`'s `.target(...)`).
fn release_asset_target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("linux_amd64"),
        ("linux", "aarch64") => Ok("linux_arm64"),
        ("macos", "x86_64") => Ok("darwin_amd64"),
        ("macos", "aarch64") => Ok("darwin_arm64"),
        ("windows", "x86_64") => Ok("windows_amd64"),
        (os, arch) => Err(Error::Publish(format!(
            "no pretty-pdf build available for {os}/{arch} \u{2014} see \
             https://github.com/{REPO_OWNER}/{REPO_NAME}/releases"
        ))),
    }
}

/// Resolves a usable `pretty-pdf` binary: `$PATH` first (so a manually
/// installed copy is always respected and never re-downloaded), then the
/// cache file under `cache_dir` (left over from a previous call), and only
/// downloads a fresh copy when neither exists. `cache_dir` is the caller's
/// own directory to manage (e.g. `{data_dir}/bin`) — this module doesn't
/// resolve XDG paths itself, matching `shiki-core`'s existing
/// config-crate-free design (see `update.rs` for the same split).
pub fn ensure_binary(cache_dir: &Path) -> Result<PathBuf> {
    if crate::process::on_path(BIN_NAME) {
        return Ok(PathBuf::from(BIN_NAME));
    }
    let cached = cache_dir.join(bin_file_name());
    if cached.is_file() {
        return Ok(cached);
    }

    std::fs::create_dir_all(cache_dir)?;
    let target = release_asset_target()?;
    let mut builder = self_update::backends::github::Update::configure();
    builder
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name(BIN_NAME)
        .target(target)
        // go-pretty-pdf ships both a `.tar.gz` and a `.zip` per platform —
        // without this, `target`'s substring match against the release's
        // asset list is ambiguous (two assets contain the same target
        // string).
        .asset_identifier(if cfg!(windows) { ".zip" } else { ".tar.gz" })
        .bin_path_in_archive(bin_file_name())
        // The one option that makes this install *to our own cache file*
        // instead of replacing `current_exe()`, unlike shiki's own
        // self-updater (`update.rs`), which is intentionally replacing the
        // running binary.
        .bin_install_path(&cached)
        .show_download_progress(false)
        .show_output(false)
        .no_confirm(true)
        // GitHub computes and serves a sha256 digest per release asset —
        // same integrity check `update.rs` already relies on for shiki's
        // own releases, here applied to a different repo's releases.
        .verify_release_digest(true)
        // This path only runs when `cached` doesn't exist yet, so it should
        // always fetch the latest release — there's no installed version to
        // compare against.
        .current_version("0.0.0");
    let updater = builder.build().map_err(|e| Error::Publish(e.to_string()))?;
    updater.update().map_err(|e| Error::Publish(e.to_string()))?;
    Ok(cached)
}

/// Minimal frontmatter go-pretty-pdf actually requires — deliberately not
/// shiki's own `Frontmatter` shape (date/tags/notebook/links mean nothing to
/// it); its README documents "Required frontmatter fields: id, title."
#[derive(Serialize)]
struct MdxFrontmatter {
    id: String,
    title: String,
}

/// Renders `notes` (already sorted by the caller — the same `(date, title)`
/// order `commands/export.rs` uses) into a themed PDF at `out` via
/// `pretty-pdf`. Downloads/caches the binary first if needed (see
/// `ensure_binary`).
pub fn publish(notes: &[Note], theme: &str, cache_dir: &Path, out: &Path) -> Result<()> {
    let bin = ensure_binary(cache_dir)?;

    let tmp = tempfile::tempdir()?;
    for (i, note) in notes.iter().enumerate() {
        let frontmatter = MdxFrontmatter {
            id: format!("[{}.0.0]", i + 1),
            title: note.frontmatter.title.clone(),
        };
        let yaml = serde_yaml::to_string(&frontmatter)?;
        let contents = format!("---\n{yaml}---\n\n{}\n", note.body);
        // `.mdx`, not `.md` — verified live against a real `pretty-pdf`
        // binary: it only picks up `.mdx` files from `--source`, despite
        // go-pretty-pdf's own README describing `.md` as accepted too.
        let filename = format!("{:03}-{}.mdx", i + 1, note.file_stem());
        std::fs::write(tmp.path().join(filename), contents)?;
    }

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let result = std::process::Command::new(&bin)
        .arg("build")
        .arg("--source")
        .arg(tmp.path())
        .arg("--theme")
        .arg(theme)
        .arg("--out")
        .arg(out)
        .output()
        .map_err(|e| Error::Publish(format!("failed to run '{}': {e}", bin.display())))?;

    if !result.status.success() {
        return Err(Error::Publish(format!(
            "pretty-pdf build failed: {}",
            String::from_utf8_lossy(&result.stderr)
        )));
    }
    Ok(())
}

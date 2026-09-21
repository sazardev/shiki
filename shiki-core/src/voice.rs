//! Voice capture for `shiki capture --voice` — records from the microphone
//! and transcribes locally with whisper.cpp (`whisper-cli`), the same
//! external-binary pattern as `publish`'s `pretty-pdf` (auto-fetched from
//! its own GitHub release) and `spell`'s `hunspell` (must be installed).
//! Nothing leaves the machine: the audio is a local temp file, the model
//! runs on the CPU, and only the resulting transcript text is handed to
//! shiki's normal capture path.
//!
//! Recording uses the first available tool of `arecord` (Linux/ALSA, the
//! canonical 16 kHz WAV recorder), `ffmpeg` (any platform), or `sox` — the
//! same "external binary, clear error if missing" approach as the rest of
//! the codebase. The whisper.cpp binary itself is auto-fetched from
//! ggml-org/whisper.cpp's GitHub releases the first time it's needed (the
//! `whisper-bin-*` assets, cached under `{data_dir}/bin`), and the model
//! (`ggml-*.bin`) is downloaded once from Hugging Face into
//! `{data_dir}/bin/models/` — `curl`/`wget` is used for that, since
//! `self_update` only talks to GitHub Releases.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::{process::on_path, Error, Result};

/// The default whisper.cpp model — a ~140 MB English-only `base` model,
/// the usual trade-off between transcription quality and CPU speed for a
/// quick voice capture.
pub const DEFAULT_MODEL: &str = "ggml-base.en.bin";

#[cfg(feature = "self-update")]
const WHISPER_OWNER: &str = "ggml-org";
#[cfg(feature = "self-update")]
const WHISPER_REPO: &str = "whisper.cpp";
/// whisper.cpp's converted models live in a Hugging Face repo (its own
/// `models/download-ggml-model.sh` uses the same `resolve/main` URL).
const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
/// The `/raw/…` twin of `MODEL_BASE_URL`: same repo and path, but serving
/// the small git-lfs pointer (which carries the file's sha256 and size)
/// instead of the multi-megabyte resolved model. Used purely to learn the
/// expected digest before trusting a download.
const MODEL_RAW_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/raw/main";

fn bin_file_name() -> &'static str {
    if cfg!(windows) {
        "whisper-cli.exe"
    } else {
        "whisper-cli"
    }
}

/// Whether *something* is available to record the microphone — used by
/// `shiki doctor` to warn before a `--voice` capture fails at the recording
/// step.
pub fn recorder_available() -> bool {
    on_path("arecord") || on_path("ffmpeg") || on_path("sox")
}

/// Whether a `whisper-cli` binary exists on `$PATH` or in shiki's own
/// cache dir (`cache_dir` is the caller's `{data_dir}/bin`) — used by
/// `shiki doctor`. A missing binary is a self-healing state (auto-fetched
/// on first use), so doctor reports it as informational, not a failure.
pub fn whisper_available(cache_dir: &Path) -> bool {
    on_path("whisper-cli") || cache_dir.join(bin_file_name()).is_file()
}

/// Outcome of one recorder attempt: whether it succeeded, plus its stderr
/// (collected rather than inherited, so a failed attempt is silent and the
/// reason is only surfaced if *every* recorder fails).
struct RecorderResult {
    success: bool,
    stderr: String,
}

/// Runs `command`, killing it if it hasn't exited within `timeout` —
/// std-only, so a recorder that hangs opening a missing audio device (a
/// real failure mode: `ffmpeg -f pulse` blocks indefinitely with no
/// PulseAudio/pipewire running) fails fast instead of wedging the whole
/// capture.
fn run_with_timeout(command: &mut Command, timeout: Duration) -> RecorderResult {
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            return RecorderResult {
                success: false,
                stderr: format!("could not spawn: {e}"),
            };
        }
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut buf = String::new();
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_string(&mut buf);
                }
                return RecorderResult {
                    success: status.success(),
                    stderr: buf,
                };
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return RecorderResult {
                        success: false,
                        stderr: format!("timed out after {}s", timeout.as_secs()),
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return RecorderResult {
                    success: false,
                    stderr: format!("{e}"),
                };
            }
        }
    }
}

/// Fast pre-check that a PulseAudio/pipewire socket actually exists on
/// Linux — `ffmpeg -f pulse -i default` otherwise hangs *opening* the
/// device when no sound server is running, and arecord/sox already fail
/// fast on their own, so skipping the ffmpeg attempt entirely is the
/// difference between an instant error and a hang.
fn linux_pulse_available() -> bool {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .is_some_and(|d| d.join("pulse/native").exists() || d.join("pipewire-0").exists())
}

/// Records `seconds` of microphone audio as a 16 kHz mono WAV at `out`,
/// trying `arecord` → `ffmpeg` → `sox` in that order. Returns a clear
/// error only when none of the three exists (or all three failed),
/// including the last recorder's own stderr so a real device problem
/// isn't reported as "nothing is installed".
pub fn record_to_wav(out: &Path, seconds: u32) -> Result<()> {
    let dur = seconds.to_string();
    let timeout = Duration::from_secs(u64::from(seconds) + 5);
    let mut last_error = String::new();

    // Linux/ALSA: `arecord -f S16_LE -r 16000 -c 1` produces exactly the
    // 16 kHz mono WAV whisper expects, in one shot, no format conversion.
    if cfg!(target_os = "linux") && on_path("arecord") {
        let res = run_with_timeout(
            Command::new("arecord")
                .args([
                    "-f", "S16_LE", "-r", "16000", "-c", "1", "-d", &dur, "-t", "wav",
                ])
                .arg(out),
            timeout,
        );
        if res.success {
            return Ok(());
        }
        last_error = res.stderr;
    }

    if on_path("ffmpeg") && !(cfg!(target_os = "linux") && !linux_pulse_available()) {
        // Device selection is platform-specific; `default`/`:0`/`audio=default`
        // are the sane defaults per OS (a wrong one fails fast and we move
        // on to the next recorder).
        let input: &[&str] = if cfg!(target_os = "linux") {
            &["-f", "pulse", "-i", "default"]
        } else if cfg!(target_os = "macos") {
            &["-f", "avfoundation", "-i", ":0"]
        } else {
            &["-f", "dshow", "-i", "audio=default"]
        };
        let res = run_with_timeout(
            Command::new("ffmpeg")
                .args(["-y", "-loglevel", "error"])
                .args(input)
                .args(["-t", &dur, "-ar", "16000", "-ac", "1"])
                .arg(out),
            timeout,
        );
        if res.success {
            return Ok(());
        }
        last_error = res.stderr;
    }

    if on_path("sox") {
        let res = run_with_timeout(
            Command::new("sox")
                .args(["-d", "-r", "16000", "-c", "1"])
                .arg(out)
                .args(["trim", "0", &dur]),
            timeout,
        );
        if res.success {
            return Ok(());
        }
        last_error = res.stderr;
    }

    let detail = last_error
        .lines()
        .next()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or("none available");
    Err(Error::Voice(format!(
        "voice capture failed \u{2014} no recorder produced audio (arecord/ffmpeg/sox); last attempt: {detail}"
    )))
}

/// whisper.cpp's release assets are named `whisper-bin-{platform}.{ext}` —
/// plain platform strings, not Rust target triples — so this maps the
/// current platform to the exact asset `self_update` should match (see
/// `ensure_whisper`). macOS only ships an `.xcframework` (no CLI binary),
/// so macOS must have `whisper-cli` on `$PATH` instead.
#[cfg(feature = "self-update")]
fn release_asset_target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("whisper-bin-ubuntu-x64.tar.gz"),
        ("linux", "aarch64") => Ok("whisper-bin-ubuntu-arm64.tar.gz"),
        ("windows", "x86_64") => Ok("whisper-bin-x64.zip"),
        (os, arch) => Err(Error::Voice(format!(
            "no whisper.cpp prebuilt binary for {os}/{arch} \u{2014} install `whisper-cli` on $PATH \
             (e.g. `brew install whisper-cpp`, or build whisper.cpp) and try again"
        ))),
    }
}

/// Resolves a usable `whisper-cli`: `$PATH` first (a manually installed
/// copy is always respected, never re-downloaded), then the cache file
/// under `cache_dir`, and only downloads a fresh copy from ggml-org/
/// whisper.cpp's GitHub release when neither exists — the same shape as
/// `publish::ensure_binary`. `cache_dir` is the caller's own directory
/// (e.g. `{data_dir}/bin`).
pub fn ensure_whisper(cache_dir: &Path) -> Result<PathBuf> {
    if on_path("whisper-cli") {
        return Ok(PathBuf::from("whisper-cli"));
    }
    let cached = cache_dir.join(bin_file_name());
    if cached.is_file() {
        return Ok(cached);
    }

    #[cfg(feature = "self-update")]
    {
        std::fs::create_dir_all(cache_dir)?;
        let target = release_asset_target()?;
        let mut builder = self_update::backends::github::Update::configure();
        builder
            .repo_owner(WHISPER_OWNER)
            .repo_name(WHISPER_REPO)
            .bin_name(bin_file_name())
            // The full asset filename as the target substring — `whisper-bin-x64.zip`
            // uniquely matches the plain build and skips `whisper-blas-bin-x64.zip`/
            // `whisper-cublas-*.zip`, which share the platform string.
            .target(target)
            .asset_identifier(if cfg!(windows) { ".zip" } else { ".tar.gz" })
            .bin_path_in_archive(bin_file_name())
            .bin_install_path(&cached)
            .show_download_progress(false)
            .show_output(false)
            .no_confirm(true)
            // GitHub computes and serves a sha256 digest per release asset —
            // same integrity check `publish::ensure_binary`/`update.rs` rely on.
            .verify_release_digest(true)
            .current_version("0.0.0");
        let updater = builder.build().map_err(|e| Error::Voice(e.to_string()))?;
        updater.update().map_err(|e| Error::Voice(e.to_string()))?;
        Ok(cached)
    }
    // Without the `self-update` feature (default-on for every existing
    // consumer), there's no way to auto-fetch `whisper-cli` — same "clear
    // error, not a silent no-op" contract every other missing-binary case in
    // this crate already follows.
    #[cfg(not(feature = "self-update"))]
    {
        Err(Error::Voice(
            "whisper-cli isn't on $PATH and this build has no `self-update` \
             feature to fetch it automatically"
                .to_string(),
        ))
    }
}

/// Ensures `model` (e.g. `ggml-base.en.bin`) is present in `model_dir`,
/// downloading it from Hugging Face once via `curl`/`wget` if missing.
/// Returns the path to the model file.
///
/// The download is verified against the sha256 Hugging Face publishes for
/// the file (the repo's git-lfs pointer) before it's renamed into place — a
/// corrupt or substituted model is removed and reported, never handed to
/// `whisper-cli`.
pub fn ensure_model(model_dir: &Path, model: &str) -> Result<PathBuf> {
    validate_model_name(model)?;
    std::fs::create_dir_all(model_dir)?;
    let path = model_dir.join(model);
    if path.is_file() {
        return Ok(path);
    }
    download_verified(model_dir, model)?;
    Ok(path)
}

/// `model` is joined straight onto `model_dir` and interpolated into the
/// download URL, so it has to be a bare `ggml-*.bin` filename: no path
/// separators and no `..` (which would let a crafted value write outside
/// `model_dir`), nothing that could turn the URL into a different path.
fn validate_model_name(model: &str) -> Result<()> {
    let valid = model.starts_with("ggml-")
        && model.ends_with(".bin")
        && !model.contains(['/', '\\'])
        && !model.contains("..");
    if valid {
        Ok(())
    } else {
        Err(Error::Voice(format!(
            "invalid model name '{model}' \u{2014} expected a bare 'ggml-*.bin' filename"
        )))
    }
}

/// Parses the git-lfs pointer text Hugging Face serves for a model into
/// `(sha256_hex, size_bytes)`. `None` for anything that isn't a well-formed
/// pointer.
fn parse_lfs_pointer(text: &str) -> Option<(String, u64)> {
    let mut oid = None;
    let mut size = None;
    for line in text.lines() {
        if let Some(hex) = line.strip_prefix("oid sha256:") {
            oid = Some(hex.trim());
        } else if let Some(n) = line.strip_prefix("size ") {
            size = n.trim().parse::<u64>().ok();
        }
    }
    let oid = oid?;
    if oid.len() != 64 || !oid.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some((oid.to_ascii_lowercase(), size?))
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    // sha2 0.11's `finalize()` returns `hybrid_array::Array`, which no
    // longer implements `LowerHex` (unlike the old `generic_array`-based
    // type) — format each byte by hand instead of relying on `{:x}`.
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// Downloads the git-lfs pointer for `model` and returns its expected
/// digest. Fails closed: when the pointer can't be fetched or doesn't look
/// like one, the model is not downloaded at all — installing something
/// unverified is the exact failure mode this exists to prevent.
fn fetch_expected_digest(model_dir: &Path, model: &str) -> Result<(String, u64)> {
    let url = format!("{MODEL_RAW_BASE_URL}/{model}");
    let ptr = model_dir.join(format!("{model}.ptr"));
    if curl_download(&url, &ptr).is_err() && wget_download(&url, &ptr).is_err() {
        let _ = std::fs::remove_file(&ptr);
        return Err(Error::Voice(format!(
            "could not fetch integrity metadata for '{model}' from {url} \u{2014} need curl or wget on $PATH"
        )));
    }
    let result = std::fs::metadata(&ptr)
        .and_then(|m| {
            // A real pointer is a handful of lines; a much larger response
            // means this path didn't resolve to one, so refuse instead of
            // parsing a multi-megabyte body.
            if m.len() > 4096 {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "integrity metadata is not a git-lfs pointer",
                ))
            } else {
                std::fs::read_to_string(&ptr)
            }
        })
        .map_err(Error::from)
        .and_then(|text| {
            parse_lfs_pointer(&text).ok_or_else(|| {
                Error::Voice(format!(
                    "unexpected integrity metadata for '{model}' \u{2014} refusing to install an unverified model"
                ))
            })
        });
    let _ = std::fs::remove_file(&ptr);
    result
}

fn download_verified(model_dir: &Path, model: &str) -> Result<()> {
    let (expected_hash, expected_size) = fetch_expected_digest(model_dir, model)?;
    let url = format!("{MODEL_BASE_URL}/{model}");
    let tmp = model_dir.join(format!("{model}.part"));
    if curl_download(&url, &tmp).is_err() && wget_download(&url, &tmp).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::Voice(format!(
            "could not download whisper model '{model}' from {url} \u{2014} need curl or wget on $PATH"
        )));
    }
    let checked = std::fs::metadata(&tmp)
        .map(|m| m.len())
        .map_err(Error::from)
        .and_then(|size| sha256_file(&tmp).map(|hash| (size, hash)));
    match checked {
        Ok((size, hash)) if size == expected_size && hash == expected_hash => {
            std::fs::rename(&tmp, model_dir.join(model))?;
            Ok(())
        }
        Ok((size, hash)) => {
            let _ = std::fs::remove_file(&tmp);
            Err(Error::Voice(format!(
                "integrity check failed for '{model}' (expected {expected_size} bytes sha256:{expected_hash}, got {size} bytes sha256:{hash}) \u{2014} download removed"
            )))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn curl_download(url: &str, dest: &Path) -> Result<()> {
    if !on_path("curl") {
        return Err(Error::Voice("curl not on $PATH".into()));
    }
    let status = Command::new("curl")
        .args(["-L", "-sS", "--fail", "-o"])
        .arg(dest)
        .arg(url)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Voice("curl download failed".into()))
    }
}

fn wget_download(url: &str, dest: &Path) -> Result<()> {
    if !on_path("wget") {
        return Err(Error::Voice("wget not on $PATH".into()));
    }
    let status = Command::new("wget")
        .args(["-q", "-O"])
        .arg(dest)
        .arg(url)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Voice("wget download failed".into()))
    }
}

/// Runs `whisper-cli` on `wav` and returns the plain transcript text
/// (`-nt`, no timestamps, so stdout is just the words).
pub fn transcribe(bin: &Path, model: &Path, wav: &Path) -> Result<String> {
    let out = Command::new(bin)
        .args(["-m"])
        .arg(model)
        .args(["-f"])
        .arg(wav)
        .args(["-nt"])
        .output()
        .map_err(|e| Error::Voice(format!("failed to run '{}': {e}", bin.display())))?;
    if !out.status.success() {
        return Err(Error::Voice(format!(
            "whisper-cli failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        return Err(Error::Voice(
            "whisper-cli produced no transcript \u{2014} did the microphone hear anything?".into(),
        ));
    }
    Ok(text)
}

/// The full `shiki capture --voice` pipeline: record `seconds` to a temp
/// WAV, make sure `whisper-cli` + the model exist (fetching both on first
/// use), transcribe, and return the transcript — which the caller then
/// captures like any other text.
/// A pluggable voice-capture capability — `NativeVoice` just calls the free
/// functions below (which shell out to a recorder + `whisper-cli`); a
/// future non-native consumer (e.g. one with no local microphone/process to
/// spawn) can implement this instead. Purely additive: `recorder_available`/
/// `whisper_available`/`capture_transcript` are untouched. Lower priority
/// than the other ports here — `voice` has zero call sites in either GUI
/// consumer today, it's a CLI-only feature (`shiki capture --voice`).
pub trait VoiceCapture: Send + Sync {
    fn recorder_available(&self) -> bool;
    fn whisper_available(&self, cache_dir: &Path) -> bool;
    fn capture_transcript(&self, cache_dir: &Path, seconds: u32, model: &str) -> Result<String>;
}

pub struct NativeVoice;

impl VoiceCapture for NativeVoice {
    fn recorder_available(&self) -> bool {
        recorder_available()
    }

    fn whisper_available(&self, cache_dir: &Path) -> bool {
        whisper_available(cache_dir)
    }

    fn capture_transcript(&self, cache_dir: &Path, seconds: u32, model: &str) -> Result<String> {
        capture_transcript(cache_dir, seconds, model)
    }
}

pub fn capture_transcript(cache_dir: &Path, seconds: u32, model: &str) -> Result<String> {
    let tmp = tempfile::tempdir()?;
    let wav = tmp.path().join("capture.wav");
    record_to_wav(&wav, seconds)?;
    let bin = ensure_whisper(cache_dir)?;
    let model = ensure_model(&cache_dir.join("models"), model)?;
    transcribe(&bin, &model, &wav)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_is_a_ggml_bin() {
        assert!(DEFAULT_MODEL.starts_with("ggml-"));
        assert!(DEFAULT_MODEL.ends_with(".bin"));
    }

    #[test]
    fn validate_model_name_accepts_the_real_models() {
        assert!(validate_model_name(DEFAULT_MODEL).is_ok());
        assert!(validate_model_name("ggml-large-v3-turbo.bin").is_ok());
    }

    #[test]
    fn validate_model_name_rejects_traversal_and_non_models() {
        for bad in [
            "../ggml-base.en.bin",
            "../evil.bin",
            "ggml-../evil.bin",
            "ggml-base/other.bin",
            "ggml-base\\other.bin",
            "evil.bin",
            "ggml-base.en",
            "",
            "ggml-base.en.bin/../../x",
        ] {
            assert!(
                validate_model_name(bad).is_err(),
                "expected '{bad}' to be rejected"
            );
        }
    }

    #[test]
    fn parse_lfs_pointer_reads_oid_and_size() {
        let pointer = "version https://git-lfs.github.com/spec/v1\n\
oid sha256:a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002\n\
size 147964211\n";
        assert_eq!(
            parse_lfs_pointer(pointer),
            Some((
                "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002".to_string(),
                147_964_211
            ))
        );
    }

    #[test]
    fn parse_lfs_pointer_normalizes_uppercase_hex() {
        let pointer =
            "oid sha256:A03779C86DF3323075F5E796CB2CE5029F00EC8869EEE3FDFB897AFE36C6D002\nsize 3\n";
        let (hash, size) = parse_lfs_pointer(pointer).unwrap();
        assert_eq!(
            hash,
            "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002"
        );
        assert_eq!(size, 3);
    }

    #[test]
    fn parse_lfs_pointer_rejects_non_pointers() {
        assert_eq!(parse_lfs_pointer(""), None);
        assert_eq!(parse_lfs_pointer("<!doctype html><html>…"), None);
        // Truncated oid and missing size are both malformed.
        assert_eq!(parse_lfs_pointer("oid sha256:abc\nsize 10\n"), None);
        assert_eq!(
            parse_lfs_pointer(
                "oid sha256:a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002\n"
            ),
            None
        );
    }

    #[test]
    fn sha256_file_matches_a_known_vector() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("abc.bin");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}

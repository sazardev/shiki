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

use crate::{process::on_path, Error, Result};

/// The default whisper.cpp model — a ~140 MB English-only `base` model,
/// the usual trade-off between transcription quality and CPU speed for a
/// quick voice capture.
pub const DEFAULT_MODEL: &str = "ggml-base.en.bin";

const WHISPER_OWNER: &str = "ggml-org";
const WHISPER_REPO: &str = "whisper.cpp";
/// whisper.cpp's converted models live in a Hugging Face repo (its own
/// `models/download-ggml-model.sh` uses the same `resolve/main` URL).
const MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

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

/// Ensures `model` (e.g. `ggml-base.en.bin`) is present in `model_dir`,
/// downloading it from Hugging Face once via `curl`/`wget` if missing.
/// Returns the path to the model file.
pub fn ensure_model(model_dir: &Path, model: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(model_dir)?;
    let path = model_dir.join(model);
    if path.is_file() {
        return Ok(path);
    }

    let url = format!("{MODEL_BASE_URL}/{model}");
    let tmp = model_dir.join(format!("{model}.part"));
    if curl_download(&url, &tmp).is_err() && wget_download(&url, &tmp).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::Voice(format!(
            "could not download whisper model '{model}' from {url} \u{2014} need curl or wget on $PATH"
        )));
    }
    std::fs::rename(&tmp, &path)?;
    Ok(path)
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
}

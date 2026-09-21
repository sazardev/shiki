//! Opens a URL in the user's default browser — used by the footer's Buy Me
//! a Coffee link. Each OS has its own "hand this URL to whatever's
//! registered as the default browser" command; there's no cross-platform
//! standard binary for it the way `xdg-mime`/`open -W -t` cover editors.

/// Spawns the OS's default-browser opener for `url`. Fire-and-forget: the
/// caller doesn't wait on it, matching how external-editor spawns elsewhere
/// in this codebase are best-effort (`let _ = ...`).
pub fn open_url(url: &str) -> std::io::Result<std::process::Child> {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()
    }
    #[cfg(target_os = "windows")]
    {
        // `start`'s first argument after the shell built-in is treated as a
        // window title if quoted, so an empty title arg is required —
        // without it, a URL containing `&` gets misparsed as the title.
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    }
    // Anything that isn't linux/macos/windows (a WASM target, most notably —
    // confirmed live: without this arm, the function body was empty there,
    // failing to compile at all against its declared `Result` return type)
    // has no process to spawn in the first place, so this is a real error,
    // not a silent no-op.
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = url;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "no browser opener is implemented for this platform",
        ))
    }
}

/// A pluggable "open this URL" capability — `NativeBrowser` just spawns the
/// OS opener via `open_url` above; a future non-native consumer (e.g. one
/// that asks a host process to open the URL instead) can implement this
/// instead. Purely additive: `open_url` itself is untouched, and returns
/// the real `Child` handle callers of the free function already rely on —
/// the trait returns `Result<()>` instead, since a non-native implementation
/// has no real child process to hand back.
pub trait BrowserOpener: Send + Sync {
    fn open(&self, url: &str) -> std::io::Result<()>;
}

pub struct NativeBrowser;

impl BrowserOpener for NativeBrowser {
    fn open(&self, url: &str) -> std::io::Result<()> {
        open_url(url).map(|_child| ())
    }
}

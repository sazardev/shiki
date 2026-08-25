//! Terminal clipboard via OSC 52, not a native clipboard crate.
//!
//! ratatui owns the alternate screen, but writing an OSC 52 escape sequence
//! straight to stdout still reaches the real terminal underneath it. Modern
//! terminals (kitty, iTerm2, Alacritty, WezTerm, Windows Terminal, and
//! tmux/screen with clipboard passthrough enabled) intercept this and set
//! the system clipboard — the same mechanism Yazi and Helix use, so it works
//! the same whether shiki is running locally or over SSH.

use base64::Engine;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

pub fn copy(text: &str) {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let mut stdout = std::io::stdout();
    let _ = write!(stdout, "\x1b]52;c;{encoded}\x07");
    let _ = stdout.flush();
}

/// A single, process-lifetime `arboard::Clipboard`, not a fresh one per
/// call — verified live (via `wl-copy`/`wl-paste` against the real Wayland
/// clipboard) that constructing-then-immediately-dropping one, as every
/// earlier version of `copy_os`/`paste_os` did, hits arboard's own X11
/// backend's "dropped very quickly after writing" guard: on Linux it
/// checks `stderr().is_terminal()` and, if true, `eprintln!`s a multi-line
/// warning straight to the real terminal underneath — bypassing ratatui
/// entirely and visibly corrupting the alternate-screen TUI on *every*
/// Ctrl+C/Ctrl+V. Keeping one instance alive for the app's whole lifetime
/// (arboard's own documented recommendation for exactly this situation)
/// means it's never dropped until the process exits, long past the ~100ms
/// window that guard checks for.
fn os_clipboard() -> Option<&'static Mutex<arboard::Clipboard>> {
    static CELL: OnceLock<Option<Mutex<arboard::Clipboard>>> = OnceLock::new();
    CELL.get_or_init(|| arboard::Clipboard::new().ok().map(Mutex::new))
        .as_ref()
}

/// Writes `text` to the real OS clipboard (`config.editor.os_clipboard`).
/// Returns `false` on any failure — most commonly there being no display
/// server to reach at all (a headless SSH session with no `$DISPLAY`/
/// `$WAYLAND_DISPLAY`) — so the caller can fall back to `copy` (OSC 52,
/// which works fine over SSH) instead.
pub fn copy_os(text: &str) -> bool {
    let Some(cb) = os_clipboard() else {
        return false;
    };
    cb.lock().is_ok_and(|mut cb| cb.set_text(text).is_ok())
}

/// Reads the real OS clipboard, or `None` if it can't be reached (no
/// display server) or holds something other than plain text.
pub fn paste_os() -> Option<String> {
    os_clipboard()?.lock().ok()?.get_text().ok()
}

/// Reads an *image* from the real OS clipboard, if one is there — the
/// entry point for pasting screenshots straight into a note. `None` means
/// no display server, or the clipboard holds text rather than an image
/// (callers try `paste_os` first, so text keeps winning).
pub fn paste_image() -> Option<arboard::ImageData<'static>> {
    let cb = os_clipboard()?;
    let mut cb = cb.lock().ok()?;
    let img = cb.get_image().ok()?;
    // arboard's own buffer is borrowed-lifetime; detach it so the image
    // outlives the clipboard lock.
    Some(arboard::ImageData {
        width: img.width,
        height: img.height,
        bytes: std::borrow::Cow::Owned(img.bytes.into_owned()),
    })
}

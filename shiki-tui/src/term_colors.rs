//! Queries the terminal's own foreground/background colors (OSC 10/11) once
//! at TUI startup.
//!
//! SGR colors have no alpha channel, so the `default` (terminal-inherit)
//! theme's `selection = "auto"` can't literally ask the terminal for "the
//! text color at 20% opacity": it has to read the terminal's actual fg/bg
//! RGB values and precompute `fg` over `bg` at 20% itself (see
//! `render.rs::selection_bg`). OSC 10/11 is the standard, widely-supported
//! way to ask (Ghostty, Kitty, Alacritty, foot, Windows Terminal, …); when
//! the terminal doesn't answer (tmux without `allow-passthrough`, a serial
//! console, a non-VT Windows console, a test harness) the caller falls back
//! to the fixed ANSI `darkgray` band this replaced.
//!
//! The query reads `/dev/tty` directly rather than stdin: crossterm's event
//! source owns stdin, and an OSC response that lost the race with the timeout
//! would otherwise be parsed as `Alt+]` plus stray characters and typed into
//! the app.

use ratatui::style::Color;

#[cfg(unix)]
use std::time::{Duration, Instant};

/// How long to wait for the terminal's reply before giving up — one 120 ms
/// budget for both responses, paid only once per process at startup.
#[cfg(unix)]
const QUERY_TIMEOUT: Duration = Duration::from_millis(120);

/// The alpha the `"auto"` selection slot blends the terminal's foreground
/// over its background with, in percent.
pub const SELECTION_ALPHA_PERCENT: u8 = 20;

/// Asks the terminal for its default foreground (OSC 10) and background
/// (OSC 11) colors, in that order. `None` when there's no controlling
/// terminal, the terminal doesn't answer within `QUERY_TIMEOUT`, or it
/// answers in a format this doesn't recognize.
#[cfg(unix)]
pub fn query_fg_bg() -> Option<(Color, Color)> {
    use std::io::{Read, Write};
    use std::os::unix::io::AsRawFd;

    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    tty.write_all(b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\").ok()?;
    tty.flush().ok()?;

    let fd = tty.as_raw_fd();
    let deadline = Instant::now() + QUERY_TIMEOUT;
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 128];
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut pfd, 1, remaining.as_millis() as i32) };
        if ready <= 0 {
            break;
        }
        match tty.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if parse_fg_bg(&buf).is_some() {
                    break;
                }
            }
        }
    }

    // A response that arrived right at the deadline (or after both were
    // already parsed) must not be left in the tty queue for crossterm to
    // misparse as keystrokes — drain whatever is immediately available.
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    while unsafe { libc::poll(&mut pfd, 1, 0) } > 0 {
        match tty.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }

    parse_fg_bg(&buf)
}

/// Non-Unix builds have no `/dev/tty`/OSC path wired up — the `"auto"`
/// selection falls back to `darkgray`, same as an unresponsive terminal.
#[cfg(not(unix))]
pub fn query_fg_bg() -> Option<(Color, Color)> {
    None
}

/// Extracts both OSC 10 and OSC 11 replies from the raw bytes read off the
/// tty. Either reply missing (or unparseable) means the whole query is
/// treated as failed — a half-known pair would blend against the wrong
/// background.
fn parse_fg_bg(bytes: &[u8]) -> Option<(Color, Color)> {
    let fg = parse_response(bytes, 10)?;
    let bg = parse_response(bytes, 11)?;
    Some((fg, bg))
}

/// Finds `ESC ] <code> ;` in `bytes` and parses the payload up to the first
/// `BEL` or `ESC \` (ST) terminator.
fn parse_response(bytes: &[u8], code: u8) -> Option<Color> {
    let prefix = format!("\x1b]{code};");
    let start = bytes
        .windows(prefix.len())
        .position(|window| window == prefix.as_bytes())?
        + prefix.len();
    let rest = &bytes[start..];
    let bel = rest.iter().position(|&b| b == 0x07);
    let st = rest.windows(2).position(|window| window == b"\x1b\\");
    let end = match (bel, st) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => return None,
    };
    let spec = std::str::from_utf8(&rest[..end]).ok()?;
    let (r, g, b) = parse_color_spec(spec)?;
    Some(Color::Rgb(r, g, b))
}

/// The two color specs terminals actually reply with: xterm-style
/// `rgb:RRRR/GGGG/BBBB` (each component 1-4 hex digits, scaled to 8 bits)
/// and the CSS-style `#RRGGBB` some terminals use.
fn parse_color_spec(spec: &str) -> Option<(u8, u8, u8)> {
    if let Some(rest) = spec.strip_prefix("rgb:") {
        let mut parts = rest.split('/');
        let r = scale_hex(parts.next()?)?;
        let g = scale_hex(parts.next()?)?;
        let b = scale_hex(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        return Some((r, g, b));
    }
    let hex = spec.strip_prefix('#')?;
    match hex.len() {
        3 => {
            let expand = |c: char| -> Option<u8> {
                let digit = c.to_digit(16)? as u8;
                Some(digit * 16 + digit)
            };
            let mut chars = hex.chars();
            let r = expand(chars.next()?)?;
            let g = expand(chars.next()?)?;
            let b = expand(chars.next()?)?;
            Some((r, g, b))
        }
        6 => {
            let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
            Some((pair(0)?, pair(2)?, pair(4)?))
        }
        _ => None,
    }
}

/// `"18"` -> `0x18`, `"1818"` -> `0x18`, `"f"` -> `0xff` — OSC components
/// carry their own bit depth, so scale each to the full 0-255 range.
fn scale_hex(component: &str) -> Option<u8> {
    if component.is_empty() || component.len() > 4 {
        return None;
    }
    let value = u32::from_str_radix(component, 16).ok()?;
    let max = (1u32 << (4 * component.len() as u32)) - 1;
    Some(((value * 255 + max / 2) / max) as u8)
}

/// Blends `fg` over `bg` at `percent` alpha — the precomputed stand-in for
/// "the text color at 20% opacity" (SGR has no alpha). `percent` is clamped
/// to 0-100.
pub fn blend_fg_over_bg(fg: (u8, u8, u8), bg: (u8, u8, u8), percent: u8) -> (u8, u8, u8) {
    let p = percent.min(100) as u32;
    let mix = |f: u8, b: u8| ((b as u32 * (100 - p) + f as u32 * p + 50) / 100) as u8;
    (mix(fg.0, bg.0), mix(fg.1, bg.1), mix(fg.2, bg.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_ghostty_style_reply() {
        let raw = b"\x1b]10;rgb:b9b9/bebe/c6c6\x1b\\\x1b]11;rgb:1818/1a1a/1f1f\x1b\\";
        assert_eq!(
            parse_fg_bg(raw),
            Some((Color::Rgb(0xb9, 0xbe, 0xc6), Color::Rgb(0x18, 0x1a, 0x1f)))
        );
    }

    #[test]
    fn parses_bel_terminated_and_hash_replies() {
        let raw = b"\x1b]10;#b9bec6\x07\x1b]11;#181a1f\x07";
        assert_eq!(
            parse_fg_bg(raw),
            Some((Color::Rgb(0xb9, 0xbe, 0xc6), Color::Rgb(0x18, 0x1a, 0x1f)))
        );
    }

    #[test]
    fn parses_short_and_three_digit_components() {
        // Kitty/Alacritty 4-digit form and CSS #rgb both collapse correctly.
        assert_eq!(
            parse_fg_bg(b"\x1b]10;rgb:ffff/0/8080\x1b\\\x1b]11;#000\x1b\\"),
            Some((Color::Rgb(0xff, 0x00, 0x80), Color::Rgb(0, 0, 0)))
        );
    }

    #[test]
    fn ignores_surrounding_keystrokes() {
        // Anything typed before the replies arrived rides along in the same
        // buffer; the parser only looks at the OSC payloads.
        let raw = b"hello\x1b]10;rgb:1111/2222/3333\x1b\\world\x1b]11;rgb:4444/5555/6666\x1b\\";
        assert_eq!(
            parse_fg_bg(raw),
            Some((Color::Rgb(0x11, 0x22, 0x33), Color::Rgb(0x44, 0x55, 0x66)))
        );
    }

    #[test]
    fn a_missing_reply_fails_the_whole_query() {
        assert_eq!(parse_fg_bg(b"\x1b]10;rgb:1111/2222/3333\x1b\\"), None);
        assert_eq!(parse_fg_bg(b""), None);
        assert_eq!(
            parse_fg_bg(b"\x1b]10;not-a-color\x1b\\\x1b]11;#000000\x07"),
            None
        );
    }

    #[test]
    fn blend_matches_the_hand_computed_ghostty_aether_band() {
        // #b9bec6 over #181a1f at 20% = #383b40 — the value the plan and the
        // live TUI test assert on.
        assert_eq!(
            blend_fg_over_bg((0xb9, 0xbe, 0xc6), (0x18, 0x1a, 0x1f), 20),
            (0x38, 0x3b, 0x40)
        );
    }

    #[test]
    fn blend_endpoints_are_exact() {
        let fg = (10, 20, 30);
        let bg = (200, 210, 220);
        assert_eq!(blend_fg_over_bg(fg, bg, 0), bg);
        assert_eq!(blend_fg_over_bg(fg, bg, 100), fg);
        // Clamped rather than wrapping.
        assert_eq!(blend_fg_over_bg(fg, bg, 255), fg);
    }
}

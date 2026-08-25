//! Renders block-level `![alt](path)` images in PREVIEW as terminal art by
//! shelling out to `chafa` (a terminal image renderer) — the same
//! external-binary pattern as `shiki_core::publish`'s `pretty-pdf`. chafa
//! converts an image to ANSI-colored half-block art and prints it as text
//! lines, which drop straight into the preview's existing `Vec<Line>` model
//! and scroll like any other row — unlike a real terminal-graphics protocol
//! (kitty/sixel), which anchors images to screen coordinates and breaks
//! scrolling. When `chafa` isn't available, or the file can't be decoded,
//! the caller falls back to the inline icon+alt representation.
//!
//! Deliberately no new dependencies: chafa is invoked as an external
//! process, and its 24-bit SGR output is parsed by the small hand-rolled
//! `ansi_to_line` below rather than pulling in an ANSI-parsing crate.

use std::path::{Path, PathBuf};
use std::process::Command;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Everything the markdown renderer needs to know about image rendering,
/// bundled so `markdown_to_lines_indexed` stays a plain function taking
/// plain data (it's called from `app.rs`'s preview cache and from tests).
pub struct ImageCtx {
    /// Master switch (`[general] preview_images`).
    pub enabled: bool,
    /// Resolved `chafa` binary; `None` means "don't even try". The caller
    /// resolves this once per refresh from `chafa_path`/`$PATH` via
    /// `chafa_binary`.
    pub chafa: Option<PathBuf>,
    /// The art's target width in columns, precomputed by the caller as
    /// `preview_image_scale × preview panel width` (clamped to a sane
    /// range). Stored precomputed so the pure markdown renderer never has
    /// to know the panel layout.
    pub cols: usize,
    /// Directories to resolve a relative image path against, tried in order:
    /// the note's own folder, the notebook root, then `data_dir`.
    pub base_dirs: Vec<PathBuf>,
}

impl ImageCtx {
    /// The `chafa` binary to shell out to: `chafa_path` when set, otherwise
    /// whatever's found on `$PATH` (the same split-paths scan
    /// `shiki_core::process::on_path` does, but returning the actual path
    /// rather than a bool).
    pub fn chafa_binary(chafa_path: &str) -> Option<PathBuf> {
        let explicit = if chafa_path.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(chafa_path.trim()))
        };
        explicit.filter(|p| p.is_file()).or_else(chafa_on_path)
    }
}

/// First existing `chafa` binary on `$PATH`, if any.
fn chafa_on_path() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("chafa"))
            .find(|candidate| candidate.is_file())
    })
}

/// Parses `![alt](path)` — returning the `path` only when the image is the
/// *whole* line (modulo surrounding whitespace). An image embedded mid-line
/// keeps the single-span icon+alt rendering; only a line wholly given over
/// to the image gets multi-row art. Returns `None` for `http(s)://` URLs —
/// only local files are rendered, remote fetching is out of scope.
pub fn whole_line_image_path(line: &str) -> Option<String> {
    let t = line.trim();
    if !t.starts_with("![") {
        return None;
    }
    let rest = &t[2..];
    let close_bracket = rest.find(']')?;
    let after = rest[close_bracket + 1..].strip_prefix('(')?;
    let close_paren = after.find(')')?;
    if !after[close_paren + 1..].trim().is_empty() {
        return None;
    }
    let path = &after[..close_paren];
    let path = path.trim();
    if path.is_empty() || path.starts_with("http://") || path.starts_with("https://") {
        None
    } else {
        Some(path.to_string())
    }
}

/// Parses Obsidian's embed syntax `![[file]]` — returning the file
/// reference only when the embed is the *whole* line (modulo surrounding
/// whitespace), like `whole_line_image_path`. Alias (`|text`) and
/// sub-address (`#heading`/`^block`) parts are stripped; empty or remote
/// references yield `None`.
pub fn whole_line_embed_path(line: &str) -> Option<String> {
    let t = line.trim();
    if !(t.starts_with("![[") && t.ends_with("]]")) {
        return None;
    }
    let inner = &t[3..t.len() - 2];
    let inner = inner.split('|').next().unwrap_or("");
    let inner = inner.split(['#', '^']).next().unwrap_or("").trim();
    if inner.is_empty() || inner.starts_with("http://") || inner.starts_with("https://") {
        None
    } else {
        Some(inner.to_string())
    }
}

/// Resolves an embed `spec` against the ctx's base directories — first the
/// ordinary relative-path chain (`resolve_image_path`), then an
/// Obsidian-style name lookup: vaults commonly keep images in some
/// subfolder (`attachments/`, `_files/`, the note's own folder…) while the
/// embed only carries the bare file name, so each base directory's
/// immediate subdirectories are tried too. Depth stays bounded (one level),
/// keeping this cheap enough to run during a preview refresh.
pub fn resolve_embed_path(base_dirs: &[PathBuf], spec: &str) -> Option<PathBuf> {
    if let Some(found) = resolve_image_path(base_dirs, spec) {
        return Some(found);
    }
    for base in base_dirs {
        let Ok(entries) = std::fs::read_dir(base) else {
            continue;
        };
        for entry in entries.flatten() {
            let candidate = entry.path().join(spec);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Resolves an image `spec` against the ctx's base directories, returning
/// the first existing absolute path. An absolute `spec` is checked as-is.
pub fn resolve_image_path(base_dirs: &[PathBuf], spec: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(spec);
    if candidate.is_absolute() {
        return candidate.is_file().then_some(candidate);
    }
    base_dirs
        .iter()
        .map(|base| base.join(&candidate))
        .find(|p| p.is_file())
}

/// Renders `path` to terminal-art rows at `cols` columns wide (chafa picks
/// the height to preserve aspect ratio). Returns `None` when the binary
/// isn't available, chafa exits non-zero, or it produces no output — the
/// caller keeps the icon+alt fallback in every one of those cases.
pub fn render_rows(chafa: &Path, path: &Path, cols: usize) -> Option<Vec<Line<'static>>> {
    let cols = cols.clamp(1, 200);
    let output = Command::new(chafa)
        .args([
            "--format=symbols",
            "--colors=full",
            "--animate=off",
            // Classic half-block glyphs (`▀`/`▄`), which every terminal font
            // supports, rather than chafa's default eighth-block "shades"
            // set (`▂`/`▃`/…) that some fonts render as boxes.
            "--symbols=block",
            &format!("--size={cols}x"),
        ])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let fallback = Style::default().fg(Color::Gray);
    let lines: Vec<Line<'static>> = stdout
        .lines()
        .map(|raw| ansi_to_line(raw, fallback))
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(lines)
}

/// Converts one raw output line (possibly containing ANSI SGR escape
/// sequences) into a `Line` of styled spans. Only SGR (`ESC [ ... m`) is
/// interpreted; any other escape sequence is dropped rather than surfacing
/// as garbage. Background color is applied too (half-block art paints its
/// lower half via the background), except reset-to-default which clears it.
fn ansi_to_line(raw: &str, fallback: Style) -> Line<'static> {
    let bytes = raw.as_bytes();
    let mut style = fallback;
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            flush(&mut text, &mut spans, style);
            if bytes.get(i + 1) == Some(&b'[') {
                // CSI sequence: skip until the final byte in @..~ ; SGR when
                // it's `m`.
                let mut j = i + 2;
                while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'm' {
                    apply_sgr(&bytes[i + 2..j], &mut style);
                }
                i = j + 1;
            } else {
                // A stray ESC (OSC, etc.) — skip this byte; the sequence
                // terminating byte will be consumed as ordinary text, which
                // for chafa's output is acceptable noise.
                i += 1;
            }
        } else {
            let ch_len = utf8_len(bytes[i]);
            text.push_str(&raw[i..i + ch_len]);
            i += ch_len;
        }
    }
    flush(&mut text, &mut spans, style);
    Line::from(spans)
}

fn flush(text: &mut String, spans: &mut Vec<Span<'static>>, style: Style) {
    if !text.is_empty() {
        spans.push(Span::styled(std::mem::take(text), style));
    }
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

const BASIC: [Color; 8] = [
    Color::Black,
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::Gray,
];
const BRIGHT: [Color; 8] = [
    Color::DarkGray,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightCyan,
    Color::White,
];

/// Applies an SGR parameter block (the bytes between `ESC [` and `m`) to
/// `style`, honoring: reset, bold, italic, 30-37/90-97/38;2;r;g;b/38;5;n
/// foregrounds, and 40-47/100-107/48;… backgrounds. Everything else is
/// ignored. The string is split on `;` and each numeric code dispatched
/// through a `u16` match — `&str` ranges aren't pattern-matchable, so the
/// 30-37/40-47 bands are matched numerically instead.
fn apply_sgr(params: &[u8], style: &mut Style) {
    let parts: Vec<&str> = std::str::from_utf8(params)
        .unwrap_or("")
        .split(';')
        .collect();
    let mut i = 0;
    while i < parts.len() {
        let p = parts[i];
        match p {
            "0" => *style = Style::default(),
            "1" => *style = style.add_modifier(Modifier::BOLD),
            "3" => *style = style.add_modifier(Modifier::ITALIC),
            "38" | "48" => {
                let foreground = p == "38";
                match parts.get(i + 1).copied() {
                    Some("2") => {
                        let mut rgb = [0u8; 3];
                        for (k, slot) in rgb.iter_mut().enumerate() {
                            if let Some(num) = parts.get(i + 2 + k).copied() {
                                *slot = num.parse().unwrap_or(0);
                            }
                        }
                        if foreground {
                            *style = style.fg(Color::Rgb(rgb[0], rgb[1], rgb[2]));
                        } else {
                            *style = style.bg(Color::Rgb(rgb[0], rgb[1], rgb[2]));
                        }
                        i += 4;
                    }
                    Some("5") => {
                        let n: Option<u8> = parts.get(i + 2).and_then(|s| s.parse().ok());
                        if let Some(n) = n {
                            if foreground {
                                *style = style.fg(Color::Indexed(n));
                            } else {
                                *style = style.bg(Color::Indexed(n));
                            }
                        }
                        i += 2;
                    }
                    _ => {}
                }
            }
            _ => {
                if let Ok(n) = p.parse::<u16>() {
                    match n {
                        30..=37 => *style = style.fg(BASIC[(n - 30) as usize]),
                        40..=47 => *style = style.bg(BASIC[(n - 40) as usize]),
                        90..=97 => *style = style.fg(BRIGHT[(n - 90) as usize]),
                        100..=107 => *style = style.bg(BRIGHT[(n - 100) as usize]),
                        _ => {}
                    }
                }
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &Line<'static>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn whole_line_image_extracts_path() {
        assert_eq!(
            whole_line_image_path("![a cat](img/cat.png)").as_deref(),
            Some("img/cat.png")
        );
        assert_eq!(
            whole_line_image_path("  ![a cat](img/cat.png)  ").as_deref(),
            Some("img/cat.png")
        );
    }

    #[test]
    fn inline_image_is_not_whole_line() {
        assert_eq!(
            whole_line_image_path("see ![a cat](img/cat.png) here"),
            None
        );
        assert_eq!(whole_line_image_path("![a](b) trailing"), None);
    }

    #[test]
    fn remote_and_empty_images_are_rejected() {
        assert_eq!(whole_line_image_path("![](https://x/y.png)"), None);
        assert_eq!(whole_line_image_path("![](http://x/y.png)"), None);
        assert_eq!(whole_line_image_path("![]()"), None);
    }

    #[test]
    fn non_image_lines_are_rejected() {
        assert_eq!(whole_line_image_path("# heading"), None);
        assert_eq!(whole_line_image_path("[link](url)"), None);
        assert_eq!(whole_line_image_path(""), None);
    }

    #[test]
    fn embed_syntax_extracts_the_file_reference() {
        assert_eq!(
            whole_line_embed_path("![[screenshot.png]]").as_deref(),
            Some("screenshot.png")
        );
        assert_eq!(
            whole_line_embed_path("  ![[attachments/diagram.png]]  ").as_deref(),
            Some("attachments/diagram.png")
        );
        // Alias and sub-address parts don't change which file is meant.
        assert_eq!(
            whole_line_embed_path("![[photo.png|the vault door]]").as_deref(),
            Some("photo.png")
        );
        assert_eq!(
            whole_line_embed_path("![[note#Section]]").as_deref(),
            Some("note")
        );
    }

    #[test]
    fn embed_syntax_rejects_everything_that_is_not_a_local_embed() {
        // Markdown form belongs to the other parser.
        assert_eq!(whole_line_embed_path("![alt](img.png)"), None);
        // Not the whole line / not an embed at all.
        assert_eq!(whole_line_embed_path("text ![[a.png]] more"), None);
        assert_eq!(whole_line_embed_path("[[a.png]]"), None);
        // Empty, remote.
        assert_eq!(whole_line_embed_path("![[ ]]"), None);
        assert_eq!(whole_line_embed_path("![[https://x/y.png]]"), None);
    }

    #[test]
    fn resolve_embed_finds_a_bare_name_inside_subfolders() {
        let dir = std::env::temp_dir().join("shiki-embed-resolve-test");
        let _ = std::fs::create_dir_all(&dir);
        let attachments = dir.join("attachments");
        let _ = std::fs::create_dir_all(&attachments);
        std::fs::write(attachments.join("pic.png"), "fake png").unwrap();
        let bases = vec![dir.clone()];
        let resolved = resolve_embed_path(&bases, "pic.png").unwrap();
        assert_eq!(resolved, attachments.join("pic.png"));
        // And a spec that already carries its folder resolves directly,
        // without the scan.
        let direct = resolve_embed_path(&bases, "attachments/pic.png").unwrap();
        assert_eq!(direct, attachments.join("pic.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_relative_against_first_existing_base() {
        let dir = std::env::temp_dir().join("shiki-term-image-test");
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("pic.png");
        std::fs::write(&target, "fake png").unwrap();
        let bases = vec![dir.clone(), std::env::temp_dir()];
        let resolved = resolve_image_path(&bases, "pic.png").unwrap();
        assert_eq!(resolved, target);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_resolves_to_nothing() {
        let dir = std::env::temp_dir().join("shiki-term-image-test-missing");
        let resolved = resolve_image_path(std::slice::from_ref(&dir), "nope.png");
        assert!(resolved.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sgr_parses_24bit_foreground() {
        let mut style = Style::default();
        apply_sgr(b"38;2;255;0;0", &mut style);
        assert_eq!(style.fg, Some(Color::Rgb(255, 0, 0)));
    }

    #[test]
    fn sgr_parses_indexed_and_background() {
        let mut style = Style::default();
        apply_sgr(b"38;5;196;48;2;0;0;255", &mut style);
        assert_eq!(style.fg, Some(Color::Indexed(196)));
        assert_eq!(style.bg, Some(Color::Rgb(0, 0, 255)));
    }

    #[test]
    fn sgr_reset_clears_style() {
        let mut style = Style::default().fg(Color::Red);
        apply_sgr(b"0", &mut style);
        assert_eq!(style.fg, None);
    }

    #[test]
    fn ansi_line_produces_styled_spans() {
        let line = ansi_to_line("\u{1b}[38;2;10;20;30m▀\u{1b}[0m rest", Style::default());
        assert_eq!(text(&line), "▀ rest");
        assert_eq!(line.spans[0].style.fg, Some(Color::Rgb(10, 20, 30)));
        // After the reset, the second span is unadorned.
        assert_eq!(line.spans[1].style.fg, None);
    }

    #[test]
    fn render_rows_is_none_without_a_binary() {
        // A bogus chafa path must fall through to None, not panic.
        let rows = render_rows(Path::new("/nonexistent/chafa"), Path::new("/tmp/x.png"), 40);
        assert!(rows.is_none());
    }
}

//! Real syntax highlighting for fenced code blocks in the PREVIEW panel,
//! via `syntect`. Before this existed, every code fence rendered as flat
//! dimmed text (see `render.rs`'s `markdown_to_lines_indexed`) — the same
//! visual treatment as a blockquote, with no per-token color at all despite
//! `syntect` already being a workspace dependency.
//!
//! `SyntaxSet`/`ThemeSet` are process-wide, loaded once behind `OnceLock`
//! (both are non-trivial to build — they parse a bundle of `.sublime-syntax`/
//! `.tmTheme` files) and never rebuilt, same "expensive walk once" discipline
//! as `App`'s various `refresh_*_cache` methods.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}

/// Picks a bundled syntect theme by the *app* theme's own background
/// luminance, so a code block reads as dark-on-dark or light-on-light
/// consistent with whichever shiki theme is active (e.g. catppuccin-latte)
/// rather than a single fixed syntect theme clashing with a light palette.
fn syntect_theme_name(dark: bool) -> &'static str {
    if dark {
        "base16-ocean.dark"
    } else {
        "InspiredGitHub"
    }
}

fn find_syntax<'a>(set: &'a SyntaxSet, lang: &str) -> Option<&'a SyntaxReference> {
    set.find_syntax_by_token(lang)
        .or_else(|| set.find_syntax_by_extension(lang))
}

/// Whether `lang` (a fence's info-string language tag, already lowercased)
/// is one `syntect`'s bundled syntax defs actually recognize — checked
/// up front so the fence-language line itself can be styled to signal
/// "this will be highlighted" vs. "this falls back to plain dimmed text".
pub(crate) fn is_known_language(lang: &str) -> bool {
    !lang.is_empty() && find_syntax(syntax_set(), lang).is_some()
}

/// One code fence's worth of incremental highlighter state: created when a
/// ` ```lang ` fence with a recognized language opens, fed one source line
/// at a time in order, dropped the moment the fence closes. `syntect`'s
/// `HighlightLines` keeps internal parse-stack state across lines (needed
/// for correct highlighting of multi-line constructs like block comments),
/// so it can't be recreated per-line — this struct exists specifically to
/// carry that state across `markdown_to_lines_indexed`'s line-by-line loop.
pub(crate) struct CodeHighlighter {
    highlighter: HighlightLines<'static>,
}

impl CodeHighlighter {
    /// `None` if `lang` isn't recognized — the caller falls back to the
    /// plain dimmed-text style code fences always used before this existed.
    pub(crate) fn new(lang: &str, dark: bool) -> Option<Self> {
        let set = syntax_set();
        let syntax = find_syntax(set, lang)?;
        let theme = theme_set().themes.get(syntect_theme_name(dark))?;
        Some(Self {
            highlighter: HighlightLines::new(syntax, theme),
        })
    }

    /// Tokenizes one source line (no trailing newline) into styled spans.
    /// `syntect` expects a trailing `\n` for some syntaxes to parse
    /// correctly (multi-line comments, doc strings), so one is appended
    /// before highlighting and stripped back off the last span afterward.
    pub(crate) fn highlight(&mut self, line: &str) -> Vec<Span<'static>> {
        let with_newline = format!("{line}\n");
        let Ok(ranges) = self.highlighter.highlight_line(&with_newline, syntax_set()) else {
            return vec![Span::styled(line.to_string(), Style::default())];
        };
        ranges
            .into_iter()
            .map(|(style, text)| {
                let text = text.strip_suffix('\n').unwrap_or(text);
                let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                let mut modifier = Modifier::empty();
                if style.font_style.contains(FontStyle::BOLD) {
                    modifier |= Modifier::BOLD;
                }
                if style.font_style.contains(FontStyle::ITALIC) {
                    modifier |= Modifier::ITALIC;
                }
                if style.font_style.contains(FontStyle::UNDERLINE) {
                    modifier |= Modifier::UNDERLINED;
                }
                Span::styled(
                    text.to_string(),
                    Style::default().fg(fg).add_modifier(modifier),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_language_is_recognized() {
        assert!(is_known_language("rust"));
        assert!(is_known_language("rs"));
        assert!(is_known_language("python"));
    }

    #[test]
    fn unknown_language_falls_back() {
        assert!(!is_known_language(""));
        assert!(!is_known_language("not-a-real-language"));
    }

    #[test]
    fn highlighter_tokenizes_a_rust_line_into_multiple_styled_spans() {
        let mut hl = CodeHighlighter::new("rust", true).expect("rust is a known language");
        let spans = hl.highlight("fn main() {}");
        let full: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full, "fn main() {}");
        // A keyword and a plain identifier should not share the exact same
        // color — that's the whole point of real syntax highlighting vs.
        // the old flat-dim rendering.
        assert!(spans.len() > 1, "expected more than one styled span");
    }
}

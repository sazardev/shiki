//! Real syntax highlighting for fenced code blocks in the PREVIEW panel,
//! via `syntect`. Before this existed, every code fence rendered as flat
//! dimmed text (see `render.rs`'s `markdown_to_lines_indexed`) — the same
//! visual treatment as a blockquote, with no per-token color at all despite
//! `syntect` already being a workspace dependency.
//!
//! Two things layer on top of that first pass:
//!
//! - **Language coverage** comes from `two_face::syntax::extra_newlines`, not
//!   `syntect::parsing::SyntaxSet::load_defaults_newlines`. Plain syntect's
//!   bundled defaults don't include TypeScript or TSX at all (a long-standing
//!   gap in syntect's own asset bundle) — a ```tsx fence rendered as flat
//!   dimmed text even after highlighting existed. `two-face` bundles ~150
//!   extra syntaxes (from `bat`'s asset collection, TypeScript/TSX included)
//!   on top of syntect's own defaults, so it's a drop-in replacement rather
//!   than something merged in alongside them.
//! - **Colors adapt to whichever shiki theme is active**, rather than a
//!   syntect theme bundled from `syntect::highlighting::ThemeSet` (the
//!   original version picked between exactly two fixed themes,
//!   `base16-ocean.dark`/`InspiredGitHub`, by dark/light only — every one of
//!   shiki's 12+ themes rendered code fences in the same two palettes
//!   regardless of the theme's own accent/hue, e.g. gruvbox's code blocks
//!   never looked gruvbox-yellow). `build_runtime_theme` constructs a
//!   `syntect::highlighting::Theme` in memory from the active `Theme`'s own
//!   color slots (`accent`/`success`/`warning`/`link`/`tag`/`muted`/`fg`) —
//!   there's no dedicated "keyword"/"string"/"function" slot in
//!   `shiki_config::Theme`, so this reuses the closest existing slot for
//!   each (see `build_runtime_theme`'s own comment for the exact mapping).
//! - **ANSI/`Reset` slots stay terminal-native.** The `default` (terminal-
//!   inherit) theme deliberately uses bare ANSI names (`blue`, `green`,
//!   …) and `"reset"` instead of hex colors. syntect's colors are RGBA, so
//!   those slots used to be approximated with fixed RGB values — code
//!   fences never matched the user's actual terminal palette. Non-RGB slots
//!   are now smuggled through syntect as a private sentinel and decoded
//!   back to the original `ratatui::Color` (see `ANSI_MARKER_*`), so code
//!   highlighting in `default` is rendered by the terminal's own color
//!   scheme just like the rest of the UI.
//!
//! `SyntaxSet` is process-wide, loaded once behind `OnceLock` (non-trivial to
//! build — it parses a bundle of `.sublime-syntax` files), same "expensive
//! walk once" discipline as `App`'s various `refresh_*_cache` methods. The
//! runtime `Theme` can't share that treatment: it's derived from the active
//! shiki theme's colors, which change (theme picker live-preview, switching
//! themes) — it's rebuilt per `CodeHighlighter::new` call instead, which is
//! cheap (a dozen or so `ThemeItem`s, no file I/O).

use std::str::FromStr;
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::highlighting::{
    Color as SyntectColor, FontStyle, HighlightIterator, HighlightState, Highlighter,
    ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

fn find_syntax<'a>(set: &'a SyntaxSet, lang: &str) -> Option<&'a SyntaxReference> {
    set.find_syntax_by_token(lang)
        .or_else(|| set.find_syntax_by_extension(lang))
}

/// Whether `lang` (a fence's info-string language tag, already lowercased)
/// is one `syntect`'s (plus `two-face`'s extra bundle's) syntax defs actually
/// recognize — checked up front so the fence-language line itself can be
/// styled to signal "this will be highlighted" vs. "this falls back to plain
/// dimmed text".
pub(crate) fn is_known_language(lang: &str) -> bool {
    !lang.is_empty() && find_syntax(syntax_set(), lang).is_some()
}

/// The color inputs `CodeHighlighter` builds a syntect `Theme` from — every
/// field is a color slot already resolved off the active `shiki_config::Theme`
/// (`render::hex_to_color`). A plain struct instead of seven positional args,
/// since `CodeHighlighter::new` and `build_runtime_theme` both need the full
/// set together. Slots that aren't `Color::Rgb` (the `default` theme's ANSI
/// names/`Reset`, or a user override with `"blue"`) stay ANSI through the
/// highlighter — see the sentinel note below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SyntaxPalette {
    pub(crate) fg: Color,
    pub(crate) accent: Color,
    pub(crate) muted: Color,
    pub(crate) link: Color,
    pub(crate) tag: Color,
    pub(crate) success: Color,
    pub(crate) warning: Color,
}

/// syntect only stores RGBA, but `default`'s slots are ANSI names/`Reset`
/// that only the terminal can resolve. Non-RGB `ratatui::Color`s are encoded
/// as this impossible-as-a-real-theme marker (`r`/`b` fixed, `g` selecting
/// the kind, `a` carrying the variant) and decoded back in `highlight()`.
/// Real theme RGB slots always go through with `a == 0xFF`, so a genuine
/// `#534849` can't be mistaken for a marker.
const ANSI_MARKER_R: u8 = 0x53; // 'S'
const ANSI_MARKER_B: u8 = 0x49; // 'I'
const ANSI_MARKER_G_NAMED: u8 = 0x48; // 'H'
const ANSI_MARKER_G_INDEXED: u8 = 0x4A; // 'J'

/// `Reset`/the 16 ANSI names, in the order used by the marker's alpha byte.
fn named_color(index: u8) -> Option<Color> {
    Some(match index {
        0 => Color::Reset,
        1 => Color::Black,
        2 => Color::Red,
        3 => Color::Green,
        4 => Color::Yellow,
        5 => Color::Blue,
        6 => Color::Magenta,
        7 => Color::Cyan,
        8 => Color::Gray,
        9 => Color::DarkGray,
        10 => Color::LightRed,
        11 => Color::LightGreen,
        12 => Color::LightYellow,
        13 => Color::LightBlue,
        14 => Color::LightMagenta,
        15 => Color::LightCyan,
        16 => Color::White,
        _ => return None,
    })
}

fn named_index(color: Color) -> Option<u8> {
    Some(match color {
        Color::Reset => 0,
        Color::Black => 1,
        Color::Red => 2,
        Color::Green => 3,
        Color::Yellow => 4,
        Color::Blue => 5,
        Color::Magenta => 6,
        Color::Cyan => 7,
        Color::Gray => 8,
        Color::DarkGray => 9,
        Color::LightRed => 10,
        Color::LightGreen => 11,
        Color::LightYellow => 12,
        Color::LightBlue => 13,
        Color::LightMagenta => 14,
        Color::LightCyan => 15,
        Color::White => 16,
        Color::Rgb(..) | Color::Indexed(_) => return None,
    })
}

fn ansi_sentinel(color: Color) -> Option<SyntectColor> {
    if let Some(index) = named_index(color) {
        return Some(SyntectColor {
            r: ANSI_MARKER_R,
            g: ANSI_MARKER_G_NAMED,
            b: ANSI_MARKER_B,
            a: index,
        });
    }
    match color {
        Color::Indexed(index) => Some(SyntectColor {
            r: ANSI_MARKER_R,
            g: ANSI_MARKER_G_INDEXED,
            b: ANSI_MARKER_B,
            a: index,
        }),
        _ => None,
    }
}

/// The inverse of `ansi_sentinel`: a real RGB color passes through untouched,
/// an ANSI/`Reset` marker becomes the exact `ratatui::Color` the terminal
/// should resolve itself.
fn ratatui_color(color: SyntectColor) -> Color {
    if color.r == ANSI_MARKER_R && color.b == ANSI_MARKER_B {
        match color.g {
            // Named/Reset markers always carry an alpha in 0..=16, so they
            // can't collide with a real RGB slot (always alpha 0xFF).
            ANSI_MARKER_G_NAMED if color.a != 0xFF => {
                return named_color(color.a).unwrap_or(Color::Reset);
            }
            // Indexed(255) legitimately uses alpha 0xFF, so the `g` marker
            // (not the alpha) is what identifies this branch.
            ANSI_MARKER_G_INDEXED => return Color::Indexed(color.a),
            _ => {}
        }
    }
    Color::Rgb(color.r, color.g, color.b)
}

/// Approximates an RGB `ratatui::Color` as a concrete RGB `syntect` can use;
/// ANSI names/`Reset`/`Indexed` are encoded as sentinels so they survive the
/// trip through syntect and reach the terminal as native colors. This is what
/// makes the `default` theme's code fences follow the terminal's own palette
/// instead of a fixed guess at what "blue" usually looks like.
fn to_syntect_color(color: Color) -> SyntectColor {
    if let Some(sentinel) = ansi_sentinel(color) {
        return sentinel;
    }
    match color {
        Color::Rgb(r, g, b) => SyntectColor { r, g, b, a: 0xFF },
        // Every non-RGB variant is handled by `ansi_sentinel`; this arm is
        // only reachable if ratatui grows a new `Color` variant.
        _ => SyntectColor {
            r: 0xd0,
            g: 0xd0,
            b: 0xd0,
            a: 0xFF,
        },
    }
}

/// One scope-selector rule, e.g. `("keyword", accent, None)` — builds a
/// `ThemeItem` matching any scope beginning with `selector` (TextMate scope
/// matching is prefix-based per path component, so `"keyword"` also matches
/// `keyword.control.ts`, `keyword.operator.js`, etc.).
fn scope_rule(
    selector: &str,
    color: SyntectColor,
    font_style: Option<FontStyle>,
) -> Option<ThemeItem> {
    ScopeSelectors::from_str(selector)
        .ok()
        .map(|scope| ThemeItem {
            scope,
            style: StyleModifier {
                foreground: Some(color),
                background: None,
                font_style,
            },
        })
}

/// Builds a `syntect::highlighting::Theme` from `palette` in memory, rather
/// than loading a bundled `.tmTheme` — there's no per-token-role color slot
/// in `shiki_config::Theme` (it's `bg`/`fg`/`accent`/`selection`/... , not
/// `keyword`/`string`/`function`), so each TextMate scope category below is
/// mapped onto whichever existing slot reads closest to how GitHub/most
/// editor themes color that category:
/// - `comment` → `muted`, italic (already the "de-emphasized text" slot)
/// - `string`/`constant.character` → `success` (strings read as "data", the
///   same role `success` plays elsewhere — green in most themes)
/// - `constant.numeric`/`constant.language` → `warning` (numbers/booleans;
///   distinct from strings without a dedicated color slot)
/// - `keyword`/`storage` → `accent` — covers control-flow keywords
///   (`if`/`return`) *and* the declaration keywords (`const`/`let`/
///   `function`/`class`), which TextMate grammars scope as `storage.*`, not
///   `keyword.*`; without `storage` here `function`/`const` in a ```tsx
///   fence stayed plain-colored, which is the specific gap that prompted
///   this file
/// - `entity.name.function`/`support.function`/`entity.name.tag` → `link`
///   (function/method/JSX-tag names — GitHub colors these distinctly from
///   both keywords and plain identifiers)
/// - `entity.name.type`/`entity.name.class`/`support.type`/`support.class`
///   → `tag` (type/class/interface names)
///
/// Anything not listed (plain identifiers, punctuation, operators) falls
/// through to `settings.foreground` (`fg`) — the same "everything not
/// explicitly styled reads as normal text" default a real `.tmTheme` has.
fn build_runtime_theme(palette: &SyntaxPalette) -> Theme {
    let fg = to_syntect_color(palette.fg);
    let accent = to_syntect_color(palette.accent);
    let muted = to_syntect_color(palette.muted);
    let link = to_syntect_color(palette.link);
    let tag = to_syntect_color(palette.tag);
    let success = to_syntect_color(palette.success);
    let warning = to_syntect_color(palette.warning);

    let scopes: Vec<ThemeItem> = [
        ("comment", muted, Some(FontStyle::ITALIC)),
        ("string", success, None),
        ("constant.character", success, None),
        ("constant.numeric", warning, None),
        ("constant.language", warning, None),
        ("keyword", accent, None),
        ("storage", accent, None),
        ("entity.name.function", link, None),
        ("support.function", link, None),
        ("entity.name.tag", link, None),
        ("entity.name.type", tag, None),
        ("entity.name.class", tag, None),
        ("support.type", tag, None),
        ("support.class", tag, None),
    ]
    .into_iter()
    .filter_map(|(selector, color, font_style)| scope_rule(selector, color, font_style))
    .collect();

    Theme {
        name: Some("shiki-adaptive".to_string()),
        author: None,
        settings: ThemeSettings {
            foreground: Some(fg),
            ..ThemeSettings::default()
        },
        scopes,
    }
}

/// One code fence's worth of incremental highlighter state: created when a
/// ` ```lang ` fence with a recognized language opens, fed one source line
/// at a time in order, dropped the moment the fence closes. `syntect`'s
/// parse/highlight state keeps internal parse-stack state across lines
/// (needed for correct highlighting of multi-line constructs like block
/// comments), so it can't be recreated per-line — this struct exists
/// specifically to carry that state across `markdown_to_lines_indexed`'s
/// line-by-line loop.
///
/// Owns its `Theme` rather than borrowing one from a `'static` bundled
/// `ThemeSet` (the original version did, since the theme was one of two
/// fixed, process-wide values) — `build_runtime_theme` constructs a fresh
/// one per active shiki theme, so there's nothing `'static` to borrow from
/// any more. `syntect::highlighting::Highlighter` only borrows a `Theme` for
/// as long as a single call needs it (it's a cheap wrapper, not something
/// worth persisting), so it's reconstructed from `self.theme` inside every
/// `highlight()` call instead of stored — that sidesteps the alternative
/// (storing a `Theme` and a `Highlighter<'_>` borrowing it in the same
/// struct, which Rust can't express without unsafe self-reference).
pub(crate) struct CodeHighlighter {
    theme: Theme,
    parse_state: ParseState,
    highlight_state: HighlightState,
}

impl CodeHighlighter {
    /// `None` if `lang` isn't recognized — the caller falls back to the
    /// plain dimmed-text style code fences always used before this existed.
    pub(crate) fn new(lang: &str, palette: &SyntaxPalette) -> Option<Self> {
        let syntax = find_syntax(syntax_set(), lang)?;
        let theme = build_runtime_theme(palette);
        let highlighter = Highlighter::new(&theme);
        let highlight_state = HighlightState::new(&highlighter, ScopeStack::new());
        Some(Self {
            theme,
            parse_state: ParseState::new(syntax),
            highlight_state,
        })
    }

    /// Tokenizes one source line (no trailing newline) into styled spans.
    /// `syntect` expects a trailing `\n` for some syntaxes to parse
    /// correctly (multi-line comments, doc strings), so one is appended
    /// before highlighting and stripped back off the last span afterward.
    pub(crate) fn highlight(&mut self, line: &str) -> Vec<Span<'static>> {
        let with_newline = format!("{line}\n");
        let Ok(ops) = self.parse_state.parse_line(&with_newline, syntax_set()) else {
            return vec![Span::styled(line.to_string(), Style::default())];
        };
        let highlighter = Highlighter::new(&self.theme);
        let ranges: Vec<(syntect::highlighting::Style, &str)> = HighlightIterator::new(
            &mut self.highlight_state,
            &ops[..],
            &with_newline,
            &highlighter,
        )
        .collect();
        ranges
            .into_iter()
            .map(|(style, text)| {
                let text = text.strip_suffix('\n').unwrap_or(text);
                let fg = ratatui_color(style.foreground);
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

    const PALETTE: SyntaxPalette = SyntaxPalette {
        fg: Color::White,
        accent: Color::Blue,
        muted: Color::Gray,
        link: Color::Cyan,
        tag: Color::Magenta,
        success: Color::Green,
        warning: Color::Yellow,
    };

    #[test]
    fn known_language_is_recognized() {
        assert!(is_known_language("rust"));
        assert!(is_known_language("rs"));
        assert!(is_known_language("python"));
    }

    #[test]
    fn typescript_and_tsx_are_recognized() {
        // Plain syntect's bundled defaults don't include TypeScript/TSX at
        // all — this only passes because `syntax_set()` uses `two-face`'s
        // extended bundle instead of `SyntaxSet::load_defaults_newlines`.
        assert!(is_known_language("typescript"));
        assert!(is_known_language("ts"));
        assert!(is_known_language("tsx"));
    }

    #[test]
    fn unknown_language_falls_back() {
        assert!(!is_known_language(""));
        assert!(!is_known_language("not-a-real-language"));
    }

    #[test]
    fn highlighter_tokenizes_a_rust_line_into_multiple_styled_spans() {
        let mut hl = CodeHighlighter::new("rust", &PALETTE).expect("rust is a known language");
        let spans = hl.highlight("fn main() {}");
        let full: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(full, "fn main() {}");
        // A keyword and a plain identifier should not share the exact same
        // color — that's the whole point of real syntax highlighting vs.
        // the old flat-dim rendering.
        assert!(spans.len() > 1, "expected more than one styled span");
    }

    #[test]
    fn tsx_keywords_and_strings_pick_up_the_active_palette_colors() {
        let mut hl = CodeHighlighter::new("tsx", &PALETTE).expect("tsx is a known language");
        let spans = hl.highlight(r#"const greet = (): string => "hi";"#);
        let const_span = spans
            .iter()
            .find(|s| s.content.as_ref() == "const")
            .expect("`const` is tokenized as its own span");
        assert_eq!(
            const_span.style.fg,
            Some(Color::Blue),
            "storage keywords use `accent` (Blue here), natively"
        );
        let string_span = spans
            .iter()
            .find(|s| s.content.as_ref().contains("hi"))
            .expect("the string literal is tokenized as its own span");
        assert_eq!(
            string_span.style.fg,
            Some(Color::Green),
            "string literals use `success` (Green here), natively"
        );
    }

    #[test]
    fn ansi_slots_round_trip_through_syntect_as_native_colors() {
        // The `default` theme's slots are `Reset`/ANSI names, which syntect
        // can't store as such — the sentinel round-trip is what lets the
        // terminal's own palette render code fences in inherited mode.
        for color in [
            Color::Reset,
            Color::Black,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Gray,
            Color::DarkGray,
            Color::LightRed,
            Color::LightGreen,
            Color::LightYellow,
            Color::LightBlue,
            Color::LightMagenta,
            Color::LightCyan,
            Color::White,
            Color::Indexed(42),
            Color::Indexed(255),
        ] {
            assert_eq!(
                ratatui_color(to_syntect_color(color)),
                color,
                "{color:?} must survive the syntect round-trip"
            );
        }

        // A real hex slot is never mistaken for the marker.
        let real = Color::Rgb(ANSI_MARKER_R, ANSI_MARKER_G_NAMED, ANSI_MARKER_B);
        assert_eq!(ratatui_color(to_syntect_color(real)), real);
    }

    #[test]
    fn default_theme_palette_highlights_plain_text_with_terminal_reset() {
        let palette = SyntaxPalette {
            fg: Color::Reset,
            accent: Color::Blue,
            muted: Color::DarkGray,
            link: Color::Cyan,
            tag: Color::Magenta,
            success: Color::Green,
            warning: Color::Yellow,
        };
        let mut hl = CodeHighlighter::new("rust", &palette).unwrap();
        let spans = hl.highlight("let x = 1;");
        let keyword = spans
            .iter()
            .find(|s| s.content.as_ref() == "let")
            .expect("`let` is its own span");
        assert_eq!(keyword.style.fg, Some(Color::Blue));
        let plain = spans
            .iter()
            .find(|s| s.content.as_ref().contains('x'))
            .expect("the identifier is part of a span");
        assert_eq!(
            plain.style.fg,
            Some(Color::Reset),
            "plain code text must defer to the terminal's own foreground"
        );
        let number = spans
            .iter()
            .find(|s| s.content.as_ref() == "1")
            .expect("the number is its own span");
        assert_eq!(number.style.fg, Some(Color::Yellow));
    }

    #[test]
    fn different_shiki_themes_produce_different_code_colors() {
        // The whole point of building the syntect Theme from `SyntaxPalette`
        // at runtime: two different active shiki themes must not render a
        // code fence in the same colors.
        let mut warm = CodeHighlighter::new("rust", &PALETTE).unwrap();
        let mut cool_palette = PALETTE;
        cool_palette.accent = Color::Magenta;
        let mut cool = CodeHighlighter::new("rust", &cool_palette).unwrap();
        let warm_fn = warm
            .highlight("fn main() {}")
            .into_iter()
            .find(|s| s.content.as_ref() == "fn")
            .expect("`fn` is tokenized as its own span");
        let cool_fn = cool
            .highlight("fn main() {}")
            .into_iter()
            .find(|s| s.content.as_ref() == "fn")
            .expect("`fn` is tokenized as its own span");
        assert_ne!(warm_fn.style.fg, cool_fn.style.fg);
    }
}

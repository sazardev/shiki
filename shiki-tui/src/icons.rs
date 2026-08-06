//! Nerd Font glyphs used across the TUI. Requires a terminal font patched by
//! Nerd Fonts (https://www.nerdfonts.com) — these are the original Font
//! Awesome 4 private-use codepoints, which every Nerd Font patch includes.
//!
//! Every glyph below is an [`Icon`], not a bare `char` — `[theme] icons`
//! (Settings, THEME tab) can turn all ~30 of them off at once for a
//! plain-text UI on a terminal without a patched font. This is a *global*
//! flag (`set_enabled`, called once per frame from `draw()`) rather than a
//! `bool` threaded through every call site: every existing
//! `format!("{}", icons::NOTE)`-shaped call across the crate (over 100 of
//! them) keeps working completely unchanged, since `Icon` implements
//! `Display` the same way `char` did — only this module and the one line in
//! `draw()` that calls `set_enabled` needed to change.

use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Sets the global icon toggle for this frame — called once, at the top of
/// `draw()`, from `config.theme.icons`. `Relaxed` ordering is enough: this
/// is a single-threaded render loop, not cross-thread synchronization.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// A glyph that renders as its Nerd Font codepoint when icons are on, or a
/// plain-text `fallback` when they're off — plus, for most icons, its own
/// trailing separator space baked in (`space_after: true`), so a call site
/// never hardcodes a literal space right after an `Icon` in its format
/// string; the icon supplies it, or supplies nothing at all when disabled,
/// so text immediately follows with no leftover gap. `space_after: false`
/// is the deliberate exception for [`UPLOAD`]/[`DOWNLOAD`], which render
/// tight against the ahead/behind count that follows them (`↑1`, `↓1` —
/// see the footer/drawer git-status doc comments) in both states. Decorative
/// glyphs' `fallback` is `""` (icons off should read as "no icon", not "a
/// mystery blank placeholder"); [`ARROW`]'s is `">"`: it's the
/// list-selection marker, not decoration, and a plain-text UI still needs
/// *some* way to show which row is selected (the themed highlight
/// background still applies either way, but a visible marker helps on a
/// monochrome/no-bg theme too).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Icon(char, &'static str, bool);

impl std::fmt::Display for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = if ENABLED.load(Ordering::Relaxed) {
            Some(self.0.to_string())
        } else if self.1.is_empty() {
            None
        } else {
            Some(self.1.to_string())
        };
        match text {
            Some(t) if self.2 => write!(f, "{t} "),
            Some(t) => write!(f, "{t}"),
            None => write!(f, ""),
        }
    }
}

impl Icon {
    /// The bare glyph-or-fallback with no trailing separator space — for
    /// the rare spot where an icon sits tight against surrounding syntax
    /// (e.g. inside `[ ]` brackets) rather than followed by prose, so the
    /// normal auto-appended space would land in the wrong place (before a
    /// closing bracket) instead of disappearing along with the icon.
    pub fn bare(self) -> String {
        if ENABLED.load(Ordering::Relaxed) {
            self.0.to_string()
        } else {
            self.1.to_string()
        }
    }
}

pub const NOTEBOOK: Icon = Icon('\u{f07c}', "", true); // folder-open
pub const NOTE: Icon = Icon('\u{f0f6}', "", true); // file-text-o
pub const TAG: Icon = Icon('\u{f02b}', "", true); // tag
pub const SEARCH: Icon = Icon('\u{f002}', "", true); // search
pub const CALENDAR: Icon = Icon('\u{f133}', "", true); // calendar
pub const GIT: Icon = Icon('\u{f126}', "", true); // code-fork
pub const CHECK: Icon = Icon('\u{f00c}', "", true); // check
pub const WARNING: Icon = Icon('\u{f071}', "", true); // exclamation-triangle
pub const KEYBOARD: Icon = Icon('\u{f11c}', "", true); // keyboard
pub const EYE: Icon = Icon('\u{f06e}', "", true); // eye
pub const ARROW: Icon = Icon('\u{f061}', ">", true); // arrow-right, used as the list selection marker
pub const PENCIL: Icon = Icon('\u{f040}', "", true); // pencil, edit/rename
pub const POWER: Icon = Icon('\u{f011}', "", true); // power-off, quit
pub const COLUMNS: Icon = Icon('\u{f0db}', "", true); // columns, panel focus
pub const LIST: Icon = Icon('\u{f03a}', "", true); // list-ul, logs modal
pub const CLIPBOARD: Icon = Icon('\u{f0ea}', "", true); // clipboard, copy-logs shortcut
pub const TREE: Icon = Icon('\u{f1bb}', "", true); // tree, notebook tree view
pub const DOWNLOAD: Icon = Icon('\u{f019}', "", false); // download, "needs pull" indicator (behind), tight against the count
pub const UPLOAD: Icon = Icon('\u{f093}', "", false); // upload, "needs push" indicator (ahead), tight against the count
pub const HISTORY: Icon = Icon('\u{f1da}', "", true); // history, note version history / date toggle
pub const COFFEE: Icon = Icon('\u{f0f4}', "", true); // coffee, buy-me-a-coffee footer link
pub const LINK: Icon = Icon('\u{f0c1}', "", true); // link (chain), wikilinks/backlinks modal
pub const UNDO: Icon = Icon('\u{f0e2}', "", true); // undo, restore-from-trash
pub const GEAR: Icon = Icon('\u{f013}', "", true); // gear/cog, settings screen
pub const IMAGE: Icon = Icon('\u{f03e}', "", true); // picture-o, markdown image placeholder in the preview
pub const PDF: Icon = Icon('\u{f1c1}', "", true); // file-pdf-o, publish-to-PDF action
pub const EXPAND: Icon = Icon('\u{f065}', "", true); // arrows-alt (expand), zen mode
pub const REPEAT: Icon = Icon('\u{f01e}', "", true); // repeat, recurring task indicator

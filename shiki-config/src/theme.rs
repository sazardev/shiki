use serde::{Deserialize, Serialize};

/// A theme's color palette. Each slot is either a `#rrggbb` hex color, one of
/// the terminal's native ANSI names (`red`, `green`, `blue`, `yellow`,
/// `magenta`, `cyan`, `black`, `gray`/`grey`, `darkgray`/`darkgrey`), or
/// `"reset"` to inherit whatever the terminal's own default color is —
/// see `shiki-tui/src/render.rs::hex_to_color` for how these resolve.
/// Kept agnostic of ratatui/crossterm so shiki-config isn't coupled to the TUI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    /// Which group the palette belongs to — used by the theme picker to
    /// group rows under headers (LoL / Games / Hacker / Classic) and by
    /// the picker's filter, so typing "hack"/"pok"/"lol" narrows by family
    /// as well as by name. `"System"` for the terminal-inherit `default`.
    #[serde(default = "family_default")]
    pub family: &'static str,
    pub bg: String,
    pub fg: String,
    pub accent: String,
    pub selection: String,
    pub border: String,
    pub statusbar: String,
    pub highlight: String,
    pub error: String,
    pub warning: String,
    pub success: String,
    pub inactive: String,
    pub scrollbar: String,
    pub tab_active: String,
    pub tab_inactive: String,
    pub panel_title: String,
    pub cursor: String,
    pub link: String,
    pub tag: String,
    pub muted: String,
}

fn family_default() -> &'static str {
    "Classic"
}

impl Theme {
    /// Fallback used if the configured theme isn't found, and the theme
    /// picker's "default" entry: rather than forcing a fixed palette, this
    /// inherits the terminal's own background/foreground (`"reset"`) and
    /// uses the terminal's native ANSI colors for accents, so it looks right
    /// whatever the user's terminal color scheme is instead of clashing with it.
    pub fn terminal_default() -> Self {
        Self {
            name: "default".into(),
            family: "System",
            bg: "reset".into(),
            fg: "reset".into(),
            accent: "blue".into(),
            selection: "darkgray".into(),
            border: "reset".into(),
            statusbar: "reset".into(),
            highlight: "yellow".into(),
            error: "red".into(),
            warning: "yellow".into(),
            success: "green".into(),
            inactive: "darkgray".into(),
            scrollbar: "darkgray".into(),
            tab_active: "blue".into(),
            tab_inactive: "darkgray".into(),
            panel_title: "magenta".into(),
            cursor: "reset".into(),
            link: "cyan".into(),
            tag: "magenta".into(),
            muted: "darkgray".into(),
        }
    }
}

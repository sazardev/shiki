use serde::{Deserialize, Serialize};

/// A theme's color palette. Each slot is either a `#rrggbb` hex color, one of
/// the terminal's native ANSI names (`red`, `green`, `blue`, `yellow`,
/// `magenta`, `cyan`, `black`, `gray`/`grey`, `darkgray`/`darkgrey`), or
/// `"reset"` to inherit whatever the terminal's own default color is —
/// see `shiki-tui/src/render.rs::hex_to_color` for how these resolve.
/// `selection` additionally accepts `"auto"`: derive the selected-row
/// background from the theme's own `fg`/`bg` (the terminal's real colors,
/// queried over OSC 10/11, when either slot is `"reset"`) as a 20%-alpha
/// blend — see `shiki-tui/src/render.rs::selection_bg`. The `default`
/// (terminal-inherit) theme uses it so its highlight always matches whatever
/// the terminal's color scheme is instead of a fixed ANSI gray that can
/// clash with it.
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

/// Which side of the terminal's color scheme is currently active. Produced by
/// the TUI from the terminal background it reads over OSC 11, and consumed by
/// `Config::theme_for_scheme` to pick the right half of an `"auto"` theme.
/// Kept here (not in shiki-tui) so the config crate can resolve the choice
/// without ever touching a terminal or ratatui.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeScheme {
    Dark,
    Light,
}

impl ThemeScheme {
    /// `"dark"`/`"light"` — used in status messages and tests.
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
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
            // "auto" instead of a fixed `darkgray`: the selected-row band is
            // derived from the terminal's own fg/bg at 20% alpha (see
            // `shiki-tui/src/render.rs::selection_bg`), so it matches the
            // system color scheme by construction — a fixed ANSI gray can be
            // nearly invisible against one palette and drown out `accent` on
            // another.
            selection: "auto".into(),
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

    /// The picker/CLI entry for `theme.name = "auto"`: a marker palette, not
    /// the one that actually renders. Resolution happens in
    /// `Config::theme_for_scheme`, which swaps this for `auto_dark` or
    /// `auto_light` based on the terminal background. Its slots mirror
    /// `terminal_default` so a live preview before the first scheme reply
    /// still looks like a coherent theme rather than a broken one.
    pub fn auto_entry() -> Self {
        Self {
            name: "auto".into(),
            family: "System",
            ..Self::terminal_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_entry_is_a_system_marker_not_a_real_palette() {
        let auto = Theme::auto_entry();
        assert_eq!(auto.name, "auto");
        assert_eq!(auto.family, "System");
        // Same terminal-inherit slots as `default`, so a preview before the
        // first OSC reply reads as a real theme.
        let default = Theme::terminal_default();
        assert_eq!(auto.bg, default.bg);
        assert_eq!(auto.fg, default.fg);
        assert_eq!(auto.selection, "auto");
    }

    #[test]
    fn scheme_labels_are_stable() {
        assert_eq!(ThemeScheme::Dark.label(), "dark");
        assert_eq!(ThemeScheme::Light.label(), "light");
    }
}

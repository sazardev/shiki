use crate::theme::Theme;

/// gruvbox ships three contrast levels per background — `hard`/`medium`/
/// `soft` (morhetz's spec defines them via `bg0_hard`/`bg0`/`bg0_soft`) —
/// so besides the medium `dark`/`light` there's a `dark-hard`, `dark-soft`,
/// `light-hard` and `light-soft`. Only the neutral backgrounds move between
/// levels; every accent (yellow/red/green/aqua/purple) stays identical per
/// spec. The one non-spec hex in the file is the dark-hard `statusbar`:
/// gruvbox's palette bottoms out at `bg0_hard` (`#1d2021`), which is that
/// theme's own `bg`, so its statusbar is a hair darker (`#141617`) to keep
/// the status band visible.
pub fn dark() -> Theme {
    theme! {
        "gruvbox-dark", "Classic",
        bg: "#282828", fg: "#ebdbb2", accent: "#fabd2f",
        selection: "#3c3836", border: "#504945", statusbar: "#1d2021",
        highlight: "#d79921", error: "#cc241d", warning: "#d79921",
        success: "#98971a", inactive: "#928374", scrollbar: "#504945",
        tab_active: "#b16286", tab_inactive: "#928374",
        panel_title: "#d3869b", cursor: "#ebdbb2", link: "#fabd2f",
        tag: "#b16286", muted: "#a89984"
    }
}

pub fn dark_hard() -> Theme {
    theme! {
        "gruvbox-dark-hard", "Classic",
        bg: "#1d2021", fg: "#ebdbb2", accent: "#fabd2f",
        selection: "#3c3836", border: "#504945", statusbar: "#141617",
        highlight: "#d79921", error: "#cc241d", warning: "#d79921",
        success: "#98971a", inactive: "#928374", scrollbar: "#3c3836",
        tab_active: "#b16286", tab_inactive: "#928374",
        panel_title: "#d3869b", cursor: "#ebdbb2", link: "#fabd2f",
        tag: "#b16286", muted: "#a89984"
    }
}

pub fn dark_soft() -> Theme {
    theme! {
        "gruvbox-dark-soft", "Classic",
        bg: "#32302f", fg: "#ebdbb2", accent: "#fabd2f",
        selection: "#3c3836", border: "#504945", statusbar: "#1d2021",
        highlight: "#d79921", error: "#cc241d", warning: "#d79921",
        success: "#98971a", inactive: "#928374", scrollbar: "#504945",
        tab_active: "#b16286", tab_inactive: "#928374",
        panel_title: "#d3869b", cursor: "#ebdbb2", link: "#fabd2f",
        tag: "#b16286", muted: "#a89984"
    }
}

pub fn light() -> Theme {
    theme! {
        "gruvbox-light", "Classic",
        bg: "#fbf1c7", fg: "#3c3836", accent: "#b57614",
        selection: "#ebdbb2", border: "#d5c4a1", statusbar: "#f2e5bc",
        highlight: "#d79921", error: "#cc241d", warning: "#d79921",
        success: "#98971a", inactive: "#928374", scrollbar: "#d5c4a1",
        tab_active: "#b16286", tab_inactive: "#928374",
        panel_title: "#8f3f71", cursor: "#3c3836", link: "#b57614",
        tag: "#b16286", muted: "#7c6f64"
    }
}

pub fn light_hard() -> Theme {
    theme! {
        "gruvbox-light-hard", "Classic",
        bg: "#f9f5d7", fg: "#3c3836", accent: "#b57614",
        selection: "#ebdbb2", border: "#d5c4a1", statusbar: "#f2e5bc",
        highlight: "#d79921", error: "#cc241d", warning: "#d79921",
        success: "#98971a", inactive: "#928374", scrollbar: "#d5c4a1",
        tab_active: "#b16286", tab_inactive: "#928374",
        panel_title: "#8f3f71", cursor: "#3c3836", link: "#b57614",
        tag: "#b16286", muted: "#7c6f64"
    }
}

pub fn light_soft() -> Theme {
    theme! {
        "gruvbox-light-soft", "Classic",
        bg: "#f2e5bc", fg: "#3c3836", accent: "#b57614",
        selection: "#ebdbb2", border: "#d5c4a1", statusbar: "#ebdbb2",
        highlight: "#d79921", error: "#cc241d", warning: "#d79921",
        success: "#98971a", inactive: "#928374", scrollbar: "#d5c4a1",
        tab_active: "#b16286", tab_inactive: "#928374",
        panel_title: "#8f3f71", cursor: "#3c3836", link: "#b57614",
        tag: "#b16286", muted: "#7c6f64"
    }
}

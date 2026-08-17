use crate::theme::Theme;

/// The canonical "Atom One Dark" palette — bg/fg/red/green/yellow/orange/
/// blue/purple/cyan/comment-grey match the original atom-one-dark-syntax
/// values (the same ones every faithful one-dark port, vim or otherwise,
/// still uses unchanged years later); `cursor` uses One Dark's own
/// distinctive caret blue (`#528bff`) rather than reusing `fg`, since it's
/// as recognizable a part of the theme's identity as the accent color.
pub fn one_dark() -> Theme {
    theme! {
        "one-dark", "Classic",
        bg: "#282c34", fg: "#abb2bf", accent: "#61afef",
        selection: "#3e4451", border: "#3e4451", statusbar: "#21252b",
        highlight: "#e5c07b", error: "#e06c75", warning: "#d19a66",
        success: "#98c379", inactive: "#5c6370", scrollbar: "#3e4451",
        tab_active: "#61afef", tab_inactive: "#5c6370",
        panel_title: "#c678dd", cursor: "#528bff", link: "#61afef",
        tag: "#c678dd", muted: "#5c6370"
    }
}

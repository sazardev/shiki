use crate::theme::Theme;

/// Only the flagship `tokyo-night` palette ships now — the `storm`/`moon`
/// variants were dropped from the catalog to keep the theme list lean, but
/// the values here are unchanged from tokyonight.nvim's official spec.
pub fn night() -> Theme {
    theme! {
        "tokyo-night", "Classic",
        bg: "#1a1b26", fg: "#c0caf5", accent: "#7aa2f7",
        selection: "#283457", border: "#292e42", statusbar: "#16161e",
        highlight: "#e0af68", error: "#f7768e", warning: "#e0af68",
        success: "#9ece6a", inactive: "#565f89", scrollbar: "#292e42",
        tab_active: "#bb9af7", tab_inactive: "#565f89",
        panel_title: "#7dcfff", cursor: "#c0caf5", link: "#7aa2f7",
        tag: "#bb9af7", muted: "#565f89"
    }
}

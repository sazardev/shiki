use crate::theme::Theme;

/// Only the flagship `tokyo-night` palette ships now — the `storm`/`moon`
/// variants were dropped from the catalog to keep the theme list lean, but
/// the values here are unchanged from tokyonight.nvim's official spec.
pub fn night() -> Theme {
    Theme {
        name: "tokyo-night".into(),
        bg: "#1a1b26".into(),
        fg: "#c0caf5".into(),
        accent: "#7aa2f7".into(),
        selection: "#283457".into(),
        border: "#292e42".into(),
        statusbar: "#16161e".into(),
        highlight: "#e0af68".into(),
        error: "#f7768e".into(),
        warning: "#e0af68".into(),
        success: "#9ece6a".into(),
        inactive: "#565f89".into(),
        scrollbar: "#292e42".into(),
        tab_active: "#bb9af7".into(),
        tab_inactive: "#565f89".into(),
        panel_title: "#7dcfff".into(),
        cursor: "#c0caf5".into(),
        link: "#7aa2f7".into(),
        tag: "#bb9af7".into(),
        muted: "#565f89".into(),
    }
}

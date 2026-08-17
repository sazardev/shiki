use crate::theme::Theme;

pub fn nord() -> Theme {
    theme! {
        "nord", "Classic",
        bg: "#2e3440", fg: "#d8dee9", accent: "#88c0d0",
        selection: "#434c5e", border: "#3b4252", statusbar: "#242933",
        highlight: "#ebcb8b", error: "#bf616a", warning: "#d08770",
        success: "#a3be8c", inactive: "#4c566a", scrollbar: "#3b4252",
        tab_active: "#b48ead", tab_inactive: "#4c566a",
        panel_title: "#81a1c1", cursor: "#d8dee9", link: "#5e81ac",
        tag: "#b48ead", muted: "#8fbcbb"
    }
}

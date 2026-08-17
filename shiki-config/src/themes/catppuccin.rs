use crate::theme::Theme;

/// Only the flagship `mocha` palette ships now — the `macchiato`/`frappe`/
/// `latte` variants were dropped from the catalog to keep the theme list
/// lean, but the values here are unchanged from catppuccin's official spec.
pub fn mocha() -> Theme {
    theme! {
        "catppuccin-mocha", "Classic",
        bg: "#1e1e2e", fg: "#cdd6f4", accent: "#89b4fa",
        selection: "#45475a", border: "#313244", statusbar: "#181825",
        highlight: "#f9e2af", error: "#f38ba8", warning: "#fab387",
        success: "#a6e3a1", inactive: "#6c7086", scrollbar: "#45475a",
        tab_active: "#cba6f7", tab_inactive: "#6c7086",
        panel_title: "#f5c2e7", cursor: "#f5e0dc", link: "#89b4fa",
        tag: "#cba6f7", muted: "#a6adc8"
    }
}

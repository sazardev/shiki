use crate::theme::Theme;

/// Only the flagship `mocha` palette ships now — the `macchiato`/`frappe`/
/// `latte` variants were dropped from the catalog to keep the theme list
/// lean, but the values here are unchanged from catppuccin's official spec.
pub fn mocha() -> Theme {
    Theme {
        name: "catppuccin-mocha".into(),
        bg: "#1e1e2e".into(),
        fg: "#cdd6f4".into(),
        accent: "#89b4fa".into(),
        selection: "#45475a".into(),
        border: "#313244".into(),
        statusbar: "#181825".into(),
        highlight: "#f9e2af".into(),
        error: "#f38ba8".into(),
        warning: "#fab387".into(),
        success: "#a6e3a1".into(),
        inactive: "#6c7086".into(),
        scrollbar: "#45475a".into(),
        tab_active: "#cba6f7".into(),
        tab_inactive: "#6c7086".into(),
        panel_title: "#f5c2e7".into(),
        cursor: "#f5e0dc".into(),
        link: "#89b4fa".into(),
        tag: "#cba6f7".into(),
        muted: "#a6adc8".into(),
    }
}

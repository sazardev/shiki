use crate::theme::Theme;

pub fn dark() -> Theme {
    Theme {
        name: "gruvbox-dark".into(),
        bg: "#282828".into(),
        fg: "#ebdbb2".into(),
        accent: "#fabd2f".into(),
        selection: "#3c3836".into(),
        border: "#504945".into(),
        statusbar: "#1d2021".into(),
        highlight: "#d79921".into(),
        error: "#cc241d".into(),
        warning: "#d79921".into(),
        success: "#98971a".into(),
        inactive: "#928374".into(),
        scrollbar: "#504945".into(),
        tab_active: "#b16286".into(),
        tab_inactive: "#928374".into(),
        panel_title: "#d3869b".into(),
        cursor: "#ebdbb2".into(),
        link: "#fabd2f".into(),
        tag: "#b16286".into(),
        muted: "#a89984".into(),
    }
}

pub fn light() -> Theme {
    Theme {
        name: "gruvbox-light".into(),
        bg: "#fbf1c7".into(),
        fg: "#3c3836".into(),
        accent: "#b57614".into(),
        selection: "#ebdbb2".into(),
        border: "#d5c4a1".into(),
        statusbar: "#f2e5bc".into(),
        highlight: "#d79921".into(),
        error: "#cc241d".into(),
        warning: "#d79921".into(),
        success: "#98971a".into(),
        inactive: "#928374".into(),
        scrollbar: "#d5c4a1".into(),
        tab_active: "#b16286".into(),
        tab_inactive: "#928374".into(),
        panel_title: "#8f3f71".into(),
        cursor: "#3c3836".into(),
        link: "#b57614".into(),
        tag: "#b16286".into(),
        muted: "#7c6f64".into(),
    }
}

use crate::theme::Theme;

/// Only the classic `solarized-dark` palette ships now — the light variant
/// was dropped from the catalog to keep the theme list lean, but the values
/// here are unchanged from Ethan Schoonover's official spec.
pub fn dark() -> Theme {
    Theme {
        name: "solarized-dark".into(),
        bg: "#002b36".into(),
        fg: "#839496".into(),
        accent: "#268bd2".into(),
        selection: "#073642".into(),
        border: "#586e75".into(),
        statusbar: "#073642".into(),
        highlight: "#b58900".into(),
        error: "#dc322f".into(),
        warning: "#cb4b16".into(),
        success: "#859900".into(),
        inactive: "#586e75".into(),
        scrollbar: "#073642".into(),
        tab_active: "#6c71c4".into(),
        tab_inactive: "#586e75".into(),
        panel_title: "#d33682".into(),
        cursor: "#93a1a1".into(),
        link: "#268bd2".into(),
        tag: "#6c71c4".into(),
        muted: "#657b83".into(),
    }
}

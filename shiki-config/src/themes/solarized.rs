use crate::theme::Theme;

/// Only the classic `solarized-dark` palette ships now — the light variant
/// was dropped from the catalog to keep the theme list lean, but the values
/// here are unchanged from Ethan Schoonover's official spec.
pub fn dark() -> Theme {
    theme! {
        "solarized-dark", "Classic",
        bg: "#002b36", fg: "#839496", accent: "#268bd2",
        selection: "#073642", border: "#586e75", statusbar: "#073642",
        highlight: "#b58900", error: "#dc322f", warning: "#cb4b16",
        success: "#859900", inactive: "#586e75", scrollbar: "#073642",
        tab_active: "#6c71c4", tab_inactive: "#586e75",
        panel_title: "#d33682", cursor: "#93a1a1", link: "#268bd2",
        tag: "#6c71c4", muted: "#657b83"
    }
}

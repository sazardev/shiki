use crate::theme::Theme;

/// The classic Monokai palette (Wimer Hazenberg's original Sublime
/// Text/TextMate scheme) — bg/fg/comment/pink/orange/yellow/green/cyan/
/// purple match the values that have stayed identical across every
/// faithful port since. Not "Monokai Pro" (a separate, later, paid
/// product with a different, cooler-toned palette) — this is the
/// original, still the far more widely recognized of the two.
pub fn monokai() -> Theme {
    theme! {
        "monokai", "Classic",
        bg: "#272822", fg: "#f8f8f2", accent: "#f92672",
        selection: "#49483e", border: "#3e3d32", statusbar: "#1e1f1c",
        highlight: "#e6db74", error: "#f92672", warning: "#fd971f",
        success: "#a6e22e", inactive: "#75715e", scrollbar: "#49483e",
        tab_active: "#f92672", tab_inactive: "#75715e",
        panel_title: "#66d9ef", cursor: "#f8f8f2", link: "#66d9ef",
        tag: "#ae81ff", muted: "#75715e"
    }
}

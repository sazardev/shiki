use crate::theme::Theme;

/// Official palette from draculatheme.com/contribute — bg/current-line/fg/
/// comment/cyan/green/orange/pink/purple/red/yellow are all taken verbatim
/// from the spec; the remaining slots (statusbar/scrollbar/tab_inactive/
/// panel_title/link/tag) have no equivalent in the spec itself and are
/// chosen the same way every other theme in this file already does —
/// reusing the spec's own colors for a slot that reads naturally that way
/// (e.g. `tag` as pink, `link` as cyan) rather than inventing new hex
/// values.
pub fn dracula() -> Theme {
    theme! {
        "dracula", "Classic",
        bg: "#282a36", fg: "#f8f8f2", accent: "#bd93f9",
        selection: "#44475a", border: "#44475a", statusbar: "#21222c",
        highlight: "#f1fa8c", error: "#ff5555", warning: "#ffb86c",
        success: "#50fa7b", inactive: "#6272a4", scrollbar: "#44475a",
        tab_active: "#bd93f9", tab_inactive: "#6272a4",
        panel_title: "#8be9fd", cursor: "#f8f8f2", link: "#8be9fd",
        tag: "#ff79c6", muted: "#6272a4"
    }
}

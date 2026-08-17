use crate::theme::Theme;

/// League of Legends champion palettes. Each theme keys off the champion's
/// most iconic colors rather than the official leagueoflegends.com brand
/// palette: Jinx's neon pink and electric blue eyes (her heterochromia) are
/// the two co-headliners — the deep blue Zaun night is the background, neon
/// pink drives accent/title/tag while electric blue drives links/tabs/cursor/
/// success — plus her explosive yellow; Teemo's scout-green suit with his
/// yellow scarf and mushroom accents; and Ahri's magenta fox tails with the
/// purple of her essence orb. Every secondary slot (statusbar/scrollbar/
/// panel_title/link/tag) reuses those same champion colors in the way the
/// rest of the themes in this file do — nothing invented.
pub fn jinx() -> Theme {
    theme! {
        "LoL (Jinx)", "LoL",
        bg: "#0e1b2c", fg: "#eaf2fb", accent: "#ff3da5",
        selection: "#1a2c44", border: "#243a56", statusbar: "#091321",
        highlight: "#ffe14d", error: "#ff3860", warning: "#ffa63d",
        success: "#2ee6ff", inactive: "#4c6078", scrollbar: "#1a2c44",
        tab_active: "#2ee6ff", tab_inactive: "#4c6078",
        panel_title: "#ff3da5", cursor: "#2ee6ff", link: "#2ee6ff",
        tag: "#ff3da5", muted: "#7e92a8"
    }
}

pub fn teemo() -> Theme {
    theme! {
        "LoL (Teemo)", "LoL",
        bg: "#16220f", fg: "#e9f2dc", accent: "#8bc34a",
        selection: "#22331a", border: "#2d4423", statusbar: "#101a0b",
        highlight: "#fbc02d", error: "#e5533d", warning: "#f79b2e",
        success: "#9ccc65", inactive: "#5c6f49", scrollbar: "#22331a",
        tab_active: "#fbc02d", tab_inactive: "#5c6f49",
        panel_title: "#fbc02d", cursor: "#dcedc8", link: "#9ccc65",
        tag: "#8bc34a", muted: "#7e9170"
    }
}

pub fn ahri() -> Theme {
    theme! {
        "LoL (Ahri)", "LoL",
        bg: "#241529", fg: "#f7e9f2", accent: "#e8448f",
        selection: "#351d40", border: "#482957", statusbar: "#1b0f1f",
        highlight: "#c9a1ff", error: "#ff3d6e", warning: "#ffa25c",
        success: "#6ee7b7", inactive: "#7d5d93", scrollbar: "#351d40",
        tab_active: "#b388ff", tab_inactive: "#7d5d93",
        panel_title: "#ff7ab8", cursor: "#ffffff", link: "#b388ff",
        tag: "#e8448f", muted: "#9c7fb0"
    }
}

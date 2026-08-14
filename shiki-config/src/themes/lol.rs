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
    Theme {
        name: "LoL (Jinx)".into(),
        bg: "#0e1b2c".into(),
        fg: "#eaf2fb".into(),
        accent: "#ff3da5".into(),
        selection: "#1a2c44".into(),
        border: "#243a56".into(),
        statusbar: "#091321".into(),
        highlight: "#ffe14d".into(),
        error: "#ff3860".into(),
        warning: "#ffa63d".into(),
        success: "#2ee6ff".into(),
        inactive: "#4c6078".into(),
        scrollbar: "#1a2c44".into(),
        tab_active: "#2ee6ff".into(),
        tab_inactive: "#4c6078".into(),
        panel_title: "#ff3da5".into(),
        cursor: "#2ee6ff".into(),
        link: "#2ee6ff".into(),
        tag: "#ff3da5".into(),
        muted: "#7e92a8".into(),
    }
}

pub fn teemo() -> Theme {
    Theme {
        name: "LoL (Teemo)".into(),
        bg: "#16220f".into(),
        fg: "#e9f2dc".into(),
        accent: "#8bc34a".into(),
        selection: "#22331a".into(),
        border: "#2d4423".into(),
        statusbar: "#101a0b".into(),
        highlight: "#fbc02d".into(),
        error: "#e5533d".into(),
        warning: "#f79b2e".into(),
        success: "#9ccc65".into(),
        inactive: "#5c6f49".into(),
        scrollbar: "#22331a".into(),
        tab_active: "#fbc02d".into(),
        tab_inactive: "#5c6f49".into(),
        panel_title: "#fbc02d".into(),
        cursor: "#dcedc8".into(),
        link: "#9ccc65".into(),
        tag: "#8bc34a".into(),
        muted: "#7e9170".into(),
    }
}

pub fn ahri() -> Theme {
    Theme {
        name: "LoL (Ahri)".into(),
        bg: "#241529".into(),
        fg: "#f7e9f2".into(),
        accent: "#e8448f".into(),
        selection: "#351d40".into(),
        border: "#482957".into(),
        statusbar: "#1b0f1f".into(),
        highlight: "#c9a1ff".into(),
        error: "#ff3d6e".into(),
        warning: "#ffa25c".into(),
        success: "#6ee7b7".into(),
        inactive: "#7d5d93".into(),
        scrollbar: "#351d40".into(),
        tab_active: "#b388ff".into(),
        tab_inactive: "#7d5d93".into(),
        panel_title: "#ff7ab8".into(),
        cursor: "#ffffff".into(),
        link: "#b388ff".into(),
        tag: "#e8448f".into(),
        muted: "#9c7fb0".into(),
    }
}

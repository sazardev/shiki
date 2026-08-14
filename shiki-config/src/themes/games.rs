use crate::theme::Theme;

/// Video-game franchise palettes, same one-file-per-franchise idea as `lol.rs`.
/// Each theme keys off the most iconic colors of its game — Pikachu's electric
/// yellow with the red of his cheeks, Charizard's ember orange, Gengar's ghost
/// purple, Zelda's triforce gold on a dark Hyrule forest, Portal's Aperture
/// orange/blue (the one light "laboratory" palette of the bunch), Mario and
/// Luigi's red/blue and green/blue, Overwatch's orange/team-blue, Halo's
/// spartan green and UNSC blue on open space, and Stardew Valley's crop-green
/// with harvest amber. Every secondary slot reuses those same franchise colors
/// the way the rest of the themes in this file do — nothing invented.
pub fn pokemon_pikachu() -> Theme {
    Theme {
        name: "Pokémon (Pikachu)".into(),
        bg: "#1a1c2e".into(),
        fg: "#fff5d6".into(),
        accent: "#f6c344".into(),
        selection: "#2a2d45".into(),
        border: "#383c5e".into(),
        statusbar: "#13141f".into(),
        highlight: "#ffe066".into(),
        error: "#ff5252".into(),
        warning: "#ff9f43".into(),
        success: "#7bed9f".into(),
        inactive: "#55597a".into(),
        scrollbar: "#2a2d45".into(),
        tab_active: "#ff3b3b".into(),
        tab_inactive: "#55597a".into(),
        panel_title: "#f6c344".into(),
        cursor: "#ff3b3b".into(),
        link: "#f6c344".into(),
        tag: "#ff3b3b".into(),
        muted: "#858aa8".into(),
    }
}

pub fn pokemon_charizard() -> Theme {
    Theme {
        name: "Pokémon (Charizard)".into(),
        bg: "#1a0f0d".into(),
        fg: "#ffe9d6".into(),
        accent: "#ff6b35".into(),
        selection: "#2d1913".into(),
        border: "#40241a".into(),
        statusbar: "#120a09".into(),
        highlight: "#ffd93d".into(),
        error: "#ff4444".into(),
        warning: "#ffa502".into(),
        success: "#ff8c42".into(),
        inactive: "#6e4a38".into(),
        scrollbar: "#2d1913".into(),
        tab_active: "#ff3b3b".into(),
        tab_inactive: "#6e4a38".into(),
        panel_title: "#ff6b35".into(),
        cursor: "#ffd93d".into(),
        link: "#ff8c42".into(),
        tag: "#ff6b35".into(),
        muted: "#9a7563".into(),
    }
}

pub fn pokemon_gengar() -> Theme {
    Theme {
        name: "Pokémon (Gengar)".into(),
        bg: "#12101f".into(),
        fg: "#e8d9ff".into(),
        accent: "#9d6bff".into(),
        selection: "#1f1b33".into(),
        border: "#2d2747".into(),
        statusbar: "#0d0b17".into(),
        highlight: "#ff6bd6".into(),
        error: "#c76bff".into(),
        warning: "#b08cff".into(),
        success: "#6bffd0".into(),
        inactive: "#4a4266".into(),
        scrollbar: "#1f1b33".into(),
        tab_active: "#c76bff".into(),
        tab_inactive: "#4a4266".into(),
        panel_title: "#9d6bff".into(),
        cursor: "#ff6bd6".into(),
        link: "#9d6bff".into(),
        tag: "#c76bff".into(),
        muted: "#6e6490".into(),
    }
}

pub fn zelda() -> Theme {
    Theme {
        name: "Zelda".into(),
        bg: "#0c1710".into(),
        fg: "#e8f0dc".into(),
        accent: "#c6a45c".into(),
        selection: "#15261a".into(),
        border: "#1e3526".into(),
        statusbar: "#080f0a".into(),
        highlight: "#ffd700".into(),
        error: "#e5533d".into(),
        warning: "#ffa63d".into(),
        success: "#6bbf59".into(),
        inactive: "#3f5344".into(),
        scrollbar: "#15261a".into(),
        tab_active: "#0b6e2e".into(),
        tab_inactive: "#3f5344".into(),
        panel_title: "#c6a45c".into(),
        cursor: "#ffd700".into(),
        link: "#3aa3ff".into(),
        tag: "#0b6e2e".into(),
        muted: "#7a8d7e".into(),
    }
}

pub fn portal() -> Theme {
    Theme {
        name: "Portal".into(),
        bg: "#e8e8e0".into(),
        fg: "#2b2b2b".into(),
        accent: "#ff8a3d".into(),
        selection: "#d0d0c8".into(),
        border: "#b8b8b0".into(),
        statusbar: "#dcdcd4".into(),
        highlight: "#ffb36b".into(),
        error: "#e0362c".into(),
        warning: "#ff8a3d".into(),
        success: "#2f9de0".into(),
        inactive: "#8a8a85".into(),
        scrollbar: "#d0d0c8".into(),
        tab_active: "#2f9de0".into(),
        tab_inactive: "#8a8a85".into(),
        panel_title: "#ff8a3d".into(),
        cursor: "#2f9de0".into(),
        link: "#2f9de0".into(),
        tag: "#ff8a3d".into(),
        muted: "#8a8a85".into(),
    }
}

pub fn super_mario() -> Theme {
    Theme {
        name: "Super Mario".into(),
        bg: "#16121a".into(),
        fg: "#f5eee2".into(),
        accent: "#e52521".into(),
        selection: "#261f28".into(),
        border: "#352c38".into(),
        statusbar: "#0f0c13".into(),
        highlight: "#ffd75e".into(),
        error: "#ff3b30".into(),
        warning: "#ff9f43".into(),
        success: "#5dbb63".into(),
        inactive: "#5d5464".into(),
        scrollbar: "#261f28".into(),
        tab_active: "#2456a5".into(),
        tab_inactive: "#5d5464".into(),
        panel_title: "#e52521".into(),
        cursor: "#ffd75e".into(),
        link: "#2456a5".into(),
        tag: "#ff9f43".into(),
        muted: "#8a8194".into(),
    }
}

pub fn super_mario_luigi() -> Theme {
    Theme {
        name: "Super Mario (Luigi)".into(),
        bg: "#0e1a12".into(),
        fg: "#e4f2e6".into(),
        accent: "#4a9e5c".into(),
        selection: "#1a2b1f".into(),
        border: "#243a2c".into(),
        statusbar: "#0a130d".into(),
        highlight: "#ffd75e".into(),
        error: "#e5533d".into(),
        warning: "#ff9f43".into(),
        success: "#6fcf78".into(),
        inactive: "#4a5d50".into(),
        scrollbar: "#1a2b1f".into(),
        tab_active: "#2456a5".into(),
        tab_inactive: "#4a5d50".into(),
        panel_title: "#4a9e5c".into(),
        cursor: "#ffd75e".into(),
        link: "#2456a5".into(),
        tag: "#e52521".into(),
        muted: "#758a7e".into(),
    }
}

pub fn overwatch() -> Theme {
    Theme {
        name: "Overwatch".into(),
        bg: "#14161c".into(),
        fg: "#e8eaf0".into(),
        accent: "#f99e1a".into(),
        selection: "#23262f".into(),
        border: "#30343f".into(),
        statusbar: "#0e0f14".into(),
        highlight: "#ffd75e".into(),
        error: "#ff5a5a".into(),
        warning: "#ff9f43".into(),
        success: "#218ffe".into(),
        inactive: "#565b66".into(),
        scrollbar: "#23262f".into(),
        tab_active: "#218ffe".into(),
        tab_inactive: "#565b66".into(),
        panel_title: "#f99e1a".into(),
        cursor: "#218ffe".into(),
        link: "#218ffe".into(),
        tag: "#f99e1a".into(),
        muted: "#7d828c".into(),
    }
}

pub fn halo() -> Theme {
    Theme {
        name: "Halo".into(),
        bg: "#0d1420".into(),
        fg: "#dbe6f2".into(),
        accent: "#6fa55c".into(),
        selection: "#18233a".into(),
        border: "#212f4d".into(),
        statusbar: "#090d16".into(),
        highlight: "#ffd75e".into(),
        error: "#ff5a5a".into(),
        warning: "#ff9f43".into(),
        success: "#7bbfe0".into(),
        inactive: "#3f4a63".into(),
        scrollbar: "#18233a".into(),
        tab_active: "#3b6ea5".into(),
        tab_inactive: "#3f4a63".into(),
        panel_title: "#6fa55c".into(),
        cursor: "#7bbfe0".into(),
        link: "#3b6ea5".into(),
        tag: "#6fa55c".into(),
        muted: "#64708c".into(),
    }
}

pub fn stardew() -> Theme {
    Theme {
        name: "Stardew Valley".into(),
        bg: "#1a1812".into(),
        fg: "#f0e8d8".into(),
        accent: "#6aa84f".into(),
        selection: "#2a251a".into(),
        border: "#3a3426".into(),
        statusbar: "#12100c".into(),
        highlight: "#e0a020".into(),
        error: "#c23b22".into(),
        warning: "#d4881f".into(),
        success: "#8bc34a".into(),
        inactive: "#5d5748".into(),
        scrollbar: "#2a251a".into(),
        tab_active: "#e0a020".into(),
        tab_inactive: "#5d5748".into(),
        panel_title: "#e0a020".into(),
        cursor: "#f0e8d8".into(),
        link: "#6aa84f".into(),
        tag: "#c23b22".into(),
        muted: "#8a8270".into(),
    }
}

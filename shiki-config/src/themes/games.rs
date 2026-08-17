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
    theme! {
        "Pokémon (Pikachu)", "Games",
        bg: "#1a1c2e", fg: "#fff5d6", accent: "#f6c344",
        selection: "#2a2d45", border: "#383c5e", statusbar: "#13141f",
        highlight: "#ffe066", error: "#ff5252", warning: "#ff9f43",
        success: "#7bed9f", inactive: "#55597a", scrollbar: "#2a2d45",
        tab_active: "#ff3b3b", tab_inactive: "#55597a",
        panel_title: "#f6c344", cursor: "#ff3b3b", link: "#f6c344",
        tag: "#ff3b3b", muted: "#858aa8"
    }
}

pub fn pokemon_charizard() -> Theme {
    theme! {
        "Pokémon (Charizard)", "Games",
        bg: "#1a0f0d", fg: "#ffe9d6", accent: "#ff6b35",
        selection: "#2d1913", border: "#40241a", statusbar: "#120a09",
        highlight: "#ffd93d", error: "#ff4444", warning: "#ffa502",
        success: "#ff8c42", inactive: "#6e4a38", scrollbar: "#2d1913",
        tab_active: "#ff3b3b", tab_inactive: "#6e4a38",
        panel_title: "#ff6b35", cursor: "#ffd93d", link: "#ff8c42",
        tag: "#ff6b35", muted: "#9a7563"
    }
}

pub fn pokemon_gengar() -> Theme {
    theme! {
        "Pokémon (Gengar)", "Games",
        bg: "#12101f", fg: "#e8d9ff", accent: "#9d6bff",
        selection: "#1f1b33", border: "#2d2747", statusbar: "#0d0b17",
        highlight: "#ff6bd6", error: "#c76bff", warning: "#b08cff",
        success: "#6bffd0", inactive: "#4a4266", scrollbar: "#1f1b33",
        tab_active: "#c76bff", tab_inactive: "#4a4266",
        panel_title: "#9d6bff", cursor: "#ff6bd6", link: "#9d6bff",
        tag: "#c76bff", muted: "#6e6490"
    }
}

pub fn zelda() -> Theme {
    theme! {
        "Zelda", "Games",
        bg: "#0c1710", fg: "#e8f0dc", accent: "#c6a45c",
        selection: "#15261a", border: "#1e3526", statusbar: "#080f0a",
        highlight: "#ffd700", error: "#e5533d", warning: "#ffa63d",
        success: "#6bbf59", inactive: "#3f5344", scrollbar: "#15261a",
        tab_active: "#0b6e2e", tab_inactive: "#3f5344",
        panel_title: "#c6a45c", cursor: "#ffd700", link: "#3aa3ff",
        tag: "#0b6e2e", muted: "#7a8d7e"
    }
}

pub fn portal() -> Theme {
    theme! {
        "Portal", "Games",
        bg: "#e8e8e0", fg: "#2b2b2b", accent: "#ff8a3d",
        selection: "#d0d0c8", border: "#b8b8b0", statusbar: "#dcdcd4",
        highlight: "#ffb36b", error: "#e0362c", warning: "#ff8a3d",
        success: "#2f9de0", inactive: "#8a8a85", scrollbar: "#d0d0c8",
        tab_active: "#2f9de0", tab_inactive: "#8a8a85",
        panel_title: "#ff8a3d", cursor: "#2f9de0", link: "#2f9de0",
        tag: "#ff8a3d", muted: "#8a8a85"
    }
}

pub fn super_mario() -> Theme {
    theme! {
        "Super Mario", "Games",
        bg: "#16121a", fg: "#f5eee2", accent: "#e52521",
        selection: "#261f28", border: "#352c38", statusbar: "#0f0c13",
        highlight: "#ffd75e", error: "#ff3b30", warning: "#ff9f43",
        success: "#5dbb63", inactive: "#5d5464", scrollbar: "#261f28",
        tab_active: "#2456a5", tab_inactive: "#5d5464",
        panel_title: "#e52521", cursor: "#ffd75e", link: "#2456a5",
        tag: "#ff9f43", muted: "#8a8194"
    }
}

pub fn super_mario_luigi() -> Theme {
    theme! {
        "Super Mario (Luigi)", "Games",
        bg: "#0e1a12", fg: "#e4f2e6", accent: "#4a9e5c",
        selection: "#1a2b1f", border: "#243a2c", statusbar: "#0a130d",
        highlight: "#ffd75e", error: "#e5533d", warning: "#ff9f43",
        success: "#6fcf78", inactive: "#4a5d50", scrollbar: "#1a2b1f",
        tab_active: "#2456a5", tab_inactive: "#4a5d50",
        panel_title: "#4a9e5c", cursor: "#ffd75e", link: "#2456a5",
        tag: "#e52521", muted: "#758a7e"
    }
}

pub fn overwatch() -> Theme {
    theme! {
        "Overwatch", "Games",
        bg: "#14161c", fg: "#e8eaf0", accent: "#f99e1a",
        selection: "#23262f", border: "#30343f", statusbar: "#0e0f14",
        highlight: "#ffd75e", error: "#ff5a5a", warning: "#ff9f43",
        success: "#218ffe", inactive: "#565b66", scrollbar: "#23262f",
        tab_active: "#218ffe", tab_inactive: "#565b66",
        panel_title: "#f99e1a", cursor: "#218ffe", link: "#218ffe",
        tag: "#f99e1a", muted: "#7d828c"
    }
}

pub fn halo() -> Theme {
    theme! {
        "Halo", "Games",
        bg: "#0d1420", fg: "#dbe6f2", accent: "#6fa55c",
        selection: "#18233a", border: "#212f4d", statusbar: "#090d16",
        highlight: "#ffd75e", error: "#ff5a5a", warning: "#ff9f43",
        success: "#7bbfe0", inactive: "#3f4a63", scrollbar: "#18233a",
        tab_active: "#3b6ea5", tab_inactive: "#3f4a63",
        panel_title: "#6fa55c", cursor: "#7bbfe0", link: "#3b6ea5",
        tag: "#6fa55c", muted: "#64708c"
    }
}

pub fn stardew() -> Theme {
    theme! {
        "Stardew Valley", "Games",
        bg: "#1a1812", fg: "#f0e8d8", accent: "#6aa84f",
        selection: "#2a251a", border: "#3a3426", statusbar: "#12100c",
        highlight: "#e0a020", error: "#c23b22", warning: "#d4881f",
        success: "#8bc34a", inactive: "#5d5748", scrollbar: "#2a251a",
        tab_active: "#e0a020", tab_inactive: "#5d5748",
        panel_title: "#e0a020", cursor: "#f0e8d8", link: "#6aa84f",
        tag: "#c23b22", muted: "#8a8270"
    }
}

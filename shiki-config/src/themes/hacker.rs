use crate::theme::Theme;

/// Hacker/cyberpunk-culture palettes, same one-file-per-franchise idea as
/// `lol.rs`/`games.rs`. Each keys off its universe's most iconic screen
/// colors: the Matrix's green phosphor on near-black, Cyberpunk 2077's
/// Night City yellow/cyan/pink, Arasaka corp's red/black with gold, the
/// synthwave sun's magenta/purple/cyan, Tron's electric blue grid, a
/// Vault-Tec CRT-green Fallout terminal, Blade Runner's orange-and-teal
/// Los Angeles noir, Doom's hellfire red, Ghost in the Shell's teal/blue,
/// and Mr. Robot's fsociety red with cyan. Every secondary slot reuses
/// those same colors the way the rest of the themes in this file do.
pub fn matrix() -> Theme {
    theme! {
        "Matrix", "Hacker",
        bg: "#0d0208", fg: "#00ff41", accent: "#00ff41",
        selection: "#003b00", border: "#008f11", statusbar: "#080100",
        highlight: "#a8ff60", error: "#ff4444", warning: "#ffb000",
        success: "#00ff41", inactive: "#006600", scrollbar: "#003b00",
        tab_active: "#00ff41", tab_inactive: "#006600",
        panel_title: "#00ff41", cursor: "#00ff41", link: "#a8ff60",
        tag: "#00ff41", muted: "#008f11"
    }
}

pub fn cyberpunk() -> Theme {
    theme! {
        "Cyberpunk 2077", "Hacker",
        bg: "#100a18", fg: "#e8e6f0", accent: "#fcee0a",
        selection: "#22143a", border: "#33204f", statusbar: "#0a0610",
        highlight: "#ff003c", error: "#ff003c", warning: "#fcee0a",
        success: "#00f0ff", inactive: "#554570", scrollbar: "#22143a",
        tab_active: "#00f0ff", tab_inactive: "#554570",
        panel_title: "#fcee0a", cursor: "#00f0ff", link: "#00f0ff",
        tag: "#ff003c", muted: "#7d6f96"
    }
}

pub fn arasaka() -> Theme {
    theme! {
        "Arasaka", "Hacker",
        bg: "#120808", fg: "#f5e6e6", accent: "#ff003c",
        selection: "#241010", border: "#331717", statusbar: "#0c0505",
        highlight: "#ffd700", error: "#ff003c", warning: "#ffb700",
        success: "#ff6b6b", inactive: "#5a2e2e", scrollbar: "#241010",
        tab_active: "#ffd700", tab_inactive: "#5a2e2e",
        panel_title: "#ff003c", cursor: "#ffd700", link: "#ff6b6b",
        tag: "#ff003c", muted: "#8a5a5a"
    }
}

pub fn synthwave() -> Theme {
    theme! {
        "Synthwave", "Hacker",
        bg: "#16082e", fg: "#f0e6ff", accent: "#ff2ec4",
        selection: "#241246", border: "#33206a", statusbar: "#0f0520",
        highlight: "#ffd400", error: "#ff2e6e", warning: "#ff9e2e",
        success: "#00ffff", inactive: "#584388", scrollbar: "#241246",
        tab_active: "#00ffff", tab_inactive: "#584388",
        panel_title: "#ff2ec4", cursor: "#00ffff", link: "#00ffff",
        tag: "#ff2ec4", muted: "#8a74b8"
    }
}

pub fn tron() -> Theme {
    theme! {
        "Tron", "Hacker",
        bg: "#0a1014", fg: "#d6f4ff", accent: "#00f6ff",
        selection: "#0e2026", border: "#122d36", statusbar: "#060a0d",
        highlight: "#00d8ff", error: "#ff4d4d", warning: "#ffb84d",
        success: "#00f6ff", inactive: "#2e5a66", scrollbar: "#0e2026",
        tab_active: "#00f6ff", tab_inactive: "#2e5a66",
        panel_title: "#00f6ff", cursor: "#00f6ff", link: "#00d8ff",
        tag: "#00f6ff", muted: "#4d7a86"
    }
}

pub fn fallout_terminal() -> Theme {
    theme! {
        "Fallout Terminal", "Hacker",
        bg: "#0a0f0a", fg: "#00ff00", accent: "#00ff00",
        selection: "#0f2410", border: "#1a3a1a", statusbar: "#060a06",
        highlight: "#aaffaa", error: "#ff5555", warning: "#ffb000",
        success: "#00ff00", inactive: "#1d3a1d", scrollbar: "#0f2410",
        tab_active: "#00ff00", tab_inactive: "#1d3a1d",
        panel_title: "#00ff00", cursor: "#00ff00", link: "#aaffaa",
        tag: "#00ff00", muted: "#3a6b3a"
    }
}

pub fn blade_runner() -> Theme {
    theme! {
        "Blade Runner", "Hacker",
        bg: "#0e0a08", fg: "#f2e9de", accent: "#ff9e3d",
        selection: "#1d150f", border: "#2b1f15", statusbar: "#090705",
        highlight: "#ffce6b", error: "#ff5a5a", warning: "#ff9e3d",
        success: "#37c8ab", inactive: "#5a4a38", scrollbar: "#1d150f",
        tab_active: "#37c8ab", tab_inactive: "#5a4a38",
        panel_title: "#ff9e3d", cursor: "#ffce6b", link: "#37c8ab",
        tag: "#ff9e3d", muted: "#8a7660"
    }
}

pub fn doom() -> Theme {
    theme! {
        "Doom", "Hacker",
        bg: "#0d0505", fg: "#f2e0e0", accent: "#e62525",
        selection: "#1f0a0a", border: "#2d0f0f", statusbar: "#080303",
        highlight: "#ff7b3d", error: "#ff2e2e", warning: "#ff9e3d",
        success: "#7bc25e", inactive: "#5a2424", scrollbar: "#1f0a0a",
        tab_active: "#e62525", tab_inactive: "#5a2424",
        panel_title: "#ff7b3d", cursor: "#ff7b3d", link: "#7bc25e",
        tag: "#e62525", muted: "#8a5050"
    }
}

pub fn ghost_in_the_shell() -> Theme {
    theme! {
        "Ghost in the Shell", "Hacker",
        bg: "#0a1214", fg: "#d8f2ee", accent: "#2dd4bf",
        selection: "#122024", border: "#1a2f34", statusbar: "#060a0c",
        highlight: "#8fe3d0", error: "#ff5a7a", warning: "#ffc05a",
        success: "#2dd4bf", inactive: "#33535a", scrollbar: "#122024",
        tab_active: "#37b6ff", tab_inactive: "#33535a",
        panel_title: "#2dd4bf", cursor: "#37b6ff", link: "#37b6ff",
        tag: "#2dd4bf", muted: "#5d7a80"
    }
}

pub fn mr_robot() -> Theme {
    theme! {
        "Mr. Robot", "Hacker",
        bg: "#0c0c10", fg: "#e8e8f0", accent: "#ff3b3b",
        selection: "#1c1c24", border: "#292933", statusbar: "#08080b",
        highlight: "#ffd75e", error: "#ff3b3b", warning: "#ff9e3d",
        success: "#37c8ab", inactive: "#555566", scrollbar: "#1c1c24",
        tab_active: "#37c8ab", tab_inactive: "#555566",
        panel_title: "#ff3b3b", cursor: "#ffd75e", link: "#37c8ab",
        tag: "#ff3b3b", muted: "#7d7d8f"
    }
}

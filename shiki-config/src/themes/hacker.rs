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
    Theme {
        name: "Matrix".into(),
        bg: "#0d0208".into(),
        fg: "#00ff41".into(),
        accent: "#00ff41".into(),
        selection: "#003b00".into(),
        border: "#008f11".into(),
        statusbar: "#080100".into(),
        highlight: "#a8ff60".into(),
        error: "#ff4444".into(),
        warning: "#ffb000".into(),
        success: "#00ff41".into(),
        inactive: "#006600".into(),
        scrollbar: "#003b00".into(),
        tab_active: "#00ff41".into(),
        tab_inactive: "#006600".into(),
        panel_title: "#00ff41".into(),
        cursor: "#00ff41".into(),
        link: "#a8ff60".into(),
        tag: "#00ff41".into(),
        muted: "#008f11".into(),
    }
}

pub fn cyberpunk() -> Theme {
    Theme {
        name: "Cyberpunk 2077".into(),
        bg: "#100a18".into(),
        fg: "#e8e6f0".into(),
        accent: "#fcee0a".into(),
        selection: "#22143a".into(),
        border: "#33204f".into(),
        statusbar: "#0a0610".into(),
        highlight: "#ff003c".into(),
        error: "#ff003c".into(),
        warning: "#fcee0a".into(),
        success: "#00f0ff".into(),
        inactive: "#554570".into(),
        scrollbar: "#22143a".into(),
        tab_active: "#00f0ff".into(),
        tab_inactive: "#554570".into(),
        panel_title: "#fcee0a".into(),
        cursor: "#00f0ff".into(),
        link: "#00f0ff".into(),
        tag: "#ff003c".into(),
        muted: "#7d6f96".into(),
    }
}

pub fn arasaka() -> Theme {
    Theme {
        name: "Arasaka".into(),
        bg: "#120808".into(),
        fg: "#f5e6e6".into(),
        accent: "#ff003c".into(),
        selection: "#241010".into(),
        border: "#331717".into(),
        statusbar: "#0c0505".into(),
        highlight: "#ffd700".into(),
        error: "#ff003c".into(),
        warning: "#ffb700".into(),
        success: "#ff6b6b".into(),
        inactive: "#5a2e2e".into(),
        scrollbar: "#241010".into(),
        tab_active: "#ffd700".into(),
        tab_inactive: "#5a2e2e".into(),
        panel_title: "#ff003c".into(),
        cursor: "#ffd700".into(),
        link: "#ff6b6b".into(),
        tag: "#ff003c".into(),
        muted: "#8a5a5a".into(),
    }
}

pub fn synthwave() -> Theme {
    Theme {
        name: "Synthwave".into(),
        bg: "#16082e".into(),
        fg: "#f0e6ff".into(),
        accent: "#ff2ec4".into(),
        selection: "#241246".into(),
        border: "#33206a".into(),
        statusbar: "#0f0520".into(),
        highlight: "#ffd400".into(),
        error: "#ff2e6e".into(),
        warning: "#ff9e2e".into(),
        success: "#00ffff".into(),
        inactive: "#584388".into(),
        scrollbar: "#241246".into(),
        tab_active: "#00ffff".into(),
        tab_inactive: "#584388".into(),
        panel_title: "#ff2ec4".into(),
        cursor: "#00ffff".into(),
        link: "#00ffff".into(),
        tag: "#ff2ec4".into(),
        muted: "#8a74b8".into(),
    }
}

pub fn tron() -> Theme {
    Theme {
        name: "Tron".into(),
        bg: "#0a1014".into(),
        fg: "#d6f4ff".into(),
        accent: "#00f6ff".into(),
        selection: "#0e2026".into(),
        border: "#122d36".into(),
        statusbar: "#060a0d".into(),
        highlight: "#00d8ff".into(),
        error: "#ff4d4d".into(),
        warning: "#ffb84d".into(),
        success: "#00f6ff".into(),
        inactive: "#2e5a66".into(),
        scrollbar: "#0e2026".into(),
        tab_active: "#00f6ff".into(),
        tab_inactive: "#2e5a66".into(),
        panel_title: "#00f6ff".into(),
        cursor: "#00f6ff".into(),
        link: "#00d8ff".into(),
        tag: "#00f6ff".into(),
        muted: "#4d7a86".into(),
    }
}

pub fn fallout_terminal() -> Theme {
    Theme {
        name: "Fallout Terminal".into(),
        bg: "#0a0f0a".into(),
        fg: "#00ff00".into(),
        accent: "#00ff00".into(),
        selection: "#0f2410".into(),
        border: "#1a3a1a".into(),
        statusbar: "#060a06".into(),
        highlight: "#aaffaa".into(),
        error: "#ff5555".into(),
        warning: "#ffb000".into(),
        success: "#00ff00".into(),
        inactive: "#1d3a1d".into(),
        scrollbar: "#0f2410".into(),
        tab_active: "#00ff00".into(),
        tab_inactive: "#1d3a1d".into(),
        panel_title: "#00ff00".into(),
        cursor: "#00ff00".into(),
        link: "#aaffaa".into(),
        tag: "#00ff00".into(),
        muted: "#3a6b3a".into(),
    }
}

pub fn blade_runner() -> Theme {
    Theme {
        name: "Blade Runner".into(),
        bg: "#0e0a08".into(),
        fg: "#f2e9de".into(),
        accent: "#ff9e3d".into(),
        selection: "#1d150f".into(),
        border: "#2b1f15".into(),
        statusbar: "#090705".into(),
        highlight: "#ffce6b".into(),
        error: "#ff5a5a".into(),
        warning: "#ff9e3d".into(),
        success: "#37c8ab".into(),
        inactive: "#5a4a38".into(),
        scrollbar: "#1d150f".into(),
        tab_active: "#37c8ab".into(),
        tab_inactive: "#5a4a38".into(),
        panel_title: "#ff9e3d".into(),
        cursor: "#ffce6b".into(),
        link: "#37c8ab".into(),
        tag: "#ff9e3d".into(),
        muted: "#8a7660".into(),
    }
}

pub fn doom() -> Theme {
    Theme {
        name: "Doom".into(),
        bg: "#0d0505".into(),
        fg: "#f2e0e0".into(),
        accent: "#e62525".into(),
        selection: "#1f0a0a".into(),
        border: "#2d0f0f".into(),
        statusbar: "#080303".into(),
        highlight: "#ff7b3d".into(),
        error: "#ff2e2e".into(),
        warning: "#ff9e3d".into(),
        success: "#7bc25e".into(),
        inactive: "#5a2424".into(),
        scrollbar: "#1f0a0a".into(),
        tab_active: "#e62525".into(),
        tab_inactive: "#5a2424".into(),
        panel_title: "#ff7b3d".into(),
        cursor: "#ff7b3d".into(),
        link: "#7bc25e".into(),
        tag: "#e62525".into(),
        muted: "#8a5050".into(),
    }
}

pub fn ghost_in_the_shell() -> Theme {
    Theme {
        name: "Ghost in the Shell".into(),
        bg: "#0a1214".into(),
        fg: "#d8f2ee".into(),
        accent: "#2dd4bf".into(),
        selection: "#122024".into(),
        border: "#1a2f34".into(),
        statusbar: "#060a0c".into(),
        highlight: "#8fe3d0".into(),
        error: "#ff5a7a".into(),
        warning: "#ffc05a".into(),
        success: "#2dd4bf".into(),
        inactive: "#33535a".into(),
        scrollbar: "#122024".into(),
        tab_active: "#37b6ff".into(),
        tab_inactive: "#33535a".into(),
        panel_title: "#2dd4bf".into(),
        cursor: "#37b6ff".into(),
        link: "#37b6ff".into(),
        tag: "#2dd4bf".into(),
        muted: "#5d7a80".into(),
    }
}

pub fn mr_robot() -> Theme {
    Theme {
        name: "Mr. Robot".into(),
        bg: "#0c0c10".into(),
        fg: "#e8e8f0".into(),
        accent: "#ff3b3b".into(),
        selection: "#1c1c24".into(),
        border: "#292933".into(),
        statusbar: "#08080b".into(),
        highlight: "#ffd75e".into(),
        error: "#ff3b3b".into(),
        warning: "#ff9e3d".into(),
        success: "#37c8ab".into(),
        inactive: "#555566".into(),
        scrollbar: "#1c1c24".into(),
        tab_active: "#37c8ab".into(),
        tab_inactive: "#555566".into(),
        panel_title: "#ff3b3b".into(),
        cursor: "#ffd75e".into(),
        link: "#37c8ab".into(),
        tag: "#ff3b3b".into(),
        muted: "#7d7d8f".into(),
    }
}

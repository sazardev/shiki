use crate::theme::Theme;

/// Builds a `Theme` from the full 19 color slots plus its `family`.
/// Every slot is a required argument — a theme that forgets one (or adds a
/// new slot to `Theme` without updating every palette) fails to compile
/// instead of silently falling back. Defined here, before the `mod`
/// declarations, so every child module below sees it in textual scope.
macro_rules! theme {
    (
        $name:expr, $family:expr,
        bg: $bg:expr, fg: $fg:expr, accent: $accent:expr,
        selection: $selection:expr, border: $border:expr, statusbar: $statusbar:expr,
        highlight: $highlight:expr, error: $error:expr, warning: $warning:expr,
        success: $success:expr, inactive: $inactive:expr, scrollbar: $scrollbar:expr,
        tab_active: $tab_active:expr, tab_inactive: $tab_inactive:expr,
        panel_title: $panel_title:expr, cursor: $cursor:expr, link: $link:expr,
        tag: $tag:expr, muted: $muted:expr
    ) => {
        Theme {
            name: $name.into(),
            family: $family,
            bg: $bg.into(),
            fg: $fg.into(),
            accent: $accent.into(),
            selection: $selection.into(),
            border: $border.into(),
            statusbar: $statusbar.into(),
            highlight: $highlight.into(),
            error: $error.into(),
            warning: $warning.into(),
            success: $success.into(),
            inactive: $inactive.into(),
            scrollbar: $scrollbar.into(),
            tab_active: $tab_active.into(),
            tab_inactive: $tab_inactive.into(),
            panel_title: $panel_title.into(),
            cursor: $cursor.into(),
            link: $link.into(),
            tag: $tag.into(),
            muted: $muted.into(),
        }
    };
}

mod catppuccin;
mod dracula;
mod games;
mod gruvbox;
mod hacker;
mod lol;
mod monokai;
mod nord;
mod one_dark;
mod solarized;
mod tokyo_night;

/// All themes included out of the box, in the order they're shown in the config.
/// Alphabetical (case-insensitive) except for `default` (the terminal-inherit
/// theme), which stays last on purpose. `docs/js/main.js`'s THEMES array
/// mirrors this exactly — same order, same names.
pub fn all() -> Vec<Theme> {
    vec![
        hacker::arasaka(),
        hacker::blade_runner(),
        catppuccin::mocha(),
        hacker::cyberpunk(),
        hacker::doom(),
        dracula::dracula(),
        hacker::fallout_terminal(),
        hacker::ghost_in_the_shell(),
        gruvbox::dark(),
        gruvbox::dark_hard(),
        gruvbox::dark_soft(),
        gruvbox::light(),
        gruvbox::light_hard(),
        gruvbox::light_soft(),
        games::halo(),
        lol::ahri(),
        lol::jinx(),
        lol::teemo(),
        hacker::matrix(),
        monokai::monokai(),
        hacker::mr_robot(),
        nord::nord(),
        one_dark::one_dark(),
        games::overwatch(),
        games::pokemon_charizard(),
        games::pokemon_gengar(),
        games::pokemon_pikachu(),
        games::portal(),
        solarized::dark(),
        games::stardew(),
        games::super_mario(),
        games::super_mario_luigi(),
        hacker::synthwave(),
        tokyo_night::night(),
        hacker::tron(),
        games::zelda(),
        Theme::terminal_default(),
    ]
}

/// Looks up a built-in theme by name (e.g. `"catppuccin-mocha"`).
pub fn by_name(name: &str) -> Option<Theme> {
    all().into_iter().find(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every slot a theme may use, so `valid_slots` can't miss a field if
    /// `Theme` grows a new one.
    fn slots(t: &Theme) -> Vec<&str> {
        vec![
            &t.bg,
            &t.fg,
            &t.accent,
            &t.selection,
            &t.border,
            &t.statusbar,
            &t.highlight,
            &t.error,
            &t.warning,
            &t.success,
            &t.inactive,
            &t.scrollbar,
            &t.tab_active,
            &t.tab_inactive,
            &t.panel_title,
            &t.cursor,
            &t.link,
            &t.tag,
            &t.muted,
        ]
    }

    /// The same accepted forms `shiki-tui/src/render.rs::hex_to_color`
    /// resolves: `#rrggbb` hex, terminal ANSI names, or `"reset"`.
    fn is_valid_slot(value: &str) -> bool {
        let lower = value.to_ascii_lowercase();
        match lower.as_str() {
            "" | "reset" | "black" | "red" | "green" | "yellow" | "blue" | "magenta" | "cyan"
            | "white" | "gray" | "grey" | "darkgray" | "darkgrey" => return true,
            _ => {}
        }
        let hex = value.trim_start_matches('#');
        hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
    }

    const KNOWN_FAMILIES: &[&str] = &["LoL", "Games", "Hacker", "Classic", "System"];

    #[test]
    fn every_slot_parses_as_a_valid_color() {
        for theme in all() {
            for slot in slots(&theme) {
                assert!(
                    is_valid_slot(slot),
                    "{}: invalid color slot `{slot}`",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn names_are_unique() {
        let themes = all();
        let mut names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
        let dupes = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            dupes,
            "duplicate theme names in all(): {:?}",
            names
        );
    }

    #[test]
    fn list_is_alphabetical_with_default_last() {
        let themes = all();
        assert_eq!(
            themes.last().unwrap().name,
            "default",
            "`default` must stay last"
        );
        let names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
        let mut sorted: Vec<&str> = names[..names.len() - 1].to_vec();
        sorted.sort_by_key(|n| n.to_lowercase());
        assert_eq!(
            &names[..names.len() - 1],
            sorted.as_slice(),
            "themes must be alphabetical (case-insensitive), `default` last"
        );
    }

    #[test]
    fn every_theme_has_a_known_family() {
        for theme in all() {
            assert!(
                KNOWN_FAMILIES.contains(&theme.family),
                "{}: unknown family `{}`",
                theme.name,
                theme.family
            );
        }
    }

    #[test]
    fn by_name_resolves_every_theme() {
        for theme in all() {
            let resolved =
                by_name(&theme.name).unwrap_or_else(|| panic!("{} must resolve", theme.name));
            assert_eq!(resolved.name, theme.name);
            assert_eq!(resolved.family, theme.family);
            assert_eq!(resolved, theme);
        }
    }

    /// Guards the "37 built-in themes" claim on the site/docs — a theme
    /// added or removed here must also update main.js/styles.css/screenshots.
    #[test]
    fn catalog_has_37_themes() {
        assert_eq!(all().len(), 37);
    }
}

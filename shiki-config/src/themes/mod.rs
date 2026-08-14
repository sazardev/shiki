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

use crate::theme::Theme;

/// All themes included out of the box, in the order they're shown in the config.
/// Alphabetical (case-insensitive) except for `default` (the terminal-inherit
/// theme), which stays last on purpose. `docs/js/main.js`'s THEMES array
/// mirrors this exactly — same order, same names.
pub fn all() -> Vec<Theme> {
    vec![
        hacker::arasaka(),
        hacker::blade_runner(),
        catppuccin::frappe(),
        catppuccin::latte(),
        catppuccin::macchiato(),
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
        solarized::light(),
        games::stardew(),
        games::super_mario(),
        games::super_mario_luigi(),
        hacker::synthwave(),
        tokyo_night::night(),
        tokyo_night::moon(),
        tokyo_night::storm(),
        hacker::tron(),
        games::zelda(),
        Theme::terminal_default(),
    ]
}

/// Looks up a built-in theme by name (e.g. `"catppuccin-mocha"`).
pub fn by_name(name: &str) -> Option<Theme> {
    all().into_iter().find(|t| t.name == name)
}

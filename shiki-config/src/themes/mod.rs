mod catppuccin;
mod dracula;
mod games;
mod gruvbox;
mod lol;
mod monokai;
mod nord;
mod one_dark;
mod solarized;
mod tokyo_night;

use crate::theme::Theme;

/// All themes included out of the box, in the order they're shown in the config.
pub fn all() -> Vec<Theme> {
    vec![
        catppuccin::mocha(),
        catppuccin::macchiato(),
        catppuccin::frappe(),
        catppuccin::latte(),
        tokyo_night::storm(),
        tokyo_night::night(),
        tokyo_night::moon(),
        gruvbox::dark(),
        gruvbox::light(),
        nord::nord(),
        solarized::dark(),
        solarized::light(),
        dracula::dracula(),
        one_dark::one_dark(),
        monokai::monokai(),
        lol::jinx(),
        lol::teemo(),
        lol::ahri(),
        games::pokemon_pikachu(),
        games::pokemon_charizard(),
        games::pokemon_gengar(),
        games::zelda(),
        games::portal(),
        games::super_mario(),
        games::super_mario_luigi(),
        games::overwatch(),
        games::halo(),
        games::stardew(),
        Theme::terminal_default(),
    ]
}

/// Looks up a built-in theme by name (e.g. `"catppuccin-mocha"`).
pub fn by_name(name: &str) -> Option<Theme> {
    all().into_iter().find(|t| t.name == name)
}

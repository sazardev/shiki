use anyhow::Result;
use shiki_config::Config;

pub fn list(config: &Config) -> Result<()> {
    let mut themes: Vec<_> = shiki_config::themes::all()
        .into_iter()
        .map(|t| {
            let marker = if t.name == config.theme.name {
                "*"
            } else {
                " "
            };
            (marker, t)
        })
        .collect();
    // Grouped by family, alphabetical within — same order the TUI picker
    // shows. `default` (System) naturally lands last.
    themes.sort_by(|(_, a), (_, b)| {
        a.family
            .to_lowercase()
            .cmp(&b.family.to_lowercase())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let mut last_family: Option<&str> = None;
    for (marker, theme) in themes {
        if last_family != Some(theme.family) {
            println!("  ── {} ──", theme.family);
            last_family = Some(theme.family);
        }
        println!("{marker} {}", theme.name);
    }
    Ok(())
}

pub fn set(config: &mut Config, name: &str, notebook: Option<&str>) -> Result<()> {
    if shiki_config::themes::by_name(name).is_none() {
        anyhow::bail!("unknown theme '{name}' — run `shiki theme list` to see available themes");
    }
    // The "currently committed base" differs for per-notebook overrides, so
    // only reset overrides when the *effective* base actually changes —
    // re-running `set` on the theme that's already active used to silently
    // wipe any hand-written custom colors for no reason.
    let committed_base = config.theme_for(notebook).name;
    let base_changed = committed_base != name;
    match notebook {
        Some(nb) => {
            let over = config.notebooks.entry(nb.to_string()).or_default();
            over.theme_name = Some(name.to_string());
            if base_changed {
                over.theme_overrides = Default::default();
            }
            config.save(&Config::default_path()?)?;
            println!("theme set to '{name}' for notebook '{nb}'");
        }
        None => {
            config.theme.name = name.to_string();
            if base_changed {
                config.theme.overrides = Default::default();
            }
            config.save(&Config::default_path()?)?;
            println!("theme set to '{name}'");
        }
    }
    Ok(())
}

/// Scaffolds every one of the 19 color slots into config.toml's
/// `[theme.overrides]` (or, with `notebook`, that notebook's own
/// `[notebooks.<name>]` override), copied from a real theme's values — a
/// genuine starting point to edit slot-by-slot, rather than hand-writing hex
/// codes from scratch with no example to copy from. `from` defaults to
/// whichever theme is currently active (globally, or for `notebook`) if not
/// given.
pub fn create(config: &mut Config, from: Option<&str>, notebook: Option<&str>) -> Result<()> {
    let base_name = from
        .map(str::to_string)
        .unwrap_or_else(|| config.theme_for(notebook).name);
    let base = shiki_config::themes::by_name(&base_name).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown theme '{base_name}' — run `shiki theme list` to see available themes"
        )
    })?;
    let path = Config::default_path()?;
    match notebook {
        Some(nb) => {
            // Same reasoning as the global case below: the base theme
            // becomes this notebook's `theme_name` too, not just the source
            // for its override values.
            let over = config.notebooks.entry(nb.to_string()).or_default();
            over.theme_name = Some(base_name.clone());
            over.theme_overrides = shiki_config::config::ThemeOverrides::from_theme(&base);
            config.save(&path)?;
            println!(
                "scaffolded all 19 color slots from '{base_name}' into {}'s [notebooks.{nb}] — \
                 edit any value there; removing a key falls back to the global theme for that slot",
                path.display()
            );
        }
        None => {
            // The base theme becomes `theme.name` too, not just the source
            // for the override values — every slot is about to be explicitly
            // overridden with `base`'s own colors, so leaving `theme.name`
            // pointing at whatever was active before would make the printed
            // "falls back to" guidance below wrong the moment someone
            // removes a key.
            config.theme.name = base_name.clone();
            config.theme.overrides = shiki_config::config::ThemeOverrides::from_theme(&base);
            config.save(&path)?;
            println!(
                "scaffolded all 19 color slots from '{base_name}' into {}'s [theme.overrides] — \
                 edit any value there; removing a key falls back to '{base_name}' for that slot",
                path.display()
            );
        }
    }
    Ok(())
}

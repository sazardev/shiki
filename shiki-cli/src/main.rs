mod commands;
mod tui;

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use shiki_config::Config;
use shiki_core::NotebookStore;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "shiki",
    version,
    about = "shiki (私記) — personal notes in the terminal"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Creates a new note and opens $EDITOR
    New {
        title: String,
        #[arg(short, long)]
        notebook: Option<String>,
    },
    /// Lists the notes in a notebook
    List {
        #[arg(short = 'n', long)]
        notebook: Option<String>,
    },
    /// Edits a note with $EDITOR
    Edit {
        note: String,
        #[arg(short, long)]
        notebook: Option<String>,
    },
    /// Shows the rendered contents of a note
    Show {
        note: String,
        #[arg(short, long)]
        notebook: Option<String>,
    },
    /// Searches notes by title (fuzzy)
    Search {
        query: String,
        #[arg(short, long)]
        notebook: Option<String>,
    },
    /// Creates or opens today's daily note
    Daily {
        #[arg(short, long)]
        notebook: Option<String>,
    },
    /// Git commit (+ push if enabled) for a notebook
    Sync {
        #[arg(short = 'n', long)]
        notebook: Option<String>,
    },
    /// Shows the path to the config file
    Config,
    /// Checks the environment (config, data dir, git, editor, terminal, notebooks)
    Doctor,
    /// Manages notebooks
    Notebook {
        #[command(subcommand)]
        action: NotebookAction,
    },
    /// Lists or switches the color theme
    Theme {
        #[command(subcommand)]
        action: ThemeAction,
    },
}

#[derive(Subcommand)]
enum NotebookAction {
    Create {
        name: String,
    },
    List,
    Rename {
        old: String,
        new: String,
    },
    /// Permanently deletes a notebook and every note in it — irreversible,
    /// so it requires `--yes` rather than deleting on the bare name alone
    /// (the TUI's equivalent action is gated behind its own confirm
    /// dialog; this is the CLI's version of that same guard).
    Delete {
        name: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum ThemeAction {
    /// Lists all built-in themes, marking the active one
    List,
    /// Sets the active theme by name (see `shiki theme list`)
    Set { name: String },
    /// Scaffolds every one of the 19 color slots as an explicit override in
    /// config.toml, copied from a real theme's values — a starting point to
    /// edit, not blank fields. Defaults to the currently active theme if
    /// `--from` is omitted.
    Create {
        #[arg(long)]
        from: Option<String>,
    },
}

struct Context {
    config: Config,
    store: NotebookStore,
}

impl Context {
    fn load() -> Result<Self> {
        let config_path = Config::default_path()?;
        let config = Config::load_or_init(&config_path)?;
        let templates_dir = Config::default_templates_dir()?;
        shiki_core::templates::ensure_defaults(&templates_dir)?;
        let data_dir = match config.general.data_dir.as_ref() {
            Some(dir) => PathBuf::from(dir),
            None => Config::default_data_dir()
                .context("could not determine a default data directory (no $HOME/$XDG_DATA_HOME?) — set general.data_dir in config.toml")?,
        };
        let custom_paths = config.notebook_custom_paths();
        let store = NotebookStore::new_with_custom_paths(data_dir, custom_paths);
        Ok(Self { config, store })
    }

    fn notebook_name(&self, override_name: Option<String>) -> String {
        override_name.unwrap_or_else(|| self.config.general.default_notebook.clone())
    }

    /// The editor command to actually launch — `general.use_favorite_editor`
    /// (the TUI's `i` binding already resolves this way) means the CLI
    /// should too, rather than always using the plain configured
    /// `general.editor` regardless of that setting. Falls back to the
    /// configured editor when favorite-editor detection finds nothing, same
    /// as the TUI's own fallback.
    fn resolve_editor(&self) -> String {
        if self.config.general.use_favorite_editor {
            if let Some(fav) = shiki_core::editor::detect_favorite_editor() {
                return fav;
            }
        }
        self.config.general.editor.clone()
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    // Handled before `Context::load()` — doctor needs to work (and say why)
    // even when the config is broken, which is precisely the situation
    // someone reaching for it is usually in.
    if matches!(cli.command, Some(Commands::Doctor)) {
        return commands::doctor::run();
    }

    let mut ctx = Context::load()?;

    match cli.command {
        None => tui::launch(ctx.config, ctx.store),
        Some(Commands::New { title, notebook }) => {
            let notebook = ctx.notebook_name(notebook);
            let editor = ctx.resolve_editor();
            commands::new::run(&ctx.store, &notebook, &title, &editor)
        }
        Some(Commands::List { notebook }) => {
            let notebook = ctx.notebook_name(notebook);
            commands::list::run(&ctx.store, &notebook)
        }
        Some(Commands::Edit { note, notebook }) => {
            let notebook = ctx.notebook_name(notebook);
            let editor = ctx.resolve_editor();
            commands::edit::run(&ctx.store, &notebook, &note, &editor)
        }
        Some(Commands::Show { note, notebook }) => {
            let notebook = ctx.notebook_name(notebook);
            commands::show::run(&ctx.store, &notebook, &note)
        }
        Some(Commands::Search { query, notebook }) => {
            let notebook = ctx.notebook_name(notebook);
            commands::search::run(&ctx.store, &notebook, &query)
        }
        Some(Commands::Daily { notebook }) => {
            let notebook = ctx.notebook_name(notebook);
            let templates_dir = Config::default_templates_dir()?;
            let editor = ctx.resolve_editor();
            commands::daily::run(
                &ctx.store,
                &notebook,
                &templates_dir,
                &editor,
                &ctx.config.general.daily_template,
            )
        }
        Some(Commands::Sync { notebook }) => {
            let notebook = ctx.notebook_name(notebook);
            commands::sync::run(&ctx.store, &notebook, &ctx.config)
        }
        Some(Commands::Config) => commands::config::run(),
        Some(Commands::Doctor) => unreachable!("handled before Context::load() above"),
        Some(Commands::Notebook { action }) => match action {
            NotebookAction::Create { name } => commands::notebook::create(&ctx.store, &name),
            NotebookAction::List => commands::notebook::list(&ctx.store),
            NotebookAction::Rename { old, new } => {
                commands::notebook::rename(&ctx.store, &old, &new)
            }
            NotebookAction::Delete { name, yes } => {
                commands::notebook::delete(&ctx.store, &name, yes)
            }
        },
        Some(Commands::Theme { action }) => match action {
            ThemeAction::List => commands::theme::list(&ctx.config),
            ThemeAction::Set { name } => commands::theme::set(&mut ctx.config, &name),
            ThemeAction::Create { from } => {
                commands::theme::create(&mut ctx.config, from.as_deref())
            }
        },
    }
}

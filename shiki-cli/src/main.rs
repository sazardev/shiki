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
    /// Creates a new note. Opens $EDITOR unless `--body`/`--stdin` is given,
    /// in which case the note is created non-interactively — for scripting/
    /// automation (piping generated content in from another program).
    New {
        title: String,
        #[arg(short, long)]
        notebook: Option<String>,
        /// Note body text, given directly instead of opening $EDITOR.
        #[arg(long, conflicts_with = "stdin")]
        body: Option<String>,
        /// Reads the note body from stdin instead of opening $EDITOR —
        /// e.g. `echo "content" | shiki new "title" --stdin`.
        #[arg(long)]
        stdin: bool,
        /// Comma-separated tags, e.g. `--tags work,idea`.
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
    /// Lists the notes in a notebook
    List {
        #[arg(short = 'n', long)]
        notebook: Option<String>,
        /// Emits a JSON array instead of plain text — for scripting.
        #[arg(long)]
        json: bool,
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
        /// Emits a JSON object instead of plain text — for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Searches notes by title (fuzzy)
    Search {
        query: String,
        #[arg(short, long)]
        notebook: Option<String>,
        /// Emits a JSON array instead of plain text — for scripting.
        #[arg(long)]
        json: bool,
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
    /// Exports every note in a notebook to a single HTML or Markdown file
    Export {
        #[arg(short = 'n', long)]
        notebook: Option<String>,
        /// Output file path.
        #[arg(short, long)]
        out: PathBuf,
        /// Output format — defaults to a self-contained HTML file.
        #[arg(long, value_enum, default_value = "html")]
        format: commands::export::ExportFormat,
    },
    /// Renders every note in a notebook to a themed PDF via `pretty-pdf`
    /// (go-pretty-pdf) — fetched automatically on first use if it isn't
    /// already on `$PATH`, no manual install step required.
    Publish {
        #[arg(short = 'n', long)]
        notebook: Option<String>,
        /// Output PDF path — defaults to `{data_dir}/exports/{notebook}.pdf`
        /// so it doesn't land inside the git-tracked notebook directory.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// One of go-pretty-pdf's 17 built-in themes — defaults to
        /// `export.pdf_theme` in config.toml.
        #[arg(long)]
        theme: Option<String>,
    },
    /// Lists checkbox tasks (`- [ ]`) across notebooks — the TUI's tasks
    /// view (leader+`t`), scriptable. `--count`/`--json` are made for
    /// status bars: e.g. `shiki tasks --overdue --count` in a waybar/
    /// polybar/tmux module.
    Tasks {
        /// Only tasks from this notebook (default: all notebooks).
        #[arg(short = 'n', long)]
        notebook: Option<String>,
        /// Only overdue tasks (due before today). Combinable with
        /// `--today` for "due today or earlier".
        #[arg(long)]
        overdue: bool,
        /// Only tasks due exactly today.
        #[arg(long)]
        today: bool,
        /// Include already-completed tasks too.
        #[arg(long)]
        all: bool,
        /// Prints just the number of matching tasks — for status bars.
        #[arg(long, conflicts_with = "json")]
        count: bool,
        /// Emits a JSON array instead of plain text — for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Renders the `[[wikilink]]` connection graph of your notes as a
    /// force-directed layout right in the terminal — hubs (`◉`) pull their
    /// linked notes around them, orphans (`○`) drift free and are listed
    /// below.
    Graph {
        /// Only this notebook's graph (default: all notebooks — links
        /// never cross notebooks, so each renders as its own cluster).
        #[arg(short = 'n', long)]
        notebook: Option<String>,
        /// Canvas width in columns (default: the terminal's own width).
        #[arg(long)]
        width: Option<u16>,
        /// Emits nodes/edges/orphans as JSON instead of drawing — for
        /// piping into real graph tooling (graphviz, gephi, d3).
        #[arg(long)]
        json: bool,
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
    List {
        /// Emits a JSON array instead of plain text — for scripting.
        #[arg(long)]
        json: bool,
    },
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
        Some(Commands::New {
            title,
            notebook,
            body,
            stdin,
            tags,
        }) => {
            let notebook = ctx.notebook_name(notebook);
            let editor = ctx.resolve_editor();
            let non_interactive_body = if stdin {
                use std::io::Read as _;
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .context("failed to read note body from stdin")?;
                Some(buf)
            } else {
                body
            };
            commands::new::run(
                &ctx.store,
                &notebook,
                &title,
                &editor,
                non_interactive_body.as_deref(),
                &tags,
            )
        }
        Some(Commands::List { notebook, json }) => {
            let notebook = ctx.notebook_name(notebook);
            commands::list::run(&ctx.store, &notebook, json)
        }
        Some(Commands::Edit { note, notebook }) => {
            let notebook = ctx.notebook_name(notebook);
            let editor = ctx.resolve_editor();
            commands::edit::run(&ctx.store, &notebook, &note, &editor)
        }
        Some(Commands::Show {
            note,
            notebook,
            json,
        }) => {
            let notebook = ctx.notebook_name(notebook);
            commands::show::run(&ctx.store, &notebook, &note, json)
        }
        Some(Commands::Search {
            query,
            notebook,
            json,
        }) => {
            let notebook = ctx.notebook_name(notebook);
            commands::search::run(&ctx.store, &notebook, &query, json)
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
                ctx.config.general.daily_agenda,
            )
        }
        Some(Commands::Sync { notebook }) => {
            let notebook = ctx.notebook_name(notebook);
            commands::sync::run(&ctx.store, &notebook, &ctx.config)
        }
        Some(Commands::Export {
            notebook,
            out,
            format,
        }) => {
            let notebook = ctx.notebook_name(notebook);
            commands::export::run(&ctx.store, &notebook, &out, format)
        }
        Some(Commands::Publish {
            notebook,
            out,
            theme,
        }) => {
            let notebook = ctx.notebook_name(notebook);
            let theme = theme.unwrap_or_else(|| ctx.config.export.pdf_theme.clone());
            let export_dir = ctx.config.export.export_dir.trim();
            let export_dir = if export_dir.is_empty() {
                ctx.store.root.join("exports")
            } else {
                std::path::PathBuf::from(export_dir)
            };
            let out = out.unwrap_or(export_dir.join(format!("{notebook}.pdf")));
            let cache_dir = ctx.store.root.join("bin");
            commands::publish::run(&ctx.store, &notebook, &out, &theme, &cache_dir)
        }
        Some(Commands::Tasks {
            notebook,
            overdue,
            today,
            all,
            count,
            json,
        }) => commands::tasks::run(
            &ctx.store,
            notebook.as_deref(),
            &commands::tasks::Filters {
                overdue,
                today,
                all,
            },
            json,
            count,
        ),
        Some(Commands::Graph {
            notebook,
            width,
            json,
        }) => commands::graph::run(&ctx.store, notebook.as_deref(), width, json),
        Some(Commands::Config) => commands::config::run(),
        Some(Commands::Doctor) => unreachable!("handled before Context::load() above"),
        Some(Commands::Notebook { action }) => match action {
            NotebookAction::Create { name } => commands::notebook::create(&ctx.store, &name),
            NotebookAction::List { json } => commands::notebook::list(&ctx.store, json),
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

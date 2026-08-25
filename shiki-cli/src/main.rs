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
    /// Captures a quick note with no editor and (almost) no ceremony —
    /// tries a running TUI's capture daemon first (if it's enabled and
    /// listening) so the note shows up there live, then falls back to
    /// writing straight to disk if no daemon is reachable. Meant for
    /// scripts/launchers (rofi, waybar, Raycast, hotkeys, etc.), not
    /// interactive use.
    Capture {
        /// Text to capture. Omit it to read from stdin instead, e.g.
        /// `echo "idea" | shiki capture`. Ignored (and not read) when
        /// `--check`/`--undo` is given. If it starts with `"<notebook>:
        /// "` and no `-n` was given, and `<notebook>` names a real
        /// notebook, that notebook is used automatically (the prefix is
        /// stripped from the saved text) — e.g. `shiki capture "work:
        /// call Ana"`.
        text: Option<String>,
        /// Overrides both `general.default_notebook` and content-prefix
        /// routing — always wins outright when given.
        #[arg(short, long)]
        notebook: Option<String>,
        /// Comma-separated tags, e.g. `--tags work,idea`. Ignored when
        /// `--daily` is given — an appended bullet has no frontmatter of
        /// its own to tag.
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        /// Creates the note inside this subfolder of the notebook instead
        /// of its root, e.g. `--folder work/meetings`. Ignored when
        /// `--daily` is given — a daily note's path is always fixed.
        #[arg(long)]
        folder: Option<String>,
        /// Appends the text as a bullet to today's daily note instead of
        /// creating a new note — for using the daily note as a running
        /// inbox rather than one note per capture.
        #[arg(long)]
        daily: bool,
        /// Emits `{"path": ..., "daemon": ..., "daily": ...}` instead of a
        /// plain sentence — for scripts/browser extensions.
        #[arg(long)]
        json: bool,
        /// Reports whether a capture daemon is reachable right now and
        /// exits — doesn't capture anything or read stdin. Exits non-zero
        /// when unreachable, so `shiki capture --check && ...` composes.
        #[arg(long, conflicts_with = "undo")]
        check: bool,
        /// Reverses the single most recent capture (whichever kind, from
        /// either this machine's daemon or the standalone fallback) —
        /// moves a plain note to trash, or strips the bullet back off a
        /// `--daily` append. Doesn't touch stdin/text.
        #[arg(long)]
        undo: bool,
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
    /// Queries notes by frontmatter field — a Dataview-style filter/sort,
    /// e.g. `shiki query 'where status = pending sort due asc'`. Same
    /// engine as the TUI's leader+`q` modal, so a query means the same
    /// thing in both places. `--count`/`--json` are for status bars and
    /// scripting, same as `shiki tasks`.
    Query {
        /// The query DSL string (quote it). Omit if using `--saved`.
        dsl: Option<String>,
        /// Runs a query saved under `[queries]` in config.toml
        /// instead of a literal DSL string.
        #[arg(long, conflicts_with = "dsl")]
        saved: Option<String>,
        /// Only notes from this notebook (default: all notebooks).
        #[arg(short = 'n', long)]
        notebook: Option<String>,
        /// Prints just the number of matching notes — for status bars.
        #[arg(long, conflicts_with = "json")]
        count: bool,
        /// Emits a JSON array instead of plain text — for scripting.
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
    /// Browser extension — install the native host and pack the extension (limpio y guardado)
    Extension {
        #[command(subcommand)]
        action: commands::extension::ExtensionAction,
    },
    /// Imports notes from other apps (Obsidian vaults, Notion exports)
    Import {
        #[command(subcommand)]
        action: ImportAction,
    },
}

#[derive(Subcommand)]
enum ImportAction {
    /// Adopts an existing Obsidian vault as a shiki notebook — in-place by
    /// default (registered under `[notebooks.<name>] path`, files stay
    /// where they are), or copied into the data dir with `--copy`.
    Obsidian {
        /// Path to the vault's root folder (`~` expands).
        path: String,
        /// Notebook name (defaults to the folder's own name).
        #[arg(long)]
        name: Option<String>,
        /// Copy everything into a fresh notebook under the data dir
        /// instead of adopting the folder in place.
        #[arg(long)]
        copy: bool,
        /// Merge inline #hashtags into each note's frontmatter tags
        /// (notes without frontmatter are left untouched).
        #[arg(long)]
        tags: bool,
        /// With adoption: run `git init` when the vault isn't already a
        /// repo, so sync works from day one.
        #[arg(long)]
        git_init: bool,
    },
    /// Converts a Notion markdown export (the `.zip`, or an already-
    /// extracted folder) into a fresh notebook: UUID suffixes are stripped
    /// from page/folder names, internal page links become `[[wikilinks]]`,
    /// CSV databases are reported but skipped.
    Notion {
        /// Path to the export's `.zip` or extracted folder (`~` expands).
        path: String,
        /// Notebook name (defaults to the zip/folder name, cleaned).
        #[arg(long)]
        name: Option<String>,
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
        /// Also lists notebooks untracked via "keep files, just untrack"
        /// (marked `(hidden)`), which are excluded by default.
        #[arg(long)]
        all: bool,
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
    /// Enables encryption at rest for an existing notebook — prompts for a
    /// passphrase (twice), re-encrypts every existing note, and commits.
    /// The same `age::scrypt` passphrase engine the TUI uses.
    Encrypt {
        name: String,
    },
    /// Reverses `encrypt`: decrypts every note back to plain text.
    Decrypt {
        name: String,
    },
    /// Changes an encrypted notebook's passphrase without first decrypting
    /// to plain text in between: verifies the old passphrase against the
    /// canary, re-encrypts every note with the new one, and rewrites the
    /// canary. The `decrypt` + `encrypt` two-step also works, but this never
    /// leaves the notes unencrypted on disk mid-operation.
    Rekey {
        name: String,
    },
}

#[derive(Subcommand)]
enum ThemeAction {
    /// Lists all built-in themes, marking the active one
    List,
    /// Sets the active theme by name (see `shiki theme list`). With
    /// `--notebook <name>`, sets that notebook's override instead of the
    /// global theme.
    Set {
        name: String,
        #[arg(long)]
        notebook: Option<String>,
    },
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

    let mut cli = Cli::parse();

    // Handled before `Context::load()` — doctor and extension need to work
    // even when the config is broken.
    // `matches!` guards the `take()`: a bare `take()` would consume *any*
    // command into this check and leave `cli.command` as None, silently
    // routing every other subcommand (list, sync, import…) into the TUI.
    if matches!(cli.command, Some(Commands::Extension { .. })) {
        if let Some(Commands::Extension { action }) = cli.command.take() {
            return commands::extension::run(action);
        }
    }
    if matches!(cli.command, Some(Commands::Doctor)) {
        return commands::doctor::run();
    }

    let mut ctx = Context::load()?;

    match cli.command {
        None => tui::launch(ctx.config, ctx.store),
        Some(Commands::Capture {
            text,
            notebook,
            tags,
            folder,
            daily,
            json,
            check,
            undo,
        }) => {
            // Deliberately NOT resolved via `ctx.notebook_name` here, unlike
            // every other command — `commands::capture::run` needs to know
            // whether `-n` was actually given at all, since an explicit
            // override always wins over content-prefix routing, which in
            // turn wins over `default_notebook`. Collapsing that into an
            // already-resolved `String` up front would make it
            // indistinguishable from "the user typed `-n personal`".
            commands::capture::run(
                &ctx.store,
                &ctx.config,
                notebook,
                text,
                &tags,
                daily,
                json,
                check,
                undo,
                folder,
            )
        }
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
                &ctx.config,
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
                &ctx.config,
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
        Some(Commands::Query {
            dsl,
            saved,
            notebook,
            count,
            json,
        }) => {
            let dsl = match (dsl, saved) {
                (Some(d), None) => d,
                (None, Some(name)) => ctx
                    .config
                    .queries
                    .get(&name)
                    .cloned()
                    .with_context(|| format!("no saved query named '{name}'"))?,
                (None, None) => anyhow::bail!("provide a query string or --saved <name>"),
                (Some(_), Some(_)) => unreachable!("clap enforces --saved conflicts_with dsl"),
            };
            commands::query::run(&ctx.store, notebook.as_deref(), &dsl, json, count)
        }
        Some(Commands::Config) => commands::config::run(),
        Some(Commands::Doctor) => unreachable!("handled before Context::load() above"),
        Some(Commands::Notebook { action }) => match action {
            NotebookAction::Create { name } => commands::notebook::create(&ctx.store, &name),
            NotebookAction::List { json, all } => {
                commands::notebook::list(&ctx.store, &ctx.config, json, all)
            }
            NotebookAction::Rename { old, new } => {
                commands::notebook::rename(&ctx.store, &old, &new)
            }
            NotebookAction::Delete { name, yes } => {
                commands::notebook::delete(&ctx.store, &name, yes)
            }
            NotebookAction::Encrypt { name } => {
                commands::notebook::encrypt(&ctx.store, &mut ctx.config, &name)
            }
            NotebookAction::Decrypt { name } => {
                commands::notebook::decrypt(&ctx.store, &mut ctx.config, &name)
            }
            NotebookAction::Rekey { name } => {
                commands::notebook::rekey(&ctx.store, &mut ctx.config, &name)
            }
        },
        Some(Commands::Theme { action }) => match action {
            ThemeAction::List => commands::theme::list(&ctx.config),
            ThemeAction::Set { name, notebook } => {
                commands::theme::set(&mut ctx.config, &name, notebook.as_deref())
            }
            ThemeAction::Create { from } => {
                commands::theme::create(&mut ctx.config, from.as_deref())
            }
        },
        Some(Commands::Extension { .. }) => unreachable!("handled before Context::load"),
        Some(Commands::Import { action }) => match action {
            ImportAction::Obsidian {
                path,
                name,
                copy,
                tags,
                git_init,
            } => commands::import::obsidian(
                &mut ctx.config,
                &ctx.store,
                &path,
                name.as_deref(),
                copy,
                tags,
                git_init,
            ),
            ImportAction::Notion { path, name } => {
                commands::import::notion(&ctx.store, &path, name.as_deref())
            }
        },
    }
}

//! shiki-core: pure notebook/note/git/search/templates domain logic — see
//! `IDEA.md`/`CLAUDE.md` at the repo root for the full design spec.
//!
//! **Every public function/method in this crate is synchronous and
//! blocking — there is no async anywhere in `shiki-core`, on purpose (see
//! `shiki-tui`'s own doc comments for why the terminal crates stay
//! synchronous throughout).** It is safe to call any of them from any
//! thread — nothing here is reentrant-unsafe or relies on thread-local
//! state — so a consumer that itself runs on an async runtime (a future web
//! backend, for instance) should wrap each call in that runtime's blocking
//! adapter (e.g. Tokio's `spawn_blocking`) rather than expecting an
//! `async fn` version to show up here; `shiki-tui`'s own background git
//! operations (`App::spawn_git_op`, a plain `std::thread::spawn` + `mpsc`)
//! are the existing example of this same pattern, just without an async
//! runtime backing the call site.
//!
//! Portability seams for a future non-native consumer (no local git2, no
//! local disk) live behind small traits rather than a bespoke config
//! system: `vcs::VcsPort`/`fs::FileStore`, injected into
//! `notebook::NotebookStore`/`notebook::Notebook` via
//! `NotebookStore::new_with_backends`, and the process-capability traits in
//! `editor`/`browser`/`spell`/`voice`/`update`/`publish` (each with a
//! `Native*` default implementing it against the free functions those
//! modules already exposed). `git2`/`self_update`/`libc` are genuine
//! optional Cargo features (`git2-backend`/`self-update`/
//! `unix-process-check`, all default-on) — `cargo check -p shiki-core
//! --no-default-features` compiles clean. None of this has been exercised
//! against an actual non-native target yet (e.g. `wasm32-unknown-unknown`)
//! — `notebook::tests::notebook_store_works_entirely_through_injected_in_memory_backends`
//! is the closest thing to a real second backend today, and it's still an
//! in-process test double, not a different OS/target.

pub mod agent_connect;
pub mod attachments;
pub mod browser;
pub mod capture;
pub mod clock;
pub mod crypto;
pub mod daily;
pub mod editor;
pub mod export;
pub mod fs;
#[cfg(feature = "git2-backend")]
pub mod git;
pub mod headings;
pub mod last_capture;
pub mod markdown;
pub mod note;
pub mod notebook;
pub mod pagination;
pub mod process;
pub mod publish;
pub mod query;
pub mod reminders;
pub mod search;
pub mod spell;
pub mod tags;
pub mod tasks;
pub mod templates;
pub mod trash;
#[cfg(feature = "self-update")]
pub mod update;
pub mod vcs;
pub mod voice;
pub mod wikilinks;

pub use daily::daily_note_path;
pub use last_capture::LastCapture;
pub use note::{Frontmatter, Note};
pub use notebook::{Notebook, NotebookStore};
pub use search::SearchEngine;
pub use tags::TagIndex;
pub use templates::Template;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("toml error: {0}")]
    Toml(#[from] toml::ser::Error),
    /// Gated with `git.rs` behind the `git2-backend` feature — no consumer
    /// exhaustively `match`es this enum today (verified across `shiki-tui`/
    /// `shiki-desktop`/`shiki-cli`/`shiki-native-host`, all propagate via
    /// `?`/`.to_string()`/`Display`), so removing this variant under a
    /// non-default feature set is safe for every existing consumer.
    #[cfg(feature = "git2-backend")]
    #[error("git error: {0}")]
    Git(#[from] git2::Error),
    #[error("note not found: {0}")]
    NoteNotFound(String),
    /// A note lookup by slug/title (`notebook::find_note`) matched more
    /// than one note — e.g. two same-titled notes in different folders.
    #[error("{0}")]
    AmbiguousNote(String),
    #[error("notebook not found: {0}")]
    NotebookNotFound(String),
    #[error("notebook '{0}' already exists")]
    NotebookExists(String),
    #[error("template not found: {0}")]
    TemplateNotFound(String),
    #[error("invalid notebook name '{0}': must not be empty, '.', '..', or contain '/' or '\\\\'")]
    InvalidName(String),
    #[error("update error: {0}")]
    Update(String),
    #[error("publish error: {0}")]
    Publish(String),
    /// A failed/unavailable OS desktop-notification attempt
    /// (`reminders::send_notification`) — always non-fatal to the caller,
    /// which only ever logs this, never propagates it as a hard failure.
    #[error("notification error: {0}")]
    Notify(String),
    /// A move/copy target that already has something at that path — moves
    /// and copies error here rather than silently overwriting whatever's
    /// already there.
    #[error("already exists: {0}")]
    DestinationExists(String),
    /// A task toggle whose target line no longer exists in the file — the
    /// note changed on disk between building the task list and toggling.
    #[error("task not found in {0} — the note changed since the list was built")]
    TaskNotFound(String),
    /// A folder move/copy whose destination is the source itself, or nested
    /// inside it — copying a folder into its own subtree would otherwise
    /// recurse forever (the freshly created destination becomes one of the
    /// source's own children by the time the walk reaches it).
    #[error("cannot move/copy '{0}' into itself or one of its own subfolders")]
    DestinationInsideSource(String),
    /// Encryption/decryption failure — a wrong or missing passphrase for an
    /// encrypted notebook, or an `age` error setting up encryption.
    #[error("{0}")]
    Encryption(String),
    /// `tags::rename_tag`'s new name, empty — a bare rename-to-nothing
    /// would silently become a delete, which isn't what "rename" means;
    /// removing a tag entirely isn't exposed as a distinct operation yet.
    #[error("new tag name can't be empty")]
    EmptyTagName,
    /// A `spell::` failure — hunspell missing, or it errored out while
    /// checking/suggesting.
    #[error("spell check error: {0}")]
    Spell(String),
    /// A `voice::` failure — no recorder/whisper binary, a download
    /// failure, or whisper-cli errored while transcribing.
    #[error("voice capture error: {0}")]
    Voice(String),
    /// An `agent_connect::` failure — an existing client config file that
    /// doesn't parse as clean JSON (never partially merged into in that
    /// case, see `agent_connect`'s own doc comment), or an unsupported
    /// client/scope combination.
    #[error("{0}")]
    AgentConnect(String),
}

pub type Result<T> = std::result::Result<T, Error>;

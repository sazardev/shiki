pub mod browser;
pub mod daily;
pub mod editor;
pub mod git;
pub mod note;
pub mod notebook;
pub mod search;
pub mod tags;
pub mod tasks;
pub mod templates;
pub mod trash;
pub mod update;
pub mod wikilinks;

pub use daily::daily_note_path;
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
    #[error("git error: {0}")]
    Git(#[from] git2::Error),
    #[error("note not found: {0}")]
    NoteNotFound(String),
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
}

pub type Result<T> = std::result::Result<T, Error>;

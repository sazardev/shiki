//! One `#[derive(Deserialize, JsonSchema)]` struct per tool. Field doc
//! comments become each parameter's description in the tool's JSON
//! schema — the actual "bien filtradito" payoff of MCP over the CLI: the
//! AI sees a real, typed schema instead of having to construct and quote
//! a shell command by hand.

use std::collections::HashMap;

use rmcp::schemars;
use serde::Deserialize;

/// Pagination fields shared by every list-shaped tool — flattened into
/// each params struct below (`#[serde(flatten)]`) rather than nested, so
/// the tool's schema stays flat and obvious rather than
/// `{"pagination": {"limit": ...}}`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct Page {
    /// Max items to return. Defaults to 50 if omitted — a safe cap so a
    /// large notebook can't fill the whole context in one call. Ignored
    /// when `no_limit` is true.
    pub limit: Option<usize>,
    /// How many matching items to skip before the page starts. Use with
    /// `limit` to page through a large result set.
    #[serde(default)]
    pub offset: usize,
    /// Returns every matching item in one call, ignoring `limit` entirely.
    /// Only use this when you actually need everything — for a large
    /// notebook this can be a lot of text.
    #[serde(default)]
    pub no_limit: bool,
}

impl Page {
    pub fn effective_limit(&self) -> Option<usize> {
        shiki_core::pagination::effective_limit(self.limit, self.no_limit)
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListNotesParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    #[serde(flatten)]
    pub page: Page,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ShowNoteParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// The note's title or filename slug.
    pub note: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchNotesParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// Fuzzy match against note titles.
    pub query: String,
    #[serde(flatten)]
    pub page: Page,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct QueryNotesParams {
    /// Only this notebook. Omit to query across every notebook.
    pub notebook: Option<String>,
    /// A Dataview-style filter/sort expression over frontmatter, e.g.
    /// `where status = pending sort due asc`. Built-in fields: title,
    /// date, tags, notebook, path; any custom frontmatter field (set via
    /// the `set_field` tool) is also queryable by name.
    pub dsl: String,
    #[serde(flatten)]
    pub page: Page,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTasksParams {
    /// Only tasks from this notebook. Omit for every notebook.
    pub notebook: Option<String>,
    /// Only tasks due strictly before today.
    #[serde(default)]
    pub overdue: bool,
    /// Only tasks due exactly today.
    #[serde(default)]
    pub today: bool,
    /// Include already-completed tasks too (excluded by default).
    #[serde(default)]
    pub include_done: bool,
    #[serde(flatten)]
    pub page: Page,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IndexParams {
    /// Only this notebook. Omit for a structural overview of every
    /// notebook — note counts, busiest folders, busiest tags. Never
    /// includes note bodies or titles; call this first when exploring an
    /// unfamiliar shiki setup, before `list_notes`/`search_notes`.
    pub notebook: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListNotebooksParams {
    /// Also include notebooks that were untracked ("keep files, just
    /// forget about it") — excluded by default.
    #[serde(default)]
    pub include_hidden: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NewNoteParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    pub title: String,
    /// Initial body text. Omit for an empty note.
    pub body: Option<String>,
    /// Tags to set on the new note.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creates the note inside this subfolder of the notebook (e.g.
    /// `work/meetings`) instead of the notebook root. Auto-created if it
    /// doesn't exist yet.
    pub folder: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EditNoteParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// The note's title or filename slug.
    pub note: String,
    /// Replaces the note's whole body with this text. Give exactly one of
    /// `body`/`append`.
    pub body: Option<String>,
    /// Appends this text after the note's existing body instead of
    /// replacing it. Give exactly one of `body`/`append`.
    pub append: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteNoteParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// The note's title or filename slug.
    pub note: String,
    /// Must be explicitly `true` to actually delete — a safety gate
    /// against an accidental call. The note is trashed, not permanently
    /// erased (recoverable from the notebook's `trash/` directory).
    pub confirm: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameNoteParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// The note's current title or filename slug.
    pub note: String,
    pub new_title: String,
    /// Rewrites every inbound `[[wikilink]]` to this note so it still
    /// resolves under the new title. Defaults to true.
    pub rewrite_links: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MoveNoteParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// The note's title or filename slug.
    pub note: String,
    /// Destination as `notebook/path/within/it`, e.g. `work/meetings` or
    /// just `personal` for that notebook's root. The notebook segment
    /// must already exist; the rest is created as needed.
    pub destination: String,
    /// Copies instead of moving — the source note is left in place.
    #[serde(default)]
    pub copy: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TagNoteParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// The note's title or filename slug.
    pub note: String,
    /// Tags to add (already-present tags are left as-is, never
    /// duplicated).
    #[serde(default)]
    pub add: Vec<String>,
    /// Tags to remove.
    #[serde(default)]
    pub remove: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetFieldParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// The note's title or filename slug.
    pub note: String,
    /// Custom frontmatter fields to set, e.g. `{"priority": "3",
    /// "status": "pending"}`. Values are parsed as YAML scalars (`3` ->
    /// int, `true` -> bool, else string) so they keep their real type when
    /// read back via `query_notes`. The named fields title/date/tags/
    /// aliases/notebook/links/template can't be set this way — use
    /// `rename_note`/`tag_note` for those.
    #[serde(default)]
    pub set: HashMap<String, String>,
    /// Custom field names to remove.
    #[serde(default)]
    pub unset: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateFolderParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// Folder path within the notebook, e.g. `work/meetings/2026`. Any
    /// depth; parent folders are created as needed.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteFolderParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    pub path: String,
    /// Must be explicitly `true` — this recursively deletes the folder
    /// and everything inside it, with no trash/undo.
    pub confirm: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NotebookOnlyParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DiffParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// Diff just this note; omit for every pending change in the notebook.
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LogParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    /// Commit history for just this note; omit for the notebook-wide log.
    pub note: Option<String>,
    #[serde(flatten)]
    pub page: Page,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateNotebookParams {
    pub name: String,
    /// Optional git remote URL (or local bare-repo path) to configure as
    /// the new notebook's origin right away.
    pub remote: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RenameNotebookParams {
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteNotebookParams {
    pub name: String,
    /// Must be explicitly `true` — permanently deletes the notebook and
    /// every note in it, no trash/undo.
    pub confirm: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NotebookNameParams {
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MoveFolderParams {
    /// Notebook name. Omit to use the configured default notebook.
    pub notebook: Option<String>,
    pub path: String,
    /// Destination as `notebook/path/within/it`, same format as
    /// `move_note`.
    pub destination: String,
    /// Copies instead of moving.
    #[serde(default)]
    pub copy: bool,
}

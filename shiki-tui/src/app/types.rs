use crate::input::InputBox;

/// What a text-input popup is currently collecting a value for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingInput {
    NewNote,
    NewNotebook,
    /// Staged follow-up to `NewNotebook`'s plain-name path: one optional
    /// "sync this notebook to Git?" prompt, shown only when
    /// `git.remote_template` didn't already configure a remote (users who
    /// set up a template never get asked). The input takes the URL; the
    /// just-created notebook's *name* lives in
    /// `App.pending_new_notebook_remote`, same "one variant, the real
    /// state lives alongside it" shape as `RenameTag`.
    NewNotebookRemote,
    NewFolder,
    RenameNote,
    RenameNotebook,
    /// Renames (or merges into an existing tag) whichever tag is selected
    /// in the tags modal's level 1 — the old name lives in
    /// `App.pending_rename_tag`, same "one variant, the real state lives
    /// alongside it" shape as `NotebookPassphrase`/`PassphrasePurpose`.
    /// Operates across every notebook (`shiki_core::tags::rename_tag`),
    /// not just the current directory the tags modal itself browses —
    /// see that function's own doc comment for why.
    RenameTag,
    /// A name to save the query modal's current (already-valid) DSL text
    /// under (`Ctrl+S`, `App.pending_save_query_dsl` carries the DSL
    /// itself). Typing an existing saved query's name overwrites it,
    /// same "typing an existing tag's name merges" precedent `RenameTag`
    /// already set for "this name already means something, reuse it."
    SaveQuery,
    Search,
    SetRemote,
    /// Editing a notebook's remote from inside the Settings modal's
    /// NOTEBOOKS section (level 2) rather than the notebooks-panel `R`
    /// binding — which notebook is always `App.settings_notebook_drill`,
    /// not tracked here, so this stays a plain unit variant like the rest.
    SettingsNotebookRemote,
    /// Same idea, for that notebook's `auto_sync_every` override — the
    /// boolean overrides (`auto_push`/`auto_sync`) don't need a text prompt
    /// at all, they cycle in place on `Enter` (see
    /// `App::cycle_notebook_bool_override`).
    SettingsNotebookAutoSyncEvery,
    /// A GENERAL-tab text field (`default_notebook`/`editor`/`daily_template`)
    /// — which one is always `GeneralField::ALL[App.settings_selected]`, not
    /// tracked here, same reasoning as the `SettingsNotebook*` variants above.
    SettingsGeneralText,
    /// A GIT-tab (global `[git]` defaults) text/number field — which one is
    /// `GitField::ALL[App.settings_selected]`. The boolean fields
    /// (`auto_commit`/`auto_push`/`sign_commits`/`auto_sync`) don't need a
    /// prompt at all, they toggle in place on `Enter`.
    SettingsGitText,
    /// A brand-new snippet's trigger, typed via SNIPPETS level 1's `a` —
    /// the only `Settings*` prompt not tied to `settings_selected`/a drill
    /// field, since the snippet doesn't exist yet to index into.
    SettingsSnippetTrigger,
    /// The drilled-into snippet's `label` (SNIPPETS level 2) — which
    /// snippet is `App.settings_snippet_drill`. Its `body` isn't edited
    /// through a `PendingInput` prompt at all (see `App.editing_snippet`);
    /// a snippet body is arbitrary multi-line text, which a single-line
    /// prompt can't hold.
    SettingsSnippetLabel,
    /// Move or copy — one or more items (`App.pending_batch` always holds
    /// the actual list, even for a single item, so there's exactly one
    /// apply path regardless of how many things are selected). Which of
    /// the two this is comes from `pending_batch`'s `BatchOp`, not a
    /// separate variant here.
    MoveOrCopy,
    /// Export path for the selected notebook — `shiki_core::export`, shared
    /// with the `shiki export` CLI command. Format is inferred from the
    /// typed extension (`.md`/`.markdown` -> Markdown, anything else,
    /// including the prefilled `.html`, -> HTML) rather than a separate
    /// format-picker step, so one prompt is enough.
    ExportNotebook,
    /// Save path for the PDF publish (`shiki_core::publish`) — only ever
    /// opened when `[export].ask_export_path` is on; see
    /// `App::start_publish_path_prompt`.
    PublishPath,
    /// Generic single-line text field for the EXPORT tab in Settings
    /// (`export_dir`) — same shape as `SettingsGeneralText`/`SettingsGitText`:
    /// which field it's editing is recovered at confirm time via
    /// `ExportField::ALL[self.settings_selected]`, not carried on this
    /// variant.
    SettingsExportText,
    /// A notebook passphrase — masked input (`App::start_masked_input`).
    /// Which notebook, and what the passphrase is *for* (unlocking an
    /// already-encrypted one to read it, enabling encryption for the first
    /// time, confirming that same new passphrase, or disabling encryption),
    /// live in `App.passphrase_prompt_notebook`/`App.passphrase_purpose` —
    /// same "tracked separately, this stays a plain unit variant" reasoning
    /// as `SettingsNotebookRemote`.
    NotebookPassphrase,
    /// Any step of the metadata modal's prompts (tags, a new field's key,
    /// a new or existing field's value) — which step is `App.metadata_prompt`,
    /// same "one variant, the real state lives alongside it" shape as
    /// `NotebookPassphrase`/`PassphrasePurpose` above, since the metadata
    /// modal's own multi-step "key, then value" flow for a brand-new field
    /// needs exactly the same kind of chaining that one already does.
    Metadata,
}

/// Which step of the metadata modal's editing flow a `PendingInput::Metadata`
/// prompt answers — see `App::start_metadata_prompt`/`App::confirm_input`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MetadataPrompt {
    /// Comma-separated tags, prefilled with the note's current ones.
    Tags,
    /// A brand-new field's key — chains straight into `NewFieldValue` once
    /// confirmed, the same way `PassphrasePurpose::Enable` chains into
    /// `EnableConfirm`.
    NewFieldKey,
    /// The new field's value, once its key (carried here) is known.
    NewFieldValue(String),
    /// An existing field's value, prefilled with its current one — the
    /// field's key never changes, only editing what it's set to.
    FieldValue(String),
}

/// One entry in the query modal's suggestions dropdown — `display` is what
/// the list shows (a saved query's `★ name`, or, for a generated example,
/// just the DSL itself, same string as `dsl`), `dsl` is what actually gets
/// filled into the input box and run. Kept as two separate strings rather
/// than one, unlike the plain generated examples this replaced, specifically
/// so a saved query's *name* can be searched/displayed without that name
/// ever being mistaken for part of the query text that gets executed.
#[derive(Debug, Clone)]
pub(crate) struct QuerySuggestion {
    pub display: String,
    pub dsl: String,
    /// `Some(name)` for a saved query (its key in `Config.queries`) —
    /// `None` for a generated example, which can't be deleted from here
    /// since there's nothing saved to delete. `App::handle_query_key`'s
    /// `Ctrl+D` uses this to know whether the highlighted suggestion is
    /// even a valid delete target.
    pub saved_name: Option<String>,
}

impl QuerySuggestion {
    pub(crate) fn generated(dsl: String) -> Self {
        Self {
            display: dsl.clone(),
            dsl,
            saved_name: None,
        }
    }

    pub(crate) fn saved(name: &str, dsl: String) -> Self {
        Self {
            display: format!("{} {name}", crate::icons::STAR),
            dsl,
            saved_name: Some(name.to_string()),
        }
    }
}

/// What a `PendingInput::NotebookPassphrase` prompt's answer will be used
/// for — see `App::confirm_input`'s handling of that variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PassphrasePurpose {
    /// Just need the passphrase to decrypt an already-encrypted notebook
    /// for reading/writing this session — cached in `notebook_passphrases`
    /// once it proves correct.
    Unlock,
    /// Turning encryption on for a plaintext notebook — first of two
    /// entries (typo protection); the value is stashed in
    /// `App.passphrase_pending_first` and immediately followed by
    /// `EnableConfirm`.
    Enable,
    /// The second entry of `Enable` — must match `passphrase_pending_first`
    /// or the whole thing aborts with no config/notebook change made.
    EnableConfirm,
    /// Turning encryption off — decrypts every note back to plain text.
    Disable,
}

impl PendingInput {
    /// Falls back to this when `App.pending_input_title` is `None` — every
    /// variant except `MoveOrCopy`/the `Settings*` ones always has a plain
    /// static title; those depend on which notebook/snippet/field they're
    /// acting on (and, for `MoveOrCopy`, which op/items), so their title is
    /// always computed and set explicitly by whatever starts that
    /// particular input instead.
    pub(crate) fn title(self) -> &'static str {
        match self {
            PendingInput::NewNote => " New note (@ for quick date/template) ",
            PendingInput::NewNotebook => " New notebook ",
            PendingInput::NewNotebookRemote => " Git remote (URL or local path, empty = skip) ",
            PendingInput::NewFolder => " New folder ",
            PendingInput::RenameNote | PendingInput::RenameNotebook => " Rename ",
            PendingInput::RenameTag => " Rename/merge tag ",
            PendingInput::SaveQuery => " Save query as ",
            PendingInput::Search => " Jump to note ",
            PendingInput::SetRemote => " Git remote (URL or local path) ",
            PendingInput::SettingsNotebookRemote => " Git remote (URL or local path) ",
            PendingInput::SettingsNotebookAutoSyncEvery => " Auto-sync every N changes ",
            PendingInput::SettingsGeneralText
            | PendingInput::SettingsGitText
            | PendingInput::SettingsExportText => " Edit value ",
            PendingInput::SettingsSnippetTrigger => " New snippet trigger ",
            PendingInput::SettingsSnippetLabel => " Snippet label ",
            PendingInput::MoveOrCopy => " Move/copy to ",
            PendingInput::ExportNotebook => " Export path (.html or .md) ",
            PendingInput::PublishPath => " Save PDF as ",
            PendingInput::NotebookPassphrase => " Passphrase ",
            PendingInput::Metadata => " Metadata ",
        }
    }

    /// Small muted line rendered under the input box, `None` for every
    /// variant except the ones whose valid input isn't obvious from the
    /// title alone — e.g. `NewNotebook` silently accepts a git URL instead
    /// of a plain name (see `App::confirm_input`'s `looks_like_git_url`
    /// branch), which nothing else in the modal hints at.
    pub(crate) fn hint(self) -> Option<&'static str> {
        match self {
            PendingInput::NewNotebookRemote => Some(
                "Empty Enter skips — the notebook works locally either way. You can always \
                 add a remote later with R, or set git.remote_template to stop being asked.",
            ),
            PendingInput::NewNotebook => Some(
                "A name creates a new local notebook. Paste a repo URL (https://, git@, ssh://) \
                 to clone it instead — make sure you're logged in first if it's private. A path \
                 (/abs, ~/docs, ./relative) adopts that existing directory instead.",
            ),
            PendingInput::ExportNotebook => {
                Some("Format is inferred from the extension — .md/.markdown for Markdown, anything else for HTML.")
            }
            PendingInput::RenameTag => Some(
                "Applies to every note in every notebook that has this tag, not just here. \
                 Typing an existing tag's name merges into it instead of creating a duplicate.",
            ),
            PendingInput::SaveQuery => Some(
                "Saves to config.toml under [queries] — shows up as a ★ suggestion (in either \
                 query surface) from now on. An existing name overwrites that saved query.",
            ),
            _ => None,
        }
    }
}

/// A quick `@`-triggered shortcut typed into the `NewNote` title prompt —
/// `today`/`yesterday`/`tomorrow` resolve to a computed date (no template),
/// every other entry is a real `.md` file already sitting in the templates
/// dir (same discovery `open_template_picker` does). Picking one from the
/// dropdown finishes note creation immediately, skipping the normal
/// "type a title, Enter, then pick a template from `show_template_picker`"
/// two-step flow — that's the whole point of `@` being faster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum QuickCommand {
    Today,
    Yesterday,
    Tomorrow,
    Template(String),
}

impl QuickCommand {
    pub(crate) fn label(&self) -> String {
        match self {
            QuickCommand::Today => "today".to_string(),
            QuickCommand::Yesterday => "yesterday".to_string(),
            QuickCommand::Tomorrow => "tomorrow".to_string(),
            QuickCommand::Template(name) => name.clone(),
        }
    }

    /// `None` for `Template` — only the three date commands have one.
    pub(crate) fn date(&self) -> Option<chrono::NaiveDate> {
        let today = chrono::Local::now().date_naive();
        match self {
            QuickCommand::Today => Some(today),
            QuickCommand::Yesterday => today.pred_opt(),
            QuickCommand::Tomorrow => today.succ_opt(),
            QuickCommand::Template(_) => None,
        }
    }

    /// One line for the dropdown list — date commands preview the actual
    /// resolved date (matching the `%Y-%m-%d` convention used everywhere
    /// else dates are formatted, see `daily.rs`) so the user sees exactly
    /// what they'll get before picking it.
    pub(crate) fn display(&self) -> String {
        match self {
            QuickCommand::Template(name) => format!("{name}  (template)"),
            _ => format!(
                "{}  \u{2192} {}",
                self.label(),
                self.date()
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default()
            ),
        }
    }
}

/// One note or folder captured by absolute path at the moment a move/copy/
/// delete was initiated — capturing eagerly (rather than re-deriving from
/// `selected_note()`/`selected_folder()` at confirm time) means a
/// background sync's `reload_notes` completing while the prompt/confirm
/// dialog is open can't shift the underlying list out from under an
/// in-flight action.
#[derive(Debug, Clone)]
pub(crate) enum SelectedEntry {
    Note(std::path::PathBuf),
    Folder(std::path::PathBuf),
}

/// One item moved into the trash by a delete — enough to move it back to
/// exactly where it came from.
#[derive(Debug, Clone)]
pub(crate) struct TrashedEntry {
    pub(crate) notebook: String,
    pub(crate) original_path: std::path::PathBuf,
    pub(crate) trash_path: std::path::PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatchOp {
    Move,
    Copy,
}

/// A mouse gesture in progress (or just released) over PREVIEW's note
/// body — `Option<T>` shape, not a bare field, since it's transient state
/// that only exists between a `Down` and its matching `Up`, same convention
/// as `pending_batch`/`pending_delete`/`sync_in_flight` rather than
/// `visual_anchor`'s mode-scoped one (this isn't tied to `Mode::Visual`).
/// Both rows are document-row indices into `note_preview_lines()`, already
/// resolved via `panel_preview::preview_row_at` at hit-test time. Doubles as
/// the state for two distinct gestures: a plain click (`dragged` stays
/// `false`, `anchor_row == current_row` at release) enters `Mode::Edit` at
/// that row (see `App::enter_edit_at_preview_row`); an actual click-and-drag
/// (`dragged` set the moment any `Drag` event arrives) selects and copies
/// the spanned rows to the clipboard on release, same as before this
/// struct's `dragged` field existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PreviewSelection {
    pub(crate) anchor_row: usize,
    pub(crate) current_row: usize,
    pub(crate) dragged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeleteTarget {
    Note,
    Folder,
    Notebook,
}

/// Re-exported from `app::cache` so existing `use crate::app::NotePreviewCache`
/// paths keep working — the real definition now lives in `cache.rs` as a
/// named struct rather than an anonymous 7-tuple.
pub(crate) use crate::app::cache::NotePreviewCache;

/// Which of the find/replace bar's two fields is currently typed into —
/// `Tab` switches between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FindField {
    Query,
    Replace,
}

/// Ctrl+F's find/replace bar (`config.editor.find_replace`), open only
/// inside `Mode::Edit` — `None` means the bar is closed. Reuses `InputBox`
/// for both fields, same as the global-search bar (`global_search_input`),
/// rather than inventing new text-input handling.
pub(crate) struct EditorFindState {
    pub(crate) query: InputBox,
    pub(crate) replace: InputBox,
    pub(crate) focus: FindField,
    /// The cursor position when the bar was opened — where a fresh search
    /// (no existing selection yet, e.g. right after opening or right after
    /// closing) starts scanning from.
    pub(crate) anchor: (usize, usize),
}

/// The selected file's ours/theirs sides, drilled into from the conflict
/// resolver's flat file list — `ours`/`theirs` are each a unified diff
/// against the common ancestor (`shiki_core::git::conflict_diff`), so the
/// two panes read the same way the history modal's single diff pane
/// already does, just side by side instead of one after the other.
#[derive(Debug, Clone)]
pub(crate) struct ConflictView {
    pub(crate) file: std::path::PathBuf,
    pub(crate) ours: Vec<shiki_core::git::DiffLine>,
    pub(crate) theirs: Vec<shiki_core::git::DiffLine>,
    pub(crate) scroll: u16,
}

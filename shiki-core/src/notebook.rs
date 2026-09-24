use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[cfg(feature = "git2-backend")]
use crate::git;
use crate::note::{Frontmatter, Note};
use crate::{Error, Result};

/// Slug for a new note in `dir`, guaranteed to not collide with an existing
/// file there. Falls back to a timestamp-based slug when `title` slugifies
/// to an empty string (symbol/emoji-only titles), then appends `-2`, `-3`,
/// etc. if that slug (or the timestamp fallback) is already taken — two
/// titles that slugify the same, e.g. "Q3 Report" and "Q3, Report!", must
/// not silently overwrite each other.
fn unique_slug(dir: &Path, title: &str, fs: &dyn crate::fs::FileStore) -> String {
    let base = Note::slugify(title);
    let base = if base.is_empty() {
        format!("untitled-{}", chrono::Local::now().timestamp())
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut n = 2;
    while fs.exists(&dir.join(format!("{candidate}.md"))) {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    candidate
}

/// Same as `unique_slug`, but a collision with `ignore` (the note's own
/// current path, mid-rename) doesn't count — renaming a note to the title
/// it already effectively has shouldn't be blocked by itself. `ext` is the
/// note's own extension (`rename_note_at` preserves it rather than always
/// renaming onto `.md` — see there), so the collision check looks for the
/// same file type the rename is actually producing.
fn unique_slug_excluding(
    dir: &Path,
    title: &str,
    ignore: &Path,
    ext: &str,
    fs: &dyn crate::fs::FileStore,
) -> String {
    let base = Note::slugify(title);
    let base = if base.is_empty() {
        format!("untitled-{}", chrono::Local::now().timestamp())
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut n = 2;
    loop {
        let path = dir.join(format!("{candidate}.{ext}"));
        if !fs.exists(&path) || path == *ignore {
            return candidate;
        }
        candidate = format!("{base}-{n}");
        n += 1;
    }
}

/// Whether `dest` is `source` itself, or nested anywhere inside it —
/// checked via canonicalized path components (`Path::starts_with`), not a
/// naive string prefix, so e.g. `projects` vs `projects-archive` don't
/// false-positive. `dest` typically doesn't exist yet (the caller only
/// calls this once it's confirmed nothing already sits there), so this
/// walks up to the nearest existing ancestor to canonicalize, then
/// re-appends the not-yet-created suffix components before comparing.
fn is_same_or_nested(source: &Path, dest: &Path) -> bool {
    let source = source
        .canonicalize()
        .unwrap_or_else(|_| source.to_path_buf());
    let mut existing_ancestor = dest;
    let mut pending: Vec<&std::ffi::OsStr> = Vec::new();
    while !existing_ancestor.exists() {
        match (existing_ancestor.file_name(), existing_ancestor.parent()) {
            (Some(name), Some(parent)) => {
                pending.push(name);
                existing_ancestor = parent;
            }
            _ => break,
        }
    }
    let mut dest_resolved = existing_ancestor
        .canonicalize()
        .unwrap_or_else(|_| existing_ancestor.to_path_buf());
    for part in pending.into_iter().rev() {
        dest_resolved.push(part);
    }
    dest_resolved == source || dest_resolved.starts_with(&source)
}

/// File extensions shiki treats as a note when listing a notebook's
/// contents — `.md` (what shiki itself always creates), plus every other
/// extension that's really just "Markdown with optional YAML frontmatter"
/// under a different name, so a notebook pointed at an existing
/// non-shiki directory shows those files too instead of silently hiding
/// them: `.mdx` and `.txt` (an Obsidian vault commonly has both), `.qmd`
/// (Quarto, the scientific-publishing notebook format), `.rmd` (R
/// Markdown, Quarto's direct predecessor — same community, same shape),
/// and `.markdown` (the verbose spelling some static-site generators,
/// e.g. Jekyll, default to). None of these get any special
/// treatment beyond being recognized at all — they're all the exact same
/// shape `Note::from_file` already parses, no new parsing logic per
/// extension. The match in `list_dir` lowercases the file's actual
/// extension before comparing, so this list only needs the lowercase
/// spelling once: R Markdown's real-world convention is `.Rmd` (capital
/// R, lowercase `md`), not `.rmd` — without case-insensitive matching,
/// adding `"rmd"` here wouldn't actually recognize the files it's for.
/// This only affects *reading/listing* — new notes are always created as
/// `.md` (`create_note_in`); an existing non-`.md` file kept its own
/// extension (original case included) through rename/move/copy (see
/// `rename_note_at`), rather than being silently converted to `.md` the
/// first time it's touched from inside shiki.
///
/// This is the *built-in* list, fixed at compile time. A user can extend it
/// further, per notebook-store instance, via `Notebook::with_extra_extensions`/
/// `NotebookStore::with_extra_extensions` — e.g. to treat `.py`/`.org`/
/// arbitrary code or text files as notes too. That path is for genuinely
/// user-chosen, possibly-arbitrary extensions (configured in the Settings
/// modal), so it stays separate from this compile-time list rather than
/// growing this array itself.
const NOTE_EXTENSIONS: [&str; 6] = ["md", "mdx", "txt", "qmd", "rmd", "markdown"];

/// A notebook is a directory with its own git repo, containing notes with
/// one of `NOTE_EXTENSIONS`' extensions (in practice, almost always `.md`).
///
/// `crypto` is `None` on every `Notebook` `NotebookStore` itself returns —
/// `shiki-core` has no access to `shiki-config`'s `Config` (the dependency
/// chain is one-way: `shiki-core -> shiki-config -> shiki-tui -> shiki-cli`),
/// so it can't know on its own whether a notebook is configured as
/// encrypted, let alone hold the passphrase to prove it. `shiki-tui`/
/// `shiki-cli`, which see both crates, resolve that (`Config::encrypt_for`
/// plus whatever passphrase is cached/typed) and attach it via
/// `with_crypto` before using the notebook for any note I/O.
#[derive(Clone)]
pub struct Notebook {
    pub name: String,
    pub path: PathBuf,
    pub crypto: Option<crate::crypto::NotebookCrypto>,
    /// How this notebook's own methods (`list_dir`/`create_note_in`/
    /// `delete_note_at`/…) read/write plain files. Defaults to
    /// `fs::LocalFs` on every `Notebook` `Notebook::new` itself returns —
    /// callers that got theirs from a `NotebookStore` instead inherit
    /// whichever backend that store was built with (see
    /// `NotebookStore::fs`); nothing outside `shiki-core` needs to touch
    /// this field directly.
    fs: std::sync::Arc<dyn crate::fs::FileStore>,
    /// User-configured extensions (no leading dot, e.g. `"py"`) that
    /// `list_dir` also treats as a note, on top of the built-in
    /// `NOTE_EXTENSIONS`. Empty by default — `shiki-core` has no access to
    /// `shiki-config`, so it can't read `general.note_extra_extensions`
    /// itself; `shiki-tui`/`shiki-cli`/etc. resolve that and attach it via
    /// `with_extra_extensions`, same reasoning as `crypto` above.
    extra_extensions: Vec<String>,
}

// Same reasoning as `NotebookStore`'s manual `Debug` impl: `dyn FileStore`
// isn't `Debug` on its own.
impl std::fmt::Debug for Notebook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Notebook")
            .field("name", &self.name)
            .field("path", &self.path)
            .field("crypto", &self.crypto)
            .field("fs", &"<dyn FileStore>")
            .field("extra_extensions", &self.extra_extensions)
            .finish()
    }
}

impl Notebook {
    pub fn new(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            name: name.into(),
            path,
            crypto: None,
            fs: std::sync::Arc::new(crate::fs::LocalFs),
            extra_extensions: Vec::new(),
        }
    }

    /// Attaches (or clears, via `None`) the passphrase this notebook's
    /// note I/O should encrypt/decrypt with — see the struct's own doc
    /// comment for why this can't be resolved inside `shiki-core` itself.
    pub fn with_crypto(mut self, crypto: Option<crate::crypto::NotebookCrypto>) -> Self {
        self.crypto = crypto;
        self
    }

    /// Attaches a non-default `FileStore` — the seam a `NotebookStore`
    /// built with `new_with_backends` uses to propagate its own backend to
    /// every `Notebook` it hands out (`list`/`get`/`create`/`rename`), and
    /// that a caller constructing a `Notebook` directly (bypassing a store
    /// entirely) can use the same way.
    pub fn with_fs_backend(mut self, fs: std::sync::Arc<dyn crate::fs::FileStore>) -> Self {
        self.fs = fs;
        self
    }

    /// Attaches the user's own extra note extensions (`general.
    /// note_extra_extensions`, resolved by the caller — see the field's own
    /// doc comment for why `shiki-core` can't read it directly), the same
    /// propagation seam `with_fs_backend` already establishes. Values are
    /// matched case-insensitively and without a leading dot, same as the
    /// built-in `NOTE_EXTENSIONS`; a leading dot a user types by habit
    /// (`.py` vs `py`) is stripped here rather than silently never
    /// matching anything.
    pub fn with_extra_extensions(mut self, extra: Vec<String>) -> Self {
        self.extra_extensions = extra
            .into_iter()
            .map(|e| e.trim().trim_start_matches('.').to_string())
            .filter(|e| !e.is_empty())
            .collect();
        self
    }

    /// Lists the immediate contents of `relative` (a path within this
    /// notebook; `""` for the notebook root itself): subfolder names and
    /// notes, separately — a notebook can be nested arbitrarily deep, same
    /// as `nb`, and the caller (the Notes panel) walks one level at a time.
    /// Folders are sorted alphabetically; `.git` is never listed as a folder.
    ///
    /// A note file (any of `NOTE_EXTENSIONS`) that doesn't parse as a shiki
    /// note (no `---` frontmatter — common in an imported/pre-existing
    /// repo, one from `nb`, a plain `.txt`/`.mdx` file from an Obsidian
    /// vault, or a `.qmd`/`.rmd`/`.markdown` file from some other tool)
    /// still shows up: `Note::from_file` synthesizes metadata for those
    /// rather than failing, so nothing here needs to skip them.
    pub fn list_dir(&self, relative: &Path) -> Result<(Vec<String>, Vec<Note>)> {
        let dir = self.path.join(relative);
        if !self.fs.exists(&dir) {
            return Ok((Vec::new(), Vec::new()));
        }
        let mut entries: Vec<PathBuf> = self.fs.read_dir(&dir)?;
        entries.sort();

        let mut folders = Vec::new();
        let mut notes = Vec::new();
        for path in entries {
            if self.fs.is_dir(&path) {
                // Every dot-directory is invisible: `.git` is the original
                // case, but an adopted Obsidian vault also brings
                // `.obsidian/`, `.trash/`, `.smart-env/`… — none of those
                // are folders a user wants showing up (or being recursed
                // into) as notebook folders.
                if path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                {
                    continue;
                }
                if let Some(name) = path.file_name() {
                    folders.push(name.to_string_lossy().to_string());
                }
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    let lower = ext.to_ascii_lowercase();
                    NOTE_EXTENSIONS.contains(&lower.as_str())
                        || self
                            .extra_extensions
                            .iter()
                            .any(|e| e.eq_ignore_ascii_case(&lower))
                })
            {
                notes.push(Note::from_file_in_notebook_with_crypto_and_fs(
                    &path,
                    Some(&self.name),
                    self.crypto.as_ref(),
                    self.fs.as_ref(),
                )?);
            }
        }
        Ok((folders, notes))
    }

    /// Notes at the notebook's root only — the common case (CLI commands,
    /// the daily note check) that doesn't care about subfolders.
    pub fn list_notes(&self) -> Result<Vec<Note>> {
        Ok(self.list_dir(Path::new(""))?.1)
    }

    /// Every note in the notebook, at any folder depth — the pool for a
    /// global (cross-notebook) search, so nested notes are still findable.
    pub fn all_notes_recursive(&self) -> Result<Vec<Note>> {
        let mut out = Vec::new();
        self.collect_notes(Path::new(""), &mut out)?;
        Ok(out)
    }

    fn collect_notes(&self, relative: &Path, out: &mut Vec<Note>) -> Result<()> {
        let mut visited = HashSet::new();
        self.collect_notes_guarded(relative, out, &mut visited)
    }

    /// `visited` holds the canonicalized path of every directory already
    /// walked — a self-referential symlink inside a notebook would otherwise
    /// recurse forever and stack-overflow global search. A directory that
    /// can't be canonicalized (rare I/O race) is walked anyway rather than
    /// skipped, matching this function's existing "best effort" behavior.
    fn collect_notes_guarded(
        &self,
        relative: &Path,
        out: &mut Vec<Note>,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<()> {
        let dir = self.path.join(relative);
        if let Ok(real) = dir.canonicalize() {
            if !visited.insert(real) {
                return Ok(());
            }
        }
        let (folders, notes) = self.list_dir(relative)?;
        out.extend(notes);
        for folder in folders {
            self.collect_notes_guarded(&relative.join(folder), out, visited)?;
        }
        Ok(())
    }

    /// Creates a new note from a title and an initial body, in `relative`
    /// (a path within this notebook; `""` for the notebook root).
    pub fn create_note_in(
        &self,
        relative: &Path,
        title: &str,
        body: impl Into<String>,
    ) -> Result<Note> {
        let dir = self.path.join(relative);
        self.fs.create_dir_all(&dir)?;
        let slug = unique_slug(&dir, title, self.fs.as_ref());
        let path = dir.join(format!("{slug}.md"));
        let note = Note::new(path, Frontmatter::new(title, &self.name), body.into());
        note.save_with_crypto_and_fs(self.crypto.as_ref(), self.fs.as_ref())?;
        Ok(note)
    }

    pub fn create_note(&self, title: &str, body: impl Into<String>) -> Result<Note> {
        self.create_note_in(Path::new(""), title, body)
    }

    /// Creates an empty subfolder in `relative` (a path within this
    /// notebook; `""` for the notebook root) — same name validation as
    /// notebooks themselves (`validate_name`), since this becomes a path
    /// component the same way. Notes can already be created at any depth
    /// (`create_note_in` calls `create_dir_all` as a side effect), but
    /// there was previously no way to make an *empty* folder up front from
    /// the TUI — only folders that already existed on disk (e.g. from an
    /// imported repo) were navigable, not creatable.
    pub fn create_folder_in(&self, relative: &Path, name: &str) -> Result<PathBuf> {
        validate_name(name)?;
        let dir = self.path.join(relative).join(name);
        self.fs.create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn note_path(&self, slug: &str) -> PathBuf {
        self.path.join(format!("{slug}.md"))
    }

    /// Deletes the note at its actual path (wherever it lives — root or a
    /// nested folder), not a path reconstructed from a root-relative slug.
    pub fn delete_note_at(&self, path: &Path) -> Result<()> {
        if !self.fs.exists(path) {
            return Err(Error::NoteNotFound(path.display().to_string()));
        }
        self.fs.remove_file(path)?;
        Ok(())
    }

    /// Renames the note at `path`, keeping it in the same folder and the
    /// same file extension (original case included, e.g. a `.Rmd` file
    /// stays `.Rmd`, not `.rmd`) — a non-`.md` note (see `NOTE_EXTENSIONS`)
    /// renamed from inside shiki stays that same file type rather than
    /// being silently converted to `.md`, the one extension shiki itself
    /// ever creates new notes with.
    pub fn rename_note_at(&self, path: &Path, new_title: &str) -> Result<Note> {
        let mut note = Note::from_file_in_notebook_with_crypto_and_fs(
            path,
            Some(&self.name),
            self.crypto.as_ref(),
            self.fs.as_ref(),
        )?;
        let dir = path.parent().unwrap_or(&self.path);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("md");
        let slug = unique_slug_excluding(dir, new_title, path, ext, self.fs.as_ref());
        let new_path = dir.join(format!("{slug}.{ext}"));
        note.frontmatter.title = new_title.to_string();
        note.path = new_path;
        note.save_with_crypto_and_fs(self.crypto.as_ref(), self.fs.as_ref())?;
        if path != note.path {
            self.fs.remove_file(path)?;
        }
        Ok(note)
    }

    /// Deletes the folder at `relative` (within this notebook) and
    /// everything inside it — recursive, no confirmation of its own; the
    /// caller (`App`) gates this behind a confirm dialog, same as
    /// note/notebook delete.
    pub fn delete_folder_at(&self, relative: &Path) -> Result<()> {
        let dir = self.path.join(relative);
        if !self.fs.is_dir(&dir) {
            return Err(Error::NoteNotFound(dir.display().to_string()));
        }
        self.fs.remove_dir_all(&dir)?;
        Ok(())
    }

    /// Copies the note at `path` into `dest_notebook` at `dest_relative` (a
    /// directory within it — created if missing), preserving its filename.
    /// Rewrites `frontmatter.notebook` only when `dest_notebook` is
    /// actually a different notebook than `self` — a plain filesystem copy
    /// would otherwise leave a stale `notebook:` field in the copy's own
    /// YAML frontmatter. Errors rather than silently overwriting if a note
    /// already exists at the destination.
    pub fn copy_note_to(
        &self,
        path: &Path,
        dest_notebook: &Notebook,
        dest_relative: &Path,
    ) -> Result<Note> {
        // Decrypted with *this* notebook's key, re-encrypted (or left
        // plain) with the *destination*'s — the two can differ (copying
        // out of an encrypted notebook into a plain one, or vice versa),
        // and a note's ciphertext is only ever meaningful under the key of
        // whichever notebook it currently lives in.
        let mut copy = Note::from_file_in_notebook_with_crypto_and_fs(
            path,
            Some(&self.name),
            self.crypto.as_ref(),
            self.fs.as_ref(),
        )?;
        let dest_dir = dest_notebook.path.join(dest_relative);
        dest_notebook.fs.create_dir_all(&dest_dir)?;
        let file_name = path
            .file_name()
            .ok_or_else(|| Error::NoteNotFound(path.display().to_string()))?;
        let dest_path = dest_dir.join(file_name);
        if dest_notebook.fs.exists(&dest_path) {
            return Err(Error::DestinationExists(dest_path.display().to_string()));
        }
        copy.path = dest_path;
        if dest_notebook.name != self.name {
            copy.frontmatter.notebook = dest_notebook.name.clone();
        }
        copy.save_with_crypto_and_fs(dest_notebook.crypto.as_ref(), dest_notebook.fs.as_ref())?;
        Ok(copy)
    }

    /// Same as `copy_note_to`, then removes the original file — the actual
    /// "move," generalized from what was previously only reachable as
    /// "move to a different notebook's root" in the TUI.
    pub fn move_note_to(
        &self,
        path: &Path,
        dest_notebook: &Notebook,
        dest_relative: &Path,
    ) -> Result<Note> {
        let copy = self.copy_note_to(path, dest_notebook, dest_relative)?;
        self.fs.remove_file(path)?;
        Ok(copy)
    }

    /// Recursively copies the folder at `relative` (within this notebook —
    /// itself and everything inside it, at any depth) into `dest_notebook`
    /// at `dest_relative`, preserving the folder's own name. Every note
    /// inside gets the same cross-notebook frontmatter rewrite
    /// `copy_note_to` does for a single note — not just top-level ones —
    /// and empty subfolders are preserved too, not only ones that happen to
    /// contain a note (recurses via `list_dir`'s own folder list, not by
    /// walking notes and inferring folders from their paths). Errors if a
    /// folder already exists at the destination.
    pub fn copy_folder_to(
        &self,
        relative: &Path,
        dest_notebook: &Notebook,
        dest_relative: &Path,
    ) -> Result<()> {
        let source_dir = self.path.join(relative);
        let folder_name = source_dir
            .file_name()
            .ok_or_else(|| Error::NoteNotFound(source_dir.display().to_string()))?;
        let dest_relative = dest_relative.join(folder_name);
        let dest_dir = dest_notebook.path.join(&dest_relative);
        if dest_notebook.fs.exists(&dest_dir) {
            return Err(Error::DestinationExists(dest_dir.display().to_string()));
        }
        // A destination equal to (or nested inside) the source folder would
        // otherwise recurse forever: `dest_dir` gets created *before* this
        // walks the source's own children, so once the walk reaches that
        // freshly created directory it recurses into itself indefinitely —
        // reachable in practice through the `m` (move) prompt, which
        // prefills the item's current address; appending a segment to that
        // prefill targets a subpath of the source itself.
        if is_same_or_nested(&source_dir, &dest_dir) {
            return Err(Error::DestinationInsideSource(
                source_dir.display().to_string(),
            ));
        }
        dest_notebook.fs.create_dir_all(&dest_dir)?;
        let (folders, notes) = self.list_dir(relative)?;
        for note in &notes {
            self.copy_note_to(&note.path, dest_notebook, &dest_relative)?;
        }
        for folder in folders {
            self.copy_folder_to(&relative.join(&folder), dest_notebook, &dest_relative)?;
        }
        Ok(())
    }

    /// Same as `copy_folder_to`, then removes the original directory (and
    /// everything inside it) — the actual "move."
    pub fn move_folder_to(
        &self,
        relative: &Path,
        dest_notebook: &Notebook,
        dest_relative: &Path,
    ) -> Result<()> {
        self.copy_folder_to(relative, dest_notebook, dest_relative)?;
        self.fs.remove_dir_all(&self.path.join(relative))?;
        Ok(())
    }
}

/// Manages the collection of notebooks under the data directory (`~/.local/share/shiki/`).
///
/// Notebooks can live either under `root` (the default data directory) or at
/// custom absolute paths configured in `[notebooks.<name>] path = "..."`.
#[derive(Clone)]
pub struct NotebookStore {
    pub root: PathBuf,
    /// Custom absolute paths keyed by notebook name — these override the
    /// default `root/<name>` location for individual notebooks.
    pub custom_paths: HashMap<String, PathBuf>,
    /// How this store detects/initializes a notebook's git repo
    /// (`NotebookStore::create`/`list`). Defaults to `git::NativeVcs` —
    /// every existing caller (`new`/`new_with_custom_paths`) gets that
    /// default and never needs to know this field exists. A future
    /// non-native consumer supplies its own `VcsPort` impl via
    /// `new_with_vcs_backend`/`new_with_backends` instead, which is what
    /// lets this crate's `git2` dependency become truly optional (Cargo
    /// feature-gated) without breaking notebook creation/listing for
    /// callers that don't need that.
    vcs: std::sync::Arc<dyn crate::vcs::VcsPort>,
    /// How this store (and every `Notebook` it hands out — see
    /// `Notebook::fs`) reads/writes plain files. Defaults to
    /// `fs::LocalFs`, same "every existing caller is unaffected" shape as
    /// `vcs` above; a future non-native consumer (no local disk) supplies
    /// its own `FileStore` via `new_with_backends`.
    fs: std::sync::Arc<dyn crate::fs::FileStore>,
    /// User-configured extra note extensions (see `Notebook::
    /// with_extra_extensions`), propagated to every `Notebook` this store
    /// hands out. Empty by default, same shape as `custom_paths`: a plain
    /// `pub` field a caller can mutate directly in place (`shiki-tui`'s
    /// Settings modal does exactly this when the setting changes), not
    /// just a constructor parameter — so an edit takes effect immediately
    /// without needing to rebuild the whole store.
    pub extra_extensions: Vec<String>,
}

// `dyn VcsPort`/`dyn FileStore` don't implement `Debug` on their own (that
// would force every implementor, including any future non-native one, to
// derive it too) — this prints everything else and a fixed placeholder for
// each, same as `NotebookStore`'s old derived `Debug` showed every other
// field verbatim.
impl std::fmt::Debug for NotebookStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotebookStore")
            .field("root", &self.root)
            .field("custom_paths", &self.custom_paths)
            .field("vcs", &"<dyn VcsPort>")
            .field("fs", &"<dyn FileStore>")
            .field("extra_extensions", &self.extra_extensions)
            .finish()
    }
}

/// Rejects names that would escape `root` when joined as a path component —
/// empty, `.`/`..`, or containing a path separator. Notebook names come
/// straight from user input (the "new notebook" prompt), so without this a
/// name like `..` or `foo/bar` would silently create/delete outside the
/// intended data directory.
fn validate_name(name: &str) -> Result<()> {
    let invalid =
        name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\\');
    if invalid {
        return Err(Error::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Validates a `/`-separated relative path for use with `create_note_in`/
/// `create_folder_in` — e.g. `shiki capture --folder`. Each component is
/// checked with the same rule as a single notebook/folder name
/// (`validate_name`): non-empty, not `.`/`..`, no embedded separator.
/// Rejects the whole path on the first bad component rather than silently
/// dropping it, so a typo'd `--folder ../../etc` fails loudly instead of
/// writing somewhere unintended.
pub fn validate_relative_path(relative: &str) -> Result<PathBuf> {
    let mut path = PathBuf::new();
    for component in relative.split('/') {
        validate_name(component)?;
        path.push(component);
    }
    Ok(path)
}

/// If `text` looks like `"<name>: <rest>"` where `<name>` case-insensitively
/// matches one of `existing_notebooks`, returns that notebook's exact
/// stored name plus the remaining text (leading whitespace trimmed) — used
/// by `shiki capture` to route `"work: call Ana"` into the `work` notebook
/// automatically. Returns `None` for anything else (no colon, an empty
/// prefix, or a prefix that isn't a real notebook), meaning the caller
/// should fall back to whatever notebook it would otherwise use. Only ever
/// consulted when the caller has no *explicit* notebook override of its
/// own — an explicit `-n <notebook>` always wins without this being
/// consulted at all, so a real note whose text happens to start with
/// `"word: "` is never mis-routed as long as a target was actually given.
pub fn route_by_prefix<'a>(
    text: &'a str,
    existing_notebooks: &[String],
) -> Option<(String, &'a str)> {
    let (prefix, rest) = text.split_once(':')?;
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return None;
    }
    let matched = existing_notebooks
        .iter()
        .find(|n| n.eq_ignore_ascii_case(prefix))?;
    Some((matched.clone(), rest.trim_start()))
}

/// Splits `"notebook/path/within/it"` into an existing notebook plus a
/// destination-relative path inside it — the address format the TUI's `m`
/// (move/copy) prompt (`App::parse_move_target`, which now delegates here
/// instead of duplicating this logic) and the CLI's `move`/`folder move`
/// commands both use. The first segment must already name a real notebook
/// — never auto-created, since a notebook is a new git repo and creating
/// one from a typo would be surprising; everything after it is the
/// destination folder, not checked for existence up front since
/// `copy_note_to`/`copy_folder_to` create it as needed, same as
/// `create_note_in`/`create_folder_in` already do. Always splits on a
/// literal `/`, regardless of platform — callers must join addresses with
/// `/`, not `PathBuf`'s `Display` (which uses `\` on Windows).
pub fn parse_address(store: &NotebookStore, value: &str) -> Result<(Notebook, PathBuf)> {
    let mut parts = value.split('/').filter(|s| !s.is_empty());
    let notebook_name = parts
        .next()
        .ok_or_else(|| Error::NotebookNotFound("(empty target)".to_string()))?;
    let dest_notebook = store
        .get(notebook_name)
        .map_err(|_| Error::NotebookNotFound(notebook_name.to_string()))?;
    let rest: PathBuf = parts.collect();
    Ok((dest_notebook, rest))
}

/// Resolves a notebook by name with a clear "not found" message — shared
/// by every caller (CLI, MCP server) that needs to turn a bare name into a
/// real `Notebook` before doing anything else with it.
pub fn get_notebook(store: &NotebookStore, name: &str) -> Result<Notebook> {
    store
        .get(name)
        .map_err(|_| Error::NotebookNotFound(format!("{name} \u{2014} see the notebook list")))
}

/// Resolves a note by slug or by (case-insensitive) title match within an
/// already-resolved (and, if needed, already-decrypted) notebook —
/// searched recursively across every folder, so two notes with the same
/// title/slug in different folders both match `needle`. Errors with a
/// clear disambiguation message (listing each match's folder) rather than
/// silently returning whichever one the recursive walk happened to find
/// first. Takes `&Notebook` rather than `(store, name)` so every caller
/// resolves (and decrypts, if needed) the notebook exactly once instead of
/// this function doing a second, crypto-blind lookup of its own — a second
/// resolution is how an encrypted note's ciphertext used to silently get
/// parsed as a plain-body note instead of being decrypted or erroring.
pub fn find_note(nb: &Notebook, needle: &str) -> Result<Note> {
    let notes = nb.all_notes_recursive()?;
    let slug = Note::slugify(needle);
    let mut matches: Vec<Note> = notes
        .into_iter()
        .filter(|n| n.file_stem() == slug || n.frontmatter.title.eq_ignore_ascii_case(needle))
        .collect();
    match matches.len() {
        0 => Err(Error::NoteNotFound(format!("'{needle}' in '{}'", nb.name))),
        1 => Ok(matches.remove(0)),
        _ => {
            let mut locations: Vec<String> = matches
                .iter()
                .map(|n| {
                    n.path
                        .strip_prefix(&nb.path)
                        .unwrap_or(&n.path)
                        .display()
                        .to_string()
                })
                .collect();
            locations.sort();
            Err(Error::AmbiguousNote(format!(
                "'{needle}' matches {} notes in '{}' \u{2014} be more specific: {}",
                matches.len(),
                nb.name,
                locations.join(", ")
            )))
        }
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;

    #[test]
    fn validate_relative_path_accepts_multi_segment_paths() {
        assert_eq!(
            validate_relative_path("work/meetings").unwrap(),
            PathBuf::from("work").join("meetings")
        );
    }

    #[test]
    fn validate_relative_path_rejects_traversal_in_any_segment() {
        assert!(validate_relative_path("work/..").is_err());
        assert!(validate_relative_path("../etc").is_err());
        assert!(validate_relative_path("").is_err());
    }

    #[test]
    fn route_by_prefix_matches_case_insensitively_and_trims_rest() {
        let notebooks = vec!["Work".to_string(), "personal".to_string()];
        let (name, rest) = route_by_prefix("work:   call Ana", &notebooks).unwrap();
        assert_eq!(name, "Work");
        assert_eq!(rest, "call Ana");
    }

    #[test]
    fn route_by_prefix_returns_none_for_no_colon_or_unknown_prefix() {
        let notebooks = vec!["work".to_string()];
        assert!(route_by_prefix("just some text", &notebooks).is_none());
        assert!(route_by_prefix("unknown: text", &notebooks).is_none());
        assert!(route_by_prefix(": text", &notebooks).is_none());
    }

    #[test]
    fn parse_address_splits_notebook_from_the_rest() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().join("data-dir"));
        store.create("work").unwrap();

        let (nb, rest) = parse_address(&store, "work/meetings/2026").unwrap();
        assert_eq!(nb.name, "work");
        assert_eq!(rest, PathBuf::from("meetings").join("2026"));
    }

    #[test]
    fn parse_address_with_just_a_notebook_name_has_an_empty_rest() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().join("data-dir"));
        store.create("personal").unwrap();

        let (nb, rest) = parse_address(&store, "personal").unwrap();
        assert_eq!(nb.name, "personal");
        assert_eq!(rest, PathBuf::new());
    }

    #[test]
    fn parse_address_rejects_an_unknown_notebook_and_an_empty_target() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().join("data-dir"));

        assert!(parse_address(&store, "ghost/notes").is_err());
        assert!(parse_address(&store, "").is_err());
        assert!(parse_address(&store, "///").is_err());
    }

    #[test]
    fn get_notebook_finds_a_real_one_and_errors_clearly_on_a_missing_one() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().join("data-dir"));
        store.create("personal").unwrap();

        assert_eq!(get_notebook(&store, "personal").unwrap().name, "personal");
        assert!(get_notebook(&store, "ghost").is_err());
    }

    #[test]
    fn find_note_matches_by_slug_or_title_case_insensitively() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().join("data-dir"));
        let nb = store.create("personal").unwrap();
        nb.create_note("Grocery List", "milk").unwrap();

        assert_eq!(
            find_note(&nb, "grocery-list").unwrap().frontmatter.title,
            "Grocery List"
        );
        assert_eq!(
            find_note(&nb, "grocery list").unwrap().frontmatter.title,
            "Grocery List"
        );
    }

    #[test]
    fn find_note_errors_clearly_when_missing_or_ambiguous() {
        let tmp = tempfile::tempdir().unwrap();
        let store = NotebookStore::new(tmp.path().join("data-dir"));
        let nb = store.create("personal").unwrap();

        assert!(matches!(
            find_note(&nb, "nope").unwrap_err(),
            Error::NoteNotFound(_)
        ));

        nb.create_note_in(Path::new("a"), "Same Title", "").unwrap();
        nb.create_note_in(Path::new("b"), "Same Title", "").unwrap();
        assert!(matches!(
            find_note(&nb, "same title").unwrap_err(),
            Error::AmbiguousNote(_)
        ));
    }
}

impl NotebookStore {
    /// Requires the `git2-backend` feature (default-on — every existing
    /// consumer gets this unchanged) since it defaults `vcs` to
    /// `git::NativeVcs`. A consumer built without that feature (no local
    /// git2) uses `new_with_vcs_backend` directly instead, supplying its own
    /// `VcsPort`.
    #[cfg(feature = "git2-backend")]
    pub fn new(root: PathBuf) -> Self {
        Self::new_with_vcs_backend(root, HashMap::new(), std::sync::Arc::new(git::NativeVcs))
    }

    /// Same feature requirement as `new` above, and for the same reason.
    #[cfg(feature = "git2-backend")]
    pub fn new_with_custom_paths(root: PathBuf, custom_paths: HashMap<String, PathBuf>) -> Self {
        Self::new_with_vcs_backend(root, custom_paths, std::sync::Arc::new(git::NativeVcs))
    }

    /// Same as `new_with_custom_paths`, but with an explicit `VcsPort`
    /// instead of always defaulting to `git::NativeVcs` — the seam a future
    /// non-native consumer (no local git2, but still a local disk) uses
    /// instead of the two constructors above. Always available regardless
    /// of `git2-backend`.
    pub fn new_with_vcs_backend(
        root: PathBuf,
        custom_paths: HashMap<String, PathBuf>,
        vcs: std::sync::Arc<dyn crate::vcs::VcsPort>,
    ) -> Self {
        Self::new_with_backends(
            root,
            custom_paths,
            vcs,
            std::sync::Arc::new(crate::fs::LocalFs),
        )
    }

    /// Same as `new_with_vcs_backend`, with an explicit `FileStore` too
    /// instead of always defaulting to `fs::LocalFs` — the full seam a
    /// future non-native consumer (no local git2, no local disk) uses.
    pub fn new_with_backends(
        root: PathBuf,
        custom_paths: HashMap<String, PathBuf>,
        vcs: std::sync::Arc<dyn crate::vcs::VcsPort>,
        fs: std::sync::Arc<dyn crate::fs::FileStore>,
    ) -> Self {
        Self {
            root,
            custom_paths,
            vcs,
            fs,
            extra_extensions: Vec::new(),
        }
    }

    /// Returns the path for a notebook, checking custom paths first.
    fn path_for(&self, name: &str) -> Option<PathBuf> {
        self.custom_paths
            .get(name)
            .cloned()
            .or_else(|| Some(self.root.join(name)))
    }

    pub fn list(&self) -> Result<Vec<Notebook>> {
        let mut notebooks: Vec<Notebook> = Vec::new();
        // Collect names from custom paths to avoid duplicates
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        // 1. Custom-path notebooks
        for (name, path) in &self.custom_paths {
            if path.is_dir() {
                seen.insert(name.clone());
                notebooks.push(
                    Notebook::new(name.clone(), path.clone())
                        .with_fs_backend(self.fs.clone())
                        .with_extra_extensions(self.extra_extensions.clone()),
                );
            }
        }

        // 2. Subdirectories under the root data dir that are actually git repos.
        //
        // A notebook is *always* git-managed from creation (`NotebookStore::create`
        // calls `git::init_repo` immediately), so requiring a `.git` directory here
        // is a real, load-bearing distinction, not a heuristic: it's what actually
        // separates a notebook from an incidental sibling directory. This matters in
        // practice on macOS, where `directories::ProjectDirs` resolves `config_dir()`
        // and `data_dir()` to the exact same path — `default_templates_dir()` (a
        // plain, non-git `templates/` folder living in the config dir) then ends up
        // sitting directly inside the data dir too, and used to get listed as a
        // notebook purely as a side effect of that OS-specific path collision.
        if self.fs.exists(&self.root) {
            for path in self.fs.read_dir(&self.root)? {
                if !self.fs.is_dir(&path) || !self.vcs.is_repo(&path) {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                // Don't add a notebook twice if a custom path uses the same name
                if seen.insert(name.clone()) {
                    notebooks.push(
                        Notebook::new(name, path)
                            .with_fs_backend(self.fs.clone())
                            .with_extra_extensions(self.extra_extensions.clone()),
                    );
                }
            }
        }

        notebooks.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(notebooks)
    }

    /// Every note across every notebook (any folder depth), paired with the
    /// notebook it lives in — the pool for a global (cross-notebook) search.
    pub fn all_notes(&self) -> Result<Vec<(Notebook, Note)>> {
        let mut all = Vec::new();
        for notebook in self.list()? {
            for note in notebook.all_notes_recursive()? {
                all.push((notebook.clone(), note));
            }
        }
        Ok(all)
    }

    pub fn get(&self, name: &str) -> Result<Notebook> {
        validate_name(name)?;
        let path = self
            .path_for(name)
            .ok_or_else(|| Error::NotebookNotFound(name.to_string()))?;
        if !self.fs.is_dir(&path) {
            return Err(Error::NotebookNotFound(name.to_string()));
        }
        Ok(Notebook::new(name, path)
            .with_fs_backend(self.fs.clone())
            .with_extra_extensions(self.extra_extensions.clone()))
    }

    /// Creates a new notebook with its own git repo.
    /// If a custom path is configured for this notebook name, creates it
    /// there; otherwise creates it under the default data directory.
    pub fn create(&self, name: &str) -> Result<Notebook> {
        validate_name(name)?;
        let path = self
            .path_for(name)
            .ok_or_else(|| Error::NotebookNotFound(name.to_string()))?;
        if self.fs.exists(&path) {
            return Err(Error::NotebookExists(name.to_string()));
        }
        self.fs.create_dir_all(&path)?;
        self.vcs.init_repo(&path)?;
        Ok(Notebook::new(name, path)
            .with_fs_backend(self.fs.clone())
            .with_extra_extensions(self.extra_extensions.clone()))
    }

    pub fn rename(&self, old_name: &str, new_name: &str) -> Result<Notebook> {
        validate_name(old_name)?;
        validate_name(new_name)?;
        let old_path = self
            .path_for(old_name)
            .ok_or_else(|| Error::NotebookNotFound(old_name.to_string()))?;
        if !self.fs.is_dir(&old_path) {
            return Err(Error::NotebookNotFound(old_name.to_string()));
        }
        // `new_name` might have its own configured custom path — honor that
        // first. Otherwise, if `old_name` itself lived at a custom path,
        // rename it in place (as a sibling of `old_path`) rather than
        // defaulting to `root.join(new_name)`, which would silently move a
        // notebook out of wherever it actually lived (e.g. an Obsidian
        // vault) and into shiki's own data directory.
        let new_path = match self.custom_paths.get(new_name) {
            Some(configured) => configured.clone(),
            None if self.custom_paths.contains_key(old_name) => old_path
                .parent()
                .map(|parent| parent.join(new_name))
                .unwrap_or_else(|| self.root.join(new_name)),
            None => self.root.join(new_name),
        };
        if self.fs.exists(&new_path) {
            return Err(Error::NotebookExists(new_name.to_string()));
        }
        self.fs.rename(&old_path, &new_path)?;
        Ok(Notebook::new(new_name, new_path)
            .with_fs_backend(self.fs.clone())
            .with_extra_extensions(self.extra_extensions.clone()))
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        validate_name(name)?;
        let path = self
            .path_for(name)
            .ok_or_else(|| Error::NotebookNotFound(name.to_string()))?;
        if !self.fs.is_dir(&path) {
            return Err(Error::NotebookNotFound(name.to_string()));
        }
        self.fs.remove_dir_all(&path)?;
        Ok(())
    }
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain `Notebook` at a fresh directory under a shared tempdir — no
    /// git init, since none of these operations (copy/move/delete) touch
    /// git at all, only the filesystem.
    fn test_notebook(root: &Path, name: &str) -> Notebook {
        let path = root.join(name);
        std::fs::create_dir_all(&path).unwrap();
        Notebook::new(name, path)
    }

    #[test]
    fn move_note_to_rewrites_frontmatter_across_notebooks_and_removes_source() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        let b = test_notebook(tmp.path(), "b");
        let note = a.create_note("Grocery list", "milk, eggs").unwrap();

        let moved = a.move_note_to(&note.path, &b, Path::new("")).unwrap();

        assert_eq!(moved.frontmatter.notebook, "b");
        assert!(
            !note.path.exists(),
            "source note should be gone after a move"
        );
        assert!(moved.path.exists());
        assert_eq!(Note::from_file(&moved.path).unwrap().body, "milk, eggs");
    }

    #[test]
    fn copy_note_to_same_notebook_keeps_frontmatter_and_leaves_source() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        a.create_folder_in(Path::new(""), "archive").unwrap();
        let note = a.create_note("Idea", "body").unwrap();

        let copy = a
            .copy_note_to(&note.path, &a, Path::new("archive"))
            .unwrap();

        assert_eq!(copy.frontmatter.notebook, "a");
        assert!(note.path.exists(), "copy must not remove the source");
        assert!(copy.path.exists());
    }

    #[test]
    fn copy_note_to_errors_when_destination_already_has_that_file() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        let b = test_notebook(tmp.path(), "b");
        let note = a.create_note("Dup", "one").unwrap();
        // Something already sitting at the destination filename in b.
        b.create_note_in(Path::new(""), "Dup", "two").unwrap();

        let result = a.copy_note_to(&note.path, &b, Path::new(""));
        assert!(matches!(result, Err(Error::DestinationExists(_))));
    }

    #[test]
    fn list_dir_includes_txt_mdx_qmd_rmd_and_markdown_files_alongside_md() {
        let tmp = tempfile::tempdir().unwrap();
        let nb = test_notebook(tmp.path(), "vault");
        nb.create_note("Shiki note", "body").unwrap();
        std::fs::write(nb.path.join("plain.txt"), "just text").unwrap();
        std::fs::write(nb.path.join("obsidian.mdx"), "# mdx content").unwrap();
        std::fs::write(nb.path.join("analysis.qmd"), "# quarto content").unwrap();
        // Real-world R Markdown convention is capital-R `.Rmd`, not `.rmd` —
        // this exercises the case-insensitive extension match, not just the
        // lowercase spelling already in `NOTE_EXTENSIONS`.
        std::fs::write(nb.path.join("report.Rmd"), "# r markdown content").unwrap();
        std::fs::write(nb.path.join("post.markdown"), "# jekyll post").unwrap();
        std::fs::write(nb.path.join("ignored.png"), []).unwrap();

        let (_, notes) = nb.list_dir(Path::new("")).unwrap();
        let stems: Vec<String> = notes.iter().map(|n| n.file_stem()).collect();

        assert!(stems.contains(&"shiki-note".to_string()));
        assert!(stems.contains(&"plain".to_string()));
        assert!(stems.contains(&"obsidian".to_string()));
        assert!(stems.contains(&"analysis".to_string()));
        assert!(stems.contains(&"report".to_string()));
        assert!(stems.contains(&"post".to_string()));
        assert_eq!(
            notes.len(),
            6,
            "non-note extensions must be excluded: {stems:?}"
        );
    }

    #[test]
    fn list_dir_ignores_arbitrary_extensions_without_opt_in() {
        let tmp = tempfile::tempdir().unwrap();
        let nb = test_notebook(tmp.path(), "vault");
        std::fs::write(nb.path.join("script.py"), "print('hi')").unwrap();

        let (_, notes) = nb.list_dir(Path::new("")).unwrap();

        assert!(
            notes.is_empty(),
            ".py must not show up by default: {notes:?}"
        );
    }

    #[test]
    fn with_extra_extensions_makes_list_dir_pick_up_user_configured_extensions() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("vault");
        std::fs::create_dir_all(&path).unwrap();
        let nb = Notebook::new("vault", path.clone())
            .with_extra_extensions(vec!["py".to_string(), "org".to_string()]);
        std::fs::write(path.join("script.py"), "print('hi')").unwrap();
        std::fs::write(path.join("notes.org"), "* heading").unwrap();
        std::fs::write(path.join("ignored.png"), []).unwrap();

        let (_, notes) = nb.list_dir(Path::new("")).unwrap();
        let stems: Vec<String> = notes.iter().map(|n| n.file_stem()).collect();

        assert!(stems.contains(&"script".to_string()));
        assert!(stems.contains(&"notes".to_string()));
        assert_eq!(notes.len(), 2, "only opted-in extensions: {stems:?}");
    }

    #[test]
    fn with_extra_extensions_matches_case_insensitively_and_strips_a_leading_dot() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("vault");
        std::fs::create_dir_all(&path).unwrap();
        // A user might type ".PY" or "PY" or "py" in the settings prompt —
        // all should behave identically.
        let nb = Notebook::new("vault", path.clone())
            .with_extra_extensions(vec![".PY".to_string(), "  ".to_string()]);
        std::fs::write(path.join("script.py"), "print('hi')").unwrap();

        let (_, notes) = nb.list_dir(Path::new("")).unwrap();

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].file_stem(), "script");
    }

    #[test]
    fn notebook_store_propagates_extra_extensions_to_every_notebook_it_hands_out() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store =
            NotebookStore::new_with_custom_paths(tmp.path().to_path_buf(), HashMap::new());
        store.extra_extensions = vec!["py".to_string()];
        let created = store.create("code").unwrap();
        std::fs::write(created.path.join("script.py"), "print('hi')").unwrap();

        // Through `create`'s return value...
        let (_, notes) = created.list_dir(Path::new("")).unwrap();
        assert_eq!(notes.len(), 1);

        // ...and independently through `get`/`list`, which mint a fresh
        // `Notebook` from the store rather than reusing `created`.
        let fetched = store.get("code").unwrap();
        let (_, notes) = fetched.list_dir(Path::new("")).unwrap();
        assert_eq!(notes.len(), 1);

        let listed = store.list().unwrap();
        let code = listed.iter().find(|n| n.name == "code").unwrap();
        let (_, notes) = code.list_dir(Path::new("")).unwrap();
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn list_dir_skips_every_dot_directory_not_just_git() {
        // An adopted Obsidian vault carries `.obsidian/` (settings),
        // `.trash/` (its own soft deletes), `.smart-env/` — none of those
        // are notebook folders and none of their contents are notes.
        let tmp = tempfile::tempdir().unwrap();
        let nb = test_notebook(tmp.path(), "vault");
        nb.create_note("Real note", "body").unwrap();
        for dot in [".git", ".obsidian", ".trash", ".smart-env"] {
            let dir = nb.path.join(dot);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("hidden-note.md"), "# hidden").unwrap();
        }

        let (folders, notes) = nb.list_dir(Path::new("")).unwrap();

        assert!(folders.is_empty(), "dot-dirs must not list: {folders:?}");
        assert_eq!(notes.len(), 1, "only the real note shows up");
        assert_eq!(notes[0].frontmatter.title, "Real note");
        // And the recursive walk doesn't descend into them either.
        assert_eq!(nb.all_notes_recursive().unwrap().len(), 1);
    }

    #[test]
    fn rename_note_at_preserves_a_non_md_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let nb = test_notebook(tmp.path(), "vault");
        let path = nb.path.join("old-name.txt");
        std::fs::write(&path, "content").unwrap();

        let renamed = nb.rename_note_at(&path, "New Name").unwrap();

        assert_eq!(renamed.path.extension().unwrap(), "txt");
        assert!(!path.exists());
        assert!(renamed.path.exists());
    }

    #[test]
    fn rename_note_at_preserves_a_qmd_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let nb = test_notebook(tmp.path(), "vault");
        let path = nb.path.join("old-analysis.qmd");
        std::fs::write(&path, "---\ntitle: Old\n---\ncontent").unwrap();

        let renamed = nb.rename_note_at(&path, "New Analysis").unwrap();

        assert_eq!(renamed.path.extension().unwrap(), "qmd");
        assert!(!path.exists());
        assert!(renamed.path.exists());
    }

    #[test]
    fn rename_note_at_preserves_the_original_case_of_a_capital_r_rmd_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let nb = test_notebook(tmp.path(), "vault");
        let path = nb.path.join("old-report.Rmd");
        std::fs::write(&path, "---\ntitle: Old\n---\ncontent").unwrap();

        let renamed = nb.rename_note_at(&path, "New Report").unwrap();

        assert_eq!(
            renamed.path.extension().unwrap(),
            "Rmd",
            "must stay .Rmd, not be lowercased to .rmd"
        );
        assert!(!path.exists());
        assert!(renamed.path.exists());
    }

    #[test]
    fn copy_folder_to_preserves_nested_structure_and_rewrites_every_note() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        let b = test_notebook(tmp.path(), "b");
        a.create_note_in(Path::new("projects/web"), "Todo", "ship it")
            .unwrap();
        a.create_folder_in(Path::new("projects"), "empty-subfolder")
            .unwrap();

        a.copy_folder_to(Path::new("projects"), &b, Path::new(""))
            .unwrap();

        let nested = Note::from_file(&b.path.join("projects/web/todo.md")).unwrap();
        assert_eq!(nested.frontmatter.notebook, "b");
        assert_eq!(nested.body, "ship it");
        assert!(b.path.join("projects/empty-subfolder").is_dir());
        // Source is untouched by a copy.
        assert!(a.path.join("projects/web/todo.md").exists());
    }

    #[test]
    fn move_folder_to_removes_the_source_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        let b = test_notebook(tmp.path(), "b");
        a.create_note_in(Path::new("projects"), "Todo", "x")
            .unwrap();

        a.move_folder_to(Path::new("projects"), &b, Path::new(""))
            .unwrap();

        assert!(!a.path.join("projects").exists());
        assert!(b.path.join("projects/todo.md").exists());
    }

    #[test]
    fn copy_folder_to_errors_when_destination_folder_already_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        let b = test_notebook(tmp.path(), "b");
        a.create_folder_in(Path::new(""), "projects").unwrap();
        b.create_folder_in(Path::new(""), "projects").unwrap();

        let result = a.copy_folder_to(Path::new("projects"), &b, Path::new(""));
        assert!(matches!(result, Err(Error::DestinationExists(_))));
    }

    #[test]
    fn copy_folder_to_rejects_a_destination_nested_inside_the_source() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        a.create_folder_in(Path::new(""), "projects").unwrap();

        // Same notebook, destination is a subpath of the source itself —
        // this used to create `projects/projects/projects/...` forever.
        let result = a.copy_folder_to(Path::new("projects"), &a, Path::new("projects/nested"));

        assert!(matches!(result, Err(Error::DestinationInsideSource(_))));
    }

    #[test]
    fn copy_folder_to_rejects_copying_a_folder_onto_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        a.create_folder_in(Path::new(""), "projects").unwrap();

        let result = a.copy_folder_to(Path::new("projects"), &a, Path::new(""));

        assert!(matches!(
            result,
            Err(Error::DestinationExists(_)) | Err(Error::DestinationInsideSource(_))
        ));
    }

    #[test]
    fn delete_folder_at_removes_the_directory_and_its_contents() {
        let tmp = tempfile::tempdir().unwrap();
        let a = test_notebook(tmp.path(), "a");
        a.create_note_in(Path::new("scratch"), "Temp", "x").unwrap();

        a.delete_folder_at(Path::new("scratch")).unwrap();

        assert!(!a.path.join("scratch").exists());
    }

    #[test]
    fn rename_a_custom_path_notebook_stays_in_place_instead_of_moving_into_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data-dir");
        let vault = tmp.path().join("vault");
        let work = vault.join("work");
        std::fs::create_dir_all(&work).unwrap();

        let mut custom_paths = HashMap::new();
        custom_paths.insert("work".to_string(), work.clone());
        let store = NotebookStore::new_with_custom_paths(root.clone(), custom_paths);

        let renamed = store.rename("work", "work2").unwrap();

        assert_eq!(renamed.path, vault.join("work2"));
        assert!(
            vault.join("work2").is_dir(),
            "renamed notebook should stay next to where it lived, not move into root"
        );
        assert!(!work.exists(), "old directory should be gone after rename");
        assert!(
            !root.join("work2").exists(),
            "rename must not relocate a custom-path notebook into the default data dir"
        );
    }

    #[test]
    fn rename_a_custom_path_notebook_honors_the_new_names_own_custom_path() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data-dir");
        let old_location = tmp.path().join("old-vault").join("work");
        let new_location = tmp.path().join("new-vault").join("work2");
        std::fs::create_dir_all(&old_location).unwrap();
        std::fs::create_dir_all(new_location.parent().unwrap()).unwrap();

        let mut custom_paths = HashMap::new();
        custom_paths.insert("work".to_string(), old_location.clone());
        custom_paths.insert("work2".to_string(), new_location.clone());
        let store = NotebookStore::new_with_custom_paths(root, custom_paths);

        let renamed = store.rename("work", "work2").unwrap();

        assert_eq!(renamed.path, new_location);
        assert!(new_location.is_dir());
        assert!(!old_location.exists());
    }

    #[test]
    fn rename_a_root_notebook_still_lands_under_root_as_before() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data-dir");
        let store = NotebookStore::new(root.clone());
        store.create("work").unwrap();

        let renamed = store.rename("work", "work2").unwrap();

        assert_eq!(renamed.path, root.join("work2"));
        assert!(root.join("work2").is_dir());
    }

    #[test]
    fn list_only_picks_up_real_notebooks_not_plain_sibling_directories() {
        // Reproduces the macOS bug from issue #43: `directories::ProjectDirs`
        // resolves `config_dir()` and `data_dir()` to the same path there, so
        // the (plain, non-git) templates directory ends up sitting directly
        // inside the data dir alongside real notebooks.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data-dir");
        let store = NotebookStore::new(root.clone());
        store.create("personal").unwrap();
        std::fs::create_dir_all(root.join("templates")).unwrap();

        let names: Vec<String> = store.list().unwrap().into_iter().map(|n| n.name).collect();

        assert_eq!(names, vec!["personal".to_string()]);
    }

    #[test]
    fn list_ignores_a_directory_whose_git_repo_was_never_initialized() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data-dir");
        std::fs::create_dir_all(root.join("not-a-notebook")).unwrap();
        let store = NotebookStore::new(root);

        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn an_encrypted_notebooks_notes_round_trip_through_list_dir_and_create() {
        let tmp = tempfile::tempdir().unwrap();
        let crypto = crate::crypto::NotebookCrypto::new("correct horse battery staple");
        let nb = test_notebook(tmp.path(), "vault").with_crypto(Some(crypto.clone()));

        nb.create_note_in(Path::new(""), "Secret Plans", "top secret content")
            .unwrap();

        // On disk, the file is ciphertext.
        let raw = std::fs::read_to_string(nb.path.join("secret-plans.md")).unwrap();
        assert!(crate::crypto::looks_encrypted(&raw));

        // list_dir, given the same crypto, reads it back as a normal note.
        let (_, notes) = nb.list_dir(Path::new("")).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].frontmatter.title, "Secret Plans");
        assert_eq!(notes[0].body, "top secret content");

        // The same notebook with no crypto attached can't read it.
        let locked = Notebook::new("vault", nb.path.clone());
        assert!(locked.list_dir(Path::new("")).is_err());
    }

    /// A tiny in-memory `FileStore` — exists purely to prove the injected-
    /// backend design actually works end-to-end for a non-native consumer,
    /// not as a real general-purpose virtual filesystem (no permissions, no
    /// symlinks, nothing beyond a plain mutex for concurrent access).
    struct MemFs {
        files: std::sync::Mutex<HashMap<PathBuf, Vec<u8>>>,
        dirs: std::sync::Mutex<HashSet<PathBuf>>,
    }

    impl MemFs {
        fn new() -> Self {
            Self {
                files: std::sync::Mutex::new(HashMap::new()),
                dirs: std::sync::Mutex::new(HashSet::new()),
            }
        }
    }

    impl crate::fs::FileStore for MemFs {
        fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "not found"))
        }

        fn write(&self, path: &Path, contents: &[u8]) -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                self.create_dir_all(parent)?;
            }
            self.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), contents.to_vec());
            Ok(())
        }

        fn read_dir(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
            let mut out: HashSet<PathBuf> = HashSet::new();
            for file in self.files.lock().unwrap().keys() {
                if file.parent() == Some(path) {
                    out.insert(file.clone());
                }
            }
            for dir in self.dirs.lock().unwrap().iter() {
                if dir.parent() == Some(path) {
                    out.insert(dir.clone());
                }
            }
            Ok(out.into_iter().collect())
        }

        fn create_dir_all(&self, path: &Path) -> std::io::Result<()> {
            let mut dirs = self.dirs.lock().unwrap();
            let mut current = PathBuf::new();
            for component in path.components() {
                current.push(component);
                dirs.insert(current.clone());
            }
            Ok(())
        }

        fn remove_file(&self, path: &Path) -> std::io::Result<()> {
            self.files.lock().unwrap().remove(path);
            Ok(())
        }

        fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
            self.files
                .lock()
                .unwrap()
                .retain(|p, _| !p.starts_with(path));
            self.dirs.lock().unwrap().retain(|p| !p.starts_with(path));
            Ok(())
        }

        fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
            let mut files = self.files.lock().unwrap();
            let moved: Vec<(PathBuf, Vec<u8>)> = files
                .iter()
                .filter(|(p, _)| p.starts_with(from))
                .map(|(p, v)| (to.join(p.strip_prefix(from).unwrap()), v.clone()))
                .collect();
            files.retain(|p, _| !p.starts_with(from));
            for (p, v) in moved {
                files.insert(p, v);
            }
            drop(files);
            let mut dirs = self.dirs.lock().unwrap();
            if dirs.remove(from) {
                dirs.insert(to.to_path_buf());
            }
            Ok(())
        }

        fn exists(&self, path: &Path) -> bool {
            self.files.lock().unwrap().contains_key(path)
                || self.dirs.lock().unwrap().contains(path)
        }

        fn is_dir(&self, path: &Path) -> bool {
            self.dirs.lock().unwrap().contains(path)
        }

        fn modified(&self, _path: &Path) -> std::io::Result<std::time::SystemTime> {
            Ok(std::time::SystemTime::now())
        }
    }

    /// A tiny in-memory `VcsPort` — just tracks which paths were
    /// "initialized" as a repo, no real git semantics whatsoever.
    struct MemVcs {
        inited: std::sync::Mutex<HashSet<PathBuf>>,
    }

    impl MemVcs {
        fn new() -> Self {
            Self {
                inited: std::sync::Mutex::new(HashSet::new()),
            }
        }
    }

    impl crate::vcs::VcsPort for MemVcs {
        fn init_repo(&self, path: &Path) -> Result<()> {
            self.inited.lock().unwrap().insert(path.to_path_buf());
            Ok(())
        }

        fn is_repo(&self, path: &Path) -> bool {
            self.inited.lock().unwrap().contains(path)
        }
    }

    /// Proves the `FileStore`/`VcsPort` seam actually works end-to-end for a
    /// non-native backend, not just that it type-checks against
    /// `LocalFs`/`NativeVcs` — no real disk or git2 touched anywhere here.
    #[test]
    fn notebook_store_works_entirely_through_injected_in_memory_backends() {
        let store = NotebookStore::new_with_backends(
            PathBuf::from("/virtual/root"),
            HashMap::new(),
            std::sync::Arc::new(MemVcs::new()),
            std::sync::Arc::new(MemFs::new()),
        );

        let nb = store.create("personal").unwrap();
        assert!(nb.path.starts_with("/virtual/root"));

        let note = nb.create_note("Grocery list", "milk, eggs").unwrap();
        assert_eq!(note.frontmatter.title, "Grocery list");

        let (_, notes) = nb.list_dir(Path::new("")).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].body, "milk, eggs");

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "personal");

        nb.delete_note_at(&note.path).unwrap();
        let (_, notes) = nb.list_dir(Path::new("")).unwrap();
        assert!(notes.is_empty());
    }
}

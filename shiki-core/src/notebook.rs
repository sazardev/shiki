use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::note::{Frontmatter, Note};
use crate::{git, Error, Result};

/// Slug for a new note in `dir`, guaranteed to not collide with an existing
/// file there. Falls back to a timestamp-based slug when `title` slugifies
/// to an empty string (symbol/emoji-only titles), then appends `-2`, `-3`,
/// etc. if that slug (or the timestamp fallback) is already taken — two
/// titles that slugify the same, e.g. "Q3 Report" and "Q3, Report!", must
/// not silently overwrite each other.
fn unique_slug(dir: &Path, title: &str) -> String {
    let base = Note::slugify(title);
    let base = if base.is_empty() {
        format!("untitled-{}", chrono::Local::now().timestamp())
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut n = 2;
    while dir.join(format!("{candidate}.md")).exists() {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    candidate
}

/// Same as `unique_slug`, but a collision with `ignore` (the note's own
/// current path, mid-rename) doesn't count — renaming a note to the title
/// it already effectively has shouldn't be blocked by itself.
fn unique_slug_excluding(dir: &Path, title: &str, ignore: &Path) -> String {
    let base = Note::slugify(title);
    let base = if base.is_empty() {
        format!("untitled-{}", chrono::Local::now().timestamp())
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut n = 2;
    loop {
        let path = dir.join(format!("{candidate}.md"));
        if !path.exists() || path == *ignore {
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

/// A notebook is a directory with its own git repo, containing `.md` notes.
#[derive(Debug, Clone)]
pub struct Notebook {
    pub name: String,
    pub path: PathBuf,
}

impl Notebook {
    pub fn new(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            name: name.into(),
            path,
        }
    }

    /// Lists the immediate contents of `relative` (a path within this
    /// notebook; `""` for the notebook root itself): subfolder names and
    /// notes, separately — a notebook can be nested arbitrarily deep, same
    /// as `nb`, and the caller (the Notes panel) walks one level at a time.
    /// Folders are sorted alphabetically; `.git` is never listed as a folder.
    ///
    /// `.md` files that don't parse as a shiki note (no `---` frontmatter —
    /// common in an imported/pre-existing repo, or one from `nb`) still show
    /// up: `Note::from_file` synthesizes metadata for those rather than
    /// failing, so nothing here needs to skip them.
    pub fn list_dir(&self, relative: &Path) -> Result<(Vec<String>, Vec<Note>)> {
        let dir = self.path.join(relative);
        if !dir.exists() {
            return Ok((Vec::new(), Vec::new()));
        }
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        entries.sort();

        let mut folders = Vec::new();
        let mut notes = Vec::new();
        for path in entries {
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == ".git") {
                    continue;
                }
                if let Some(name) = path.file_name() {
                    folders.push(name.to_string_lossy().to_string());
                }
            } else if path.extension().is_some_and(|ext| ext == "md") {
                notes.push(Note::from_file_in_notebook(&path, &self.name)?);
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
        std::fs::create_dir_all(&dir)?;
        let slug = unique_slug(&dir, title);
        let path = dir.join(format!("{slug}.md"));
        let note = Note::new(path, Frontmatter::new(title, &self.name), body.into());
        note.save()?;
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
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn note_path(&self, slug: &str) -> PathBuf {
        self.path.join(format!("{slug}.md"))
    }

    /// Deletes the note at its actual path (wherever it lives — root or a
    /// nested folder), not a path reconstructed from a root-relative slug.
    pub fn delete_note_at(&self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(Error::NoteNotFound(path.display().to_string()));
        }
        std::fs::remove_file(path)?;
        Ok(())
    }

    /// Renames the note at `path`, keeping it in the same folder.
    pub fn rename_note_at(&self, path: &Path, new_title: &str) -> Result<Note> {
        let mut note = Note::from_file_in_notebook(path, &self.name)?;
        let dir = path.parent().unwrap_or(&self.path);
        let slug = unique_slug_excluding(dir, new_title, path);
        let new_path = dir.join(format!("{slug}.md"));
        note.frontmatter.title = new_title.to_string();
        note.path = new_path;
        note.save()?;
        if path != note.path {
            std::fs::remove_file(path)?;
        }
        Ok(note)
    }

    /// Deletes the folder at `relative` (within this notebook) and
    /// everything inside it — recursive, no confirmation of its own; the
    /// caller (`App`) gates this behind a confirm dialog, same as
    /// note/notebook delete.
    pub fn delete_folder_at(&self, relative: &Path) -> Result<()> {
        let dir = self.path.join(relative);
        if !dir.is_dir() {
            return Err(Error::NoteNotFound(dir.display().to_string()));
        }
        std::fs::remove_dir_all(&dir)?;
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
        let mut copy = Note::from_file_in_notebook(path, &self.name)?;
        let dest_dir = dest_notebook.path.join(dest_relative);
        std::fs::create_dir_all(&dest_dir)?;
        let file_name = path
            .file_name()
            .ok_or_else(|| Error::NoteNotFound(path.display().to_string()))?;
        let dest_path = dest_dir.join(file_name);
        if dest_path.exists() {
            return Err(Error::DestinationExists(dest_path.display().to_string()));
        }
        copy.path = dest_path;
        if dest_notebook.name != self.name {
            copy.frontmatter.notebook = dest_notebook.name.clone();
        }
        copy.save()?;
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
        std::fs::remove_file(path)?;
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
        if dest_dir.exists() {
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
        std::fs::create_dir_all(&dest_dir)?;
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
        std::fs::remove_dir_all(self.path.join(relative))?;
        Ok(())
    }
}

/// Manages the collection of notebooks under the data directory (`~/.local/share/shiki/`).
///
/// Notebooks can live either under `root` (the default data directory) or at
/// custom absolute paths configured in `[notebooks.<name>] path = "..."`.
#[derive(Debug, Clone)]
pub struct NotebookStore {
    pub root: PathBuf,
    /// Custom absolute paths keyed by notebook name — these override the
    /// default `root/<name>` location for individual notebooks.
    pub custom_paths: HashMap<String, PathBuf>,
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

impl NotebookStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            custom_paths: HashMap::new(),
        }
    }

    pub fn new_with_custom_paths(root: PathBuf, custom_paths: HashMap<String, PathBuf>) -> Self {
        Self { root, custom_paths }
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
                notebooks.push(Notebook::new(name.clone(), path.clone()));
            }
        }

        // 2. Subdirectories under the root data dir
        if self.root.exists() {
            for entry in std::fs::read_dir(&self.root)? {
                let entry = entry?;
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                // Don't add a notebook twice if a custom path uses the same name
                if seen.insert(name.clone()) {
                    notebooks.push(Notebook::new(name, path));
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
        if !path.is_dir() {
            return Err(Error::NotebookNotFound(name.to_string()));
        }
        Ok(Notebook::new(name, path))
    }

    /// Creates a new notebook with its own git repo.
    /// If a custom path is configured for this notebook name, creates it
    /// there; otherwise creates it under the default data directory.
    pub fn create(&self, name: &str) -> Result<Notebook> {
        validate_name(name)?;
        let path = self
            .path_for(name)
            .ok_or_else(|| Error::NotebookNotFound(name.to_string()))?;
        if path.exists() {
            return Err(Error::NotebookExists(name.to_string()));
        }
        std::fs::create_dir_all(&path)?;
        git::init_repo(&path)?;
        Ok(Notebook::new(name, path))
    }

    pub fn rename(&self, old_name: &str, new_name: &str) -> Result<Notebook> {
        validate_name(old_name)?;
        validate_name(new_name)?;
        let old_path = self
            .path_for(old_name)
            .ok_or_else(|| Error::NotebookNotFound(old_name.to_string()))?;
        if !old_path.is_dir() {
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
        if new_path.exists() {
            return Err(Error::NotebookExists(new_name.to_string()));
        }
        std::fs::rename(&old_path, &new_path)?;
        Ok(Notebook::new(new_name, new_path))
    }

    pub fn delete(&self, name: &str) -> Result<()> {
        validate_name(name)?;
        let path = self
            .path_for(name)
            .ok_or_else(|| Error::NotebookNotFound(name.to_string()))?;
        if !path.is_dir() {
            return Err(Error::NotebookNotFound(name.to_string()));
        }
        std::fs::remove_dir_all(path)?;
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
}

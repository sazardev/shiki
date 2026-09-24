//! `shiki-mcp` — an MCP (Model Context Protocol) server exposing shiki's
//! note/notebook operations as typed tools, so an MCP client (Claude
//! Desktop, Claude Code, any other MCP client) can drive shiki directly
//! over stdio with structured, schema-validated arguments instead of
//! shelling out to the `shiki` CLI and quoting/parsing text. Calls
//! straight into `shiki-core` — the same domain logic the TUI and CLI
//! already share — never the `shiki` binary itself; this crate doesn't
//! depend on `shiki-cli` at all (see the workspace's own one-way
//! dependency chain).

mod helpers;
mod params;

use std::path::{Path, PathBuf};

use helpers::{
    diff_file_json, get_and_unlock, note_summary, parse_field_value, passphrase_from_env,
    reserved_field_owner, resolve_notebook_name, tool_err,
};
use params::*;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};
use shiki_config::Config;
use shiki_core::pagination::{page_json, paginate};
use shiki_core::{notebook, NotebookStore};

/// `store` re-reads the filesystem on every call (`NotebookStore::list`/
/// `get`/`all_notes_recursive` all do a fresh walk, nothing cached), so it
/// never goes stale on its own. `config` is different — plain fields read
/// straight off a snapshot loaded once at startup would silently miss a
/// notebook this same server just encrypted/decrypted via
/// `encrypt_notebook`/`decrypt_notebook`/`rekey_notebook` (which persist to
/// `config.toml` on disk but wouldn't be reflected in that snapshot).
/// `RwLock` fixes the self-inflicted case: those three tools take a write
/// lock, mutate, save, and every other tool's `self.config()` read lock
/// sees the update immediately afterward. It does *not* pick up an
/// external edit (e.g. someone hand-editing `config.toml`, or a
/// concurrently-running TUI's own settings change) — this server reads
/// its own writes correctly but doesn't watch the file for others'.
struct Shiki {
    store: NotebookStore,
    config: std::sync::RwLock<Config>,
}

impl Shiki {
    fn config(&self) -> std::sync::RwLockReadGuard<'_, Config> {
        self.config.read().unwrap_or_else(|e| e.into_inner())
    }
}

#[tool_router]
impl Shiki {
    #[tool(description = "Lists notes in a notebook (metadata only, no bodies), paginated.")]
    fn list_notes(&self, Parameters(p): Parameters<ListNotesParams>) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let notes = nb.all_notes_recursive().map_err(tool_err)?;
        let (page, total) = paginate(notes, p.page.offset, p.page.effective_limit());
        let items: Vec<_> = page.iter().map(note_summary).collect();
        Ok(page_json(items, total, p.page.offset, p.page.effective_limit()).to_string())
    }

    #[tool(description = "Shows a note's full rendered content, including its body.")]
    fn show_note(&self, Parameters(p): Parameters<ShowNoteParams>) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let note = notebook::find_note(&nb, &p.note).map_err(tool_err)?;
        Ok(serde_json::json!({
            "title": note.frontmatter.title,
            "date": note.frontmatter.date.to_string(),
            "tags": note.frontmatter.tags,
            "slug": note.file_stem(),
            "path": note.path,
            "body": note.body,
        })
        .to_string())
    }

    #[tool(description = "Fuzzy-searches note titles in a notebook, paginated by relevance.")]
    fn search_notes(
        &self,
        Parameters(p): Parameters<SearchNotesParams>,
    ) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let notes = nb.all_notes_recursive().map_err(tool_err)?;
        let mut engine = shiki_core::SearchEngine::new();
        let hits = engine.search(&p.query, &notes);
        let (page, total) = paginate(hits, p.page.offset, p.page.effective_limit());
        let items: Vec<_> = page
            .iter()
            .map(|hit| {
                let note = &notes[hit.index];
                let mut v = note_summary(note);
                v["score"] = serde_json::json!(hit.score);
                v
            })
            .collect();
        Ok(page_json(items, total, p.page.offset, p.page.effective_limit()).to_string())
    }

    #[tool(
        description = "Dataview-style filter/sort over frontmatter across one or every notebook, paginated. On a bad DSL string, the error lists the built-in fields and every custom field actually seen in your notes."
    )]
    fn query_notes(
        &self,
        Parameters(p): Parameters<QueryNotesParams>,
    ) -> Result<String, ErrorData> {
        let today = chrono::Local::now().date_naive();
        let pool = self.store.all_notes().map_err(tool_err)?;
        let query = shiki_core::query::parse(&p.dsl).map_err(|e| {
            let known = shiki_core::query::known_fields(&pool);
            let seen = if known.is_empty() {
                String::new()
            } else {
                format!("; seen in your notes: {}", known.join(", "))
            };
            ErrorData::invalid_params(
                format!(
                    "query error: {e}; built-in fields: {}{seen}; example: {}",
                    shiki_core::query::BUILTIN_FIELDS,
                    shiki_core::query::EXAMPLE_QUERY,
                ),
                None,
            )
        })?;
        let rows = shiki_core::query::run_query(&pool, &query, p.notebook.as_deref(), today);
        let (page, total) = paginate(rows, p.page.offset, p.page.effective_limit());
        let items: Vec<_> = page
            .iter()
            .map(|r| {
                let fields: serde_json::Map<String, serde_json::Value> = r
                    .fields
                    .iter()
                    .filter_map(|(k, v)| {
                        let key = k.as_str()?.to_string();
                        let value = serde_json::to_value(v).ok()?;
                        Some((key, value))
                    })
                    .collect();
                serde_json::json!({
                    "notebook": r.notebook,
                    "note": r.note_title,
                    "location": r.location,
                    "path": r.path,
                    "fields": fields,
                })
            })
            .collect();
        Ok(page_json(items, total, p.page.offset, p.page.effective_limit()).to_string())
    }

    #[tool(description = "Lists checkbox tasks across notebooks, urgency-sorted, paginated.")]
    fn list_tasks(&self, Parameters(p): Parameters<ListTasksParams>) -> Result<String, ErrorData> {
        let today = chrono::Local::now().date_naive();
        let pool = self.store.all_notes().map_err(tool_err)?;
        struct Row {
            task: shiki_core::tasks::Task,
            location: String,
            notebook: String,
            note_title: String,
            path: PathBuf,
        }
        let mut rows: Vec<Row> = pool
            .iter()
            .filter(|(nb, _)| p.notebook.as_deref().is_none_or(|w| nb.name == w))
            .flat_map(|(nb, note)| {
                let location = shiki_core::tasks::location_of(nb, note);
                shiki_core::tasks::extract(&note.body)
                    .into_iter()
                    .map(move |task| Row {
                        task,
                        location: location.clone(),
                        notebook: nb.name.clone(),
                        note_title: note.frontmatter.title.clone(),
                        path: note.path.clone(),
                    })
            })
            .filter(|r| {
                if !p.include_done && r.task.done {
                    return false;
                }
                match (p.overdue, p.today) {
                    (false, false) => true,
                    (o, t) => r
                        .task
                        .due
                        .is_some_and(|d| (o && d < today) || (t && d == today)),
                }
            })
            .collect();
        rows.sort_by_key(|r| (r.task.due.is_none(), r.task.due));
        let (page, total) = paginate(rows, p.page.offset, p.page.effective_limit());
        let items: Vec<_> = page
            .iter()
            .map(|r| {
                serde_json::json!({
                    "text": r.task.text,
                    "done": r.task.done,
                    "due": r.task.due.map(|d| d.to_string()),
                    "overdue": !r.task.done && r.task.due.is_some_and(|d| d < today),
                    "recurrence": r.task.recurrence,
                    "notebook": r.notebook,
                    "note": r.note_title,
                    "location": r.location,
                    "path": r.path,
                })
            })
            .collect();
        Ok(page_json(items, total, p.page.offset, p.page.effective_limit()).to_string())
    }

    #[tool(
        description = "Structural overview of your notebooks (note counts, busiest folders, busiest tags — never bodies or titles). Call this first when exploring an unfamiliar shiki setup, before list_notes/search_notes/query_notes."
    )]
    fn index(&self, Parameters(p): Parameters<IndexParams>) -> Result<String, ErrorData> {
        const TOP_N: usize = 25;
        let targets: Vec<shiki_core::Notebook> = match p.notebook.as_deref() {
            Some(name) => vec![notebook::get_notebook(&self.store, name).map_err(tool_err)?],
            None => {
                let config = self.config();
                self.store
                    .list()
                    .map_err(tool_err)?
                    .into_iter()
                    .filter(|nb| {
                        !config
                            .notebooks
                            .get(&nb.name)
                            .is_some_and(|over| over.hidden)
                    })
                    .collect()
            }
        };
        let top_n =
            |counts: std::collections::HashMap<String, usize>| -> (Vec<serde_json::Value>, bool) {
                let mut all: Vec<(String, usize)> = counts.into_iter().collect();
                all.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                let truncated = all.len() > TOP_N;
                all.truncate(TOP_N);
                (
                    all.into_iter()
                        .map(|(name, count)| serde_json::json!({"name": name, "count": count}))
                        .collect(),
                    truncated,
                )
            };
        let notebooks: Vec<serde_json::Value> = targets
            .into_iter()
            .map(|nb| {
                let name = nb.name.clone();
                let nb = match get_and_unlock(&self.store, &self.config(), &name) {
                    Ok(nb) => nb,
                    Err(e) => {
                        return serde_json::json!({"name": name, "locked": e.message.to_string()})
                    }
                };
                let notes = match nb.all_notes_recursive() {
                    Ok(n) => n,
                    Err(e) => return serde_json::json!({"name": name, "locked": e.to_string()}),
                };
                let mut folder_counts = std::collections::HashMap::new();
                let mut tag_counts = std::collections::HashMap::new();
                for note in &notes {
                    let folder = note
                        .path
                        .strip_prefix(&nb.path)
                        .unwrap_or(&note.path)
                        .parent()
                        .map(|p| p.display().to_string().replace('\\', "/"))
                        .unwrap_or_default();
                    *folder_counts.entry(folder).or_insert(0) += 1;
                    for tag in &note.frontmatter.tags {
                        *tag_counts.entry(tag.clone()).or_insert(0) += 1;
                    }
                }
                let (folders, folders_truncated) = top_n(folder_counts);
                let (tags, tags_truncated) = top_n(tag_counts);
                serde_json::json!({
                    "name": nb.name,
                    "note_count": notes.len(),
                    "folders": folders,
                    "folders_truncated": folders_truncated,
                    "tags": tags,
                    "tags_truncated": tags_truncated,
                })
            })
            .collect();
        Ok(serde_json::json!({ "notebooks": notebooks }).to_string())
    }

    #[tool(description = "Lists notebooks with their note counts.")]
    fn list_notebooks(
        &self,
        Parameters(p): Parameters<ListNotebooksParams>,
    ) -> Result<String, ErrorData> {
        let config = self.config();
        let items: Vec<serde_json::Value> = self
            .store
            .list()
            .map_err(tool_err)?
            .into_iter()
            .filter(|nb| {
                p.include_hidden
                    || !config
                        .notebooks
                        .get(&nb.name)
                        .is_some_and(|over| over.hidden)
            })
            .map(|nb| match nb.all_notes_recursive() {
                Ok(notes) => serde_json::json!({
                    "name": nb.name,
                    "path": nb.path,
                    "hidden": config.notebooks.get(&nb.name).is_some_and(|o| o.hidden),
                    "note_count": notes.len(),
                    "error": null,
                }),
                Err(e) => serde_json::json!({
                    "name": nb.name,
                    "path": nb.path,
                    "hidden": config.notebooks.get(&nb.name).is_some_and(|o| o.hidden),
                    "note_count": null,
                    "error": e.to_string(),
                }),
            })
            .collect();
        Ok(serde_json::json!({ "notebooks": items }).to_string())
    }

    #[tool(description = "Creates a new note.")]
    fn new_note(&self, Parameters(p): Parameters<NewNoteParams>) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = match self.store.get(&name) {
            Ok(nb) => nb,
            Err(_) => self.store.create(&name).map_err(tool_err)?,
        };
        let nb = get_and_unlock(&self.store, &self.config(), &nb.name)?;
        let folder = p.folder.as_deref().unwrap_or("");
        let mut note = nb
            .create_note_in(Path::new(folder), &p.title, p.body.unwrap_or_default())
            .map_err(tool_err)?;
        if !p.tags.is_empty() {
            note.frontmatter.tags = p.tags;
            note.save_with_crypto(nb.crypto.as_ref())
                .map_err(tool_err)?;
        }
        Ok(serde_json::json!({ "path": note.path, "title": note.frontmatter.title }).to_string())
    }

    #[tool(description = "Replaces or appends to a note's body, non-interactively.")]
    fn edit_note(&self, Parameters(p): Parameters<EditNoteParams>) -> Result<String, ErrorData> {
        if p.body.is_none() == p.append.is_none() {
            return Err(ErrorData::invalid_params(
                "give exactly one of `body` (replace) or `append`",
                None,
            ));
        }
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let mut note = notebook::find_note(&nb, &p.note).map_err(tool_err)?;
        if let Some(body) = p.body {
            note.body = body;
        } else if let Some(append) = p.append {
            if !note.body.is_empty() && !note.body.ends_with('\n') {
                note.body.push('\n');
            }
            note.body.push_str(&append);
        }
        note.save_with_crypto(nb.crypto.as_ref())
            .map_err(tool_err)?;
        Ok(serde_json::json!({ "path": note.path, "title": note.frontmatter.title }).to_string())
    }

    #[tool(description = "Deletes a note (trash-first, recoverable). Requires confirm: true.")]
    fn delete_note(
        &self,
        Parameters(p): Parameters<DeleteNoteParams>,
    ) -> Result<String, ErrorData> {
        if !p.confirm {
            return Err(ErrorData::invalid_params(
                "this removes the note from its usual location \u{2014} call again with confirm: true",
                None,
            ));
        }
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let note = notebook::find_note(&nb, &p.note).map_err(tool_err)?;
        let suffix = chrono::Local::now().timestamp_millis().to_string();
        let trash_path = Config::default_trash_dir().ok().and_then(|root| {
            shiki_core::trash::move_to_trash(&note.path, &root.join(&nb.name), &suffix).ok()
        });
        if trash_path.is_none() {
            nb.delete_note_at(&note.path).map_err(tool_err)?;
        }
        Ok(serde_json::json!({ "path": note.path, "trashed": trash_path }).to_string())
    }

    #[tool(description = "Renames a note, rewriting every inbound [[wikilink]] to it by default.")]
    fn rename_note(
        &self,
        Parameters(p): Parameters<RenameNoteParams>,
    ) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let old = notebook::find_note(&nb, &p.note).map_err(tool_err)?;
        let rewrite = p.rewrite_links.unwrap_or(true);
        let (links, files) = if rewrite {
            let old_targets = vec![old.frontmatter.title.clone(), old.file_stem()];
            let pool = self.store.all_notes().map_err(tool_err)?;
            let (links, files, _touched) =
                shiki_core::wikilinks::rewrite_links_to(&old_targets, &p.new_title, &pool)
                    .map_err(tool_err)?;
            (links, files)
        } else {
            (0, 0)
        };
        let renamed = nb
            .rename_note_at(&old.path, &p.new_title)
            .map_err(tool_err)?;
        Ok(serde_json::json!({
            "old_path": old.path,
            "new_path": renamed.path,
            "title": renamed.frontmatter.title,
            "links_rewritten": links,
            "notes_touched": files,
        })
        .to_string())
    }

    #[tool(description = "Moves or (with copy: true) copies a note to notebook/path/within.")]
    fn move_note(&self, Parameters(p): Parameters<MoveNoteParams>) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let source_nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let source_note = notebook::find_note(&source_nb, &p.note).map_err(tool_err)?;
        let (dest_notebook, dest_relative) =
            notebook::parse_address(&self.store, &p.destination).map_err(tool_err)?;
        let dest_notebook = get_and_unlock(&self.store, &self.config(), &dest_notebook.name)?;
        let result = if p.copy {
            source_nb.copy_note_to(&source_note.path, &dest_notebook, &dest_relative)
        } else {
            source_nb.move_note_to(&source_note.path, &dest_notebook, &dest_relative)
        }
        .map_err(tool_err)?;
        Ok(serde_json::json!({
            "source_path": source_note.path,
            "dest_path": result.path,
            "copied": p.copy,
        })
        .to_string())
    }

    #[tool(description = "Adds/removes tags on a note's frontmatter.")]
    fn tag_note(&self, Parameters(p): Parameters<TagNoteParams>) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let mut note = notebook::find_note(&nb, &p.note).map_err(tool_err)?;
        let mut changed = false;
        for tag in &p.add {
            let tag = tag.trim();
            if !tag.is_empty() && !note.frontmatter.tags.iter().any(|t| t == tag) {
                note.frontmatter.tags.push(tag.to_string());
                changed = true;
            }
        }
        if !p.remove.is_empty() {
            let before = note.frontmatter.tags.len();
            note.frontmatter
                .tags
                .retain(|t| !p.remove.iter().any(|r| r.trim() == t));
            changed |= note.frontmatter.tags.len() != before;
        }
        if changed {
            note.save_with_crypto(nb.crypto.as_ref())
                .map_err(tool_err)?;
        }
        Ok(serde_json::json!({ "path": note.path, "tags": note.frontmatter.tags }).to_string())
    }

    #[tool(
        description = "Sets/unsets custom frontmatter fields — the same map query_notes filters/sorts on."
    )]
    fn set_field(&self, Parameters(p): Parameters<SetFieldParams>) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let mut note = notebook::find_note(&nb, &p.note).map_err(tool_err)?;
        for key in p.set.keys().chain(p.unset.iter()) {
            if let Some(owner) = reserved_field_owner(key) {
                return Err(ErrorData::invalid_params(
                    format!("'{key}' isn't a custom field \u{2014} use {owner} instead"),
                    None,
                ));
            }
        }
        for (key, raw_value) in &p.set {
            note.frontmatter.extra.insert(
                serde_yaml::Value::String(key.clone()),
                parse_field_value(raw_value),
            );
        }
        for key in &p.unset {
            note.frontmatter.extra.remove(key.as_str());
        }
        note.save_with_crypto(nb.crypto.as_ref())
            .map_err(tool_err)?;
        Ok(serde_json::json!({ "path": note.path, "fields": note.frontmatter.extra }).to_string())
    }

    #[tool(description = "Creates an empty folder at any depth within a notebook.")]
    fn create_folder(
        &self,
        Parameters(p): Parameters<CreateFolderParams>,
    ) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let full = notebook::validate_relative_path(&p.path).map_err(tool_err)?;
        let folder_name = full
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                ErrorData::invalid_params(format!("'{}' has no folder name", p.path), None)
            })?
            .to_string();
        let parent = full.parent().unwrap_or(Path::new(""));
        let created = nb
            .create_folder_in(parent, &folder_name)
            .map_err(tool_err)?;
        Ok(serde_json::json!({ "path": created }).to_string())
    }

    #[tool(
        description = "Recursively deletes a folder and everything inside it (no trash). Requires confirm: true."
    )]
    fn delete_folder(
        &self,
        Parameters(p): Parameters<DeleteFolderParams>,
    ) -> Result<String, ErrorData> {
        if !p.confirm {
            return Err(ErrorData::invalid_params(
                "this permanently deletes the folder and everything inside it \u{2014} call again with confirm: true",
                None,
            ));
        }
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let relative = notebook::validate_relative_path(&p.path).map_err(tool_err)?;
        nb.delete_folder_at(&relative).map_err(tool_err)?;
        Ok(serde_json::json!({ "path": p.path, "deleted": true }).to_string())
    }

    #[tool(
        description = "Moves or (with copy: true) copies a whole folder to notebook/path/within."
    )]
    fn move_folder(
        &self,
        Parameters(p): Parameters<MoveFolderParams>,
    ) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = get_and_unlock(&self.store, &self.config(), &name)?;
        let relative = notebook::validate_relative_path(&p.path).map_err(tool_err)?;
        let (dest_notebook, dest_relative) =
            notebook::parse_address(&self.store, &p.destination).map_err(tool_err)?;
        let dest_notebook = get_and_unlock(&self.store, &self.config(), &dest_notebook.name)?;
        if p.copy {
            nb.copy_folder_to(&relative, &dest_notebook, &dest_relative)
        } else {
            nb.move_folder_to(&relative, &dest_notebook, &dest_relative)
        }
        .map_err(tool_err)?;
        Ok(serde_json::json!({
            "source": p.path,
            "destination": p.destination,
            "copied": p.copy,
        })
        .to_string())
    }

    #[tool(
        description = "Commits (and pushes, if the notebook's sync policy has auto_push on) a notebook's pending changes to git. Without this, changes made through the other tools stay uncommitted indefinitely."
    )]
    fn sync_notebook(
        &self,
        Parameters(p): Parameters<NotebookOnlyParams>,
    ) -> Result<String, ErrorData> {
        let config = self.config();
        let name = resolve_notebook_name(&config, p.notebook.as_deref());
        let nb = notebook::get_notebook(&self.store, &name).map_err(tool_err)?;
        let sync = config.sync_for(&name);

        let (committed, summary) = if config.git.auto_commit {
            let summary =
                shiki_core::git::diff_summary(&nb.path).unwrap_or_else(|_| "changes".to_string());
            let message = format!("{}{summary}", config.git.commit_prefix);
            let committed = shiki_core::git::commit_all(&nb.path, &message).map_err(tool_err)?;
            (Some(committed), Some(summary))
        } else {
            (None, None)
        };

        let mut pushed = false;
        let mut push_note = None;
        if sync.auto_push {
            if shiki_core::git::remote_url(&nb.path).is_none() {
                push_note = Some("no git remote configured for this notebook".to_string());
            } else {
                shiki_core::git::push(&nb.path, &config.git.remote).map_err(tool_err)?;
                pushed = true;
            }
        }
        Ok(serde_json::json!({
            "notebook": name,
            "auto_commit_enabled": config.git.auto_commit,
            "committed": committed,
            "summary": summary,
            "auto_push_enabled": sync.auto_push,
            "pushed": pushed,
            "push_note": push_note,
        })
        .to_string())
    }

    #[tool(
        description = "Pending changes (working tree vs last commit) for a note or the whole notebook. Not available for encrypted notebooks."
    )]
    fn diff_notebook(&self, Parameters(p): Parameters<DiffParams>) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = notebook::get_notebook(&self.store, &name).map_err(tool_err)?;
        if self.config().encrypt_for(&name) {
            return Err(ErrorData::invalid_params(
                "diff isn't available for encrypted notebooks",
                None,
            ));
        }
        let files: Vec<serde_json::Value> = match &p.note {
            Some(needle) => {
                let note = notebook::find_note(&nb, needle).map_err(tool_err)?;
                let relative = note
                    .path
                    .strip_prefix(&nb.path)
                    .unwrap_or(&note.path)
                    .display()
                    .to_string()
                    .replace('\\', "/");
                let lines = shiki_core::git::working_tree_diff(&nb.path, Path::new(&relative))
                    .map_err(tool_err)?;
                vec![diff_file_json(&relative, &lines)]
            }
            None => {
                let dirty = shiki_core::git::dirty_files(&nb.path).map_err(tool_err)?;
                dirty
                    .iter()
                    .map(|file| {
                        let lines = shiki_core::git::working_tree_diff(&nb.path, Path::new(file))
                            .unwrap_or_default();
                        diff_file_json(file, &lines)
                    })
                    .collect()
            }
        };
        Ok(serde_json::json!({ "notebook": name, "files": files }).to_string())
    }

    #[tool(
        description = "Recent git commits for a notebook, or every commit that touched one note, paginated."
    )]
    fn log_notebook(&self, Parameters(p): Parameters<LogParams>) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = notebook::get_notebook(&self.store, &name).map_err(tool_err)?;
        let revisions = match &p.note {
            Some(needle) => {
                let unlocked = get_and_unlock(&self.store, &self.config(), &name)?;
                let note = notebook::find_note(&unlocked, needle).map_err(tool_err)?;
                let relative = note
                    .path
                    .strip_prefix(&nb.path)
                    .unwrap_or(&note.path)
                    .display()
                    .to_string()
                    .replace('\\', "/");
                shiki_core::git::file_history(&nb.path, Path::new(&relative)).map_err(tool_err)?
            }
            None => shiki_core::git::recent_commits(&nb.path, usize::MAX).map_err(tool_err)?,
        };
        let (page, total) = paginate(revisions, p.page.offset, p.page.effective_limit());
        let items: Vec<_> = page
            .iter()
            .map(|rev| {
                serde_json::json!({
                    "commit": rev.commit_id,
                    "date": rev.date.to_rfc3339(),
                    "message": rev.message,
                })
            })
            .collect();
        Ok(page_json(items, total, p.page.offset, p.page.effective_limit()).to_string())
    }

    #[tool(
        description = "Creates or opens today's daily note (auto-creates the notebook if needed; injects a due-tasks agenda on first creation, if enabled)."
    )]
    fn daily_note(
        &self,
        Parameters(p): Parameters<NotebookOnlyParams>,
    ) -> Result<String, ErrorData> {
        let name = resolve_notebook_name(&self.config(), p.notebook.as_deref());
        let nb = match self.store.get(&name) {
            Ok(nb) => nb,
            Err(_) => self.store.create(&name).map_err(tool_err)?,
        };
        let nb = get_and_unlock(&self.store, &self.config(), &nb.name)?;
        let today = chrono::Local::now().date_naive();
        let templates_dir = Config::default_templates_dir().map_err(tool_err)?;
        shiki_core::templates::ensure_defaults(&templates_dir).map_err(tool_err)?;
        let (daily_template, daily_agenda) = {
            let config = self.config();
            (
                config.general.daily_template.clone(),
                config.general.daily_agenda,
            )
        };
        let agenda = daily_agenda
            .then(|| {
                self.store
                    .all_notes()
                    .ok()
                    .and_then(|pool| shiki_core::tasks::agenda_section(&pool, today))
            })
            .flatten();
        let note = shiki_core::daily::create_or_open(
            &nb,
            today,
            &templates_dir,
            &daily_template,
            agenda.as_deref(),
        )
        .map_err(tool_err)?;
        Ok(serde_json::json!({
            "path": note.path,
            "title": note.frontmatter.title,
            "body": note.body,
        })
        .to_string())
    }

    #[tool(description = "Creates a new notebook, optionally with a git remote.")]
    fn create_notebook(
        &self,
        Parameters(p): Parameters<CreateNotebookParams>,
    ) -> Result<String, ErrorData> {
        let nb = self.store.create(&p.name).map_err(tool_err)?;
        let mut remote_set = false;
        if let Some(url) = p.remote.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
            shiki_core::git::set_remote(&nb.path, url).map_err(tool_err)?;
            remote_set = true;
        }
        Ok(
            serde_json::json!({ "name": nb.name, "path": nb.path, "remote_set": remote_set })
                .to_string(),
        )
    }

    #[tool(description = "Renames a notebook.")]
    fn rename_notebook(
        &self,
        Parameters(p): Parameters<RenameNotebookParams>,
    ) -> Result<String, ErrorData> {
        let nb = self
            .store
            .rename(&p.old_name, &p.new_name)
            .map_err(tool_err)?;
        Ok(serde_json::json!({ "name": nb.name, "path": nb.path }).to_string())
    }

    #[tool(
        description = "Permanently deletes a notebook and every note in it — no trash/undo. Requires confirm: true."
    )]
    fn delete_notebook(
        &self,
        Parameters(p): Parameters<DeleteNotebookParams>,
    ) -> Result<String, ErrorData> {
        if !p.confirm {
            return Err(ErrorData::invalid_params(
                "this permanently deletes the notebook and every note in it \u{2014} call again with confirm: true",
                None,
            ));
        }
        self.store.delete(&p.name).map_err(tool_err)?;
        Ok(serde_json::json!({ "name": p.name, "deleted": true }).to_string())
    }

    #[tool(
        description = "Enables encryption at rest for a plaintext notebook — re-encrypts every existing note. The new passphrase comes from SHIKI_NEW_PASSPHRASE in this server's environment; there's no recovery if it's lost."
    )]
    fn encrypt_notebook(
        &self,
        Parameters(p): Parameters<NotebookNameParams>,
    ) -> Result<String, ErrorData> {
        let config_path = Config::default_path().map_err(tool_err)?;
        let mut config = Config::load_or_init(&config_path).map_err(tool_err)?;
        if config.encrypt_for(&p.name) {
            return Err(ErrorData::invalid_params(
                format!("'{}' is already encrypted", p.name),
                None,
            ));
        }
        let nb = notebook::get_notebook(&self.store, &p.name).map_err(tool_err)?;
        let passphrase = passphrase_from_env("SHIKI_NEW_PASSPHRASE")?;
        let crypto = shiki_core::crypto::NotebookCrypto::new(passphrase);
        let canary = shiki_core::crypto::canary_blob(&crypto).map_err(tool_err)?;
        std::fs::write(nb.path.join(shiki_core::crypto::CANARY_FILE), canary).map_err(tool_err)?;
        let notes = nb.all_notes_recursive().map_err(tool_err)?;
        for note in &notes {
            note.save_with_crypto(Some(&crypto)).map_err(tool_err)?;
        }
        config.notebooks.entry(p.name.clone()).or_default().encrypt = true;
        config.save(&config_path).map_err(tool_err)?;
        *self.config.write().unwrap_or_else(|e| e.into_inner()) = config;
        let commit_warning = shiki_core::git::commit_all(&nb.path, "shiki: enable encryption")
            .err()
            .map(|e| e.to_string());
        Ok(serde_json::json!({
            "name": p.name,
            "encrypted": true,
            "note_count": notes.len(),
            "commit_warning": commit_warning,
        })
        .to_string())
    }

    #[tool(
        description = "Reverses encrypt_notebook: decrypts every note back to plain text. The current passphrase comes from SHIKI_PASSPHRASE."
    )]
    fn decrypt_notebook(
        &self,
        Parameters(p): Parameters<NotebookNameParams>,
    ) -> Result<String, ErrorData> {
        let config_path = Config::default_path().map_err(tool_err)?;
        let mut config = Config::load_or_init(&config_path).map_err(tool_err)?;
        if !config.encrypt_for(&p.name) {
            return Err(ErrorData::invalid_params(
                format!("'{}' is not encrypted", p.name),
                None,
            ));
        }
        let nb = notebook::get_notebook(&self.store, &p.name).map_err(tool_err)?;
        let passphrase = passphrase_from_env("SHIKI_PASSPHRASE")?;
        let crypto = shiki_core::crypto::NotebookCrypto::new(passphrase);
        let canary_path = nb.path.join(shiki_core::crypto::CANARY_FILE);
        let canary = std::fs::read_to_string(&canary_path).map_err(|_| {
            ErrorData::invalid_params(
                format!(
                    "missing canary file \u{2014} was '{}' really encrypted by shiki?",
                    p.name
                ),
                None,
            )
        })?;
        match shiki_core::crypto::verify_canary(&crypto, &canary) {
            Ok(true) => {}
            Ok(false) => {
                return Err(ErrorData::invalid_params(
                    "canary file is corrupted, not just a wrong passphrase",
                    None,
                ))
            }
            Err(e) => {
                return Err(ErrorData::invalid_params(
                    format!("wrong passphrase: {e}"),
                    None,
                ))
            }
        }
        let nb_unlocked = nb.clone().with_crypto(Some(crypto));
        let notes = nb_unlocked.all_notes_recursive().map_err(tool_err)?;
        for note in &notes {
            note.save_with_crypto(None).map_err(tool_err)?;
        }
        std::fs::remove_file(&canary_path).ok();
        if let Some(over) = config.notebooks.get_mut(&p.name) {
            over.encrypt = false;
        }
        config.save(&config_path).map_err(tool_err)?;
        *self.config.write().unwrap_or_else(|e| e.into_inner()) = config;
        let commit_warning = shiki_core::git::commit_all(&nb.path, "shiki: disable encryption")
            .err()
            .map(|e| e.to_string());
        Ok(serde_json::json!({
            "name": p.name,
            "encrypted": false,
            "note_count": notes.len(),
            "commit_warning": commit_warning,
        })
        .to_string())
    }

    #[tool(
        description = "Changes an encrypted notebook's passphrase without a plaintext gap. Old passphrase from SHIKI_PASSPHRASE, new one from SHIKI_NEW_PASSPHRASE."
    )]
    fn rekey_notebook(
        &self,
        Parameters(p): Parameters<NotebookNameParams>,
    ) -> Result<String, ErrorData> {
        let config_path = Config::default_path().map_err(tool_err)?;
        let config = Config::load_or_init(&config_path).map_err(tool_err)?;
        if !config.encrypt_for(&p.name) {
            return Err(ErrorData::invalid_params(
                format!("'{}' is not encrypted", p.name),
                None,
            ));
        }
        let nb = notebook::get_notebook(&self.store, &p.name).map_err(tool_err)?;
        let old = passphrase_from_env("SHIKI_PASSPHRASE")?;
        let old_crypto = shiki_core::crypto::NotebookCrypto::new(old);
        let canary_path = nb.path.join(shiki_core::crypto::CANARY_FILE);
        let canary = std::fs::read_to_string(&canary_path).map_err(|_| {
            ErrorData::invalid_params(
                format!(
                    "missing canary file \u{2014} was '{}' really encrypted by shiki?",
                    p.name
                ),
                None,
            )
        })?;
        match shiki_core::crypto::verify_canary(&old_crypto, &canary) {
            Ok(true) => {}
            Ok(false) => {
                return Err(ErrorData::invalid_params(
                    "canary file is corrupted, not just a wrong passphrase",
                    None,
                ))
            }
            Err(e) => {
                return Err(ErrorData::invalid_params(
                    format!("wrong passphrase: {e}"),
                    None,
                ))
            }
        }
        let new_passphrase = passphrase_from_env("SHIKI_NEW_PASSPHRASE")?;
        let new_crypto = shiki_core::crypto::NotebookCrypto::new(new_passphrase);
        let nb_unlocked = nb.clone().with_crypto(Some(old_crypto));
        let notes = nb_unlocked.all_notes_recursive().map_err(tool_err)?;
        for note in &notes {
            note.save_with_crypto(Some(&new_crypto)).map_err(tool_err)?;
        }
        let new_canary = shiki_core::crypto::canary_blob(&new_crypto).map_err(tool_err)?;
        std::fs::write(&canary_path, new_canary).map_err(tool_err)?;
        let commit_warning = shiki_core::git::commit_all(&nb.path, "shiki: rekey passphrase")
            .err()
            .map(|e| e.to_string());
        Ok(serde_json::json!({
            "name": p.name,
            "rekeyed": true,
            "note_count": notes.len(),
            "commit_warning": commit_warning,
        })
        .to_string())
    }

    #[tool(
        description = "Environment sanity check for this MCP server: config, data/template directories, and every notebook's reachability. Call this before relying on the server in a new environment."
    )]
    fn doctor(&self) -> Result<String, ErrorData> {
        let config_path = Config::default_path().map_err(tool_err)?;
        let config_parses = Config::load_or_init(&config_path).is_ok();
        let data_dir = self.store.root.clone();
        let data_dir_exists = data_dir.is_dir();
        let templates_dir = Config::default_templates_dir().ok();
        let templates_dir_exists = templates_dir.as_ref().is_some_and(|d| d.is_dir());
        let notebooks = self.store.list().map_err(tool_err)?;
        let notebook_reports: Vec<serde_json::Value> = notebooks
            .iter()
            .map(|nb| match nb.all_notes_recursive() {
                Ok(notes) => {
                    serde_json::json!({"name": nb.name, "ok": true, "note_count": notes.len()})
                }
                Err(e) => serde_json::json!({"name": nb.name, "ok": false, "error": e.to_string()}),
            })
            .collect();
        Ok(serde_json::json!({
            "config_path": config_path,
            "config_parses": config_parses,
            "data_dir": data_dir,
            "data_dir_exists": data_dir_exists,
            "templates_dir": templates_dir,
            "templates_dir_exists": templates_dir_exists,
            "notebooks": notebook_reports,
            "shiki_passphrase_set": std::env::var("SHIKI_PASSPHRASE").is_ok(),
        })
        .to_string())
    }
}

#[tool_handler(
    name = "shiki-mcp",
    instructions = "Manage the user's shiki notes: notebooks of Markdown files with YAML frontmatter, each notebook its own git repo. Call `index` first to orient (note counts, folders, tags) before list_notes/search_notes/query_notes. Every write tool acts on a specific notebook (defaults to the configured default notebook when omitted) and a note is addressed by its title or filename slug. Destructive tools (delete_note, delete_folder, delete_notebook) require an explicit confirm: true. IMPORTANT: writes are not committed to git automatically \u{2014} call `sync_notebook` after a batch of changes to actually commit (and push, if enabled) them; without it, changes just sit as uncommitted working-tree state. `encrypt_notebook`/`decrypt_notebook`/`rekey_notebook` read passphrases only from this server's own SHIKI_PASSPHRASE/SHIKI_NEW_PASSPHRASE environment variables, never interactively."
)]
impl ServerHandler for Shiki {}

fn load_context() -> anyhow::Result<Shiki> {
    let config_path = Config::default_path()?;
    let config = Config::load_or_init(&config_path)?;
    let templates_dir = Config::default_templates_dir()?;
    shiki_core::templates::ensure_defaults(&templates_dir)?;
    let data_dir = match config.general.data_dir.as_ref() {
        Some(dir) => PathBuf::from(dir),
        None => Config::default_data_dir()?,
    };
    let custom_paths = config.notebook_custom_paths();
    let mut store = NotebookStore::new_with_custom_paths(data_dir, custom_paths);
    store.extra_extensions = config.general.note_extra_extensions.clone();
    Ok(Shiki {
        store,
        config: std::sync::RwLock::new(config),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let shiki = load_context()?;
    let service = shiki.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

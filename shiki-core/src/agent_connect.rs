//! `shiki agent connect` — registers the `shiki-mcp` server (see `mcp`'s
//! docs in `IDEA.md`) with popular AI coding tools' own MCP config files,
//! and drops in usage guidance (a real Claude Code Skill, or an
//! `AGENTS.md` section for every other client) so a connected agent
//! actually discovers *how* to use the tools well — paginate, call
//! `sync_notebook`, the encrypted-notebook env vars — not just that they
//! exist.
//!
//! Every merge here only ever touches the one nested key it owns
//! (`mcpServers.shiki` / `mcp.shiki`) via a real JSON parse, never a
//! textual patch — and refuses to touch a file at all if it doesn't parse
//! as clean JSON, printing the snippet to paste by hand instead. That's
//! why `Continue` gets a brand-new dedicated file
//! (`.continue/mcpServers/shiki.yaml`) rather than a merge into its own
//! `config.yaml`: that file can have comments/anchors this module has no
//! business trying to parse, so the safer move is picking a file nothing
//! else owns instead of attempting a risky merge.
//!
//! Key order in a rewritten file may shift (this crate's `serde_json`
//! dependency doesn't enable `preserve_order`, so `Value::Object` sorts
//! alphabetically on serialize) — a minor diff-noise cosmetic, not data
//! loss; every sibling key's *value* is always preserved untouched.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::process;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentClient {
    ClaudeCode,
    ClaudeDesktop,
    OpenCode,
    Cursor,
    Windsurf,
    Continue,
}

impl AgentClient {
    pub const ALL: [AgentClient; 6] = [
        AgentClient::ClaudeCode,
        AgentClient::ClaudeDesktop,
        AgentClient::OpenCode,
        AgentClient::Cursor,
        AgentClient::Windsurf,
        AgentClient::Continue,
    ];

    pub fn id(self) -> &'static str {
        match self {
            AgentClient::ClaudeCode => "claude-code",
            AgentClient::ClaudeDesktop => "claude-desktop",
            AgentClient::OpenCode => "opencode",
            AgentClient::Cursor => "cursor",
            AgentClient::Windsurf => "windsurf",
            AgentClient::Continue => "continue",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            AgentClient::ClaudeCode => "Claude Code",
            AgentClient::ClaudeDesktop => "Claude Desktop",
            AgentClient::OpenCode => "OpenCode",
            AgentClient::Cursor => "Cursor",
            AgentClient::Windsurf => "Windsurf",
            AgentClient::Continue => "Continue",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        AgentClient::ALL.into_iter().find(|c| c.id() == id)
    }

    /// Claude Code gets a real Skill file instead of an `AGENTS.md`
    /// section — every other client falls back to `AGENTS.md`, the one
    /// convention all of them actually read.
    pub fn uses_skill_file(self) -> bool {
        matches!(self, AgentClient::ClaudeCode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The tool's global/user config — works regardless of which
    /// directory you're in when talking to the agent. The default for
    /// every client except `Continue`, since shiki notebooks aren't
    /// bound to any one project.
    User,
    /// The tool's project-local config, rooted at the given
    /// `project_root` (normally the current directory).
    Project,
}

/// Not every client supports both scopes — `ClaudeDesktop`/`Windsurf` are
/// user-only (no project-level config exists for either), and `Continue`'s
/// own MCP convention is project-only.
pub fn supports_scope(client: AgentClient, scope: Scope) -> bool {
    !matches!(
        (client, scope),
        (AgentClient::ClaudeDesktop, Scope::Project)
            | (AgentClient::Windsurf, Scope::Project)
            | (AgentClient::Continue, Scope::User)
    )
}

pub fn default_scope(client: AgentClient) -> Scope {
    if client == AgentClient::Continue {
        Scope::Project
    } else {
        Scope::User
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MergeError {
    #[error("existing file isn't valid JSON")]
    NotValidJson,
    #[error("existing file's top level isn't a JSON object")]
    NotAnObject,
}

/// The config file this client/scope combination reads, per each tool's
/// own documented MCP setup (verified against each project's current
/// docs, not assumed from general MCP convention — the top-level key
/// differs, e.g. OpenCode's `mcp` vs. everyone else's `mcpServers`).
pub fn config_path(
    client: AgentClient,
    scope: Scope,
    project_root: &Path,
) -> crate::Result<PathBuf> {
    if !supports_scope(client, scope) {
        return Err(crate::Error::AgentConnect(format!(
            "{} doesn't support {} scope",
            client.display_name(),
            scope_label(scope)
        )));
    }
    Ok(match (client, scope) {
        (AgentClient::ClaudeCode, Scope::User) => process::expand_home("~/.claude.json"),
        (AgentClient::ClaudeCode, Scope::Project) => project_root.join(".mcp.json"),
        (AgentClient::ClaudeDesktop, Scope::User) => claude_desktop_config_path(),
        (AgentClient::Cursor, Scope::User) => process::expand_home("~/.cursor/mcp.json"),
        (AgentClient::Cursor, Scope::Project) => project_root.join(".cursor").join("mcp.json"),
        (AgentClient::Windsurf, Scope::User) => {
            process::expand_home("~/.codeium/windsurf/mcp_config.json")
        }
        (AgentClient::OpenCode, Scope::User) => {
            process::expand_home("~/.config/opencode/opencode.json")
        }
        (AgentClient::OpenCode, Scope::Project) => project_root.join("opencode.json"),
        (AgentClient::Continue, Scope::Project) => project_root
            .join(".continue")
            .join("mcpServers")
            .join("shiki.yaml"),
        // Unreachable: every (client, scope) pair not covered above already
        // returned early via the `supports_scope` guard.
        (AgentClient::ClaudeDesktop, Scope::Project)
        | (AgentClient::Windsurf, Scope::Project)
        | (AgentClient::Continue, Scope::User) => unreachable!(),
    })
}

fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::User => "user",
        Scope::Project => "project",
    }
}

fn claude_desktop_config_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        process::expand_home("~/Library/Application Support/Claude/claude_desktop_config.json")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| process::expand_home("~"));
        appdata.join("Claude").join("claude_desktop_config.json")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        process::expand_home("~/.config/Claude/claude_desktop_config.json")
    }
}

/// `(top-level object key, this server's entry key)` — every JSON-based
/// client except OpenCode uses `mcpServers`; OpenCode uses `mcp`. Never
/// called for `Continue`, which doesn't go through the JSON merge path at
/// all (see the module doc comment).
fn entry_key_path(client: AgentClient) -> (&'static str, &'static str) {
    match client {
        AgentClient::OpenCode => ("mcp", "shiki"),
        _ => ("mcpServers", "shiki"),
    }
}

/// The JSON value for this client's own `shiki` entry shape — confirmed
/// live per-client (see the plan doc / `IDEA.md`), not a single assumed
/// shape: Claude Code wants an explicit `"type": "stdio"`, OpenCode wants
/// `"type": "local"` plus `command` as an array combining executable +
/// args, everyone else just wants a bare `{command, args}`.
fn entry_value(client: AgentClient, command: &str) -> serde_json::Value {
    match client {
        AgentClient::ClaudeCode => serde_json::json!({
            "type": "stdio",
            "command": command,
            "args": [],
        }),
        AgentClient::OpenCode => serde_json::json!({
            "type": "local",
            "command": [command],
            "enabled": true,
        }),
        _ => serde_json::json!({
            "command": command,
            "args": [],
        }),
    }
}

/// Pure merge: only ever inserts/overwrites `root[top_key][entry_key]` —
/// every other key in `existing`, at any depth, passes through untouched.
/// `existing = None` starts from an empty object (the file doesn't exist
/// yet). Returns `Err` instead of guessing at anything that isn't clean,
/// parseable JSON with an object at the top.
pub fn upsert_mcp_entry(
    existing: Option<&str>,
    client: AgentClient,
    command: &str,
) -> Result<String, MergeError> {
    let mut root: serde_json::Value = match existing {
        Some(text) => serde_json::from_str(text).map_err(|_| MergeError::NotValidJson)?,
        None => serde_json::json!({}),
    };
    let root_obj = root.as_object_mut().ok_or(MergeError::NotAnObject)?;
    let (top_key, entry_key) = entry_key_path(client);
    let top = root_obj
        .entry(top_key)
        .or_insert_with(|| serde_json::json!({}));
    let top_obj = top.as_object_mut().ok_or(MergeError::NotAnObject)?;
    top_obj.insert(entry_key.to_string(), entry_value(client, command));
    Ok(serde_json::to_string_pretty(&root).expect("Value always serializes"))
}

/// The inverse of `upsert_mcp_entry` — removes only `root[top_key]
/// [entry_key]`, leaving everything else (including a now-empty
/// `top_key` object) exactly as it was.
pub fn remove_mcp_entry(existing: &str, client: AgentClient) -> Result<String, MergeError> {
    let mut root: serde_json::Value =
        serde_json::from_str(existing).map_err(|_| MergeError::NotValidJson)?;
    let root_obj = root.as_object_mut().ok_or(MergeError::NotAnObject)?;
    let (top_key, entry_key) = entry_key_path(client);
    if let Some(top_obj) = root_obj.get_mut(top_key).and_then(|v| v.as_object_mut()) {
        top_obj.remove(entry_key);
    }
    Ok(serde_json::to_string_pretty(&root).expect("Value always serializes"))
}

/// The exact JSON block to paste by hand — used both for the `generic`
/// client (which never writes a file) and as the fallback message when a
/// real client's own config file failed the safety check above.
fn manual_snippet(client: AgentClient, command: &str) -> String {
    let (top_key, entry_key) = entry_key_path(client);
    let snippet = serde_json::json!({
        top_key: { entry_key: entry_value(client, command) }
    });
    serde_json::to_string_pretty(&snippet).unwrap_or_default()
}

/// The plain `mcpServers`-shaped snippet shown by `shiki agent connect
/// generic` — the same shape Claude Desktop/Cursor/Windsurf already use,
/// which is the closest thing to a lowest-common-denominator MCP config.
pub fn generic_snippet(command: &str) -> String {
    manual_snippet(AgentClient::Cursor, command)
}

#[derive(Serialize)]
struct ContinueMcpServer<'a> {
    name: &'a str,
    command: &'a str,
    args: Vec<&'a str>,
}

fn continue_yaml(command: &str) -> String {
    serde_yaml::to_string(&ContinueMcpServer {
        name: "shiki",
        command,
        args: vec![],
    })
    .expect("a plain string struct always serializes to YAML")
}

const AGENTS_MD_BEGIN: &str = "<!-- shiki:agent-section -->";
const AGENTS_MD_END: &str = "<!-- /shiki:agent-section -->";

/// The single source of usage guidance fed into both the Claude Code
/// Skill and the `AGENTS.md` section, so the two can't drift apart.
pub fn agent_usage_guide() -> &'static str {
    "- Call `index` first to orient — note counts, busiest folders/tags — before listing or \
searching everything.\n\
- Every list-shaped tool (`list_notes`, `search_notes`, `query_notes`, `list_tasks`, \
`log_notebook`) is paginated (`limit`/`offset`, default 50) — check `has_more` and page through \
instead of assuming one call returned everything.\n\
- Nothing is committed to git automatically. After a batch of note/notebook changes, call \
`sync_notebook` to commit (and push, if that notebook's sync policy allows it) — otherwise \
changes just sit as uncommitted working-tree state.\n\
- `delete_note`/`delete_folder`/`delete_notebook` require an explicit `confirm: true` argument.\n\
- An encrypted notebook needs `SHIKI_PASSPHRASE` (and `SHIKI_NEW_PASSPHRASE` for \
`encrypt_notebook`/`rekey_notebook`) set in the MCP server process's own environment — it has no \
terminal to prompt on."
}

fn claude_code_skill_md() -> String {
    format!(
        "---\nname: shiki\ndescription: Manage the user's shiki notes and notebooks (create, \
search, tag, organize, sync to git) via the shiki-mcp tools. Use this whenever the user asks to \
save, find, organize, or review their personal notes.\n---\n\n{}\n",
        agent_usage_guide()
    )
}

fn agents_md_section() -> String {
    format!(
        "{AGENTS_MD_BEGIN}\n## Using shiki\n\n{}\n{AGENTS_MD_END}\n",
        agent_usage_guide()
    )
}

/// Idempotent: re-running replaces the previous section in place instead
/// of appending a duplicate. Appends a fresh section (creating the file's
/// content from scratch if `existing` is `None`) when no markers are
/// found yet.
pub fn upsert_agents_md_section(existing: Option<&str>) -> String {
    let section = agents_md_section();
    let Some(text) = existing else {
        return section;
    };
    match (text.find(AGENTS_MD_BEGIN), text.find(AGENTS_MD_END)) {
        (Some(start), Some(end_marker)) => {
            let end = end_marker + AGENTS_MD_END.len();
            format!(
                "{}{}{}",
                &text[..start],
                section,
                text[end..].trim_start_matches('\n')
            )
        }
        _ => {
            let mut out = text.trim_end_matches('\n').to_string();
            out.push_str("\n\n");
            out.push_str(&section);
            out
        }
    }
}

/// Removes a previously-inserted section; text with no markers at all is
/// returned unchanged.
pub fn remove_agents_md_section(existing: &str) -> String {
    match (existing.find(AGENTS_MD_BEGIN), existing.find(AGENTS_MD_END)) {
        (Some(start), Some(end_marker)) => {
            let end = end_marker + AGENTS_MD_END.len();
            format!(
                "{}{}",
                &existing[..start],
                existing[end..].trim_start_matches('\n')
            )
        }
        _ => existing.to_string(),
    }
}

fn skill_file_path(scope: Scope, project_root: &Path) -> PathBuf {
    match scope {
        Scope::User => process::expand_home("~/.claude/skills/shiki/SKILL.md"),
        Scope::Project => project_root
            .join(".claude")
            .join("skills")
            .join("shiki")
            .join("SKILL.md"),
    }
}

/// Searches `path_env` (normally `$PATH`, passed in explicitly so this
/// stays unit-testable) for a `shiki-mcp` binary.
pub fn find_shiki_mcp_on_path(path_env: &str) -> Option<PathBuf> {
    std::env::split_paths(path_env).find_map(|dir| {
        let candidate = dir.join("shiki-mcp");
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(target_os = "windows")]
        {
            let with_exe = dir.join("shiki-mcp.exe");
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
        None
    })
}

pub struct ConnectOutcome {
    pub mcp_config_path: PathBuf,
    pub guidance_path: PathBuf,
    pub shiki_mcp_found: bool,
    pub warning: Option<String>,
}

/// Resolves the `shiki-mcp` command to embed in a written config: an
/// absolute path when it's found on `$PATH` (GUI-launched clients like
/// Cursor/Windsurf/Claude Desktop are known to inherit a minimal PATH, so
/// a bare command name is the wrong default there), otherwise the bare
/// name `"shiki-mcp"` plus a warning for the caller to surface.
fn resolve_command() -> (String, bool, Option<String>) {
    let path_env = std::env::var("PATH").unwrap_or_default();
    match find_shiki_mcp_on_path(&path_env) {
        Some(p) => (p.display().to_string(), true, None),
        None => (
            "shiki-mcp".to_string(),
            false,
            Some(
                "shiki-mcp not found on $PATH — run `cargo install shiki-mcp`, then it will \
resolve automatically."
                    .to_string(),
            ),
        ),
    }
}

/// Registers `shiki-mcp` with `client` at `scope`, and writes/updates its
/// usage guidance (a Skill file for Claude Code, an `AGENTS.md` section
/// for everyone else). Never partially applies a broken merge: a JSON
/// merge failure aborts before anything is written and reports the exact
/// manual snippet to paste instead.
pub fn connect(
    client: AgentClient,
    scope: Scope,
    project_root: &Path,
) -> crate::Result<ConnectOutcome> {
    let (command, shiki_mcp_found, warning) = resolve_command();

    let mcp_config_path = config_path(client, scope, project_root)?;
    if client == AgentClient::Continue {
        write_file(&mcp_config_path, &continue_yaml(&command))?;
    } else {
        let existing = std::fs::read_to_string(&mcp_config_path).ok();
        let contents = upsert_mcp_entry(existing.as_deref(), client, &command).map_err(|e| {
            crate::Error::AgentConnect(format!(
                "{e} at {} — paste this in by hand instead:\n{}",
                mcp_config_path.display(),
                manual_snippet(client, &command)
            ))
        })?;
        write_file(&mcp_config_path, &contents)?;
    }

    let guidance_path = if client.uses_skill_file() {
        let path = skill_file_path(scope, project_root);
        write_file(&path, &claude_code_skill_md())?;
        path
    } else {
        let path = project_root.join("AGENTS.md");
        let existing = std::fs::read_to_string(&path).ok();
        write_file(&path, &upsert_agents_md_section(existing.as_deref()))?;
        path
    };

    Ok(ConnectOutcome {
        mcp_config_path,
        guidance_path,
        shiki_mcp_found,
        warning,
    })
}

pub struct DisconnectOutcome {
    pub mcp_config_path: PathBuf,
    pub mcp_removed: bool,
    pub guidance_path: PathBuf,
    pub guidance_removed: bool,
}

pub fn disconnect(
    client: AgentClient,
    scope: Scope,
    project_root: &Path,
) -> crate::Result<DisconnectOutcome> {
    let mcp_config_path = config_path(client, scope, project_root)?;
    let mcp_removed = if client == AgentClient::Continue {
        if mcp_config_path.is_file() {
            std::fs::remove_file(&mcp_config_path)?;
            true
        } else {
            false
        }
    } else {
        match std::fs::read_to_string(&mcp_config_path) {
            Ok(existing) => {
                let updated = remove_mcp_entry(&existing, client).map_err(|e| {
                    crate::Error::AgentConnect(format!(
                        "{e} at {} — remove the \"shiki\" entry by hand",
                        mcp_config_path.display()
                    ))
                })?;
                write_file(&mcp_config_path, &updated)?;
                true
            }
            Err(_) => false,
        }
    };

    let (guidance_path, guidance_removed) = if client.uses_skill_file() {
        let path = skill_file_path(scope, project_root);
        let removed = path.is_file();
        if removed {
            std::fs::remove_file(&path)?;
        }
        (path, removed)
    } else {
        let path = project_root.join("AGENTS.md");
        match std::fs::read_to_string(&path) {
            Ok(existing) if existing.contains(AGENTS_MD_BEGIN) => {
                write_file(&path, &remove_agents_md_section(&existing))?;
                (path, true)
            }
            _ => (path, false),
        }
    };

    Ok(DisconnectOutcome {
        mcp_config_path,
        mcp_removed,
        guidance_path,
        guidance_removed,
    })
}

pub struct ClientStatus {
    pub config_path: PathBuf,
    pub file_exists: bool,
    pub shiki_connected: bool,
}

pub fn status(
    client: AgentClient,
    scope: Scope,
    project_root: &Path,
) -> crate::Result<ClientStatus> {
    let path = config_path(client, scope, project_root)?;
    let file_exists = path.is_file();
    let shiki_connected = if !file_exists {
        false
    } else if client == AgentClient::Continue {
        true
    } else {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .map(|root| {
                let (top_key, entry_key) = entry_key_path(client);
                root.get(top_key).and_then(|t| t.get(entry_key)).is_some()
            })
            .unwrap_or(false)
    };
    Ok(ClientStatus {
        config_path: path,
        file_exists,
        shiki_connected,
    })
}

fn write_file(path: &Path, contents: &str) -> crate::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ids_round_trip() {
        for client in AgentClient::ALL {
            assert_eq!(AgentClient::from_id(client.id()), Some(client));
        }
        assert_eq!(AgentClient::from_id("nonexistent"), None);
    }

    #[test]
    fn upsert_creates_fresh_file_when_none_exists() {
        let out = upsert_mcp_entry(None, AgentClient::Cursor, "/bin/shiki-mcp").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mcpServers"]["shiki"]["command"], "/bin/shiki-mcp");
        assert_eq!(v["mcpServers"]["shiki"]["args"], serde_json::json!([]));
    }

    #[test]
    fn upsert_preserves_unrelated_sibling_keys() {
        let existing = r#"{
            "mcpServers": { "other-tool": { "command": "other" } },
            "someUnrelatedSetting": true
        }"#;
        let out = upsert_mcp_entry(Some(existing), AgentClient::Cursor, "/bin/shiki-mcp").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mcpServers"]["other-tool"]["command"], "other");
        assert_eq!(v["someUnrelatedSetting"], true);
        assert_eq!(v["mcpServers"]["shiki"]["command"], "/bin/shiki-mcp");
    }

    #[test]
    fn upsert_is_idempotent() {
        let first = upsert_mcp_entry(None, AgentClient::Cursor, "/bin/shiki-mcp").unwrap();
        let second = upsert_mcp_entry(Some(&first), AgentClient::Cursor, "/bin/shiki-mcp").unwrap();
        let v: serde_json::Value = serde_json::from_str(&second).unwrap();
        assert_eq!(v["mcpServers"].as_object().unwrap().len(), 1);
    }

    #[test]
    fn upsert_rejects_malformed_json_without_writing_anything() {
        let err = upsert_mcp_entry(Some("{ not json"), AgentClient::Cursor, "cmd").unwrap_err();
        assert_eq!(err, MergeError::NotValidJson);
    }

    #[test]
    fn upsert_rejects_non_object_top_level() {
        let err = upsert_mcp_entry(Some("[1, 2, 3]"), AgentClient::Cursor, "cmd").unwrap_err();
        assert_eq!(err, MergeError::NotAnObject);
    }

    #[test]
    fn opencode_uses_mcp_key_and_array_command() {
        let out = upsert_mcp_entry(None, AgentClient::OpenCode, "/bin/shiki-mcp").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mcp"]["shiki"]["type"], "local");
        assert_eq!(
            v["mcp"]["shiki"]["command"],
            serde_json::json!(["/bin/shiki-mcp"])
        );
    }

    #[test]
    fn claude_code_uses_stdio_type() {
        let out = upsert_mcp_entry(None, AgentClient::ClaudeCode, "shiki-mcp").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["mcpServers"]["shiki"]["type"], "stdio");
    }

    #[test]
    fn remove_deletes_only_the_shiki_entry() {
        let existing = r#"{
            "mcpServers": { "shiki": { "command": "x" }, "other-tool": { "command": "y" } }
        }"#;
        let out = remove_mcp_entry(existing, AgentClient::Cursor).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert!(v["mcpServers"].get("shiki").is_none());
        assert_eq!(v["mcpServers"]["other-tool"]["command"], "y");
    }

    #[test]
    fn remove_on_malformed_json_errors_without_guessing() {
        let err = remove_mcp_entry("not json", AgentClient::Cursor).unwrap_err();
        assert_eq!(err, MergeError::NotValidJson);
    }

    #[test]
    fn agents_md_section_appends_when_no_existing_file() {
        let out = upsert_agents_md_section(None);
        assert!(out.contains(AGENTS_MD_BEGIN));
        assert!(out.contains(AGENTS_MD_END));
    }

    #[test]
    fn agents_md_section_appends_after_existing_content() {
        let out = upsert_agents_md_section(Some("# My project\n\nSome notes.\n"));
        assert!(out.starts_with("# My project"));
        assert!(out.contains(AGENTS_MD_BEGIN));
    }

    #[test]
    fn agents_md_section_replaces_in_place_on_second_run_not_duplicated() {
        let first = upsert_agents_md_section(Some("# Project\n"));
        let second = upsert_agents_md_section(Some(&first));
        assert_eq!(
            second.matches(AGENTS_MD_BEGIN).count(),
            1,
            "must not duplicate the marker on a second run"
        );
        assert!(second.starts_with("# Project"));
    }

    #[test]
    fn agents_md_section_preserves_content_after_the_marker() {
        let with_section = upsert_agents_md_section(Some("# Project\n"));
        let with_trailer = format!("{with_section}\n## Something else\n\ntext\n");
        let updated = upsert_agents_md_section(Some(&with_trailer));
        assert!(updated.contains("## Something else"));
        assert_eq!(updated.matches(AGENTS_MD_BEGIN).count(), 1);
    }

    #[test]
    fn remove_agents_md_section_strips_only_the_marked_block() {
        let with_section = upsert_agents_md_section(Some("# Project\n\nIntro.\n"));
        let removed = remove_agents_md_section(&with_section);
        assert!(removed.starts_with("# Project"));
        assert!(!removed.contains(AGENTS_MD_BEGIN));
    }

    #[test]
    fn remove_agents_md_section_is_a_no_op_without_markers() {
        let text = "# Project\n\nNo shiki section here.\n";
        assert_eq!(remove_agents_md_section(text), text);
    }

    #[test]
    fn find_shiki_mcp_on_path_finds_a_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("shiki-mcp");
        std::fs::write(&bin, "").unwrap();
        let path_env = dir.path().to_string_lossy().to_string();
        assert_eq!(find_shiki_mcp_on_path(&path_env), Some(bin));
    }

    #[test]
    fn find_shiki_mcp_on_path_returns_none_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path_env = dir.path().to_string_lossy().to_string();
        assert_eq!(find_shiki_mcp_on_path(&path_env), None);
    }

    #[test]
    fn supports_scope_matches_each_clients_real_capabilities() {
        assert!(!supports_scope(AgentClient::ClaudeDesktop, Scope::Project));
        assert!(!supports_scope(AgentClient::Windsurf, Scope::Project));
        assert!(!supports_scope(AgentClient::Continue, Scope::User));
        assert!(supports_scope(AgentClient::ClaudeCode, Scope::Project));
        assert!(supports_scope(AgentClient::OpenCode, Scope::User));
    }

    #[test]
    fn continue_gets_its_own_dedicated_project_only_file() {
        assert_eq!(default_scope(AgentClient::Continue), Scope::Project);
        let dir = tempfile::tempdir().unwrap();
        let path = config_path(AgentClient::Continue, Scope::Project, dir.path()).unwrap();
        assert!(path.ends_with(".continue/mcpServers/shiki.yaml"));
    }

    #[test]
    fn connect_and_disconnect_round_trip_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = connect(AgentClient::Cursor, Scope::Project, dir.path()).unwrap();
        assert!(outcome.mcp_config_path.is_file());
        assert!(outcome.guidance_path.is_file());
        let text = std::fs::read_to_string(&outcome.mcp_config_path).unwrap();
        assert!(text.contains("\"shiki\""));

        let disconnected = disconnect(AgentClient::Cursor, Scope::Project, dir.path()).unwrap();
        assert!(disconnected.mcp_removed);
        assert!(disconnected.guidance_removed);
        let text_after = std::fs::read_to_string(&disconnected.mcp_config_path).unwrap();
        assert!(!text_after.contains("\"shiki\""));
    }

    #[test]
    fn connect_writes_a_real_skill_file_for_claude_code() {
        let dir = tempfile::tempdir().unwrap();
        let outcome = connect(AgentClient::ClaudeCode, Scope::Project, dir.path()).unwrap();
        assert!(outcome
            .guidance_path
            .ends_with(".claude/skills/shiki/SKILL.md"));
        let text = std::fs::read_to_string(&outcome.guidance_path).unwrap();
        assert!(text.starts_with("---\nname: shiki"));
    }

    #[test]
    fn status_reports_not_connected_for_a_fresh_project() {
        let dir = tempfile::tempdir().unwrap();
        let st = status(AgentClient::Cursor, Scope::Project, dir.path()).unwrap();
        assert!(!st.file_exists);
        assert!(!st.shiki_connected);
    }

    #[test]
    fn status_reports_connected_after_connect() {
        let dir = tempfile::tempdir().unwrap();
        connect(AgentClient::Cursor, Scope::Project, dir.path()).unwrap();
        let st = status(AgentClient::Cursor, Scope::Project, dir.path()).unwrap();
        assert!(st.file_exists);
        assert!(st.shiki_connected);
    }
}

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::Result;

/// Whichever single entry (folder or note) was highlighted in NOTES at the
/// point the session was saved. A plain `Option<String>` name isn't enough
/// on its own since a folder and a note can share a display name — the
/// `kind` tag disambiguates which list (`Notebook::list_dir`'s folders vs.
/// notes) to look it up in on restore.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectedEntry {
    Folder {
        name: String,
    },
    /// `stem` is the note's filename without its `.md` extension — the same
    /// identity `refresh_notes_preserve_selection` already re-selects by
    /// after a reload, reused here instead of a second matching scheme.
    Note {
        stem: String,
    },
}

/// Persisted "where was I" state for `general.remember_last_session` — which
/// notebook, which folder inside it, which entry was selected, and which
/// panel had focus, restored verbatim on the next launch. Saved once, right
/// before the app exits (see `shiki-tui`'s `App::save_session`); this struct
/// itself has no knowledge of the TUI's `Focus` enum (shiki-config sits
/// below shiki-tui in the dependency chain, see CLAUDE.md's Architecture
/// section), so `focus` is stored as a plain lowercase string
/// (`"notebooks"`/`"notes"`/`"preview"`) and translated on both ends by the
/// caller instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub notebook: String,
    /// Breadcrumb of folder names from the notebook root — empty means the
    /// notebook's root folder itself.
    #[serde(default)]
    pub notes_path: Vec<String>,
    #[serde(default)]
    pub selected: Option<SelectedEntry>,
    #[serde(default)]
    pub focus: String,
}

/// Rejects a `notes_path` component that would escape the notebook when
/// joined onto its real path (`self.path.join(relative)` in
/// `shiki-core::Notebook::list_dir`) — empty, `.`/`..`, or containing a
/// path separator. `notes_path` is restored verbatim from a persisted
/// `session.toml`, so a hand-edited or corrupted file with e.g.
/// `notes_path = ["..", "..", "etc"]` would otherwise let the NOTES panel
/// walk outside the notebook (and outside the data directory entirely) the
/// moment the session is restored. Mirrors `notebook::validate_name`'s
/// rules in `shiki-core` — duplicated rather than shared, since
/// `shiki-config` doesn't depend on `shiki-core` (see CLAUDE.md's
/// Architecture section on the one-way dependency chain).
fn is_safe_path_component(component: &str) -> bool {
    !component.is_empty()
        && component != "."
        && component != ".."
        && !component.contains('/')
        && !component.contains('\\')
}

impl SessionState {
    /// Reads a previously saved session, if the file exists and is valid.
    /// Any failure (missing file, unreadable, malformed TOML) is treated the
    /// same way — no session to restore — rather than surfacing an error;
    /// losing the exact cursor position on a corrupt/missing file is not
    /// worth failing startup over.
    pub fn load(path: &Path) -> Option<Self> {
        let contents = std::fs::read_to_string(path).ok()?;
        let mut session: Self = toml::from_str(&contents).ok()?;
        session
            .notes_path
            .retain(|component| is_safe_path_component(component));
        Some(session)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!(
            "shiki-session-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("session.toml");
        let session = SessionState {
            notebook: "work".into(),
            notes_path: vec!["projects".into(), "shiki".into()],
            selected: Some(SelectedEntry::Note {
                stem: "roadmap".into(),
            }),
            focus: "preview".into(),
        };
        session.save(&path).expect("save must succeed");
        let loaded = SessionState::load(&path).expect("a just-saved session must load back");
        assert_eq!(loaded, session);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_for_a_missing_file() {
        assert!(SessionState::load(Path::new("/nonexistent/shiki-session.toml")).is_none());
    }

    #[test]
    fn load_strips_path_traversal_components_from_notes_path() {
        let dir = std::env::temp_dir().join(format!(
            "shiki-session-traversal-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.toml");
        std::fs::write(
            &path,
            r#"
notebook = "work"
notes_path = ["..", "..", "etc", "projects"]
focus = "notes"
"#,
        )
        .unwrap();

        let loaded = SessionState::load(&path).expect("valid TOML must still load");

        assert_eq!(
            loaded.notes_path,
            vec!["etc".to_string(), "projects".to_string()]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_returns_none_for_malformed_toml() {
        let dir = std::env::temp_dir().join(format!(
            "shiki-session-malformed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.toml");
        std::fs::write(&path, "not valid toml {{{").unwrap();
        assert!(SessionState::load(&path).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}

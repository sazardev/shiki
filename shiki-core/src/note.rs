use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::Result;

/// Normalizes CRLF/CR line endings to plain `\n` before frontmatter parsing.
/// A file round-tripped through an external editor on Windows (or just
/// saved with CRLF endings) otherwise fails `try_parse_frontmatter`'s exact
/// `"---\n"`/`"\n---"` match entirely and silently loses its frontmatter,
/// falling through to `synthesize_frontmatter` as if the file had none.
fn normalize_line_endings(contents: String) -> String {
    if contents.contains('\r') {
        contents.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        contents
    }
}

/// YAML frontmatter at the top of every note.
///
/// `extra` captures any user-defined key beyond the six named fields (e.g.
/// `status: pending`, `priority: 3`) via `#[serde(flatten)]` — without it,
/// an unrecognized key parses fine but is silently dropped on the next
/// `to_file_contents` round-trip, since only the named fields get
/// reserialized. This is what makes a Dataview-style query over frontmatter
/// possible: `shiki_core::query` reads fields out of `extra` the same way it
/// reads the named ones. A note with no `---` block (`synthesize_frontmatter`)
/// always has an empty `extra` — absence is the normal case, not an error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub title: String,
    pub date: NaiveDate,
    #[serde(default)]
    pub tags: Vec<String>,
    pub notebook: String,
    #[serde(default)]
    pub links: Vec<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(flatten)]
    pub extra: serde_yaml::Mapping,
}

impl Frontmatter {
    pub fn new(title: impl Into<String>, notebook: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            date: chrono::Local::now().date_naive(),
            tags: Vec::new(),
            notebook: notebook.into(),
            links: Vec::new(),
            template: None,
            extra: serde_yaml::Mapping::new(),
        }
    }
}

/// A note: path on disk, parsed frontmatter, and markdown body.
#[derive(Debug, Clone)]
pub struct Note {
    pub path: PathBuf,
    pub frontmatter: Frontmatter,
    pub body: String,
}

impl Note {
    pub fn new(path: PathBuf, frontmatter: Frontmatter, body: String) -> Self {
        Self {
            path,
            frontmatter,
            body,
        }
    }

    /// Slug derived from the title: lowercase, spaces -> dashes, no special characters.
    pub fn slugify(title: &str) -> String {
        let mut slug = String::with_capacity(title.len());
        let mut last_was_dash = false;
        for c in title.trim().chars() {
            if c.is_alphanumeric() {
                slug.push(c.to_ascii_lowercase());
                last_was_dash = false;
            } else if !last_was_dash {
                slug.push('-');
                last_was_dash = true;
            }
        }
        slug.trim_matches('-').to_string()
    }

    pub fn file_stem(&self) -> String {
        self.path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    /// Parses a `.md` file. Notes written by shiki have YAML frontmatter
    /// delimited by `---`; anything else (a plain markdown file dropped in
    /// from elsewhere — `nb`, an existing repo, a manual export) is still a
    /// valid note, just without that metadata. Rather than rejecting those,
    /// this synthesizes a title (first `# heading`, else the filename), a
    /// date (the file's mtime), and treats the whole file as the body — see
    /// `synthesize_frontmatter` and `from_file_in_notebook` for the
    /// notebook-aware variant. The only real failure mode left is I/O.
    pub fn from_file(path: &Path) -> Result<Self> {
        Self::from_file_with_crypto(path, None)
    }

    /// Like `from_file`, but decrypts first when `crypto` is given and the
    /// file's content is age-armored (`crypto::looks_encrypted`) — an
    /// encrypted file with no `crypto` provided is a clear
    /// `Error::Encryption`, never treated as a plain note with no
    /// frontmatter (which is what would happen if the armor header were
    /// left for `try_parse_frontmatter`/`synthesize_frontmatter` to deal
    /// with, neither of which know what ciphertext is).
    pub fn from_file_with_crypto(
        path: &Path,
        crypto: Option<&crate::crypto::NotebookCrypto>,
    ) -> Result<Self> {
        let contents = Self::read_and_decrypt(path, crypto)?;
        let (frontmatter, body) = Self::split(path, &contents, None);
        Ok(Self {
            path: path.to_path_buf(),
            frontmatter,
            body,
        })
    }

    /// Like `from_file`, but passes `notebook_name` through to
    /// `synthesize_frontmatter` so the `frontmatter.notebook` field is
    /// correct even for notes nested several folders deep inside the
    /// notebook — the old `synthesize_frontmatter` read the notebook name
    /// from `path.parent().file_name()`, which would pick up an
    /// intermediate folder name instead of the notebook itself.
    pub fn from_file_in_notebook(path: &Path, notebook_name: &str) -> Result<Self> {
        Self::from_file_in_notebook_with_crypto(path, notebook_name, None)
    }

    /// `from_file_in_notebook`, decrypting first when `crypto` is given —
    /// see `from_file_with_crypto`.
    pub fn from_file_in_notebook_with_crypto(
        path: &Path,
        notebook_name: &str,
        crypto: Option<&crate::crypto::NotebookCrypto>,
    ) -> Result<Self> {
        let contents = Self::read_and_decrypt(path, crypto)?;
        let (frontmatter, body) = Self::split(path, &contents, Some(notebook_name));
        Ok(Self {
            path: path.to_path_buf(),
            frontmatter,
            body,
        })
    }

    /// Reads `path` and, if its content is age-armored, decrypts it —
    /// erroring clearly if it's encrypted but no `crypto` (passphrase) was
    /// given, rather than falling through to `synthesize_frontmatter` and
    /// silently treating ciphertext as a plain-text note body.
    fn read_and_decrypt(
        path: &Path,
        crypto: Option<&crate::crypto::NotebookCrypto>,
    ) -> Result<String> {
        let raw = normalize_line_endings(std::fs::read_to_string(path)?);
        if crate::crypto::looks_encrypted(&raw) {
            match crypto {
                Some(c) => c.decrypt(&raw),
                None => Err(crate::Error::Encryption(format!(
                    "'{}' is encrypted — no passphrase available",
                    path.display()
                ))),
            }
        } else {
            Ok(raw)
        }
    }

    fn split(path: &Path, contents: &str, notebook: Option<&str>) -> (Frontmatter, String) {
        Self::try_parse_frontmatter(contents).unwrap_or_else(|| {
            (
                Self::synthesize_frontmatter(path, contents, notebook),
                contents.to_string(),
            )
        })
    }

    /// The closing delimiter must be a *line* consisting of exactly `---`,
    /// not just the first literal occurrence of that substring — a YAML
    /// block scalar (e.g. `title: |`) can legitimately contain a line that
    /// reads `---` as part of its text, and the old `rest.find("\n---")`
    /// would truncate the frontmatter right there, silently losing every
    /// field after it (and failing the parse entirely if that left invalid
    /// YAML behind).
    fn try_parse_frontmatter(contents: &str) -> Option<(Frontmatter, String)> {
        let rest = contents.strip_prefix("---\n")?;
        let mut offset = 0;
        let mut end = None;
        for line in rest.split_inclusive('\n') {
            if line.trim_end_matches('\n') == "---" {
                end = Some(offset);
                break;
            }
            offset += line.len();
        }
        let end = end?;
        let yaml = &rest[..end];
        let body = rest[end + 3..].trim_start_matches('\n').to_string();
        let frontmatter: Frontmatter = serde_yaml::from_str(yaml).ok()?;
        Some((frontmatter, body))
    }

    /// Best-effort metadata for a note that arrived with no frontmatter of
    /// its own. When `notebook` is `Some`, it's used as the `notebook` field
    /// directly; when `None`, falls back to `path.parent().file_name()` for
    /// backward compatibility with callers that don't know the notebook.
    fn synthesize_frontmatter(path: &Path, contents: &str, notebook: Option<&str>) -> Frontmatter {
        let title = contents
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| Self::title_from_filename(path));
        let notebook = match notebook {
            Some(name) => name.to_string(),
            None => path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        };
        let date = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
            .map(|dt| dt.date_naive())
            .unwrap_or_else(|| chrono::Local::now().date_naive());
        Frontmatter {
            title,
            date,
            tags: Vec::new(),
            notebook,
            links: Vec::new(),
            template: None,
            extra: serde_yaml::Mapping::new(),
        }
    }

    fn title_from_filename(path: &Path) -> String {
        path.file_stem()
            .map(|s| s.to_string_lossy().replace(['-', '_'], " "))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    /// Serializes the full note (frontmatter + body) to the on-disk file
    /// format — the plaintext form, always; encryption (if any) happens in
    /// `save_with_crypto`/`save`, one layer further out, since this is also
    /// what the note-history modal needs when displaying an old revision.
    pub fn to_file_contents(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(&self.frontmatter)?;
        Ok(format!("---\n{yaml}---\n\n{}", self.body))
    }

    pub fn save(&self) -> Result<()> {
        self.save_with_crypto(None)
    }

    /// `save`, encrypting the serialized content when `crypto` is given —
    /// the write-side counterpart of `from_file_with_crypto`. A notebook's
    /// encryption setting is a property of the notebook, not the note, so
    /// this always encrypts when `crypto` is `Some` rather than trying to
    /// detect "was this file already encrypted" first.
    pub fn save_with_crypto(&self, crypto: Option<&crate::crypto::NotebookCrypto>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let plaintext = self.to_file_contents()?;
        let contents = match crypto {
            Some(c) => c.encrypt(&plaintext)?,
            None => plaintext,
        };
        std::fs::write(&self.path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_lowercases_and_dashes_special_characters() {
        assert_eq!(Note::slugify("Hello, World!"), "hello-world");
        assert_eq!(Note::slugify("  spaced  out  "), "spaced-out");
        assert_eq!(Note::slugify("Q3 Report"), "q3-report");
    }

    #[test]
    fn slugify_of_symbols_only_title_is_empty() {
        assert_eq!(Note::slugify("!!!"), "");
        assert_eq!(Note::slugify("🎉🎉"), "");
    }

    #[test]
    fn round_trips_frontmatter_and_body_through_to_file_contents() {
        let note = Note::new(
            PathBuf::from("/tmp/does-not-matter.md"),
            Frontmatter::new("My Title", "personal"),
            "Some body text.".to_string(),
        );
        let contents = note.to_file_contents().unwrap();
        let (parsed, body) = Note::try_parse_frontmatter(&contents).unwrap();
        assert_eq!(parsed.title, "My Title");
        assert_eq!(parsed.notebook, "personal");
        assert_eq!(body, "Some body text.");
    }

    #[test]
    fn try_parse_frontmatter_rejects_content_with_no_leading_delimiter() {
        assert!(Note::try_parse_frontmatter("# Just a heading\n\nbody").is_none());
    }

    #[test]
    fn try_parse_frontmatter_does_not_truncate_on_a_literal_dashes_line_inside_yaml() {
        // A YAML block scalar can legitimately contain a line that reads
        // exactly "---" — the closing delimiter search must skip past it
        // and find the real closing "---" line instead of stopping early.
        let contents = "---\ntitle: |\n  Section\n  ---\n  more text\ndate: 2024-01-01\nnotebook: personal\n---\n\nbody here";
        let (fm, body) = Note::try_parse_frontmatter(contents).unwrap();
        assert_eq!(fm.title, "Section\n---\nmore text\n");
        assert_eq!(fm.notebook, "personal");
        assert_eq!(body, "body here");
    }

    #[test]
    fn from_file_normalizes_crlf_before_parsing_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("note.md");
        std::fs::write(
            &path,
            "---\r\ntitle: CRLF Note\r\ndate: 2024-01-01\r\nnotebook: personal\r\n---\r\n\r\nbody line\r\n",
        )
        .unwrap();

        let note = Note::from_file(&path).unwrap();

        assert_eq!(note.frontmatter.title, "CRLF Note");
        assert_eq!(note.frontmatter.notebook, "personal");
        assert_eq!(note.body, "body line\n");
    }

    #[test]
    fn from_file_synthesizes_frontmatter_for_a_plain_markdown_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plain.md");
        std::fs::write(&path, "# A Real Heading\n\nSome content.").unwrap();

        let note = Note::from_file_in_notebook(&path, "work").unwrap();

        assert_eq!(note.frontmatter.title, "A Real Heading");
        assert_eq!(note.frontmatter.notebook, "work");
        assert_eq!(note.body, "# A Real Heading\n\nSome content.");
    }

    #[test]
    fn extra_frontmatter_fields_survive_a_round_trip_with_their_yaml_types() {
        // Spike for shiki_core::query: #[serde(flatten)] on `extra` next to
        // a typed field like `date: NaiveDate` is a known serde_yaml sharp
        // edge (flatten routes deserialization through an internal buffer
        // that has historically mangled scalar types). Assert the typed
        // fields AND the extra fields keep their real YAML types across
        // parse -> reserialize -> reparse, not just that parsing succeeds.
        let contents = "---\n\
            title: T\n\
            date: 2026-08-06\n\
            notebook: work\n\
            status: pending\n\
            priority: 3\n\
            done: false\n\
            due: 2026-08-10\n\
            ---\n\
            \n\
            body";
        let (fm, _) = Note::try_parse_frontmatter(contents).unwrap();
        assert_eq!(fm.title, "T");
        assert_eq!(
            fm.date,
            chrono::NaiveDate::from_ymd_opt(2026, 8, 6).unwrap()
        );
        assert_eq!(fm.notebook, "work");
        assert_eq!(
            fm.extra.get("status").and_then(|v| v.as_str()),
            Some("pending")
        );
        assert_eq!(fm.extra.get("priority").and_then(|v| v.as_i64()), Some(3));
        assert_eq!(fm.extra.get("done").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(
            fm.extra.get("due").and_then(|v| v.as_str()),
            Some("2026-08-10")
        );

        // Round-trip through to_file_contents and reparse.
        let note = Note::new(PathBuf::from("/tmp/x.md"), fm, "body".to_string());
        let written = note.to_file_contents().unwrap();
        let (fm2, _) = Note::try_parse_frontmatter(&written).unwrap();
        assert_eq!(
            fm2.date,
            chrono::NaiveDate::from_ymd_opt(2026, 8, 6).unwrap()
        );
        assert_eq!(fm2.extra.get("priority").and_then(|v| v.as_i64()), Some(3));
        assert_eq!(fm2.extra.get("done").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn synthesized_frontmatter_has_empty_extra() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("plain.md");
        std::fs::write(&path, "no heading here").unwrap();

        let note = Note::from_file_in_notebook(&path, "work").unwrap();

        assert!(note.frontmatter.extra.is_empty());
    }

    #[test]
    fn from_file_falls_back_to_filename_when_no_heading_present() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("my-plain-note.md");
        std::fs::write(&path, "no heading here").unwrap();

        let note = Note::from_file_in_notebook(&path, "work").unwrap();

        assert_eq!(note.frontmatter.title, "my plain note");
    }

    #[test]
    fn save_with_crypto_writes_ciphertext_that_round_trips_back_to_the_note() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("secret.md");
        let crypto = crate::crypto::NotebookCrypto::new("hunter2");

        let note = Note::new(
            path.clone(),
            Frontmatter::new("Secret", "vault"),
            "top secret body".to_string(),
        );
        note.save_with_crypto(Some(&crypto)).unwrap();

        // On disk, it's really ciphertext, not a readable note.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(crate::crypto::looks_encrypted(&raw));
        assert!(!raw.contains("top secret"));

        // With the right passphrase it reads back exactly as written.
        let read_back = Note::from_file_with_crypto(&path, Some(&crypto)).unwrap();
        assert_eq!(read_back.frontmatter.title, "Secret");
        assert_eq!(read_back.body, "top secret body");

        // With no passphrase (or the wrong one) it's a clear error, never
        // silently parsed as a plain note with no frontmatter.
        assert!(Note::from_file_with_crypto(&path, None).is_err());
        let wrong = crate::crypto::NotebookCrypto::new("wrong");
        assert!(Note::from_file_with_crypto(&path, Some(&wrong)).is_err());
    }
}

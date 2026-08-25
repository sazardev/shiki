//! `shiki import` — one-command migration from the two sources people
//! actually have: an existing Obsidian vault (`obsidian`, adopted
//! in-place by default) and a Notion markdown export (`notion`, converted
//! into a fresh notebook under shiki's data dir).

use anyhow::{Context as _, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use shiki_config::config::NotebookGitOverride;
use shiki_config::Config;
use shiki_core::NotebookStore;

/// What `import obsidian` did, for the summary printout.
struct ObsidianReport {
    notes: usize,
    folders: usize,
    assets: usize,
}

pub fn obsidian(
    config: &mut Config,
    store: &NotebookStore,
    raw_path: &str,
    name: Option<&str>,
    copy: bool,
    convert_tags: bool,
    git_init: bool,
) -> Result<()> {
    let src = expand_home(raw_path)?;
    anyhow::ensure!(
        src.is_dir(),
        "'{}' isn't a directory (pass the vault's root folder)",
        src.display()
    );
    let default_name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .context("could not derive a notebook name from that path — pass --name")?;
    let name = name.unwrap_or(&default_name);
    shiki_core::notebook::validate_relative_path(name)
        .with_context(|| format!("'{name}' can't be a notebook name"))?;
    anyhow::ensure!(
        store.get(name).is_err(),
        "a notebook named '{name}' already exists — pass --name to pick another"
    );

    if copy {
        let report = copy_obsidian_into_data_dir(store, &src, name)?;
        println!(
            "imported '{name}' into the data dir: {} notes, {} folders, {} other files",
            report.notes, report.folders, report.assets
        );
        if convert_tags {
            if let Ok(nb) = store.get(name) {
                let (updated, skipped) = convert_inline_tags_reported(&nb.path);
                println!(
                    "{updated} notes gained frontmatter tags; {skipped} plain notes left untouched"
                );
            }
        }
        return Ok(());
    }

    // Adopt in-place: register the vault's own directory as the notebook's
    // path — zero duplication, matching shiki's bring-your-own-notes
    // philosophy ([notebooks.<name>] path).
    let abs = src.canonicalize().unwrap_or_else(|_| src.clone());
    if !abs.join(".git").is_dir() {
        if git_init {
            shiki_core::git::init_repo(&abs)
                .with_context(|| format!("could not git init '{}'", abs.display()))?;
            println!("git repo initialized at '{}'", abs.display());
        } else {
            println!(
                "note: '{}' is not a git repo — re-run with --git-init to enable sync",
                abs.display()
            );
        }
    }
    config.notebooks.insert(
        name.to_string(),
        NotebookGitOverride {
            path: Some(abs.display().to_string()),
            ..Default::default()
        },
    );
    config.save(&Config::default_path()?)?;
    println!(
        "adopted '{}' as notebook '{name}' (registered in config.toml)",
        abs.display()
    );

    let report = survey_obsidian(&abs);
    println!(
        "{} notes, {} folders, {} other files picked up (dot-directories ignored)",
        report.notes, report.folders, report.assets
    );
    if convert_tags {
        let (converted, skipped) = convert_inline_tags_reported(&abs);
        println!("{converted} notes gained frontmatter tags; {skipped} plain notes left untouched (no frontmatter to merge into)");
    } else {
        println!("(tip: re-run with --tags to merge inline #hashtags into frontmatter)");
    }
    Ok(())
}

fn survey_obsidian(root: &Path) -> ObsidianReport {
    let mut report = ObsidianReport {
        notes: 0,
        folders: 0,
        assets: 0,
    };
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| !is_dot_entry(e))
    {
        let Ok(entry) = entry else { continue };
        match entry.file_type() {
            t if t.is_dir() => report.folders += 1,
            _ => {
                if is_note_file(entry.path()) {
                    report.notes += 1;
                } else {
                    report.assets += 1;
                }
            }
        }
    }
    report
}

fn copy_obsidian_into_data_dir(
    store: &NotebookStore,
    src: &Path,
    name: &str,
) -> Result<ObsidianReport> {
    let nb = store.create(name)?;
    let mut report = ObsidianReport {
        notes: 0,
        folders: 0,
        assets: 0,
    };
    copy_tree(src, &nb.path, &mut report)?;
    Ok(report)
}

fn copy_tree(src: &Path, dest: &Path, report: &mut ObsidianReport) -> Result<()> {
    for entry in std::fs::read_dir(src)?.filter_map(|e| e.ok()) {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let from = entry.path();
        let to = dest.join(&name);
        if from.is_dir() {
            std::fs::create_dir_all(&to)?;
            report.folders += 1;
            copy_tree(&from, &to, report)?;
        } else {
            std::fs::copy(&from, &to)?;
            if is_note_file(&from) {
                report.notes += 1;
            } else {
                report.assets += 1;
            }
        }
    }
    Ok(())
}

/// Merges inline hashtags into every frontmatter-carrying note under
/// `root`. Plain files without frontmatter are deliberately untouched —
/// importing must never invent structure inside someone else's vault.
/// Returns (notes updated, plain notes skipped).
fn convert_inline_tags_reported(root: &Path) -> (usize, usize) {
    let mut updated = 0;
    let mut skipped = 0;
    for entry in WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| !is_dot_entry(e))
    {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() || !is_note_file(entry.path()) {
            continue;
        }
        let path = entry.path();
        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        match merge_tags_into_frontmatter(&contents) {
            Some((new_contents, _)) => {
                if std::fs::write(path, new_contents).is_ok() {
                    updated += 1;
                }
            }
            None => skipped += 1,
        }
    }
    (updated, skipped)
}

/// Rewrites one note's frontmatter block so its `tags` also carries every
/// inline `#hashtag` found in the body. Works on the *raw* YAML via a
/// generic mapping round-trip rather than shiki's strict `Frontmatter`
/// struct — foreign vaults routinely omit `notebook:`/`date:` (Obsidian
/// doesn't write them), and parsing through the strict struct would fail,
/// synthesize a brand-new frontmatter, and silently clobber the author's
/// own dates/titles on save. Returns the new file contents plus how many
/// tags were added; `None` means "leave this file alone" (no frontmatter,
/// unparseable YAML, or no inline tags to add).
fn merge_tags_into_frontmatter(contents: &str) -> Option<(String, usize)> {
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

    let mut mapping: serde_yaml::Mapping = serde_yaml::from_str(yaml).ok()?;
    let inline = shiki_core::tags::inline_hashtags(&body);
    if inline.is_empty() {
        return None;
    }
    let existing: Vec<String> = mapping
        .get(serde_yaml::Value::from("tags"))
        .cloned()
        .and_then(|v| serde_yaml::from_value(v).ok())
        .unwrap_or_default();
    let added: Vec<&String> = inline
        .iter()
        .filter(|t| !existing.iter().any(|e| e == *t))
        .collect();
    if added.is_empty() {
        return None;
    }
    let added_count = added.len();
    let mut merged = existing;
    merged.extend(added.into_iter().cloned());
    mapping.insert(
        serde_yaml::Value::from("tags"),
        serde_yaml::Value::from(merged),
    );
    let serialized = serde_yaml::to_string(&mapping).ok()?;
    Some((format!("---\n{serialized}---\n\n{body}"), added_count))
}

fn is_note_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md") | Some("txt") | Some("mdx")
    )
}

fn is_dot_entry(entry: &walkdir::DirEntry) -> bool {
    entry.depth() > 0 && entry.file_name().to_string_lossy().starts_with('.')
}

// ---------------------------------------------------------------------------
// Notion
// ---------------------------------------------------------------------------

/// Notion exports name every page/folder `<Title> <32-hex-uuid>` — strip
/// that suffix wherever it appears.
fn clean_notion_segment(segment: &str) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\s+[0-9a-fA-F]{32}$").unwrap());
    re.replace_all(segment.trim(), "").trim().to_string()
}

/// Minimal percent-decoding for Notion's URL-encoded internal links
/// (`My%20Page%20abc123.md`) — enough for filenames, which is all these
/// ever contain.
fn decode_percent(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(b) = bytes.get(i + 1..i + 3).and_then(|h| {
                std::str::from_utf8(h)
                    .ok()
                    .and_then(|s| u8::from_str_radix(s, 16).ok())
            }) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Rewrites a Notion export body's markdown links that point at other
/// exported pages (`[Text](Folder/Page%20Name%20uuid.md)`) into shiki
/// wikilinks resolved against `titles` (cleaned-stem → cleaned title).
/// Anything that doesn't resolve — images, CSV attachments, external URLs —
/// passes through untouched.
fn rewrite_notion_links(body: &str, titles: &BTreeMap<String, String>) -> (String, usize) {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\[([^\]]*)\]\(([^)\s]+\.md)\)").unwrap());
    let mut rewrites = 0usize;
    let out = re.replace_all(body, |caps: &regex::Captures| {
        let display = caps[1].trim();
        let target = decode_percent(&caps[2]);
        let stem = Path::new(&target)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let cleaned = clean_notion_segment(&stem);
        match titles.get(&cleaned.to_lowercase()) {
            Some(title) => {
                rewrites += 1;
                if display.is_empty() || display.eq_ignore_ascii_case(&cleaned) {
                    format!("[[{title}]]")
                } else {
                    format!("[[{title}|{display}]]")
                }
            }
            None => caps[0].to_string(),
        }
    });
    (out.into_owned(), rewrites)
}

pub fn notion(store: &NotebookStore, raw_path: &str, name: Option<&str>) -> Result<()> {
    let src = expand_home(raw_path)?;
    anyhow::ensure!(src.exists(), "'{}' doesn't exist", src.display());

    // Zip exports get unpacked into a throwaway dir; everything downstream
    // only sees ordinary folders. `scratch` owns that dir for the whole
    // function (a no-op tempdir when the input was already a folder).
    let scratch;
    let root: PathBuf = if src.is_file() && src.extension().and_then(|e| e.to_str()) == Some("zip")
    {
        let tmp = tempfile::Builder::new()
            .prefix("shiki-notion-import-")
            .tempdir()?;
        let root = tmp.path().to_path_buf();
        extract_zip(&src, &root)?;
        scratch = tmp;
        root
    } else if src.is_dir() {
        scratch = tempfile::tempdir()?; // never written to, just keeps types aligned
        src.clone()
    } else {
        anyhow::bail!("'{}' is neither a .zip export nor a folder", src.display());
    };

    let default_name = src
        .file_stem()
        .map(|n| clean_notion_segment(&n.to_string_lossy()))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "notion-import".to_string());
    let name = name.unwrap_or(&default_name);
    shiki_core::notebook::validate_relative_path(name)
        .with_context(|| format!("'{name}' can't be a notebook name"))?;
    anyhow::ensure!(
        store.get(name).is_err(),
        "a notebook named '{name}' already exists — pass --name to pick another"
    );
    let nb = store.create(name)?;

    // First pass: index every exported page's cleaned stem → cleaned title,
    // so second-pass link rewriting can resolve across the whole export.
    let mut titles: BTreeMap<String, String> = BTreeMap::new();
    let mut md_files: Vec<PathBuf> = Vec::new();
    let mut csv_count = 0usize;
    let mut asset_count = 0usize;
    for entry in WalkDir::new(&root).min_depth(1).into_iter().flatten() {
        if !entry.file_type().is_file() || is_dot_entry(&entry) {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        match ext {
            "csv" => csv_count += 1,
            "md" => {
                md_files.push(entry.path().to_path_buf());
                let stem = entry
                    .path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let cleaned = clean_notion_segment(&stem);
                if !cleaned.is_empty() {
                    titles.insert(cleaned.to_lowercase(), cleaned);
                }
            }
            _ => asset_count += 1,
        }
    }

    let mut imported = 0usize;
    let mut folders: BTreeMap<PathBuf, ()> = BTreeMap::new();
    let mut links_rewritten = 0usize;
    let mut failed = 0usize;
    for md in &md_files {
        let rel = match md.strip_prefix(&root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        // Create the destination folders one cleaned segment at a time,
        // remembering which ones already exist so the walk stays cheap and
        // idempotent.
        let rel_parent = rel.parent().unwrap_or(Path::new(""));
        let mut acc = PathBuf::new();
        for seg in rel_parent.components() {
            let seg_name = seg.as_os_str().to_string_lossy().to_string();
            let cleaned_seg = clean_notion_segment(&seg_name);
            acc.push(if cleaned_seg.is_empty() {
                seg_name
            } else {
                cleaned_seg
            });
            if !folders.contains_key(&acc) {
                let parent_of = acc.parent().map(|p| p.to_path_buf()).unwrap_or_default();
                let last = acc
                    .file_name()
                    .map(|s| s.to_os_string())
                    .unwrap_or_default();
                nb.create_folder_in(&parent_of, &last.to_string_lossy())?;
                folders.insert(acc.clone(), ());
            }
        }
        let relative_parent = acc.to_string_lossy().to_string();
        let target_dir = if relative_parent.is_empty() {
            PathBuf::from("")
        } else {
            acc.clone()
        };
        let Ok(body) = std::fs::read_to_string(md) else {
            failed += 1;
            continue;
        };
        let (rewritten, count) = rewrite_notion_links(&body, &titles);
        links_rewritten += count;
        let stem = md
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let title = clean_notion_segment(&stem);
        let title = if title.is_empty() {
            "untitled".to_string()
        } else {
            title
        };
        match nb.create_note_in(&target_dir, &title, rewritten) {
            Ok(_) => imported += 1,
            Err(_) => failed += 1,
        }
    }

    println!(
        "imported '{name}' from the Notion export: {imported} pages, {} folders, \
         {links_rewritten} links converted to wikilinks; {csv_count} CSV databases and \
         {asset_count} other files skipped (Notion databases have no plain-text equivalent)",
        folders.len()
    );
    if failed > 0 {
        println!("{failed} pages could not be read/imported");
    }
    drop(scratch);
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file =
        std::fs::File::open(zip_path).with_context(|| format!("opening {}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("reading {} as a zip archive", zip_path.display()))?;
    archive.extract(dest).context("unpacking the zip export")?;
    Ok(())
}

/// `~/…` expansion for the source path argument — imports usually come from
/// somewhere in the home directory and shell quoting varies.
fn expand_home(raw: &str) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed == "~" || trimmed.starts_with("~/") {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .context("could not resolve '~' (no HOME set)")?;
        let expanded = if trimmed == "~" {
            PathBuf::from(home)
        } else {
            PathBuf::from(home).join(trimmed.trim_start_matches("~/"))
        };
        Ok(expanded)
    } else {
        Ok(PathBuf::from(trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notion_segments_lose_their_uuid_tail() {
        assert_eq!(
            clean_notion_segment("Weekly Plan 0123456789abcdef0123456789abcdef"),
            "Weekly Plan"
        );
        assert_eq!(clean_notion_segment("Just A Page"), "Just A Page");
        // Only a full 32-char hex tail counts — real titles ending in
        // hex-ish words survive.
        assert_eq!(clean_notion_segment("Cafe deadbeef"), "Cafe deadbeef");
    }

    #[test]
    fn percent_decoding_handles_spaces_and_utf8() {
        assert_eq!(
            decode_percent("My%20Page%201234abcd.md"),
            "My Page 1234abcd.md"
        );
        assert_eq!(decode_percent("plain.md"), "plain.md");
        assert_eq!(decode_percent("caf%C3%A9%20menu.md"), "café menu.md");
    }

    #[test]
    fn notion_links_become_wikilinks_only_when_the_target_exists() {
        let titles = BTreeMap::from([("weekly plan".to_string(), "Weekly Plan".to_string())]);
        let body = "See [Weekly Plan](Weekly%20Plan%200123456789abcdef0123456789abcdef.md) \
                    and [Missing](Nowhere%20fedcba98765432100123456789abcdef.md), \
                    plus [an image](pic.png).";
        let (out, count) = rewrite_notion_links(body, &titles);
        assert_eq!(count, 1);
        assert!(out.contains("[[Weekly Plan]]"), "{out}");
        assert!(
            out.contains("[Missing](Nowhere%20fedcba98765432100123456789abcdef.md)"),
            "{out}"
        );
        assert!(out.contains("[an image](pic.png)"), "{out}");
    }

    #[test]
    fn home_expansion_expands_tilde_prefixes_only() {
        assert_eq!(
            expand_home("/abs/path").unwrap(),
            PathBuf::from("/abs/path")
        );
    }

    #[test]
    fn tag_merge_preserves_foreign_frontmatter_verbatim() {
        let contents = "---\ntitle: Welcome\ndate: 2026-01-15\ntags: [start]\n---\n\n# Welcome\n\nbody with #idea here.\n";
        let (out, added) = merge_tags_into_frontmatter(contents).unwrap();
        assert_eq!(added, 1);
        // The author's own keys survive untouched — only tags gains an entry.
        assert!(out.contains("title: Welcome"), "{out}");
        assert!(out.contains("date: 2026-01-15"), "{out}");
        assert!(out.contains("start"), "{out}");
        assert!(out.contains("idea"), "{out}");
        assert!(
            !out.contains("notebook:"),
            "must not invent a notebook field: {out}"
        );
    }

    #[test]
    fn tag_merge_skips_notes_without_frontmatter_or_without_tags_to_add() {
        // No frontmatter at all.
        assert_eq!(merge_tags_into_frontmatter("plain body #tag\n"), None);
        // Frontmatter but nothing new inline.
        let no_new =
            "---\ntitle: T\nnotebook: nb\ndate: 2026-01-01\ntags: [a]\n---\n\nnothing inline\n";
        assert_eq!(merge_tags_into_frontmatter(no_new), None);
        // Already merged (duplicate) -> no-op.
        let dup = "---\ntitle: T\ndate: 2026-01-01\ntags: [idea]\n---\n\n#idea again\n";
        assert_eq!(merge_tags_into_frontmatter(dup), None);
    }
}

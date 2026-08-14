//! Spell-checking by shelling out to `hunspell` — the same external-binary
//! pattern as `publish`'s `pretty-pdf`, so no dictionary is bundled and
//! which language is checked is the system's own. Two operations, both on
//! hunspell's plain command-line interfaces: `check_text` lists a body's
//! misspelled words and maps each back to its byte range in the original
//! text, and `suggestions` fetches corrections for a single word.
//!
//! When hunspell isn't installed both return empty — the caller (the TUI's
//! spell-check pass) checks `hunspell_available()` first and reports the
//! missing binary instead of silently reporting "no misspellings".

use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::process::on_path;

/// One misspelled word found in a body: the word itself plus its `[start,
/// end)` byte range into the text that was checked, so a caller can locate
/// (and later replace) it without re-searching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Misspell {
    pub word: String,
    pub start: usize,
    pub end: usize,
}

/// Whether a `hunspell` binary exists on `$PATH` — the TUI's doctor check
/// and spell-check pass both use this so the "missing binary" case is a
/// friendly message, not a parse of an empty result.
pub fn hunspell_available() -> bool {
    on_path("hunspell")
}

fn base_cmd(lang: Option<&str>) -> Option<Command> {
    if !hunspell_available() {
        return None;
    }
    let mut cmd = Command::new("hunspell");
    if let Some(lang) = lang.filter(|l| !l.trim().is_empty()) {
        cmd.args(["-d", lang.trim()]);
    }
    Some(cmd)
}

/// Every misspelled word in `text`, with its byte range. `lang` selects a
/// dictionary (`-d`, e.g. `es_ES`); `None` uses the system default. Empty
/// result when hunspell is unavailable or `text` has no misspellings — call
/// `hunspell_available()` first to tell those apart.
pub fn check_text(text: &str, lang: Option<&str>) -> crate::Result<Vec<Misspell>> {
    let Some(mut cmd) = base_cmd(lang) else {
        return Ok(Vec::new());
    };
    cmd.arg("-l").stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| crate::Error::Spell(e.to_string()))?;
    child
        .stdin
        .take()
        .expect("stdin was configured piped")
        .write_all(text.as_bytes())
        .map_err(|e| crate::Error::Spell(e.to_string()))?;
    let output = child
        .wait_with_output()
        .map_err(|e| crate::Error::Spell(e.to_string()))?;
    if !output.status.success() {
        return Err(crate::Error::Spell(format!(
            "hunspell exited with {status}",
            status = output.status
        )));
    }
    let words: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    Ok(map_words_to_offsets(text, &words))
}

/// Maps each word reported by `hunspell -l` back to the first unclaimed
/// occurrence in `text` (a repeated word that's actually spelled right
/// somewhere still gets flagged for each real misspelling it appears as).
/// Words hunspell reports but that can't be found (a stale line, or a
/// match inside a larger token) are dropped rather than guessed at.
fn map_words_to_offsets(text: &str, words: &[String]) -> Vec<Misspell> {
    let mut claimed: HashSet<usize> = HashSet::new();
    let mut out = Vec::new();
    for word in words {
        if word.is_empty() {
            continue;
        }
        let mut search_from = 0;
        while let Some(rel) = text[search_from..].find(word.as_str()) {
            let start = search_from + rel;
            if claimed.insert(start) {
                out.push(Misspell {
                    word: word.clone(),
                    start,
                    end: start + word.len(),
                });
                break;
            }
            search_from = start + 1;
        }
    }
    out.sort_by_key(|m| m.start);
    out
}

/// Correction candidates for one word, via hunspell's `-a` (ispell
/// compatibility) mode: the `&` response line lists suggestions after `:`.
/// Empty when hunspell is missing or offers nothing.
pub fn suggestions(word: &str, lang: Option<&str>) -> Vec<String> {
    let Some(mut cmd) = base_cmd(lang) else {
        return Vec::new();
    };
    cmd.arg("-a").stdin(Stdio::piped()).stdout(Stdio::piped());
    let Ok(mut child) = cmd.spawn() else {
        return Vec::new();
    };
    let _ = child.stdin.take().map(|mut stdin| {
        let _ = stdin.write_all(format!("{word}\n").as_bytes());
    });
    let Ok(output) = child.wait_with_output() else {
        return Vec::new();
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find(|l| l.starts_with('&'))
        .and_then(|l| l.split(':').nth(1))
        .map(|list| {
            list.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_words_to_first_unclaimed_occurrences() {
        let text = "ab cd ab ef";
        let words = vec!["ab".to_string(), "ab".to_string(), "zz".to_string()];
        let misses = map_words_to_offsets(text, &words);
        // "zz" isn't in the text, so it's dropped; the two "ab" land on the
        // first and third tokens.
        assert_eq!(misses.len(), 2);
        assert_eq!(&text[misses[0].start..misses[0].end], "ab");
        assert_eq!(&text[misses[1].start..misses[1].end], "ab");
        assert!(misses[0].start < misses[1].start);
        assert!(misses.iter().all(|m| m.start < m.end));
    }

    #[test]
    fn sorts_misses_by_position() {
        let text = "zz aa zz bb";
        let words = vec!["bb".to_string(), "zz".to_string()];
        let misses = map_words_to_offsets(text, &words);
        // "zz" appears twice; first claimed occurrence is at 0, and "bb" at 9.
        assert_eq!(&text[misses[0].start..misses[0].end], "zz");
        assert_eq!(&text[misses[1].start..misses[1].end], "bb");
        assert_eq!(misses.len(), 2);
    }

    #[test]
    fn missing_words_are_dropped() {
        let misses = map_words_to_offsets("only correct words here", &["nope".to_string()]);
        assert!(misses.is_empty());
    }
}

//! Attachment filename helpers shared by every consumer that saves a
//! pasted image into a notebook's attachments folder — `shiki-tui`'s
//! clipboard paste and `shiki-desktop`'s browser-Clipboard-API bridge used
//! to each carry an identical, independently-maintained copy of the
//! collision-suffix logic below.

use std::path::{Path, PathBuf};

/// `stem.png`, or `stem-2.png`/`stem-3.png`/… past the first collision —
/// never overwrites an existing file.
pub fn unique_file(dir: &Path, stem: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.png"));
    if !first.exists() {
        return first;
    }
    for n in 2.. {
        let candidate = dir.join(format!("{stem}-{n}.png"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_call_uses_the_bare_stem() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            unique_file(tmp.path(), "pasted"),
            tmp.path().join("pasted.png")
        );
    }

    #[test]
    fn colliding_names_get_a_numeric_suffix_not_an_overwrite() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("pasted.png"), b"occupied").unwrap();

        let second = unique_file(tmp.path(), "pasted");

        assert_eq!(second, tmp.path().join("pasted-2.png"));
        std::fs::write(&second, b"occupied too").unwrap();
        assert_eq!(
            unique_file(tmp.path(), "pasted"),
            tmp.path().join("pasted-3.png")
        );
    }
}

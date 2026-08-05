//! Markdown ATX headings (`#` through `######`) extracted from a note's
//! body, for the outline-jump modal (`shiki-tui`'s PREVIEW-scope `o` /
//! `Mode::Edit`'s Ctrl+O). `extract` is a pure function of a body string,
//! same shape as `tasks::extract` — unit-testable without touching disk.

use std::sync::OnceLock;

use regex::Regex;

/// One heading line found in a note's body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    /// 0-based line index within the body, for `CursorMove::Jump`/preview
    /// scroll positioning.
    pub line: usize,
    /// 1-6, the number of leading `#` characters.
    pub level: u8,
    /// The heading text with the leading `#`s and surrounding whitespace
    /// stripped.
    pub text: String,
}

fn heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(#{1,6})\s+(.*)$").unwrap())
}

/// Every ATX heading in `body`, in document order.
pub fn extract(body: &str) -> Vec<Heading> {
    body.lines()
        .enumerate()
        .filter_map(|(line, text)| {
            let caps = heading_re().captures(text)?;
            Some(Heading {
                line,
                level: caps[1].len() as u8,
                text: caps[2].trim_end().to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_headings_in_order() {
        let body = "# Title\n\nSome text\n## Section\ntext\n### Sub\n";
        let headings = extract(body);
        assert_eq!(headings.len(), 3);
        assert_eq!(
            headings[0],
            Heading {
                line: 0,
                level: 1,
                text: "Title".into()
            }
        );
        assert_eq!(
            headings[1],
            Heading {
                line: 3,
                level: 2,
                text: "Section".into()
            }
        );
        assert_eq!(
            headings[2],
            Heading {
                line: 5,
                level: 3,
                text: "Sub".into()
            }
        );
    }

    #[test]
    fn ignores_non_heading_hashes() {
        let body = "no heading here\n#nohash\n#### Real heading";
        let headings = extract(body);
        assert_eq!(headings.len(), 1);
        assert_eq!(headings[0].text, "Real heading");
    }

    #[test]
    fn trims_trailing_whitespace_from_text() {
        let body = "## Section   \n";
        let headings = extract(body);
        assert_eq!(headings[0].text, "Section");
    }

    #[test]
    fn empty_body_has_no_headings() {
        assert!(extract("").is_empty());
    }
}

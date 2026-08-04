use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

use crate::note::Note;

/// Fuzzy search result: index into the searched slice + score.
#[derive(Debug, Clone, Copy)]
pub struct SearchHit {
    pub index: usize,
    pub score: u32,
}

/// Fuzzy search engine over note titles, using the same matcher as Helix.
pub struct SearchEngine {
    matcher: Matcher,
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
        }
    }
}

impl SearchEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Searches `query` across the titles of `notes`, returning hits sorted by descending score.
    pub fn search(&mut self, query: &str, notes: &[Note]) -> Vec<SearchHit> {
        let haystacks: Vec<&str> = notes.iter().map(|n| n.frontmatter.title.as_str()).collect();
        self.search_text(query, &haystacks)
    }

    /// Searches `query` against arbitrary `haystacks` (e.g. title+body combined
    /// for a full-text search across notes), returning hits sorted by
    /// descending score. `index` in each hit refers back into `haystacks`.
    pub fn search_text(&mut self, query: &str, haystacks: &[&str]) -> Vec<SearchHit> {
        if query.is_empty() {
            return (0..haystacks.len())
                .map(|index| SearchHit { index, score: 0 })
                .collect();
        }
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        // One scratch buffer reused across every haystack (`.clear()` keeps
        // its allocated capacity instead of freeing it) rather than a fresh
        // `Vec::new()` per haystack inside the loop — global search re-runs
        // this on every keystroke across every note in the notebook, so a
        // per-note allocation here is a real, avoidable cost at scale.
        let mut buf = Vec::new();
        let mut hits: Vec<SearchHit> = haystacks
            .iter()
            .enumerate()
            .filter_map(|(index, text)| {
                buf.clear();
                let utf32 = nucleo_matcher::Utf32Str::new(text, &mut buf);
                pattern
                    .score(utf32, &mut self.matcher)
                    .map(|score| SearchHit { index, score })
            })
            .collect();
        hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
        hits
    }
}

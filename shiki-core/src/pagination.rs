//! Shared page-slicing and `--json`/tool-result shaping for any surface
//! whose result set can be unboundedly large (`shiki list`/`search`/
//! `query`/`tasks`/`log` at the CLI, and their MCP-tool equivalents) — one
//! implementation so a caller that learns to page through one of these
//! pages through every other one the same way, on either interface.

use serde_json::Value;

/// The page size every paginated caller falls back to when neither an
/// explicit limit nor "no limit" is requested — a safe default so a large
/// notebook/knowledge base can never be dumped into a script's or an AI
/// agent's context in one call by accident.
pub const DEFAULT_PAGE_LIMIT: usize = 50;

/// Resolves the effective page limit from the two mutually-exclusive
/// inputs every paginated command exposes: `no_limit` (`None`, i.e.
/// "everything") always wins when `true`; otherwise `limit` if given, else
/// `DEFAULT_PAGE_LIMIT`.
pub fn effective_limit(limit: Option<usize>, no_limit: bool) -> Option<usize> {
    if no_limit {
        None
    } else {
        Some(limit.unwrap_or(DEFAULT_PAGE_LIMIT))
    }
}

/// Slices `items` to `[offset, offset+limit)` (or from `offset` to the end
/// when `limit` is `None`) and returns `(page, total)` — `total` is the
/// count *before* slicing, so callers can report whether more remain
/// without a second query. Takes ownership rather than a slice
/// specifically so callers don't pay for cloning items that never make it
/// onto the page (`Vec::into_iter().skip().take()` moves instead of
/// copying).
pub fn paginate<T>(items: Vec<T>, offset: usize, limit: Option<usize>) -> (Vec<T>, usize) {
    let total = items.len();
    let page: Vec<T> = match limit {
        Some(l) => items.into_iter().skip(offset).take(l).collect(),
        None => items.into_iter().skip(offset).collect(),
    };
    (page, total)
}

/// Wraps an already-paginated `items` array (the JSON-mapped page, not the
/// raw domain objects) with the metadata a caller needs to know whether to
/// keep paging — `{"total", "offset", "limit", "returned", "has_more",
/// "items"}` — the one shape every paginated CLI command's `--json` output
/// and every equivalent MCP tool result uses.
pub fn page_json(items: Vec<Value>, total: usize, offset: usize, limit: Option<usize>) -> Value {
    let returned = items.len();
    serde_json::json!({
        "total": total,
        "offset": offset,
        "limit": limit,
        "returned": returned,
        "has_more": offset + returned < total,
        "items": items,
    })
}

/// The plain-text footer printed under a truncated page — `None` when
/// everything that matched fit on this one page, so callers can just
/// `if let Some(line) = page_footer(...) { println!("{line}"); }`
/// unconditionally after printing the page's rows.
pub fn page_footer(shown: usize, offset: usize, total: usize) -> Option<String> {
    if offset + shown >= total {
        return None;
    }
    Some(format!(
        "\u{2014} showing {}\u{2013}{} of {total} \u{2014} continue with --offset {} (or --no-limit for everything)",
        offset + 1,
        offset + shown,
        offset + shown,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginate_with_a_limit_slices_the_middle() {
        let (page, total) = paginate(vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9], 3, Some(4));
        assert_eq!(page, vec![3, 4, 5, 6]);
        assert_eq!(total, 10);
    }

    #[test]
    fn paginate_with_no_limit_returns_everything_from_the_offset() {
        let (page, total) = paginate(vec![0, 1, 2, 3, 4], 2, None);
        assert_eq!(page, vec![2, 3, 4]);
        assert_eq!(total, 5);
    }

    #[test]
    fn paginate_past_the_end_is_an_empty_page_not_a_panic() {
        let (page, total) = paginate(vec![0, 1, 2], 10, Some(5));
        assert!(page.is_empty());
        assert_eq!(total, 3);
    }

    #[test]
    fn page_footer_is_none_when_the_page_covers_everything() {
        assert_eq!(page_footer(10, 0, 10), None);
        assert_eq!(page_footer(3, 7, 10), None);
    }

    #[test]
    fn page_footer_names_the_next_offset_when_more_remain() {
        assert_eq!(
            page_footer(50, 0, 532),
            Some(
                "\u{2014} showing 1\u{2013}50 of 532 \u{2014} continue with --offset 50 (or --no-limit for everything)"
                    .to_string()
            )
        );
    }

    #[test]
    fn effective_limit_no_limit_wins_over_an_explicit_limit() {
        assert_eq!(effective_limit(Some(10), true), None);
    }

    #[test]
    fn effective_limit_uses_the_given_limit_or_falls_back_to_the_default() {
        assert_eq!(effective_limit(Some(10), false), Some(10));
        assert_eq!(effective_limit(None, false), Some(DEFAULT_PAGE_LIMIT));
    }
}

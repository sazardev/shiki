//! A small Dataview-style query language over note frontmatter — `where
//! status = pending sort due asc`. Parsed by a hand-rolled recursive-descent
//! parser (no parser-combinator dependency, consistent with `tasks.rs`'s own
//! regex-based extraction), then evaluated against the in-memory note pool.
//! `parse`/`run_query` are pure functions of already-loaded data — no I/O —
//! so both `shiki query` (CLI) and the TUI query modal can share them and
//! can never disagree about what a given query means.

use std::cmp::Ordering;
use std::path::PathBuf;

use chrono::NaiveDate;

use crate::{Note, Notebook};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    /// `~` — case-insensitive substring match.
    Like,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    /// Resolved against the `today` passed into `run_query` at evaluation
    /// time, never at parse time — a saved query's `today` must mean the day
    /// it's *run*, not the day it was written.
    Today,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Cmp {
        field: String,
        op: Op,
        value: Value,
    },
    /// `contains "text"` — a free-text substring match over title + body,
    /// case-insensitive.
    Contains(String),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub filter: Option<Expr>,
    pub sort: Option<(String, SortDir)>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum QueryError {
    #[error("unterminated string — missing a closing '\"'")]
    UnterminatedString,
    #[error("unexpected character '{0}' — supported operators are = != > < >= <= ~")]
    UnexpectedChar(char),
    #[error("expected a field name (e.g. title, date, notebook, tag, template, or any custom frontmatter field)")]
    ExpectedField,
    #[error("expected a comparison operator (=, !=, >, <, >=, <=, ~) after the field name")]
    ExpectedOperator,
    #[error(
        "expected a value after the operator — a word, \"quoted string\", a number, or `today`"
    )]
    ExpectedValue,
    #[error("expected a closing ')'")]
    ExpectedCloseParen,
    #[error("unexpected trailing input — did you forget `and`/`or` between conditions?")]
    TrailingTokens,
    #[error("empty query — try something like: where status = pending sort due asc")]
    Empty,
}

// ---------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    LParen,
    RParen,
    Op(Op),
}

fn tokenize(input: &str) -> Result<Vec<Tok>, QueryError> {
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                tokens.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Tok::RParen);
                i += 1;
            }
            '"' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(QueryError::UnterminatedString);
                }
                tokens.push(Tok::Str(chars[start..i].iter().collect()));
                i += 1;
            }
            '=' => {
                tokens.push(Tok::Op(Op::Eq));
                i += 1;
            }
            '~' => {
                tokens.push(Tok::Op(Op::Like));
                i += 1;
            }
            '!' if chars.get(i + 1) == Some(&'=') => {
                tokens.push(Tok::Op(Op::Ne));
                i += 2;
            }
            '>' if chars.get(i + 1) == Some(&'=') => {
                tokens.push(Tok::Op(Op::Ge));
                i += 2;
            }
            '>' => {
                tokens.push(Tok::Op(Op::Gt));
                i += 1;
            }
            '<' if chars.get(i + 1) == Some(&'=') => {
                tokens.push(Tok::Op(Op::Le));
                i += 2;
            }
            '<' => {
                tokens.push(Tok::Op(Op::Lt));
                i += 1;
            }
            _ => {
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !"()=!><~\"".contains(chars[i])
                {
                    i += 1;
                }
                if start == i {
                    return Err(QueryError::UnexpectedChar(c));
                }
                tokens.push(Tok::Word(chars[start..i].iter().collect()));
            }
        }
    }
    Ok(tokens)
}

// ---------------------------------------------------------------------
// Parser (recursive descent: or_expr -> and_expr -> cmp)
// ---------------------------------------------------------------------

struct Parser<'a> {
    toks: &'a [Tok],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<&Tok> {
        let t = self.toks.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn peek_word_lower(&self) -> Option<String> {
        match self.peek() {
            Some(Tok::Word(w)) => Some(w.to_ascii_lowercase()),
            _ => None,
        }
    }

    /// Consumes the next token if it's the given keyword (case-insensitive).
    fn eat_word(&mut self, kw: &str) -> bool {
        if self.peek_word_lower().as_deref() == Some(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_or(&mut self) -> Result<Expr, QueryError> {
        let mut left = self.parse_and()?;
        while self.eat_word("or") {
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, QueryError> {
        let mut left = self.parse_cmp()?;
        while self.eat_word("and") {
            let right = self.parse_cmp()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_cmp(&mut self) -> Result<Expr, QueryError> {
        if matches!(self.peek(), Some(Tok::LParen)) {
            self.next();
            let e = self.parse_or()?;
            match self.next() {
                Some(Tok::RParen) => {}
                _ => return Err(QueryError::ExpectedCloseParen),
            }
            return Ok(e);
        }
        if self.eat_word("contains") {
            let s = self.parse_value_string()?;
            return Ok(Expr::Contains(s));
        }
        let field = match self.next() {
            Some(Tok::Word(w)) => w.clone(),
            _ => return Err(QueryError::ExpectedField),
        };
        let op = match self.next() {
            Some(Tok::Op(o)) => *o,
            _ => return Err(QueryError::ExpectedOperator),
        };
        let value = self.parse_value()?;
        Ok(Expr::Cmp { field, op, value })
    }

    fn parse_value(&mut self) -> Result<Value, QueryError> {
        match self.next() {
            Some(Tok::Str(s)) => Ok(Value::Str(s.clone())),
            Some(Tok::Word(w)) => {
                if w.eq_ignore_ascii_case("today") {
                    Ok(Value::Today)
                } else if let Ok(n) = w.parse::<f64>() {
                    Ok(Value::Num(n))
                } else {
                    Ok(Value::Str(w.clone()))
                }
            }
            _ => Err(QueryError::ExpectedValue),
        }
    }

    fn parse_value_string(&mut self) -> Result<String, QueryError> {
        match self.next() {
            Some(Tok::Str(s)) => Ok(s.clone()),
            Some(Tok::Word(w)) => Ok(w.clone()),
            _ => Err(QueryError::ExpectedValue),
        }
    }
}

/// Parses a query DSL string. Empty/whitespace-only input is `Err(Empty)`
/// rather than an all-notes-no-filter query, so an empty search box in the
/// TUI can distinguish "nothing typed yet" from "a query that matches all".
pub fn parse(input: &str) -> Result<Query, QueryError> {
    if input.trim().is_empty() {
        return Err(QueryError::Empty);
    }
    let toks = tokenize(input)?;
    let mut p = Parser {
        toks: &toks,
        pos: 0,
    };

    let filter = if p.eat_word("where") {
        Some(p.parse_or()?)
    } else {
        None
    };

    let sort = if p.eat_word("sort") {
        let field = match p.next() {
            Some(Tok::Word(w)) => w.clone(),
            _ => return Err(QueryError::ExpectedField),
        };
        let dir = if p.eat_word("desc") {
            SortDir::Desc
        } else {
            p.eat_word("asc");
            SortDir::Asc
        };
        Some((field, dir))
    } else {
        None
    };

    if p.pos != p.toks.len() {
        return Err(QueryError::TrailingTokens);
    }
    if filter.is_none() && sort.is_none() {
        return Err(QueryError::ExpectedField);
    }
    Ok(Query { filter, sort })
}

// ---------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------

fn yaml_value_to_string(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

/// The six named frontmatter fields plus anything in `extra`, as a plain
/// string — `None` when the field is genuinely absent (never an error; see
/// module doc). `tags` is deliberately excluded here — `tag`/`tags`
/// comparisons test membership instead, handled separately in `eval_cmp`.
fn field_string(note: &Note, field: &str) -> Option<String> {
    let fm = &note.frontmatter;
    match field.to_ascii_lowercase().as_str() {
        "title" => Some(fm.title.clone()),
        "date" => Some(fm.date.format("%Y-%m-%d").to_string()),
        "notebook" => Some(fm.notebook.clone()),
        "template" => fm.template.clone(),
        _ => fm.extra.get(field).map(yaml_value_to_string),
    }
}

fn value_to_string(v: &Value, today: NaiveDate) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Num(n) => {
            if n.fract() == 0.0 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        Value::Today => today.format("%Y-%m-%d").to_string(),
    }
}

/// Coercion ladder: date -> number -> case-insensitive string. Unquoted YAML
/// scalars like `due: 2026-08-10` parse as plain strings (`serde_yaml`
/// doesn't infer a date type), so a textual `due < today` comparison would
/// otherwise compare "2026-08-10" and "2026-08-06" lexicographically — which
/// happens to work for same-length ISO dates, but only by accident. Trying a
/// real date parse first makes that correctness explicit rather than
/// incidental.
fn compare_ordering(a: &str, b: &str) -> Ordering {
    if let (Ok(da), Ok(db)) = (
        NaiveDate::parse_from_str(a, "%Y-%m-%d"),
        NaiveDate::parse_from_str(b, "%Y-%m-%d"),
    ) {
        return da.cmp(&db);
    }
    if let (Ok(na), Ok(nb)) = (a.parse::<f64>(), b.parse::<f64>()) {
        return na.partial_cmp(&nb).unwrap_or(Ordering::Equal);
    }
    a.to_lowercase().cmp(&b.to_lowercase())
}

fn apply_op(op: Op, ord: Ordering) -> bool {
    match op {
        Op::Eq => ord == Ordering::Equal,
        Op::Ne => ord != Ordering::Equal,
        Op::Gt => ord == Ordering::Greater,
        Op::Lt => ord == Ordering::Less,
        Op::Ge => ord != Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Like => unreachable!("Like is handled separately in eval_cmp, never via apply_op"),
    }
}

fn eval_cmp(field: &str, op: Op, value: &Value, note: &Note, today: NaiveDate) -> bool {
    if field.eq_ignore_ascii_case("tag") || field.eq_ignore_ascii_case("tags") {
        let needle = value_to_string(value, today);
        let has = note
            .frontmatter
            .tags
            .iter()
            .any(|t| t.eq_ignore_ascii_case(&needle));
        return match op {
            Op::Ne => !has,
            _ => has,
        };
    }
    let value_str = value_to_string(value, today);
    match field_string(note, field) {
        // Absent field: `=`/ordering/`~` never match; `!=` always does —
        // "not equal to X" is true of a field that isn't even there.
        None => matches!(op, Op::Ne),
        Some(field_val) => {
            if op == Op::Like {
                field_val.to_lowercase().contains(&value_str.to_lowercase())
            } else {
                apply_op(op, compare_ordering(&field_val, &value_str))
            }
        }
    }
}

fn eval_expr(expr: &Expr, note: &Note, today: NaiveDate) -> bool {
    match expr {
        Expr::And(a, b) => eval_expr(a, note, today) && eval_expr(b, note, today),
        Expr::Or(a, b) => eval_expr(a, note, today) || eval_expr(b, note, today),
        Expr::Contains(text) => {
            let haystack = format!("{} {}", note.frontmatter.title, note.body).to_lowercase();
            haystack.contains(&text.to_lowercase())
        }
        Expr::Cmp { field, op, value } => eval_cmp(field, *op, value, note, today),
    }
}

/// One note matched by a query — the row shape both the CLI and the TUI
/// modal render. `fields` is the note's own `extra` frontmatter map, so a
/// caller can display exactly the columns a query actually referenced (or
/// just show everything) without re-parsing the note.
#[derive(Debug, Clone)]
pub struct QueryRow {
    pub location: String,
    pub notebook: String,
    pub note_title: String,
    pub path: PathBuf,
    pub fields: serde_yaml::Mapping,
}

/// Runs `q` over `pool` (typically `NotebookStore::all_notes()`, loaded once
/// by the caller — see the TUI modal, which loads it on open and re-runs
/// only this function per keystroke rather than re-walking the filesystem).
/// `notebook` narrows to one notebook by name, matching every other
/// notebook-scoped command's `Option<&str>` convention (`shiki tasks
/// -n <name>`, `shiki graph -n <name>`).
pub fn run_query(
    pool: &[(Notebook, Note)],
    q: &Query,
    notebook: Option<&str>,
    today: NaiveDate,
) -> Vec<QueryRow> {
    let mut matched: Vec<&(Notebook, Note)> = pool
        .iter()
        .filter(|(nb, _)| notebook.is_none_or(|w| nb.name == w))
        .filter(|(_, note)| q.filter.as_ref().is_none_or(|e| eval_expr(e, note, today)))
        .collect();

    if let Some((field, dir)) = &q.sort {
        matched.sort_by(|(_, a), (_, b)| {
            let av = field_string(a, field);
            let bv = field_string(b, field);
            let ord = match (&av, &bv) {
                (None, None) => Ordering::Equal,
                // Absent sort key sinks to the bottom regardless of
                // direction — same convention as `panel_tasks::build`'s
                // `sort_by_key(|r| (due.is_none(), due))`.
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => compare_ordering(a, b),
            };
            match dir {
                SortDir::Asc => ord,
                SortDir::Desc if av.is_none() || bv.is_none() => ord,
                SortDir::Desc => ord.reverse(),
            }
        });
    }

    matched
        .into_iter()
        .map(|(nb, note)| QueryRow {
            location: crate::tasks::location_of(nb, note),
            notebook: nb.name.clone(),
            note_title: note.frontmatter.title.clone(),
            path: note.path.clone(),
            fields: note.frontmatter.extra.clone(),
        })
        .collect()
}

/// Every custom frontmatter field name actually present across `pool`'s
/// notes (their `extra` maps) — deduped, sorted. Used to build a "here's
/// what you can actually query on" hint next to a parse error, in both the
/// CLI (`shiki query`) and the TUI query modals — pool-driven rather than a
/// fixed list, since custom fields are whatever a user happened to write.
pub fn known_fields(pool: &[(Notebook, Note)]) -> Vec<String> {
    let mut fields: Vec<String> = pool
        .iter()
        .flat_map(|(_, note)| note.frontmatter.extra.keys())
        .filter_map(|k| k.as_str().map(str::to_string))
        .collect();
    fields.sort();
    fields.dedup();
    fields
}

/// The always-available fields (not stored per-note) plus the DSL's basic
/// shape — shown alongside `known_fields` so the hint is useful even for a
/// notebook with no custom frontmatter fields at all yet.
pub const BUILTIN_FIELDS: &str = "title, date, notebook, tag, template";
pub const EXAMPLE_QUERY: &str = "where status = pending sort due asc";

/// Every distinct value actually seen for exactly `field` across `pool`'s
/// notes (their `extra` maps) — deduped, sorted. Unlike `known_fields`
/// (field *names*, across every field) or `suggest_queries` (whole example
/// DSL strings), this answers "what have I already put in this one field
/// before" — used by the TUI's metadata editor to suggest previously-used
/// values (e.g. every `project` name used anywhere) when editing a field.
pub fn field_values(pool: &[(Notebook, Note)], field: &str) -> Vec<String> {
    let mut values: Vec<String> = pool
        .iter()
        .filter_map(|(_, note)| note.frontmatter.extra.get(field))
        .map(yaml_value_to_string)
        .filter(|v| !v.is_empty())
        .collect();
    values.sort();
    values.dedup();
    values
}

fn quote_if_needed(v: &str) -> String {
    if !v.is_empty()
        && v.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        v.to_string()
    } else {
        format!("{v:?}")
    }
}

/// A big, varied set of ready-to-run example queries generated from the
/// fields and values actually present in `pool` — shown the moment query
/// mode opens (before anything's typed), and used as a live "did you mean"
/// list while an in-progress query doesn't parse yet (see the TUI's
/// `refresh_query`/`refresh_global_search`). Genuinely reads what's in the
/// notes rather than being a fixed hardcoded example, so it stays useful
/// for a notebook with a completely different set of custom fields —
/// deliberately generous rather than a curated top few: `App::
/// matching_suggestions` already narrows this live as the user types, so a
/// long list here costs nothing once someone's typing "proj" and only the
/// project-related ones remain, but *not* generating a value (e.g. every
/// distinct `project` name, not just the two most common) would make that
/// filtering come up empty for exactly the thing being typed.
pub fn suggest_queries(pool: &[(Notebook, Note)]) -> Vec<String> {
    let mut values: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (_, note) in pool {
        for (k, v) in note.frontmatter.extra.iter() {
            let Some(key) = k.as_str() else { continue };
            let val = yaml_value_to_string(v);
            if val.is_empty() {
                continue;
            }
            let entry = values.entry(key.to_string()).or_default();
            if !entry.contains(&val) {
                entry.push(val);
            }
        }
    }

    // Fields with more distinct values sort first — a field with only one
    // value everywhere still gets its `where`/`sort` examples, just later
    // in the list, since a single-value field is still meaningful to sort
    // or combine with `and`/`or`, just not as interesting to filter alone.
    let mut fields: Vec<(&String, &Vec<String>)> = values.iter().collect();
    fields.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));

    let today = chrono::Local::now().date_naive();
    let mut examples = Vec::new();

    for (field, vals) in &fields {
        // Every distinct value gets its own example — "por todos los
        // projects", not just the top couple — so typing any real value
        // this notebook actually uses finds a matching suggestion.
        for val in vals.iter() {
            examples.push(format!("where {field} = {}", quote_if_needed(val)));
        }
        // Sorting is meaningful for any field, not just dates — both
        // directions, since "asc"/"desc" are themselves things a user
        // might type looking for a sort example.
        examples.push(format!("sort {field} asc"));
        examples.push(format!("sort {field} desc"));

        if vals.iter().any(|v| is_iso_date(v)) {
            let in_a_week = (today + chrono::Duration::days(7)).format("%Y-%m-%d");
            let in_a_month = (today + chrono::Duration::days(30)).format("%Y-%m-%d");
            examples.push(format!("where {field} = today"));
            examples.push(format!("where {field} < today"));
            examples.push(format!("where {field} > today"));
            examples.push(format!("where {field} >= today and {field} <= {in_a_week}"));
            examples.push(format!(
                "where {field} >= today and {field} <= {in_a_month}"
            ));
        }
    }

    // Combine the two most-varied fields, and offer an `or` between the
    // top field's own two most common values.
    if let [(f1, v1), (f2, v2), ..] = fields.as_slice() {
        if let (Some(a), Some(b)) = (v1.first(), v2.first()) {
            examples.push(format!(
                "where {f1} = {} and {f2} = {}",
                quote_if_needed(a),
                quote_if_needed(b)
            ));
        }
    }
    if let Some((field, vals)) = fields.first() {
        if let [a, b, ..] = vals.as_slice() {
            examples.push(format!(
                "where {field} = {} or {field} = {}",
                quote_if_needed(a),
                quote_if_needed(b)
            ));
        }
    }

    for word in pool
        .iter()
        .flat_map(|(_, n)| n.frontmatter.title.split_whitespace())
        .filter(|w| w.len() > 3 && w.chars().all(|c| c.is_alphanumeric()))
        .take(3)
    {
        examples.push(format!("where contains \"{}\"", word.to_lowercase()));
    }

    let mut seen = std::collections::HashSet::new();
    examples.retain(|e| seen.insert(e.clone()));
    examples.truncate(60);
    examples
}

fn is_iso_date(s: &str) -> bool {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::Frontmatter;

    fn note_with(title: &str, extra: &[(&str, serde_yaml::Value)], tags: &[&str]) -> Note {
        let mut fm = Frontmatter::new(title, "nb");
        fm.tags = tags.iter().map(|s| s.to_string()).collect();
        for (k, v) in extra {
            fm.extra
                .insert(serde_yaml::Value::String(k.to_string()), v.clone());
        }
        Note::new(
            PathBuf::from(format!("/tmp/nb/{title}.md")),
            fm,
            String::new(),
        )
    }

    fn pool_with(notes: Vec<Note>) -> Vec<(Notebook, Note)> {
        notes
            .into_iter()
            .map(|n| (Notebook::new("nb", PathBuf::from("/tmp/nb")), n))
            .collect()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 6).unwrap()
    }

    #[test]
    fn parses_where_with_and_or_and_parens() {
        let q = parse("where status = pending and (priority > 1 or tag = work)").unwrap();
        assert!(matches!(q.filter, Some(Expr::And(_, _))));
        assert_eq!(q.sort, None);
    }

    #[test]
    fn parses_sort_only() {
        let q = parse("sort date desc").unwrap();
        assert_eq!(q.filter, None);
        assert_eq!(q.sort, Some(("date".to_string(), SortDir::Desc)));
    }

    #[test]
    fn parses_every_operator() {
        for (src, op) in [
            ("where a = 1", Op::Eq),
            ("where a != 1", Op::Ne),
            ("where a > 1", Op::Gt),
            ("where a < 1", Op::Lt),
            ("where a >= 1", Op::Ge),
            ("where a <= 1", Op::Le),
            ("where a ~ 1", Op::Like),
        ] {
            let q = parse(src).unwrap();
            assert!(
                matches!(q.filter, Some(Expr::Cmp { op: o, .. }) if o == op),
                "{src}"
            );
        }
    }

    #[test]
    fn today_resolves_at_eval_time_not_parse_time() {
        let q = parse("where due < today").unwrap();
        assert!(matches!(
            q.filter,
            Some(Expr::Cmp {
                value: Value::Today,
                ..
            })
        ));
    }

    #[test]
    fn malformed_dsl_returns_query_error_not_panic() {
        assert!(parse("where").is_err());
        assert!(parse("where status =").is_err());
        assert!(parse("where status pending").is_err());
        assert!(parse("").is_err());
        assert!(parse("where (status = a").is_err());
    }

    #[test]
    fn filters_by_extra_field_with_type_coercion() {
        let pool = pool_with(vec![
            note_with("A", &[("status", "pending".into())], &[]),
            note_with("B", &[("status", "done".into())], &[]),
        ]);
        let q = parse("where status = pending").unwrap();
        let rows = run_query(&pool, &q, None, today());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].note_title, "A");
    }

    #[test]
    fn numeric_comparison_uses_numeric_not_lexicographic_order() {
        let pool = pool_with(vec![
            note_with("Low", &[("priority", 2.into())], &[]),
            note_with("High", &[("priority", 10.into())], &[]),
        ]);
        // Lexicographic string compare would put "10" before "2".
        let q = parse("where priority >= 3").unwrap();
        let rows = run_query(&pool, &q, None, today());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].note_title, "High");
    }

    #[test]
    fn date_comparison_against_today() {
        let pool = pool_with(vec![
            note_with("Past", &[("due", "2026-08-01".into())], &[]),
            note_with("Future", &[("due", "2026-08-10".into())], &[]),
        ]);
        let q = parse("where due < today").unwrap();
        let rows = run_query(&pool, &q, None, today());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].note_title, "Past");
    }

    #[test]
    fn absent_field_excludes_from_eq_but_included_in_ne() {
        let pool = pool_with(vec![note_with("NoStatus", &[], &[])]);
        let eq = parse("where status = pending").unwrap();
        assert_eq!(run_query(&pool, &eq, None, today()).len(), 0);
        let ne = parse("where status != pending").unwrap();
        assert_eq!(run_query(&pool, &ne, None, today()).len(), 1);
    }

    #[test]
    fn absent_field_never_matches_like_or_ordering() {
        let pool = pool_with(vec![note_with("NoDue", &[], &[])]);
        for dsl in [
            "where due < today",
            "where due ~ today",
            "where due > today",
        ] {
            let q = parse(dsl).unwrap();
            assert_eq!(run_query(&pool, &q, None, today()).len(), 0, "{dsl}");
        }
    }

    #[test]
    fn tag_field_tests_membership_not_substring() {
        let pool = pool_with(vec![
            note_with("Work", &[], &["work", "urgent"]),
            note_with("Home", &[], &["home"]),
        ]);
        let q = parse("where tag = work").unwrap();
        let rows = run_query(&pool, &q, None, today());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].note_title, "Work");
    }

    #[test]
    fn contains_matches_title_or_body_case_insensitively() {
        let mut note = note_with("Roadmap", &[], &[]);
        note.body = "shipping the Query engine next".to_string();
        let pool = pool_with(vec![note]);
        let q = parse("where contains \"query engine\"").unwrap();
        assert_eq!(run_query(&pool, &q, None, today()).len(), 1);
    }

    #[test]
    fn sort_sinks_absent_field_to_the_bottom_regardless_of_direction() {
        let pool = pool_with(vec![
            note_with("NoPriority", &[], &[]),
            note_with("Low", &[("priority", 1.into())], &[]),
            note_with("High", &[("priority", 5.into())], &[]),
        ]);
        let asc = parse("sort priority asc").unwrap();
        let rows = run_query(&pool, &asc, None, today());
        assert_eq!(
            rows.iter()
                .map(|r| r.note_title.as_str())
                .collect::<Vec<_>>(),
            vec!["Low", "High", "NoPriority"]
        );
        let desc = parse("sort priority desc").unwrap();
        let rows = run_query(&pool, &desc, None, today());
        assert_eq!(
            rows.iter()
                .map(|r| r.note_title.as_str())
                .collect::<Vec<_>>(),
            vec!["High", "Low", "NoPriority"]
        );
    }

    #[test]
    fn notebook_scoping_narrows_results() {
        let mut pool = pool_with(vec![note_with("InNb", &[], &[])]);
        pool.push((
            Notebook::new("other", PathBuf::from("/tmp/other")),
            note_with("InOther", &[], &[]),
        ));
        let q = parse("sort date asc").unwrap();
        let rows = run_query(&pool, &q, Some("nb"), today());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].notebook, "nb");
    }

    #[test]
    fn suggested_queries_use_real_fields_and_values_from_the_pool() {
        let pool = pool_with(vec![
            note_with(
                "Fix login bug",
                &[
                    ("status", "pending".into()),
                    ("priority", "high".into()),
                    ("due", "2026-08-07".into()),
                ],
                &[],
            ),
            note_with(
                "Update dependencies",
                &[
                    ("status", "done".into()),
                    ("priority", "low".into()),
                    ("due", "2026-07-30".into()),
                ],
                &[],
            ),
        ]);
        let suggestions = suggest_queries(&pool);
        assert!(!suggestions.is_empty());
        // Every generated example must itself be a valid, parseable query —
        // a suggestion that doesn't parse would be worse than none at all.
        for s in &suggestions {
            assert!(parse(s).is_ok(), "suggestion {s:?} failed to parse");
        }
        assert!(suggestions.iter().any(|s| s.contains("status")));
        assert!(suggestions.iter().any(|s| s.starts_with("sort due")));
    }

    #[test]
    fn suggested_queries_cover_every_distinct_value_and_sort_both_directions() {
        let pool = pool_with(vec![
            note_with(
                "A",
                &[
                    ("status", "pending".into()),
                    ("project", "alpha".into()),
                    ("due", "2026-08-07".into()),
                ],
                &[],
            ),
            note_with(
                "B",
                &[
                    ("status", "in-progress".into()),
                    ("project", "beta".into()),
                    ("due", "2026-07-30".into()),
                ],
                &[],
            ),
            note_with(
                "C",
                &[
                    ("status", "done".into()),
                    ("project", "gamma".into()),
                    ("due", "2026-06-01".into()),
                ],
                &[],
            ),
        ]);
        let suggestions = suggest_queries(&pool);
        for s in &suggestions {
            assert!(parse(s).is_ok(), "suggestion {s:?} failed to parse");
        }
        // Every distinct status/project value gets its own example, not
        // just the top couple — this is the whole point of the richer
        // generation ("por todos los projects").
        for status in ["pending", "in-progress", "done"] {
            assert!(
                suggestions.contains(&format!("where status = {status}")),
                "missing suggestion for status = {status}"
            );
        }
        for project in ["alpha", "beta", "gamma"] {
            assert!(
                suggestions.contains(&format!("where project = {project}")),
                "missing suggestion for project = {project}"
            );
        }
        // Both sort directions, for a plain (non-date) field.
        assert!(suggestions.contains(&"sort status asc".to_string()));
        assert!(suggestions.contains(&"sort status desc".to_string()));
        // Relative-date variety: today, overdue, upcoming, week, month.
        assert!(suggestions.contains(&"where due = today".to_string()));
        assert!(suggestions.contains(&"where due < today".to_string()));
        assert!(suggestions.contains(&"where due > today".to_string()));
        assert!(suggestions
            .iter()
            .any(|s| s.starts_with("where due >= today and due <=")));
        // No duplicates.
        let mut sorted = suggestions.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), suggestions.len());
    }

    #[test]
    fn suggested_queries_is_empty_for_a_pool_with_no_custom_fields() {
        let pool = pool_with(vec![note_with("Plain note", &[], &[])]);
        // No `extra` fields anywhere to build a `where`/`sort` example from
        // — still shouldn't panic, just yields nothing (or a `contains`
        // example only, if the title has a word long enough).
        let suggestions = suggest_queries(&pool);
        for s in &suggestions {
            assert!(parse(s).is_ok(), "suggestion {s:?} failed to parse");
        }
    }

    #[test]
    fn field_values_deduped_and_sorted_scoped_to_one_field() {
        let pool = pool_with(vec![
            note_with(
                "A",
                &[("project", "alpha".into()), ("priority", "high".into())],
                &[],
            ),
            note_with("B", &[("project", "beta".into())], &[]),
            note_with("C", &[("project", "alpha".into())], &[]),
        ]);
        assert_eq!(
            field_values(&pool, "project"),
            vec!["alpha".to_string(), "beta".to_string()]
        );
        assert_eq!(field_values(&pool, "priority"), vec!["high".to_string()]);
        assert!(field_values(&pool, "nonexistent").is_empty());
    }
}

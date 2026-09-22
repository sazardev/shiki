//! `shiki query` — the Dataview-style frontmatter query language,
//! scriptable. Same source of truth (`NotebookStore::all_notes` +
//! `shiki_core::query::{parse, run_query}`) as the TUI's query modal
//! (leader+`q`), so a DSL string can never mean something different on the
//! command line than it does in the TUI.

use std::io::IsTerminal;

use anyhow::Result;
use shiki_core::query::QueryRow;
use shiki_core::NotebookStore;

use super::{page_footer, page_json, paginate};

pub fn run(
    store: &NotebookStore,
    notebook: Option<&str>,
    dsl: &str,
    json: bool,
    count: bool,
    offset: usize,
    limit: Option<usize>,
) -> Result<()> {
    let today = chrono::Local::now().date_naive();
    let pool = store.all_notes()?;
    let query = shiki_core::query::parse(dsl).map_err(|e| {
        let known = shiki_core::query::known_fields(&pool);
        let seen = if known.is_empty() {
            String::new()
        } else {
            format!("\n  seen in your notes: {}", known.join(", "))
        };
        anyhow::anyhow!(
            "query error: {e}\n  built-in fields: {}{seen}\n  example: {}",
            shiki_core::query::BUILTIN_FIELDS,
            shiki_core::query::EXAMPLE_QUERY,
        )
    })?;
    let rows = shiki_core::query::run_query(&pool, &query, notebook, today);

    if count {
        println!("{}", rows.len());
        return Ok(());
    }
    let (page, total) = paginate(rows, offset, limit);
    if json {
        let items: Vec<serde_json::Value> = page
            .iter()
            .map(|r| {
                let fields: serde_json::Map<String, serde_json::Value> = r
                    .fields
                    .iter()
                    .filter_map(|(k, v)| {
                        let key = k.as_str()?.to_string();
                        let value = serde_json::to_value(v).ok()?;
                        Some((key, value))
                    })
                    .collect();
                serde_json::json!({
                    "notebook": r.notebook,
                    "note": r.note_title,
                    "location": r.location,
                    "path": r.path,
                    "fields": fields,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string(&page_json(items, total, offset, limit))?
        );
        return Ok(());
    }

    if page.is_empty() {
        if total == 0 {
            println!("(no matching notes)");
        } else {
            println!("(nothing at offset {offset} \u{2014} {total} matching note(s) total)");
        }
        return Ok(());
    }
    let shown = page.len();
    let color = std::io::stdout().is_terminal();
    for r in &page {
        println!("{}", format_row(r, color));
    }
    if let Some(footer) = page_footer(shown, offset, total) {
        println!("{footer}");
    }
    Ok(())
}

fn format_row(row: &QueryRow, color: bool) -> String {
    const DIM: &str = "\x1b[2m";
    const RESET: &str = "\x1b[0m";
    let fields: Vec<String> = row
        .fields
        .iter()
        .filter_map(|(k, v)| {
            let key = k.as_str()?;
            Some(format!("{key}={}", yaml_scalar(v)))
        })
        .collect();
    let fields_str = if fields.is_empty() {
        String::new()
    } else if color {
        format!("  {DIM}{}{RESET}", fields.join(" "))
    } else {
        format!("  {}", fields.join(" "))
    };
    let location = if color {
        format!("  {DIM}{}{RESET}", row.location)
    } else {
        format!("  {}", row.location)
    };
    format!("{}{fields_str}{location}", row.note_title)
}

fn yaml_scalar(v: &serde_yaml::Value) -> String {
    shiki_core::query::yaml_value_to_string(v)
}

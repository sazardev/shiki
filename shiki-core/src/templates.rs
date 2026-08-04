use std::collections::HashMap;
use std::path::Path;

use crate::{Error, Result};

/// A note template: text with `{{key}}` placeholders.
#[derive(Debug, Clone)]
pub struct Template {
    pub name: String,
    pub contents: String,
}

impl Template {
    pub fn load(templates_dir: &Path, name: &str) -> Result<Self> {
        let path = templates_dir.join(format!("{name}.md"));
        if !path.exists() {
            return Err(Error::TemplateNotFound(name.to_string()));
        }
        let contents = std::fs::read_to_string(path)?;
        Ok(Self {
            name: name.to_string(),
            contents,
        })
    }

    /// Substitutes `{{key}}` placeholders with the given values, in a
    /// single left-to-right pass over `self.contents`. Deliberately not a
    /// sequential `String::replace` per key: that approach rescans the
    /// *already-substituted* output on every subsequent key, so a value
    /// that happens to contain another placeholder's literal text (e.g.
    /// a title of `Meeting {{date}} notes`) gets that text substituted
    /// again on the next iteration instead of being left alone.
    pub fn render(&self, vars: &HashMap<&str, String>) -> String {
        let contents = self.contents.as_str();
        let mut out = String::with_capacity(contents.len());
        let mut rest = contents;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after_open = &rest[start + 2..];
            match after_open.find("}}") {
                Some(end) => {
                    let key = &after_open[..end];
                    match vars.get(key) {
                        Some(value) => out.push_str(value),
                        None => out.push_str(&rest[start..start + 4 + end]),
                    }
                    rest = &after_open[end + 2..];
                }
                None => {
                    // Unterminated `{{` — emit it literally and stop scanning.
                    out.push_str(&rest[start..]);
                    rest = "";
                    break;
                }
            }
        }
        out.push_str(rest);
        out
    }
}

pub const DEFAULT_TEMPLATE: &str = "# {{title}}\n\n";
pub const DAILY_TEMPLATE: &str = "# {{date}}\n\n## Tasks\n\n- [ ] \n\n## Notes\n\n";
pub const MEETING_TEMPLATE: &str =
    "# {{title}}\n\nDate: {{date}}\n\n## Attendees\n\n## Agenda\n\n## Notes\n\n## Action Items\n\n";
pub const STANDUP_TEMPLATE: &str =
    "# {{title}}\n\nDate: {{date}}\n\n## Yesterday\n\n## Today\n\n## Blockers\n\n";
pub const RETRO_TEMPLATE: &str = "# {{title}}\n\nDate: {{date}}\n\n## What Went Well\n\n## What Didn't Go Well\n\n## Action Items\n\n- [ ] \n";
pub const ONE_ON_ONE_TEMPLATE: &str =
    "# {{title}}\n\nDate: {{date}}\n\n## Talking Points\n\n## Notes\n\n## Action Items\n\n- [ ] \n";
pub const BUG_TEMPLATE: &str = "# {{title}}\n\nDate: {{date}}\nSeverity: \nStatus: Open\n\n## Summary\n\n## Steps to Reproduce\n\n1. \n2. \n3. \n\n## Expected Behavior\n\n## Actual Behavior\n\n## Environment\n\n- \n\n## Fix Notes\n\n";
pub const SPEC_TEMPLATE: &str = "# {{title}}\n\nDate: {{date}}\nStatus: Draft\n\n## Problem\n\n## Goals\n\n## Non-Goals\n\n## Proposal\n\n## Alternatives Considered\n\n## Open Questions\n\n";
pub const POSTMORTEM_TEMPLATE: &str = "# {{title}}\n\nDate: {{date}}\nSeverity: \nStatus: Draft\n\n## Summary\n\n## Timeline\n\n## Root Cause\n\n## Impact\n\n## Action Items\n\n- [ ] \n\n## Lessons Learned\n\n";
pub const REVIEW_TEMPLATE: &str = "# {{title}}\n\nDate: {{date}}\nPR/MR: \n\n## Summary\n\n## Comments\n\n## Decision\n\n- [ ] Approved\n- [ ] Changes requested\n";
pub const WEEKLY_TEMPLATE: &str = "# {{title}}\n\nWeek of: {{date}}\n\n## Highlights\n\n## Metrics\n\n## Challenges\n\n## Next Week Priorities\n\n- [ ] \n";
pub const BRAINSTORM_TEMPLATE: &str = "# {{title}}\n\nDate: {{date}}\n\n## Problem / Prompt\n\n## Ideas\n\n- \n\n## Next Steps\n\n- [ ] \n";

/// Creates the default templates in `templates_dir` if they don't exist yet
/// — one file per const above, `if !path.exists()` so re-running this
/// (it happens on every CLI/TUI launch, `shiki-cli/src/main.rs`'s
/// `Context::load`) only ever fills in newly-added templates and never
/// overwrites one a user has since customized.
pub fn ensure_defaults(templates_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(templates_dir)?;
    let defaults = [
        ("default.md", DEFAULT_TEMPLATE),
        ("daily.md", DAILY_TEMPLATE),
        ("meeting.md", MEETING_TEMPLATE),
        ("standup.md", STANDUP_TEMPLATE),
        ("retro.md", RETRO_TEMPLATE),
        ("1on1.md", ONE_ON_ONE_TEMPLATE),
        ("bug.md", BUG_TEMPLATE),
        ("spec.md", SPEC_TEMPLATE),
        ("postmortem.md", POSTMORTEM_TEMPLATE),
        ("review.md", REVIEW_TEMPLATE),
        ("weekly.md", WEEKLY_TEMPLATE),
        ("brainstorm.md", BRAINSTORM_TEMPLATE),
    ];
    for (filename, contents) in defaults {
        let path = templates_dir.join(filename);
        if !path.exists() {
            std::fs::write(path, contents)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_does_not_resubstitute_a_value_containing_placeholder_text() {
        let template = Template {
            name: "t".to_string(),
            contents: "# {{title}}\n\nDate: {{date}}\n".to_string(),
        };
        let mut vars = HashMap::new();
        vars.insert("title", "Meeting {{date}} notes".to_string());
        vars.insert("date", "2026-08-02".to_string());

        let out = template.render(&vars);

        assert_eq!(out, "# Meeting {{date}} notes\n\nDate: 2026-08-02\n");
    }

    #[test]
    fn render_leaves_unknown_placeholders_untouched() {
        let template = Template {
            name: "t".to_string(),
            contents: "{{title}} / {{unknown}}".to_string(),
        };
        let mut vars = HashMap::new();
        vars.insert("title", "Todo".to_string());

        assert_eq!(template.render(&vars), "Todo / {{unknown}}");
    }
}

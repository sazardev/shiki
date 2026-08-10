use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::icons;
use crate::render::{hex_to_color, panel_block};

/// The Settings screen's tabs — left/right (`App::switch_settings_section`)
/// cycles through these; each has its own row list rather than one long
/// scroll. Every tab is now genuinely actionable (see each section's row
/// builder/field enum below), not just NOTEBOOKS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    Theme,
    Git,
    Editor,
    Export,
    Notebooks,
    Snippets,
}

impl SettingsSection {
    pub const ALL: [SettingsSection; 7] = [
        SettingsSection::General,
        SettingsSection::Theme,
        SettingsSection::Git,
        SettingsSection::Editor,
        SettingsSection::Export,
        SettingsSection::Notebooks,
        SettingsSection::Snippets,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsSection::General => "GENERAL",
            SettingsSection::Theme => "THEME",
            SettingsSection::Git => "GIT",
            SettingsSection::Editor => "EDITOR",
            SettingsSection::Export => "EXPORT",
            SettingsSection::Notebooks => "NOTEBOOKS",
            SettingsSection::Snippets => "SNIPPETS",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// GENERAL's rows, in display/selection order — `ALL`'s order must match
/// `general_rows` below exactly, since `App::handle_general_field_enter`
/// indexes into `ALL` with `settings_selected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneralField {
    DefaultNotebook,
    Editor,
    DailyTemplate,
    UseFavoriteEditor,
    EnableCaptureDaemon,
    MouseDragSelection,
    ShowHints,
    RememberLastSession,
    ShowCoffeeLink,
    SkipDeleteConfirm,
    ShowDates,
    WikilinkAutocomplete,
    DailyAgenda,
    CompactFooter,
    StatusMessageTimeoutSecs,
    DrawerWidth,
    TasksShowDoneDefault,
    DefaultNoteSort,
    LogHistoryLimit,
    TrashRetentionDays,
    ReadingWpm,
    PageStep,
}

impl GeneralField {
    pub const ALL: [GeneralField; 22] = [
        GeneralField::DefaultNotebook,
        GeneralField::Editor,
        GeneralField::DailyTemplate,
        GeneralField::UseFavoriteEditor,
        GeneralField::EnableCaptureDaemon,
        GeneralField::MouseDragSelection,
        GeneralField::ShowHints,
        GeneralField::RememberLastSession,
        GeneralField::ShowCoffeeLink,
        GeneralField::SkipDeleteConfirm,
        GeneralField::ShowDates,
        GeneralField::WikilinkAutocomplete,
        GeneralField::DailyAgenda,
        GeneralField::CompactFooter,
        GeneralField::StatusMessageTimeoutSecs,
        GeneralField::DrawerWidth,
        GeneralField::TasksShowDoneDefault,
        GeneralField::DefaultNoteSort,
        GeneralField::LogHistoryLimit,
        GeneralField::TrashRetentionDays,
        GeneralField::ReadingWpm,
        GeneralField::PageStep,
    ];
}

/// THEME's rows — `Name` opens the existing theme picker (leader+`c`)
/// rather than duplicating theme-switching logic; `Overrides` is left
/// informational (19 individual color slots don't fit a single-row edit);
/// `Icons` toggles in place, same shape as GIT/EDITOR's booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeField {
    Name,
    Icons,
    Overrides,
}

impl ThemeField {
    pub const ALL: [ThemeField; 3] = [ThemeField::Name, ThemeField::Icons, ThemeField::Overrides];
}

/// GIT's rows (the global `[git]` defaults, not a notebook's own overrides
/// — see `NotebookField` for those), in display/selection order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitField {
    AutoCommit,
    AutoPush,
    CommitPrefix,
    Remote,
    Branch,
    SignCommits,
    AutoSync,
    AutoSyncEvery,
    RemoteTemplate,
}

impl GitField {
    pub const ALL: [GitField; 9] = [
        GitField::AutoCommit,
        GitField::AutoPush,
        GitField::CommitPrefix,
        GitField::Remote,
        GitField::Branch,
        GitField::SignCommits,
        GitField::AutoSync,
        GitField::AutoSyncEvery,
        GitField::RemoteTemplate,
    ];
}

/// EDITOR's rows — every field is a plain bool toggle (no drill-down, same
/// flat shape as GENERAL/GIT), gating one native-editor UX behavior each;
/// see `EditorConfig`'s own doc comments for what each one does and why it
/// defaults the way it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorField {
    MouseSelection,
    FindReplace,
    OsClipboard,
    SelectAllCtrlA,
    LineNumbers,
    MultiCursor,
    AutoListContinue,
    FormatShortcuts,
    AutoPairBrackets,
    PasteUrlAsLink,
    SnippetExpandTab,
    TypewriterScroll,
    MoveLine,
    DuplicateLine,
    BlockIndentSelect,
}

impl EditorField {
    pub const ALL: [EditorField; 15] = [
        EditorField::MouseSelection,
        EditorField::FindReplace,
        EditorField::OsClipboard,
        EditorField::SelectAllCtrlA,
        EditorField::LineNumbers,
        EditorField::MultiCursor,
        EditorField::AutoListContinue,
        EditorField::FormatShortcuts,
        EditorField::AutoPairBrackets,
        EditorField::PasteUrlAsLink,
        EditorField::SnippetExpandTab,
        EditorField::TypewriterScroll,
        EditorField::MoveLine,
        EditorField::DuplicateLine,
        EditorField::BlockIndentSelect,
    ];
}

/// go-pretty-pdf's 17 built-in themes, in the order its own README lists
/// them — `handle_export_field_enter` cycles `pdf_theme` through this exact
/// list (wrapping), rather than a free-text prompt: a typo'd theme name
/// would only ever surface as an opaque `pretty-pdf` error at publish time,
/// so constraining input to the known-valid set up front is the safer
/// default.
pub const PDF_THEMES: [&str; 17] = [
    "default",
    "minimal",
    "modern",
    "classic",
    "corporate",
    "dark",
    "academic",
    "editorial",
    "sepia",
    "terminal",
    "blueprint",
    "ivy",
    "government",
    "resume",
    "legal",
    "latex",
    "gruvbox",
];

/// EXPORT's rows — `PdfTheme` cycles (see `PDF_THEMES`); `ExportDir` opens a
/// text prompt showing where PDFs actually land, resolved (not just the raw
/// config string) since empty is a valid value meaning "the app's own data
/// dir"; `AskExportPath` toggles in place, same flat shape as GIT/EDITOR's
/// booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportField {
    PdfTheme,
    ExportDir,
    AskExportPath,
}

impl ExportField {
    pub const ALL: [ExportField; 3] = [
        ExportField::PdfTheme,
        ExportField::ExportDir,
        ExportField::AskExportPath,
    ];
}

/// One editable row within a drilled-into notebook (NOTEBOOKS section, level
/// 2) — `ALL`'s order is the row order both `notebook_field_rows` and
/// `App::handle_settings_notebook_field_key` index into, so the two can't
/// disagree about which row is which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotebookField {
    Remote,
    AutoPush,
    AutoSync,
    AutoSyncEvery,
    /// Unlike the three above, this isn't a plain in-place toggle —
    /// flipping it on/off means actually re-encrypting/decrypting every
    /// note, so `Enter` here starts a passphrase prompt
    /// (`App::start_passphrase_prompt`) instead of cycling a value.
    Encryption,
}

impl NotebookField {
    pub const ALL: [NotebookField; 5] = [
        NotebookField::Remote,
        NotebookField::AutoPush,
        NotebookField::AutoSync,
        NotebookField::AutoSyncEvery,
        NotebookField::Encryption,
    ];
}

/// One editable row within a drilled-into snippet (SNIPPETS section, level
/// 2) — same shape as `NotebookField`. `Body` edits through the same
/// `Mode::Edit`/`InlineEditor` machinery a note's own body uses (see
/// `App::editing_snippet`), since a snippet body is arbitrary multi-line
/// text, not something a single-line prompt can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetField {
    Label,
    Body,
}

impl SnippetField {
    pub const ALL: [SnippetField; 2] = [SnippetField::Label, SnippetField::Body];
}

/// Every real notebook (not just ones with a `[notebooks.<name>]` override),
/// sorted by name — the NOTEBOOKS section lists all of them, since a
/// notebook's git remote is worth showing regardless of whether it has any
/// config override at all.
pub fn sorted_notebook_names(app: &App) -> Vec<String> {
    let mut names: Vec<String> = app.notebooks.iter().map(|nb| nb.name.clone()).collect();
    names.sort();
    names
}

/// Every configured snippet trigger, sorted — shared by the SNIPPETS level-1
/// row builder and `App`'s key handler (drilling in / deleting by index).
pub fn sorted_snippet_triggers(app: &App) -> Vec<String> {
    let mut triggers: Vec<String> = app.config.snippets.keys().cloned().collect();
    triggers.sort();
    triggers
}

fn row_line(app: &App, label: &str, value: String) -> Line<'static> {
    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);
    Line::from(vec![
        Span::styled(format!("  {label:<20}"), Style::default().fg(muted)),
        Span::styled(value, Style::default().fg(fg)),
    ])
}

fn general_rows(app: &App) -> Vec<Line<'static>> {
    let cfg = &app.config;
    vec![
        row_line(
            app,
            "default_notebook",
            cfg.general.default_notebook.clone(),
        ),
        row_line(app, "editor", cfg.general.editor.clone()),
        row_line(app, "daily_template", cfg.general.daily_template.clone()),
        row_line(
            app,
            "use_favorite_editor",
            cfg.general.use_favorite_editor.to_string(),
        ),
        row_line(
            app,
            "enable_capture_daemon",
            cfg.general.enable_capture_daemon.to_string(),
        ),
        row_line(
            app,
            "mouse_drag_selection",
            cfg.general.mouse_drag_selection.to_string(),
        ),
        row_line(app, "show_hints", cfg.general.show_hints.to_string()),
        row_line(
            app,
            "remember_last_session",
            cfg.general.remember_last_session.to_string(),
        ),
        row_line(
            app,
            "show_coffee_link",
            cfg.general.show_coffee_link.to_string(),
        ),
        row_line(
            app,
            "skip_delete_confirm",
            cfg.general.skip_delete_confirm.to_string(),
        ),
        row_line(app, "show_dates", cfg.general.show_dates.to_string()),
        row_line(
            app,
            "wikilink_autocomplete",
            cfg.general.wikilink_autocomplete.to_string(),
        ),
        row_line(app, "daily_agenda", cfg.general.daily_agenda.to_string()),
        row_line(
            app,
            "compact_footer",
            cfg.general.compact_footer.to_string(),
        ),
        row_line(
            app,
            "status_message_timeout_secs",
            cfg.general.status_message_timeout_secs.to_string(),
        ),
        row_line(app, "drawer_width", cfg.general.drawer_width.to_string()),
        row_line(
            app,
            "tasks_show_done_default",
            cfg.general.tasks_show_done_default.to_string(),
        ),
        row_line(
            app,
            "default_note_sort",
            cfg.general.default_note_sort.clone(),
        ),
        row_line(
            app,
            "log_history_limit",
            cfg.general.log_history_limit.to_string(),
        ),
        row_line(
            app,
            "trash_retention_days",
            if cfg.general.trash_retention_days == 0 {
                "never".to_string()
            } else {
                cfg.general.trash_retention_days.to_string()
            },
        ),
        row_line(app, "reading_wpm", cfg.general.reading_wpm.to_string()),
        row_line(app, "page_step", cfg.general.page_step.to_string()),
    ]
}

fn theme_rows(app: &App) -> Vec<Line<'static>> {
    let cfg = &app.config;
    let set = cfg.theme.overrides.set_count();
    vec![
        row_line(app, "name", cfg.theme.name.clone()),
        row_line(app, "icons", cfg.theme.icons.to_string()),
        row_line(
            app,
            "overrides",
            if set == 0 {
                "none".to_string()
            } else {
                format!("{set} of 19 slots set")
            },
        ),
    ]
}

fn git_rows(app: &App) -> Vec<Line<'static>> {
    let cfg = &app.config;
    vec![
        row_line(app, "auto_commit", cfg.git.auto_commit.to_string()),
        row_line(app, "auto_push", cfg.git.auto_push.to_string()),
        row_line(app, "commit_prefix", cfg.git.commit_prefix.clone()),
        row_line(app, "remote", cfg.git.remote.clone()),
        row_line(app, "branch", cfg.git.branch.clone()),
        row_line(app, "sign_commits", cfg.git.sign_commits.to_string()),
        row_line(app, "auto_sync", cfg.git.auto_sync.to_string()),
        row_line(app, "auto_sync_every", cfg.git.auto_sync_every.to_string()),
        row_line(
            app,
            "remote_template",
            if cfg.git.remote_template.is_empty() {
                "(none)".to_string()
            } else {
                cfg.git.remote_template.clone()
            },
        ),
    ]
}

fn editor_rows(app: &App) -> Vec<Line<'static>> {
    let cfg = &app.config;
    vec![
        row_line(
            app,
            "mouse_selection",
            cfg.editor.mouse_selection.to_string(),
        ),
        row_line(app, "find_replace", cfg.editor.find_replace.to_string()),
        row_line(app, "os_clipboard", cfg.editor.os_clipboard.to_string()),
        row_line(
            app,
            "select_all_ctrl_a",
            cfg.editor.select_all_ctrl_a.to_string(),
        ),
        row_line(app, "line_numbers", cfg.editor.line_numbers.to_string()),
        row_line(app, "multi_cursor", cfg.editor.multi_cursor.to_string()),
        row_line(
            app,
            "auto_list_continue",
            cfg.editor.auto_list_continue.to_string(),
        ),
        row_line(
            app,
            "format_shortcuts",
            cfg.editor.format_shortcuts.to_string(),
        ),
        row_line(
            app,
            "auto_pair_brackets",
            cfg.editor.auto_pair_brackets.to_string(),
        ),
        row_line(
            app,
            "paste_url_as_link",
            cfg.editor.paste_url_as_link.to_string(),
        ),
        row_line(
            app,
            "snippet_expand_tab",
            cfg.editor.snippet_expand_tab.to_string(),
        ),
        row_line(
            app,
            "typewriter_scroll",
            cfg.editor.typewriter_scroll.to_string(),
        ),
        row_line(app, "move_line", cfg.editor.move_line.to_string()),
        row_line(app, "duplicate_line", cfg.editor.duplicate_line.to_string()),
        row_line(
            app,
            "block_indent_select",
            cfg.editor.block_indent_select.to_string(),
        ),
    ]
}

fn export_rows(app: &App) -> Vec<Line<'static>> {
    let export_dir = if app.config.export.export_dir.trim().is_empty() {
        format!("(default) {}", app.resolved_export_dir().to_string_lossy())
    } else {
        app.config.export.export_dir.clone()
    };
    vec![
        row_line(app, "pdf_theme", app.config.export.pdf_theme.clone()),
        row_line(app, "export_dir", export_dir),
        row_line(
            app,
            "ask_export_path",
            app.config.export.ask_export_path.to_string(),
        ),
    ]
}

/// NOTEBOOKS level 1 — one row per real notebook, showing its git remote
/// (redacted) rather than the per-notebook sync-policy overrides this used
/// to show instead; `notebook_field_rows` (level 2) covers those.
fn notebook_list_rows(app: &App) -> Vec<Line<'static>> {
    let names = sorted_notebook_names(app);
    if names.is_empty() {
        return vec![row_line(app, "", "no notebooks yet".to_string())];
    }
    names
        .iter()
        .map(|name| {
            let remote = app
                .notebooks
                .iter()
                .find(|nb| &nb.name == name)
                .and_then(|nb| shiki_core::git::remote_url(&nb.path))
                .map(|url| shiki_core::git::redact_credentials(&url))
                .unwrap_or_else(|| "(no remote)".to_string());
            row_line(app, name, remote)
        })
        .collect()
}

/// NOTEBOOKS level 2 — the drilled-into notebook's own remote plus its
/// three sync-policy overrides, each falling back to "inherit (<global>)"
/// when unset rather than a bare `false`/`0`, so it's visible whether a
/// value is this notebook's own or just the `[git]` default showing through.
fn notebook_field_rows(app: &App, name: &str) -> Vec<Line<'static>> {
    let remote = app
        .notebooks
        .iter()
        .find(|nb| nb.name == name)
        .and_then(|nb| shiki_core::git::remote_url(&nb.path))
        .map(|url| shiki_core::git::redact_credentials(&url))
        .unwrap_or_else(|| "(none — enter to set)".to_string());
    let over = app.config.notebooks.get(name).cloned().unwrap_or_default();
    let bool_cell = |v: Option<bool>, global: bool| match v {
        Some(b) => b.to_string(),
        None => format!("inherit ({global})"),
    };
    let num_cell = |v: Option<u32>, global: u32| match v {
        Some(n) => n.to_string(),
        None => format!("inherit ({global})"),
    };
    vec![
        row_line(app, "remote", remote),
        row_line(
            app,
            "auto_push",
            bool_cell(over.auto_push, app.config.git.auto_push),
        ),
        row_line(
            app,
            "auto_sync",
            bool_cell(over.auto_sync, app.config.git.auto_sync),
        ),
        row_line(
            app,
            "auto_sync_every",
            num_cell(over.auto_sync_every, app.config.git.auto_sync_every),
        ),
        row_line(
            app,
            "encrypted",
            if over.encrypt {
                "true (enter to disable — prompts for passphrase)".to_string()
            } else {
                "false (enter to enable — prompts for a new passphrase)".to_string()
            },
        ),
    ]
}

/// SNIPPETS level 1 — one row per configured `/`-menu command. Unlike the
/// other sections' level-1 lists, this one can be empty by construction (a
/// fresh config has no snippets at all) and still needs to support `a`
/// (create) from that empty state, so the placeholder row explicitly says
/// so rather than reusing "none configured" wording that implied nothing
/// could be done about it.
fn snippet_list_rows(app: &App) -> Vec<Line<'static>> {
    let triggers = sorted_snippet_triggers(app);
    if triggers.is_empty() {
        return vec![row_line(
            app,
            "",
            "no snippets yet — press 'a' to add one".to_string(),
        )];
    }
    triggers
        .iter()
        .map(|trigger| {
            let snippet = &app.config.snippets[trigger];
            let label = snippet.label.clone().unwrap_or_else(|| trigger.clone());
            row_line(app, trigger, label)
        })
        .collect()
}

/// SNIPPETS level 2 — the drilled-into snippet's label and a one-line
/// preview of its body (the real multi-line body is edited through
/// `Mode::Edit`, not shown inline here).
fn snippet_field_rows(app: &App, trigger: &str) -> Vec<Line<'static>> {
    let snippet = app.config.snippets.get(trigger);
    let label = snippet
        .and_then(|s| s.label.clone())
        .unwrap_or_else(|| trigger.to_string());
    let body_preview = match snippet.map(|s| s.body.as_str()) {
        None | Some("") => "(empty — enter to edit)".to_string(),
        Some(body) => {
            let first_line = body.lines().next().unwrap_or_default();
            let extra_lines = body.lines().count().saturating_sub(1);
            if extra_lines > 0 {
                format!("{first_line}  (+{extra_lines} more line(s) — enter to edit)")
            } else {
                format!("{first_line}  (enter to edit)")
            }
        }
    };
    vec![
        row_line(app, "label", label),
        row_line(app, "body", body_preview),
    ]
}

/// Rows for whatever's currently visible: the active tab, and — for
/// NOTEBOOKS/SNIPPETS specifically — whichever level the corresponding
/// `App.settings_*_drill` field selects. Every row here is a real,
/// 1:1-indexable list item (no header/blank-line filler mixed in), since
/// `App::handle_settings_key` indexes directly into it for navigation and
/// row actions.
pub fn build(app: &App) -> Vec<Line<'static>> {
    match app.settings_section {
        SettingsSection::General => general_rows(app),
        SettingsSection::Theme => theme_rows(app),
        SettingsSection::Git => git_rows(app),
        SettingsSection::Editor => editor_rows(app),
        SettingsSection::Export => export_rows(app),
        SettingsSection::Notebooks => match &app.settings_notebook_drill {
            Some(name) => notebook_field_rows(app, name),
            None => notebook_list_rows(app),
        },
        SettingsSection::Snippets => match &app.settings_snippet_drill {
            Some(trigger) => snippet_field_rows(app, trigger),
            None => snippet_list_rows(app),
        },
    }
}

fn tab_bar(app: &App) -> Line<'static> {
    let accent = hex_to_color(&app.theme.accent);
    let muted = hex_to_color(&app.theme.muted);
    let mut spans = Vec::new();
    for (i, section) in SettingsSection::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(muted)));
        }
        let style = if *section == app.settings_section {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(muted)
        };
        spans.push(Span::styled(section.label(), style));
    }
    Line::from(spans)
}

/// Near-full-screen popup, same sizing convention as which-key. A one-line
/// tab bar (`←`/`→` to switch) sits above the active section's row list —
/// every tab is actionable now (`Enter` toggles a boolean in place, opens a
/// prompt for text/number fields, or drills into a notebook/snippet), not
/// just NOTEBOOKS.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let margin_x = area.width / 10;
    let margin_y = area.height / 10;
    let popup_area = Rect {
        x: area.x + margin_x,
        y: area.y + margin_y,
        width: area.width.saturating_sub(margin_x * 2),
        height: area.height.saturating_sub(margin_y * 2),
    };
    frame.render_widget(Clear, popup_area);

    let drilled = app.settings_notebook_drill.is_some() || app.settings_snippet_drill.is_some();
    let breadcrumb = match (&app.settings_notebook_drill, &app.settings_snippet_drill) {
        (Some(name), _) => format!(" › {name}"),
        (_, Some(trigger)) => format!(" › {trigger}"),
        _ => String::new(),
    };
    let hint = if drilled {
        "j/k move · enter edit/toggle · esc/h back"
    } else if app.settings_section == SettingsSection::Snippets {
        "←/→ section · j/k move · enter open · a new · d delete · esc/q close"
    } else {
        "←/→ section · j/k move · enter edit/toggle · esc/q close"
    };
    let title = format!(" {}Settings{breadcrumb} — {hint} ", icons::GEAR);

    let block = panel_block(Line::from(title), true, &app.theme);
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    frame.render_widget(Paragraph::new(tab_bar(app)), chunks[0]);

    let lines = build(app);
    let items: Vec<ListItem> = lines.into_iter().map(ListItem::new).collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(hex_to_color(&app.theme.selection))
            .add_modifier(Modifier::BOLD),
    );

    let selected = if drilled {
        app.settings_field_selected
    } else {
        app.settings_selected
    };
    let mut state = ListState::default();
    state.select(Some(selected));
    frame.render_stateful_widget(list, chunks[1], &mut state);
}

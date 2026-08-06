use crate::app::{
    centered_rect, drawer_area, global_search_layout, global_search_popup_area, relative_folder,
    App, Mode, PendingInput, UpdateState,
};
use crate::icons;
use crate::render::{hex_to_color, panel_block};
use crate::{
    layout, panel_drawer, panel_notebooks, panel_notes, panel_outline, panel_preview,
    panel_settings, panel_tags, panel_tasks, status_bar, which,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::{Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(frame: &mut Frame, app: &App) {
    icons::set_enabled(app.config.theme.icons);
    let background = ratatui::widgets::Block::default()
        .style(ratatui::style::Style::default().bg(hex_to_color(&app.theme.bg)));
    frame.render_widget(background, frame.area());

    let areas = layout::split(frame.area(), app.focus, app.zen_mode);
    panel_notebooks::render(frame, areas.notebooks, app);
    panel_notes::render(frame, areas.notes, app);
    if app.mode == Mode::Edit {
        if let Some(editor) = &app.editor {
            let gutter_style = Style::default().fg(hex_to_color(&app.theme.muted));
            let secondary_cursor_style = Style::default()
                .bg(hex_to_color(&app.theme.accent))
                .fg(hex_to_color(&app.theme.bg))
                .add_modifier(Modifier::BOLD);
            editor.render(
                frame,
                areas.preview,
                crate::editor::RenderOptions {
                    line_numbers: app.config.editor.line_numbers,
                    gutter_style,
                    secondary_cursor_style,
                    secondary_cursors: &app.editor_secondary_cursors,
                    typewriter_scroll: app.config.editor.typewriter_scroll,
                },
            );
            if app.show_slash_menu {
                render_slash_menu(frame, areas.preview, editor, app);
            }
            if app.show_wikilink_menu {
                render_wikilink_menu(frame, areas.preview, editor, app);
            }
            if app.editor_find.is_some() {
                render_editor_find(frame, areas.preview, editor, app);
            }
        }
    } else {
        panel_preview::render(frame, areas.preview, app);
    }
    status_bar::render(frame, areas.status_bar, app);

    if let Some(kind) = app.pending_input {
        let quick_matches = if kind == PendingInput::NewNote {
            app.quick_template_filtered()
        } else {
            Vec::new()
        };
        let width = (frame.area().width / 2).max(30);
        let title = app
            .pending_input_title
            .as_deref()
            .unwrap_or_else(|| kind.title());

        if app.quick_template_query().is_some() {
            // Same height budget `render_template_picker` uses (option
            // count + 2 for the list's own border, capped so a long
            // template list can't ever push the popup off-screen), stacked
            // under the input's own fixed 3 rows instead of replacing it —
            // the title text stays visible and editable while the dropdown
            // is up.
            let list_height = (quick_matches.len() as u16 + 2).min(frame.area().height / 2);
            let popup_area = centered_rect(frame.area(), width, 3 + list_height);
            frame.render_widget(Clear, popup_area);
            let [input_area, list_area] =
                Layout::vertical([Constraint::Length(3), Constraint::Length(list_height)])
                    .areas(popup_area);
            app.input
                .render(frame, input_area, title, hex_to_color(&app.theme.accent));

            let items: Vec<ListItem> = quick_matches
                .iter()
                .map(|cmd| ListItem::new(cmd.display()))
                .collect();
            let highlight_symbol = format!("{}", icons::ARROW);
            let list_title = format!(" {}Quick template ", icons::CALENDAR);
            let list = List::new(items)
                .block(panel_block(Line::from(list_title), true, &app.theme))
                .highlight_style(
                    Style::default()
                        .bg(hex_to_color(&app.theme.selection))
                        .fg(hex_to_color(&app.theme.accent))
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(highlight_symbol.as_str());
            let mut state = ListState::default();
            if !quick_matches.is_empty() {
                state.select(Some(app.quick_template_selected));
            }
            frame.render_stateful_widget(list, list_area, &mut state);
        } else if let Some(hint) = kind.hint().filter(|_| app.config.general.show_hints) {
            // Stacked under the input box's own fixed 3 rows, same idea as
            // the quick-template dropdown above — the hint is informational
            // only, so it never affects input handling, just what's drawn.
            let hint_height = hint_line_count(hint, width.saturating_sub(2));
            let popup_area = centered_rect(frame.area(), width, 3 + 1 + hint_height);
            frame.render_widget(Clear, popup_area);
            let [input_area, _spacer, hint_area] = Layout::vertical([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(hint_height),
            ])
            .areas(popup_area);
            app.input
                .render(frame, input_area, title, hex_to_color(&app.theme.accent));
            let hint_paragraph = Paragraph::new(hint)
                .style(
                    Style::default()
                        .fg(hex_to_color(&app.theme.muted))
                        .add_modifier(Modifier::ITALIC),
                )
                .alignment(ratatui::layout::Alignment::Center)
                .wrap(Wrap { trim: true });
            frame.render_widget(hint_paragraph, hint_area);
        } else {
            let popup_area = centered_rect(frame.area(), width, 3);
            frame.render_widget(Clear, popup_area);
            app.input
                .render(frame, popup_area, title, hex_to_color(&app.theme.accent));
        }
    }

    if app.show_tags {
        let rows = match &app.tags_viewing {
            None => app.tag_index().len(),
            Some(tag) => app
                .notes
                .iter()
                .filter(|n| n.frontmatter.tags.iter().any(|t| t == tag))
                .count()
                .max(1),
        };
        let popup_area = centered_rect(frame.area(), 40, (rows as u16 + 2).max(3));
        frame.render_widget(Clear, popup_area);
        panel_tags::render(frame, popup_area, app);
    }

    if app.show_outline {
        let rows = app.outline_headings.len().max(1);
        let popup_area = centered_rect(frame.area(), 50, (rows as u16 + 2).max(3));
        frame.render_widget(Clear, popup_area);
        panel_outline::render(frame, popup_area, app);
    }

    if app.show_drawer {
        let drawer_rect = drawer_area(frame.area(), app.config.general.drawer_width);
        frame.render_widget(Clear, drawer_rect);
        panel_drawer::render(frame, drawer_rect, app);
    }

    if app.show_theme_picker {
        render_theme_picker(frame, frame.area(), app);
    }

    if app.show_template_picker {
        render_template_picker(frame, frame.area(), app);
    }

    if app.show_global_search {
        render_global_search(frame, frame.area(), app);
    }

    if app.show_logs {
        render_logs(frame, frame.area(), app);
    }

    if app.show_tree {
        render_tree(frame, frame.area(), app);
    }

    if app.show_links {
        render_links(frame, frame.area(), app);
    }

    if app.show_tasks {
        panel_tasks::render(frame, frame.area(), app);
    }

    if app.show_history {
        render_history(frame, frame.area(), app);
    }

    if app.show_update {
        render_update(frame, frame.area(), app);
    }

    if app.show_settings {
        panel_settings::render(frame, frame.area(), app);
    }

    if app.show_which_key {
        which::render(frame, frame.area(), app);
    }

    if let Some(dialog) = &app.confirm {
        let popup_area = centered_rect(frame.area(), (dialog.display_len() as u16 + 4).max(30), 3);
        frame.render_widget(Clear, popup_area);
        dialog.render(frame, popup_area, hex_to_color(&app.theme.warning));
    }
}

/// Greedy word-wrap line count for a `PendingInput` hint, so the popup can
/// be sized to fit it exactly before `Paragraph`'s own `Wrap` does the real
/// wrapping at render time.
fn hint_line_count(text: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    let mut lines: u16 = 1;
    let mut current = 0usize;
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        let sep = if current == 0 { 0 } else { 1 };
        if current + sep + word_len > width {
            lines += 1;
            current = word_len;
        } else {
            current += sep + word_len;
        }
    }
    lines
}

fn render_theme_picker(frame: &mut Frame, frame_area: Rect, app: &App) {
    let height = (app.available_themes.len() as u16 + 2).min(frame_area.height.saturating_sub(2));
    let popup_area = centered_rect(frame_area, 40, height);
    frame.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = app
        .available_themes
        .iter()
        .map(|t| ListItem::new(t.name.clone()))
        .collect();
    let highlight_symbol = format!("{}", icons::ARROW);
    let title = format!(" {}Pick a theme ", icons::EYE);
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    state.select(Some(app.theme_picker_index));
    frame.render_stateful_widget(list, popup_area, &mut state);
}

fn render_template_picker(frame: &mut Frame, frame_area: Rect, app: &App) {
    let height =
        (app.template_picker_options.len() as u16 + 2).min(frame_area.height.saturating_sub(2));
    let popup_area = centered_rect(frame_area, 40, height);
    frame.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = app
        .template_picker_options
        .iter()
        .map(|opt| match opt {
            Some(name) => ListItem::new(name.clone()),
            None => ListItem::new("(blank, no template)"),
        })
        .collect();
    let highlight_symbol = format!("{}", icons::ARROW);
    let title = format!(" {}Pick a template ", icons::NOTE);
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    state.select(Some(app.template_picker_index));
    frame.render_stateful_widget(list, popup_area, &mut state);
}

fn render_global_search(frame: &mut Frame, frame_area: Rect, app: &App) {
    let popup_area = global_search_popup_area(frame_area);
    frame.render_widget(Clear, popup_area);
    let (input_area, list_area) = global_search_layout(popup_area);

    app.global_search_input.render(
        frame,
        input_area,
        &format!(" {}Search all notes ", icons::SEARCH),
        hex_to_color(&app.theme.accent),
    );

    let items: Vec<ListItem> = app
        .global_search_results
        .iter()
        .filter_map(|hit| {
            let (nb, note) = app.global_search_pool.get(hit.index)?;
            Some(ListItem::new(format!(
                "{}{} \u{203A} {}",
                icons::NOTE,
                nb.name,
                note.frontmatter.title
            )))
        })
        .collect();
    let highlight_symbol = format!("{}", icons::ARROW);
    let count = app.global_search_results.len();
    let title = format!(" Results [{count}] ");
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    if !app.global_search_results.is_empty() {
        state.select(Some(app.global_search_selected));
    }
    frame.render_stateful_widget(list, list_area, &mut state);
}

fn render_update(frame: &mut Frame, frame_area: Rect, app: &App) {
    let popup_area = centered_rect(frame_area, (frame_area.width * 2 / 3).max(50), 7);
    frame.render_widget(Clear, popup_area);

    let current = env!("CARGO_PKG_VERSION");
    let (title, body) = match &app.update_state {
        Some(UpdateState::Checking) => (
            " Checking for updates ".to_string(),
            "Checking GitHub Releases\u{2026}".to_string(),
        ),
        Some(UpdateState::Available(version)) => (
            format!(" {} Update available ", icons::DOWNLOAD),
            format!("v{current} \u{2192} v{version}\n\n[enter] Download & install    [esc] Cancel"),
        ),
        Some(UpdateState::UpToDate) => (
            " Up to date ".to_string(),
            format!("You're on the latest version (v{current}).\n\n[esc] Close"),
        ),
        Some(UpdateState::Downloading) => (
            " Installing ".to_string(),
            "Downloading, verifying, and installing\u{2026}".to_string(),
        ),
        Some(UpdateState::Installed(version)) => (
            " Installed ".to_string(),
            format!("Installed v{version} \u{2014} restarting shiki\u{2026}"),
        ),
        Some(UpdateState::Error(message)) => (
            " Update failed ".to_string(),
            format!("{message}\n\n[esc] Close"),
        ),
        None => (" Update ".to_string(), String::new()),
    };

    let paragraph = ratatui::widgets::Paragraph::new(body)
        .wrap(ratatui::widgets::Wrap { trim: false })
        .block(panel_block(Line::from(title), true, &app.theme));
    frame.render_widget(paragraph, popup_area);
}

fn render_logs(frame: &mut Frame, frame_area: Rect, app: &App) {
    let height = (frame_area.height * 2 / 3).max(6);
    let popup_area = centered_rect(frame_area, (frame_area.width * 3 / 4).max(40), height);
    frame.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = app
        .log_history
        .iter()
        .map(|entry| {
            ListItem::new(format!(
                "{}  {}",
                entry.at.format("%H:%M:%S"),
                entry.message
            ))
        })
        .collect();
    let highlight_symbol = format!("{}", icons::ARROW);
    let title = format!(
        " {}Logs [{}]  \u{2014}  y/c copy all \u{B7} esc/q close ",
        icons::LIST,
        app.log_history.len()
    );
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    if !app.log_history.is_empty() {
        state.select(Some(app.logs_selected));
    }
    frame.render_stateful_widget(list, popup_area, &mut state);
}

fn render_tree(frame: &mut Frame, frame_area: Rect, app: &App) {
    let height = (frame_area.height * 3 / 4).max(8);
    let popup_area = centered_rect(frame_area, (frame_area.width * 3 / 4).max(40), height);
    frame.render_widget(Clear, popup_area);

    let muted = hex_to_color(&app.theme.muted);
    let fg = hex_to_color(&app.theme.fg);
    let items: Vec<ListItem> = app
        .tree_rows
        .iter()
        .map(|row| match row {
            crate::tree::TreeRow::Folder { depth, name } => {
                ListItem::new(Line::from(Span::styled(
                    format!("{}{}{name}/", "  ".repeat(*depth), icons::NOTEBOOK),
                    Style::default().fg(muted).add_modifier(Modifier::BOLD),
                )))
            }
            crate::tree::TreeRow::Note { depth, note } => ListItem::new(Line::from(Span::styled(
                format!(
                    "{}{}{}",
                    "  ".repeat(*depth),
                    icons::NOTE,
                    note.frontmatter.title
                ),
                Style::default().fg(fg),
            ))),
        })
        .collect();
    let highlight_symbol = format!("{}", icons::ARROW);
    let title = format!(
        " {}Tree [{} notes]  \u{2014}  enter open \u{B7} esc/q close ",
        icons::TREE,
        app.tree_note_count()
    );
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    state.select(app.tree_selected_row());
    frame.render_stateful_widget(list, popup_area, &mut state);
}

fn render_links(frame: &mut Frame, frame_area: Rect, app: &App) {
    let height = ((app.link_rows.len() as u16) + 2).max(4);
    let popup_area = centered_rect(frame_area, (frame_area.width * 3 / 4).max(40), height);
    frame.render_widget(Clear, popup_area);

    let muted = hex_to_color(&app.theme.muted);
    let fg = hex_to_color(&app.theme.fg);
    let error = hex_to_color(&app.theme.error);
    let items: Vec<ListItem> = app
        .link_rows
        .iter()
        .map(|row| match row {
            crate::links_panel::LinkRow::Header(label) => ListItem::new(Line::from(Span::styled(
                label.to_string(),
                Style::default().fg(muted).add_modifier(Modifier::BOLD),
            ))),
            crate::links_panel::LinkRow::Outgoing {
                text,
                resolved: Some(_),
            } => ListItem::new(Line::from(Span::styled(
                format!("  {}{text}", icons::LINK),
                Style::default().fg(fg),
            ))),
            crate::links_panel::LinkRow::Outgoing {
                text,
                resolved: None,
            } => ListItem::new(Line::from(Span::styled(
                format!("  {}{text}  (no matching note)", icons::WARNING),
                Style::default().fg(error),
            ))),
            crate::links_panel::LinkRow::Backlink { note } => {
                ListItem::new(Line::from(Span::styled(
                    format!("  {}{}", icons::NOTE, note.frontmatter.title),
                    Style::default().fg(fg),
                )))
            }
            // A mention is a *candidate* link, not an existing one — muted
            // so it reads as weaker than a real backlink at a glance.
            crate::links_panel::LinkRow::Mention { note } => {
                ListItem::new(Line::from(Span::styled(
                    format!("  {}{}", icons::SEARCH, note.frontmatter.title),
                    Style::default().fg(muted),
                )))
            }
        })
        .collect();
    let highlight_symbol = format!("{}", icons::ARROW);
    let title = format!(
        " {}Links  \u{2014}  enter jump \u{B7} c link mention \u{B7} esc/q close ",
        icons::LINK
    );
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    state.select(crate::links_panel::selected_row(
        &app.link_rows,
        app.link_selected,
    ));
    frame.render_stateful_widget(list, popup_area, &mut state);
}

fn render_history(frame: &mut Frame, frame_area: Rect, app: &App) {
    let height = (frame_area.height * 3 / 4).max(8);
    let popup_area = centered_rect(frame_area, (frame_area.width * 3 / 4).max(50), height);
    frame.render_widget(Clear, popup_area);

    let fg = hex_to_color(&app.theme.fg);
    let muted = hex_to_color(&app.theme.muted);

    if let Some((commit_id, content)) = &app.history_viewing {
        let short = commit_id.chars().take(7).collect::<String>();
        let title = format!(
            " {}Revision {short}  \u{2014}  r revert \u{B7} esc back ",
            icons::HISTORY
        );
        let paragraph = ratatui::widgets::Paragraph::new(content.as_str())
            .block(panel_block(Line::from(title), true, &app.theme))
            .wrap(ratatui::widgets::Wrap { trim: false });
        frame.render_widget(paragraph, popup_area);
        return;
    }

    let items: Vec<ListItem> = app
        .history_entries
        .iter()
        .map(|entry| {
            let short = entry.commit_id.chars().take(7).collect::<String>();
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{} ", entry.date.format("%Y-%m-%d %H:%M")),
                    Style::default().fg(fg),
                ),
                Span::styled(format!("{short}  "), Style::default().fg(muted)),
                Span::styled(entry.message.clone(), Style::default().fg(fg)),
            ]))
        })
        .collect();
    let highlight_symbol = format!("{}", icons::ARROW);
    let title = format!(
        " {}History [{} revisions]  \u{2014}  enter view \u{B7} r revert \u{B7} esc/q close ",
        icons::HISTORY,
        app.history_entries.len()
    );
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());

    let mut state = ListState::default();
    if !app.history_entries.is_empty() {
        state.select(Some(app.history_selected));
    }
    frame.render_stateful_widget(list, popup_area, &mut state);
}

/// Anchors the `/`-menu right under the current line inside the editor's
/// own inner area (`editor.cursor_screen_row()`, computed by
/// `InlineEditor::render` the same pass that just ran) — falls back to
/// showing it *above* the line instead when there isn't enough room below,
/// e.g. typing `/` on the last visible line of a long note.
fn render_slash_menu(
    frame: &mut Frame,
    area: Rect,
    editor: &crate::editor::InlineEditor,
    app: &App,
) {
    let inner = editor.inner_area(area);
    if inner.width < 10 || inner.height < 3 {
        return;
    }
    let matches = app.slash_menu_filtered();
    let width = inner.width.min(44);
    let max_height = inner.height.saturating_sub(1).max(3);
    let height = (matches.len() as u16 + 2).clamp(3, max_height);

    let cursor_row = editor.cursor_screen_row();
    let below_y = inner.y + cursor_row + 1;
    let popup_y = if below_y + height <= inner.y + inner.height {
        below_y
    } else {
        inner.y + cursor_row.saturating_sub(height)
    };

    let popup_area = Rect {
        x: inner.x,
        y: popup_y,
        width,
        height,
    };
    frame.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = matches
        .iter()
        .map(|cmd| ListItem::new(format!("/{}  {}", cmd.trigger, cmd.label)))
        .collect();
    let highlight_symbol = format!("{}", icons::ARROW);
    let title = format!(" {}Slash menu ", icons::PENCIL);
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());
    let mut state = ListState::default();
    if !matches.is_empty() {
        state.select(Some(app.slash_menu_selected));
    }
    frame.render_stateful_widget(list, popup_area, &mut state);
}

/// Anchors the `[[wikilink]]` autocomplete the same way `render_slash_menu`
/// anchors the `/`-menu — right under the current line, flipping above it
/// when there isn't room below. Kept as a separate function (rather than
/// generalizing `render_slash_menu`) since the two lists render different
/// row shapes: plain `trigger  label` strings there, note titles (with a
/// "no matches" placeholder) here.
fn render_wikilink_menu(
    frame: &mut Frame,
    area: Rect,
    editor: &crate::editor::InlineEditor,
    app: &App,
) {
    let inner = editor.inner_area(area);
    if inner.width < 10 || inner.height < 3 {
        return;
    }
    let matches = app.wikilink_menu_filtered();
    let width = inner.width.min(56);
    let max_height = inner.height.saturating_sub(1).max(3);
    let row_count = matches.len().max(1);
    let height = (row_count as u16 + 2).clamp(3, max_height);

    let cursor_row = editor.cursor_screen_row();
    let below_y = inner.y + cursor_row + 1;
    let popup_y = if below_y + height <= inner.y + inner.height {
        below_y
    } else {
        inner.y + cursor_row.saturating_sub(height)
    };

    let popup_area = Rect {
        x: inner.x,
        y: popup_y,
        width,
        height,
    };
    frame.render_widget(Clear, popup_area);

    // Shows each candidate's folder breadcrumb, not just its title — two
    // notes named the same thing in different folders (a real, common case:
    // e.g. a per-project "Notes.md") would otherwise be indistinguishable
    // in this list, unlike the NOTES panel itself, which is always scoped
    // to one folder at a time and never has this ambiguity.
    let notebook_path = app.selected_notebook().map(|nb| nb.path.clone());
    let items: Vec<ListItem> = if matches.is_empty() {
        vec![ListItem::new("no matching notes")]
    } else {
        matches
            .iter()
            .map(|note| {
                let folder = notebook_path
                    .as_deref()
                    .map(|root| relative_folder(&note.path, root))
                    .unwrap_or_default();
                let label = if folder.is_empty() {
                    note.frontmatter.title.clone()
                } else {
                    format!("{}  \u{203A}  {}", folder.join("/"), note.frontmatter.title)
                };
                ListItem::new(label)
            })
            .collect()
    };
    let highlight_symbol = format!("{}", icons::ARROW);
    let title = format!(" {}Link to note ", icons::LINK);
    let list = List::new(items)
        .block(panel_block(Line::from(title), true, &app.theme))
        .highlight_style(
            Style::default()
                .bg(hex_to_color(&app.theme.selection))
                .fg(hex_to_color(&app.theme.accent))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(highlight_symbol.as_str());
    let mut state = ListState::default();
    if !matches.is_empty() {
        state.select(Some(app.wikilink_menu_selected));
    }
    frame.render_stateful_widget(list, popup_area, &mut state);
}

/// Ctrl+F's find/replace bar — two stacked single-line `InputBox`es pinned
/// to the top of the editor's own inner area, the focused one bordered in
/// the accent color so it's clear which field typing goes to. No dedicated
/// "match highlight" style exists here: a match becomes the editor's own
/// selection (`App::editor_find_step`), so it's already visible through
/// the same selection-highlight rendering `InlineEditor::render` always
/// does.
fn render_editor_find(
    frame: &mut Frame,
    area: Rect,
    editor: &crate::editor::InlineEditor,
    app: &App,
) {
    use crate::app::FindField;
    let Some(state) = &app.editor_find else {
        return;
    };
    let inner = editor.inner_area(area);
    if inner.width < 20 || inner.height < 6 {
        return;
    }
    let accent = hex_to_color(&app.theme.accent);
    let muted = hex_to_color(&app.theme.muted);
    // Anchored top-*right*, not the full inner width from the left edge —
    // same placement VS Code's own find widget uses, specifically so a
    // short note's lines (which start at the left edge) aren't completely
    // hidden underneath it the way a full-width bar would.
    let width = inner.width.min(44);
    let x = inner.x + inner.width - width;
    let clear_area = Rect {
        x,
        y: inner.y,
        width,
        height: 6,
    };
    frame.render_widget(Clear, clear_area);
    let query_area = Rect {
        x,
        y: inner.y,
        width,
        height: 3,
    };
    let replace_area = Rect {
        x,
        y: inner.y + 3,
        width,
        height: 3,
    };
    let query_color = if state.focus == FindField::Query {
        accent
    } else {
        muted
    };
    let replace_color = if state.focus == FindField::Replace {
        accent
    } else {
        muted
    };
    state
        .query
        .render(frame, query_area, " Find (enter/shift+enter) ", query_color);
    state.replace.render(
        frame,
        replace_area,
        " Replace (ctrl+enter/ctrl+alt+enter) ",
        replace_color,
    );
}

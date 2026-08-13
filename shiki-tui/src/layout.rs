use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::Focus;

/// Screen areas: 3 panels on top + status bar at the bottom. In `Single`
/// tier, the two non-focused panels are zero-sized rather than absent —
/// every panel is always rendered unconditionally regardless of tier
/// (`draw()` doesn't special-case any of this), and rendering into a
/// zero-sized `Rect` is a safe no-op in ratatui, so this keeps that code
/// untouched instead of needing an `Option<Rect>` everywhere.
pub struct Areas {
    pub notebooks: Rect,
    pub notes: Rect,
    pub preview: Rect,
    pub status_bar: Rect,
}

/// Width/height a panel shrinks to when it's not part of the current
/// reading path — just its border line, so it stays visible but out of the
/// way (Yazi-style Miller columns) instead of competing for space with the
/// panel you're actually using. One line is enough: the user already knows
/// there's a collapsed panel there from the border alone, so there's no
/// need to spend extra space showing a sliver of content nobody can read.
const COLLAPSED: u16 = 1;

/// Below this width, three side-by-side columns leave each one too narrow
/// to be worth reading (a "portrait"/tall-narrow window, a tmux pane split
/// several ways, …) — panels stack vertically instead.
const STACK_WIDTH: u16 = 70;

/// Below this width or height, there isn't room for even a collapsed
/// sibling panel — show only the focused one, full screen. `l`/`h`/`tab`
/// still move `Focus` exactly as they always do; this tier only changes
/// which panel(s) that's reflected onto screen.
const SINGLE_WIDTH: u16 = 46;
const SINGLE_HEIGHT: u16 = 14;

/// `zen_mode` forces the same full-screen-focused-panel `single()` tier
/// `columned`/`stacked` only fall into on a genuinely small terminal —
/// `App::zen_mode` (`leader z`) opts into it regardless of actual terminal
/// size, to hide NOTEBOOKS/NOTES for distraction-free writing. No new
/// layout math: it's the exact same tier a small terminal already gets.
///
/// `drawer_width` is the notebook drawer's overlay width (`leader+b`, default
/// 30) while it's open, 0 otherwise — the drawer is a left-anchored sidebar
/// drawn on top of the panels, so the panels' main area is *pushed* right by
/// that much while it's open rather than letting it cover the first columns
/// of whatever panel sits underneath (which hid the left edge of every
/// PREVIEW line, e.g. the first half of a long math formula, behind the
/// drawer). The status bar stays full-width below.
pub fn split(area: Rect, focus: Focus, zen_mode: bool, drawer_width: u16) -> Areas {
    // No outer margin and no gap between constraints: panels go edge-to-edge
    // with the terminal and with each other, so the only "padding" visible
    // anywhere is each panel's own border.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .spacing(0)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);
    let main = rows[0];
    let status_bar = rows[1];
    let main = if drawer_width > 0 {
        Rect {
            x: main.x + drawer_width,
            width: main.width.saturating_sub(drawer_width),
            ..main
        }
    } else {
        main
    };

    if zen_mode || main.width < SINGLE_WIDTH || main.height < SINGLE_HEIGHT {
        return single(main, status_bar, focus);
    }
    if main.width < STACK_WIDTH {
        return stacked(main, status_bar, focus);
    }
    columned(main, status_bar, focus)
}

/// Wide terminals: the original 3-column Miller-columns layout.
fn columned(main: Rect, status_bar: Rect, focus: Focus) -> Areas {
    let (notebooks_c, notes_c, preview_c) = tier_constraints(focus);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .margin(0)
        .spacing(0)
        .constraints([notebooks_c, notes_c, preview_c])
        .split(main);
    Areas {
        notebooks: cols[0],
        notes: cols[1],
        preview: cols[2],
        status_bar,
    }
}

/// Narrow-but-tall terminals: the same focused/collapsed proportions as
/// `columned`, just stacked top-to-bottom instead of side-by-side, so each
/// visible panel still gets the terminal's *full* width to work with.
fn stacked(main: Rect, status_bar: Rect, focus: Focus) -> Areas {
    let (notebooks_c, notes_c, preview_c) = tier_constraints(focus);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .spacing(0)
        .constraints([notebooks_c, notes_c, preview_c])
        .split(main);
    Areas {
        notebooks: rows[0],
        notes: rows[1],
        preview: rows[2],
        status_bar,
    }
}

/// Very small terminals: only the focused panel is drawn at all, full
/// screen — no collapsed siblings, since even a 1-column/1-row sliver
/// doesn't fit alongside a panel that's actually usable at this size.
fn single(main: Rect, status_bar: Rect, focus: Focus) -> Areas {
    let zero = Rect {
        x: main.x,
        y: main.y,
        width: 0,
        height: 0,
    };
    let (notebooks, notes, preview) = match focus {
        Focus::Notebooks => (main, zero, zero),
        Focus::Notes => (zero, main, zero),
        Focus::Preview => (zero, zero, main),
    };
    Areas {
        notebooks,
        notes,
        preview,
        status_bar,
    }
}

/// Shared focused/collapsed proportions for both `columned` and `stacked` —
/// they only differ in which axis these get split along.
fn tier_constraints(focus: Focus) -> (Constraint, Constraint, Constraint) {
    match focus {
        Focus::Notebooks => (
            Constraint::Fill(1),
            Constraint::Fill(2),
            Constraint::Fill(2),
        ),
        Focus::Notes => (
            Constraint::Length(COLLAPSED),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ),
        Focus::Preview => (
            Constraint::Length(COLLAPSED),
            Constraint::Length(COLLAPSED),
            Constraint::Fill(1),
        ),
    }
}

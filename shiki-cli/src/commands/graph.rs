//! `shiki graph` — the note-connection graph (real, resolved `[[wikilinks]]`
//! via `wikilinks::edges`) rendered as a 2D force-directed layout straight
//! in the terminal, Obsidian-graph-style: nodes settle into clusters, hubs
//! pull their satellites around them, and orphans drift to the edges. The
//! layout is classic Fruchterman–Reingold on a plain char canvas — no TUI,
//! no alternate screen, just printed lines, so the output survives in
//! scrollback and can be piped/screenshotted like any other CLI output.
//!
//! Links only resolve within a notebook (titles are notebook-scoped), so
//! the graph of "all notebooks" is naturally a set of disjoint clusters —
//! each notebook's notes are laid out together but never cross-linked.

use anyhow::{Context, Result};
use shiki_core::{wikilinks, Note, Notebook, NotebookStore};
use std::io::IsTerminal;

/// Above this many nodes, only the best-connected ones are drawn (plus a
/// note saying so) — a 500-node hairball helps nobody in 40 rows.
const MAX_NODES: usize = 60;
const LABEL_MAX: usize = 18;

struct GraphNode {
    title: String,
    notebook: String,
    degree: usize,
}

pub fn run(
    store: &NotebookStore,
    notebook: Option<&str>,
    width: Option<u16>,
    json: bool,
) -> Result<()> {
    // Per-notebook pools, since links can't cross notebooks — edges are
    // computed inside each pool, then merged with shifted indices.
    let notebooks: Vec<Notebook> = match notebook {
        Some(name) => vec![store.get(name).with_context(|| {
            format!("notebook '{name}' not found \u{2014} see `shiki notebook list`")
        })?],
        None => store.list()?,
    };
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    let mut orphan_titles: Vec<String> = Vec::new();
    for nb in &notebooks {
        let notes: Vec<Note> = nb.all_notes_recursive()?;
        let base = nodes.len();
        let local_edges = wikilinks::edges(&notes);
        for i in wikilinks::orphans(&notes) {
            orphan_titles.push(format!("{}/{}", nb.name, notes[i].frontmatter.title));
        }
        let mut degree = vec![0usize; notes.len()];
        for &(a, b) in &local_edges {
            degree[a] += 1;
            degree[b] += 1;
        }
        for (i, note) in notes.iter().enumerate() {
            nodes.push(GraphNode {
                title: note.frontmatter.title.clone(),
                notebook: nb.name.clone(),
                degree: degree[i],
            });
        }
        edges.extend(local_edges.iter().map(|&(a, b)| (base + a, base + b)));
    }

    if json {
        let out = serde_json::json!({
            "nodes": nodes.iter().map(|n| serde_json::json!({
                "title": n.title, "notebook": n.notebook, "degree": n.degree,
            })).collect::<Vec<_>>(),
            "edges": edges,
            "orphans": orphan_titles,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    if nodes.is_empty() {
        println!("(no notes to graph)");
        return Ok(());
    }

    // Keep only the best-connected nodes when the graph is too big to read.
    let total = nodes.len();
    if total > MAX_NODES {
        let mut by_degree: Vec<usize> = (0..total).collect();
        by_degree.sort_by_key(|&i| std::cmp::Reverse(nodes[i].degree));
        let keep: std::collections::HashSet<usize> =
            by_degree[..MAX_NODES].iter().copied().collect();
        let mut remap = vec![usize::MAX; total];
        let mut kept_nodes = Vec::with_capacity(MAX_NODES);
        for i in 0..total {
            if keep.contains(&i) {
                remap[i] = kept_nodes.len();
                kept_nodes.push(GraphNode {
                    title: nodes[i].title.clone(),
                    notebook: nodes[i].notebook.clone(),
                    degree: nodes[i].degree,
                });
            }
        }
        edges.retain(|&(a, b)| remap[a] != usize::MAX && remap[b] != usize::MAX);
        for e in &mut edges {
            *e = (remap[e.0], remap[e.1]);
        }
        nodes = kept_nodes;
    }

    let width = width
        .or_else(|| crossterm::terminal::size().ok().map(|(w, _)| w))
        .unwrap_or(100)
        .clamp(40, 240) as usize;
    let height = (width / 3).clamp(16, 60);

    let positions = layout(nodes.len(), &edges, width as f32, height as f32);
    let canvas = draw(&nodes, &edges, &positions, width, height);
    let color = std::io::stdout().is_terminal();
    print!("{}", render_canvas(&canvas, color));

    println!();
    let scope = notebook
        .map(|n| format!("notebook '{n}'"))
        .unwrap_or_else(|| "all notebooks".into());
    println!(
        "{} notes \u{B7} {} links \u{B7} {scope}",
        total,
        edges.len()
    );
    if total > MAX_NODES {
        println!("(showing the {MAX_NODES} most-connected notes of {total})");
    }
    if !orphan_titles.is_empty() {
        println!("\norphans (no links in or out):");
        for title in &orphan_titles {
            println!("  \u{25CB} {title}");
        }
    }
    Ok(())
}

/// Fruchterman–Reingold: repulsion between every pair, attraction along
/// edges, cooling over iterations. Deterministic (seeded by index, no RNG)
/// so the same notebook renders the same graph every run — a graph that
/// reshuffles on every invocation reads as noise. Y distances are weighted
/// 2× during force calculation to compensate for terminal cells being
/// roughly twice as tall as wide, so clusters come out round-ish on
/// screen instead of vertically squashed.
fn layout(n: usize, edges: &[(usize, usize)], width: f32, height: f32) -> Vec<(f32, f32)> {
    let mut pos: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            // Deterministic golden-angle spiral as the starting state.
            let a = i as f32 * 2.399_963;
            let r = 0.35 * ((i + 1) as f32 / n as f32).sqrt();
            (width * (0.5 + r * a.cos()), height * (0.5 + r * a.sin()))
        })
        .collect();
    if n <= 1 {
        return pos;
    }
    let area = width * height;
    let k = (area / n as f32).sqrt() * 0.6;
    let iterations = 250;
    let mut temp = width.max(height) / 8.0;
    let cool = temp / iterations as f32;

    for _ in 0..iterations {
        let mut disp = vec![(0f32, 0f32); n];
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = pos[i].0 - pos[j].0;
                let dy = (pos[i].1 - pos[j].1) * 2.0;
                let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                let force = k * k / dist;
                let (ux, uy) = (dx / dist, dy / dist);
                disp[i].0 += ux * force;
                disp[i].1 += uy * force;
                disp[j].0 -= ux * force;
                disp[j].1 -= uy * force;
            }
        }
        for &(a, b) in edges {
            let dx = pos[a].0 - pos[b].0;
            let dy = (pos[a].1 - pos[b].1) * 2.0;
            let dist = (dx * dx + dy * dy).sqrt().max(0.01);
            let force = dist * dist / k;
            let (ux, uy) = (dx / dist, dy / dist);
            disp[a].0 -= ux * force;
            disp[a].1 -= uy * force;
            disp[b].0 += ux * force;
            disp[b].1 += uy * force;
        }
        for i in 0..n {
            let (dx, dy) = disp[i];
            let len = (dx * dx + dy * dy).sqrt().max(0.01);
            let step = len.min(temp);
            pos[i].0 = (pos[i].0 + dx / len * step).clamp(1.0, width - 2.0);
            pos[i].1 = (pos[i].1 + dy / len * step).clamp(1.0, height - 2.0);
        }
        temp = (temp - cool).max(0.05);
    }
    pos
}

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    Empty,
    Edge(char),
    Node(char),
    Label(char),
    Orphan(char),
}

fn draw(
    nodes: &[GraphNode],
    edges: &[(usize, usize)],
    positions: &[(f32, f32)],
    width: usize,
    height: usize,
) -> Vec<Vec<Cell>> {
    let mut canvas = vec![vec![Cell::Empty; width]; height];
    let cell_of = |p: (f32, f32)| -> (usize, usize) {
        (
            (p.0.round() as usize).min(width - 1),
            (p.1.round() as usize).min(height - 1),
        )
    };

    // Edges first — nodes and labels overwrite them, never the reverse.
    for &(a, b) in edges {
        let (x0, y0) = cell_of(positions[a]);
        let (x1, y1) = cell_of(positions[b]);
        for (x, y) in line_cells(x0 as i32, y0 as i32, x1 as i32, y1 as i32) {
            let (x, y) = (x as usize, y as usize);
            if matches!(canvas[y][x], Cell::Empty) {
                canvas[y][x] = Cell::Edge(edge_char(x0 as i32, y0 as i32, x1 as i32, y1 as i32));
            }
        }
    }

    for (i, node) in nodes.iter().enumerate() {
        let (x, y) = cell_of(positions[i]);
        let marker = if node.degree >= 3 {
            '\u{25C9}' // ◉ hub
        } else if node.degree == 0 {
            '\u{25CB}' // ○ orphan
        } else {
            '\u{25CF}' // ● regular
        };
        canvas[y][x] = if node.degree == 0 {
            Cell::Orphan(marker)
        } else {
            Cell::Node(marker)
        };
        let mut label: String = node.title.chars().take(LABEL_MAX).collect();
        if node.title.chars().count() > LABEL_MAX {
            label.push('\u{2026}');
        }
        // Label to the right of the marker, or to the left when it would
        // run off the canvas — stopping at another node/label so two close
        // nodes truncate rather than overwrite each other.
        let chars: Vec<char> = label.chars().collect();
        if x + 2 + chars.len() <= width {
            for (dx, ch) in chars.iter().enumerate() {
                let cx = x + 2 + dx;
                match canvas[y][cx] {
                    Cell::Node(_) | Cell::Label(_) | Cell::Orphan(_) => break,
                    _ => canvas[y][cx] = Cell::Label(*ch),
                }
            }
        } else if x >= chars.len() + 2 {
            for (dx, ch) in chars.iter().enumerate() {
                let cx = x - 2 - chars.len() + 1 + dx;
                match canvas[y][cx] {
                    Cell::Node(_) | Cell::Label(_) | Cell::Orphan(_) => break,
                    _ => canvas[y][cx] = Cell::Label(*ch),
                }
            }
        }
    }
    canvas
}

/// Bresenham, excluding both endpoints (the node markers live there).
fn line_cells(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        if (x, y) != (x0, y0) && (x, y) != (x1, y1) {
            cells.push((x, y));
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
    cells
}

fn edge_char(x0: i32, y0: i32, x1: i32, y1: i32) -> char {
    let (dx, dy) = ((x1 - x0).abs(), (y1 - y0).abs());
    if dy == 0 {
        '\u{2500}' // ─
    } else if dx == 0 {
        '\u{2502}' // │
    } else if (x1 > x0) == (y1 > y0) {
        '\u{2572}' // ╲
    } else {
        '\u{2571}' // ╱
    }
}

fn render_canvas(canvas: &[Vec<Cell>], color: bool) -> String {
    const DIM: &str = "\x1b[2m";
    const CYAN: &str = "\x1b[36m";
    const YELLOW: &str = "\x1b[33m";
    const RESET: &str = "\x1b[0m";
    let mut out = String::new();
    for row in canvas {
        let mut line = String::new();
        for cell in row {
            match cell {
                Cell::Empty => line.push(' '),
                Cell::Edge(c) if color => line.push_str(&format!("{DIM}{c}{RESET}")),
                Cell::Node(c) if color => line.push_str(&format!("{YELLOW}{c}{RESET}")),
                Cell::Orphan(c) if color => line.push_str(&format!("{DIM}{c}{RESET}")),
                Cell::Label(c) if color => line.push_str(&format!("{CYAN}{c}{RESET}")),
                Cell::Edge(c) | Cell::Node(c) | Cell::Label(c) | Cell::Orphan(c) => line.push(*c),
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

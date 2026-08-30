//! Renders the *content* of a ` ```mermaid ` fence as a readable terminal
//! diagram instead of raw Mermaid source. A terminal can't render an actual
//! SVG/PNG diagram, but it can do a lot better than echoing the markup:
//! flowcharts (`graph TD`/`flowchart LR`, …) are laid out as an indented tree
//! with box-drawing connectors and node shapes, and sequence diagrams render
//! participants as columns with messages drawn between them.
//!
//! This is a deliberately small hand-rolled parser (matching the codebase's
//! `tasks.rs`/`query.rs`/`mathfmt.rs` convention — no parser-combinator
//! dependency), covering the constructs notes actually use: node definitions
//! (`A[Label]`, `A(Label)`, `A{Label}`, `A((Label))`, `A[[Label]]`, `A>Label]`),
//! edges (`-->`, `---`, `-.->`, `==>`, `~~~`, with `-- label -->`/`-->|label|`
//! label forms), `subgraph` blocks, and sequence diagrams (`participant X`,
//! `X->>Y: message`, `X-->>Y:`, `X->Y:`, `X--xY:`, `X-)Y:`). Anything it can't
//! parse returns `None` so the caller falls back to the existing flat styling —
//! a note with an exotic diagram still shows *something*, just not prettified.
//!
//! Pure function of a string + colors — unit-testable without a TUI.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::collections::{HashMap, HashSet};

/// Node shapes Mermaid draws — each maps to a distinct bracket style in the
/// terminal so the diagram's structure is readable at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `A[Label]` — rectangle
    Rect,
    /// `A(Label)` — rounded rectangle
    Rounded,
    /// `A{Label}` — diamond / decision
    Diamond,
    /// `A((Label))` — circle
    Circle,
    /// `A[[Label]]` — subroutine / subprocess
    Subroutine,
    /// `A>Label]` — asymmetric (flag)
    Asym,
    /// bare `A` — plain node, no shape
    Plain,
}

impl Shape {
    /// The open bracket for this shape — `None` for `Plain`.
    fn open(self) -> Option<&'static str> {
        Some(match self {
            Shape::Rect => "[",
            Shape::Rounded => "(",
            Shape::Diamond => "{",
            Shape::Circle => "((",
            Shape::Subroutine => "[[",
            Shape::Asym => ">",
            Shape::Plain => return None,
        })
    }
    fn close(self) -> &'static str {
        match self {
            Shape::Rect => "]",
            Shape::Rounded => ")",
            Shape::Diamond => "}",
            Shape::Circle => "))",
            Shape::Subroutine => "]]",
            Shape::Asym => "]",
            Shape::Plain => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeKind {
    /// `-->`
    Solid,
    /// `---` (no arrowhead)
    Link,
    /// `-.->`
    Dotted,
    /// `-.-` (no arrowhead)
    DottedLink,
    /// `==>`
    Thick,
    /// `===` (no arrowhead)
    ThickLink,
    /// `~~>`
    Invisible,
}

impl EdgeKind {
    fn dash(self) -> &'static str {
        match self {
            EdgeKind::Solid => "─",
            EdgeKind::Link => "─",
            EdgeKind::Dotted | EdgeKind::Invisible => "·",
            EdgeKind::Thick => "═",
            EdgeKind::ThickLink => "═",
            EdgeKind::DottedLink => "·",
        }
    }
    fn arrow(self) -> &'static str {
        match self {
            EdgeKind::Solid | EdgeKind::Dotted | EdgeKind::Thick | EdgeKind::Invisible => "▶",
            EdgeKind::Link | EdgeKind::ThickLink | EdgeKind::DottedLink => "",
        }
    }
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    shape: Shape,
    label: String,
}

#[derive(Debug, Clone)]
struct Edge {
    from: String,
    to: String,
    label: Option<String>,
    kind: EdgeKind,
}

#[derive(Debug, Clone)]
struct Graph {
    /// `"TD"`, `"LR"`, `"BT"`, `"RL"` — used for the arrow glyph.
    direction: String,
    /// Node definitions in first-seen order.
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

/// Which diagram kind a mermaid source is — the parser picks the renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagramKind {
    Graph,
    Sequence,
}

/// Renders a mermaid source's content into styled lines, or `None` if it
/// isn't a parseable flowchart/sequence diagram (caller falls back to the
/// flat text styling). Colors come from the active theme so the diagram reads
/// with the same accent/muted conventions as everything else in PREVIEW.
pub fn render(
    source: &str,
    fg: ratatui::style::Color,
    accent: ratatui::style::Color,
    muted: ratatui::style::Color,
) -> Option<(DiagramKind, Vec<Line<'static>>)> {
    let trimmed = source.trim();
    if let Some(kind) = kind_of(trimmed) {
        let styles = Styles { fg, accent, muted };
        let lines = match kind {
            DiagramKind::Graph => render_graph(&parse_graph(trimmed)?, &styles),
            DiagramKind::Sequence => render_sequence(&parse_sequence(trimmed)?, &styles),
        };
        Some((kind, lines))
    } else {
        None
    }
}

fn kind_of(source: &str) -> Option<DiagramKind> {
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if line.starts_with("graph ") || line.starts_with("flowchart ") {
            return Some(DiagramKind::Graph);
        }
        if line.starts_with("sequenceDiagram") || line == "sequence" {
            return Some(DiagramKind::Sequence);
        }
        // A first content line that isn't a diagram header means this isn't a
        // parseable mermaid diagram — give up rather than guess.
        return None;
    }
    None
}

fn is_directive(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("");
    matches!(
        first,
        "classDef"
            | "class"
            | "style"
            | "linkStyle"
            | "click"
            | "direction"
            | "accTitle"
            | "accDescr"
    )
}

// --- graph / flowchart parser ------------------------------------------------

fn parse_graph(source: &str) -> Option<Graph> {
    let mut graph = Graph {
        direction: "TD".to_string(),
        nodes: Vec::new(),
        edges: Vec::new(),
    };
    let mut by_id: HashMap<String, Node> = HashMap::new();
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if let Some(rest) = line
            .strip_prefix("graph ")
            .or_else(|| line.strip_prefix("flowchart "))
        {
            graph.direction = rest
                .split_whitespace()
                .next()
                .unwrap_or("TD")
                .trim_end_matches(',')
                .to_ascii_uppercase();
            continue;
        }
        if line.starts_with("subgraph") || line == "end" || is_directive(line) {
            continue;
        }
        parse_statement(line, &mut graph, &mut by_id);
    }
    // Every node that appears only as an edge endpoint still needs a Node
    // entry (with a label equal to its id) so the renderer can draw it.
    for edge in graph.edges.clone() {
        for id in [edge.from.clone(), edge.to.clone()] {
            if let std::collections::hash_map::Entry::Vacant(v) = by_id.entry(id.clone()) {
                let node = Node {
                    id: id.clone(),
                    shape: Shape::Plain,
                    label: id.clone(),
                };
                graph.nodes.push(node.clone());
                v.insert(node);
            }
        }
    }
    Some(graph)
}

/// Parses one flowchart statement line, which may hold several
/// `node --> node --> node` chains separated by `;`.
fn parse_statement(line: &str, graph: &mut Graph, by_id: &mut HashMap<String, Node>) {
    let mut rest = line.trim();
    let mut previous: Option<String> = None;
    while !rest.is_empty() {
        // A `;` separates statements within a line.
        if let Some(idx) = rest.find(';') {
            let before = &rest[..idx];
            if !before.trim().is_empty() {
                parse_statement(before, graph, by_id);
            }
            rest = rest[idx + 1..].trim();
            continue;
        }
        if let Some((consumed, node)) = read_node(rest) {
            let id = node.id.clone();
            register_node(node, graph, by_id);
            previous = Some(id);
            rest = rest[consumed..].trim();
            continue;
        }
        if let Some((consumed, kind, label)) = read_edge(rest) {
            if let Some(from) = previous.take() {
                // The edge's target is the next node definition in the
                // remaining text.
                rest = rest[consumed..].trim();
                if let Some((nconsumed, to_node)) = read_node(rest) {
                    let to = to_node.id.clone();
                    register_node(to_node, graph, by_id);
                    graph.edges.push(Edge {
                        from,
                        to,
                        label,
                        kind,
                    });
                    previous = Some(graph.edges.last().unwrap().to.clone());
                    rest = rest[nconsumed..].trim();
                    continue;
                }
                // No node after the edge — drop the dangling edge.
                previous = None;
                continue;
            }
            rest = rest[consumed..].trim();
            continue;
        }
        break;
    }
}

fn register_node(node: Node, graph: &mut Graph, by_id: &mut HashMap<String, Node>) {
    if !by_id.contains_key(&node.id) {
        graph.nodes.push(node.clone());
        by_id.insert(node.id.clone(), node);
    }
}

/// Reads one node definition at the start of `s` — `A`, `A[Label]`, `A(Label)`,
/// `A{Label}`, `A((Label))`, `A[[Label]]`, `A>Label]`. Returns the bytes
/// consumed and the node.
fn read_node(s: &str) -> Option<(usize, Node)> {
    let bytes = s.as_bytes();
    let mut id = String::new();
    let mut i = 0usize;
    if i < bytes.len() && bytes[i] == b'"' {
        // quoted id: `"my id"[Label]`
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i] != b'"' {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        id = s[start..i].to_string();
        i += 1; // closing quote
    } else {
        // Ids start with a letter/digit/underscore (never `-`, which is an
        // edge-operator marker); later chars may include `-`.
        if i >= bytes.len() || !(bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            return None;
        }
        while i < bytes.len()
            && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'-')
        {
            id.push(bytes[i] as char);
            i += 1;
        }
    }
    // Optional shape bracket — a bare id (no bracket) is a Plain node.
    let rest = &s[i..];
    let shape = match rest.chars().next() {
        Some('[') if rest.starts_with("[[") => Shape::Subroutine,
        Some('(') if rest.starts_with("((") => Shape::Circle,
        Some('[') => Shape::Rect,
        Some('(') => Shape::Rounded,
        Some('{') => Shape::Diamond,
        Some('>') => Shape::Asym,
        _ => {
            return Some((
                i,
                Node {
                    id: id.clone(),
                    shape: Shape::Plain,
                    label: id,
                },
            ))
        }
    };
    let (open, close) = match shape {
        Shape::Rect => ("[", "]"),
        Shape::Rounded => ("(", ")"),
        Shape::Diamond => ("{", "}"),
        Shape::Asym => (">", "]"),
        Shape::Subroutine => ("[[", "]]"),
        Shape::Circle => ("((", "))"),
        Shape::Plain => unreachable!(),
    };
    let inner = read_bracketed(&s[i..], open, close)?;
    let label = strip_quotes(inner.trim()).to_string();
    Some((
        i + open.len() + inner.len() + close.len(),
        Node { id, shape, label },
    ))
}

/// Reads `open ... close` at the start of `s` and returns the inner text.
fn read_bracketed<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let inner = s.strip_prefix(open)?;
    let end = inner.find(close)?;
    Some(&inner[..end])
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Reads an edge operator at the start of `s`, returning (consumed, edge kind,
/// optional label). Supports `-->`, `---`, `-.->`, `-.-`, `==>`, `===`, `~~~`,
/// plus the labeled forms `-- label -->`, `-. label .->`, `== label ==>`,
/// `-->|label|`, and endpoint markers `--o`/`--x` (rendered as a solid edge).
/// The caller then parses the edge's target node from what remains.
fn read_edge(s: &str) -> Option<(usize, EdgeKind, Option<String>)> {
    // Label-between-dashes forms first: `-- label -->` / `-. label .->` /
    // `== label ==>`.
    for (open, close, kind) in [
        ("--", "-->", EdgeKind::Solid),
        ("-.", ".->", EdgeKind::Dotted),
        ("==", "==>", EdgeKind::Thick),
    ] {
        if let Some(rest) = s.strip_prefix(open) {
            if let Some(idx) = rest.find(close) {
                let label = strip_quotes(rest[..idx].trim());
                if !label.is_empty() {
                    let consumed = open.len() + idx + close.len();
                    return Some((consumed, kind, Some(label.to_string())));
                }
            }
        }
    }
    // Bare operators.
    let (len, kind) = if s.starts_with("-->") {
        (3, EdgeKind::Solid)
    } else if s.starts_with("---") {
        (3, EdgeKind::Link)
    } else if s.starts_with("-.->") {
        (4, EdgeKind::Dotted)
    } else if s.starts_with("-.-") {
        (3, EdgeKind::DottedLink)
    } else if s.starts_with("==>") {
        (3, EdgeKind::Thick)
    } else if s.starts_with("===") {
        (3, EdgeKind::ThickLink)
    } else if s.starts_with("~~~") {
        (3, EdgeKind::Invisible)
    } else if s.starts_with("--o") || s.starts_with("--x") {
        (3, EdgeKind::Solid)
    } else {
        return None;
    };
    let rest = &s[len..];
    // Optional `|label|` right after the arrow: `-->|text|`.
    if let Some(label_rest) = rest.strip_prefix('|') {
        if let Some(idx) = label_rest.find('|') {
            let label = strip_quotes(label_rest[..idx].trim());
            let consumed = len + 1 + idx + 1;
            return Some((consumed, kind, Some(label.to_string())));
        }
    }
    Some((len, kind, None))
}

// --- sequence diagram parser ------------------------------------------------

#[derive(Debug, Clone)]
struct Participant {
    id: String,
    label: String,
}

#[derive(Debug, Clone)]
struct SeqMessage {
    from: String,
    to: String,
    /// `Some(true)` = dashed (`-->>`/`-->`), `Some(false)` = solid, `None` = cross
    dashed: Option<bool>,
    text: String,
}

#[derive(Debug, Clone)]
struct Sequence {
    participants: Vec<Participant>,
    messages: Vec<SeqMessage>,
}

fn parse_sequence(source: &str) -> Option<Sequence> {
    let mut seq = Sequence {
        participants: Vec::new(),
        messages: Vec::new(),
    };
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if line.starts_with("sequenceDiagram") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("participant ") {
            let mut parts = rest.splitn(3, char::is_whitespace);
            let id = parts.next().unwrap_or("").trim().to_string();
            let label = parts
                .next()
                .filter(|p| p == &"as")
                .and_then(|_| parts.next())
                .map(|l| strip_quotes(l.trim()).to_string())
                .unwrap_or_else(|| id.clone());
            seq.participants.push(Participant { id, label });
            continue;
        }
        if let Some(rest) = line.strip_prefix("actor ") {
            let id = rest.trim().to_string();
            seq.participants.push(Participant {
                id: id.clone(),
                label: id,
            });
            continue;
        }
        // `Note left of A:` / `Note over A,B:` — render as a standalone line.
        if line.starts_with("Note ") {
            let text = line.split_once(':').map(|(_, t)| t.trim()).unwrap_or("");
            seq.messages.push(SeqMessage {
                from: String::new(),
                to: String::new(),
                dashed: Some(false),
                text: format!("✎ {text}"),
            });
            continue;
        }
        if let Some((from, dashed, to, text)) = parse_message(line) {
            seq.messages.push(SeqMessage {
                from,
                to,
                dashed,
                text,
            });
        }
    }
    // A message referencing an unknown participant adds it implicitly (a
    // common shorthand), labeled by its id.
    for msg in &seq.messages {
        for id in [&msg.from, &msg.to] {
            if !id.is_empty() && !seq.participants.iter().any(|p| &p.id == id) {
                seq.participants.push(Participant {
                    id: id.clone(),
                    label: id.clone(),
                });
            }
        }
    }
    Some(seq)
}

/// `A->>B: text`, `A-->>B:`, `A->B:`, `A-->B:`, `A--xB:`, `A-)B:`.
fn parse_message(line: &str) -> Option<(String, Option<bool>, String, String)> {
    let (head, text) = match line.split_once(':') {
        Some((h, t)) => (h.trim(), t.trim().to_string()),
        None => return None,
    };
    for (op, dashed) in [
        ("-->>", Some(true)),
        ("--x", None),
        ("--)", Some(true)),
        ("-->", Some(true)),
        ("->>", Some(false)),
        ("-x", None),
        ("-)", Some(false)),
        ("->", Some(false)),
    ] {
        if let Some((from, to)) = head.split_once(op) {
            return Some((from.trim().to_string(), dashed, to.trim().to_string(), text));
        }
    }
    None
}

// --- rendering --------------------------------------------------------------

struct Styles {
    fg: ratatui::style::Color,
    accent: ratatui::style::Color,
    muted: ratatui::style::Color,
}

type Outgoing = HashMap<String, Vec<(String, Option<String>, EdgeKind)>>;

/// Shared context for the recursive graph walk — `id`/`prefix`/`visited`/`out`
/// change per call, everything else is constant across the walk.
struct GraphLayout<'a> {
    by_id: &'a HashMap<String, Node>,
    outgoing: &'a Outgoing,
    styles: &'a Styles,
}

impl GraphLayout<'_> {
    /// Recursive descent. `visited` guards against cycles (a node already
    /// drawn is rendered as a muted reference line instead of recursing).
    fn walk(
        &self,
        id: &str,
        prefix: &str,
        visited: &mut HashSet<String>,
        out: &mut Vec<Line<'static>>,
    ) {
        if visited.contains(id) {
            let node = self.by_id.get(id);
            out.push(match node {
                Some(n) => Line::from(vec![Span::styled(
                    format!("{prefix}… {}", node_box_text(n)),
                    Style::default().fg(self.styles.muted),
                )]),
                None => Line::from(Span::styled(
                    format!("{prefix}… {id}"),
                    Style::default().fg(self.styles.muted),
                )),
            });
            return;
        }
        visited.insert(id.to_string());
        if let Some(node) = self.by_id.get(id) {
            out.push(node_line(node, self.styles, prefix));
        }
        let children = self.outgoing.get(id).cloned().unwrap_or_default();
        let last = children.len().saturating_sub(1);
        for (i, (to, label, kind)) in children.into_iter().enumerate() {
            let is_last = i == last;
            let branch = if is_last { "└─" } else { "├─" };
            let child_prefix = format!("{prefix}{}", if is_last { "   " } else { "│  " });
            out.push(edge_line(
                prefix,
                branch,
                label.as_deref(),
                kind,
                self.styles,
            ));
            self.walk(&to, &child_prefix, visited, out);
        }
    }
}

/// Renders the node box for a node: `[Label]`, `(Label)`, `{Label}`, `((Label))`,
/// `[[Label]]`, `>Label]`, or bare `Label`. The label is accent-colored, the
/// shape brackets muted.
fn node_line(node: &Node, styles: &Styles, prefix: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![Span::styled(
        prefix.to_string(),
        Style::default().fg(styles.muted),
    )];
    if let Some(open) = node.shape.open() {
        spans.push(Span::styled(
            open.to_string(),
            Style::default().fg(styles.muted),
        ));
    }
    spans.push(Span::styled(
        node.label.clone(),
        Style::default()
            .fg(styles.accent)
            .add_modifier(Modifier::BOLD),
    ));
    let close = node.shape.close();
    if !close.is_empty() {
        spans.push(Span::styled(
            close.to_string(),
            Style::default().fg(styles.muted),
        ));
    }
    Line::from(spans)
}

/// One edge row: `{prefix}{branch} {label?}{dash}{arrow}`.
fn edge_line(
    prefix: &str,
    branch: &str,
    label: Option<&str>,
    kind: EdgeKind,
    styles: &Styles,
) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{prefix}{branch} "),
        Style::default().fg(styles.muted),
    )];
    if let Some(label) = label {
        spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(styles.fg),
        ));
        spans.push(Span::styled(" ", Style::default().fg(styles.muted)));
    }
    let dash = kind.dash();
    let arrow = kind.arrow();
    spans.push(Span::styled(
        format!("{dash}{dash}{dash}{arrow}"),
        Style::default().fg(styles.accent),
    ));
    Line::from(spans)
}

fn render_graph(graph: &Graph, styles: &Styles) -> Vec<Line<'static>> {
    let mut outgoing: HashMap<String, Vec<(String, Option<String>, EdgeKind)>> = HashMap::new();
    let mut indegree: HashMap<String, usize> = HashMap::new();
    for node in &graph.nodes {
        outgoing.entry(node.id.clone()).or_default();
        indegree.entry(node.id.clone()).or_insert(0);
    }
    for edge in &graph.edges {
        outgoing.entry(edge.from.clone()).or_default().push((
            edge.to.clone(),
            edge.label.clone(),
            edge.kind,
        ));
        *indegree.entry(edge.to.clone()).or_insert(0) += 1;
    }
    // Roots = nodes with no incoming edge, in definition order.
    let mut roots: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| indegree.get(&n.id).copied().unwrap_or(0) == 0)
        .map(|n| n.id.clone())
        .collect();
    if roots.is_empty() {
        // All nodes in a cycle — pick the first as root.
        roots = graph.nodes.iter().map(|n| n.id.clone()).collect();
    }
    let by_id: HashMap<String, Node> = graph
        .nodes
        .iter()
        .cloned()
        .map(|n| (n.id.clone(), n))
        .collect();
    let mut visited: HashSet<String> = HashSet::new();
    let mut out: Vec<Line<'static>> = Vec::new();

    let layout = GraphLayout {
        by_id: &by_id,
        outgoing: &outgoing,
        styles,
    };
    for root in roots {
        if visited.contains(&root) {
            continue;
        }
        layout.walk(&root, "", &mut visited, &mut out);
    }
    // Unreachable nodes (present in the def but not connected to any root).
    for node in &graph.nodes {
        if !visited.contains(&node.id) {
            out.push(node_line(node, styles, ""));
        }
    }
    out
}

fn node_box_text(node: &Node) -> String {
    match node.shape.open() {
        Some(open) => format!("{open}{}{}", node.label, node.shape.close()),
        None => node.label.clone(),
    }
}

fn render_sequence(seq: &Sequence, styles: &Styles) -> Vec<Line<'static>> {
    let mut out: Vec<Line<'static>> = Vec::new();
    if seq.participants.is_empty() {
        return out;
    }
    // Column layout: each participant gets a fixed-width column wide enough
    // for its label (min 3), plus a leading pipe column.
    let widths: Vec<usize> = seq
        .participants
        .iter()
        .map(|p| p.label.chars().count().max(3) + 1)
        .collect();
    let col_starts: Vec<usize> = {
        let mut starts = Vec::with_capacity(widths.len());
        let mut acc = 0usize;
        for w in &widths {
            starts.push(acc);
            acc += w;
        }
        starts
    };
    let total = col_starts
        .last()
        .map(|s| s + widths.last().unwrap())
        .unwrap_or(0);

    // Header row: participant labels in their columns.
    let mut header_spans = vec![Span::styled(" ".repeat(col_starts[0]), Style::default())];
    for (i, p) in seq.participants.iter().enumerate() {
        let pad = col_starts[i] - header_len(&header_spans);
        header_spans.push(Span::styled(
            " ".repeat(pad),
            Style::default().fg(styles.muted),
        ));
        header_spans.push(Span::styled(
            p.label.clone(),
            Style::default()
                .fg(styles.accent)
                .add_modifier(Modifier::BOLD),
        ));
    }
    out.push(Line::from(header_spans));

    // Messages: pipes under every participant, arrows between source/target.
    for msg in &seq.messages {
        let from_idx = seq.participants.iter().position(|p| p.id == msg.from);
        let to_idx = seq.participants.iter().position(|p| p.id == msg.to);
        if msg.from.is_empty() && msg.to.is_empty() {
            // A note — just its text, indented.
            out.push(Line::from(Span::styled(
                format!("   {}", msg.text),
                Style::default().fg(styles.muted),
            )));
            continue;
        }
        let (from_idx, to_idx) = match (from_idx, to_idx) {
            (Some(f), Some(t)) => (f, t),
            _ => continue,
        };
        let mut row = vec![' '; total];
        // Pipes under every participant.
        for &start in col_starts.iter() {
            row[start] = '│';
        }
        let dashed = msg.dashed.unwrap_or(false);
        let fill = if dashed { '·' } else { '─' };
        // Fill the span between the two participants' columns (leaving the
        // pipes themselves intact).
        let lo = col_starts[from_idx.min(to_idx)];
        let hi = col_starts[from_idx.max(to_idx)];
        if from_idx == to_idx {
            // A self-message (`A->>A: text`): there is no span to fill and
            // slicing `lo + 1..hi` would panic, so mark the participant's
            // own pipe with a loop glyph instead.
            row[lo] = '↺';
        } else {
            for cell in row[lo + 1..hi].iter_mut() {
                if *cell == ' ' {
                    *cell = fill;
                }
            }
            // Arrowhead on the target's pipe: `▶` when moving right, `◀` left.
            row[col_starts[to_idx]] = if from_idx < to_idx { '▶' } else { '◀' };
        }
        let mut spans = vec![Span::styled(
            row.iter().collect::<String>(),
            Style::default().fg(styles.accent),
        )];
        if !msg.text.is_empty() {
            spans.push(Span::styled(
                format!("  {}", msg.text),
                Style::default().fg(styles.fg),
            ));
        }
        out.push(Line::from(spans));
    }
    out
}

fn header_len(spans: &[Span<'static>]) -> usize {
    spans.iter().map(|s| s.content.chars().count()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    const FG: Color = Color::White;
    const ACCENT: Color = Color::Blue;
    const MUTED: Color = Color::Gray;

    fn text(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn unknown_source_returns_none() {
        assert!(render("not a diagram at all", FG, ACCENT, MUTED).is_none());
        assert!(render("", FG, ACCENT, MUTED).is_none());
    }

    #[test]
    fn graph_kind_is_detected() {
        let (kind, _) = render("graph TD\nA --> B", FG, ACCENT, MUTED).unwrap();
        assert_eq!(kind, DiagramKind::Graph);
    }

    #[test]
    fn renders_node_shapes_and_tree() {
        let src = "graph TD\nA[Christmas] -->|Get money| B(Go shopping)\nB --> C{Let me think}\nC -->|One| D[Laptop]\nC -->|Two| E[iPhone]";
        let (_, lines) = render(src, FG, ACCENT, MUTED).unwrap();
        let t = text(&lines);
        assert_eq!(t[0], "[Christmas]");
        // Root -> child: edge row then the child's node row beneath it.
        assert_eq!(t[1], "└─ Get money ───▶");
        assert_eq!(t[2], "   (Go shopping)");
        assert!(t.iter().any(|l| l.contains("{Let me think}")), "{t:?}");
        assert!(t.iter().any(|l| l.contains("[Laptop]")), "{t:?}");
        assert!(t.iter().any(|l| l.contains("[iPhone]")), "{t:?}");
        assert!(t.iter().any(|l| l.contains("├─ One ───▶")), "{t:?}");
        assert!(t.iter().any(|l| l.contains("└─ Two ───▶")), "{t:?}");
    }

    #[test]
    fn direction_header_sets_uppercase() {
        let (_, lines) = render("flowchart LR\nA --> B", FG, ACCENT, MUTED).unwrap();
        let t = text(&lines);
        assert_eq!(t[0], "A");
        assert_eq!(t[1], "└─ ───▶");
        assert_eq!(t[2], "   B");
    }

    #[test]
    fn bare_ids_become_plain_nodes() {
        let (_, lines) = render("graph TD\nA --> B", FG, ACCENT, MUTED).unwrap();
        let t = text(&lines);
        assert_eq!(t[0], "A");
        assert_eq!(t[1], "└─ ───▶");
        assert_eq!(t[2], "   B");
    }

    #[test]
    fn edge_label_between_dashes() {
        let (_, lines) = render("graph TD\nA -- hello --> B", FG, ACCENT, MUTED).unwrap();
        let t = text(&lines);
        assert!(t[1].contains("hello"), "{t:?}");
    }

    #[test]
    fn sequence_participants_and_messages_render() {
        let src = "sequenceDiagram\nparticipant Alice\nparticipant Bob\nAlice->>Bob: Hello Bob\nBob-->>Alice: Hi Alice";
        let (kind, lines) = render(src, FG, ACCENT, MUTED).unwrap();
        assert_eq!(kind, DiagramKind::Sequence);
        let t = text(&lines);
        assert!(t[0].contains("Alice"), "{t:?}");
        assert!(t[0].contains("Bob"), "{t:?}");
        assert!(t.iter().any(|l| l.contains("Hello Bob")), "{t:?}");
        assert!(t.iter().any(|l| l.contains("Hi Alice")), "{t:?}");
        assert!(t[1].contains('▶'), "solid arrow: {t:?}");
        assert!(t[2].contains('◀'), "back arrow: {t:?}");
    }

    #[test]
    fn sequence_shorthand_participants_are_implicit() {
        let src = "sequenceDiagram\nAlice->>Bob: hi";
        let (_, lines) = render(src, FG, ACCENT, MUTED).unwrap();
        let t = text(&lines);
        assert!(t[0].contains("Alice") && t[0].contains("Bob"), "{t:?}");
    }

    #[test]
    fn comment_lines_are_ignored() {
        let (_, lines) = render("graph TD\n%% a comment\nA --> B", FG, ACCENT, MUTED).unwrap();
        let t = text(&lines);
        assert_eq!(t, vec!["A", "└─ ───▶", "   B"]);
    }
    #[test]
    fn sequence_self_message_does_not_panic() {
        let src = "sequenceDiagram\nCron->>DB: read\nCron->>Cron: skip if empty\nCron->>DB: write";
        let (_, lines) = render(src, FG, ACCENT, MUTED).unwrap();
        let t = text(&lines);
        assert!(t[2].contains('↺'), "self-message loop glyph: {t:?}");
        assert!(t[2].contains("skip if empty"), "{t:?}");
        assert!(t[3].contains('▶'), "later messages still render: {t:?}");
    }
}

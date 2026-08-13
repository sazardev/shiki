#!/usr/bin/env bash
# scripts/render-demo.sh — a disposable playground for seeing every Markdown
# construct the PREVIEW renderer supports at once, including the collapsible
# `<details>`/`<summary>` blocks (click the summary row to fold/unfold).
#
# Seeds a "demo" notebook with notes covering headings, lists, checkboxes,
# blockquotes, tables, fenced code blocks (with real syntax highlighting),
# `$$math$$` blocks, horizontal rules, callouts, wikilinks/backlinks, and
# `<details>` sections — so the renderer can be eyeballed without hand-
# writing the notes one at a time.
#
# Uses the exact XDG-override convention CLAUDE.md documents for exercising
# the CLI without touching real user config/data. Directories are wiped on
# every run, so re-running always starts from the same known state.
#
# Usage:
#   scripts/render-demo.sh           seeds data, launches the TUI
#   scripts/render-demo.sh --no-tui  seeds data, prints the seeded paths,
#                                     and exits instead of launching the TUI

set -euo pipefail
cd "$(dirname "$0")/.."

export XDG_CONFIG_HOME=/tmp/shiki-render-config
export XDG_DATA_HOME=/tmp/shiki-render-data
rm -rf "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"

echo "==> building shiki-cli (debug)"
cargo build -p shiki-cli --quiet
BIN=./target/debug/shiki

echo "==> seeding the 'demo' notebook"
"$BIN" notebook create demo >/dev/null

# $1 = title, $2 = body. Multiline bodies are passed as single strings
# with literal newlines ($'...' or heredoc-captured).
new_note() {
    local title="$1" body="$2"
    "$BIN" new "$title" -n demo --body "$body" >/dev/null
}

# --- Headings, lists, checkboxes, blockquotes -------------------------------
new_note "Formatting overview" $'# Big heading\n\n## Second level\n\n### Third level\n\nIntro text with **bold**, *italic*, `inline code`, a [link](https://example.com) and an ![image](https://example.com/img.png).\n\n- bullet one\n- bullet two\n  - nested bullet\n\n1. ordered one\n2. ordered two\n\n- [ ] open task\n- [x] done task\n\n> A blockquote with **bold** inside.\n\n---\n\nTrailing paragraph after the rule.'

# --- Tables -----------------------------------------------------------------
new_note "Data tables" $'# Tables\n\n| Name | Status | Priority |\n| --- | --- | --- |\n| login | done | high |\n| docs | pending | medium |\n| auth | in-progress | high |\n\nA paragraph after the table.'

# --- Code fences with syntax highlighting ----------------------------------
new_note "Code samples" $'# Code\n\n```rust file:main.rs\nfn main() {\n    let notes = vec!["one", "two"];\n    for n in &notes {\n        println!("note: {n}");\n    }\n}\n```\n\n```tsx file:App.tsx\nconst App = () => {\n  return <h1>{title}</h1>;\n};\n```\n\n```bash file:deploy.sh\necho "hello shiki" | grep shiki\n```\n\nPlain fence with no language:\n\n```\njust some monospace text\n```'

# --- Math blocks -------------------------------------------------------------
new_note "Math blocks" $'# Math\n\n$$E = mc^2$$\n\n$$\\int_0^\\infty e^{-x^2} dx = \\frac{\\sqrt{\\pi}}{2}$$\n\nInline math $$a^2 + b^2 = c^2$$ stays on its line.'

# --- Callouts, dividers, details ---------------------------------------------
new_note "Callouts and details" $'# Callouts & collapsible sections\n\n> **Note:** this is a note callout.\n\n> **Warning:** careful with this one.\n\n<details>\n<summary>Click me — this block is collapsible</summary>\n\nThis whole section is hidden until you click the summary row.\nIt can hold **any** markdown: lists, code, even nested details.\n\n- hidden bullet\n- another hidden bullet\n\n</details>\n\n<details>\n<summary>A nested details example</summary>\n\n<details>\n<summary>Nested block inside</summary>\n\ndeeply hidden content\n\n</details>\n\nOuter content between the nested blocks.\n\n</details>\n\nPlain paragraph after both collapsible sections.'

# --- Wikilinks / backlinks ----------------------------------------------------
new_note "Hub — links out" $'# Hub\n\nOutgoing links live in this note:\n\n- See [[Formatting overview]]\n- See [[Data tables]]\n- See [[Code samples]]\n- See [[Daily notes]]\n- Broken link to [[This note does not exist]]'

new_note "Daily notes" $'# Daily notes\n\nA short note that is a backlink target. The links modal (leader+B, or L in\nPREVIEW) shows what links to this note.'

# --- A task-heavy note with @due tags ------------------------------------------
new_note "Task list" $'# Tasks\n\n- [ ] review the demo @due(tomorrow)\n- [ ] write more tests @due(+3d)\n- [ ] deploy the release @due(2026-12-31)\n- [x] ship v0.9.0 @due(2026-08-04)\n- [ ] someday maybe'

# --- Mermaid diagrams (flowchart + sequence) -----------------------------------
new_note "Mermaid diagrams" $'# Mermaid\n\nFlowchart:\n\n```mermaid\ngraph TD\n    A[Christmas] -->|Get money| B(Go shopping)\n    B --> C{Let me think}\n    C -->|One| D[Laptop]\n    C -->|Two| E[iPhone]\n```\n\nSequence:\n\n```mermaid\nsequenceDiagram\n    participant Alice\n    participant Bob\n    Alice->>Bob: Hello Bob, how are you?\n    Bob-->>Alice: I am good thanks!\n```'

echo
echo "==> demo notebook ready at \$XDG_DATA_HOME/shiki/demo"
echo
echo "Notes seeded:"
"$BIN" list -n demo | sed 's/^/  /'
echo
echo "Things to try in the TUI:"
echo "  l l                 -> NOTES then PREVIEW, read the note"
echo "  j/k                 -> move between notes"
echo "  click a <summary>   -> fold/unfold a <details> block"
echo "  leader+B            -> links modal (wikilinks/backlinks)"
echo "  leader+t            -> global tasks view"
echo "  leader+q            -> query mode"
echo

if [[ "${1:-}" == "--no-tui" ]]; then
    echo "==> seeded only (--no-tui); launch the TUI with: ./scripts/render-demo.sh"
    exit 0
fi

echo "==> launching the TUI (leader is Space by default) — Esc to close a modal, q to quit"
exec "$BIN"

// CommonMark has no concept of YAML frontmatter — `@lezer/markdown` parses
// shiki's `---\n...\n---\n` block at the top of every note as a *Setext
// heading* (one or more non-blank lines followed by a line of `-`/`=`,
// CommonMark's own rule for that syntax), tagged identically to a real
// `##` heading. That's real, verified live: every frontmatter field
// (`title:`, `date:`, `tags:`, …) rendered as one giant accent-colored
// heading once `markdownHighlight.ts` gave headings a real color — before
// that it was merely bold+underlined default-colored text, so the same
// mis-parse was there all along, just quieter. Since ATX (`# heading`) and
// Setext (`heading\n---`) headings share the exact same tag, a
// HighlightStyle rule can't tell "real heading" from "accidental
// frontmatter" apart — this instead recognizes shiki's specific on-disk
// convention (frontmatter is always the very first lines, opened and
// closed by a bare `---`) directly off the document text and forces those
// lines to a neutral, muted look regardless of whatever tag the parser
// gave them.
import { StateField, type EditorState } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";

const frontmatterLineDeco = Decoration.line({ class: "cm-frontmatter-line" });

function frontmatterLineNumbers(state: EditorState): number[] {
  const doc = state.doc;
  if (doc.lines < 2 || doc.line(1).text.trim() !== "---") return [];
  const lines: number[] = [1];
  for (let n = 2; n <= doc.lines; n++) {
    lines.push(n);
    if (doc.line(n).text.trim() === "---") return lines;
  }
  return []; // no closing delimiter yet (mid-edit) — nothing to decorate
}

function build(state: EditorState): DecorationSet {
  const lines = frontmatterLineNumbers(state);
  if (lines.length === 0) return Decoration.none;
  return Decoration.set(lines.map((n) => frontmatterLineDeco.range(state.doc.line(n).from)));
}

export const frontmatterField = StateField.define<DecorationSet>({
  create: build,
  update(deco, tr) {
    return tr.docChanged ? build(tr.state) : deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

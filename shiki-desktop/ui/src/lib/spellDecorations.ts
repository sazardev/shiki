// Squiggly-underline decorations for misspelled ranges (Ctrl+E) — mirrors
// the TUI's `underline_missed_ranges` (only rows matching the last-checked
// snapshot stay underlined). Simplification vs. the TUI's exact per-row
// tracking: any document edit invalidates the *whole* snapshot rather than
// tracking which specific ranges are still valid post-edit — much simpler,
// and a stale underline is a minor cosmetic issue, not a correctness one
// (the next Ctrl+E re-checks from scratch regardless).
import { StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";

export interface SpellRange {
  start: number;
  end: number;
}

export const setSpellIssues = StateEffect.define<SpellRange[]>();

const spellMark = Decoration.mark({ class: "cm-misspell" });

export const spellField = StateField.define<DecorationSet>({
  create() {
    return Decoration.none;
  },
  update(deco, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setSpellIssues)) {
        return Decoration.set(
          effect.value.filter((r) => r.start < r.end).map((r) => spellMark.range(r.start, r.end)),
          true,
        );
      }
    }
    if (tr.docChanged) return Decoration.none;
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

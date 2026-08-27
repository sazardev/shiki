// Custom CodeMirror 6 commands for parity with shiki-tui's InlineEditor
// (`handle_edit_key`, `shiki-tui/src/key_handlers.rs`) — only the behaviors
// CM6 doesn't already provide for free via `basicSetup`/`@codemirror/commands`/
// `@codemirror/search` (undo/redo, bracket auto-pair + wrap-on-selection,
// multi-selection plumbing, and — once wired in EditorPane.svelte —
// find/replace and select-next-occurrence all come from those packages
// directly, no custom code needed here).
import { EditorSelection, type ChangeSpec } from "@codemirror/state";
import { EditorView, type Command } from "@codemirror/view";
import { indentMore, indentLess } from "@codemirror/commands";

// ---- list-line detection, shared by Tab/Enter/Backspace ----

const CHECKBOX_RE = /^(\s*)([-*])(\s\[[ xX]\])(\s)/;
const BULLET_RE = /^(\s*)([-*])(\s)/;
const ORDERED_RE = /^(\s*)(\d+)([.)])(\s)/;

interface ListMatch {
  /// The whole matched prefix, e.g. `"  - [ ] "`.
  prefix: string;
  /// The prefix to continue with on the *next* line — a checkbox always
  /// resets to unchecked, an ordered marker increments.
  continuation: string;
}

function matchListPrefix(text: string): ListMatch | null {
  let m = CHECKBOX_RE.exec(text);
  if (m) {
    const [prefix, indent, bullet] = m;
    return { prefix, continuation: `${indent}${bullet} [ ] ` };
  }
  m = ORDERED_RE.exec(text);
  if (m) {
    const [prefix, indent, num, punct] = m;
    return { prefix, continuation: `${indent}${Number(num) + 1}${punct} ` };
  }
  m = BULLET_RE.exec(text);
  if (m) {
    const [prefix, indent, bullet] = m;
    return { prefix, continuation: `${indent}${bullet} ` };
  }
  return null;
}

function isListLine(text: string): boolean {
  return matchListPrefix(text) !== null;
}

// ---- Tab / Shift-Tab ----
// Mirrors `key_handlers.rs:6459-6486`: a multi-line selection block-indents
// every spanned line; a single list/checkbox line nests one level (both are
// really "indent this line," so both reuse CM6's own `indentMore`/
// `indentLess` rather than reimplementing line-prefix insertion by hand);
// anything else falls back to a plain two-space insert at the cursor, since
// `indentMore` would indent the *whole* line even when the TUI would have
// just inserted spaces at the cursor position.

export interface TabOptions {
  blockIndentSelect: boolean;
  listNesting: boolean;
}

export function editorTab(opts: TabOptions): Command {
  return (view) => {
    const { state } = view;
    const sel = state.selection.main;
    const multiLine = !sel.empty && state.doc.lineAt(sel.from).number !== state.doc.lineAt(sel.to).number;
    if (multiLine && opts.blockIndentSelect) return indentMore(view);
    const line = state.doc.lineAt(sel.from);
    if (!multiLine && opts.listNesting && isListLine(line.text)) return indentMore(view);
    view.dispatch(state.update(state.replaceSelection("  "), { scrollIntoView: true, userEvent: "input" }));
    return true;
  };
}

export function editorShiftTab(opts: TabOptions): Command {
  return (view) => {
    const { state } = view;
    const sel = state.selection.main;
    const multiLine = !sel.empty && state.doc.lineAt(sel.from).number !== state.doc.lineAt(sel.to).number;
    const line = state.doc.lineAt(sel.from);
    const shouldOutdent = multiLine ? opts.blockIndentSelect : opts.listNesting && isListLine(line.text);
    if (shouldOutdent) return indentLess(view);
    // Swallow the key rather than falling through to nothing (which would
    // move focus out of the editor, since basicSetup deliberately doesn't
    // bind Shift-Tab) when there's genuinely nothing to outdent.
    return true;
  };
}

// ---- Enter: list/checkbox auto-continue ----
// Mirrors `try_auto_continue_list` (`key_handlers.rs:6678`): continuing an
// empty item clears its marker instead of repeating it, so pressing Enter
// twice exits the list rather than piling up empty bullets forever.

export function editorEnterListContinue(): Command {
  return (view) => {
    const { state } = view;
    const sel = state.selection.main;
    if (!sel.empty) return false;
    const line = state.doc.lineAt(sel.head);
    const m = matchListPrefix(line.text);
    if (!m) return false;
    const isEmptyItem = line.text.trim() === m.prefix.trim();
    const changes: ChangeSpec = isEmptyItem
      ? { from: line.from, to: line.to, insert: "" }
      : { from: sel.head, insert: `\n${m.continuation}` };
    const newPos = isEmptyItem ? line.from : sel.head + 1 + m.continuation.length;
    view.dispatch(
      state.update({ changes, selection: EditorSelection.cursor(newPos), scrollIntoView: true, userEvent: "input" }),
    );
    return true;
  };
}

// ---- Backspace: remove an empty list prefix in one step ----
// Mirrors `try_backspace_exit_list` (`key_handlers.rs:6723`) — only engages
// right after a marker with nothing typed after it yet; otherwise falls
// through to CM6's ordinary single-character backspace.

export function editorBackspaceListExit(): Command {
  return (view) => {
    const { state } = view;
    const sel = state.selection.main;
    if (!sel.empty) return false;
    const line = state.doc.lineAt(sel.head);
    if (sel.head !== line.to) return false;
    const m = matchListPrefix(line.text);
    if (!m || line.text !== m.prefix) return false;
    view.dispatch(
      state.update({
        changes: { from: line.from, to: line.to, insert: "" },
        selection: EditorSelection.cursor(line.from),
        scrollIntoView: true,
        userEvent: "delete",
      }),
    );
    return true;
  };
}

// ---- Home: toggle first-non-whitespace <-> column 0 ----

export function editorSmartHome(): Command {
  return (view) => {
    const { state } = view;
    const sel = state.selection.main;
    const line = state.doc.lineAt(sel.head);
    const firstNonWs = line.text.search(/\S/);
    const target = firstNonWs === -1 ? line.from : line.from + firstNonWs;
    const newPos = sel.head === target ? line.from : target;
    view.dispatch({ selection: EditorSelection.cursor(newPos), scrollIntoView: true });
    return true;
  };
}

// Cursor-motion commands from `@codemirror/commands` (`cursorDocStart`,
// `cursorDocEnd`, …) already request `scrollIntoView: true` on their own
// dispatch — verified against the package's own source. Wrapping them to
// issue one more explicit scroll-into-view dispatch afterward is
// redundant on paper, but cheap, and a real, reproducible bug (a
// scroll-into-view request silently not visually applying after jumping
// to a scrolled-off-screen position, while the cursor move/edit itself
// still lands correctly) was hit live and not conclusively isolated to a
// single root cause — this makes the behavior self-healing regardless of
// why the first request didn't stick.
export function withScrollIntoView(cmd: Command): Command {
  return (view) => {
    const handled = cmd(view);
    if (handled) view.dispatch({ effects: EditorView.scrollIntoView(view.state.selection.main.head) });
    return handled;
  };
}

// ---- Ctrl+D: timestamp insert (only reachable when multi_cursor is off —
// `selectNextOccurrence` from @codemirror/search takes the key instead when
// multi_cursor is on, same precedence `key_handlers.rs:6329-6346` gives) ----

export function editorInsertTimestamp(withTime: boolean): Command {
  return (view) => {
    const now = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    const date = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
    const stamp = withTime ? `${date} ${pad(now.getHours())}:${pad(now.getMinutes())}` : date;
    view.dispatch(view.state.update(view.state.replaceSelection(stamp), { scrollIntoView: true, userEvent: "input" }));
    return true;
  };
}

// ---- Ctrl+B / Ctrl+Alt+I: wrap selection, or insert an empty pair with the
// cursor in the middle ----
// Mirrors `wrap_or_insert_pair` (`key_handlers.rs:7156`) — closeBrackets
// already does this for bracket/quote pairs, but `**`/`_` aren't brackets
// it knows about, so those two need their own command.

export function editorWrapSelection(mark: string): Command {
  return (view) => {
    const { state } = view;
    const changes = state.changeByRange((range) => {
      if (range.empty) {
        return {
          changes: [{ from: range.from, insert: mark + mark }],
          range: EditorSelection.cursor(range.from + mark.length),
        };
      }
      return {
        changes: [
          { from: range.from, insert: mark },
          { from: range.to, insert: mark },
        ],
        range: EditorSelection.range(range.from + mark.length, range.to + mark.length),
      };
    });
    view.dispatch(state.update(changes, { scrollIntoView: true, userEvent: "input" }));
    return true;
  };
}

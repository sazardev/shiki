// [[wikilink]] autocomplete for the CodeMirror editor — the desktop mirror
// of shiki-tui's wikilink menu (`App.wikilink_candidates`/`wikilink_results`
// in shiki-tui/src/app.rs and key_handlers.rs). Registered as markdown
// language data (`markdownLanguage.data.of({autocomplete: ...})`) rather
// than a second `autocompletion()` call — basicSetup already installs the
// one completion extension the editor needs; language data is the
// supported way to add more sources to it.
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { api, type NoteInfo } from "./api";

// Snapshot-once-per-notebook, same "expensive walk once, cheap re-score per
// keystroke" split as the TUI's own wikilink_candidates/wikilink_results —
// avoids an IPC round trip on every keystroke inside `[[`.
let cachedNotebook: string | null = null;
let cachedNotes: NoteInfo[] = [];

async function candidatesFor(notebook: string): Promise<NoteInfo[]> {
  if (cachedNotebook !== notebook) {
    cachedNotes = await api.listNotes(notebook);
    cachedNotebook = notebook;
  }
  return cachedNotes;
}

/// Call after any note create/rename/delete so a stale snapshot doesn't
/// linger — cheap to over-call, since the next `[[` just re-fetches.
export function invalidateWikilinkCache() {
  cachedNotebook = null;
}

export interface WikilinkSourceOptions {
  enabled: () => boolean;
  notebook: () => string | null;
  /// The note currently being edited — excluded from candidates, same as
  /// the TUI excluding the note being edited from `wikilink_candidates`
  /// ("linking to yourself isn't useful").
  excludePath: () => string | null;
}

export function wikilinkCompletionSource(opts: WikilinkSourceOptions) {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    if (!opts.enabled()) return null;
    const notebook = opts.notebook();
    if (!notebook) return null;
    const match = context.matchBefore(/\[\[[^\]]*/);
    if (!match) return null;

    const query = match.text.slice(2).toLowerCase();
    const exclude = opts.excludePath();
    const notes = await candidatesFor(notebook);
    const options = notes
      .filter((n) => n.path !== exclude)
      .filter((n) => n.title.toLowerCase().includes(query))
      .slice(0, 50)
      .map((n) => ({
        label: n.title,
        // Folder breadcrumb, mirrors the TUI's per-candidate breadcrumb row
        // (two notes with the same title in different folders are
        // otherwise indistinguishable).
        detail: n.path.includes("/") ? n.path.slice(0, n.path.lastIndexOf("/")) : undefined,
        type: "text",
        apply: `[[${n.title}]]`,
      }));

    // `auto_pair_brackets`'s closeBrackets already auto-inserted a `]]`
    // right after the cursor the moment `[[` was typed (verified live: a
    // naive `to: context.pos` left a stray `]]]]` behind after accepting a
    // candidate) — extend the replaced range past it so the completion
    // fully owns both the opening and the auto-paired closing marker.
    const after = context.state.sliceDoc(context.pos, context.pos + 2);
    const trailingClose = after.startsWith("]]") ? 2 : after.startsWith("]") ? 1 : 0;

    return { from: match.from, to: context.pos + trailingClose, options, filter: false };
  };
}

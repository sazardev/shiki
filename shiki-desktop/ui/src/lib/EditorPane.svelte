<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { EditorView, basicSetup } from "codemirror";
  import { markdown, markdownLanguage, pasteURLAsLink } from "@codemirror/lang-markdown";
  import { languages } from "@codemirror/language-data";
  import { syntaxHighlighting } from "@codemirror/language";
  import { markdownHighlightStyle } from "./markdownHighlight";
  import { keymap, type KeyBinding } from "@codemirror/view";
  import { Prec, type Extension } from "@codemirror/state";
  import { moveLineUp, moveLineDown, copyLineDown, cursorDocStart, cursorDocEnd } from "@codemirror/commands";
  import { search, searchKeymap, selectNextOccurrence } from "@codemirror/search";
  import {
    editorTab,
    editorShiftTab,
    editorEnterListContinue,
    editorBackspaceListExit,
    editorSmartHome,
    editorInsertTimestamp,
    editorWrapSelection,
    withScrollIntoView,
  } from "./editorCommands";
  import { wikilinkCompletionSource } from "./wikilinkCompletion";
  import { mergedCommands, slashCompletionSource } from "./slashMenu";
  import { setSpellIssues, spellField, type SpellRange } from "./spellDecorations";
  import { frontmatterField } from "./frontmatterDecoration";
  import { api, type MisspellInfo } from "./api";

  interface Props {
    value: string;
    title: string;
    onSave: (content: string) => void;
    onCancel: () => void;
    /// Scratchpad-only: Esc discards instead of saving (the TUI's
    /// scratchpad is explicit about this — "Esc discards the buffer;
    /// Ctrl+S stages it"), unlike a real note's Esc, which always saves.
    escDiscards?: boolean;
    /// The full `Config` (as returned by `api.getConfig()`) — only
    /// `config.editor`/`config.general` are read here. Omitted by the
    /// scratchpad's own `<EditorPane>` usage, which has no note-scoped
    /// config to apply, so every read below falls back to shiki-config's
    /// own `EditorConfig` defaults.
    config?: any;
    notebook?: string | null;
    path?: string | null;
  }

  let { value, title, onSave, onCancel, escDiscards = false, config = null, notebook = null, path = null }: Props =
    $props();
  let container: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;
  // The "aviso" (notice) VS Code itself uses: a filled dot in place of a
  // plain status marker once the buffer differs from what was loaded, so
  // there's a visible signal that closing/switching would lose work —
  // today `Cancel` silently discards with no warning at all otherwise.
  let dirty = $state(false);

  // ---- spellcheck (Ctrl+E) ----
  let spellIssues: MisspellInfo[] = $state([]);
  let spellPanelOpen = $state(false);
  let spellMessage = $state("");
  let openSuggestionsFor: number | null = $state(null); // index into spellIssues
  let suggestions: string[] = $state([]);

  async function runSpellCheck() {
    if (!view) return;
    const available = await api.spellAvailable();
    if (!available) {
      spellMessage = "hunspell not found on $PATH — spellcheck unavailable";
      spellPanelOpen = true;
      spellIssues = [];
      return;
    }
    const issues = await api.spellCheck(view.state.doc.toString());
    spellIssues = issues;
    spellMessage = issues.length === 0 ? "no misspellings found" : "";
    spellPanelOpen = true;
    openSuggestionsFor = null;
    view.dispatch({ effects: setSpellIssues.of(issues.map((i): SpellRange => ({ start: i.start, end: i.end }))) });
  }

  function closeSpellPanel() {
    spellPanelOpen = false;
    openSuggestionsFor = null;
    view?.dispatch({ effects: setSpellIssues.of([]) });
  }

  async function openSuggestions(index: number) {
    openSuggestionsFor = index;
    suggestions = await api.spellSuggestions(spellIssues[index].word);
  }

  async function applySuggestion(index: number, word: string) {
    const issue = spellIssues[index];
    view?.dispatch({ changes: { from: issue.start, to: issue.end, insert: word } });
    // Offsets of every other issue after this one are now stale (the
    // replacement rarely has the same length as the original word) —
    // simplest correct fix is just re-checking from scratch, same
    // "invalidate rather than patch" simplification `spellDecorations.ts`
    // already documents.
    await runSpellCheck();
  }

  // ---- image paste (Ctrl+V with an image on the clipboard) ----
  function imagePasteHandler(enabled: boolean, nb: string | null) {
    return (event: ClipboardEvent, editorView: EditorView): boolean => {
      if (!enabled || !nb) return false;
      const items = event.clipboardData?.items;
      if (!items) return false;
      for (let i = 0; i < items.length; i++) {
        const item = items[i];
        if (!item.type.startsWith("image/")) continue;
        const file = item.getAsFile();
        if (!file) continue;
        event.preventDefault();
        void (async () => {
          const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
          try {
            const result = await api.savePastedImage(nb, bytes);
            editorView.dispatch(editorView.state.replaceSelection(result.markdown_link));
          } catch (e) {
            console.error(e);
          }
        })();
        return true;
      }
      // No image item — fall through to CM6/browser's own text paste,
      // same "text always wins, image is the special case" precedence the
      // TUI's own Ctrl+V handling documents.
      return false;
    };
  }

  // Every behavior below mirrors one arm of shiki-tui's `handle_edit_key`
  // (`shiki-tui/src/key_handlers.rs`) — see that file's doc comments for the
  // exact TUI behavior each binding matches. Defaults mirror `EditorConfig`'s
  // own defaults (`shiki-config/src/config.rs`) so a missing `[editor]`
  // table (or the mock backend, which has no config at all) still behaves
  // like a fresh install rather than everything-off.
  function buildKeymap(cfg: any): KeyBinding[] {
    const ed = cfg?.editor ?? {};
    const bindings: KeyBinding[] = [];

    if (ed.find_replace ?? true) bindings.push(...searchKeymap);

    if (ed.move_line ?? true) {
      bindings.push({ key: "Alt-ArrowUp", run: moveLineUp }, { key: "Alt-ArrowDown", run: moveLineDown });
    }
    if (ed.duplicate_line ?? true) {
      bindings.push({ key: "Alt-d", run: copyLineDown });
    }

    // Ctrl+D is dual-purpose, same precedence the TUI gives multi_cursor
    // over insert_timestamp when both are configured on
    // (`key_handlers.rs:6329-6346`).
    if (ed.multi_cursor ?? false) {
      bindings.push({ key: "Mod-d", run: selectNextOccurrence });
    } else if (ed.insert_timestamp ?? true) {
      bindings.push({ key: "Mod-d", run: editorInsertTimestamp(ed.timestamp_with_time ?? false) });
    }

    if (ed.format_shortcuts ?? true) {
      bindings.push(
        { key: "Mod-b", run: editorWrapSelection("**") },
        { key: "Mod-Alt-i", run: editorWrapSelection("_") },
      );
    }

    bindings.push(
      { key: "Mod-Home", run: withScrollIntoView(cursorDocStart) },
      { key: "Mod-End", run: withScrollIntoView(cursorDocEnd) },
      { key: "Home", run: editorSmartHome() },
    );

    if (ed.auto_list_continue ?? true) {
      bindings.push(
        { key: "Enter", run: editorEnterListContinue() },
        { key: "Backspace", run: editorBackspaceListExit() },
      );
    }

    const tabOpts = { blockIndentSelect: ed.block_indent_select ?? true, listNesting: ed.auto_list_continue ?? true };
    bindings.push({ key: "Tab", run: editorTab(tabOpts) }, { key: "Shift-Tab", run: editorShiftTab(tabOpts) });

    if (ed.spellcheck ?? false) {
      bindings.push({
        key: "Mod-e",
        run: () => {
          // Toggles/closes if already open, same as the TUI's own Ctrl+E.
          if (spellPanelOpen) closeSpellPanel();
          else void runSpellCheck();
          return true;
        },
      });
    }

    bindings.push(
      {
        key: "Mod-s",
        run: (view) => {
          dirty = false;
          onSave(view.state.doc.toString());
          return true;
        },
      },
      {
        // Matches shiki-tui's `handle_edit_key`: Esc saves and exits for a
        // real note — it does not discard, there's no separate "discard"
        // shortcut in the TUI either. The scratchpad is the one exception
        // (`escDiscards`), which the TUI documents explicitly as discarding
        // on Esc instead.
        key: "Escape",
        run: (view) => {
          if (escDiscards) {
            onCancel();
          } else {
            dirty = false;
            onSave(view.state.doc.toString());
          }
          return true;
        },
      },
    );

    return bindings;
  }

  onMount(() => {
    if (!container) return;
    const gen = config?.general ?? {};
    const ed = config?.editor ?? {};
    const extra: Extension[] = [];
    if (gen.wikilink_autocomplete ?? true) {
      extra.push(
        markdownLanguage.data.of({
          autocomplete: wikilinkCompletionSource({
            enabled: () => true,
            notebook: () => notebook,
            excludePath: () => path,
          }),
        }),
      );
    }
    if (ed.paste_url_as_link ?? true) extra.push(pasteURLAsLink);
    if (ed.spellcheck ?? false) extra.push(spellField);
    extra.push(EditorView.domEventHandlers({ paste: imagePasteHandler(ed.paste_images ?? true, notebook) }));

    extra.push(
      markdownLanguage.data.of({
        autocomplete: slashCompletionSource({
          commands: () => mergedCommands(config?.snippets),
          vars: () => {
            const now = new Date();
            const pad = (n: number) => String(n).padStart(2, "0");
            return {
              title,
              date: `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`,
              time: `${pad(now.getHours())}:${pad(now.getMinutes())}`,
              notebook: notebook ?? "",
            };
          },
        }),
      }),
    );

    view = new EditorView({
      parent: container,
      doc: value,
      extensions: [
        basicSetup,
        markdown({ codeLanguages: languages }),
        // Not `{fallback: true}` — `basicSetup` already registers
        // `defaultHighlightStyle` internally as *its own* fallback
        // highlighter (verified in `codemirror`'s own source), so marking
        // ours as a fallback too meant both applied simultaneously: our
        // colors *plus* defaultHighlightStyle's stray `textDecoration:
        // underline` on headings — visible live as every frontmatter line
        // and heading rendering fully underlined. Registering ours as a
        // real (non-fallback, default) highlighter gives it priority: it
        // fully covers every tag `@lezer/markdown` emits, so
        // defaultHighlightStyle's fallback never gets a chance to apply.
        syntaxHighlighting(markdownHighlightStyle()),
        search({ top: true }),
        Prec.highest(keymap.of(buildKeymap(config))),
        editorTheme(isDarkTheme()),
        EditorView.updateListener.of((update) => {
          // Deferred, not synchronous: `updateListener` runs *inside* CM6's
          // own transaction-dispatch pipeline, before its scroll-into-view
          // measurement pass finishes. Setting Svelte state synchronously
          // here triggers a reactive DOM patch that races that pass —
          // verified live: with a synchronous `dirty = true` here, jumping
          // the cursor to a scrolled-off-screen position (Ctrl+End on a
          // long note, or opening the slash-menu near the bottom) silently
          // stopped scrolling into view at all, every time, while the
          // *content* edits themselves still landed correctly — a rendering
          // race, not a data bug. Pushing the state write to a separate
          // macrotask lets CM6 finish its own update cycle first.
          if (update.docChanged && !dirty) setTimeout(() => (dirty = true), 0);
        }),
        frontmatterField,
        ...extra,
      ],
    });
    view.focus();
  });

  onDestroy(() => {
    view?.destroy();
  });

  // `@codemirror/autocomplete`'s own baseTheme styles `.cm-tooltip-autocomplete`
  // conditionally on `.cm-editor.cm-light`/`.cm-editor.cm-dark` — a class
  // EditorView.theme() only adds when told which one via its second
  // argument. Without it (the state before this), the wikilink/slash-menu
  // dropdown fell through to *no* matching base rule at all and rendered as
  // an unstyled white-background browser default — verified live via
  // screenshot, not assumed. Every shiki theme is a hex-driven palette, so
  // rather than hardcoding `dark: true` (wrong for a genuinely light theme
  // like solarized-light), compute it from the resolved `--bg`'s luminance.
  function isDarkTheme(): boolean {
    if (typeof window === "undefined") return true;
    const bg = getComputedStyle(document.body).getPropertyValue("--bg").trim();
    const m = /^#?([0-9a-f]{6})$/i.exec(bg);
    if (!m) return true; // ANSI-name/"reset" themes: assume dark, this app's own default
    const hex = m[1];
    const r = parseInt(hex.slice(0, 2), 16);
    const g = parseInt(hex.slice(2, 4), 16);
    const b = parseInt(hex.slice(4, 6), 16);
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255 < 0.5;
  }

  // Modeled on VS Code's own editor feel — generous line-height and content
  // gutter so text has room to breathe, a 2px cursor (CM6's default is a
  // hairline that's easy to lose track of), a translucent (not flat-opaque)
  // selection/active-line via color-mix so multiple overlapping highlights
  // (selection + active line + search match) still read as layered instead
  // of fighting for the same flat color, and soft-rounded floating widgets
  // (autocomplete/search/spell panel) — VS Code's own tooltips/dropdowns are
  // never square, unlike the rest of this app's deliberately flat/square
  // TUI-mirroring chrome (`app.css`'s `border-radius: 0 !important`); these
  // three selectors are the one intentional, scoped exception to that rule,
  // beating the `!important` reset via higher selector specificity.
  const editorTheme = (dark: boolean) => EditorView.theme({
    "&": {
      backgroundColor: "var(--bg)",
      color: "var(--fg)",
      height: "100%",
      fontSize: "14px",
    },
    ".cm-content": {
      fontFamily: "ui-monospace, 'Cascadia Mono', Consolas, monospace",
      caretColor: "var(--cursor)",
      lineHeight: "1.7",
      padding: "1.1rem 0",
    },
    ".cm-line": { padding: "0 1.3rem" },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--cursor)",
      borderLeftWidth: "2px",
    },
    // A visible rail, not just a color shift — the gutter reads as a
    // distinct margin next to the text instead of blending into it.
    ".cm-gutters": {
      backgroundColor: "var(--bg)",
      color: "color-mix(in srgb, var(--muted) 88%, transparent)",
      borderRight: "1px solid color-mix(in srgb, var(--border) 70%, transparent)",
      paddingTop: "1.1rem",
    },
    ".cm-lineNumbers .cm-gutterElement": { padding: "0 1rem 0 0.4rem", minWidth: "2.6em" },
    ".cm-activeLineGutter": {
      backgroundColor: "transparent",
      color: "var(--fg)",
      fontWeight: "600",
    },
    ".cm-activeLine": {
      backgroundColor: "color-mix(in srgb, var(--fg) 5%, transparent)",
    },
    ".cm-selectionBackground, ::selection": {
      backgroundColor: "color-mix(in srgb, var(--accent) 28%, transparent) !important",
    },
    ".cm-focused": { outline: "none" },
    "&.cm-focused .cm-selectionBackground": {
      backgroundColor: "color-mix(in srgb, var(--accent) 32%, transparent) !important",
    },
    ".cm-matchingBracket, .cm-nonmatchingBracket": {
      backgroundColor: "color-mix(in srgb, var(--accent) 22%, transparent)",
      outline: "1px solid color-mix(in srgb, var(--accent) 60%, transparent)",
    },
    ".cm-scroller": { scrollbarWidth: "thin" },
    ".cm-panels, .cm-tooltip, .cm-tooltip-autocomplete": {
      backgroundColor: "var(--statusbar)",
      color: "var(--fg)",
      border: "1px solid var(--border)",
      borderRadius: "6px !important",
      boxShadow: "0 4px 16px color-mix(in srgb, black 35%, transparent)",
    },
    ".cm-panels": { borderRadius: "0 !important", boxShadow: "none", borderLeft: "none", borderRight: "none" },
    ".cm-panel input, .cm-panel button": {
      backgroundColor: "var(--bg)",
      color: "var(--fg)",
      border: "1px solid var(--border)",
      borderRadius: "4px !important",
      padding: "0.15rem 0.4rem",
    },
    ".cm-tooltip-autocomplete ul": {
      backgroundColor: "var(--statusbar)",
      color: "var(--fg)",
      fontFamily: "inherit",
    },
    ".cm-tooltip-autocomplete ul li": {
      color: "var(--fg)",
    },
    ".cm-tooltip-autocomplete ul li[aria-selected]": {
      backgroundColor: "color-mix(in srgb, var(--accent) 22%, transparent) !important",
      color: "var(--fg)",
    },
    ".cm-completionDetail": {
      color: "var(--muted)",
      fontStyle: "normal",
    },
    ".cm-misspell": {
      textDecoration: "underline wavy var(--error)",
      textDecorationSkipInk: "none",
    },
    // Overrides whatever heading/etc. styling the parser's Setext
    // mis-detection applied (see frontmatterDecoration.ts) — `span`, not a
    // specific tag class, since the point is to reset *every* inline mark
    // inside these lines uniformly, not chase which particular tag landed
    // on which token.
    ".cm-frontmatter-line": {
      opacity: "0.8",
    },
    ".cm-frontmatter-line span": {
      color: "var(--muted) !important",
      fontWeight: "normal !important",
      fontStyle: "normal !important",
      fontSize: "1em !important",
      textDecoration: "none !important",
    },
  }, { dark });
</script>

<div class="editor">
  <div class="editor-bar">
    <span class="editor-dot" class:dirty title={dirty ? "unsaved changes" : "saved"} aria-hidden="true"></span>
    <span class="editor-title">{title}</span>
    <span class="editor-hint">Esc save · Ctrl+F find · Ctrl+B/⌥I bold/italic · Tab indent</span>
    <button type="button" class="cancel-btn" onclick={onCancel}>Cancel</button>
    <button
      type="button"
      class="save-btn"
      onclick={() => {
        dirty = false;
        onSave(view?.state.doc.toString() ?? "");
      }}
    >
      Save
    </button>
  </div>
  {#if spellPanelOpen}
    <div class="spell-panel">
      {#if spellMessage}
        <span class="spell-msg">{spellMessage}</span>
      {:else}
        {#each spellIssues as issue, i (issue.start)}
          <div class="spell-item">
            <button type="button" class="spell-word" onclick={() => openSuggestions(i)}>{issue.word}</button>
            {#if openSuggestionsFor === i}
              <div class="spell-suggestions">
                {#each suggestions as s (s)}
                  <button type="button" class="spell-suggestion" onclick={() => applySuggestion(i, s)}>{s}</button>
                {:else}
                  <span class="spell-msg">no suggestions</span>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      {/if}
      <button type="button" class="spell-close" onclick={closeSpellPanel}>×</button>
    </div>
  {/if}
  <div class="editor-body" bind:this={container}></div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    height: 100%;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
  /* A VS Code editor tab, not a form toolbar: a small status dot instead of
     a boxed label, ghost buttons that only pick up weight on hover instead
     of two competing bordered boxes sitting there permanently, and a
     hairline (not a heavy 1px-solid-everywhere) bottom edge — the kind of
     restraint that reads as "IDE" rather than "web form". */
  .editor-bar {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.5rem 1rem;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    background: var(--statusbar);
  }
  /* Ring when saved, solid accent-filled dot when there are unsaved
     changes — same "notice" VS Code's own modified-file dot is, so closing
     out of an edit is never a silent surprise. */
  .editor-dot {
    width: 8px;
    height: 8px;
    border-radius: 50% !important;
    border: 1.5px solid var(--muted);
    background: transparent;
    flex-shrink: 0;
    transition: background-color 0.15s ease, border-color 0.15s ease;
  }
  .editor-dot.dirty {
    border-color: var(--accent);
    background: var(--accent);
  }
  .editor-title {
    font-weight: 600;
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .editor-hint {
    flex: 1;
    font-size: 0.72rem;
    color: color-mix(in srgb, var(--muted) 80%, transparent);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
  }
  .save-btn,
  .cancel-btn {
    background: transparent;
    border: none;
    padding: 0.25rem 0.75rem;
    cursor: pointer;
    font-weight: 600;
    transition: background-color 0.1s ease, color 0.1s ease;
  }
  .save-btn {
    color: var(--accent);
  }
  .save-btn:hover {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }
  .cancel-btn {
    color: var(--muted);
  }
  .cancel-btn:hover {
    background: color-mix(in srgb, var(--fg) 8%, transparent);
    color: var(--fg);
  }
  /* `min-height: 0` is load-bearing, not decoration — a flex child's
     default `min-height: auto` means "never shrink below your content's
     natural height," so without this, CM6's `.cm-scroller` (which needs a
     *bounded* parent to know when to start scrolling internally) just kept
     growing to fit the whole document instead of clipping to the
     available space — verified live: on a note long enough to need
     scrolling, Ctrl+End/typing at the bottom/mouse wheel all moved the
     cursor and content correctly, but the *viewport* never followed,
     because there was no actual scroll container, just silent overflow
     clipping one level up. Never surfaced before this because every note
     tested until now was short enough to fit in one screen. */
  .editor-body {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }
  .spell-panel {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 0.4rem;
    padding: 0.4rem 1rem;
    border-bottom: 1px solid var(--border);
    background: var(--statusbar);
    position: relative;
  }
  .spell-msg {
    font-size: 0.78rem;
    color: var(--muted);
  }
  .spell-item {
    position: relative;
  }
  .spell-word {
    background: color-mix(in srgb, var(--error) 14%, transparent);
    border: none;
    color: var(--error);
    border-radius: 4px !important;
    padding: 0.15rem 0.5rem;
    font-size: 0.78rem;
    cursor: pointer;
  }
  .spell-suggestions {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 10;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    margin-top: 0.3rem;
    padding: 0.3rem;
    background: var(--statusbar);
    border: 1px solid var(--border);
    border-radius: 6px !important;
    box-shadow: 0 4px 16px color-mix(in srgb, black 35%, transparent);
    min-width: 8rem;
  }
  .spell-suggestion {
    background: transparent;
    border: none;
    color: var(--fg);
    text-align: left;
    padding: 0.25rem 0.5rem;
    font-size: 0.8rem;
    cursor: pointer;
    border-radius: 4px !important;
  }
  .spell-suggestion:hover {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
  }
  .spell-close {
    margin-left: auto;
    background: transparent;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 1rem;
    line-height: 1;
  }
</style>

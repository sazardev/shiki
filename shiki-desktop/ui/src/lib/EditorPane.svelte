<script lang="ts">
  import { onMount } from "svelte";
  import { EditorView, basicSetup } from "codemirror";
  import { markdown } from "@codemirror/lang-markdown";
  import { languages } from "@codemirror/language-data";
  import { syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
  import { keymap } from "@codemirror/view";

  interface Props {
    value: string;
    title: string;
    onSave: (content: string) => void;
    onCancel: () => void;
  }

  let { value, title, onSave, onCancel }: Props = $props();
  let container: HTMLDivElement | undefined = $state();
  let view: EditorView | undefined;

  onMount(() => {
    if (!container) return;
    view = new EditorView({
      parent: container,
      doc: value,
      extensions: [
        basicSetup,
        markdown({ codeLanguages: languages }),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        keymap.of([
          {
            key: "Mod-s",
            run: () => {
              if (view) onSave(view.state.doc.toString());
              return true;
            },
          },
          {
            key: "Escape",
            run: () => {
              onCancel();
              return true;
            },
          },
        ]),
        editorTheme,
      ],
    });
  });

  const editorTheme = EditorView.theme({
    "&": {
      backgroundColor: "var(--bg)",
      color: "var(--fg)",
      height: "100%",
      fontSize: "14px",
    },
    ".cm-content": {
      fontFamily: "ui-monospace, 'Cascadia Mono', Consolas, monospace",
      caretColor: "var(--cursor)",
    },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--cursor)" },
    ".cm-gutters": {
      backgroundColor: "var(--bg)",
      color: "var(--muted)",
      borderRight: "1px solid var(--border)",
    },
    ".cm-activeLine": { backgroundColor: "var(--highlight)" },
    ".cm-activeLineGutter": { backgroundColor: "var(--selection)" },
    ".cm-selectionBackground, ::selection": { backgroundColor: "var(--selection)" },
    ".cm-focused": { outline: "none" },
    "&.cm-focused .cm-selectionBackground": { backgroundColor: "var(--selection)" },
  });
</script>

<div class="editor">
  <div class="editor-bar">
    <span class="editor-title">{title}</span>
    <span class="editor-hint">Ctrl+S save · Esc cancel</span>
    <button type="button" class="save-btn" onclick={() => onSave(view?.state.doc.toString() ?? "")}>
      Save
    </button>
    <button type="button" class="cancel-btn" onclick={onCancel}>Cancel</button>
  </div>
  <div class="editor-body" bind:this={container}></div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    height: 100%;
    flex: 1;
    min-width: 0;
  }
  .editor-bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.4rem 1rem;
    border-bottom: 1px solid var(--border);
    background: var(--statusbar);
  }
  .editor-title {
    font-weight: 600;
    color: var(--accent);
  }
  .editor-hint {
    flex: 1;
    font-size: 0.75rem;
    color: var(--muted);
  }
  .save-btn,
  .cancel-btn {
    background: var(--accent);
    color: var(--bg);
    border: none;
    border-radius: 4px;
    padding: 0.2rem 0.9rem;
    font-weight: 600;
    cursor: pointer;
  }
  .cancel-btn {
    background: var(--inactive);
    color: var(--fg);
  }
  .editor-body {
    flex: 1;
    overflow: hidden;
  }
</style>
<script lang="ts">
  // Outline modal (PREVIEW-scope `o`) — every heading in the selected note,
  // jump straight to one. Headings are extracted client-side from the raw
  // note body (stripping the frontmatter block first) since there's no
  // dedicated backend command for this; `onJump` receives the heading's
  // occurrence index, which the caller resolves against the *rendered*
  // `<h1>..<h6>` elements in PREVIEW — good enough as long as the note
  // doesn't have a `#`-looking line inside a fenced code block confusing
  // the count, the same "estimate, not exact" spirit already used for the
  // footer's word count.
  import { api } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    notebook: string | null;
    path: string | null;
    onJump: (index: number) => void;
  }
  let { notebook, path, onJump }: Props = $props();

  interface Heading {
    level: number;
    text: string;
  }

  let headings: Heading[] = $state([]);
  let selected = $state(0);
  let loadError = $state("");
  let listEl: HTMLDivElement | undefined = $state();

  function stripFrontmatter(raw: string): string {
    if (!raw.startsWith("---\n")) return raw;
    const end = raw.indexOf("\n---\n", 4);
    return end === -1 ? raw : raw.slice(end + 5);
  }

  function extractHeadings(body: string): Heading[] {
    const out: Heading[] = [];
    let inFence = false;
    for (const line of body.split("\n")) {
      if (/^```/.test(line)) inFence = !inFence;
      if (inFence) continue;
      const m = /^(#{1,6})\s+(.+)$/.exec(line);
      if (m) out.push({ level: m[1].length, text: m[2].trim() });
    }
    return out;
  }

  async function load() {
    if (!notebook || !path) {
      headings = [];
      return;
    }
    try {
      const note = await api.readNote(notebook, path);
      headings = extractHeadings(stripFrontmatter(note.content));
      selected = 0;
      loadError = "";
    } catch (e) {
      headings = [];
      loadError = String(e);
    }
  }

  $effect(() => {
    if (input.overlay === "outline") void load();
  });

  // See ThemePicker.svelte's identical fix: arrow-key moves alone don't
  // scroll the list, so the selection can silently move off-screen.
  $effect(() => {
    void selected;
    listEl?.querySelector<HTMLElement>(".ol-row.selected")?.scrollIntoView({ block: "nearest" });
  });

  function close() {
    input.overlay = null;
  }

  function accept(i: number) {
    if (!headings[i]) return;
    close();
    onJump(i);
  }

  function onKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        close();
        return;
      case "Enter":
        e.preventDefault();
        accept(selected);
        return;
      case "ArrowDown":
        e.preventDefault();
        selected = Math.min(selected + 1, headings.length - 1);
        return;
      case "ArrowUp":
        e.preventDefault();
        selected = Math.max(selected - 1, 0);
        return;
    }
  }
</script>

<svelte:window onkeydown={input.overlay === "outline" ? onKeydown : undefined} />

{#if input.overlay === "outline"}
  <div class="ol-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="ol-panel">
      <h3>Outline</h3>
      {#if loadError}
        <div class="ol-empty">{loadError}</div>
      {:else if headings.length === 0}
        <div class="ol-empty">no headings in this note</div>
      {:else}
        <div class="ol-list" bind:this={listEl}>
          {#each headings as h, i (i)}
            <button
              type="button"
              class="ol-row"
              class:selected={i === selected}
              style="padding-left: {0.6 + (h.level - 1) * 0.9}rem"
              onclick={() => accept(i)}
              onmouseenter={() => (selected = i)}
            >
              {h.text}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .ol-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .ol-panel {
    width: 420px;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0.8rem 0;
  }
  .ol-panel h3 {
    margin: 0 0.9rem 0.5rem;
    color: var(--accent);
  }
  .ol-list {
    overflow-y: auto;
  }
  .ol-row {
    display: block;
    width: 100%;
    padding: 0.35rem 0.6rem;
    background: none;
    border: none;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .ol-row.selected {
    background: var(--selection);
  }
  .ol-empty {
    padding: 0 0.9rem 0.5rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

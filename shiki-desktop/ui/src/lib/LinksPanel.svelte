<script lang="ts">
  // Links panel (leader+B / PREVIEW-scope L) — outgoing wikilinks,
  // backlinks, and unlinked mentions for the selected note, three sections
  // same as the TUI's links modal. Flattened into one navigable list here
  // rather than the TUI's header-interspersed rows, same simplification
  // TasksPanel already made.
  import { api } from "./api";
  import type { LinkNote } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    notebook: string | null;
    path: string | null;
    onJump: (path: string) => void;
  }
  let { notebook, path, onJump }: Props = $props();

  type Row = { section: "Outgoing" | "Backlinks" | "Mentions"; note: LinkNote };

  let rows: Row[] = $state([]);
  let selected = $state(0);
  let loading = $state(false);
  let listEl: HTMLDivElement | undefined = $state();

  async function load() {
    if (!notebook || !path) {
      rows = [];
      return;
    }
    loading = true;
    try {
      const info = await api.getLinks(notebook, path);
      rows = [
        ...info.outgoing.map((note) => ({ section: "Outgoing" as const, note })),
        ...info.backlinks.map((note) => ({ section: "Backlinks" as const, note })),
        ...info.mentions.map((note) => ({ section: "Mentions" as const, note })),
      ];
    } catch {
      rows = [];
    }
    loading = false;
    selected = 0;
  }

  $effect(() => {
    if (input.overlay === "links") void load();
  });

  // See ThemePicker.svelte's identical fix: arrow-key moves alone don't
  // scroll the list, so the selection can silently move off-screen.
  $effect(() => {
    void selected;
    listEl?.querySelector<HTMLElement>(".lk-row.selected")?.scrollIntoView({ block: "nearest" });
  });

  function close() {
    input.overlay = null;
  }

  function accept(i: number) {
    const row = rows[i];
    if (!row) return;
    close();
    onJump(row.note.path);
  }

  function onKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "Escape":
      case "q":
        e.preventDefault();
        close();
        return;
      case "Enter":
      case "l":
        e.preventDefault();
        accept(selected);
        return;
      case "j":
      case "ArrowDown":
        e.preventDefault();
        selected = Math.min(selected + 1, rows.length - 1);
        return;
      case "k":
      case "ArrowUp":
        e.preventDefault();
        selected = Math.max(selected - 1, 0);
        return;
    }
  }
</script>

<svelte:window onkeydown={input.overlay === "links" ? onKeydown : undefined} />

{#if input.overlay === "links"}
  <div class="lk-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="lk-panel">
      <h3>Links</h3>
      <div class="lk-list" bind:this={listEl}>
        {#if loading}
          <div class="lk-empty">loading…</div>
        {:else if rows.length === 0}
          <div class="lk-empty">no outgoing links, backlinks, or mentions</div>
        {:else}
          {#each rows as row, i (row.section + row.note.path)}
            <button
              type="button"
              class="lk-row"
              class:selected={i === selected}
              class:mention={row.section === "Mentions"}
              onclick={() => accept(i)}
              onmouseenter={() => (selected = i)}
            >
              <span class="lk-section">{row.section}</span>
              <span class="lk-title">{row.note.title}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .lk-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .lk-panel {
    width: 420px;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0.8rem 0;
  }
  .lk-panel h3 {
    margin: 0 0.9rem 0.5rem;
    color: var(--accent);
  }
  .lk-list {
    overflow-y: auto;
  }
  .lk-row {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    width: 100%;
    padding: 0.35rem 0.9rem;
    background: none;
    border: none;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .lk-row.selected {
    background: var(--selection);
  }
  .lk-row.mention .lk-title {
    color: var(--muted);
  }
  .lk-section {
    flex: 0 0 80px;
    font-size: 0.72rem;
    color: var(--muted);
    text-transform: uppercase;
  }
  .lk-empty {
    padding: 0.4rem 0.9rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

<script lang="ts">
  // Tree view (notes-scope T) — every note in the current notebook at any
  // depth, read-only jump list. The desktop NOTES panel only ever lists
  // one folder's root (there's no folder-drilling UI here yet), so this is
  // the one place a nested note becomes reachable without already knowing
  // its exact path.
  import { api } from "./api";
  import type { TreeNote } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    notebook: string | null;
    onJump: (path: string) => void;
  }
  let { notebook, onJump }: Props = $props();

  let notes: TreeNote[] = $state([]);
  let selected = $state(0);
  let loading = $state(false);

  async function load() {
    if (!notebook) {
      notes = [];
      return;
    }
    loading = true;
    try {
      notes = await api.notebookTree(notebook);
    } catch {
      notes = [];
    }
    loading = false;
    selected = 0;
  }

  $effect(() => {
    if (input.overlay === "tree") void load();
  });

  function close() {
    input.overlay = null;
  }

  function accept(i: number) {
    const n = notes[i];
    if (!n) return;
    close();
    onJump(n.path);
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
        selected = Math.min(selected + 1, notes.length - 1);
        return;
      case "k":
      case "ArrowUp":
        e.preventDefault();
        selected = Math.max(selected - 1, 0);
        return;
    }
  }
</script>

<svelte:window onkeydown={input.overlay === "tree" ? onKeydown : undefined} />

{#if input.overlay === "tree"}
  <div class="tv-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="tv-panel">
      <h3>Notebook tree — {notebook}</h3>
      <div class="tv-list">
        {#if loading}
          <div class="tv-empty">loading…</div>
        {:else if notes.length === 0}
          <div class="tv-empty">no notes</div>
        {:else}
          {#each notes as n, i (n.path)}
            <button
              type="button"
              class="tv-row"
              class:selected={i === selected}
              onclick={() => accept(i)}
              onmouseenter={() => (selected = i)}
            >
              {#if n.folder}<span class="tv-folder">{n.folder}/</span>{/if}<span>{n.title}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .tv-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .tv-panel {
    width: 460px;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0.8rem 0;
  }
  .tv-panel h3 {
    margin: 0 0.9rem 0.5rem;
    color: var(--accent);
  }
  .tv-list {
    overflow-y: auto;
  }
  .tv-row {
    display: block;
    width: 100%;
    padding: 0.35rem 0.9rem;
    background: none;
    border: none;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .tv-row.selected {
    background: var(--selection);
  }
  .tv-folder {
    color: var(--muted);
  }
  .tv-empty {
    padding: 0.4rem 0.9rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

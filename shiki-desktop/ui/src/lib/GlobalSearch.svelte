<script lang="ts">
  // Global search overlay (leader+g) — fuzzy search across every notebook,
  // same shape as shiki-tui's global search modal: type to search, arrows
  // to move, Enter jumps straight to the note. Backed by the existing
  // `search_notes` IPC command (unused by the UI until now).
  import { api } from "./api";
  import type { SearchResult } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    onJump: (notebook: string, path: string) => void;
  }
  let { onJump }: Props = $props();

  let query = $state("");
  let results: SearchResult[] = $state([]);
  let selected = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();
  let seq = 0;

  async function runSearch(q: string) {
    const mySeq = ++seq;
    if (!q.trim()) {
      results = [];
      return;
    }
    try {
      const hits = await api.searchNotes(q);
      if (mySeq === seq) {
        results = hits;
        selected = 0;
      }
    } catch {
      if (mySeq === seq) results = [];
    }
  }

  $effect(() => {
    void runSearch(query);
  });

  // See ThemePicker.svelte's identical fix: arrow-key moves alone don't
  // scroll the list, so the selection can silently move off-screen.
  $effect(() => {
    void selected;
    listEl?.querySelector<HTMLElement>(".gs-row.selected")?.scrollIntoView({ block: "nearest" });
  });

  $effect(() => {
    if (input.overlay === "globalSearch") {
      query = "";
      results = [];
      selected = 0;
      // A synchronous call, not requestAnimationFrame — rAF callbacks are
      // throttled (sometimes indefinitely) in a backgrounded/inactive tab,
      // which silently broke autofocus there; a direct call right after
      // Svelte's own DOM patch has no such dependency on paint timing.
      inputEl?.focus();
    }
  });

  function close() {
    input.overlay = null;
  }

  function accept(i: number) {
    const hit = results[i];
    if (!hit) return;
    close();
    onJump(hit.notebook, hit.path);
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
        selected = Math.min(selected + 1, results.length - 1);
        return;
      case "ArrowUp":
        e.preventDefault();
        selected = Math.max(selected - 1, 0);
        return;
    }
  }
</script>

{#if input.overlay === "globalSearch"}
  <div class="gs-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="gs-panel">
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={onKeydown}
        placeholder="Search every notebook… (Enter to jump, Esc to close)"
        class="gs-input"
      />
      <div class="gs-list" bind:this={listEl}>
        {#each results as hit, i (hit.notebook + hit.path)}
          <button
            type="button"
            class="gs-row"
            class:selected={i === selected}
            onclick={() => accept(i)}
            onmouseenter={() => (selected = i)}
          >
            <span class="gs-nb">{hit.notebook}</span>
            <span class="gs-title">{hit.title}</span>
          </button>
        {:else}
          <div class="gs-empty">{query.trim() ? "no matches" : "type to search"}</div>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .gs-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    padding: 10% 15%;
  }
  .gs-panel {
    width: 100%;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .gs-input {
    padding: 0.7rem 0.9rem;
    background: var(--bg);
    border: none;
    border-bottom: 1px solid var(--accent);
    color: var(--fg);
    font-size: 0.95rem;
  }
  .gs-list {
    overflow-y: auto;
    padding: 0.3rem;
  }
  .gs-row {
    display: flex;
    align-items: baseline;
    gap: 0.8rem;
    width: 100%;
    padding: 0.4rem 0.6rem;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .gs-row.selected {
    background: var(--selection);
  }
  .gs-nb {
    flex: 0 0 120px;
    color: var(--muted);
    font-size: 0.78rem;
  }
  .gs-title {
    flex: 1;
  }
  .gs-empty {
    padding: 1rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

<script lang="ts">
  // Query modal (leader+q) — a Dataview-style `where ... sort ...` DSL over
  // frontmatter, live across every notebook (`shiki_core::query`). Parse
  // errors surface inline rather than silently showing stale/empty
  // results, since a query typo looking identical to "no matches" would be
  // confusing.
  import { api } from "./api";
  import type { QueryRowInfo } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    onJump: (notebook: string, path: string) => void;
  }
  let { onJump }: Props = $props();

  let query = $state("");
  let rows: QueryRowInfo[] = $state([]);
  let selected = $state(0);
  let error = $state("");
  let inputEl: HTMLInputElement | undefined = $state();
  let seq = 0;

  async function runQuery(q: string) {
    const mySeq = ++seq;
    try {
      const result = await api.runNoteQuery(q);
      if (mySeq === seq) {
        rows = result;
        error = "";
      }
    } catch (e) {
      if (mySeq === seq) {
        rows = [];
        error = String(e);
      }
    }
  }

  $effect(() => {
    void runQuery(query);
  });

  $effect(() => {
    void query;
    selected = 0;
  });

  $effect(() => {
    if (input.overlay === "query") {
      query = "";
      inputEl?.focus();
    }
  });

  function close() {
    input.overlay = null;
  }

  function accept(i: number) {
    const row = rows[i];
    if (!row) return;
    close();
    onJump(row.notebook, row.path);
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
        selected = Math.min(selected + 1, rows.length - 1);
        return;
      case "ArrowUp":
        e.preventDefault();
        selected = Math.max(selected - 1, 0);
        return;
    }
  }
</script>

<svelte:window onkeydown={input.overlay === "query" ? onKeydown : undefined} />

{#if input.overlay === "query"}
  <div class="qr-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="qr-panel">
      <input
        bind:this={inputEl}
        bind:value={query}
        placeholder="where status = pending sort due asc"
        class="qr-input"
      />
      {#if error}
        <div class="qr-error">{error}</div>
      {/if}
      <div class="qr-list">
        {#each rows as row, i (row.notebook + row.path)}
          <button
            type="button"
            class="qr-row"
            class:selected={i === selected}
            onclick={() => accept(i)}
            onmouseenter={() => (selected = i)}
          >
            <span class="qr-loc">{row.location}</span>
          </button>
        {:else}
          {#if !error}<div class="qr-empty">no matches</div>{/if}
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .qr-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    padding: 10% 15%;
  }
  .qr-panel {
    width: 100%;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .qr-input {
    padding: 0.7rem 0.9rem;
    background: var(--bg);
    border: none;
    border-bottom: 1px solid var(--accent);
    color: var(--fg);
    font-size: 0.95rem;
  }
  .qr-error {
    padding: 0.4rem 0.9rem;
    color: var(--error);
    font-size: 0.82rem;
  }
  .qr-list {
    overflow-y: auto;
    padding: 0.3rem;
  }
  .qr-row {
    display: block;
    width: 100%;
    padding: 0.35rem 0.6rem;
    background: none;
    border: none;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .qr-row.selected {
    background: var(--selection);
  }
  .qr-empty {
    padding: 1rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

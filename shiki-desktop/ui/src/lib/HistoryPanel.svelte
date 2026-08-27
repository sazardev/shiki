<script lang="ts">
  // Note history modal (PREVIEW-scope H) — every commit that touched the
  // selected note, newest first; `r` stages a revert behind a confirm step
  // (a revert overwrites the working tree, same "ask before overwriting"
  // caution the TUI's own revert confirm dialog has).
  import { api } from "./api";
  import type { RevisionInfo } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    notebook: string | null;
    path: string | null;
    onReverted: () => void;
  }
  let { notebook, path, onReverted }: Props = $props();

  let revisions: RevisionInfo[] = $state([]);
  let selected = $state(0);
  let loading = $state(false);
  let confirming: RevisionInfo | null = $state(null);
  let listEl: HTMLDivElement | undefined = $state();

  async function load() {
    if (!notebook || !path) {
      revisions = [];
      return;
    }
    loading = true;
    try {
      revisions = await api.noteHistory(notebook, path);
    } catch {
      revisions = [];
    }
    loading = false;
    selected = 0;
    confirming = null;
  }

  $effect(() => {
    if (input.overlay === "history") void load();
  });

  // See ThemePicker.svelte's identical fix: arrow-key moves alone don't
  // scroll the list, so the selection can silently move off-screen.
  $effect(() => {
    void selected;
    listEl?.querySelector<HTMLElement>(".hs-row.selected")?.scrollIntoView({ block: "nearest" });
  });

  function close() {
    input.overlay = null;
  }

  async function doRevert() {
    if (!notebook || !path || !confirming) return;
    try {
      await api.revertNote(notebook, path, confirming.commit_id);
      close();
      onReverted();
    } catch {
      confirming = null;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (confirming) {
      if (e.key === "y" || e.key === "Enter") {
        e.preventDefault();
        void doRevert();
      } else if (e.key === "n" || e.key === "Escape") {
        e.preventDefault();
        confirming = null;
      }
      return;
    }
    switch (e.key) {
      case "Escape":
      case "q":
        e.preventDefault();
        close();
        return;
      case "r":
        e.preventDefault();
        if (revisions[selected]) confirming = revisions[selected];
        return;
      case "j":
      case "ArrowDown":
        e.preventDefault();
        selected = Math.min(selected + 1, revisions.length - 1);
        return;
      case "k":
      case "ArrowUp":
        e.preventDefault();
        selected = Math.max(selected - 1, 0);
        return;
    }
  }
</script>

<svelte:window onkeydown={input.overlay === "history" ? onKeydown : undefined} />

{#if input.overlay === "history"}
  <div class="hs-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="hs-panel">
      <h3>History</h3>
      {#if confirming}
        <div class="hs-confirm">
          Revert working copy to <b>{confirming.commit_id.slice(0, 7)}</b>? (y/n)
        </div>
      {:else if loading}
        <div class="hs-empty">loading…</div>
      {:else if revisions.length === 0}
        <div class="hs-empty">no history for this note</div>
      {:else}
        <div class="hs-hint">r to revert working copy · j/k move · Esc close</div>
        <div class="hs-list" bind:this={listEl}>
          {#each revisions as rev, i (rev.commit_id)}
            <button
              type="button"
              class="hs-row"
              class:selected={i === selected}
              onclick={() => (selected = i)}
              ondblclick={() => (confirming = rev)}
              onmouseenter={() => (selected = i)}
            >
              <span class="hs-id">{rev.commit_id.slice(0, 7)}</span>
              <span class="hs-date">{rev.date}</span>
              <span class="hs-msg">{rev.message}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .hs-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .hs-panel {
    width: 480px;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0.8rem 0;
  }
  .hs-panel h3 {
    margin: 0 0.9rem 0.3rem;
    color: var(--accent);
  }
  .hs-hint {
    margin: 0 0.9rem 0.4rem;
    color: var(--muted);
    font-size: 0.75rem;
  }
  .hs-list {
    overflow-y: auto;
  }
  .hs-row {
    display: flex;
    align-items: baseline;
    gap: 0.7rem;
    width: 100%;
    padding: 0.35rem 0.9rem;
    background: none;
    border: none;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .hs-row.selected {
    background: var(--selection);
  }
  .hs-id {
    flex: 0 0 70px;
    color: var(--accent);
    font-size: 0.85rem;
  }
  .hs-date {
    flex: 0 0 130px;
    color: var(--muted);
    font-size: 0.78rem;
  }
  .hs-msg {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hs-confirm {
    padding: 0.4rem 0.9rem 0.8rem;
    color: var(--warning);
  }
  .hs-empty {
    padding: 0.4rem 0.9rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

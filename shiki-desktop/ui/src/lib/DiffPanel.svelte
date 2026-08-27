<script lang="ts">
  // Pending-changes diff (PREVIEW-scope d) — the selected note's working
  // tree vs HEAD. Read-only; there's nothing to select/act on, just Esc to
  // close, same as a plain viewer.
  import type { DiffLineInfo } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    lines: DiffLineInfo[];
  }
  let { lines }: Props = $props();

  function close() {
    input.overlay = null;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" || e.key === "q") {
      e.preventDefault();
      close();
    }
  }

  function kind(origin: string): string {
    if (origin === "+") return "add";
    if (origin === "-") return "del";
    return "ctx";
  }
</script>

<svelte:window onkeydown={input.overlay === "diff" ? onKeydown : undefined} />

{#if input.overlay === "diff"}
  <div class="df-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="df-panel">
      <h3>Pending changes</h3>
      <div class="df-list">
        {#each lines as line, i (i)}
          <div class="df-row {kind(line.origin)}">{line.origin}{line.content}</div>
        {:else}
          <div class="df-empty">no pending changes</div>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .df-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .df-panel {
    width: 640px;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0.8rem 0;
  }
  .df-panel h3 {
    margin: 0 0.9rem 0.5rem;
    color: var(--accent);
  }
  .df-list {
    overflow-y: auto;
    font-family: inherit;
    white-space: pre-wrap;
    padding: 0 0.4rem;
  }
  .df-row {
    padding: 0.05rem 0.5rem;
  }
  .df-row.add {
    color: var(--success);
    background: color-mix(in srgb, var(--success) 12%, transparent);
  }
  .df-row.del {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 12%, transparent);
  }
  .df-row.ctx {
    color: var(--muted);
  }
  .df-empty {
    padding: 0.4rem 0.9rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

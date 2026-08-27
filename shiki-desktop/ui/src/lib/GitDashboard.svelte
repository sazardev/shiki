<script lang="ts">
  // Git dashboard (NOTEBOOKS-scope G) — every notebook's sync state in
  // plain terms, read-only, same idea as the TUI's dashboard. Built by
  // looping the existing per-notebook `git_status` command; no dedicated
  // backend endpoint needed.
  import { api } from "./api";
  import type { NotebookInfo, GitStatus } from "./api";
  import { input } from "./input.svelte";
  import { gitStatusKind, gitStatusSuffix } from "./statusBar";

  interface Props {
    notebooks: NotebookInfo[];
  }
  let { notebooks }: Props = $props();

  interface Row {
    name: string;
    status: GitStatus | null;
    error: string | null;
  }

  let rows: Row[] = $state([]);
  let loading = $state(false);

  async function load() {
    loading = true;
    const out: Row[] = [];
    for (const nb of notebooks) {
      try {
        out.push({ name: nb.name, status: await api.gitStatus(nb.name), error: null });
      } catch (e) {
        out.push({ name: nb.name, status: null, error: String(e) });
      }
    }
    rows = out;
    loading = false;
  }

  $effect(() => {
    if (input.overlay === "gitDash") void load();
  });

  function close() {
    input.overlay = null;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" || e.key === "q") {
      e.preventDefault();
      close();
    }
  }

  function summary(status: GitStatus): string {
    const kind = gitStatusKind(status);
    if (kind === "clean") return "clean";
    const suffix = gitStatusSuffix(status).trim();
    return suffix || "clean";
  }
</script>

<svelte:window onkeydown={input.overlay === "gitDash" ? onKeydown : undefined} />

{#if input.overlay === "gitDash"}
  <div class="gd-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="gd-panel">
      <h3>Git dashboard</h3>
      <div class="gd-list">
        {#if loading}
          <div class="gd-empty">loading…</div>
        {:else}
          {#each rows as row (row.name)}
            <div class="gd-row">
              <span class="gd-name">{row.name}</span>
              {#if row.error}
                <span class="gd-err">{row.error}</span>
              {:else if row.status}
                <span class="gd-branch">{row.status.branch ?? "?"}</span>
                <span class="gd-status">{summary(row.status)}</span>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .gd-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .gd-panel {
    width: 460px;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0.8rem 0;
  }
  .gd-panel h3 {
    margin: 0 0.9rem 0.5rem;
    color: var(--accent);
  }
  .gd-list {
    overflow-y: auto;
  }
  .gd-row {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    padding: 0.35rem 0.9rem;
  }
  .gd-name {
    flex: 0 0 120px;
    color: var(--fg);
  }
  .gd-branch {
    flex: 0 0 90px;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .gd-status {
    color: var(--warning);
  }
  .gd-err {
    color: var(--error);
    font-size: 0.85rem;
  }
  .gd-empty {
    padding: 0.4rem 0.9rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

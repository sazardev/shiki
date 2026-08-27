<script lang="ts">
  // Logs modal (leader+l) — scrollback of every status message, persisted
  // to the same `Config::default_log_path()` file the TUI writes
  // (`~/.config/shiki/shiki.log`), so a message that already scrolled past
  // is still readable here instead of only living in the footer for
  // `STATUS_MESSAGE_TIMEOUT`. `x` clears both the in-memory list and the
  // file, same confirm-then-clear the TUI has (kept as a plain second
  // keypress here rather than a separate confirm dialog, since it's a
  // reversible-enough action — the log is a scrollback, not user data).
  import { api } from "./api";
  import type { LogEntryInfo } from "./api";
  import { input } from "./input.svelte";

  let entries: LogEntryInfo[] = $state([]);
  let loading = $state(false);

  async function load() {
    loading = true;
    try {
      entries = (await api.readLogs()).reverse(); // newest first
    } catch {
      entries = [];
    }
    loading = false;
  }

  $effect(() => {
    if (input.overlay === "logs") void load();
  });

  function close() {
    input.overlay = null;
  }

  async function clear() {
    try {
      await api.clearLogs();
      await load();
    } catch {
      // leave the list as-is
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" || e.key === "q") {
      e.preventDefault();
      close();
    } else if (e.key === "x") {
      e.preventDefault();
      void clear();
    }
  }
</script>

<svelte:window onkeydown={input.overlay === "logs" ? onKeydown : undefined} />

{#if input.overlay === "logs"}
  <div class="lg-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="lg-panel">
      <h3>Logs <span class="lg-hint">(x to clear)</span></h3>
      <div class="lg-list">
        {#if loading}
          <div class="lg-empty">loading…</div>
        {:else if entries.length === 0}
          <div class="lg-empty">no logs yet</div>
        {:else}
          {#each entries as e (e.at + e.message)}
            <div class="lg-row">
              <span class="lg-at">{new Date(e.at).toLocaleString()}</span>
              <span class="lg-msg">{e.message}</span>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .lg-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .lg-panel {
    width: 640px;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0.8rem 0;
  }
  .lg-panel h3 {
    margin: 0 0.9rem 0.5rem;
    color: var(--accent);
  }
  .lg-hint {
    color: var(--muted);
    font-size: 0.75rem;
    font-weight: normal;
  }
  .lg-list {
    overflow-y: auto;
  }
  .lg-row {
    display: flex;
    gap: 0.8rem;
    padding: 0.25rem 0.9rem;
    font-size: 0.85rem;
  }
  .lg-at {
    flex: 0 0 160px;
    color: var(--muted);
  }
  .lg-msg {
    flex: 1;
  }
  .lg-empty {
    padding: 0.4rem 0.9rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

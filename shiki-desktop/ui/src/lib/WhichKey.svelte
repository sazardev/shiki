<script lang="ts">
  // The which-key / command-palette overlay — mirrors shiki-tui's
  // `which.rs` + `App::handle_which_key_key`: near-fullscreen, filterable,
  // arrow/page/home/end navigation (deliberately not j/k — those are things
  // a user might type into the filter), Enter dispatches a bound action and
  // closes, Esc closes and resets the query.
  import { entries, navRows, actionLabel } from "./keymaps";
  import type { Action, KeyMaps } from "./keymaps";
  import { input } from "./input.svelte";

  interface Props {
    maps: KeyMaps;
    onDispatch: (action: Action) => void;
  }
  let { maps, onDispatch }: Props = $props();

  type Row =
    | { kind: "bound"; scope: string; key: string; action: Action; label: string }
    | { kind: "nav"; scope: string; key: string; label: string };

  let query = $state("");
  let selected = $state(0);
  let inputEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();

  const allRows = $derived.by((): Row[] => [
    ...navRows(maps).map((n) => ({ kind: "nav" as const, scope: n.scope, key: n.key, label: n.label })),
    ...entries(maps).map((e) => ({
      kind: "bound" as const,
      scope: e.scope,
      key: e.key,
      action: e.action,
      label: actionLabel[e.action],
    })),
  ]);

  const filtered = $derived.by((): Row[] => {
    const q = query.trim().toLowerCase();
    if (!q) return allRows;
    return allRows.filter(
      (r) => r.key.toLowerCase().includes(q) || r.label.toLowerCase().includes(q) || r.scope.toLowerCase().includes(q),
    );
  });

  $effect(() => {
    if (selected >= filtered.length) selected = Math.max(0, filtered.length - 1);
  });

  $effect(() => {
    if (input.overlay === "whichKey") {
      query = "";
      selected = 0;
      // Synchronous — see GlobalSearch.svelte's identical fix: rAF gets
      // throttled in a backgrounded/inactive tab and silently never fires.
      inputEl?.focus();
    }
  });

  $effect(() => {
    void selected;
    listEl?.querySelector<HTMLElement>(".wk-row.selected")?.scrollIntoView({ block: "nearest" });
  });

  function close() {
    input.overlay = null;
    query = "";
    selected = 0;
  }

  function accept(i: number) {
    const row = filtered[i];
    if (!row) return;
    if (row.kind === "bound") {
      close();
      onDispatch(row.action);
    }
    // Nav rows are informational only — no Action behind them to dispatch.
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
        selected = Math.min(selected + 1, filtered.length - 1);
        return;
      case "ArrowUp":
        e.preventDefault();
        selected = Math.max(selected - 1, 0);
        return;
      case "PageDown":
        e.preventDefault();
        selected = Math.min(selected + 10, filtered.length - 1);
        return;
      case "PageUp":
        e.preventDefault();
        selected = Math.max(selected - 10, 0);
        return;
      case "Home":
        e.preventDefault();
        selected = 0;
        return;
      case "End":
        e.preventDefault();
        selected = Math.max(0, filtered.length - 1);
        return;
    }
  }
</script>

{#if input.overlay === "whichKey"}
  <div class="wk-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="wk-panel">
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={onKeydown}
        placeholder="Filter keybindings… (Enter to run, Esc to close)"
        class="wk-input"
      />
      <div class="wk-list" bind:this={listEl}>
        {#each filtered as row, i (row.scope + row.key + row.label)}
          <button
            type="button"
            class="wk-row"
            class:selected={i === selected}
            class:nav={row.kind === "nav"}
            onclick={() => accept(i)}
            onmouseenter={() => (selected = i)}
          >
            <span class="wk-scope">{row.scope}</span>
            <span class="wk-key">{row.key}</span>
            <span class="wk-label">{row.label}</span>
          </button>
        {:else}
          <div class="wk-empty">no matching keybindings</div>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .wk-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    padding: 5% 8%;
  }
  .wk-panel {
    width: 100%;
    height: 100%;
    max-width: 900px;
    background: var(--bg);
    border: 1px solid var(--accent);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .wk-input {
    padding: 0.7rem 0.9rem;
    background: var(--bg);
    border: none;
    border-bottom: 1px solid var(--accent);
    color: var(--fg);
    font-size: 0.95rem;
    outline: none;
  }
  .wk-list {
    flex: 1;
    overflow-y: auto;
    padding: 0.3rem;
  }
  .wk-row {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    width: 100%;
    padding: 0.35rem 0.6rem;
    background: none;
    border: none;
    border-radius: 4px;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .wk-row.selected {
    background: var(--selection);
  }
  .wk-row.nav .wk-label {
    color: var(--muted);
  }
  .wk-scope {
    flex: 0 0 130px;
    font-size: 0.7rem;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--panel-title);
  }
  .wk-key {
    flex: 0 0 90px;
    font-family: ui-monospace, monospace;
    color: var(--accent);
  }
  .wk-label {
    flex: 1;
  }
  .wk-empty {
    padding: 1rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

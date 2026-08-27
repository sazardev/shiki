<script lang="ts">
  // Global tasks view (leader+t) — every checkbox across every notebook,
  // toggleable in place, sorted by urgency (dated-ascending first, then
  // undated) same as the TUI's flat `panel_tasks::TaskRow` list.
  import { api } from "./api";
  import type { TaskRow } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    onJump: (notebook: string, path: string) => void;
  }
  let { onJump }: Props = $props();

  let rows: TaskRow[] = $state([]);
  let selected = $state(0);
  let loading = $state(false);
  let listEl: HTMLDivElement | undefined = $state();

  function sortRows(list: TaskRow[]): TaskRow[] {
    return [...list].sort((a, b) => {
      if (a.done !== b.done) return a.done ? 1 : -1;
      if (!a.due && !b.due) return 0;
      if (!a.due) return 1;
      if (!b.due) return -1;
      return a.due.localeCompare(b.due);
    });
  }

  async function load() {
    loading = true;
    try {
      rows = sortRows(await api.listTasks());
    } catch {
      rows = [];
    }
    loading = false;
    selected = 0;
  }

  $effect(() => {
    if (input.overlay === "tasks") void load();
  });

  // See ThemePicker.svelte's identical fix: arrow-key moves alone don't
  // scroll the list, so the selection can silently move off-screen.
  $effect(() => {
    void selected;
    listEl?.querySelector<HTMLElement>(".tk-row.selected")?.scrollIntoView({ block: "nearest" });
  });

  function close() {
    input.overlay = null;
  }

  async function toggle(i: number) {
    const row = rows[i];
    if (!row) return;
    try {
      await api.toggleTask(row.notebook, row.path, row.raw_line, row.occurrence);
      rows[i] = { ...row, done: !row.done };
    } catch {
      // leave the row as-is — a failed toggle isn't worth a whole reload
    }
  }

  function jump(i: number) {
    const row = rows[i];
    if (!row) return;
    close();
    onJump(row.notebook, row.path);
  }

  function onKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "Escape":
      case "q":
        e.preventDefault();
        close();
        return;
      case " ":
      case "Enter":
        e.preventDefault();
        void toggle(selected);
        return;
      case "o":
      case "l":
        e.preventDefault();
        jump(selected);
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

  function dueClass(row: TaskRow): string {
    if (row.done || !row.due) return "muted";
    const today = new Date().toISOString().slice(0, 10);
    if (row.due < today) return "overdue";
    if (row.due === today) return "today";
    return "muted";
  }
</script>

<svelte:window onkeydown={input.overlay === "tasks" ? onKeydown : undefined} />

{#if input.overlay === "tasks"}
  <div class="tk-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="tk-panel">
      <h3>Tasks</h3>
      <div class="tk-list" bind:this={listEl}>
        {#if loading}
          <div class="tk-empty">loading…</div>
        {:else if rows.length === 0}
          <div class="tk-empty">no tasks found</div>
        {:else}
          {#each rows as row, i (row.notebook + row.path + row.raw_line + row.occurrence)}
            <div class="tk-row" class:selected={i === selected} onmouseenter={() => (selected = i)}>
              <button type="button" class="tk-check" class:done={row.done} onclick={() => toggle(i)}>
                {row.done ? "x" : " "}
              </button>
              <button type="button" class="tk-text" class:done={row.done} onclick={() => jump(i)}>
                {row.text}
              </button>
              <span class="tk-loc">{row.location}</span>
              {#if row.due}
                <span class="tk-due {dueClass(row)}">{row.due}</span>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .tk-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    padding: 8% 12%;
  }
  .tk-panel {
    width: 100%;
    max-height: 80vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0.8rem 0;
  }
  .tk-panel h3 {
    margin: 0 0.9rem 0.5rem;
    color: var(--accent);
  }
  .tk-list {
    overflow-y: auto;
  }
  .tk-row {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    padding: 0.3rem 0.9rem;
  }
  .tk-row.selected {
    background: var(--selection);
  }
  .tk-check {
    flex: 0 0 1.4em;
    background: none;
    border: 1px solid var(--muted);
    color: var(--fg);
    cursor: pointer;
    font-family: inherit;
  }
  .tk-check.done {
    border-color: var(--success);
    color: var(--success);
  }
  .tk-text {
    flex: 1;
    background: none;
    border: none;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .tk-text.done {
    color: var(--muted);
    text-decoration: line-through;
  }
  .tk-loc {
    flex: 0 0 160px;
    color: var(--muted);
    font-size: 0.75rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tk-due {
    flex: 0 0 90px;
    font-size: 0.78rem;
    text-align: right;
  }
  .tk-due.overdue {
    color: var(--error);
  }
  .tk-due.today {
    color: var(--warning);
  }
  .tk-due.muted {
    color: var(--muted);
  }
  .tk-empty {
    padding: 0.4rem 0.9rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

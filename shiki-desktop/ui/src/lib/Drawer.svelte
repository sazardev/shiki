<script lang="ts">
  // Notebook drawer (leader+b) — a persistent left sidebar listing every
  // notebook's git status in color, separate from the always-visible
  // NOTEBOOKS panel (which only shows names) and from the read-only
  // Git Dashboard modal (a point-in-time snapshot you close again) — the
  // drawer stays open while you keep working, same as the TUI's.
  import { api } from "./api";
  import type { NotebookInfo, GitStatus } from "./api";
  import { input } from "./input.svelte";
  import { gitStatusKind, gitStatusSuffix } from "./statusBar";

  interface Props {
    notebooks: NotebookInfo[];
    activeNb: string | null;
    onSelect: (name: string) => void;
  }
  let { notebooks, activeNb, onSelect }: Props = $props();

  let statuses: Record<string, GitStatus | null> = $state({});

  async function refresh() {
    const out: Record<string, GitStatus | null> = {};
    for (const nb of notebooks) {
      try {
        out[nb.name] = await api.gitStatus(nb.name);
      } catch {
        out[nb.name] = null;
      }
    }
    statuses = out;
  }

  $effect(() => {
    if (input.showDrawer) void refresh();
  });

  function colorVar(gs: GitStatus | null): string {
    if (!gs) return "muted";
    const kind = gitStatusKind(gs);
    return kind === "dirty" ? "warning" : kind === "diverged" ? "accent" : "success";
  }
</script>

{#if input.showDrawer}
  <aside class="drawer">
    <h2>DRAWER</h2>
    <ul>
      {#each notebooks as nb (nb.name)}
        <li class:selected={activeNb === nb.name}>
          <button type="button" onclick={() => onSelect(nb.name)}>
            <span class="name">{nb.name}</span>
            <span class="status" style="color: var(--{colorVar(statuses[nb.name])})">
              {statuses[nb.name] ? gitStatusSuffix(statuses[nb.name]!).trim() || "clean" : "…"}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  </aside>
{/if}

<style>
  .drawer {
    width: 180px;
    flex-shrink: 0;
    border-right: 2px solid var(--border);
    overflow-y: auto;
    padding: 0.5rem 0;
  }
  .drawer h2 {
    font-size: 0.72rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--panel-title);
    margin: 0.4rem 0.9rem;
  }
  .drawer ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .drawer li button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.1rem;
    width: 100%;
    padding: 0.35rem 0.9rem;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .drawer li button:hover {
    background: color-mix(in srgb, var(--fg) 8%, transparent);
  }
  .drawer li.selected button {
    background: var(--selection);
    border-left-color: var(--accent);
  }
  .status {
    font-size: 0.72rem;
  }
</style>

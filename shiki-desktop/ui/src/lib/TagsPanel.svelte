<script lang="ts">
  // Tags panel (leader+T) — two levels deep, same "browse then jump" shape
  // as the TUI's tags modal: level 1 lists distinct tags in the current
  // notebook, level 2 lists every note carrying the selected tag. Built
  // entirely from the already-loaded `notes` prop — no backend command
  // needed, since every note's tags are already in `NoteInfo`.
  import type { NoteInfo } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    notes: NoteInfo[];
    onJump: (path: string) => void;
  }
  let { notes, onJump }: Props = $props();

  let level: 1 | 2 = $state(1);
  let selected = $state(0);
  let viewingTag: string | null = $state(null);
  let listEl: HTMLDivElement | undefined = $state();

  const tags = $derived.by(() => {
    const set = new Set<string>();
    for (const n of notes) for (const t of n.tags) set.add(t);
    return [...set].sort();
  });

  const notesWithTag = $derived(viewingTag ? notes.filter((n) => n.tags.includes(viewingTag)) : []);

  $effect(() => {
    if (input.overlay === "tags") {
      level = 1;
      selected = 0;
      viewingTag = null;
    }
  });

  // See ThemePicker.svelte's identical fix: arrow-key moves alone don't
  // scroll the list, so the selection can silently move off-screen.
  $effect(() => {
    void selected;
    void level;
    listEl?.querySelector<HTMLElement>(".tg-row.selected")?.scrollIntoView({ block: "nearest" });
  });

  function close() {
    input.overlay = null;
  }

  function drillInto(i: number) {
    const tag = tags[i];
    if (!tag) return;
    viewingTag = tag;
    level = 2;
    selected = 0;
  }

  function back() {
    level = 1;
    viewingTag = null;
    selected = 0;
  }

  function acceptNote(i: number) {
    const n = notesWithTag[i];
    if (!n) return;
    close();
    onJump(n.path);
  }

  function onKeydown(e: KeyboardEvent) {
    const len = level === 1 ? tags.length : notesWithTag.length;
    switch (e.key) {
      case "Escape":
        if (level === 2) {
          e.preventDefault();
          back();
        } else {
          e.preventDefault();
          close();
        }
        return;
      case "Backspace":
      case "h":
      case "ArrowLeft":
        if (level === 2) {
          e.preventDefault();
          back();
        }
        return;
      case "Enter":
      case "l":
      case "ArrowRight":
        e.preventDefault();
        if (level === 1) drillInto(selected);
        else acceptNote(selected);
        return;
      case "ArrowDown":
      case "j":
        e.preventDefault();
        selected = Math.min(selected + 1, len - 1);
        return;
      case "ArrowUp":
      case "k":
        e.preventDefault();
        selected = Math.max(selected - 1, 0);
        return;
    }
  }
</script>

<svelte:window onkeydown={input.overlay === "tags" ? onKeydown : undefined} />

{#if input.overlay === "tags"}
  <div class="tg-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="tg-panel">
      <h3>{level === 1 ? "Tags" : `#${viewingTag}`}</h3>
      <div class="tg-list" bind:this={listEl}>
        {#if level === 1}
          {#each tags as tag, i (tag)}
            <button
              type="button"
              class="tg-row"
              class:selected={i === selected}
              onclick={() => drillInto(i)}
              onmouseenter={() => (selected = i)}
            >
              #{tag}
            </button>
          {:else}
            <div class="tg-empty">no tags in this notebook</div>
          {/each}
        {:else}
          {#each notesWithTag as n, i (n.path)}
            <button
              type="button"
              class="tg-row"
              class:selected={i === selected}
              onclick={() => acceptNote(i)}
              onmouseenter={() => (selected = i)}
            >
              {n.title}
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .tg-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .tg-panel {
    width: 380px;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 0.8rem 0;
  }
  .tg-panel h3 {
    margin: 0 0.9rem 0.5rem;
    color: var(--accent);
  }
  .tg-list {
    overflow-y: auto;
  }
  .tg-row {
    display: block;
    width: 100%;
    padding: 0.35rem 0.9rem;
    background: none;
    border: none;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .tg-row.selected {
    background: var(--selection);
  }
  .tg-empty {
    padding: 0.4rem 0.9rem;
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

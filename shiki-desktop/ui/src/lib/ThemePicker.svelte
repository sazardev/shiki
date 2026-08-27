<script lang="ts">
  // Theme picker (leader+c) — live-previews by re-applying the theme CSS as
  // the cursor moves (same "mutate self.theme while browsing, only persist
  // to config.toml on Enter" behavior the TUI's picker has), Esc reverts to
  // whatever was actually committed before the picker opened.
  import { api } from "./api";
  import type { ThemeInfo } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    initialThemeName: string;
    onPreview: (name: string) => void;
    onConfirm: (name: string) => void | Promise<void>;
  }
  let { initialThemeName, onPreview, onConfirm }: Props = $props();

  let themes: ThemeInfo[] = $state([]);
  let selected = $state(0);
  let query = $state("");
  let inputEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLDivElement | undefined = $state();

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return themes;
    return themes.filter((t) => t.name.toLowerCase().includes(q) || t.family.toLowerCase().includes(q));
  });

  async function load() {
    themes = await api.listThemes();
    const idx = themes.findIndex((t) => t.name === initialThemeName);
    selected = Math.max(0, idx);
  }

  $effect(() => {
    if (input.overlay === "themePicker") {
      query = "";
      void load();
      inputEl?.focus();
    }
  });

  // Jump to the top filtered match on every keystroke — not just clamp.
  // `selected` starts at the *current theme's* index in the full 37-item
  // list (e.g. 8 for gruvbox-dark); typing a filter narrows `filtered` out
  // from under that index entirely, so `filtered[selected]` silently
  // resolved to `undefined` — which both the preview effect below and
  // `accept()` already guard on and skip, so live-preview *and* Enter both
  // went dead the moment a filter was typed. A bare clamp would still leave
  // `selected` pointing at the *wrong* row within the now-different
  // `filtered` array (index 8 of the full list isn't index 8 of a 3-item
  // filtered list); resetting to 0 on every query change is what
  // GlobalSearch.svelte's `runSearch` already does for the same reason.
  $effect(() => {
    // Deliberately keyed on `query`, not `filtered` — `filtered` also
    // recomputes when `themes` first loads (async, after this component
    // mounts), and resetting on *that* would stomp the real selected index
    // `load()` just computed for the current theme right back to 0.
    void query;
    selected = 0;
  });

  $effect(() => {
    const t = filtered[selected];
    if (input.overlay === "themePicker" && t) onPreview(t.name);
  });

  // Arrow-key moves change `selected` but don't touch scroll position on
  // their own — without this the highlighted row can move past the visible
  // area with no visual feedback at all until a mouse hover happens to land
  // on a still-visible row, which looked like "keyboard nav is broken."
  $effect(() => {
    void selected;
    listEl?.querySelector<HTMLElement>(".tp-row.selected")?.scrollIntoView({ block: "nearest" });
  });

  function reopenSettingsIfNeeded() {
    if (input.reopenSettingsAfterThemePicker) {
      input.reopenSettingsAfterThemePicker = false;
      input.showSettings = true;
    }
  }

  function close(revert: boolean) {
    input.overlay = null;
    if (revert) onPreview(initialThemeName);
    reopenSettingsIfNeeded();
  }

  // Async and awaits `onConfirm` before reopening Settings — `confirmTheme`
  // (App.svelte) only updates `config.theme.name` after an `await`, so
  // reopening synchronously right after firing it (and-forget) cloned a
  // stale `config` into Settings' `local`, showing the *previous* theme
  // name until closed and reopened again. Awaiting first also means the
  // reopen now lands outside the original synchronous keydown dispatch, so
  // it doesn't need the same stopImmediatePropagation escape hatch Escape
  // does below (Settings' own listener already ran, with showSettings still
  // false, before this resumes).
  async function accept(i: number) {
    const t = filtered[i];
    if (!t) return;
    input.overlay = null;
    await onConfirm(t.name);
    reopenSettingsIfNeeded();
  }

  function onKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "Escape":
        e.preventDefault();
        // `<svelte:window onkeydown={cond ? fn : undefined}>` re-evaluates
        // `cond` live on every keystroke through one persistent listener
        // rather than attaching/detaching it — so if this picker was opened
        // from Settings, reopening it (synchronously, right below) would
        // otherwise let Settings' own listener see `showSettings` flip back
        // true *within this same dispatch* and immediately close it again.
        // stopImmediatePropagation keeps this Escape from reaching it.
        if (input.reopenSettingsAfterThemePicker) e.stopImmediatePropagation();
        close(true);
        return;
      case "Enter":
        e.preventDefault();
        void accept(selected);
        return;
      case "ArrowDown":
        e.preventDefault();
        selected = Math.min(selected + 1, filtered.length - 1);
        return;
      case "ArrowUp":
        e.preventDefault();
        selected = Math.max(selected - 1, 0);
        return;
    }
  }
</script>

<svelte:window onkeydown={input.overlay === "themePicker" ? onKeydown : undefined} />

{#if input.overlay === "themePicker"}
  <div class="tp-backdrop" onclick={(e) => e.target === e.currentTarget && close(true)}>
    <div class="tp-panel">
      <input bind:this={inputEl} bind:value={query} placeholder="Filter themes…" class="tp-input" />
      <div class="tp-list" bind:this={listEl}>
        {#each filtered as t, i (t.name)}
          <button
            type="button"
            class="tp-row"
            class:selected={i === selected}
            onclick={() => accept(i)}
            onmouseenter={() => (selected = i)}
          >
            <span class="tp-family">{t.family}</span>
            <span class="tp-name">{t.name}</span>
          </button>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .tp-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .tp-panel {
    width: 380px;
    max-height: 70vh;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .tp-input {
    padding: 0.6rem 0.8rem;
    background: var(--bg);
    border: none;
    border-bottom: 1px solid var(--accent);
    color: var(--fg);
  }
  .tp-list {
    overflow-y: auto;
    padding: 0.3rem;
  }
  .tp-row {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    width: 100%;
    padding: 0.35rem 0.6rem;
    background: none;
    border: none;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .tp-row.selected {
    background: var(--selection);
  }
  .tp-family {
    flex: 0 0 70px;
    font-size: 0.72rem;
    color: var(--muted);
    text-transform: uppercase;
  }
</style>

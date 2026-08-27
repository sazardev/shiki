<script lang="ts">
  // A hand-drawn stand-in for `<select>` — native select popups are OS-drawn
  // chrome that can't be restyled to match the TUI's square, themed look
  // (this is what the Template picker in the "new note" modal used to be).
  // Renders as a themed button that opens a plain list, the same visual
  // language as every other list in this app (which-key, notebooks, notes).
  interface Option {
    value: string;
    label: string;
  }

  interface Props {
    value: string;
    options: Option[];
  }

  let { value = $bindable(), options }: Props = $props();
  let open = $state(false);
  let rootEl: HTMLDivElement | undefined = $state();

  const selectedLabel = $derived(options.find((o) => o.value === value)?.label ?? value);

  function toggle() {
    open = !open;
  }

  function pick(v: string) {
    value = v;
    open = false;
  }

  function onDocumentClick(e: MouseEvent) {
    if (open && rootEl && !rootEl.contains(e.target as Node)) open = false;
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      open = false;
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      toggle();
    }
  }
</script>

<svelte:window onclick={onDocumentClick} />

<div class="dropdown" bind:this={rootEl}>
  <button type="button" class="dropdown-trigger" onclick={toggle} onkeydown={onKeydown} aria-expanded={open}>
    <span>{selectedLabel}</span>
    <span class="caret">{open ? "▲" : "▼"}</span>
  </button>
  {#if open}
    <div class="dropdown-list">
      {#each options as o (o.value)}
        <button
          type="button"
          class="dropdown-item"
          class:selected={o.value === value}
          onclick={() => pick(o.value)}
        >
          {o.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .dropdown {
    position: relative;
  }
  .dropdown-trigger {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.45rem 0.6rem;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--fg);
    cursor: pointer;
    text-align: left;
  }
  .dropdown-trigger:hover,
  .dropdown-trigger[aria-expanded="true"] {
    border-color: var(--accent);
  }
  .caret {
    color: var(--muted);
    font-size: 0.7em;
    margin-left: 0.5em;
  }
  .dropdown-list {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    z-index: 10;
    max-height: 220px;
    overflow-y: auto;
    background: var(--bg);
    border: 1px solid var(--accent);
    margin-top: 2px;
  }
  .dropdown-item {
    display: block;
    width: 100%;
    padding: 0.4rem 0.6rem;
    background: none;
    border: none;
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .dropdown-item:hover {
    background: color-mix(in srgb, var(--fg) 8%, transparent);
  }
  .dropdown-item.selected {
    background: var(--selection);
    color: var(--accent);
  }
</style>

<script lang="ts">
  // Settings screen (leader+s) — every config option editable in place,
  // grouped by section and paged by tab, same shape as the TUI's Settings
  // screen (GENERAL/THEME/GIT/EDITOR/EXPORT/NOTEBOOKS/SNIPPETS). Rather
  // than hand-building a form row per field (General alone has 26 —
  // and every field across every section would mean 60+ bespoke rows),
  // each scalar-valued section is rendered generically: every boolean key
  // becomes a checkbox, every string/number key becomes a text/number
  // input, auto-saved on change/blur straight through `save_full_config`
  // (the whole config round-trips through Rust's `Config` type on save, so
  // a bad value is rejected with a clear error rather than corrupting
  // config.toml). NOTEBOOKS and SNIPPETS are the two structured
  // (map-valued) sections and get their own two-level drill-down, same
  // "browse then jump" shape every other two-level modal in this app uses.
  import { api } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    config: any;
    notebookNames: string[];
    onSaved: (config: any) => void;
  }
  let { config, notebookNames, onSaved }: Props = $props();

  const SECTIONS = ["GENERAL", "THEME", "GIT", "EDITOR", "EXPORT", "NOTEBOOKS", "SNIPPETS"] as const;
  type Section = (typeof SECTIONS)[number];
  let section: Section = $state("GENERAL");

  let local: any = $state(null);
  let notebookDrill: string | null = $state(null);
  let snippetDrill: string | null = $state(null);
  let newSnippetTrigger = $state("");
  let saveError = $state("");

  $effect(() => {
    if (input.showSettings && !local) {
      // `config` is a $state proxy — structuredClone can choke on Svelte 5's
      // reactive proxy wrapping directly; $state.snapshot() gives the plain,
      // cloneable object first.
      local = structuredClone($state.snapshot(config));
    }
    if (!input.showSettings) {
      local = null;
      notebookDrill = null;
      snippetDrill = null;
    }
  });

  async function save() {
    try {
      await api.saveFullConfig(local);
      saveError = "";
      onSaved(local);
    } catch (e) {
      saveError = String(e);
    }
  }

  function close() {
    input.showSettings = false;
  }

  function fieldRows(obj: Record<string, unknown>): [string, unknown][] {
    return Object.entries(obj).filter(([, v]) => typeof v === "boolean" || typeof v === "string" || typeof v === "number");
  }

  function switchSection(s: Section) {
    section = s;
    notebookDrill = null;
    snippetDrill = null;
  }

  function cycleSection(dir: 1 | -1) {
    const i = SECTIONS.indexOf(section);
    switchSection(SECTIONS[(i + dir + SECTIONS.length) % SECTIONS.length]);
  }

  // Mutating — only ever call this from an event handler. Svelte 5 forbids
  // mutating state from inside a template expression (`{@const}` counts),
  // so the template itself reads through `notebookOverrideView` instead.
  function notebookOverride(name: string): Record<string, unknown> {
    local.notebooks ??= {};
    local.notebooks[name] ??= {};
    return local.notebooks[name];
  }

  function notebookOverrideView(name: string): Record<string, unknown> {
    return (local.notebooks?.[name] as Record<string, unknown> | undefined) ?? {};
  }

  function cycleTriState(name: string, key: "auto_push" | "auto_sync") {
    const ov = notebookOverride(name);
    const cur = ov[key];
    ov[key] = cur === undefined ? true : cur === true ? false : undefined;
    void save();
  }

  function addSnippet() {
    const trigger = newSnippetTrigger.trim();
    if (!trigger) return;
    local.snippets ??= {};
    local.snippets[trigger] = { label: "", body: "" };
    newSnippetTrigger = "";
    snippetDrill = trigger;
    void save();
  }

  function deleteSnippet(trigger: string) {
    delete local.snippets[trigger];
    snippetDrill = null;
    void save();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (notebookDrill || snippetDrill) {
        e.preventDefault();
        notebookDrill = null;
        snippetDrill = null;
      } else {
        e.preventDefault();
        close();
      }
      return;
    }
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      cycleSection(-1);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      cycleSection(1);
    }
  }
</script>

<svelte:window onkeydown={input.showSettings ? onKeydown : undefined} />

{#if input.showSettings && local}
  <div class="st-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="st-panel">
      <div class="st-tabs">
        {#each SECTIONS as s (s)}
          <button type="button" class="st-tab" class:active={section === s} onclick={() => switchSection(s)}>
            {s}
          </button>
        {/each}
        <span class="st-spacer"></span>
        {#if saveError}<span class="st-error">{saveError}</span>{/if}
        <button type="button" class="tool-btn" onclick={close}>Close</button>
      </div>

      <div class="st-body">
        {#if section === "GENERAL"}
          {#each fieldRows(local.general) as [key, value] (key)}
            <label class="st-row">
              <span class="st-key">{key}</span>
              {#if typeof value === "boolean"}
                <input
                  type="checkbox"
                  checked={value}
                  onchange={(e) => {
                    local.general[key] = (e.target as HTMLInputElement).checked;
                    void save();
                  }}
                />
              {:else}
                <input
                  type={typeof value === "number" ? "number" : "text"}
                  value={value}
                  onblur={(e) => {
                    const raw = (e.target as HTMLInputElement).value;
                    local.general[key] = typeof value === "number" ? Number(raw) : raw;
                    void save();
                  }}
                  onkeydown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
                />
              {/if}
            </label>
          {/each}
        {:else if section === "THEME"}
          <div class="st-row">
            <span class="st-key">name</span>
            <span class="st-value">{local.theme?.name ?? "-"}</span>
            <button
              type="button"
              class="tool-btn"
              onclick={() => {
                input.reopenSettingsAfterThemePicker = true;
                input.showSettings = false;
                input.overlay = "themePicker";
              }}>Open picker</button
            >
          </div>
          <p class="st-hint">Individual color overrides aren't editable here — use `shiki theme create --from` (CLI) or hand-edit `[theme.overrides]` in config.toml.</p>
        {:else if section === "GIT"}
          {#each fieldRows(local.git ?? {}) as [key, value] (key)}
            <label class="st-row">
              <span class="st-key">{key}</span>
              {#if typeof value === "boolean"}
                <input
                  type="checkbox"
                  checked={value}
                  onchange={(e) => {
                    local.git[key] = (e.target as HTMLInputElement).checked;
                    void save();
                  }}
                />
              {:else}
                <input
                  type={typeof value === "number" ? "number" : "text"}
                  value={value}
                  onblur={(e) => {
                    const raw = (e.target as HTMLInputElement).value;
                    local.git[key] = typeof value === "number" ? Number(raw) : raw;
                    void save();
                  }}
                  onkeydown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
                />
              {/if}
            </label>
          {/each}
        {:else if section === "EDITOR"}
          {#each fieldRows(local.editor ?? {}) as [key, value] (key)}
            <label class="st-row">
              <span class="st-key">{key}</span>
              {#if typeof value === "boolean"}
                <input
                  type="checkbox"
                  checked={value}
                  onchange={(e) => {
                    local.editor[key] = (e.target as HTMLInputElement).checked;
                    void save();
                  }}
                />
              {:else}
                <input
                  type={typeof value === "number" ? "number" : "text"}
                  value={value}
                  onblur={(e) => {
                    const raw = (e.target as HTMLInputElement).value;
                    local.editor[key] = typeof value === "number" ? Number(raw) : raw;
                    void save();
                  }}
                  onkeydown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
                />
              {/if}
            </label>
          {/each}
        {:else if section === "EXPORT"}
          {#each fieldRows(local.export ?? {}) as [key, value] (key)}
            <label class="st-row">
              <span class="st-key">{key}</span>
              {#if typeof value === "boolean"}
                <input
                  type="checkbox"
                  checked={value}
                  onchange={(e) => {
                    local.export[key] = (e.target as HTMLInputElement).checked;
                    void save();
                  }}
                />
              {:else}
                <input
                  type={typeof value === "number" ? "number" : "text"}
                  value={value}
                  onblur={(e) => {
                    const raw = (e.target as HTMLInputElement).value;
                    local.export[key] = typeof value === "number" ? Number(raw) : raw;
                    void save();
                  }}
                  onkeydown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
                />
              {/if}
            </label>
          {/each}
        {:else if section === "NOTEBOOKS"}
          {#if notebookDrill}
            {@const ov = notebookOverrideView(notebookDrill)}
            <button type="button" class="tool-btn st-back" onclick={() => (notebookDrill = null)}>← back</button>
            <h4>{notebookDrill}</h4>
            <div class="st-row">
              <span class="st-key">auto_push</span>
              <button type="button" class="tool-btn" onclick={() => cycleTriState(notebookDrill!, "auto_push")}>
                {ov.auto_push === undefined ? "inherit" : ov.auto_push ? "true" : "false"}
              </button>
            </div>
            <div class="st-row">
              <span class="st-key">auto_sync</span>
              <button type="button" class="tool-btn" onclick={() => cycleTriState(notebookDrill!, "auto_sync")}>
                {ov.auto_sync === undefined ? "inherit" : ov.auto_sync ? "true" : "false"}
              </button>
            </div>
            <label class="st-row">
              <span class="st-key">auto_sync_every</span>
              <input
                type="number"
                value={ov.auto_sync_every ?? ""}
                placeholder="inherit"
                onblur={(e) => {
                  const raw = (e.target as HTMLInputElement).value;
                  notebookOverride(notebookDrill!).auto_sync_every = raw === "" ? undefined : Number(raw);
                  void save();
                }}
              />
            </label>
          {:else}
            {#each notebookNames as name (name)}
              <button type="button" class="st-row st-list-row" onclick={() => (notebookDrill = name)}>
                {name}
              </button>
            {/each}
          {/if}
        {:else if section === "SNIPPETS"}
          {#if snippetDrill}
            {@const sn = local.snippets[snippetDrill]}
            <button type="button" class="tool-btn st-back" onclick={() => (snippetDrill = null)}>← back</button>
            <h4>{snippetDrill}</h4>
            <label class="st-row">
              <span class="st-key">label</span>
              <input
                type="text"
                value={sn.label ?? ""}
                onblur={(e) => {
                  sn.label = (e.target as HTMLInputElement).value;
                  void save();
                }}
              />
            </label>
            <label class="st-row st-row-block">
              <span class="st-key">body</span>
              <textarea
                value={sn.body ?? ""}
                onblur={(e) => {
                  sn.body = (e.target as HTMLTextAreaElement).value;
                  void save();
                }}
              ></textarea>
            </label>
            <button type="button" class="danger-btn" onclick={() => deleteSnippet(snippetDrill!)}>Delete snippet</button>
          {:else}
            {#each Object.keys(local.snippets ?? {}) as trigger (trigger)}
              <button type="button" class="st-row st-list-row" onclick={() => (snippetDrill = trigger)}>
                /{trigger}
              </button>
            {/each}
            <div class="st-row">
              <input placeholder="new trigger" bind:value={newSnippetTrigger} onkeydown={(e) => e.key === "Enter" && addSnippet()} />
              <button type="button" class="tool-btn" onclick={addSnippet} disabled={!newSnippetTrigger.trim()}>+ add</button>
            </div>
          {/if}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .st-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .st-panel {
    width: 90%;
    height: 85%;
    max-width: 760px;
    background: var(--bg);
    border: 1px solid var(--accent);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .st-tabs {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    padding: 0.5rem 0.7rem;
    border-bottom: 1px solid var(--border);
  }
  .st-tab {
    background: none;
    border: none;
    color: var(--muted);
    padding: 0.3rem 0.6rem;
    cursor: pointer;
    font-size: 0.8rem;
    letter-spacing: 0.04em;
  }
  .st-tab.active {
    color: var(--accent);
    border-bottom: 2px solid var(--accent);
  }
  .st-spacer {
    flex: 1;
  }
  .st-error {
    color: var(--error);
    font-size: 0.78rem;
  }
  .st-body {
    flex: 1;
    overflow-y: auto;
    padding: 0.8rem 1rem;
  }
  .st-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.3rem 0;
  }
  .st-row-block {
    align-items: flex-start;
    flex-direction: column;
  }
  .st-list-row {
    width: 100%;
    background: none;
    border: none;
    border-bottom: 1px solid var(--border);
    color: var(--fg);
    text-align: left;
    cursor: pointer;
  }
  .st-key {
    flex: 0 0 220px;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .st-value {
    color: var(--fg);
  }
  .st-row input[type="text"],
  .st-row input[type="number"] {
    flex: 1;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--fg);
    padding: 0.25rem 0.5rem;
  }
  .st-row input:focus {
    border-color: var(--accent);
  }
  .st-row-block textarea {
    width: 100%;
    min-height: 120px;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--fg);
    padding: 0.4rem 0.5rem;
    font-family: inherit;
  }
  .st-hint {
    color: var(--muted);
    font-size: 0.8rem;
  }
  .st-back {
    margin-bottom: 0.5rem;
  }
  h4 {
    color: var(--accent);
    margin: 0.2rem 0 0.6rem;
  }
</style>

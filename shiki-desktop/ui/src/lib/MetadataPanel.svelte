<script lang="ts">
  // Metadata modal (notes/preview-scope M) — the selected note's tags,
  // add/remove in place. Custom frontmatter fields beyond tags (`extra` in
  // shiki-core's `Frontmatter`) are shown read-only rather than editable:
  // safely rewriting arbitrary YAML from a plain web form risks corrupting
  // it, where `tags:` is a single well-known scalar-list key this can
  // rewrite by exact match. Anything else is still reachable — `i` opens
  // the note's raw frontmatter as plain text like any other edit.
  import { api } from "./api";
  import { input } from "./input.svelte";

  interface Props {
    notebook: string | null;
    path: string | null;
    onSaved: () => void;
  }
  let { notebook, path, onSaved }: Props = $props();

  let rawContent = $state("");
  let tags: string[] = $state([]);
  let extraKeys: string[] = $state([]);
  let newTag = $state("");
  let loading = $state(false);
  let inputEl: HTMLInputElement | undefined = $state();

  async function load() {
    if (!notebook || !path) return;
    loading = true;
    try {
      const note = await api.readNote(notebook, path);
      rawContent = note.content;
      tags = [...note.frontmatter.tags];
      extraKeys = Object.keys(note.frontmatter).filter(
        (k) => !["title", "date", "tags", "aliases", "notebook", "links", "template"].includes(k),
      );
    } catch {
      rawContent = "";
      tags = [];
    }
    loading = false;
  }

  $effect(() => {
    if (input.overlay === "metadata") {
      newTag = "";
      void load();
      inputEl?.focus();
    }
  });

  function close() {
    input.overlay = null;
  }

  function removeTag(t: string) {
    tags = tags.filter((x) => x !== t);
  }

  function addTag() {
    const t = newTag.trim();
    if (t && !tags.includes(t)) tags = [...tags, t];
    newTag = "";
  }

  function rewriteTagsInContent(raw: string, tagList: string[]): string {
    const block = tagList.length === 0 ? "tags: []" : "tags:\n" + tagList.map((t) => `- ${t}`).join("\n");
    const re = /^tags:(?:\n- .*)*\n?/m;
    if (re.test(raw)) return raw.replace(re, block + "\n");
    return raw.replace(/^---\n/, `---\n${block}\n`);
  }

  async function save() {
    if (!notebook || !path) return;
    try {
      const updated = rewriteTagsInContent(rawContent, tags);
      await api.saveNote(notebook, path, updated);
      close();
      onSaved();
    } catch (e) {
      // Surface via the caller's own status handling on next load — this
      // modal has no status line of its own, so just keep it open.
      console.error(e);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  }
</script>

<svelte:window onkeydown={input.overlay === "metadata" ? onKeydown : undefined} />

{#if input.overlay === "metadata"}
  <div class="md-backdrop" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="md-panel">
      <h3>Metadata</h3>
      {#if loading}
        <div class="md-empty">loading…</div>
      {:else}
        <label>Tags</label>
        <div class="md-tags">
          {#each tags as t (t)}
            <span class="md-tag">
              #{t}
              <button type="button" class="md-tag-x" onclick={() => removeTag(t)}>×</button>
            </span>
          {/each}
        </div>
        <input
          bind:this={inputEl}
          bind:value={newTag}
          placeholder="add tag, Enter to confirm"
          onkeydown={(e) => e.key === "Enter" && addTag()}
        />
        {#if extraKeys.length > 0}
          <label>Other frontmatter fields (read-only — edit via i)</label>
          <div class="md-extra">{extraKeys.join(", ")}</div>
        {/if}
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={save}>Save</button>
          <button type="button" class="tool-btn" onclick={close}>Cancel</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .md-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }
  .md-panel {
    width: 420px;
    background: var(--bg);
    border: 1px solid var(--accent);
    padding: 1rem 1.2rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .md-panel h3 {
    margin: 0 0 0.3rem;
    color: var(--accent);
  }
  .md-panel label {
    font-size: 0.75rem;
    color: var(--muted);
    margin-top: 0.3rem;
  }
  .md-panel input {
    padding: 0.4rem 0.6rem;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--fg);
  }
  .md-panel input:focus {
    border-color: var(--accent);
  }
  .md-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .md-tag {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.15rem 0.5rem;
    background: var(--selection);
    color: var(--tag);
    font-size: 0.82rem;
  }
  .md-tag-x {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
    font-size: 0.9rem;
    line-height: 1;
  }
  .md-extra {
    color: var(--muted);
    font-size: 0.82rem;
  }
  .modal-actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
    margin-top: 0.4rem;
  }
  .md-empty {
    color: var(--muted);
    font-size: 0.85rem;
  }
</style>

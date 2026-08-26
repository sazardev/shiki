<script lang="ts">
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";
  import hljs from "highlight.js/lib/common";
  import { api } from "./lib/api";
  import type { NotebookInfo, NoteInfo, RenderedNote, GitStatus } from "./lib/api";
  import Onboarding from "./lib/Onboarding.svelte";
  import EditorPane from "./lib/EditorPane.svelte";

  // ---- app state ----
  let config: any = $state(null);
  let loadError: string = $state("");
  let notebooks: NotebookInfo[] = $state([]);
  let activeNb: string | null = $state(null);
  let notes: NoteInfo[] = $state([]);
  let searchQuery: string = $state("");
  let selectedPath: string | null = $state(null);
  let rendered: RenderedNote | null = $state(null);
  let previewError: string = $state("");
  let editing: boolean = $state(false);
  let editorContent: string = $state("");
  let git: GitStatus | null = $state(null);
  let statusMsg: string = $state("");

  // ---- modal state ----
  let newNoteOpen = $state(false);
  let newNoteTitle = $state("");
  let newNoteTemplate = $state("default");
  let renameOpen = $state(false);
  let renameValue = $state("");
  let confirmOpen = $state(false);
  let confirmAction = $state<() => void>(() => {});
  let confirmLabel = $state("");

  const themeName = $derived(config?.theme?.name ?? "…");
  const defaultNotebook = $derived(config?.general?.default_notebook ?? "…");
  const filteredNotes = $derived(
    searchQuery.trim()
      ? notes.filter(
          (n) =>
            n.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
            n.tags.some((t) => t.toLowerCase().includes(searchQuery.toLowerCase())),
        )
      : notes,
  );

  // ---- lifecycle ----
  onMount(async () => {
    try {
      const css: string = await api.getThemeCss();
      const style = document.createElement("style");
      style.setAttribute("data-shiki-theme", "active");
      style.textContent = css;
      document.head.appendChild(style);

      config = await api.getConfig();
      await loadNotebooks();
    } catch (e) {
      loadError = String(e);
    }
    setTimeout(checkForUpdate, 3000);
  });

  // ---- notebooks ----
  async function loadNotebooks() {
    notebooks = await api.listNotebooks();
    if (notebooks.length > 0) {
      const want = config?.general?.default_notebook ?? notebooks[0].name;
      const pick = notebooks.find((n) => n.name === want) ?? notebooks[0];
      await selectNotebook(pick.name);
    }
  }

  async function selectNotebook(name: string) {
    activeNb = name;
    selectedPath = null;
    rendered = null;
    editing = false;
    git = null;
    notes = await api.listNotes(name);
    try {
      git = await api.gitStatus(name);
    } catch {
      git = null;
    }
  }

  async function newNotebook(name: string) {
    await api.createNotebook(name);
    statusMsg = `notebook "${name}" created`;
    await loadNotebooks();
  }

  // ---- notes ----
  async function selectNote(path: string) {
    if (!activeNb) return;
    selectedPath = path;
    editing = false;
    previewError = "";
    try {
      rendered = await api.renderNote(activeNb, path);
      highlightCode();
    } catch (e) {
      rendered = null;
      previewError = String(e);
    }
  }

  function highlightCode() {
    requestAnimationFrame(() => {
      document.querySelectorAll(".preview pre code").forEach((el) => hljs.highlightElement(el as HTMLElement));
      document.querySelectorAll(".preview img").forEach((img) => {
        const src = img.getAttribute("src");
        if (src && !src.startsWith("http") && !src.startsWith("data:") && !src.startsWith("asset:")) {
          img.setAttribute("src", convertFileSrc(src));
        }
      });
    });
  }

  async function openEditor() {
    if (!activeNb || !selectedPath) return;
    try {
      const note = await api.readNote(activeNb, selectedPath);
      editorContent = note.content;
      editing = true;
    } catch (e) {
      statusMsg = String(e);
    }
  }

  async function saveEditor(content: string) {
    if (!activeNb || !selectedPath) return;
    try {
      await api.saveNote(activeNb, selectedPath, content);
      editing = false;
      statusMsg = "saved ✓";
      await refresh();
      await selectNote(selectedPath);
    } catch (e) {
      statusMsg = String(e);
    }
  }

  async function createNote() {
    if (!activeNb) return;
    try {
      const tpl = newNoteTemplate || null;
      const info = await api.createNote(activeNb, newNoteTitle.trim(), tpl, null);
      newNoteOpen = false;
      newNoteTitle = "";
      await refresh();
      await selectNote(info.path);
      await openEditor();
    } catch (e) {
      statusMsg = String(e);
    }
  }

  async function daily() {
    if (!activeNb) return;
    try {
      const info = await api.dailyNote(activeNb);
      await refresh();
      await selectNote(info.path);
      statusMsg = "daily note opened";
    } catch (e) {
      statusMsg = String(e);
    }
  }

  async function renameNote() {
    if (!activeNb || !selectedPath) return;
    try {
      await api.renameNote(activeNb, selectedPath, renameValue.trim());
      renameOpen = false;
      statusMsg = "renamed (wikilinks updated)";
      await refresh();
      await selectNote(selectedPath);
    } catch (e) {
      statusMsg = String(e);
    }
  }

  async function deleteNote() {
    if (!activeNb || !selectedPath) return;
    await api.deleteNote(activeNb, selectedPath);
    confirmOpen = false;
    selectedPath = null;
    rendered = null;
    statusMsg = "note deleted";
    await refresh();
  }

  async function refresh() {
    if (!activeNb) return;
    notes = await api.listNotes(activeNb);
    try {
      git = await api.gitStatus(activeNb);
    } catch {
      git = null;
    }
  }

  async function gitCommit() {
    if (!activeNb) return;
    try {
      const msg = await api.gitCommit(activeNb, "desktop edit");
      statusMsg = msg;
      await refresh();
    } catch (e) {
      statusMsg = String(e);
    }
  }

  // ---- auto-update (same as before) ----
  let update: any = $state(null);
  let updStatus: "idle" | "checking" | "ready" | "downloading" | "installing" | "done" | "error" =
    $state("idle");
  let updProgress: number = $state(0);
  let updError: string = $state("");

  async function checkForUpdate() {
    updStatus = "checking";
    try {
      const found = await check();
      if (found) {
        update = found;
        updStatus = "ready";
      } else {
        updStatus = "idle";
      }
    } catch {
      updStatus = "idle";
    }
  }

  async function installUpdate() {
    if (!update) return;
    updStatus = "downloading";
    updProgress = 0;
    try {
      await update.downloadAndInstall((event: any) => {
        switch (event.event) {
          case "Progress": {
            const { chunkLength, contentLength } = event.data;
            if (contentLength > 0) updProgress = Math.round((chunkLength / contentLength) * 100);
            break;
          }
          case "Finished":
            updStatus = "installing";
            break;
        }
      });
      updStatus = "done";
    } catch (e) {
      updStatus = "error";
      updError = String(e);
    }
  }

  // ---- modals ----
  function openRename() {
    if (!selectedPath) return;
    const note = notes.find((n) => n.path === selectedPath);
    renameValue = note?.title ?? selectedPath.replace(/\.md$/, "");
    renameOpen = true;
  }

  function askDelete() {
    confirmLabel = "Delete this note? (git history keeps it recoverable)";
    confirmAction = deleteNote;
    confirmOpen = true;
  }

  function newNotebookPrompt() {
    const name = window.prompt("New notebook name:");
    if (name) newNotebook(name);
  }
</script>

<div class="shell">
  <header>
    <span class="logo">私記 <b>shiki</b></span>
    <span class="theme">theme · {themeName}</span>
  </header>

  {#if loadError}
    <div class="banner banner-error" role="alert">config problem: {loadError}</div>
  {/if}

  {#if update}
    <div class="banner banner-update" role="status">
      {#if updStatus === "ready"}
        <span><b>v{update.version}</b> available</span>
        <button type="button" class="upd-btn" onclick={installUpdate}>Update</button>
      {:else if updStatus === "downloading"}
        <span><b>v{update.version}</b> — downloading… {updProgress}%</span>
        <span class="upd-progress"><span class="upd-bar" style="width:{updProgress}%"></span></span>
      {:else if updStatus === "installing"}
        <span>Installing… shiki will relaunch.</span>
      {:else if updStatus === "done"}
        <span>Updated to v{update.version} ✓</span>
      {:else if updStatus === "error"}
        <span class="upd-error">Update failed: {updError}</span>
      {/if}
    </div>
  {/if}

  {#if notebooks.length === 0 && !loadError}
    <Onboarding onCreate={newNotebook} onImport={() => (statusMsg = "CLI: shiki import obsidian <vault> — GUI import lands soon")} />
  {:else if editing}
    <EditorPane value={editorContent} title={selectedPath ?? ""} {onSave} onCancel={() => (editing = false)} />
  {:else}
    <div class="columns">
      <aside>
        <h2>NOTEBOOKS</h2>
        <ul>
          {#each notebooks as nb (nb.name)}
            <li class:selected={activeNb === nb.name} title={nb.path}>
              <button type="button" onclick={() => selectNotebook(nb.name)}>
                <span class="name">{nb.name}</span>
                {#if nb.encrypted}<span class="lock" title="encrypted">🔒</span>{/if}
              </button>
            </li>
          {/each}
        </ul>
        <button type="button" class="side-btn" onclick={newNotebookPrompt}>+ notebook</button>
      </aside>

      <section class="notes-col">
        <div class="notes-toolbar">
          <input
            type="search"
            placeholder="filter notes…"
            bind:value={searchQuery}
            title="Filter by title or tag"
          />
          <button type="button" class="tool-btn" onclick={() => (newNoteOpen = true)} title="New note">+ note</button>
          <button type="button" class="tool-btn" onclick={daily} title="Today's daily note">daily</button>
        </div>
        <ul class="notes-list">
          {#each filteredNotes as n (n.path)}
            <li
              class:selected={selectedPath === n.path}
              onclick={() => selectNote(n.path)}
              ondblclick={openEditor}
              title={n.tags.join(", ")}
            >
              <div class="note-title">{n.title}</div>
              <div class="note-meta">
                {n.date}
                {#if n.tags.length}<span class="tags">{n.tags.slice(0, 3).map((t) => `#${t}`).join(" ")}</span>{/if}
              </div>
            </li>
          {:else}
            <li class="empty">no notes{searchQuery ? ` matching "${searchQuery}"` : ""} — + note</li>
          {/each}
        </ul>
      </section>

      <main class="preview-col">
        {#if !selectedPath}
          <div class="placeholder">
            <p>Select a note to preview it.</p>
            <p class="hint">Double-click (or press Enter on selection) to edit — or just start typing after + note.</p>
          </div>
        {:else if previewError}
          <div class="placeholder">
            <p class="error-text">{previewError}</p>
          </div>
        {:else if rendered}
          <div class="preview-toolbar">
            <button type="button" class="tool-btn primary-btn" onclick={openEditor}>✎ edit</button>
            <button type="button" class="tool-btn" onclick={openRename}>rename</button>
            <button type="button" class="tool-btn danger-btn" onclick={askDelete}>delete</button>
            <span class="spacer"></span>
            {#if git?.dirty}
              <span class="dirty" title="uncommitted changes">● {git.changed} uncommitted</span>
              <button type="button" class="tool-btn" onclick={gitCommit}>commit</button>
            {/if}
          </div>
          <div class="preview scroll" bind:this={undefined}>
            <!-- eslint-disable-next-line svelte/no-at-html-tags -->
            {@html rendered.html}
          </div>
        {/if}
      </main>
    </div>
  {/if}

  <footer>
    <span>default · {defaultNotebook}</span>
    <span>{statusMsg || (activeNb ? `notebook · ${activeNb}` : "")}</span>
    <span>{git?.remote ? `remote · ${git.remote}` : ""}</span>
  </footer>

  {#if newNoteOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && (newNoteOpen = false)}>
      <div class="modal">
        <h3>New note</h3>
        <input
          placeholder="Title"
          bind:value={newNoteTitle}
          onkeydown={(e) => e.key === "Enter" && createNote()}
          autofocus
        />
        <label>Template</label>
        <select bind:value={newNoteTemplate}>
          <option value="default">default</option>
          <option value="daily">daily</option>
          <option value="meeting">meeting</option>
          <option value="bug">bug</option>
          <option value="spec">spec</option>
          <option value="review">review</option>
          <option value="postmortem">postmortem</option>
          <option value="standup">standup</option>
          <option value="retro">retro</option>
          <option value="1on1">1on1</option>
          <option value="weekly">weekly</option>
          <option value="brainstorm">brainstorm</option>
          <option value="">(empty)</option>
        </select>
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={createNote} disabled={!newNoteTitle.trim()}>Create</button>
          <button type="button" class="tool-btn" onclick={() => (newNoteOpen = false)}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  {#if renameOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && (renameOpen = false)}>
      <div class="modal">
        <h3>Rename note</h3>
        <input
          bind:value={renameValue}
          onkeydown={(e) => e.key === "Enter" && renameNote()}
          autofocus
        />
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={renameNote}>Rename (updates wikilinks)</button>
          <button type="button" class="tool-btn" onclick={() => (renameOpen = false)}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  {#if confirmOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && (confirmOpen = false)}>
      <div class="modal">
        <h3>{confirmLabel}</h3>
        <div class="modal-actions">
          <button type="button" class="danger-btn" onclick={confirmAction}>Delete</button>
          <button type="button" class="tool-btn" onclick={() => (confirmOpen = false)}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding: 0.6rem 1rem;
    border-bottom: 1px solid var(--border);
  }
  .logo b {
    color: var(--accent);
  }
  .theme,
  footer span {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .banner {
    padding: 0.5rem 1rem;
    font-size: 0.85rem;
  }
  .banner-error {
    background: var(--error);
    color: var(--bg);
  }
  .banner-update {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    background: var(--highlight);
    color: var(--fg);
    border-bottom: 1px solid var(--accent);
  }
  .banner-update b {
    color: var(--accent);
  }
  .upd-btn,
  .primary-btn {
    background: var(--accent);
    color: var(--bg);
    border: none;
    border-radius: 4px;
    padding: 0.25rem 0.9rem;
    font-weight: 600;
    cursor: pointer;
  }
  .upd-progress {
    flex: 1;
    height: 8px;
    background: var(--inactive);
    border-radius: 4px;
    overflow: hidden;
  }
  .upd-bar {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s ease;
  }
  .upd-error {
    color: var(--error);
  }
  .columns {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  aside {
    width: 200px;
    border-right: 1px solid var(--border);
    overflow-y: auto;
    padding: 0.5rem 0;
    display: flex;
    flex-direction: column;
  }
  aside h2,
  .notes-col h2 {
    font-size: 0.72rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--panel-title);
    margin: 0.4rem 0.9rem;
  }
  aside ul,
  .notes-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  aside li button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 0.4rem 0.9rem;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  aside li button:hover {
    background: var(--highlight);
  }
  aside li.selected button {
    background: var(--selection);
    border-left-color: var(--accent);
  }
  .side-btn {
    margin: auto 0.6rem 0.4rem;
    padding: 0.35rem;
    background: transparent;
    border: 1px dashed var(--border);
    border-radius: 4px;
    color: var(--muted);
    cursor: pointer;
  }
  .lock {
    font-size: 0.75rem;
  }
  .notes-col {
    width: 260px;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .notes-toolbar {
    display: flex;
    gap: 0.35rem;
    padding: 0.5rem;
    border-bottom: 1px solid var(--border);
  }
  .notes-toolbar input {
    flex: 1;
    min-width: 0;
    padding: 0.3rem 0.5rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--fg);
    outline: none;
  }
  .notes-toolbar input:focus {
    border-color: var(--accent);
  }
  .tool-btn {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--fg);
    padding: 0.3rem 0.55rem;
    font-size: 0.78rem;
    cursor: pointer;
    white-space: nowrap;
  }
  .tool-btn:hover {
    border-color: var(--accent);
  }
  .danger-btn {
    background: transparent;
    border: 1px solid var(--error);
    border-radius: 4px;
    color: var(--error);
    padding: 0.3rem 0.55rem;
    font-size: 0.78rem;
    cursor: pointer;
  }
  .notes-list {
    flex: 1;
    overflow-y: auto;
  }
  .notes-list li {
    padding: 0.45rem 0.7rem;
    cursor: pointer;
    border-left: 2px solid transparent;
  }
  .notes-list li:hover {
    background: var(--highlight);
  }
  .notes-list li.selected {
    background: var(--selection);
    border-left-color: var(--accent);
  }
  .note-title {
    font-size: 0.9rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note-meta {
    font-size: 0.72rem;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tags {
    color: var(--tag);
  }
  li.empty {
    padding: 0.7rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .preview-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .preview-toolbar {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.5rem;
    border-bottom: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .dirty {
    font-size: 0.78rem;
    color: var(--warning);
  }
  .preview {
    flex: 1;
    overflow-y: auto;
    padding: 1rem 1.4rem;
    font-size: 0.95rem;
    line-height: 1.6;
  }
  .placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--muted);
  }
  .error-text {
    color: var(--error);
  }
  .hint {
    font-size: 0.8rem;
  }
  footer {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.35rem 1rem;
    border-top: 1px solid var(--border);
    background: var(--statusbar);
    font-size: 0.78rem;
    overflow: hidden;
    white-space: nowrap;
  }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1.2rem 1.4rem;
    min-width: 320px;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .modal h3 {
    margin: 0;
    font-size: 1rem;
    color: var(--accent);
  }
  .modal input,
  .modal select {
    padding: 0.45rem 0.6rem;
    background: var(--highlight);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--fg);
    outline: none;
  }
  .modal label {
    font-size: 0.75rem;
    color: var(--muted);
  }
  .modal-actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }
</style>
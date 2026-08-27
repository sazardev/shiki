<script lang="ts">
  let { onCreate, onImport }: { onCreate: (name: string) => Promise<void>; onImport: () => void } =
    $props();

  let name = $state("");
  let busy = $state(false);
  let error = $state("");

  async function submit() {
    if (!name.trim()) return;
    busy = true;
    error = "";
    try {
      await onCreate(name.trim());
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }
</script>

<div class="onboarding">
  <div class="card">
    <div class="logo">私記</div>
    <h1>Welcome to <b>shiki</b></h1>
    <p class="tagline">
      Personal notes, private log. Your first step: create a notebook — a plain folder with
      git versioning under the hood. Everything you write here is yours, on your disk.
    </p>

    <form onsubmit={(e) => { e.preventDefault(); submit(); }}>
      <input
        bind:value={name}
        placeholder="Notebook name — e.g. personal, work, research"
        autofocus
      />
      {#if error}
        <div class="error">{error}</div>
      {/if}
      <button type="submit" class="primary" disabled={busy || !name.trim()}>
        {busy ? "Creating…" : "Create notebook"}
      </button>
    </form>

    <div class="divider"><span>or</span></div>

    <button type="button" class="secondary" onclick={onImport}>
      Import an existing folder (Obsidian vault, plain markdown…)
    </button>

    <ul class="tips">
      <li><b>Type in your notes</b> — markdown with frontmatter, tags and [[wikilinks]].</li>
      <li><b>Every notebook is a git repo</b> — full per-note history, commit and sync from the UI.</li>
      <li><b>Press <code>?</code>-style keys</b> — j/k to move, Enter to open, Ctrl+S to save.</li>
      <li>More: <code>shiki notebook create &lt;name&gt;</code> in a terminal also works.</li>
    </ul>
  </div>
</div>

<style>
  .onboarding {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
    overflow-y: auto;
  }
  .card {
    max-width: 460px;
    width: 100%;
    background: var(--bg);
    border: 1px solid var(--border);
    padding: 2rem 2.2rem;
  }
  .logo {
    font-size: 2rem;
    color: var(--accent);
    margin-bottom: 0.4rem;
  }
  h1 {
    margin: 0 0 0.6rem;
    font-size: 1.4rem;
    color: var(--fg);
  }
  h1 b {
    color: var(--accent);
  }
  .tagline {
    color: var(--muted);
    font-size: 0.9rem;
    line-height: 1.5;
    margin: 0 0 1.2rem;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  input {
    padding: 0.55rem 0.7rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--fg);
    font-size: 0.95rem;
    outline: none;
  }
  input:focus {
    border-color: var(--accent);
  }
  .primary {
    padding: 0.55rem;
    background: var(--accent);
    color: var(--bg);
    border: none;
    border-radius: 6px;
    font-weight: 700;
    font-size: 0.95rem;
    cursor: pointer;
  }
  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .secondary {
    width: 100%;
    padding: 0.5rem;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--fg);
    font-size: 0.9rem;
    cursor: pointer;
  }
  .secondary:hover {
    border-color: var(--accent);
  }
  .divider {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    color: var(--muted);
    font-size: 0.8rem;
    margin: 1rem 0;
  }
  .divider::before,
  .divider::after {
    content: "";
    flex: 1;
    border-top: 1px solid var(--border);
  }
  .error {
    color: var(--error);
    font-size: 0.85rem;
  }
  .tips {
    margin: 1.2rem 0 0;
    padding: 0 0 0 1rem;
    color: var(--muted);
    font-size: 0.82rem;
    line-height: 1.7;
  }
  .tips b {
    color: var(--fg);
  }
  code {
    background: var(--bg);
    padding: 0.05rem 0.3rem;
    border-radius: 3px;
  }
</style>
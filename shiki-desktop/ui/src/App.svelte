<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";

  interface NotebookInfo {
    name: string;
    path: string;
    encrypted: boolean;
  }

  interface UpdateInfo {
    version: string;
    body?: string;
    downloadAndInstall: (cb: (e: any) => void) => Promise<void>;
  }

  let notebooks: NotebookInfo[] = $state([]);
  let config: any = $state(null);
  let loadError: string = $state("");
  let selected: string | null = $state(null);

  // -- auto-update state --
  let update: UpdateInfo | null = $state(null);
  let updStatus: "idle" | "checking" | "ready" | "downloading" | "installing" | "done" | "error" =
    $state("idle");
  let updProgress: number = $state(0);
  let updError: string = $state("");

  const themeName = $derived(config?.theme?.name ?? "…");
  const defaultNotebook = $derived(config?.general?.default_notebook ?? "…");

  onMount(async () => {
    try {
      // Order matters for perceived startup: paint the palette first, then
      // fill the sidebar.
      const css: string = await invoke("get_theme_css");
      const style = document.createElement("style");
      style.setAttribute("data-shiki-theme", "active");
      style.textContent = css;
      document.head.appendChild(style);

      config = await invoke("get_config");
      notebooks = await invoke("list_notebooks");
    } catch (e) {
      loadError = String(e);
    }

    // Don't race the first paint: give the UI a beat before hitting the
    // update endpoint. Failures are silent — no endpoint in dev, offline,
    // or a 404 simply means "no update available" to the user.
    setTimeout(checkForUpdate, 3000);
  });

  async function checkForUpdate() {
    updStatus = "checking";
    try {
      const found = await check();
      if (found) {
        update = found as UpdateInfo;
        updStatus = "ready";
      } else {
        updStatus = "idle";
      }
    } catch (e) {
      updStatus = "idle";
    }
  }

  async function installUpdate() {
    if (!update) return;
    updStatus = "downloading";
    updProgress = 0;
    try {
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            updStatus = "downloading";
            break;
          case "Progress":
            const { chunkLength, contentLength } = event.data;
            if (contentLength > 0) {
              updProgress = Math.round((chunkLength / contentLength) * 100);
            }
            break;
          case "Finished":
            updStatus = "installing";
            break;
        }
      });
      // downloadAndInstall resolves after the installer runs (passive on
      // Windows); the app is being relaunched, so "done" rarely shows.
      updStatus = "done";
    } catch (e) {
      updStatus = "error";
      updError = String(e);
    }
  }
</script>

<div class="shell">
  <header>
    <span class="logo">私記 <b>shiki</b></span>
    <span class="theme">theme · {themeName}</span>
  </header>

  {#if loadError}
    <div class="banner banner-error" role="alert">
      config problem: {loadError}
    </div>
  {/if}

  {#if update}
    <div class="banner banner-update" role="status">
      {#if updStatus === "ready"}
        <span>
          <b>v{update.version}</b> available
        </span>
        <button type="button" class="upd-btn" onclick={installUpdate}>Update</button>
      {:else if updStatus === "downloading"}
        <span>
          <b>v{update.version}</b> — downloading… {updProgress}%
        </span>
        <span class="upd-progress">
          <span class="upd-bar" style="width:{updProgress}%"></span>
        </span>
      {:else if updStatus === "installing"}
        <span>Installing… shiki will relaunch.</span>
      {:else if updStatus === "done"}
        <span>Updated to v{update.version} ✓</span>
      {:else if updStatus === "error"}
        <span class="upd-error">Update failed: {updError}</span>
      {/if}
    </div>
  {/if}

  <div class="columns">
    <aside>
      <h2>NOTEBOOKS</h2>
      <ul>
        {#each notebooks as nb (nb.name)}
          <li
            class:selected={selected === nb.name}
            title={nb.path}
          >
            <button type="button" onclick={() => (selected = nb.name)}>
              <span class="name">{nb.name}</span>
              {#if nb.encrypted}<span class="lock" title="encrypted">🔒</span>{/if}
            </button>
          </li>
        {:else}
          <li class="empty">no notebooks yet — run<br /><code>shiki notebook create &lt;name&gt;</code></li>
        {/each}
      </ul>
    </aside>

    <main>
      {#if selected}
        <h1>{selected}</h1>
        <p class="hint">
          F0 scaffold — browsing notes lands here next (F1).
        </p>
      {:else}
        <p class="hint">Select a notebook.</p>
      {/if}
    </main>
  </div>

  <footer>
    <span>default · {defaultNotebook}</span>
    <span>F0 scaffold</span>
  </footer>
</div>

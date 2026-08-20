// Popup logic — talks to native host via background relay

function sendToHost(msg) {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage(msg, (res) => {
      if (chrome.runtime.lastError) resolve({ ok: false, error: chrome.runtime.lastError.message });
      else resolve(res);
    });
  });
}

const els = {
  status: document.getElementById("daemon-status"),
  notebook: document.getElementById("notebook"),
  folder: document.getElementById("folder"),
  folderCustom: document.getElementById("folder-custom"),
  tags: document.getElementById("tags"),
  daily: document.getElementById("daily"),
  text: document.getElementById("text"),
  capture: document.getElementById("capture"),
  result: document.getElementById("result"),
  refresh: document.getElementById("refresh"),
  hint: document.getElementById("selection-hint"),
  configPath: document.getElementById("config-path"),
  saveDefaults: document.getElementById("save-defaults"),
  quickSelection: document.getElementById("quick-selection"),
  quickPage: document.getElementById("quick-page"),
  openHelp: document.getElementById("open-help"),
};

let notebooksCache = [];

async function refreshStatus() {
  const res = await sendToHost({ action: "ping" });
  if (!res || res.ok === false) {
    els.status.textContent = "host not installed";
    els.status.className = "status err";
    showResult(false, res?.error || "Native host not reachable. Run host/install.sh --extension-id " + (chrome.runtime.id || ""));
    return;
  }
  if (res.daemon?.reachable) {
    els.status.textContent = res.daemon.enabled ? "daemon: on" : "daemon: off";
    els.status.className = res.daemon.enabled ? "status ok" : "status warn";
    els.status.title = res.daemon.enabled ? "TUI daemon is handling captures live" : "Captures will be written directly to disk";
  } else {
    els.status.textContent = "daemon: off";
    els.status.className = "status warn";
    els.status.title = "No TUI daemon — capture still works, just not live";
  }
  if (res.config?.config_path) {
    els.configPath.textContent = res.config.config_path.replace(/^.*\/shiki\//, "shiki/");
    els.configPath.title = res.config.config_path;
  }
}

async function loadNotebooks() {
  const res = await sendToHost({ action: "list_notebooks" });
  if (!res?.ok) {
    showResult(false, res?.error || "Could not list notebooks");
    return;
  }
  notebooksCache = res.notebooks || [];
  const stored = await chrome.storage.sync.get(["defaultNotebook"]);
  const defaultNb = stored.defaultNotebook || res.default_notebook || (notebooksCache[0]?.name) || "personal";

  els.notebook.innerHTML = "";
  for (const nb of notebooksCache) {
    const opt = document.createElement("option");
    opt.value = nb.name;
    opt.textContent = nb.is_encrypted ? `${nb.name} 🔒` : nb.name;
    if (nb.name === defaultNb) opt.selected = true;
    els.notebook.appendChild(opt);
  }
  if (!els.notebook.value && notebooksCache.length) els.notebook.value = notebooksCache[0].name;
  await loadFolders();
}

async function loadFolders() {
  const nb = els.notebook.value;
  if (!nb) return;
  const res = await sendToHost({ action: "list_folders", notebook: nb });
  els.folder.innerHTML = "";
  const optRoot = document.createElement("option");
  optRoot.value = "";
  optRoot.textContent = "(root)";
  els.folder.appendChild(optRoot);
  if (res?.ok && Array.isArray(res.folders)) {
    for (const f of res.folders) {
      if (!f) continue;
      const opt = document.createElement("option");
      opt.value = f;
      opt.textContent = f;
      els.folder.appendChild(opt);
    }
  }
  const stored = await chrome.storage.sync.get(["defaultFolder"]);
  if (stored.defaultFolder) {
    const exists = [...els.folder.options].some(o => o.value === stored.defaultFolder);
    if (exists) els.folder.value = stored.defaultFolder;
    else { els.folder.value = ""; els.folderCustom.value = stored.defaultFolder; }
  }
}

function showResult(ok, msg) {
  els.result.textContent = msg;
  els.result.className = ok ? "result ok" : "result err";
  els.result.classList.remove("hidden");
}

async function doCapture() {
  const text = els.text.value.trim();
  if (!text) { showResult(false, "Write something to capture"); els.text.focus(); return; }
  const notebook = els.notebook.value || undefined;
  const folderSelect = els.folder.value;
  const folderCustom = els.folderCustom.value.trim();
  const folder = folderCustom || folderSelect || undefined;
  const tags = els.tags.value.split(",").map(s => s.trim()).filter(Boolean);
  const daily = els.daily.checked;

  els.capture.disabled = true;
  els.capture.textContent = "Capturing…";
  els.result.classList.add("hidden");

  let url, title;
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    url = tab?.url;
    title = tab?.title;
  } catch {}

  const res = await sendToHost({ action: "capture", text, notebook, folder, tags, daily, url, title });
  els.capture.disabled = false;
  els.capture.textContent = "Capture";

  if (res?.ok) {
    showResult(true, `${res.via_daemon ? "captured (daemon): " : "captured: "}${res.path.split("/").slice(-2).join("/")}`);
    els.text.select();
    // Let background rebuild notebook submenus if a new notebook was created
    chrome.runtime.sendMessage({ action: "rebuildMenus" }, () => void chrome.runtime.lastError);
  } else {
    showResult(false, res?.error || "Capture failed");
  }
}

async function prefillFromTab() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (!tab?.id) return;
    chrome.tabs.sendMessage(tab.id, { action: "getSelection" }, (resp) => {
      if (chrome.runtime.lastError) return;
      const sel = resp?.text || "";
      if (sel && !els.text.value) {
        els.text.value = sel;
        els.hint.classList.remove("hidden");
      }
    });
  } catch {}
}

async function fillSelection() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (!tab?.id) return;
    chrome.tabs.sendMessage(tab.id, { action: "getSelection" }, (resp) => {
      if (chrome.runtime.lastError) {
        showResult(false, "Could not read selection — reload the page and try again");
        return;
      }
      const sel = resp?.text?.trim() || "";
      if (sel) {
        els.text.value = sel;
        els.text.focus();
        els.hint.classList.remove("hidden");
      } else {
        showResult(false, "No selection on this page");
        setTimeout(()=> els.result.classList.add("hidden"), 1500);
      }
    });
  } catch (e) {
    showResult(false, String(e.message));
  }
}

async function fillPage() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (!tab?.url) return;
    els.text.value = `${tab.title || ""}\n${tab.url}`.trim();
    els.text.focus();
    els.hint.textContent = "↳ Page info prefilled";
    els.hint.classList.remove("hidden");
  } catch (e) {
    showResult(false, String(e.message));
  }
}

// Events
els.notebook.addEventListener("change", loadFolders);
els.refresh.addEventListener("click", async () => { await refreshStatus(); await loadNotebooks(); });
els.capture.addEventListener("click", doCapture);
els.text.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === "Enter") doCapture();
});
els.saveDefaults.addEventListener("click", async () => {
  const folder = els.folderCustom.value.trim() || els.folder.value || "";
  await chrome.storage.sync.set({
    defaultNotebook: els.notebook.value || "",
    defaultFolder: folder,
    defaultTags: els.tags.value.split(",").map(s=>s.trim()).filter(Boolean),
    appendDaily: els.daily.checked
  });
  showResult(true, "Defaults saved");
  setTimeout(()=> els.result.classList.add("hidden"), 1500);
});
els.quickSelection?.addEventListener("click", fillSelection);
els.quickPage?.addEventListener("click", fillPage);
els.openHelp?.addEventListener("click", (e) => {
  e.preventDefault();
  chrome.tabs.create({ url: "https://sazardev.github.io/shiki/documentation.html" });
});

// Init
(async () => {
  const stored = await chrome.storage.sync.get(["defaultTags", "appendDaily"]);
  if (stored.defaultTags) els.tags.value = stored.defaultTags.join(", ");
  if (stored.appendDaily) els.daily.checked = !!stored.appendDaily;

  await refreshStatus();
  await loadNotebooks();
  await prefillFromTab();
  els.text.focus();
})();

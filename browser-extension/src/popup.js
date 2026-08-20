// Shiki Capture — popup prod: capture + search + recent + undo + templates/tags/folder
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
  folderSuggestions: document.getElementById("folder-suggestions"),
  createFolder: document.getElementById("create-folder"),
  template: document.getElementById("template"),
  tags: document.getElementById("tags"),
  tagsSuggestions: document.getElementById("tags-suggestions"),
  daily: document.getElementById("daily"),
  text: document.getElementById("text"),
  capture: document.getElementById("capture"),
  copyCapture: document.getElementById("copy-capture"),
  copyOnly: document.getElementById("copy-only"),
  result: document.getElementById("result"),
  refresh: document.getElementById("refresh"),
  hint: document.getElementById("selection-hint"),
  configPath: document.getElementById("config-path"),
  saveDefaults: document.getElementById("save-defaults"),
  quickSelection: document.getElementById("quick-selection"),
  quickPage: document.getElementById("quick-page"),
  openHelp: document.getElementById("open-help"),
  openOptions: document.getElementById("open-options"),
  undoBtn: document.getElementById("undo-btn"),
  encryptWarn: document.getElementById("encrypt-warn"),
  searchInput: document.getElementById("search-input"),
  searchResults: document.getElementById("search-results"),
  recentList: document.getElementById("recent-list"),
  recentRefresh: document.getElementById("recent-refresh"),
  logsList: document.getElementById("logs-list"),
  logsRefresh: document.getElementById("logs-refresh"),
  logsClear: document.getElementById("logs-clear"),
  logsExport: document.getElementById("logs-export"),
  logsCount: document.getElementById("logs-count"),
};

let notebooksCache = [];
let searchDebounce = null;

function showResult(ok, msg) {
  els.result.textContent = msg;
  els.result.className = ok ? "result ok" : "result err";
  els.result.classList.remove("hidden");
}

function switchTab(name) {
  document.querySelectorAll(".tab").forEach(t => {
    const active = t.dataset.tab === name;
    t.classList.toggle("active", active);
    t.setAttribute("aria-selected", active);
  });
  document.querySelectorAll(".tab-pane").forEach(p => {
    p.classList.toggle("active", p.id === `tab-${name}`);
  });
  if (name === "recent") loadRecent();
  if (name === "search") els.searchInput?.focus();
  if (name === "logs") loadLogs();
}

document.querySelectorAll(".tab").forEach(t => t.addEventListener("click", () => switchTab(t.dataset.tab)));

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
    els.status.title = res.daemon.enabled ? "TUI daemon live" : "Direct write fallback";
  } else {
    els.status.textContent = "daemon: off";
    els.status.className = "status warn";
    els.status.title = "No daemon — capture still works";
  }
  if (res.config?.config_path) {
    els.configPath.textContent = res.config.config_path.replace(/^.*\/shiki\//, "shiki/");
    els.configPath.title = res.config.config_path;
  }
}

async function loadNotebooks() {
  const res = await sendToHost({ action: "list_notebooks" });
  if (!res?.ok) { showResult(false, res?.error || "Could not list notebooks"); return; }
  notebooksCache = res.notebooks || [];
  const stored = await chrome.storage.sync.get(["defaultNotebook"]);
  // check per-domain rule for current tab
  let domainNotebook = null;
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (tab?.url) {
      const host = new URL(tab.url).hostname;
      const rules = (await chrome.storage.sync.get(["domainRules"])).domainRules || {};
      domainNotebook = rules[host] || rules[host.replace(/^www\./, "")] || null;
    }
  } catch {}
  const defaultNb = domainNotebook || stored.defaultNotebook || res.default_notebook || (notebooksCache[0]?.name) || "personal";
  els.notebook.innerHTML = "";
  for (const nb of notebooksCache) {
    const opt = document.createElement("option");
    opt.value = nb.name;
    opt.textContent = nb.is_encrypted ? `${nb.name} 🔒` : nb.name;
    if (nb.name === defaultNb) opt.selected = true;
    els.notebook.appendChild(opt);
  }
  if (!els.notebook.value && notebooksCache.length) els.notebook.value = notebooksCache[0].name;
  updateEncryptWarn();
  await Promise.all([loadFolders(), loadTags(), loadTemplates()]);
}

function updateEncryptWarn() {
  const nb = notebooksCache.find(n => n.name === els.notebook.value);
  if (nb?.is_encrypted) {
    els.encryptWarn?.classList.remove("hidden");
    els.capture.disabled = false; // still allow, host will error with clear msg
    els.capture.title = "Encrypted — unlock in TUI first";
  } else {
    els.encryptWarn?.classList.add("hidden");
    els.capture.title = "";
  }
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
  if (els.folderSuggestions) els.folderSuggestions.innerHTML = "";
  if (res?.ok && Array.isArray(res.folders)) {
    for (const f of res.folders) {
      if (!f) continue;
      const opt = document.createElement("option");
      opt.value = f;
      opt.textContent = f;
      els.folder.appendChild(opt);
      if (els.folderSuggestions) {
        const o = document.createElement("option");
        o.value = f;
        els.folderSuggestions.appendChild(o);
      }
    }
  }
  const stored = await chrome.storage.sync.get(["defaultFolder"]);
  if (stored.defaultFolder) {
    const exists = [...els.folder.options].some(o => o.value === stored.defaultFolder);
    if (exists) els.folder.value = stored.defaultFolder;
    else { els.folder.value = ""; els.folderCustom.value = stored.defaultFolder; }
  }
}

async function loadTags() {
  const res = await sendToHost({ action: "list_tags" });
  if (els.tagsSuggestions) els.tagsSuggestions.innerHTML = "";
  if (res?.ok && Array.isArray(res.tags) && els.tagsSuggestions) {
    for (const t of res.tags.slice(0, 50)) {
      const o = document.createElement("option");
      o.value = t;
      els.tagsSuggestions.appendChild(o);
    }
  }
}

async function loadTemplates() {
  const res = await sendToHost({ action: "list_templates" });
  if (!els.template) return;
  const cur = els.template.value;
  els.template.innerHTML = '<option value="">(default)</option>';
  if (res?.ok && Array.isArray(res.templates)) {
    for (const t of res.templates) {
      const o = document.createElement("option");
      o.value = t;
      o.textContent = t;
      els.template.appendChild(o);
    }
  }
  if (cur) els.template.value = cur;
}

async function loadRecent() {
  if (!els.recentList) return;
  els.recentList.innerHTML = '<div class="empty-state">Loading…</div>';
  const res = await sendToHost({ action: "recent", limit: 10 });
  if (!res?.ok) {
    els.recentList.innerHTML = `<div class="empty-state">Error: ${res?.error || "could not load"}</div>`;
    return;
  }
  if (!res.notes?.length) {
    els.recentList.innerHTML = '<div class="empty-state">No notes yet</div>';
    return;
  }
  els.recentList.innerHTML = "";
  els.recentList.classList.remove("empty");
  for (const n of res.notes) {
    const div = document.createElement("div");
    div.className = "item";
    div.innerHTML = `<div class="item-title">${escapeHtml(n.title)}</div><div class="item-meta">${escapeHtml(n.notebook)} · ${escapeHtml(n.relative)}${n.tags?.length ? " · " + escapeHtml(n.tags.join(", ")) : ""}</div>`;
    div.title = n.path;
    div.addEventListener("click", async () => {
      const r = await sendToHost({ action: "open_note", text: n.path });
      if (!r?.ok) showResult(false, r?.error || "Could not open");
      else showResult(true, `Opened: ${n.path.split("/").slice(-2).join("/")}`);
    });
    els.recentList.appendChild(div);
  }
}

async function doSearch(q) {
  if (!els.searchResults) return;
  if (!q.trim()) {
    els.searchResults.innerHTML = '<div class="empty-state">Type to search across all notebooks</div>';
    els.searchResults.classList.add("empty");
    return;
  }
  els.searchResults.innerHTML = '<div class="empty-state">Searching…</div>';
  const res = await sendToHost({ action: "search", query: q, limit: 8 });
  if (!res?.ok) {
    els.searchResults.innerHTML = `<div class="empty-state">Error: ${res?.error}</div>`;
    return;
  }
  if (!res.hits?.length) {
    els.searchResults.innerHTML = `<div class="empty-state">No hits for “${escapeHtml(q)}”</div>`;
    return;
  }
  els.searchResults.innerHTML = "";
  els.searchResults.classList.remove("empty");
  for (const h of res.hits) {
    const div = document.createElement("div");
    div.className = "item";
    div.innerHTML = `<div class="item-title">${escapeHtml(h.title)}</div><div class="item-meta">${escapeHtml(h.notebook)} · ${escapeHtml(h.path.split("/").pop())} · score ${h.score}</div><div class="item-preview">${escapeHtml(h.preview)}</div>`;
    div.title = h.path;
    div.addEventListener("click", async () => {
      const r = await sendToHost({ action: "open_note", text: h.path });
      if (!r?.ok) showResult(false, r?.error || "Could not open");
      else showResult(true, `Opened: ${h.path.split("/").slice(-2).join("/")}`);
    });
    els.searchResults.appendChild(div);
  }
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

async function doCapture() {
  const text = els.text.value.trim();
  if (!text) { showResult(false, "Write something to capture"); els.text.focus(); return; }
  const notebook = els.notebook.value || undefined;
  const folder = (els.folderCustom.value.trim() || els.folder.value) || undefined;
  const tags = els.tags.value.split(",").map(s => s.trim()).filter(Boolean);
  const daily = els.daily.checked;
  const template = els.template?.value || undefined;

  els.capture.disabled = true;
  els.capture.textContent = "Capturing…";
  els.result.classList.add("hidden");

  let url, title;
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    url = tab?.url; title = tab?.title;
  } catch {}

  const res = await sendToHost({ action: "capture", text, notebook, folder, tags, daily, template, url, title });
  els.capture.disabled = false;
  els.capture.textContent = "Capture";

  if (res?.ok) {
    showResult(true, `${res.via_daemon ? "captured (daemon): " : "captured: "}${res.path.split("/").slice(-2).join("/")}`);
    els.text.select();
    chrome.runtime.sendMessage({ action: "rebuildMenus" }, () => void chrome.runtime.lastError);
    loadRecent();
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
      const sel = (resp?.markdown || resp?.text || "").trim();
      if (sel && !els.text.value) {
        els.text.value = sel;
        if (els.hint) { els.hint.textContent = resp?.markdown && resp.markdown !== resp.text ? "↳ Markdown from selection" : "↳ Selection prefilled"; els.hint.classList.remove("hidden"); }
      }
    });
  } catch {}
}

async function fillSelection() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (!tab?.id) return;
    chrome.tabs.sendMessage(tab.id, { action: "getSelection" }, (resp) => {
      if (chrome.runtime.lastError) { showResult(false, "Reload the page and try again"); return; }
      const sel = resp?.text?.trim() || "";
      if (sel) { els.text.value = sel; els.text.focus(); els.hint?.classList.remove("hidden"); }
      else { showResult(false, "No selection"); setTimeout(()=> els.result.classList.add("hidden"), 1500); }
    });
  } catch (e) { showResult(false, String(e.message)); }
}

async function fillPage() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (!tab?.url) return;
    els.text.value = `${tab.title || ""}\n${tab.url}`.trim();
    els.text.focus();
    if (els.hint) { els.hint.textContent = "↳ Page info prefilled"; els.hint.classList.remove("hidden"); }
  } catch (e) { showResult(false, String(e.message)); }
}

async function copyToClipboard(text) {
  try { await navigator.clipboard.writeText(text); return true; } catch {
    try {
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      if (tab?.id) {
        const res = await new Promise(r => chrome.tabs.sendMessage(tab.id, { action: "copyText", text }, r));
        return !!res?.ok;
      }
    } catch {}
    return false;
  }
}

// Events
els.notebook?.addEventListener("change", () => { updateEncryptWarn(); loadFolders(); });
els.refresh?.addEventListener("click", async () => { await refreshStatus(); await loadNotebooks(); });
els.createFolder?.addEventListener("click", async () => {
  const nb = els.notebook.value;
  const folder = els.folderCustom.value.trim() || els.folder.value.trim();
  const name = folder || prompt("New folder name (e.g. work/meetings):");
  if (!name) return;
  const res = await sendToHost({ action: "create_folder", notebook: nb, folder: name });
  if (!res?.ok) showResult(false, res?.error || "Could not create folder");
  else { showResult(true, `Folder created: ${name}`); await loadFolders(); if (name) els.folderCustom.value = name; }
});
els.capture?.addEventListener("click", doCapture);
els.text?.addEventListener("keydown", (e) => { if ((e.ctrlKey || e.metaKey) && e.key === "Enter") doCapture(); });
els.saveDefaults?.addEventListener("click", async () => {
  const folder = els.folderCustom.value.trim() || els.folder.value || "";
  await chrome.storage.sync.set({
    defaultNotebook: els.notebook.value || "",
    defaultFolder: folder,
    defaultTags: els.tags.value.split(",").map(s=>s.trim()).filter(Boolean),
    appendDaily: els.daily.checked,
    defaultTemplate: els.template?.value || ""
  });
  showResult(true, "Defaults saved");
  setTimeout(()=> els.result.classList.add("hidden"), 1500);
});
els.quickSelection?.addEventListener("click", fillSelection);
els.quickPage?.addEventListener("click", fillPage);
els.undoBtn?.addEventListener("click", async () => {
  els.undoBtn.disabled = true;
  const res = await sendToHost({ action: "undo" });
  els.undoBtn.disabled = false;
  if (res?.ok) { showResult(true, `Undone: ${res.path.split("/").slice(-2).join("/")} ${res.via_daemon ? "(daemon)" : ""}`); loadRecent(); }
  else showResult(false, res?.error || "Nothing to undo");
});
els.searchInput?.addEventListener("input", (e) => {
  clearTimeout(searchDebounce);
  searchDebounce = setTimeout(() => doSearch(e.target.value), 250);
});
els.recentRefresh?.addEventListener("click", loadRecent);

async function loadLogs() {
  if (!els.logsList) return;
  const { logs = [] } = await chrome.storage.local.get("logs");
  if (els.logsCount) els.logsCount.textContent = logs.length ? `(${logs.length})` : "";
  if (!logs.length) {
    els.logsList.innerHTML = '<div class="empty-state">No logs yet — try a capture. Errors from right-click menu appear here.</div>';
    return;
  }
  els.logsList.innerHTML = "";
  els.logsList.classList.remove("empty");
  for (const e of logs.slice(0, 50)) {
    const div = document.createElement("div");
    div.className = "item";
    const ts = new Date(e.ts).toLocaleTimeString();
    const lvl = e.level === "error" ? "🔴" : e.level === "warn" ? "🟡" : "🟢";
    div.innerHTML = `<div class="item-meta">${lvl} ${ts} — ${escapeHtml(e.action)}</div><div class="item-title" style="font-weight:400; font-family: ui-monospace, monospace; font-size:11px; white-space:pre-wrap; word-break:break-all">${escapeHtml(e.message)}${e.data ? "\n" + escapeHtml(e.data.slice(0,200)) : ""}</div>`;
    els.logsList.appendChild(div);
  }
}
els.logsRefresh?.addEventListener("click", loadLogs);
els.logsClear?.addEventListener("click", async () => {
  await chrome.storage.local.set({ logs: [] });
  loadLogs();
  showResult(true, "Logs cleared");
});
els.logsExport?.addEventListener("click", async () => {
  const { logs = [] } = await chrome.storage.local.get("logs");
  const blob = new Blob([JSON.stringify(logs, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  chrome.downloads?.download?.({ url, filename: `shiki-logs-${new Date().toISOString().slice(0,10)}.json` }, () => {
    if (chrome.runtime.lastError) {
      // fallback: copy to clipboard
      navigator.clipboard.writeText(JSON.stringify(logs, null, 2)).then(()=> showResult(true,"Logs copied to clipboard"), ()=> showResult(false, "Export failed"));
    } else showResult(true, "Logs exported");
  }) || navigator.clipboard.writeText(JSON.stringify(logs, null, 2)).then(()=> showResult(true,"Logs copied"), ()=>{});
});
document.getElementById("copy-only")?.addEventListener("click", async () => {
  const text = els.text.value.trim();
  if (!text) { showResult(false, "Nothing to copy"); return; }
  const ok = await copyToClipboard(text);
  showResult(ok, ok ? `Copied ${text.length} chars` : "Copy failed");
  setTimeout(()=> ok && els.result.classList.add("hidden"), 1200);
});
document.getElementById("copy-capture")?.addEventListener("click", async () => {
  const text = els.text.value.trim();
  if (!text) { showResult(false, "Write something to copy+send"); return; }
  const copied = await copyToClipboard(text);
  showResult(copied, copied ? "Copied — now capturing…" : "Capture without copy…");
  await doCapture();
});
els.openHelp?.addEventListener("click", (e) => { e.preventDefault(); chrome.tabs.create({ url: "https://sazardev.github.io/shiki/documentation.html" }); });
els.openOptions?.addEventListener("click", (e) => { e.preventDefault(); chrome.runtime.openOptionsPage(); });

// Init + flush offline queue
(async () => {
  const stored = await chrome.storage.sync.get(["defaultTags", "appendDaily", "defaultTemplate"]);
  if (stored.defaultTags) els.tags.value = stored.defaultTags.join(", ");
  if (stored.appendDaily) els.daily.checked = !!stored.appendDaily;
  if (stored.defaultTemplate && els.template) els.template.value = stored.defaultTemplate;
  // flush any queued offline captures
  chrome.runtime.sendMessage({ action: "flushOffline" }, () => void chrome.runtime.lastError);
  await refreshStatus();
  await loadNotebooks();
  await prefillFromTab();
  loadRecent();
  els.text?.focus();
  // show offline queue count if any
  chrome.storage.local.get("offlineQueue", ({ offlineQueue }) => {
    if (offlineQueue?.length) showResult(false, `Offline queue: ${offlineQueue.length} pending — will retry`);
  });
})();

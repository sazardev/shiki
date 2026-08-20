function sendToHost(msg) {
  return new Promise(r => chrome.runtime.sendMessage(msg, res => r(chrome.runtime.lastError ? { ok:false, error: chrome.runtime.lastError.message } : res)));
}
const els = {
  defaultNotebook: document.getElementById("defaultNotebook"),
  defaultFolder: document.getElementById("defaultFolder"),
  defaultTags: document.getElementById("defaultTags"),
  defaultTemplate: document.getElementById("defaultTemplate"),
  appendDaily: document.getElementById("appendDaily"),
  showNotifications: document.getElementById("showNotifications"),
  rules: document.getElementById("rules"),
  addRule: document.getElementById("addRule"),
  save: document.getElementById("save"),
  reset: document.getElementById("reset"),
  result: document.getElementById("result"),
  status: document.getElementById("status"),
  testHost: document.getElementById("testHost"),
  testResult: document.getElementById("testResult"),
  foldersOpts: document.getElementById("foldersOpts"),
  tagsOpts: document.getElementById("tagsOpts"),
};
let notebooks = [];

async function loadNotebooks() {
  const res = await sendToHost({ action: "list_notebooks" });
  notebooks = res?.ok ? res.notebooks : [];
  els.defaultNotebook.innerHTML = "";
  for (const nb of notebooks) {
    const o = document.createElement("option");
    o.value = nb.name; o.textContent = nb.is_encrypted ? `${nb.name} 🔒` : nb.name;
    els.defaultNotebook.appendChild(o);
  }
  // also fill per-rule selects
}

async function loadFoldersFor(nbName) {
  if (!nbName) return [];
  const res = await sendToHost({ action: "list_folders", notebook: nbName });
  return res?.ok ? res.folders.filter(Boolean) : [];
}

async function loadTags() {
  const res = await sendToHost({ action: "list_tags" });
  if (els.tagsOpts) {
    els.tagsOpts.innerHTML = "";
    for (const t of (res?.tags || []).slice(0,50)) {
      const o = document.createElement("option"); o.value = t; els.tagsOpts.appendChild(o);
    }
  }
}

async function loadTemplates() {
  const res = await sendToHost({ action: "list_templates" });
  const cur = els.defaultTemplate.value;
  els.defaultTemplate.innerHTML = '<option value="">(default)</option>';
  for (const t of (res?.templates || [])) {
    const o = document.createElement("option"); o.value = t; o.textContent = t; els.defaultTemplate.appendChild(o);
  }
  if (cur) els.defaultTemplate.value = cur;
}

function addRuleRow(domain="", notebook="") {
  const row = document.createElement("div");
  row.className = "rule-row";
  const input = document.createElement("input");
  input.placeholder = "example.com";
  input.value = domain;
  input.className = "rule-domain";
  const sel = document.createElement("select");
  sel.className = "rule-notebook";
  const btn = document.createElement("button");
  btn.className = "icon-btn";
  btn.title = "Remove";
  btn.textContent = "✕";
  row.appendChild(input);
  row.appendChild(sel);
  row.appendChild(btn);
  for (const nb of notebooks) {
    const o = document.createElement("option"); o.value = nb.name; o.textContent = nb.name; if (nb.name===notebook) o.selected=true; sel.appendChild(o);
  }
  btn.addEventListener("click", () => row.remove());
  // update folders datalist when notebook changes
  sel.addEventListener("change", async () => {
    const folders = await loadFoldersFor(sel.value);
    if (els.foldersOpts) {
      els.foldersOpts.innerHTML = "";
      for (const f of folders) { const o=document.createElement("option"); o.value=f; els.foldersOpts.appendChild(o); }
    }
  });
  els.rules.appendChild(row);
}

async function loadAll() {
  await loadNotebooks();
  await Promise.all([loadTags(), loadTemplates()]);
  const stored = await chrome.storage.sync.get(["defaultNotebook","defaultFolder","defaultTags","defaultTemplate","appendDaily","showNotifications","domainRules","customHostPath"]);
  if (stored.defaultNotebook) els.defaultNotebook.value = stored.defaultNotebook;
  if (stored.defaultFolder) els.defaultFolder.value = stored.defaultFolder;
  if (stored.defaultTags) els.defaultTags.value = stored.defaultTags.join(", ");
  if (stored.defaultTemplate) els.defaultTemplate.value = stored.defaultTemplate;
  if (stored.appendDaily) els.appendDaily.checked = !!stored.appendDaily;
  if (stored.showNotifications === false) els.showNotifications.checked = false;
  if (stored.customHostPath) document.getElementById("customHostPath").value = stored.customHostPath;
  els.rules.innerHTML = "";
  const rules = stored.domainRules || {};
  for (const [domain, nb] of Object.entries(rules)) addRuleRow(domain, nb);
  if (!Object.keys(rules).length) addRuleRow("", notebooks[0]?.name || "");

  // folders datalist for current default notebook
  const folders = await loadFoldersFor(els.defaultNotebook.value);
  if (els.foldersOpts) {
    els.foldersOpts.innerHTML = "";
    for (const f of folders) { const o=document.createElement("option"); o.value=f; els.foldersOpts.appendChild(o); }
  }
}

els.defaultNotebook.addEventListener("change", async () => {
  const folders = await loadFoldersFor(els.defaultNotebook.value);
  if (els.foldersOpts) {
    els.foldersOpts.innerHTML = "";
    for (const f of folders) { const o=document.createElement("option"); o.value=f; els.foldersOpts.appendChild(o); }
  }
});

els.addRule.addEventListener("click", () => addRuleRow("", els.defaultNotebook.value));

els.save.addEventListener("click", async () => {
  const domainRules = {};
  for (const row of els.rules.querySelectorAll(".rule-row")) {
    const d = row.querySelector(".rule-domain").value.trim().toLowerCase();
    const nb = row.querySelector(".rule-notebook").value;
    if (d && nb) domainRules[d] = nb;
  }
  await chrome.storage.sync.set({
    defaultNotebook: els.defaultNotebook.value || "",
    defaultFolder: els.defaultFolder.value.trim(),
    defaultTags: els.defaultTags.value.split(",").map(s=>s.trim()).filter(Boolean),
    defaultTemplate: els.defaultTemplate.value || "",
    appendDaily: !!els.appendDaily.checked,
    showNotifications: !!els.showNotifications.checked,
    domainRules,
    customHostPath: document.getElementById("customHostPath").value.trim()
  });
  els.result.textContent = "Options saved";
  els.result.className = "result ok";
  els.result.classList.remove("hidden");
  els.status.textContent = "saved ✓";
  setTimeout(()=> els.result.classList.add("hidden"), 1500);
  // rebuild context menus in background
  chrome.runtime.sendMessage({ action: "rebuildMenus" }, () => void chrome.runtime.lastError);
});

els.reset.addEventListener("click", async () => {
  if (!confirm("Reset all options to defaults?")) return;
  await chrome.storage.sync.clear();
  await loadAll();
  els.result.textContent = "Reset done";
  els.result.className = "result ok";
  els.result.classList.remove("hidden");
});

els.testHost.addEventListener("click", async () => {
  els.testResult.textContent = "Testing…";
  const res = await sendToHost({ action: "ping" });
  if (res?.ok) els.testResult.textContent = `OK — daemon ${res.daemon.reachable ? (res.daemon.enabled ? "on" : "off") : "not running"}, config ${res.config?.config_path || ""}`;
  else els.testResult.textContent = `Failed: ${res?.error || "no host"}`;
});

loadAll();

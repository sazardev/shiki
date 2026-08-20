// Shiki Capture — background service worker
// Handles context menus, keyboard shortcut, and relays to native host.

const NATIVE_HOST = "com.shiki.native";
const LOG_LIMIT = 200;
async function addLog(level, action, message, data=null) {
  const entry = { ts: Date.now(), level, action, message: String(message).slice(0,500), data: data?JSON.stringify(data).slice(0,1000):null };
  try {
    const { logs=[] } = await chrome.storage.local.get("logs");
    logs.unshift(entry);
    if (logs.length>LOG_LIMIT) logs.length=LOG_LIMIT;
    await chrome.storage.local.set({ logs });
  } catch {}
  const fn = level==="error"?console.error:level==="warn"?console.warn:console.log;
  fn(`[shiki:${action}] ${message}`, data||"");
}

async function sendToHost(msg) {
  return new Promise((resolve, reject) => {
    if (typeof chrome.runtime.sendNativeMessage !== "function") {
      reject(new Error("chrome.runtime.sendNativeMessage is not available — missing 'nativeMessaging' permission? Check manifest.json and reload extension. Manifest permissions: " + JSON.stringify(chrome.runtime.getManifest().permissions)));
      return;
    }
    try {
      chrome.runtime.sendNativeMessage(NATIVE_HOST, msg, (response) => {
        if (chrome.runtime.lastError) {
          const err = new Error(chrome.runtime.lastError.message);
          addLog("error", msg.action, err.message, msg);
          reject(err);
        } else if (response === undefined) {
          const err = new Error("Native host not found or not allowed for this extension ID. Run: ./host/install.sh --extension-id " + chrome.runtime.id + "  (current ID: " + chrome.runtime.id + ")");
          addLog("error", msg.action, err.message, msg);
          reject(err);
        } else {
          if (response?.ok===false) addLog("warn", msg.action, response.error||"host error", msg);
          else addLog("info", msg.action, "ok", { action: msg.action });
          resolve(response);
        }
      });
    } catch (e) {
      addLog("error", msg.action, String(e.message||e), msg);
      reject(e);
    }
  });
}

// ── Context menus ───────────────────────────────────────────────────────────
// Fast path: top-level single-click for selection/page (no submenu hover)
// + organized parent "Shiki" for the rest + dynamic notebook submenus

async function rebuildContextMenus() {
  await chrome.contextMenus.removeAll();

  // Top-level fast actions (one click, no submenu)
  chrome.contextMenus.create({
    id: "shiki-capture-selection",
    title: "✦ Send to Shiki",
    contexts: ["selection"]
  });
  chrome.contextMenus.create({
    id: "shiki-capture-page",
    title: "⎙ Save page to Shiki",
    contexts: ["page"]
  });

  // Parent for everything else
  chrome.contextMenus.create({
    id: "shiki-parent",
    title: "Shiki",
    contexts: ["all"]
  });
  chrome.contextMenus.create({
    id: "shiki-quick-note",
    parentId: "shiki-parent",
    title: "Quick note…",
    contexts: ["all"]
  });
  chrome.contextMenus.create({
    id: "shiki-capture-selection-parent",
    parentId: "shiki-parent",
    title: "Send selection",
    contexts: ["selection"]
  });
  chrome.contextMenus.create({
    id: "shiki-capture-link",
    parentId: "shiki-parent",
    title: "Save link",
    contexts: ["link"]
  });
  chrome.contextMenus.create({
    id: "shiki-capture-image",
    parentId: "shiki-parent",
    title: "Save image",
    contexts: ["image"]
  });
  chrome.contextMenus.create({
    id: "shiki-copy-send",
    parentId: "shiki-parent",
    title: "Copy + Send to Shiki",
    contexts: ["selection"]
  });
  chrome.contextMenus.create({
    id: "shiki-capture-daily",
    parentId: "shiki-parent",
    title: "Append to daily note",
    contexts: ["selection", "page", "link"]
  });
  chrome.contextMenus.create({
    id: "shiki-sep",
    parentId: "shiki-parent",
    type: "separator",
    contexts: ["all"]
  });

  // Dynamic: Send to notebook → (list from host) — reliable with retry + logs
  let notebooksForMenu = [];
  try {
    const res = await sendToHost({ action: "list_notebooks" });
    if (res?.ok && Array.isArray(res.notebooks) && res.notebooks.length) {
      notebooksForMenu = res.notebooks;
      addLog("info", "rebuildMenus", `built ${res.notebooks.length} notebook submenus`, null);
    } else {
      addLog("warn", "rebuildMenus", `list_notebooks empty or not ok: ${res?.error||"empty"}`, res);
    }
  } catch (e) {
    addLog("error", "rebuildMenus", `could not build notebook submenus: ${e.message}`, null);
    console.warn("[shiki] could not build notebook submenus", e.message);
  }
  // Always create parent, even if empty, with placeholder to indicate status
  chrome.contextMenus.create({
    id: "shiki-to-notebook-parent",
    parentId: "shiki-parent",
    title: notebooksForMenu.length ? "Send to notebook" : "Send to notebook (no host)",
    contexts: ["selection", "page", "link", "image"]
  });
  if (notebooksForMenu.length) {
    for (const nb of notebooksForMenu) {
      const label = nb.is_encrypted ? `${nb.name} 🔒` : nb.name;
      chrome.contextMenus.create({
        id: `shiki-to-nb::${nb.name}`,
        parentId: "shiki-to-notebook-parent",
        title: label,
        contexts: ["selection", "page", "link", "image"]
      });
    }
  } else {
    chrome.contextMenus.create({
      id: "shiki-no-notebooks",
      parentId: "shiki-to-notebook-parent",
      title: "No notebooks — check host/logs",
      enabled: false,
      contexts: ["selection", "page", "link", "image"]
    });
  }

  // Save page extras: bookmark vs article
  chrome.contextMenus.create({
    id: "shiki-save-bookmark",
    parentId: "shiki-parent",
    title: "Save bookmark",
    contexts: ["page", "link"]
  });
  chrome.contextMenus.create({
    id: "shiki-save-article",
    parentId: "shiki-parent",
    title: "Save article (Reader)",
    contexts: ["page"]
  });

  chrome.contextMenus.create({
    id: "shiki-settings",
    parentId: "shiki-parent",
    title: "Settings (popup)",
    contexts: ["all"]
  });
}

chrome.runtime.onInstalled.addListener(() => {
  rebuildContextMenus();
  flushOffline().catch(()=>{});
});
chrome.runtime.onStartup.addListener(() => {
  rebuildContextMenus();
  flushOffline().catch(()=>{});
});

// Rebuild after a successful capture (notebook may have been created) and periodically
chrome.storage.onChanged.addListener(() => {
  // Debounce — user changed defaults, not notebooks, but cheap to rebuild
});

async function doCapture({ text, url, title, notebook, daily, template }) {
  const stored = await chrome.storage.sync.get(["defaultNotebook", "defaultFolder", "defaultTags", "appendDaily", "defaultTemplate"]);
  const res = await sendToHost({
    action: "capture",
    text,
    url,
    title,
    notebook: notebook ?? stored.defaultNotebook ?? undefined,
    folder: stored.defaultFolder ?? undefined,
    tags: stored.defaultTags ?? undefined,
    daily: daily ?? stored.appendDaily ?? false,
    template: template ?? stored.defaultTemplate ?? undefined
  });
  return res;
}

// Offline queue — if host unreachable, store and retry on next popup open / startup
async function queueOffline(entry) {
  const { offlineQueue = [] } = await chrome.storage.local.get("offlineQueue");
  offlineQueue.push({ ...entry, ts: Date.now() });
  await chrome.storage.local.set({ offlineQueue });
  chrome.action.setBadgeText({ text: "!" });
  chrome.action.setBadgeBackgroundColor({ color: "#b91c1c" });
}
async function flushOffline() {
  const { offlineQueue = [] } = await chrome.storage.local.get("offlineQueue");
  if (!offlineQueue.length) return;
  const remaining = [];
  for (const e of offlineQueue) {
    try {
      const r = await sendToHost({ action: "capture", ...e });
      if (!r?.ok) remaining.push(e);
    } catch { remaining.push(e); }
  }
  await chrome.storage.local.set({ offlineQueue: remaining });
  if (!remaining.length) { chrome.action.setBadgeText({ text: "" }); }
}

function notifyCaptured(res) {
  const buttons = [{ title: "Undo" }, { title: "Open" }];
  chrome.notifications?.create?.({
    type: "basic",
    iconUrl: "icons/icon128.png",
    title: "Shiki — captured",
    message: `${res.via_daemon ? "(daemon) " : ""}${res.path.split("/").slice(-2).join("/")}`,
    buttons
  });
  chrome.action.setBadgeText({ text: "✓" });
  chrome.action.setBadgeBackgroundColor({ color: "#1c1917" });
  setTimeout(() => chrome.action.setBadgeText({ text: "" }), 1800);
  // store last for notification click
  chrome.storage.local.set({ lastCapturedPath: res.path });
}
chrome.notifications?.onButtonClicked?.addListener(async (id, btnIdx) => {
  const { lastCapturedPath } = await chrome.storage.local.get("lastCapturedPath");
  if (btnIdx === 0) {
    // Undo
    try { const r = await sendToHost({ action: "undo" }); chrome.notifications.create({ type:"basic", iconUrl:"icons/icon128.png", title:"Shiki — undone", message: r.path || "undone" }); } catch(e){ chrome.notifications.create({ type:"basic", iconUrl:"icons/icon128.png", title:"Undo failed", message: String(e.message).slice(0,100)}); }
  } else if (btnIdx === 1 && lastCapturedPath) {
    sendToHost({ action: "open_note", text: lastCapturedPath }).catch(()=>{});
  }
});
chrome.notifications?.onClicked?.addListener(async () => {
  const { lastCapturedPath } = await chrome.storage.local.get("lastCapturedPath");
  if (lastCapturedPath) sendToHost({ action: "open_note", text: lastCapturedPath }).catch(()=>{});
});

// Omnibox: type "shiki <query>" to search
if (chrome.omnibox) {
  chrome.omnibox.onInputChanged.addListener(async (text, suggest) => {
    if (!text.trim()) return;
    try {
      const res = await sendToHost({ action: "search", query: text, limit: 5 });
      if (res?.ok && res.hits?.length) {
        suggest(res.hits.map(h => ({ content: h.path, description: `${h.title} — ${h.notebook}/${h.path.split("/").pop()} ` })));
      }
    } catch {}
  });
  chrome.omnibox.onInputEntered.addListener(async (text, disposition) => {
    if (!text.trim()) return;
    // First hit or open search in popup? For now open the note
    try {
      const res = await sendToHost({ action: "search", query: text, limit: 1 });
      if (res?.ok && res.hits?.length) {
        await sendToHost({ action: "open_note", text: res.hits[0].path });
      } else {
        // No hit — capture the query itself as a quick note
        await doCapture({ text, title: "Omnibox capture", url: "" });
        notifyCaptured({ path: text.slice(0,40), via_daemon:false });
      }
    } catch(e){ console.error(e); }
  });
}

// (flush already handled in onInstalled/onStartup above)

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  const url = tab?.url || info.pageUrl || "";
  const title = tab?.title || "";
  let text = "";
  let explicitNotebook = null;
  let daily = null;
  let doCopy = false;

  // Dynamic notebook submenu
  if (info.menuItemId.startsWith("shiki-to-nb::")) {
    explicitNotebook = info.menuItemId.replace("shiki-to-nb::", "");
    text = info.selectionText || `${title}\n${url}`;
    if (info.linkUrl) {
      text = info.linkUrl;
      // keep link url as source
    }
  } else if (info.menuItemId === "shiki-copy-send") {
    text = info.selectionText || "";
    doCopy = true;
  } else if (info.menuItemId === "shiki-capture-selection" || info.menuItemId === "shiki-capture-selection-parent") {
    text = info.selectionText || "";
  } else if (info.menuItemId === "shiki-capture-link") {
    text = info.linkUrl || info.selectionText || "";
  } else if (info.menuItemId === "shiki-capture-image") {
    const src = info.srcUrl || "";
    const alt = info.selectionText || "image";
    text = src ? `![${alt}](${src})\n\nSource: ${url}` : (info.selectionText || `${title}\n${url}`);
  } else if (info.menuItemId === "shiki-save-bookmark") {
    const link = info.linkUrl || url;
    const linkTitle = info.linkUrl ? (info.selectionText || link) : title;
    text = `# ${linkTitle}\n\n🔖 Bookmark: [${linkTitle}](${link})\n\nSource: ${url}\nTags: bookmark`;
    // will add bookmark tag via doCapture tags
    // Do immediate capture with bookmark tag
    try {
      const res = await doCapture({ text, url: link, title: linkTitle, notebook: explicitNotebook, daily });
      // add bookmark tag manually if not present? doCapture already handles tags from defaults, but we want to ensure bookmark
      if (res?.ok) { notifyCaptured(res); rebuildContextMenus().catch(()=>{}); }
      else throw new Error(res?.error||"unknown");
      return;
    } catch(e){ console.error(e); addLog("error","bookmark",e.message,null); }
    return;
  } else if (info.menuItemId === "shiki-save-article") {
    // Extract article via content script (Reader mode)
    if (tab?.id) {
      try {
        const article = await new Promise((resolve) => chrome.tabs.sendMessage(tab.id, { action: "extractArticle" }, (r) => resolve(r)));
        if (article?.text) {
          text = `# ${article.title || title}\n\n${article.text}\n\nSource: ${url}\n`;
          if (article.excerpt) text += `\n> ${article.excerpt}\n`;
          const res = await doCapture({ text, url, title: article.title || title, notebook: explicitNotebook, daily });
          if (res?.ok) { notifyCaptured(res); rebuildContextMenus().catch(()=>{}); }
          else throw new Error(res?.error||"unknown");
          return;
        } else {
          text = `${title}\n${url}\n\n(Article extraction failed, saved as bookmark)`;
        }
      } catch(e){ addLog("error","article",String(e.message),null); text = `${title}\n${url}\n\n(Extraction error: ${e.message})`; }
    } else {
      text = `${title}\n${url}`;
    }
  } else if (info.menuItemId === "shiki-capture-page") {
    text = info.selectionText ? info.selectionText : `${title}\n${url}`;
  } else if (info.menuItemId === "shiki-capture-daily") {
    text = info.selectionText || `${title}\n${url}`;
    daily = true;
  } else if (info.menuItemId === "shiki-quick-note" || info.menuItemId === "shiki-settings") {
    // Open popup — requires user gesture, contextMenus click qualifies
    try { await chrome.action.openPopup(); } catch {}
    return;
  } else if (info.menuItemId === "shiki-parent") {
    return;
  } else {
    // Fallback: treat as page capture
    text = info.selectionText || `${title}\n${url}`;
  }

  if (!text) text = `${title}\n${url}`;
  // Enrich link case with title
  if (info.linkUrl && !text.includes(info.linkUrl)) {
    text = `${text}\n\nSource: [${title}](${url})`;
  }

  // Ejemplo copy y mandar: si es copy-send, copia al portapapeles primero
  if (doCopy && tab?.id) {
    try {
      await new Promise((resolve) => chrome.tabs.sendMessage(tab.id, { action: "copyText", text }, () => resolve()));
    } catch (e) {
      console.warn("[shiki] copy failed", e);
    }
  }

  try {
    const res = await doCapture({ text, url: info.linkUrl || info.srcUrl || url, title, notebook: explicitNotebook, daily });
    if (res?.ok) {
      notifyCaptured(res);
      rebuildContextMenus().catch(()=>{});
    } else {
      throw new Error(res?.error || "unknown error");
    }
  } catch (e) {
    console.error("[shiki] capture failed", e);
    // Offline queue: keep for retry if host unreachable
    const msg = String(e.message);
    if (msg.includes("Native host not found") || msg.includes("not reachable") || msg.includes("Failed to connect")) {
      await queueOffline({ text, url: info.linkUrl || info.srcUrl || url, title, notebook: explicitNotebook, daily });
      chrome.notifications?.create?.({
        type: "basic",
        iconUrl: "icons/icon128.png",
        title: "Shiki — queued offline",
        message: "Will retry when host is back"
      });
    } else {
      chrome.notifications?.create?.({
        type: "basic",
        iconUrl: "icons/icon128.png",
        title: "Shiki — capture failed",
        message: msg.slice(0, 120)
      });
    }
  }
});

// Keyboard shortcut quick capture
chrome.commands.onCommand.addListener(async (command) => {
  if (command === "quick-capture-selection") {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (!tab?.id) return;
    chrome.tabs.sendMessage(tab.id, { action: "getSelection" }, async (resp) => {
      if (chrome.runtime.lastError) {
        console.warn(chrome.runtime.lastError.message);
        return;
      }
      const text = resp?.markdown || resp?.text || "";
      const url = tab.url || "";
      const title = tab.title || "";
      if (!text && !url) return;
      const finalText = text || `${title}\n${url}`;
      try {
        const res = await doCapture({ text: finalText, url, title });
        if (res?.ok) {
          chrome.action.setBadgeText({ text: "✓" });
          setTimeout(() => chrome.action.setBadgeText({ text: "" }), 1500);
          notifyCaptured(res);
        } else { throw new Error(res?.error || "capture failed"); }
      } catch (e) {
        console.error(e);
        const msg = String(e.message);
        if (msg.includes("Native host")) await queueOffline({ text: finalText, url, title });
      }
    });
  }
});

// Relay messages from popup/content + allow popup to trigger menu rebuild — with allowlist
const ALLOWED_ACTIONS = new Set(["ping","check_daemon","list_notebooks","list_folders","list_tags","list_templates","search","recent","create_folder","capture","undo","open_note","rebuildMenus","flushOffline"]);
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  // Only allow messages from this extension (popup/options/content of same extension)
  if (sender.id && sender.id !== chrome.runtime.id) {
    sendResponse({ ok: false, error: "blocked: external sender" });
    return true;
  }
  if (!msg || typeof msg.action !== "string" || !ALLOWED_ACTIONS.has(msg.action)) {
    sendResponse({ ok: false, error: `blocked: unknown action ${msg?.action}` });
    return true;
  }
  if (msg?.action === "rebuildMenus") {
    rebuildContextMenus().then(() => sendResponse({ ok: true })).catch(e => sendResponse({ ok: false, error: e.message }));
    return true;
  }
  if (msg?.action === "flushOffline") {
    flushOffline().then(() => sendResponse({ ok: true })).catch(e => sendResponse({ ok: false, error: e.message }));
    return true;
  }
  (async () => {
    try {
      const res = await sendToHost(msg);
      sendResponse(res);
    } catch (e) {
      sendResponse({ ok: false, error: e.message || String(e) });
    }
  })();
  return true;
});

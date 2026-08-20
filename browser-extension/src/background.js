// Shiki Capture — background service worker
// Handles context menus, keyboard shortcut, and relays to native host.

const NATIVE_HOST = "com.shiki.native";

async function sendToHost(msg) {
  return new Promise((resolve, reject) => {
    if (typeof chrome.runtime.sendNativeMessage !== "function") {
      reject(new Error("chrome.runtime.sendNativeMessage is not available — missing 'nativeMessaging' permission? Check manifest.json and reload extension. Manifest permissions: " + JSON.stringify(chrome.runtime.getManifest().permissions)));
      return;
    }
    try {
      chrome.runtime.sendNativeMessage(NATIVE_HOST, msg, (response) => {
        if (chrome.runtime.lastError) {
          reject(new Error(chrome.runtime.lastError.message));
        } else if (response === undefined) {
          reject(new Error("Native host not found or not allowed for this extension ID. Run: ./host/install.sh --extension-id " + chrome.runtime.id + "  (current ID: " + chrome.runtime.id + ")"));
        } else {
          resolve(response);
        }
      });
    } catch (e) {
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

  // Dynamic: Send to notebook → (list from host)
  try {
    const res = await sendToHost({ action: "list_notebooks" });
    if (res?.ok && Array.isArray(res.notebooks) && res.notebooks.length) {
      chrome.contextMenus.create({
        id: "shiki-to-notebook-parent",
        parentId: "shiki-parent",
        title: "Send to notebook",
        contexts: ["selection", "page", "link"]
      });
      for (const nb of res.notebooks) {
        const label = nb.is_encrypted ? `${nb.name} 🔒` : nb.name;
        chrome.contextMenus.create({
          id: `shiki-to-nb::${nb.name}`,
          parentId: "shiki-to-notebook-parent",
          title: label,
          contexts: ["selection", "page", "link"]
        });
      }
    }
  } catch (e) {
    // Host not ready yet — keep static menus, will retry on next click via fallback
    console.warn("[shiki] could not build notebook submenus", e.message);
  }

  chrome.contextMenus.create({
    id: "shiki-settings",
    parentId: "shiki-parent",
    title: "Settings (popup)",
    contexts: ["all"]
  });
}

chrome.runtime.onInstalled.addListener(() => {
  rebuildContextMenus();
});
chrome.runtime.onStartup.addListener(() => {
  rebuildContextMenus();
});

// Rebuild after a successful capture (notebook may have been created) and periodically
chrome.storage.onChanged.addListener(() => {
  // Debounce — user changed defaults, not notebooks, but cheap to rebuild
});

async function doCapture({ text, url, title, notebook, daily }) {
  const stored = await chrome.storage.sync.get(["defaultNotebook", "defaultFolder", "defaultTags", "appendDaily"]);
  const res = await sendToHost({
    action: "capture",
    text,
    url,
    title,
    notebook: notebook ?? stored.defaultNotebook ?? undefined,
    folder: stored.defaultFolder ?? undefined,
    tags: stored.defaultTags ?? undefined,
    daily: daily ?? stored.appendDaily ?? false
  });
  return res;
}

function notifyCaptured(res) {
  chrome.notifications?.create?.({
    type: "basic",
    iconUrl: "icons/icon128.png",
    title: "Shiki — captured",
    message: `${res.via_daemon ? "(daemon) " : ""}${res.path.split("/").slice(-2).join("/")}`
  });
  chrome.action.setBadgeText({ text: "✓" });
  chrome.action.setBadgeBackgroundColor({ color: "#1c1917" });
  setTimeout(() => chrome.action.setBadgeText({ text: "" }), 1800);
}

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
    const res = await doCapture({ text, url: info.linkUrl || url, title, notebook: explicitNotebook, daily });
    if (res?.ok) {
      notifyCaptured(res);
      // Rebuild menus so newly created notebooks appear next time
      rebuildContextMenus().catch(()=>{});
    } else {
      throw new Error(res?.error || "unknown error");
    }
  } catch (e) {
    console.error("[shiki] capture failed", e);
    chrome.notifications?.create?.({
      type: "basic",
      iconUrl: "icons/icon128.png",
      title: "Shiki — capture failed",
      message: String(e.message).slice(0, 120)
    });
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
      const text = resp?.text || "";
      const url = tab.url || "";
      const title = tab.title || "";
      if (!text && !url) return;
      const finalText = text || `${title}\n${url}`;
      try {
        const res = await doCapture({ text: finalText, url, title });
        if (res?.ok) {
          chrome.action.setBadgeText({ text: "✓" });
          setTimeout(() => chrome.action.setBadgeText({ text: "" }), 1500);
        }
      } catch (e) {
        console.error(e);
      }
    });
  }
});

// Relay messages from popup/content + allow popup to trigger menu rebuild
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg?.action === "rebuildMenus") {
    rebuildContextMenus().then(() => sendResponse({ ok: true })).catch(e => sendResponse({ ok: false, error: e.message }));
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

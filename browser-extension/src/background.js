// Shiki Capture — background service worker
// Handles context menu, keyboard shortcut, and relays to native host.

const NATIVE_HOST = "com.shiki.native";

async function sendToHost(msg) {
  return new Promise((resolve, reject) => {
    try {
      chrome.runtime.sendNativeMessage(NATIVE_HOST, msg, (response) => {
        if (chrome.runtime.lastError) {
          reject(new Error(chrome.runtime.lastError.message));
        } else {
          resolve(response);
        }
      });
    } catch (e) {
      reject(e);
    }
  });
}

// Context menu setup
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "shiki-capture-selection",
    title: "Capture selection to Shiki",
    contexts: ["selection"]
  });
  chrome.contextMenus.create({
    id: "shiki-capture-page",
    title: "Capture page to Shiki",
    contexts: ["page", "link"]
  });
  chrome.contextMenus.create({
    id: "shiki-capture-link",
    title: "Capture link to Shiki",
    contexts: ["link"]
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  let text = "";
  let url = tab?.url || info.pageUrl || "";
  let title = tab?.title || "";

  if (info.menuItemId === "shiki-capture-selection") {
    text = info.selectionText || "";
  } else if (info.menuItemId === "shiki-capture-link") {
    text = info.linkUrl || info.selectionText || "";
    url = info.linkUrl || url;
  } else {
    // page capture — grab selection if any, else title+url
    if (info.selectionText) text = info.selectionText;
    else text = `${title}\n${url}`;
  }

  if (!text) text = `${title}\n${url}`;

  try {
    const stored = await chrome.storage.sync.get(["defaultNotebook", "defaultFolder", "defaultTags", "appendDaily"]);
    const res = await sendToHost({
      action: "capture",
      text,
      url,
      title,
      notebook: stored.defaultNotebook || undefined,
      folder: stored.defaultFolder || undefined,
      tags: stored.defaultTags || undefined,
      daily: stored.appendDaily || false
    });
    if (res?.ok) {
      chrome.notifications?.create?.({
        type: "basic",
        iconUrl: "icons/icon128.png",
        title: "Shiki — captured",
        message: `${res.via_daemon ? "(daemon) " : ""}${res.path}`
      });
      // also badge
      chrome.action.setBadgeText({ text: "✓" });
      chrome.action.setBadgeBackgroundColor({ color: "#7aa2f7" });
      setTimeout(() => chrome.action.setBadgeText({ text: "" }), 2000);
    } else {
      throw new Error(res?.error || "unknown error");
    }
  } catch (e) {
    console.error("[shiki] capture failed", e);
    // Fallback: open popup to let user retry manually
    chrome.action.openPopup?.();
  }
});

// Keyboard shortcut quick capture
chrome.commands.onCommand.addListener(async (command) => {
  if (command === "quick-capture-selection") {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (!tab?.id) return;
    chrome.tabs.sendMessage(tab.id, { action: "getSelection" }, async (resp) => {
      const text = resp?.text || "";
      const url = tab.url || "";
      const title = tab.title || "";
      if (!text && !url) return;
      const finalText = text || `${title}\n${url}`;
      try {
        const stored = await chrome.storage.sync.get(["defaultNotebook", "defaultFolder", "defaultTags", "appendDaily"]);
        const res = await sendToHost({
          action: "capture",
          text: finalText,
          url,
          title,
          notebook: stored.defaultNotebook || undefined,
          folder: stored.defaultFolder || undefined,
          tags: stored.defaultTags || undefined,
          daily: stored.appendDaily || false
        });
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

// Relay messages from popup/content
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  (async () => {
    try {
      const res = await sendToHost(msg);
      sendResponse(res);
    } catch (e) {
      sendResponse({ ok: false, error: e.message || String(e) });
    }
  })();
  return true; // keep channel open for async response
});

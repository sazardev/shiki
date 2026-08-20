// Content script — selection, copy helpers for Shiki
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg.action === "getSelection") {
    const sel = window.getSelection()?.toString() || "";
    sendResponse({ text: sel.trim() });
  } else if (msg.action === "copyText") {
    const text = msg.text || window.getSelection()?.toString() || "";
    // Try modern clipboard, fallback to execCommand
    if (navigator.clipboard && window.isSecureContext) {
      navigator.clipboard.writeText(text).then(() => sendResponse({ ok: true }), (e) => sendResponse({ ok: false, error: String(e) }));
      return true;
    } else {
      // Fallback: create textarea hack
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand("copy");
      ta.remove();
      sendResponse({ ok });
      return true;
    }
  }
  return true;
});

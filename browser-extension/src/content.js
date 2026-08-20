// Content script — only used to fetch selection when triggered via command
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg.action === "getSelection") {
    const sel = window.getSelection()?.toString() || "";
    sendResponse({ text: sel.trim() });
  }
  return true;
});

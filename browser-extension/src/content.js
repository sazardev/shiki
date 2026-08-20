// Content script — selection, copy, HTML→md, image helpers for Shiki prod
function htmlToMarkdown(html) {
  // Minimal HTML→Markdown for selection: handles b/i/a/img/code/pre/br/p/hx/ul/ol
  const tmp = document.createElement("div");
  tmp.innerHTML = html;
  function walk(node) {
    if (node.nodeType === 3) return node.textContent;
    if (node.nodeType !== 1) return "";
    const tag = node.tagName.toLowerCase();
    const inner = Array.from(node.childNodes).map(walk).join("");
    if (tag === "br") return "\n";
    if (tag === "p" || tag === "div") return inner + "\n\n";
    if (tag === "h1") return `# ${inner}\n\n`;
    if (tag === "h2") return `## ${inner}\n\n`;
    if (tag === "h3") return `### ${inner}\n\n`;
    if (tag === "strong" || tag === "b") return `**${inner}**`;
    if (tag === "em" || tag === "i") return `_${inner}_`;
    if (tag === "code") return node.parentElement?.tagName === "PRE" ? inner : `\`${inner}\``;
    if (tag === "pre") return `\n\`\`\`\n${inner}\n\`\`\`\n`;
    if (tag === "a") {
      const href = node.getAttribute("href") || "";
      return href ? `[${inner}](${href})` : inner;
    }
    if (tag === "img") {
      const alt = node.getAttribute("alt") || "image";
      const src = node.getAttribute("src") || "";
      return src ? `![${alt}](${src})` : "";
    }
    if (tag === "li") return `- ${inner}\n`;
    if (tag === "ul" || tag === "ol") return inner + "\n";
    return inner;
  }
  return walk(tmp).replace(/\n{3,}/g, "\n\n").trim();
}

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (msg.action === "getSelection") {
    const sel = window.getSelection();
    const text = sel?.toString() || "";
    let html = "";
    try {
      if (sel && sel.rangeCount) {
        const div = document.createElement("div");
        for (let i = 0; i < sel.rangeCount; i++) div.appendChild(sel.getRangeAt(i).cloneContents());
        html = div.innerHTML;
      }
    } catch {}
    const md = html ? htmlToMarkdown(html) : "";
    sendResponse({ text: text.trim(), html, markdown: md });
  } else if (msg.action === "getPageInfo") {
    const sel = window.getSelection()?.toString() || "";
    sendResponse({ url: location.href, title: document.title, selection: sel.trim(), html: document.documentElement.outerHTML.slice(0, 5000) });
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

// Shared logs helper for background + popup/options
// Persists last 200 entries in chrome.storage.local.logs
export const LOG_LIMIT = 200;

export async function addLog(level, action, message, data = null) {
  const entry = {
    ts: Date.now(),
    level, // info, warn, error
    action,
    message: String(message).slice(0, 500),
    data: data ? JSON.stringify(data).slice(0, 1000) : null,
  };
  const { logs = [] } = await chrome.storage.local.get("logs");
  logs.unshift(entry);
  if (logs.length > LOG_LIMIT) logs.length = LOG_LIMIT;
  await chrome.storage.local.set({ logs });
  // also console
  const fn = level === "error" ? console.error : level === "warn" ? console.warn : console.log;
  fn(`[shiki:${action}] ${message}`, data || "");
  return entry;
}

export async function getLogs() {
  const { logs = [] } = await chrome.storage.local.get("logs");
  return logs;
}

export async function clearLogs() {
  await chrome.storage.local.set({ logs: [] });
}

export function formatTs(ts) {
  const d = new Date(ts);
  return d.toLocaleTimeString() + "." + String(d.getMilliseconds()).padStart(3, "0");
}

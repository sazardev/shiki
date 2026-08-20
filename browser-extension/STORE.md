# Shiki Capture — Store Listing (prod)

**Name:** Shiki Capture
**Short description:** Save any selection, page or link to your local Shiki notebooks in one click — with native host, not cloud.
**Detailed description:**
Shiki (私記) is a git-native TUI note app. This extension bridges your browser to your local Shiki via a tiny native host (`shiki-native-host`) that talks to your running TUI over `127.0.0.1` (or writes directly to disk). No data leaves your machine.

- **Fast capture:** select text → right-click → Send to Shiki / Copy+Send — even `Ctrl+Shift+Y` without opening the popup. Auto-routes `work: task` to notebook `work`.
- **Where to save:** picks notebook/folder from your real `NotebookStore`, respects custom paths (`[notebooks.<name>.path]`), tags autocomplete from your vault, templates (`default`, `daily`, `meeting`…), daily-note append, per-domain rules (`github.com → work`) and `work/meetings` subfolders.
- **Read without leaving the browser:** Search across all notebooks (fuzzy, Helix matcher), Recent (last 10 by mtime), Open in file manager / editor, Undo last capture.
- **Media:** Save image as `![alt](url)`, selected HTML → clean Markdown, page as `title + URL`.
- **Reliable:** tries live daemon first, falls back to direct write, queues offline and retries. Encrypted notebooks show 🔒 and explain to unlock in TUI.
- **Design:** official 私記 logo (#282828/#ebdbb2), minimal popup that follows light/dark (`prefers-color-scheme`), no tracking.

**Permissions justification (for review):**
- `storage` — remember defaults and per-domain rules
- `contextMenus` — right-click Send to Shiki
- `activeTab`, `scripting` — read selection / HTML to markdown
- `nativeMessaging` — talk to `com.shiki.native` (local only, 127.0.0.1)
- `notifications` — captured/queued feedback with Undo/Open buttons
- `tabs`, `clipboardWrite` — copy+send, omnibox `shiki <query>`
- `<all_urls>` — capture any page you choose (no background scraping)

**Privacy:** No remote servers, no analytics, no cloud. All reads/writes are `~/.local/share/shiki` and `~/.config/shiki`. Host source is in `shiki-native-host/` (Rust, audited).

**Screenshots needed (1280x800):**
1. Popup Capture tab (light + dark)
2. Context menu → Shiki → Send to notebook
3. Search tab with hits
4. Recent tab
5. Options page per-domain rules

**Build for store:**
```sh
cd browser-extension
zip -r shiki-capture-0.1.0.zip manifest.json src/ icons/ -x "*.DS_Store"
# or
npm run build:zip
```

**Host install:** user runs `./host/install.sh --extension-id <ID>` (Linux/macOS) or `install.ps1` (Windows) — prints ID after loading unpacked. For prod, we will ship `shiki-native-host` via `cargo install shiki-native-host` and via OS packages.

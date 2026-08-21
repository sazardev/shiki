# Chrome Web Store — Listing completo (copy-paste)

Todo lo que la consola de developer pide, ya redactado. Solo falta tu cuenta ($5 one-time) y las screenshots.

---

## 1. Store listing

### Name (45 chars max)
```
Shiki Capture — notes to your local vault
```

### Short description (132 chars max)
```
Save any selection, page or image to your local Shiki notebooks in one click. Git-native, offline-first, zero cloud.
```

### Detailed description
```
Shiki Capture brings 私記 (shiki), the git-native terminal notebook, to your browser.

⚡ FAST CAPTURE
• Select text → right-click → "✦ Send to Shiki" — done. No popup needed.
• Ctrl+Shift+Y captures selection, Ctrl+Shift+S opens the full popup.
• "Copy + Send" keeps a copy in your clipboard and saves the note.
• Type "work: call Ana" and it routes itself into your "work" notebook.

📂 SAVE IT WHERE IT BELONGS
• Pick from your real notebooks and folders — including Obsidian-style custom paths.
• Per-domain rules: github.com → work, wikipedia.org → personal.
• Tags with autocomplete from your existing notes. 12 templates (meeting, bug, spec…).
• Append to today's daily note instead of creating files.

📰 READ-LATER, DONE RIGHT
• "Save article" extracts title + content in Reader mode as clean Markdown.
• "Save bookmark" keeps the URL with a 🔖 tag.
• "Save image" stores ![alt](url) Markdown.
• Full page = title + URL in one click.

🔍 YOUR NOTES, WITHOUT LEAVING THE TAB
• Popup Search tab: fuzzy search titles + body across every notebook.
• Omnibox: type "shiki <query>" and Enter opens the best match.
• Recent tab lists your last 10 notes; click to open in your default editor.

🔒 PRIVATE BY DESIGN
• Everything is written by an open-source local host (shiki-native-host) straight to
  your disk. No account, no sync service, no telemetry — your notes are git repos you own.
• Works live with a running shiki TUI (daemon mode) or silently falls back to disk writes,
  queueing captures if nothing is running.
• Light & dark UI follows your OS theme. Official 私記 branding.

SETUP (one-time, 2 minutes)
1. Install the CLI/host:  cargo install shiki-cli   (or grab a release binary)
2. Run:  shiki extension install --id <this-extension's-ID>
3. Load this extension and capture away. Full guide: https://sazardev.github.io/shiki/
```

### Category
`Productivity` → Tools (Productivity está bien)

### Language
`English (United States)` (+ añade Spanish si quieres)

### Screenshots (1280×800 o 640×400, min 1, max 5 recomendado)
1. Popup Capture tab (tema claro) con notebooks reales y texto capturado
2. Menú contextual abierto mostrando `Shiki → Send to notebook → gm-iris`
3. Popup Search tab con hits y preview
4. Nota resultante en la TUI (`shiki` corriendo) — muestra el ecosistema
5. Options page con per-domain rules

Script para tomarlas del Chromium de prueba:
```sh
npm --prefix browser-extension run dev:chrome
# luego en WSLg: abre el popup, Win+Shift+S / gnome-screenshot -w a 1280x800
```
(Alternativa: pídeme que genere mockups HTML 1280×800 renderizados a PNG.)

### Promo images (opcionales pero recomendadas)
- Small tile 440×280: logo `私記` + "Your browser → your notes"
- Marquee 1400×560: screenshot TUI + popup lado a lado

---

## 2. Privacy tab (consola)

### Single purpose
```
Quickly save selections, pages, links and images from the browser into the user's local
Shiki notebooks via a companion local native host.
```

### Permission justifications
| Permission | Justification (pégala tal cual) |
|---|---|
| `storage` | Stores user preferences (default notebook/folder/tags, per-domain rules) and a small offline queue so captures aren't lost when the native host is temporarily unavailable. |
| `contextMenus` | Provides the right-click "Send to Shiki" actions that are the core feature. |
| `activeTab` + `scripting` | Reads the text selection/title/URL of the tab the user explicitly acts on (menu click, shortcut, popup). No background reading of any page. |
| `nativeMessaging` | Required to communicate with the user-installed open-source local host `shiki-native-host`, which writes captures to the user's own local notebooks folder. Host only accepts loopback connections. |
| `notifications` | Confirms each capture and offers Undo/Open actions. |
| `downloads` (optional) | Only used when the user clicks "Export logs" to save their own diagnostic logs. |
| `content_scripts <all_urls>` | The selection-capture content script must be available on whatever page the user chooses to capture from. It only responds to explicit messages from the extension; it never collects or transmits browsing data. |

### Data usage disclosures (marcar TODO como "No")
- Does your item collect personal data? **No**
- Health, financial, auth, communications, location, web history, user activity, website content: **No** en todas
- Compliance: certifica que no vendes datos ni usos para lending/etc.

### Privacy policy URL
```
https://sazardev.github.io/shiki/privacy.html
```
(ya creado en este repo, se publica con `main`)

---

## 3. Review notes (campo "Why do you need these permissions?" / notes)

```
This extension is the browser companion of shiki (github.com/sazardev/shiki), a local-first,
open-source note-taking app whose notebooks are plain Markdown git repositories on the user's
disk. It has NO backend and collects NO data.

To function it needs the optional, user-installed native messaging host `shiki-native-host`
(Rust, source at shiki-native-host/ in the same repository). The host only accepts loopback
(127.0.0.1) connections and writes captures to ~/.local/share/shiki (or the user's configured
folder).

IMPORTANT FOR TESTING: without the local host installed, the extension intentionally shows
"host not installed" in its popup with setup instructions — this graceful degradation is by
design so the extension remains safe/inert without the companion. To see full functionality:
  cargo install shiki-cli && shiki extension install --id <extension-id>
then reload the extension; the popup will list real notebooks and right-click menus will show
"Send to notebook".

All remote-code policies are respected: no remote hosted code, no eval, no CDN scripts;
CSP is explicitly script-src 'self'.
```

---

## 4. SEO / keywords integrados

Ya están dentro del name/description sin keyword-stuffing:
- "notes", "note taking", "markdown", "local-first", "git"
- "save page", "read later", "web clipper", "bookmark"
- "offline", "no cloud", "private", "Obsidian alternative"

Long-tail que cubre el listing: *web clipper markdown*, *notes to obsidian vault* (custom paths lo hacen compatible), *offline bookmarks chrome*, *reader mode save article*.

Distribution extra post-aprobación:
1. README.md sección nueva "Browser capture" con badge de Chrome Web Store
2. Post en docs/index.html features grid + changelog entry
3. r/ObsidianMD, r/selfhosted, Hacker News (Show HN), lobste.rs
4. El listing enlaza a sazardev.github.io/shiki → funnel al TUI

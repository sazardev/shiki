# Shiki Browser Capture

Extensión Chrome / Firefox (MV3) para capturar notas a Shiki en 1 click — habla con tu Shiki local vía `shiki-native-host` (native messaging) que a su vez usa el socket `127.0.0.1` del daemon de captura `shiki-tui/src/capture.rs`.

## Qué hace

- **Popup** (`Ctrl+Shift+S`): textarea + selector de notebook + folder (lista real de carpetas del notebook) + tags + checkbox daily. Prefill con selección de la página.
- **Menú contextual**: click derecho → `Capture selection/page/link to Shiki`.
- **Atajo rápido** (`Ctrl+Shift+Y`): captura selección sin abrir popup.
- **Dónde guardar**: lee `~/.config/shiki/config.toml` + `NotebookStore::list()` / `list_dir()` — respeta `default_notebook`, custom paths `[notebooks.<name>.path]`, y nota: `work: texto` hace routing por prefijo si dejas notebook en auto.
- **Rápido**: intenta daemon vivo primero (`try_daemon` TCP 300ms `shiki-native-host/src/main.rs`), si no está cae a escritura directa a disco (igual que `shiki capture`). Guarda `last-capture.toml` para `shiki capture --undo`.

## Estructura

```
browser-extension/
  manifest.json
  src/background.js  # contextMenus + relay a native host
  src/popup.html/js/css
  src/content.js
  icons/
  host/com.shiki.native.json  # template
  host/install.sh / install.ps1 / uninstall.sh

shiki-native-host/    # binario Rust, miembro del workspace
  src/main.rs         # native messaging loop + handlers (ping, list_notebooks, list_folders, capture)
```

## Instalación (dev, 2 min)

```sh
# 1. Build host + registrar manifest (Linux/macOS)
./browser-extension/host/install.sh

# 2. Cargar extensión
# Chrome -> chrome://extensions -> Developer mode ON -> Load unpacked -> selecciona browser-extension/

# 3. Copia el ID que te muestra Chrome y re-registra:
./browser-extension/host/install.sh --extension-id <pega_id_aqui>

# 4. Recarga extensión y prueba popup -> debe decir "daemon: on/off" no "host not installed"
```

Windows:

```powershell
.\browser-extension\host\install.ps1
# luego en chrome://extensions carga unpacked, copia ID
.\browser-extension\host\install.ps1 -ExtensionId <id>
```

Firefox (temporal, MV3):
- `about:debugging` -> Load Temporary Add-on -> `manifest.json`. El host ya se instala en `~/.mozilla/native-messaging-hosts/` vía `install.sh`.

## Uso

- Popup: elige notebook/folder, escribe o deja que se prellene la selección, `Ctrl+Enter` captura.
- Guarda defaults con `Save defaults` (persiste en `chrome.storage.sync`).
- Todo `capture` añade `Source: [title](url)` si viene de una página.

## WSL

Si usas WSL: corre tanto Shiki TUI como el host y el navegador desde el mismo lado (todo en WSL o todo en Windows). Cruzado WSL<->Win no se ven por `capture.port` distinto + `127.0.0.1` no compartido sin `networkingMode=mirrored` — ver explicación del socket.

## Protocolo

Host espera JSON length-prefixed (native messaging spec, 4 bytes LE):

```json
{"action":"list_notebooks"}
{"action":"list_folders","notebook":"personal"}
{"action":"capture","text":"hello","notebook":"work","folder":"meetings","tags":["idea"],"daily":false,"url":"https://...","title":"..."}
{"action":"ping"}
```

Responde igual framing, siempre `{ok: true/false, ...}`.

## Próximos pasos (en esta rama)

- [ ] Iconos finales + tema claro/oscuro siguiendo `shiki-config/src/themes`
- [ ] Options page para `defaultFolder` por dominio (ej: github.com -> work)
- [ ] Soporte `encrypt` (ahora falla explícito si notebook locked)
- [ ] Publicar en Chrome Web Store (requiere host installer separado)

## Desinstalar

```sh
./browser-extension/host/uninstall.sh
```

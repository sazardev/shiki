#!/usr/bin/env bash
# Installs shiki-native-host and registers the native messaging manifest
# Usage: ./install.sh [--extension-id <id>]  (if you know the extension ID from chrome://extensions)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOST_BIN="$REPO_ROOT/target/release/shiki-native-host"
MANIFEST_TEMPLATE="$SCRIPT_DIR/com.shiki.native.json"

EXT_ID="${1:-}"
if [[ "${1:-}" == "--extension-id" ]]; then EXT_ID="${2:-}"; fi

echo "==> Building shiki-native-host (release)..."
cargo build --release -p shiki-native-host

if [[ ! -f "$HOST_BIN" ]]; then
  echo "Build failed: $HOST_BIN not found" >&2
  exit 1
fi
echo "    built: $HOST_BIN"

# Resolve extension ID: try to read from Chrome's installed extensions if not given
if [[ -z "$EXT_ID" ]]; then
  echo "    note: no --extension-id given, using wildcard placeholder."
  echo "    Chrome will still work if you load the extension unpacked and then re-run with its ID."
  EXT_ID="__REPLACE_WITH_EXTENSION_ID__"
fi

# Determine manifest destination per OS
OS="$(uname -s)"
if [[ "$OS" == "Darwin" ]]; then
  DEST_DIR="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
  # Also support Chromium / Brave / Arc variants
  DEST_DIR_FIREFOX="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts"
elif [[ "$OS" == "Linux" ]]; then
  DEST_DIR="$HOME/.config/google-chrome/NativeMessagingHosts"
  DEST_DIR_CHROMIUM="$HOME/.config/chromium/NativeMessagingHosts"
  DEST_DIR_FIREFOX="$HOME/.mozilla/native-messaging-hosts"
else
  echo "Unsupported OS for auto-install: $OS" >&2
  echo "Manual: copy $MANIFEST_TEMPLATE to the NativeMessagingHosts dir for your browser" >&2
  echo "and replace __REPLACE_WITH_ABSOLUTE_PATH_TO_shiki-native-host__ with $HOST_BIN" >&2
  exit 1
fi

install_manifest() {
  local dir="$1"
  mkdir -p "$dir"
  local dest="$dir/com.shiki.native.json"
  # Prod: merge allowed_origins if manifest already exists (keeps multiple IDs)
  if [[ -f "$dest" && "$EXT_ID" != "__REPLACE_WITH_EXTENSION_ID__" ]]; then
    # Try to merge with python (keeps existing IDs + adds new one)
    if command -v python3 >/dev/null 2>&1; then
      python3 -c "
import json, pathlib
dest = pathlib.Path('$dest')
data = json.loads(dest.read_text()) if dest.exists() else {}
origins = set(data.get('allowed_origins', []))
origins.add(f'chrome-extension://$EXT_ID/')
data['name'] = 'com.shiki.native'
data['description'] = 'Shiki native messaging host - bridges browser to shiki capture daemon'
data['path'] = '$HOST_BIN'
data['type'] = 'stdio'
data['allowed_origins'] = sorted(origins)
dest.write_text(json.dumps(data, indent=2))
print(f'    merged manifest: {dest} (now {len(origins)} origin(s))')
import sys
print(open(dest).read())
sys.exit(0)
" && return 0
    fi
  fi
  sed -e "s#__REPLACE_WITH_ABSOLUTE_PATH_TO_shiki-native-host__#$HOST_BIN#g" \
      -e "s#__REPLACE_WITH_EXTENSION_ID__#$EXT_ID#g" \
      "$MANIFEST_TEMPLATE" > "$dest"
  echo "    installed manifest: $dest"
  cat "$dest"
}

install_manifest "$DEST_DIR"
[[ -n "${DEST_DIR_CHROMIUM:-}" ]] && install_manifest "$DEST_DIR_CHROMIUM" || true

# Firefox uses same manifest but with allowed_extensions instead of allowed_origins
# We generate a second manifest for Firefox if needed
if [[ -n "${DEST_DIR_FIREFOX:-}" ]]; then
  mkdir -p "$DEST_DIR_FIREFOX"
  # Firefox manifest key is allowed_extensions
  python3 -c "
import json, pathlib
src = pathlib.Path('$MANIFEST_TEMPLATE')
data = json.loads(src.read_text())
data['path'] = '$HOST_BIN'
# chrome key -> firefox key
origins = data.pop('allowed_origins', [])
ids = [o.split('/')[2].split(':')[0] if '://' in o else o for o in origins]
# if placeholder, keep placeholder id
if not ids or ids[0].startswith('__'):
    ids = ['__REPLACE_WITH_EXTENSION_ID__']
data['allowed_extensions'] = ['shiki-capture@example.com'] if ids[0].startswith('__') else ids
import os
os.makedirs('$DEST_DIR_FIREFOX', exist_ok=True)
open('$DEST_DIR_FIREFOX/com.shiki.native.json','w').write(json.dumps(data, indent=2))
print('    installed firefox manifest: $DEST_DIR_FIREFOX/com.shiki.native.json')
"
fi

echo ""
echo "Done. Next steps:"
echo "  1. Open chrome://extensions, enable Developer mode, Load unpacked -> select browser-extension/"
echo "  2. Copy the Extension ID shown there"
echo "  3. Re-run: $0 --extension-id <that-id>"
echo "  4. Test: open popup, check 'daemon: on/off' and try capturing"

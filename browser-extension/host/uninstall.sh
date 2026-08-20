#!/usr/bin/env bash
set -euo pipefail
for dir in \
  "$HOME/.config/google-chrome/NativeMessagingHosts" \
  "$HOME/.config/chromium/NativeMessagingHosts" \
  "$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts" \
  "$HOME/.mozilla/native-messaging-hosts" \
  "$HOME/Library/Application Support/Mozilla/NativeMessagingHosts"
do
  rm -vf "$dir/com.shiki.native.json" 2>/dev/null || true
done
echo "Uninstalled. You may also remove target/release/shiki-native-host if you like."

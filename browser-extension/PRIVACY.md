# Privacy Policy — Shiki Capture

**No data collection.** Shiki Capture does not collect, transmit, or sell any personal data.

- All captures are written to your local Shiki notebooks (`~/.local/share/shiki` or your custom `data_dir` / `[notebooks.<name>.path]`).
- The native host (`shiki-native-host`) only listens on `127.0.0.1` with an ephemeral port recorded in `~/.config/shiki/capture.port` and only accepts connections from localhost.
- No analytics, no tracking, no remote servers, no cookies.
- `storage.sync` holds only your defaults (notebook, folder, tags, domain rules) and `storage.local` holds an offline queue that never leaves the device.

If you uninstall the extension, remove the host manifest via `host/uninstall.sh` and `cargo uninstall shiki-native-host`.

Contact: https://github.com/sazardev/shiki/issues

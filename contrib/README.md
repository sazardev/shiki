# Shiki OS Capture — integrations

This folder is git-tracked examples, not code. Copy what you need to `~/.config`.

## Rofi / Wofi / dmenu
`scripts/rofi-capture.sh` — prompt or `--clip`. Auto-picks `rofi > wofi > dmenu`.

Bind globally (Hyprland/Sway/GNOME/KDE) to `SUPER+C`:

```
bind = SUPER, C, exec, ~/shiki/scripts/rofi-capture.sh
bind = SUPER SHIFT, C, exec, ~/shiki/scripts/rofi-capture.sh --clip
```

## Waybar / Polybar
`scripts/waybar-shiki.sh` — shows `● 2` (overdue) + daemon dot.

Waybar: see `contrib/waybar-config.jsonc`. Polybar: `exec = waybar-shiki.sh --polybar`.

## Raycast (macOS)
`contrib/raycast-shiki-capture.sh` — Script Command. Put in `~/.config/raycast/scripts`, `raycast://extensions`.

## Clipboard
`scripts/clip-capture.sh` — `shiki capture --clip --source clip` with Wayland/X11 agnostic `arboard`.
Env passthrough for browser clips:

```
SHIKI_URL=https://x.com SHIKI_TITLE="Tweet" scripts/clip-capture.sh --daily
```

## URL / Title provenance
All capture paths (CLI, native host, scripts) flow through the same daemon header `url=/title=` and append `Source: [title](url)` once, in `shiki-tui/src/capture.rs` or `shiki-cli/src/commands/capture.rs with_source`.

## Headless daemon (no TUI)
`contrib/shiki-daemon.service` — run the capture daemon standalone so rofi/waybar/browser captures stay live even with no TUI open:

```sh
cp contrib/shiki-daemon.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now shiki-daemon
shiki capture --check     # -> reachable, enabled
```

## Voice capture
`shiki capture --voice` records the mic (arecord/ffmpeg/sox) and transcribes locally with whisper.cpp (`whisper-cli`, auto-fetched + model auto-downloaded on first use — nothing leaves the machine):

```sh
shiki capture --voice                     # 5s, default model ggml-base.en.bin
shiki capture --voice --seconds 10        # longer recording
shiki capture --voice --model ggml-tiny.en.bin   # smaller/faster model
shiki capture --voice --daily             # transcript into today's daily note
```
`shiki doctor` reports recorder + whisper availability.

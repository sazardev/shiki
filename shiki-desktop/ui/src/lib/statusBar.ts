// Pure helpers for the footer — mirrors shiki-tui's `status_bar.rs` (same
// word count/reading-time math, same git-status color priority and suffix
// shape) so the desktop footer reads as the same status bar, not a
// different, thinner one that happens to share a theme.

import type { GitStatus } from "./api";

export function wordCount(body: string): number {
  return body.split(/\s+/).filter((w) => w.length > 0).length;
}

export function readingTimeMinutes(words: number, wpm: number): number {
  if (words === 0) return 0;
  return Math.max(1, Math.ceil(words / Math.max(1, wpm)));
}

export type GitStatusKind = "error" | "dirty" | "diverged" | "clean";

/// Same priority as `render::git_status_color`: an error outranks dirty,
/// dirty outranks ahead/behind, which outranks clean.
export function gitStatusKind(gs: GitStatus): GitStatusKind {
  if (gs.changed > 0) return "dirty";
  if (gs.ahead > 0 || gs.behind > 0) return "diverged";
  return "clean";
}

/// The `" +{dirty} ↑{ahead} ↓{behind}"` suffix — plain arrows instead of
/// the TUI's Nerd Font glyphs, since a browser tab can't assume a patched
/// font is installed the way a configured terminal can.
export function gitStatusSuffix(gs: GitStatus): string {
  let extras = "";
  if (gs.changed > 0) extras += ` +${gs.changed}`;
  if (gs.ahead > 0) extras += ` ↑${gs.ahead}`;
  if (gs.behind > 0) extras += ` ↓${gs.behind}`;
  return extras;
}

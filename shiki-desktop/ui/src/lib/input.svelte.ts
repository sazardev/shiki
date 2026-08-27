// Mode/Focus state + the global key dispatcher — the desktop-side mirror of
// shiki-tui's `App::on_key` dispatch order (App::mode, App::focus,
// App::leader_pending). `.svelte.ts` so the runes below work outside a
// component.
//
// Dispatch order, matching `on_key` exactly:
//   1. Any blocking modal (new-note/rename/confirm prompts) sets mode to
//      "insert" while open — the window-level listener below no-ops for
//      every mode except "normal", so the modal's own local `onkeydown`
//      (already wired in App.svelte) is the only thing that sees those
//      keystrokes. Same for "edit" (CodeMirror owns its own keymap).
//   2. `whichKeyOpen` is a separate gate checked first, same as
//      `show_which_key` being checked before mode dispatch in `on_key`.
//   3. In "normal" mode: leader-pending resolves against the global map;
//      else hardcoded nav keys are tried first, then the scoped map for the
//      current focus.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { buildKeyMaps, resolveGlobal, resolveScoped } from "./keymaps";
import type { Action, Focus, KeyMaps, KeybindingsConfig } from "./keymaps";

export type Mode = "normal" | "insert" | "edit" | "visual";

/// One overlay/panel at a time — mirrors the TUI's own convention (only one
/// `show_*` modal is ever meaningfully open at once; opening a new one is
/// always preceded by closing whatever was open). A single slot means every
/// future panel (search, outline, theme picker, tags, tasks, links,
/// history, git dashboard, ...) reuses the exact same "is *an* overlay
/// open" gate in `handleKey` instead of each needing its own boolean and
/// its own line in that gate.
export type Overlay =
  | "whichKey"
  | "globalSearch"
  | "outline"
  | "themePicker"
  | "tags"
  | "tasks"
  | "links"
  | "history"
  | "gitDash"
  | "tree"
  | "query"
  | "logs"
  | "metadata"
  | "diff";

class InputState {
  mode: Mode = $state("normal");
  focus: Focus = $state("notebooks");
  leaderPending = $state(false);
  overlay: Overlay | null = $state(null);
  maps: KeyMaps | null = $state(null);
  // Persistent toggles, not modal overlays — they don't exclude each other
  // or the overlay slot above the way a modal does (the TUI's drawer sits
  // alongside the panels, pushing them, rather than covering them).
  showDrawer = $state(false);
  showSettings = $state(false);
  // Distinguishes "the theme picker was opened from Settings" (reopen
  // Settings once the picker closes) from "opened directly via leader+c"
  // (don't) — mirrors the TUI's `App.reopen_settings_after_theme_picker`.
  reopenSettingsAfterThemePicker = $state(false);
}

export const input = new InputState();

export function initKeyMaps(cfg: KeybindingsConfig) {
  input.maps = buildKeyMaps(cfg);
}

export interface KeyContext {
  moveSelection: (delta: number) => void;
  moveSelectionHome: () => void;
  moveSelectionEnd: () => void;
  movePage: (dir: 1 | -1) => void;
  focusForward: () => void;
  focusBackward: () => void;
  cycleFocus: () => void;
  dispatch: (action: Action) => void;
  /// Exits Visual mode without acting — bound to Esc. A no-op outside
  /// Visual mode (there's nothing to cancel), so it's safe to always check
  /// first in the dispatch switch below rather than needing its own
  /// mode-specific gate.
  cancelVisual: () => void;
}

export function handleKey(event: KeyboardEvent, ctx: KeyContext) {
  const maps = input.maps;
  if (!maps) return;

  if (input.overlay) return; // the open overlay component owns keys while open

  // Visual mode falls through to the exact same nav/scoped dispatch normal
  // mode uses — j/k still move `notesSelected` (the anchor stays fixed
  // separately in App.svelte), only the *meaning* of the range differs.
  if (input.mode !== "normal" && input.mode !== "visual") return;

  const key = event.key;

  if (key === "Escape") {
    event.preventDefault();
    ctx.cancelVisual();
    return;
  }

  if (input.leaderPending) {
    input.leaderPending = false;
    const action = resolveGlobal(maps, key);
    if (action) {
      event.preventDefault();
      ctx.dispatch(action);
    }
    return;
  }

  if (key === maps.leader) {
    event.preventDefault();
    input.leaderPending = true;
    return;
  }

  switch (key) {
    case "j":
    case "ArrowDown":
      event.preventDefault();
      ctx.moveSelection(1);
      return;
    case "k":
    case "ArrowUp":
      event.preventDefault();
      ctx.moveSelection(-1);
      return;
    case "PageDown":
      event.preventDefault();
      ctx.movePage(1);
      return;
    case "PageUp":
      event.preventDefault();
      ctx.movePage(-1);
      return;
    case "Home":
      event.preventDefault();
      ctx.moveSelectionHome();
      return;
    case "End":
      event.preventDefault();
      ctx.moveSelectionEnd();
      return;
    case "l":
    case "ArrowRight":
    case "Enter":
      event.preventDefault();
      ctx.focusForward();
      return;
    case "h":
    case "ArrowLeft":
      event.preventDefault();
      ctx.focusBackward();
      return;
    case "Tab":
      event.preventDefault();
      ctx.cycleFocus();
      return;
    case "?":
      event.preventDefault();
      input.overlay = "whichKey";
      return;
  }

  if (key === maps.quit) {
    event.preventDefault();
    // No-op outside the native shell (a browser tab has no window to
    // close) — `hasTauri`-style detection isn't worth importing here just
    // for this; a failed close() call is harmless and silent either way.
    if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
      void getCurrentWindow().close();
    }
    return;
  }

  const action = resolveScoped(maps, input.focus, key);
  if (action) {
    event.preventDefault();
    ctx.dispatch(action);
  }
}

export const FOCUS_ORDER: Focus[] = ["notebooks", "notes", "preview"];

export function nextFocus(f: Focus): Focus {
  const i = FOCUS_ORDER.indexOf(f);
  return FOCUS_ORDER[(i + 1) % FOCUS_ORDER.length];
}

// Action dispatch — the desktop-side mirror of shiki-tui's `App::handle_action`.
// `App.svelte` supplies one real function per Action it already implements;
// every other Action falls through to an honest "not implemented yet"
// status message instead of doing nothing silently — every keybinding stays
// reachable (and visible in the which-key overlay) even before its own
// modal/screen exists, and nobody has to wonder whether a keypress did
// anything.
import { actionLabel } from "./keymaps";
import type { Action } from "./keymaps";

export type ActionHandlers = Partial<Record<Action, () => void>>;

export interface ActionContext {
  handlers: ActionHandlers;
  setStatus: (msg: string) => void;
}

export function dispatchAction(action: Action, ctx: ActionContext) {
  const handler = ctx.handlers[action];
  if (handler) {
    handler();
    return;
  }
  ctx.setStatus(`${actionLabel[action]} — not implemented yet`);
}

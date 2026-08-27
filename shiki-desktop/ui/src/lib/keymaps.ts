// Port of shiki-tui/src/keybindings.rs — the same Action set, the same
// scoped-map structure (leader/quit/global/notebooks/notes/preview), so the
// desktop app resolves the exact same config.toml keybindings the TUI does.
// Not every Action has a working handler yet (see actions.ts) — this file
// only owns "what key means what action," never "what the action does."

export type Focus = "notebooks" | "notes" | "preview";

export type Action =
  // Global (leader-prefixed)
  | "ThemePicker"
  | "GlobalSearch"
  | "ToggleTags"
  | "ShowLogs"
  | "ToggleFavoriteEditor"
  | "CheckForUpdate"
  | "ToggleDrawer"
  | "UndoDelete"
  | "ToggleSettings"
  | "Scratchpad"
  | "ToggleTasks"
  | "ToggleQuery"
  | "PublishNotebook"
  | "ExportNotebook"
  | "ToggleZenMode"
  // Notebooks-focus
  | "NewNotebook"
  | "RenameNotebook"
  | "DeleteNotebook"
  | "SyncNotebook"
  | "PullNotebook"
  | "PullAllNotebooks"
  | "SetRemote"
  | "PushNotebook"
  | "ShowGitDash"
  // Notes-focus
  | "NewNote"
  | "NewFolder"
  | "RenameNote"
  | "DeleteNote"
  | "JumpSearch"
  | "DailyNote"
  | "MoveNote"
  | "SortNotes"
  | "ToggleTreeView"
  | "ToggleDates"
  | "ToggleVisual"
  | "CopyEntries"
  // Notes- and Preview-focus
  | "EditInline"
  | "EditExternal"
  // Preview-focus
  | "ShowHistory"
  | "ShowWorkingDiff"
  | "ShowLinks"
  | "ShowOutline"
  | "EditMetadata";

/// Mirrors shiki-tui's `parse_key`: lowercase named specs, else a single
/// literal character. Browser `KeyboardEvent.key` already carries Shift the
/// same way crossterm's `Char('A')` does (Shift+a yields `"A"`), so no
/// modifier bookkeeping is needed here either — same "match on the key
/// string alone" simplicity the Rust doc comment explains.
export function parseKey(spec: string): string | null {
  const s = spec.toLowerCase();
  switch (s) {
    case "enter":
      return "Enter";
    case "tab":
      return "Tab";
    case "esc":
    case "escape":
      return "Escape";
    case "space":
      return " ";
    case "backspace":
      return "Backspace";
    default:
      return spec.length === 1 ? spec : null;
  }
}

// The plain-string shape shiki-config's Keybindings struct serializes to
// (via get_config) — every field a bare string, one per scope.
export interface KeybindingsConfig {
  leader: string;
  quit: string;
  global: Record<string, string>;
  notebooks: Record<string, string>;
  notes: Record<string, string>;
  preview: Record<string, string>;
}

export interface KeyMaps {
  leader: string;
  quit: string;
  global: Map<string, Action>;
  notebooks: Map<string, Action>;
  notes: Map<string, Action>;
  preview: Map<string, Action>;
}

function bind(map: Map<string, Action>, spec: string | undefined, action: Action) {
  if (!spec) return;
  const key = parseKey(spec);
  if (key !== null) map.set(key, action);
}

export function buildKeyMaps(cfg: KeybindingsConfig): KeyMaps {
  const leader = parseKey(cfg.leader) ?? " ";
  const quit = parseKey(cfg.quit) ?? "q";

  const global = new Map<string, Action>();
  bind(global, cfg.global.theme_picker, "ThemePicker");
  bind(global, cfg.global.global_search, "GlobalSearch");
  bind(global, cfg.global.tags_panel, "ToggleTags");
  bind(global, cfg.global.logs, "ShowLogs");
  bind(global, cfg.global.toggle_favorite_editor, "ToggleFavoriteEditor");
  bind(global, cfg.global.check_update, "CheckForUpdate");
  bind(global, cfg.global.drawer, "ToggleDrawer");
  bind(global, cfg.global.undo_delete, "UndoDelete");
  bind(global, cfg.global.settings, "ToggleSettings");
  bind(global, cfg.global.scratchpad, "Scratchpad");
  bind(global, cfg.global.links, "ShowLinks");
  bind(global, cfg.global.tasks_panel, "ToggleTasks");
  bind(global, cfg.global.query_panel, "ToggleQuery");
  bind(global, cfg.global.publish, "PublishNotebook");
  bind(global, cfg.global.export, "ExportNotebook");
  bind(global, cfg.global.zen_mode, "ToggleZenMode");

  const notebooks = new Map<string, Action>();
  bind(notebooks, cfg.notebooks.new, "NewNotebook");
  bind(notebooks, cfg.notebooks.rename, "RenameNotebook");
  bind(notebooks, cfg.notebooks.delete, "DeleteNotebook");
  bind(notebooks, cfg.notebooks.sync, "SyncNotebook");
  bind(notebooks, cfg.notebooks.pull, "PullNotebook");
  bind(notebooks, cfg.notebooks.pull_all, "PullAllNotebooks");
  bind(notebooks, cfg.notebooks.set_remote, "SetRemote");
  bind(notebooks, cfg.notebooks.push, "PushNotebook");
  bind(notebooks, cfg.notebooks.git_dash, "ShowGitDash");

  const notes = new Map<string, Action>();
  bind(notes, cfg.notes.new, "NewNote");
  bind(notes, cfg.notes.new_folder, "NewFolder");
  bind(notes, cfg.notes.rename, "RenameNote");
  bind(notes, cfg.notes.delete, "DeleteNote");
  bind(notes, cfg.notes.edit_inline, "EditInline");
  bind(notes, cfg.notes.edit_external, "EditExternal");
  bind(notes, cfg.notes.search, "JumpSearch");
  bind(notes, cfg.notes.daily_note, "DailyNote");
  bind(notes, cfg.notes.move_to_notebook, "MoveNote");
  bind(notes, cfg.notes.sort, "SortNotes");
  bind(notes, cfg.notes.tree_view, "ToggleTreeView");
  bind(notes, cfg.notes.toggle_dates, "ToggleDates");
  bind(notes, cfg.notes.visual, "ToggleVisual");
  bind(notes, cfg.notes.copy_entries, "CopyEntries");
  bind(notes, cfg.notes.metadata, "EditMetadata");

  const preview = new Map<string, Action>();
  bind(preview, cfg.preview.edit_inline, "EditInline");
  bind(preview, cfg.preview.edit_external, "EditExternal");
  bind(preview, cfg.preview.history, "ShowHistory");
  bind(preview, cfg.preview.diff, "ShowWorkingDiff");
  bind(preview, cfg.preview.links, "ShowLinks");
  bind(preview, cfg.preview.outline, "ShowOutline");
  bind(preview, cfg.preview.metadata, "EditMetadata");

  return { leader, quit, global, notebooks, notes, preview };
}

export function resolveGlobal(maps: KeyMaps, key: string): Action | undefined {
  return maps.global.get(key);
}

export function resolveScoped(maps: KeyMaps, focus: Focus, key: string): Action | undefined {
  const map = focus === "notebooks" ? maps.notebooks : focus === "notes" ? maps.notes : maps.preview;
  return map.get(key);
}

function describeKey(key: string): string {
  if (key === " ") return "space";
  if (key === "Enter") return "enter";
  if (key === "Tab") return "tab";
  if (key === "Escape") return "esc";
  if (key === "Backspace") return "backspace";
  return key;
}

export interface KeyEntry {
  scope: "GLOBAL (leader)" | "NOTEBOOKS" | "NOTES" | "PREVIEW";
  key: string;
  action: Action;
}

/// (scope, key description, action) for the which-key overlay — mirrors
/// `KeyMaps::entries()`, grouped by scope then sorted by key within a group.
export function entries(maps: KeyMaps): KeyEntry[] {
  const out: KeyEntry[] = [];
  for (const [key, action] of maps.global) out.push({ scope: "GLOBAL (leader)", key: describeKey(key), action });
  for (const [key, action] of maps.notebooks) out.push({ scope: "NOTEBOOKS", key: describeKey(key), action });
  for (const [key, action] of maps.notes) out.push({ scope: "NOTES", key: describeKey(key), action });
  for (const [key, action] of maps.preview) out.push({ scope: "PREVIEW", key: describeKey(key), action });
  out.sort((a, b) => (a.scope === b.scope ? a.key.localeCompare(b.key) : a.scope.localeCompare(b.scope)));
  return out;
}

export interface NavRow {
  scope: "NAVIGATION";
  key: string;
  label: string;
}

/// Hardcoded navigation rows, purely informational (no Action behind them) —
/// mirrors `KeyMaps::nav_rows`.
export function navRows(maps: KeyMaps): NavRow[] {
  return [
    { scope: "NAVIGATION", key: "j / ↓", label: "move down" },
    { scope: "NAVIGATION", key: "k / ↑", label: "move up" },
    { scope: "NAVIGATION", key: "PageDown", label: "move down a page" },
    { scope: "NAVIGATION", key: "PageUp", label: "move up a page" },
    { scope: "NAVIGATION", key: "Home", label: "jump to the top" },
    { scope: "NAVIGATION", key: "End", label: "jump to the bottom" },
    { scope: "NAVIGATION", key: "l / → / enter", label: "open (a folder, or the next panel)" },
    { scope: "NAVIGATION", key: "h / ←", label: "back (up a folder, or the previous panel)" },
    { scope: "NAVIGATION", key: "Tab", label: "switch panel" },
    { scope: "NAVIGATION", key: "?", label: "this help / command palette" },
    { scope: "NAVIGATION", key: describeKey(maps.quit), label: "close window" },
  ];
}

export const actionLabel: Record<Action, string> = {
  ThemePicker: "pick theme",
  GlobalSearch: "search all notes",
  ToggleTags: "tags panel",
  ShowLogs: "view logs",
  ToggleFavoriteEditor: "toggle favorite editor",
  CheckForUpdate: "check for update",
  ToggleDrawer: "toggle notebook drawer",
  UndoDelete: "undo last delete",
  ToggleSettings: "settings (view + edit config.toml)",
  Scratchpad: "open scratchpad",
  NewNotebook: "new notebook",
  RenameNotebook: "rename notebook",
  DeleteNotebook: "delete notebook",
  SyncNotebook: "git sync",
  PullNotebook: "git pull",
  PullAllNotebooks: "git pull (all notebooks)",
  SetRemote: "set git remote",
  PushNotebook: "sync + push now (ignores auto_push)",
  ShowGitDash: "git dashboard (all notebooks)",
  NewNote: "new note",
  NewFolder: "new folder",
  RenameNote: "rename note",
  DeleteNote: "delete note",
  JumpSearch: "jump to note (fuzzy)",
  DailyNote: "daily note",
  MoveNote: "move to notebook",
  SortNotes: "cycle sort order",
  ToggleTreeView: "notebook tree (all notes)",
  ToggleDates: "toggle note dates in list",
  ToggleVisual: "select mode (multi-select)",
  CopyEntries: "copy selection to… (visual mode)",
  EditInline: "edit (insert mode)",
  EditExternal: "edit externally ($EDITOR)",
  ShowHistory: "note history (view/revert)",
  ShowWorkingDiff: "pending changes diff (falls back to history)",
  ShowLinks: "links (outgoing / backlinks / mentions)",
  ToggleTasks: "tasks (all notebooks)",
  ToggleQuery: "query notes (frontmatter filter/sort)",
  PublishNotebook: "publish notebook to PDF",
  ExportNotebook: "export notebook to HTML/Markdown",
  ToggleZenMode: "zen mode (full-screen, hide side panels)",
  ShowOutline: "outline (jump to a heading)",
  EditMetadata: "metadata (tags / frontmatter fields)",
};

// Typed wrappers over the Rust IPC commands — one place for the interface
// contract; the Svelte components never call `invoke` directly.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { mockInvoke } from "./mockBackend";

// `window.__TAURI_INTERNALS__` only exists inside the real native webview —
// a plain `npm run dev` opened in an ordinary browser tab has no IPC bridge
// at all, so every `invoke()` call would just hang/reject. Falling back to
// the fixture backend (`mockBackend.ts`) in that case is what lets the UI —
// including the keyboard-navigation layer — be driven live in a real Chrome
// tab (e.g. via the claude-in-chrome tooling) without building/launching the
// native shell. Production and `cargo tauri dev`/`tauri build` always have
// the bridge, so this never engages there.
const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const invoke = hasTauri
  ? tauriInvoke
  : <T>(cmd: string, args?: Record<string, unknown>) => mockInvoke(cmd, args) as Promise<T>;

// The native "choose a folder" dialog (import-notebook flow) — same
// hasTauri split as `invoke`: the plugin's `open()` has nothing to talk to
// in a plain browser tab, so a browser `prompt()` stands in for it there,
// which keeps the flow testable via `npm run dev` without the native shell.
export async function pickFolder(): Promise<string | null> {
  if (!hasTauri) {
    return window.prompt("Folder path to import (mock — no native picker in the browser):");
  }
  const result = await openDialog({ directory: true, multiple: false });
  return typeof result === "string" ? result : null;
}

export interface NotebookInfo {
  name: string;
  path: string;
  encrypted: boolean;
}

export interface CloneResult {
  name: string;
  message: string;
}

export type AdoptFolderResult =
  | { status: "Adopted"; name: string }
  | { status: "NeedsGitInitConfirm"; name: string };

export interface NoteInfo {
  path: string;
  title: string;
  date: string;
  tags: string[];
  modified: string;
}

export interface NoteContent {
  content: string;
  frontmatter: {
    title: string;
    date: string;
    tags: string[];
    notebook: string;
    // `shiki_core::note::Frontmatter` also flattens `aliases`/`links`/
    // `template`/arbitrary custom fields (`extra`) into the same object —
    // typed loosely here since the desktop app only edits `tags`.
    [key: string]: unknown;
  };
}

export interface RenderedNote {
  html: string;
  root: string;
}

export interface GitStatus {
  dirty: boolean;
  changed: number;
  branch: string | null;
  ahead: number;
  behind: number;
  remote: string | null;
}

export interface SearchResult {
  notebook: string;
  path: string;
  title: string;
  score: number;
}

export interface ThemeInfo {
  name: string;
  family: string;
}

export interface PullAllResult {
  notebook: string;
  ok: boolean;
  message: string;
}

export interface LinkNote {
  path: string;
  title: string;
}

export interface LinksInfo {
  outgoing: LinkNote[];
  backlinks: LinkNote[];
  mentions: LinkNote[];
}

export interface QueryRowInfo {
  location: string;
  notebook: string;
  note_title: string;
  path: string;
  fields: Record<string, string>;
}

export interface LogEntryInfo {
  at: string;
  message: string;
}

export interface TreeNote {
  path: string;
  title: string;
  folder: string;
}

export interface DiffLineInfo {
  origin: string;
  content: string;
}

export interface RevisionInfo {
  commit_id: string;
  date: string;
  message: string;
}

export interface MisspellInfo {
  word: string;
  start: number;
  end: number;
}

export interface PastedImage {
  path: string;
  markdown_link: string;
}

export interface TaskRow {
  notebook: string;
  path: string;
  location: string;
  raw_line: string;
  occurrence: number;
  done: boolean;
  text: string;
  due: string | null;
}

export const api = {
  getConfig: () => invoke<Record<string, unknown>>("get_config"),
  saveFullConfig: (config: unknown) => invoke<void>("save_full_config", { config }),
  listNotebooks: () => invoke<NotebookInfo[]>("list_notebooks"),
  getThemeCss: (name?: string) => invoke<string>("get_theme_css", { name: name ?? null }),
  getAppVersion: () => invoke<string>("get_app_version"),
  listThemes: () => invoke<ThemeInfo[]>("list_themes"),
  setTheme: (name: string) => invoke<void>("set_theme", { name }),
  createNotebook: (name: string) => invoke<void>("create_notebook", { name }),
  createNotebookFromUrl: (url: string) =>
    invoke<CloneResult>("create_notebook_from_url", { url }),
  adoptNotebookFolder: (path: string, initGitIfMissing: boolean) =>
    invoke<AdoptFolderResult>("adopt_notebook_folder", { path, initGitIfMissing }),
  renameNotebook: (oldName: string, newName: string) =>
    invoke<void>("rename_notebook", { oldName, newName }),
  deleteNotebook: (name: string) => invoke<void>("delete_notebook", { name }),

  listNotes: (notebook: string) => invoke<NoteInfo[]>("list_notes", { notebook }),
  readNote: (notebook: string, path: string) =>
    invoke<NoteContent>("read_note", { notebook, path }),
  saveNote: (notebook: string, path: string, content: string) =>
    invoke<void>("save_note", { notebook, path, content }),
  createNote: (notebook: string, title: string, template?: string | null, folder?: string | null) =>
    invoke<NoteInfo>("create_note", { notebook, title, template, folder }),
  renameNote: (notebook: string, path: string, newTitle: string) =>
    invoke<void>("rename_note", { notebook, path, newTitle }),
  deleteNote: (notebook: string, path: string) =>
    invoke<string | null>("delete_note", { notebook, path }),
  undoDeleteNote: (notebook: string, path: string, trashPath: string) =>
    invoke<void>("undo_delete_note", { notebook, path, trashPath }),
  createFolder: (notebook: string, name: string) => invoke<void>("create_folder", { notebook, name }),
  moveNote: (notebook: string, path: string, dest: string) =>
    invoke<NoteInfo>("move_note", { notebook, path, dest }),
  copyNote: (notebook: string, path: string, dest: string) =>
    invoke<NoteInfo>("copy_note", { notebook, path, dest }),
  openExternalEditor: (notebook: string, path: string) =>
    invoke<void>("open_external_editor", { notebook, path }),
  openFavoriteEditor: (notebook: string, path: string) =>
    invoke<void>("open_favorite_editor", { notebook, path }),
  toggleFavoriteEditor: () => invoke<boolean>("toggle_favorite_editor"),
  appendLog: (message: string) => invoke<void>("append_log", { message }),
  readLogs: () => invoke<LogEntryInfo[]>("read_logs"),
  clearLogs: () => invoke<void>("clear_logs"),
  runNoteQuery: (query: string, notebook?: string | null) =>
    invoke<QueryRowInfo[]>("run_note_query", { query, notebook: notebook ?? null }),
  exportNotebook: (notebook: string, format: "html" | "md") =>
    invoke<string>("export_notebook", { notebook, format }),
  publishNotebook: (notebook: string) => invoke<string>("publish_notebook", { notebook }),
  dailyNote: (notebook: string) => invoke<NoteInfo>("daily_note", { notebook }),

  renderNote: (notebook: string, path: string) =>
    invoke<RenderedNote>("render_note", { notebook, path }),
  searchNotes: (query: string, notebook?: string | null) =>
    invoke<SearchResult[]>("search_notes", { query, notebook }),

  gitStatus: (notebook: string) => invoke<GitStatus>("git_status", { notebook }),
  gitCommit: (notebook: string, message: string) =>
    invoke<string>("git_commit", { notebook, message }),
  pullNotebook: (notebook: string) => invoke<string>("pull_notebook", { notebook }),
  pullAllNotebooks: () => invoke<PullAllResult[]>("pull_all_notebooks"),
  setNotebookRemote: (notebook: string, url: string) =>
    invoke<void>("set_notebook_remote", { notebook, url }),
  getLinks: (notebook: string, path: string) => invoke<LinksInfo>("get_links", { notebook, path }),
  noteHistory: (notebook: string, path: string) =>
    invoke<RevisionInfo[]>("note_history", { notebook, path }),
  workingDiff: (notebook: string, path: string) =>
    invoke<DiffLineInfo[]>("working_diff", { notebook, path }),
  notebookTree: (notebook: string) => invoke<TreeNote[]>("notebook_tree", { notebook }),
  revertNote: (notebook: string, path: string, commitId: string) =>
    invoke<void>("revert_note", { notebook, path, commitId }),
  listTasks: () => invoke<TaskRow[]>("list_tasks"),
  toggleTask: (notebook: string, path: string, rawLine: string, occurrence: number) =>
    invoke<void>("toggle_task", { notebook, path, rawLine, occurrence }),

  spellAvailable: () => invoke<boolean>("spell_available"),
  spellCheck: (text: string) => invoke<MisspellInfo[]>("spell_check", { text }),
  spellSuggestions: (word: string) => invoke<string[]>("spell_suggestions", { word }),
  savePastedImage: (notebook: string, bytes: number[]) =>
    invoke<PastedImage>("save_pasted_image", { notebook, bytes }),
};
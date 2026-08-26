// Typed wrappers over the Rust IPC commands — one place for the interface
// contract; the Svelte components never call `invoke` directly.

import { invoke } from "@tauri-apps/api/core";

export interface NotebookInfo {
  name: string;
  path: string;
  encrypted: boolean;
}

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
  };
}

export interface RenderedNote {
  html: string;
  root: string;
}

export interface GitStatus {
  dirty: boolean;
  changed: number;
  remote: string | null;
}

export interface SearchResult {
  notebook: string;
  path: string;
  title: string;
  score: number;
}

export const api = {
  getConfig: () => invoke<Record<string, unknown>>("get_config"),
  listNotebooks: () => invoke<NotebookInfo[]>("list_notebooks"),
  getThemeCss: () => invoke<string>("get_theme_css"),
  createNotebook: (name: string) => invoke<void>("create_notebook", { name }),

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
    invoke<void>("delete_note", { notebook, path }),
  dailyNote: (notebook: string) => invoke<NoteInfo>("daily_note", { notebook }),

  renderNote: (notebook: string, path: string) =>
    invoke<RenderedNote>("render_note", { notebook, path }),
  searchNotes: (query: string, notebook?: string | null) =>
    invoke<SearchResult[]>("search_notes", { query, notebook }),

  gitStatus: (notebook: string) => invoke<GitStatus>("git_status", { notebook }),
  gitCommit: (notebook: string, message: string) =>
    invoke<string>("git_commit", { notebook, message }),
};
// Browser-only fixture backend, used exclusively when the app is running
// without the real Tauri shell (`npm run dev` opened directly in a browser
// tab instead of the native window) — lets the whole UI, including the
// keyboard-navigation layer, be developed/tested against real config
// defaults and real fixture data with nothing but a browser. `api.ts`
// switches to this automatically by detecting `window.__TAURI_INTERNALS__`;
// production/native builds never touch this file's code path.
//
// The keybinding defaults below are copied verbatim from
// `shiki-config/src/config.rs`'s `default_*_key()` functions and the theme
// colors from `shiki-config/src/themes/gruvbox.rs::dark()` — kept in sync by
// hand the same way `docs/css/styles.css` mirrors the Rust theme palettes
// (see CLAUDE.md's Marketing site section for that existing convention).

import type { NotebookInfo, NoteInfo, NoteContent, RenderedNote, GitStatus, SearchResult, QueryRowInfo } from "./api";

const KEYBINDINGS = {
  leader: "space",
  quit: "q",
  global: {
    theme_picker: "c",
    global_search: "g",
    tags_panel: "T",
    logs: "l",
    toggle_favorite_editor: "e",
    check_update: "U",
    drawer: "b",
    undo_delete: "u",
    settings: "s",
    scratchpad: "p",
    links: "B",
    tasks_panel: "t",
    query_panel: "q",
    publish: "P",
    export: "x",
    zen_mode: "z",
  },
  notebooks: {
    new: "a",
    rename: "r",
    delete: "d",
    sync: "s",
    pull: "p",
    pull_all: "P",
    set_remote: "R",
    push: "u",
    git_dash: "G",
  },
  notes: {
    new: "a",
    new_folder: "f",
    rename: "r",
    delete: "d",
    edit_inline: "i",
    edit_external: "E",
    search: "/",
    daily_note: "t",
    move_to_notebook: "m",
    sort: "o",
    tree_view: "T",
    toggle_dates: "D",
    visual: "v",
    copy_entries: "y",
    metadata: "M",
  },
  preview: {
    edit_inline: "i",
    edit_external: "E",
    history: "H",
    diff: "d",
    links: "L",
    outline: "o",
    metadata: "M",
  },
};

// Every field a theme.rs `theme!{}` invocation sets, in the exact order
// `theme_to_css` (shiki-desktop/src/commands.rs) emits them — kept as a
// plain tuple-shaped array (not an object) so each of the 37 entries below
// reads as one line, the same density the Rust `theme!{}` macro invocations
// themselves have.
type ThemeColors = [
  bg: string,
  fg: string,
  accent: string,
  selection: string,
  border: string,
  statusbar: string,
  highlight: string,
  error: string,
  warning: string,
  success: string,
  inactive: string,
  scrollbar: string,
  tabActive: string,
  tabInactive: string,
  panelTitle: string,
  cursor: string,
  link: string,
  tag: string,
  muted: string,
];

interface ThemeDef {
  name: string;
  family: string;
  colors: ThemeColors;
}

// Every built-in theme, copied verbatim from shiki-config/src/themes/*.rs
// (same "keep the mock's copy in sync by hand" convention CLAUDE.md already
// documents for docs/css/styles.css) — all 37, in the same order
// `shiki_config::themes::all()` returns them, so the picker shows the exact
// same catalog testing against the mock as it does against the real Tauri
// backend. `"default"` intentionally carries ANSI names/`"reset"` instead of
// hex (same as `Theme::terminal_default()`) — passed through verbatim, not
// special-cased, matching production's own `theme_to_css`.
const THEMES: ThemeDef[] = [
  { name: "Arasaka", family: "Hacker", colors: ["#120808", "#f5e6e6", "#ff003c", "#241010", "#331717", "#0c0505", "#ffd700", "#ff003c", "#ffb700", "#ff6b6b", "#5a2e2e", "#241010", "#ffd700", "#5a2e2e", "#ff003c", "#ffd700", "#ff6b6b", "#ff003c", "#8a5a5a"] },
  { name: "Blade Runner", family: "Hacker", colors: ["#0e0a08", "#f2e9de", "#ff9e3d", "#1d150f", "#2b1f15", "#090705", "#ffce6b", "#ff5a5a", "#ff9e3d", "#37c8ab", "#5a4a38", "#1d150f", "#37c8ab", "#5a4a38", "#ff9e3d", "#ffce6b", "#37c8ab", "#ff9e3d", "#8a7660"] },
  { name: "catppuccin-mocha", family: "Classic", colors: ["#1e1e2e", "#cdd6f4", "#89b4fa", "#45475a", "#313244", "#181825", "#f9e2af", "#f38ba8", "#fab387", "#a6e3a1", "#6c7086", "#45475a", "#cba6f7", "#6c7086", "#f5c2e7", "#f5e0dc", "#89b4fa", "#cba6f7", "#a6adc8"] },
  { name: "Cyberpunk 2077", family: "Hacker", colors: ["#100a18", "#e8e6f0", "#fcee0a", "#22143a", "#33204f", "#0a0610", "#ff003c", "#ff003c", "#fcee0a", "#00f0ff", "#554570", "#22143a", "#00f0ff", "#554570", "#fcee0a", "#00f0ff", "#00f0ff", "#ff003c", "#7d6f96"] },
  { name: "Doom", family: "Hacker", colors: ["#0d0505", "#f2e0e0", "#e62525", "#1f0a0a", "#2d0f0f", "#080303", "#ff7b3d", "#ff2e2e", "#ff9e3d", "#7bc25e", "#5a2424", "#1f0a0a", "#e62525", "#5a2424", "#ff7b3d", "#ff7b3d", "#7bc25e", "#e62525", "#8a5050"] },
  { name: "dracula", family: "Classic", colors: ["#282a36", "#f8f8f2", "#bd93f9", "#44475a", "#44475a", "#21222c", "#f1fa8c", "#ff5555", "#ffb86c", "#50fa7b", "#6272a4", "#44475a", "#bd93f9", "#6272a4", "#8be9fd", "#f8f8f2", "#8be9fd", "#ff79c6", "#6272a4"] },
  { name: "Fallout Terminal", family: "Hacker", colors: ["#0a0f0a", "#00ff00", "#00ff00", "#0f2410", "#1a3a1a", "#060a06", "#aaffaa", "#ff5555", "#ffb000", "#00ff00", "#1d3a1d", "#0f2410", "#00ff00", "#1d3a1d", "#00ff00", "#00ff00", "#aaffaa", "#00ff00", "#3a6b3a"] },
  { name: "Ghost in the Shell", family: "Hacker", colors: ["#0a1214", "#d8f2ee", "#2dd4bf", "#122024", "#1a2f34", "#060a0c", "#8fe3d0", "#ff5a7a", "#ffc05a", "#2dd4bf", "#33535a", "#122024", "#37b6ff", "#33535a", "#2dd4bf", "#37b6ff", "#37b6ff", "#2dd4bf", "#5d7a80"] },
  { name: "gruvbox-dark", family: "Classic", colors: ["#282828", "#ebdbb2", "#fabd2f", "#3c3836", "#504945", "#1d2021", "#d79921", "#cc241d", "#d79921", "#98971a", "#928374", "#504945", "#b16286", "#928374", "#d3869b", "#ebdbb2", "#fabd2f", "#b16286", "#a89984"] },
  { name: "gruvbox-dark-hard", family: "Classic", colors: ["#1d2021", "#ebdbb2", "#fabd2f", "#3c3836", "#504945", "#141617", "#d79921", "#cc241d", "#d79921", "#98971a", "#928374", "#3c3836", "#b16286", "#928374", "#d3869b", "#ebdbb2", "#fabd2f", "#b16286", "#a89984"] },
  { name: "gruvbox-dark-soft", family: "Classic", colors: ["#32302f", "#ebdbb2", "#fabd2f", "#3c3836", "#504945", "#1d2021", "#d79921", "#cc241d", "#d79921", "#98971a", "#928374", "#504945", "#b16286", "#928374", "#d3869b", "#ebdbb2", "#fabd2f", "#b16286", "#a89984"] },
  { name: "gruvbox-light", family: "Classic", colors: ["#fbf1c7", "#3c3836", "#b57614", "#ebdbb2", "#d5c4a1", "#f2e5bc", "#d79921", "#cc241d", "#d79921", "#98971a", "#928374", "#d5c4a1", "#b16286", "#928374", "#8f3f71", "#3c3836", "#b57614", "#b16286", "#7c6f64"] },
  { name: "gruvbox-light-hard", family: "Classic", colors: ["#f9f5d7", "#3c3836", "#b57614", "#ebdbb2", "#d5c4a1", "#f2e5bc", "#d79921", "#cc241d", "#d79921", "#98971a", "#928374", "#d5c4a1", "#b16286", "#928374", "#8f3f71", "#3c3836", "#b57614", "#b16286", "#7c6f64"] },
  { name: "gruvbox-light-soft", family: "Classic", colors: ["#f2e5bc", "#3c3836", "#b57614", "#ebdbb2", "#d5c4a1", "#ebdbb2", "#d79921", "#cc241d", "#d79921", "#98971a", "#928374", "#d5c4a1", "#b16286", "#928374", "#8f3f71", "#3c3836", "#b57614", "#b16286", "#7c6f64"] },
  { name: "Halo", family: "Games", colors: ["#0d1420", "#dbe6f2", "#6fa55c", "#18233a", "#212f4d", "#090d16", "#ffd75e", "#ff5a5a", "#ff9f43", "#7bbfe0", "#3f4a63", "#18233a", "#3b6ea5", "#3f4a63", "#6fa55c", "#7bbfe0", "#3b6ea5", "#6fa55c", "#64708c"] },
  { name: "LoL (Ahri)", family: "LoL", colors: ["#241529", "#f7e9f2", "#e8448f", "#351d40", "#482957", "#1b0f1f", "#c9a1ff", "#ff3d6e", "#ffa25c", "#6ee7b7", "#7d5d93", "#351d40", "#b388ff", "#7d5d93", "#ff7ab8", "#ffffff", "#b388ff", "#e8448f", "#9c7fb0"] },
  { name: "LoL (Jinx)", family: "LoL", colors: ["#0e1b2c", "#eaf2fb", "#ff3da5", "#1a2c44", "#243a56", "#091321", "#ffe14d", "#ff3860", "#ffa63d", "#2ee6ff", "#4c6078", "#1a2c44", "#2ee6ff", "#4c6078", "#ff3da5", "#2ee6ff", "#2ee6ff", "#ff3da5", "#7e92a8"] },
  { name: "LoL (Teemo)", family: "LoL", colors: ["#16220f", "#e9f2dc", "#8bc34a", "#22331a", "#2d4423", "#101a0b", "#fbc02d", "#e5533d", "#f79b2e", "#9ccc65", "#5c6f49", "#22331a", "#fbc02d", "#5c6f49", "#fbc02d", "#dcedc8", "#9ccc65", "#8bc34a", "#7e9170"] },
  { name: "Matrix", family: "Hacker", colors: ["#0d0208", "#00ff41", "#00ff41", "#003b00", "#008f11", "#080100", "#a8ff60", "#ff4444", "#ffb000", "#00ff41", "#006600", "#003b00", "#00ff41", "#006600", "#00ff41", "#00ff41", "#a8ff60", "#00ff41", "#008f11"] },
  { name: "monokai", family: "Classic", colors: ["#272822", "#f8f8f2", "#f92672", "#49483e", "#3e3d32", "#1e1f1c", "#e6db74", "#f92672", "#fd971f", "#a6e22e", "#75715e", "#49483e", "#f92672", "#75715e", "#66d9ef", "#f8f8f2", "#66d9ef", "#ae81ff", "#75715e"] },
  { name: "Mr. Robot", family: "Hacker", colors: ["#0c0c10", "#e8e8f0", "#ff3b3b", "#1c1c24", "#292933", "#08080b", "#ffd75e", "#ff3b3b", "#ff9e3d", "#37c8ab", "#555566", "#1c1c24", "#37c8ab", "#555566", "#ff3b3b", "#ffd75e", "#37c8ab", "#ff3b3b", "#7d7d8f"] },
  { name: "nord", family: "Classic", colors: ["#2e3440", "#d8dee9", "#88c0d0", "#434c5e", "#3b4252", "#242933", "#ebcb8b", "#bf616a", "#d08770", "#a3be8c", "#4c566a", "#3b4252", "#b48ead", "#4c566a", "#81a1c1", "#d8dee9", "#5e81ac", "#b48ead", "#8fbcbb"] },
  { name: "one-dark", family: "Classic", colors: ["#282c34", "#abb2bf", "#61afef", "#3e4451", "#3e4451", "#21252b", "#e5c07b", "#e06c75", "#d19a66", "#98c379", "#5c6370", "#3e4451", "#61afef", "#5c6370", "#c678dd", "#528bff", "#61afef", "#c678dd", "#5c6370"] },
  { name: "Overwatch", family: "Games", colors: ["#14161c", "#e8eaf0", "#f99e1a", "#23262f", "#30343f", "#0e0f14", "#ffd75e", "#ff5a5a", "#ff9f43", "#218ffe", "#565b66", "#23262f", "#218ffe", "#565b66", "#f99e1a", "#218ffe", "#218ffe", "#f99e1a", "#7d828c"] },
  { name: "Pokémon (Charizard)", family: "Games", colors: ["#1a0f0d", "#ffe9d6", "#ff6b35", "#2d1913", "#40241a", "#120a09", "#ffd93d", "#ff4444", "#ffa502", "#ff8c42", "#6e4a38", "#2d1913", "#ff3b3b", "#6e4a38", "#ff6b35", "#ffd93d", "#ff8c42", "#ff6b35", "#9a7563"] },
  { name: "Pokémon (Gengar)", family: "Games", colors: ["#12101f", "#e8d9ff", "#9d6bff", "#1f1b33", "#2d2747", "#0d0b17", "#ff6bd6", "#c76bff", "#b08cff", "#6bffd0", "#4a4266", "#1f1b33", "#c76bff", "#4a4266", "#9d6bff", "#ff6bd6", "#9d6bff", "#c76bff", "#6e6490"] },
  { name: "Pokémon (Pikachu)", family: "Games", colors: ["#1a1c2e", "#fff5d6", "#f6c344", "#2a2d45", "#383c5e", "#13141f", "#ffe066", "#ff5252", "#ff9f43", "#7bed9f", "#55597a", "#2a2d45", "#ff3b3b", "#55597a", "#f6c344", "#ff3b3b", "#f6c344", "#ff3b3b", "#858aa8"] },
  { name: "Portal", family: "Games", colors: ["#e8e8e0", "#2b2b2b", "#ff8a3d", "#d0d0c8", "#b8b8b0", "#dcdcd4", "#ffb36b", "#e0362c", "#ff8a3d", "#2f9de0", "#8a8a85", "#d0d0c8", "#2f9de0", "#8a8a85", "#ff8a3d", "#2f9de0", "#2f9de0", "#ff8a3d", "#8a8a85"] },
  { name: "solarized-dark", family: "Classic", colors: ["#002b36", "#839496", "#268bd2", "#073642", "#586e75", "#073642", "#b58900", "#dc322f", "#cb4b16", "#859900", "#586e75", "#073642", "#6c71c4", "#586e75", "#d33682", "#93a1a1", "#268bd2", "#6c71c4", "#657b83"] },
  { name: "Stardew Valley", family: "Games", colors: ["#1a1812", "#f0e8d8", "#6aa84f", "#2a251a", "#3a3426", "#12100c", "#e0a020", "#c23b22", "#d4881f", "#8bc34a", "#5d5748", "#2a251a", "#e0a020", "#5d5748", "#e0a020", "#f0e8d8", "#6aa84f", "#c23b22", "#8a8270"] },
  { name: "Super Mario", family: "Games", colors: ["#16121a", "#f5eee2", "#e52521", "#261f28", "#352c38", "#0f0c13", "#ffd75e", "#ff3b30", "#ff9f43", "#5dbb63", "#5d5464", "#261f28", "#2456a5", "#5d5464", "#e52521", "#ffd75e", "#2456a5", "#ff9f43", "#8a8194"] },
  { name: "Super Mario (Luigi)", family: "Games", colors: ["#0e1a12", "#e4f2e6", "#4a9e5c", "#1a2b1f", "#243a2c", "#0a130d", "#ffd75e", "#e5533d", "#ff9f43", "#6fcf78", "#4a5d50", "#1a2b1f", "#2456a5", "#4a5d50", "#4a9e5c", "#ffd75e", "#2456a5", "#e52521", "#758a7e"] },
  { name: "Synthwave", family: "Hacker", colors: ["#16082e", "#f0e6ff", "#ff2ec4", "#241246", "#33206a", "#0f0520", "#ffd400", "#ff2e6e", "#ff9e2e", "#00ffff", "#584388", "#241246", "#00ffff", "#584388", "#ff2ec4", "#00ffff", "#00ffff", "#ff2ec4", "#8a74b8"] },
  { name: "tokyo-night", family: "Classic", colors: ["#1a1b26", "#c0caf5", "#7aa2f7", "#283457", "#292e42", "#16161e", "#e0af68", "#f7768e", "#e0af68", "#9ece6a", "#565f89", "#292e42", "#bb9af7", "#565f89", "#7dcfff", "#c0caf5", "#7aa2f7", "#bb9af7", "#565f89"] },
  { name: "Tron", family: "Hacker", colors: ["#0a1014", "#d6f4ff", "#00f6ff", "#0e2026", "#122d36", "#060a0d", "#00d8ff", "#ff4d4d", "#ffb84d", "#00f6ff", "#2e5a66", "#0e2026", "#00f6ff", "#2e5a66", "#00f6ff", "#00f6ff", "#00d8ff", "#00f6ff", "#4d7a86"] },
  { name: "Zelda", family: "Games", colors: ["#0c1710", "#e8f0dc", "#c6a45c", "#15261a", "#1e3526", "#080f0a", "#ffd700", "#e5533d", "#ffa63d", "#6bbf59", "#3f5344", "#15261a", "#0b6e2e", "#3f5344", "#c6a45c", "#ffd700", "#3aa3ff", "#0b6e2e", "#7a8d7e"] },
  { name: "default", family: "System", colors: ["reset", "reset", "blue", "darkgray", "reset", "reset", "yellow", "red", "yellow", "green", "darkgray", "darkgray", "blue", "darkgray", "magenta", "reset", "cyan", "magenta", "darkgray"] },
];

function cssFor(c: ThemeColors): string {
  const [bg, fg, accent, selection, border, statusbar, highlight, error, warning, success, inactive, scrollbar, tabActive, tabInactive, panelTitle, cursor, link, tag, muted] = c;
  return `:root{--bg:${bg};--fg:${fg};--accent:${accent};--selection:${selection};--border:${border};--statusbar:${statusbar};--highlight:${highlight};--error:${error};--warning:${warning};--success:${success};--inactive:${inactive};--scrollbar:${scrollbar};--tab-active:${tabActive};--tab-inactive:${tabInactive};--panel-title:${panelTitle};--cursor:${cursor};--link:${link};--tag:${tag};--muted:${muted};}`;
}

const THEME_CSS: Record<string, string> = Object.fromEntries(THEMES.map((t) => [t.name, cssFor(t.colors)]));

function themeCss(name: string | undefined): string {
  return THEME_CSS[name ?? mockConfig.theme.name] ?? THEME_CSS["gruvbox-dark"];
}

interface MockNote {
  path: string;
  title: string;
  date: string;
  tags: string[];
  body: string;
}

const trashedNotes: Record<string, MockNote> = {};
let mockUseFavoriteEditor = false;
const mockLogs: { at: string; message: string }[] = [];
const mockConfig = {
  theme: { name: "gruvbox-dark", overrides: {} as Record<string, unknown> },
  notebooks: {} as Record<string, unknown>,
  snippets: {
    date: { label: "Insert date", body: "{{date}}" },
  } as Record<string, { label?: string; body: string }>,
};

const notebooks: Record<string, MockNote[]> = {
  personal: [
    {
      path: "hello-world.md",
      title: "Hello World",
      date: "2026-08-20",
      tags: ["welcome"],
      body: "# Hello World\n\nThis is a **mock** note served from the browser fixture backend, not the real Tauri IPC bridge.\n\n- [ ] try keyboard nav\n- [x] open which-key with `?`\n",
    },
    {
      path: "roadmap.md",
      title: "Desktop parity roadmap",
      date: "2026-08-25",
      tags: ["shiki", "roadmap"],
      body: "# Desktop parity roadmap\n\nTracking TUI parity work for shiki-desktop.\n",
    },
  ],
  work: [
    {
      path: "standup-2026-08-27.md",
      title: "Standup 2026-08-27",
      date: "2026-08-27",
      tags: ["standup"],
      body: "# Standup\n\n- shipped keyboard nav foundation\n",
    },
  ],
};

function noteInfo(nb: string, n: MockNote): NoteInfo {
  return {
    path: n.path,
    title: n.title,
    date: n.date,
    tags: n.tags,
    modified: `${n.date}T09:00:00`,
  };
}

function findNote(nb: string, path: string): MockNote | undefined {
  return notebooks[nb]?.find((n) => n.path === path);
}

function renderMd(body: string): string {
  // Deliberately not a real markdown renderer — just enough structure
  // (headings/bullets/checkboxes/bold) to visually confirm the preview pane
  // paints something representative while testing keyboard nav.
  const escaped = body.replace(/&/g, "&amp;").replace(/</g, "&lt;");
  return escaped
    .split("\n")
    .map((line) => {
      if (line.startsWith("# ")) return `<h1>${line.slice(2)}</h1>`;
      if (line.startsWith("- [ ] ")) return `<li><input type="checkbox" disabled> ${line.slice(6)}</li>`;
      if (line.startsWith("- [x] ")) return `<li><input type="checkbox" checked disabled> ${line.slice(6)}</li>`;
      if (line.startsWith("- ")) return `<li>${line.slice(2)}</li>`;
      return line.replace(/\*\*(.+?)\*\*/g, "<b>$1</b>") ? `<p>${line}</p>` : "";
    })
    .join("\n");
}

let seq = 0;

export function mockInvoke(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  switch (cmd) {
    case "get_config":
      return Promise.resolve({
        general: {
          default_notebook: "personal",
          editor: "nvim",
          daily_template: "daily",
          use_favorite_editor: mockUseFavoriteEditor,
          mouse_drag_selection: true,
          show_hints: true,
          show_coffee_link: true,
          compact_footer: false,
          reading_wpm: 200,
          default_note_sort: "date-desc",
        },
        theme: { name: mockConfig.theme.name, overrides: mockConfig.theme.overrides },
        git: {
          auto_commit: true,
          auto_push: false,
          commit_prefix: "shiki:",
          remote: "origin",
          branch: "main",
          sign_commits: false,
          auto_sync: false,
          auto_sync_every: 5,
        },
        editor: {
          spellcheck: false,
          spellcheck_lang: "en_US",
          multi_cursor: true,
          paste_images: true,
          insert_timestamp: true,
          line_numbers: true,
        },
        export: { pdf_theme: "gruvbox-dark", export_dir: "", ask_export_path: false },
        notebooks: mockConfig.notebooks,
        snippets: mockConfig.snippets,
        keybindings: KEYBINDINGS,
      });
    case "save_full_config": {
      const cfg = args.config as any;
      if (cfg?.theme?.name) mockConfig.theme.name = cfg.theme.name;
      if (cfg?.theme?.overrides) mockConfig.theme.overrides = cfg.theme.overrides;
      if (cfg?.notebooks) mockConfig.notebooks = cfg.notebooks;
      if (cfg?.snippets) mockConfig.snippets = cfg.snippets;
      if (typeof cfg?.general?.use_favorite_editor === "boolean") {
        mockUseFavoriteEditor = cfg.general.use_favorite_editor;
      }
      return Promise.resolve();
    }
    case "get_theme_css":
      return Promise.resolve(themeCss(args.name as string | undefined));
    case "get_app_version":
      return Promise.resolve("0.9.4-mock");
    case "list_themes":
      return Promise.resolve(THEMES.map((t) => ({ name: t.name, family: t.family })));
    case "set_theme":
      mockConfig.theme.name = String(args.name);
      return Promise.resolve();
    case "open_external_editor":
    case "open_favorite_editor":
      // Nothing to actually spawn in a browser tab — the mock just
      // acknowledges the call so the UI's status message/flow is testable.
      return Promise.resolve();
    case "toggle_favorite_editor":
      mockUseFavoriteEditor = !mockUseFavoriteEditor;
      return Promise.resolve(mockUseFavoriteEditor);
    case "append_log":
      mockLogs.push({ at: new Date().toISOString(), message: String(args.message) });
      return Promise.resolve();
    case "read_logs":
      return Promise.resolve([...mockLogs]);
    case "export_notebook":
      return Promise.resolve(`/mock/exports/${args.notebook}.${args.format}`);
    case "publish_notebook":
      return Promise.resolve(`/mock/exports/${args.notebook}.pdf`);
    case "run_note_query": {
      // Not a real DSL parser — the mock just filters by substring match
      // on the query text against each note's title/tags, good enough to
      // exercise the panel's rendering/navigation without reimplementing
      // shiki-core's actual `query::parse`/`run_query`.
      const q = String(args.query).trim().toLowerCase();
      const out: QueryRowInfo[] = [];
      for (const [nb, list] of Object.entries(notebooks)) {
        if (args.notebook && args.notebook !== nb) continue;
        for (const n of list) {
          if (!q || n.title.toLowerCase().includes(q) || n.tags.some((t) => t.includes(q))) {
            out.push({ location: `${nb}/${n.title}`, notebook: nb, note_title: n.title, path: n.path, fields: {} });
          }
        }
      }
      return Promise.resolve(out);
    }
    case "clear_logs":
      mockLogs.length = 0;
      mockLogs.push({ at: new Date().toISOString(), message: "logs cleared" });
      return Promise.resolve();
    case "pull_notebook":
      return Promise.resolve(`${args.notebook} already up to date`);
    case "pull_all_notebooks":
      return Promise.resolve(
        Object.keys(notebooks).map((notebook) => ({ notebook, ok: true, message: "ok" })),
      );
    case "set_notebook_remote":
      return Promise.resolve();
    case "notebook_tree": {
      const nb = String(args.notebook);
      const list = notebooks[nb] ?? [];
      return Promise.resolve(list.map((n) => ({ path: n.path, title: n.title, folder: "" })));
    }
    case "working_diff":
      return Promise.resolve([
        { origin: " ", content: "# Hello World" },
        { origin: "-", content: "old line" },
        { origin: "+", content: "This is a **mock** note served from the browser fixture backend." },
      ]);
    case "note_history":
      return Promise.resolve([
        { commit_id: "abc1234", date: "2026-08-25 09:00", message: "shiki: added (note.md)" },
        { commit_id: "def5678", date: "2026-08-20 09:00", message: "shiki: added (note.md)" },
      ]);
    case "revert_note":
      return Promise.resolve();
    case "get_links": {
      const nb = String(args.notebook);
      const list = notebooks[nb] ?? [];
      const target = list.find((n) => n.path === args.path);
      const outgoing: { path: string; title: string }[] = [];
      const backlinks: { path: string; title: string }[] = [];
      const mentions: { path: string; title: string }[] = [];
      if (target) {
        const linkRe = /\[\[([^\]|#^]+)/g;
        let m: RegExpExecArray | null;
        while ((m = linkRe.exec(target.body))) {
          const hit = list.find((n) => n.title.toLowerCase() === m![1].trim().toLowerCase());
          if (hit) outgoing.push({ path: hit.path, title: hit.title });
        }
        for (const n of list) {
          if (n.path === target.path) continue;
          const linksToTarget = new RegExp(`\\[\\[${target.title}`, "i").test(n.body);
          const mentionsTarget = n.body.toLowerCase().includes(target.title.toLowerCase());
          if (linksToTarget) backlinks.push({ path: n.path, title: n.title });
          else if (mentionsTarget) mentions.push({ path: n.path, title: n.title });
        }
      }
      return Promise.resolve({ outgoing, backlinks, mentions });
    }
    case "list_tasks": {
      const out: unknown[] = [];
      for (const [nb, list] of Object.entries(notebooks)) {
        for (const n of list) {
          const lines = n.body.split("\n");
          const seen: Record<string, number> = {};
          for (const line of lines) {
            const m = /^(\s*[-*] \[)([ xX])(\] +)(.*)$/.exec(line);
            if (!m) continue;
            const occurrence = seen[line] ?? 0;
            seen[line] = occurrence + 1;
            out.push({
              notebook: nb,
              path: n.path,
              location: `${nb}/${n.title}`,
              raw_line: line,
              occurrence,
              done: m[2].toLowerCase() === "x",
              text: m[4],
              due: null,
            });
          }
        }
      }
      return Promise.resolve(out);
    }
    case "toggle_task": {
      const nb = notebooks[String(args.notebook)];
      const n = nb?.find((x) => x.path === args.path);
      if (n) {
        const lines = n.body.split("\n");
        let count = 0;
        for (let i = 0; i < lines.length; i++) {
          if (lines[i] === args.rawLine) {
            if (count === args.occurrence) {
              lines[i] = lines[i].includes("[ ]") ? lines[i].replace("[ ]", "[x]") : lines[i].replace(/\[x\]/i, "[ ]");
              break;
            }
            count++;
          }
        }
        n.body = lines.join("\n");
      }
      return Promise.resolve();
    }
    case "list_notebooks":
      return Promise.resolve(
        Object.keys(notebooks).map((name): NotebookInfo => ({ name, path: `/mock/${name}`, encrypted: false })),
      );
    case "create_notebook": {
      const name = String(args.name);
      notebooks[name] = notebooks[name] ?? [];
      return Promise.resolve();
    }
    case "create_notebook_from_url": {
      // Same last-path-segment derivation as the real
      // `notebook_name_from_git_url` (Rust), just in JS — good enough for
      // exercising the UI without a real git clone in the browser.
      const url = String(args.url);
      const trimmed = url.replace(/\/+$/, "");
      const last = trimmed.split(/[/:]/).pop() ?? "";
      const name = last.replace(/\.git$/, "");
      if (!name) return Promise.reject(`could not derive a notebook name from '${url}'`);
      notebooks[name] = [
        {
          path: "cloned-note.md",
          title: "Cloned Note",
          date: "2026-08-27",
          tags: [],
          body: "# Cloned Note\n\nFetched from the remote repository.",
        },
      ];
      return Promise.resolve({ name, message: `cloned from ${url}` });
    }
    case "adopt_notebook_folder": {
      const path = String(args.path);
      const name = path.replace(/\/+$/, "").split("/").pop() ?? "";
      if (!name) return Promise.reject(`could not derive a notebook name from '${path}'`);
      if (notebooks[name]) return Promise.reject(`notebook '${name}' already exists`);
      if (!args.initGitIfMissing) {
        return Promise.resolve({ status: "NeedsGitInitConfirm", name });
      }
      notebooks[name] = [
        {
          path: "imported-note.md",
          title: "Imported Note",
          date: "2026-08-27",
          tags: [],
          body: "# Imported Note\n\nAdopted from an existing local folder.",
        },
      ];
      return Promise.resolve({ status: "Adopted", name });
    }
    case "rename_notebook": {
      const oldName = String(args.oldName);
      const newName = String(args.newName);
      if (!notebooks[oldName]) return Promise.reject("notebook not found");
      if (notebooks[newName]) return Promise.reject("a notebook with that name already exists");
      notebooks[newName] = notebooks[oldName];
      delete notebooks[oldName];
      return Promise.resolve();
    }
    case "delete_notebook": {
      delete notebooks[String(args.name)];
      return Promise.resolve();
    }
    case "list_notes": {
      const nb = String(args.notebook);
      return Promise.resolve((notebooks[nb] ?? []).map((n) => noteInfo(nb, n)));
    }
    case "read_note": {
      const n = findNote(String(args.notebook), String(args.path));
      if (!n) return Promise.reject("note not found");
      const content: NoteContent = {
        content: `---\ntitle: ${n.title}\ndate: ${n.date}\ntags: [${n.tags.join(", ")}]\nnotebook: ${args.notebook}\n---\n${n.body}`,
        frontmatter: { title: n.title, date: n.date, tags: n.tags, notebook: String(args.notebook) },
      };
      return Promise.resolve(content);
    }
    case "save_note": {
      const n = findNote(String(args.notebook), String(args.path));
      if (n) n.body = String(args.content).replace(/^---[\s\S]*?---\n/, "");
      return Promise.resolve();
    }
    case "create_note": {
      const nb = String(args.notebook);
      const title = String(args.title);
      seq += 1;
      const note: MockNote = {
        path: `${title.toLowerCase().replace(/\s+/g, "-")}-${seq}.md`,
        title,
        date: new Date().toISOString().slice(0, 10),
        tags: [],
        body: `# ${title}\n\n`,
      };
      notebooks[nb] = notebooks[nb] ?? [];
      notebooks[nb].unshift(note);
      return Promise.resolve(noteInfo(nb, note));
    }
    case "rename_note": {
      const n = findNote(String(args.notebook), String(args.path));
      if (n) n.title = String(args.newTitle);
      return Promise.resolve();
    }
    case "delete_note": {
      const nb = String(args.notebook);
      const removed = notebooks[nb]?.find((n) => n.path === args.path);
      notebooks[nb] = (notebooks[nb] ?? []).filter((n) => n.path !== args.path);
      if (removed) trashedNotes[`${nb}:mock-trash-${removed.path}`] = removed;
      return Promise.resolve(removed ? `mock-trash-${removed.path}` : null);
    }
    case "undo_delete_note": {
      const nb = String(args.notebook);
      const key = `${nb}:${args.trashPath}`;
      const restored = trashedNotes[key];
      if (restored) {
        notebooks[nb] = notebooks[nb] ?? [];
        notebooks[nb].unshift(restored);
        delete trashedNotes[key];
      }
      return Promise.resolve();
    }
    case "create_folder":
      // The mock notebook model is flat (no folders) — accepted as a no-op
      // so the action is reachable and honest without faking a folder tree.
      return Promise.resolve();
    case "copy_note": {
      const nb = String(args.notebook);
      const [destNb] = String(args.dest).split("/");
      const n = notebooks[nb]?.find((x) => x.path === args.path);
      if (n && notebooks[destNb]) {
        const copy = { ...n };
        notebooks[destNb].unshift(copy);
        return Promise.resolve(noteInfo(destNb, copy));
      }
      return Promise.reject("destination notebook not found");
    }
    case "move_note": {
      const nb = String(args.notebook);
      const [destNb] = String(args.dest).split("/");
      const n = notebooks[nb]?.find((x) => x.path === args.path);
      if (n && notebooks[destNb]) {
        notebooks[nb] = notebooks[nb].filter((x) => x.path !== args.path);
        notebooks[destNb].unshift(n);
        return Promise.resolve(noteInfo(destNb, n));
      }
      return Promise.reject("destination notebook not found");
    }
    case "daily_note": {
      const nb = String(args.notebook);
      const today = new Date().toISOString().slice(0, 10);
      let n = notebooks[nb]?.find((x) => x.path === `daily-${today}.md`);
      if (!n) {
        n = { path: `daily-${today}.md`, title: `Daily ${today}`, date: today, tags: ["daily"], body: `# ${today}\n\n` };
        notebooks[nb] = notebooks[nb] ?? [];
        notebooks[nb].unshift(n);
      }
      return Promise.resolve(noteInfo(nb, n));
    }
    case "render_note": {
      const n = findNote(String(args.notebook), String(args.path));
      if (!n) return Promise.reject("note not found");
      const r: RenderedNote = { html: renderMd(n.body), root: `/mock/${args.notebook}` };
      return Promise.resolve(r);
    }
    case "search_notes": {
      const q = String(args.query).toLowerCase();
      const out: SearchResult[] = [];
      for (const [nb, list] of Object.entries(notebooks)) {
        if (args.notebook && args.notebook !== nb) continue;
        for (const n of list) {
          if (n.title.toLowerCase().includes(q)) out.push({ notebook: nb, path: n.path, title: n.title, score: 100 });
        }
      }
      return Promise.resolve(out);
    }
    case "git_status": {
      // Non-trivial fixture values (dirty + ahead + behind all nonzero)
      // specifically so the footer's git segment has something to render
      // while testing — an all-zero mock would look identical to "no git
      // segment at all" and hide rendering bugs in that code path.
      const g: GitStatus = { dirty: true, changed: 2, branch: "main", ahead: 1, behind: 0, remote: null };
      return Promise.resolve(g);
    }
    case "git_commit":
      return Promise.resolve("nothing to commit");
    default:
      return Promise.reject(`mockBackend: unhandled command "${cmd}"`);
  }
}

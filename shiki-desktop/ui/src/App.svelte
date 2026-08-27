<script lang="ts">
  import { onMount } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";
  import { highlightPreviewCode } from "./lib/hljsLazy";
  import { api, pickFolder } from "./lib/api";
  import type { NotebookInfo, NoteInfo, RenderedNote, GitStatus } from "./lib/api";
  import Onboarding from "./lib/Onboarding.svelte";
  import type { Component } from "svelte";
  import WhichKey from "./lib/WhichKey.svelte";
  import Dropdown from "./lib/Dropdown.svelte";
  import GlobalSearch from "./lib/GlobalSearch.svelte";
  import Outline from "./lib/Outline.svelte";
  import ThemePicker from "./lib/ThemePicker.svelte";
  import TagsPanel from "./lib/TagsPanel.svelte";
  import GitDashboard from "./lib/GitDashboard.svelte";
  import TasksPanel from "./lib/TasksPanel.svelte";
  import LinksPanel from "./lib/LinksPanel.svelte";
  import HistoryPanel from "./lib/HistoryPanel.svelte";
  import Drawer from "./lib/Drawer.svelte";
  import DiffPanel from "./lib/DiffPanel.svelte";
  import TreeView from "./lib/TreeView.svelte";
  import MetadataPanel from "./lib/MetadataPanel.svelte";
  import LogsPanel from "./lib/LogsPanel.svelte";
  import QueryPanel from "./lib/QueryPanel.svelte";
  import Settings from "./lib/Settings.svelte";
  import type { DiffLineInfo } from "./lib/api";
  import { input, initKeyMaps, handleKey, nextFocus } from "./lib/input.svelte";
  import { dispatchAction } from "./lib/actions";
  import type { ActionHandlers } from "./lib/actions";
  import type { Action, Focus } from "./lib/keymaps";
  import { wordCount, readingTimeMinutes, gitStatusKind, gitStatusSuffix } from "./lib/statusBar";
  import { invalidateWikilinkCache } from "./lib/wikilinkCompletion";

  // ---- app state ----
  let config: any = $state(null);
  let loadError: string = $state("");
  let notebooks: NotebookInfo[] = $state([]);
  let activeNb: string | null = $state(null);
  let notes: NoteInfo[] = $state([]);
  let searchQuery: string = $state("");
  let selectedPath: string | null = $state(null);
  let rendered: RenderedNote | null = $state(null);
  let previewError: string = $state("");
  let editing: boolean = $state(false);
  let editorContent: string = $state("");
  let git: GitStatus | null = $state(null);
  let statusMsg: string = $state("");
  let appVersion: string = $state("");

  // ---- lazy-loaded editor (CodeMirror + all its language/completion
  // extensions is the single biggest chunk in the whole app — deferring it
  // out of the main bundle means browsing/reading notes doesn't pay for it
  // at all until an edit is actually about to happen) ----
  let EditorPane: Component<any> | null = $state(null);
  let editorPanePromise: Promise<Component<any>> | null = null;
  function ensureEditorPane(): Promise<Component<any>> {
    if (!editorPanePromise) {
      editorPanePromise = import("./lib/EditorPane.svelte").then((m) => {
        EditorPane = m.default as Component<any>;
        return EditorPane;
      });
    }
    return editorPanePromise;
  }

  // ---- keyboard navigation state (mirrors shiki-tui's Focus + selection) ----
  let notebooksSelected = $state(0);
  let notesSelected = $state(0);
  let searchInputEl: HTMLInputElement | undefined = $state();
  let previewEl: HTMLDivElement | undefined = $state();

  // ---- view-only toggles (Action::ToggleZenMode / SortNotes / ToggleDates) ----
  let zenMode = $state(false);
  const SORT_ORDERS = ["date-desc", "date-asc", "title-asc", "title-desc"] as const;
  type SortOrder = (typeof SORT_ORDERS)[number];
  let sortOrder: SortOrder = $state("date-desc");
  let showDates = $state(true);

  function toggleZenMode() {
    zenMode = !zenMode;
    if (zenMode) input.focus = "preview"; // the only panel still visible
  }

  function cycleSortOrder() {
    const i = SORT_ORDERS.indexOf(sortOrder);
    sortOrder = SORT_ORDERS[(i + 1) % SORT_ORDERS.length];
    setStatus(`sorted by ${sortOrder.replace("-", " ")}`);
  }

  function toggleShowDates() {
    showDates = !showDates;
  }

  function sortNotesBy(list: NoteInfo[], order: SortOrder): NoteInfo[] {
    const sorted = [...list];
    switch (order) {
      case "date-desc":
        return sorted.sort((a, b) => b.date.localeCompare(a.date));
      case "date-asc":
        return sorted.sort((a, b) => a.date.localeCompare(b.date));
      case "title-asc":
        return sorted.sort((a, b) => a.title.localeCompare(b.title));
      case "title-desc":
        return sorted.sort((a, b) => b.title.localeCompare(a.title));
    }
  }

  function setStatus(msg: string) {
    statusMsg = msg;
    // Same "every status message funnels through one place" convention the
    // TUI's own `App::set_status` has — persisted so a message that
    // already scrolled past the footer is still readable in the logs
    // modal. Fire-and-forget: a failed write shouldn't block anything the
    // status message itself was reporting.
    void api.appendLog(msg);
  }

  function clamp(n: number, lo: number, hi: number): number {
    return Math.max(lo, Math.min(hi, n));
  }

  function setFocus(f: Focus) {
    input.focus = f;
  }

  function moveSelection(delta: number) {
    if (input.focus === "notebooks") {
      if (notebooks.length === 0) return;
      const next = clamp(notebooksSelected + delta, 0, notebooks.length - 1);
      // Also select on a same-index move: the very first j/k press on a
      // freshly loaded panel has next === current (nothing to move to, e.g.
      // a single-item list) but nothing may be selected yet either — that
      // first press must still select, not silently no-op.
      if (next !== notebooksSelected || activeNb !== notebooks[next].name) void selectNotebook(notebooks[next].name);
    } else if (input.focus === "notes") {
      if (filteredNotes.length === 0) return;
      const next = clamp(notesSelected + delta, 0, filteredNotes.length - 1);
      if (next !== notesSelected || selectedPath !== filteredNotes[next].path) void selectNote(filteredNotes[next].path);
    } else {
      previewEl?.scrollBy({ top: delta * 60 });
    }
  }

  function movePage(dir: 1 | -1) {
    if (input.focus === "preview") {
      previewEl?.scrollBy({ top: dir * (previewEl.clientHeight * 0.9 || 400) });
    } else {
      moveSelection(dir * 10);
    }
  }

  function moveSelectionHome() {
    if (input.focus === "notebooks" && notebooks.length) void selectNotebook(notebooks[0].name);
    else if (input.focus === "notes" && filteredNotes.length) void selectNote(filteredNotes[0].path);
    else previewEl?.scrollTo({ top: 0 });
  }

  function moveSelectionEnd() {
    if (input.focus === "notebooks" && notebooks.length) void selectNotebook(notebooks[notebooks.length - 1].name);
    else if (input.focus === "notes" && filteredNotes.length)
      void selectNote(filteredNotes[filteredNotes.length - 1].path);
    else previewEl?.scrollTo({ top: previewEl.scrollHeight });
  }

  function focusForward() {
    if (zenMode || input.mode === "visual") return; // entering/leaving a
    // folder or panel reloads the underlying list out from under the
    // visual anchor — same reasoning the TUI's own Mode::Visual guard has.
    if (input.focus === "notebooks") input.focus = "notes";
    else if (input.focus === "notes" && selectedPath) input.focus = "preview";
  }

  function focusBackward() {
    if (zenMode || input.mode === "visual") return;
    if (input.focus === "preview") input.focus = "notes";
    else if (input.focus === "notes") input.focus = "notebooks";
  }

  function cycleFocus() {
    if (zenMode || input.mode === "visual") return;
    input.focus = nextFocus(input.focus);
  }

  function actionHandlers(): ActionHandlers {
    return {
      NewNotebook: newNotebookPrompt,
      NewNote: () => {
        if (!activeNb) return;
        input.mode = "insert";
        newNoteOpen = true;
      },
      RenameNote: openRename,
      DeleteNote: askDelete,
      DailyNote: () => void daily(),
      JumpSearch: () => searchInputEl?.focus(),
      EditInline: () => {
        if (selectedPath) void openEditor();
      },
      SyncNotebook: () => void gitCommit(),
      PushNotebook: () => void gitCommit(),
      CheckForUpdate: () => void checkForUpdate(),
      ToggleZenMode: toggleZenMode,
      SortNotes: cycleSortOrder,
      ToggleDates: toggleShowDates,
      GlobalSearch: () => (input.overlay = "globalSearch"),
      ShowOutline: () => {
        if (selectedPath) input.overlay = "outline";
      },
      ThemePicker: () => (input.overlay = "themePicker"),
      ToggleTags: () => (input.overlay = "tags"),
      ShowGitDash: () => (input.overlay = "gitDash"),
      PullNotebook: () => void pullNotebook(),
      PullAllNotebooks: () => void pullAllNotebooks(),
      SetRemote: openSetRemote,
      ToggleTasks: () => (input.overlay = "tasks"),
      ShowLinks: () => {
        if (selectedPath) input.overlay = "links";
      },
      ShowHistory: () => {
        if (selectedPath) input.overlay = "history";
      },
      ShowWorkingDiff: () => void showWorkingDiff(),
      UndoDelete: () => void undoDelete(),
      NewFolder: openNewFolder,
      MoveNote: openMoveNote,
      RenameNotebook: openRenameNotebook,
      DeleteNotebook: askDeleteNotebook,
      ToggleDrawer: () => (input.showDrawer = !input.showDrawer),
      EditExternal: () => void openExternalEditor(),
      ToggleFavoriteEditor: () => void toggleFavoriteEditor(),
      ToggleTreeView: () => (input.overlay = "tree"),
      EditMetadata: () => {
        if (selectedPath) input.overlay = "metadata";
      },
      ToggleVisual: toggleVisualMode,
      CopyEntries: openCopyEntries,
      Scratchpad: openScratchpad,
      ShowLogs: () => (input.overlay = "logs"),
      ToggleQuery: () => (input.overlay = "query"),
      ExportNotebook: openExport,
      PublishNotebook: () => void publishNotebook(),
      ToggleSettings: () => (input.showSettings = !input.showSettings),
    };
  }

  function onSettingsSaved(newConfig: any) {
    config = newConfig;
    setStatus("settings saved");
  }

  async function onMetadataSaved() {
    setStatus("metadata saved");
    await refresh();
    if (selectedPath) await selectNote(selectedPath);
  }

  async function onNoteReverted() {
    if (!selectedPath) return;
    setStatus("reverted");
    await refresh();
    await selectNote(selectedPath);
  }

  let diffLines: DiffLineInfo[] = $state([]);

  async function showWorkingDiff() {
    if (!activeNb || !selectedPath) return;
    try {
      const lines = await api.workingDiff(activeNb, selectedPath);
      if (lines.length === 0) {
        // Same "d always answers what changed here, either way" the TUI's
        // own ShowWorkingDiff does — nothing pending means history is the
        // more useful answer than an empty diff popup.
        input.overlay = "history";
        return;
      }
      diffLines = lines;
      input.overlay = "diff";
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function jumpToGlobalHit(notebook: string, path: string) {
    if (notebook !== activeNb) await selectNotebook(notebook);
    await selectNote(path);
    input.focus = "preview";
  }

  async function jumpToNoteInCurrentNotebook(path: string) {
    await selectNote(path);
    input.focus = "preview";
  }

  function jumpToOutlineHeading(index: number) {
    const heading = previewEl?.querySelectorAll("h1, h2, h3, h4, h5, h6")[index];
    heading?.scrollIntoView({ block: "start" });
  }

  function dispatch(action: Action) {
    dispatchAction(action, { handlers: actionHandlers(), setStatus });
  }

  function onWindowKeydown(e: KeyboardEvent) {
    handleKey(e, {
      moveSelection,
      moveSelectionHome,
      moveSelectionEnd,
      movePage,
      focusForward,
      focusBackward,
      cycleFocus,
      dispatch,
      cancelVisual,
    });
  }

  // ---- Visual mode (notes-scope v) ----
  let notesVisualAnchor: number | null = $state(null);

  function toggleVisualMode() {
    if (input.mode === "visual") {
      cancelVisual();
      return;
    }
    if (input.focus !== "notes" || filteredNotes.length === 0) return;
    notesVisualAnchor = notesSelected;
    input.mode = "visual";
  }

  function cancelVisual() {
    if (input.mode !== "visual") return;
    input.mode = "normal";
    notesVisualAnchor = null;
  }

  const visualRange = $derived.by((): [number, number] | null => {
    if (input.mode !== "visual" || notesVisualAnchor === null) return null;
    return [Math.min(notesVisualAnchor, notesSelected), Math.max(notesVisualAnchor, notesSelected)];
  });

  function isInVisualRange(i: number): boolean {
    return visualRange !== null && i >= visualRange[0] && i <= visualRange[1];
  }

  // ---- copy entries (Mode::Visual-only, notes-scope y) ----
  let copyEntriesOpen = $state(false);
  let copyEntriesValue = $state("");
  let copyEntriesPaths: string[] = $state([]);

  function openCopyEntries() {
    if (input.mode !== "visual" || !visualRange || !activeNb) return;
    const [lo, hi] = visualRange;
    copyEntriesPaths = filteredNotes.slice(lo, hi + 1).map((n) => n.path);
    copyEntriesValue = `${activeNb}/`;
    copyEntriesOpen = true;
    cancelVisual();
    input.mode = "insert";
  }

  function closeCopyEntries() {
    copyEntriesOpen = false;
    input.mode = "normal";
  }

  async function confirmCopyEntries() {
    if (!activeNb) return;
    const dest = copyEntriesValue.trim();
    if (!dest) return;
    let ok = 0;
    let failed = 0;
    for (const path of copyEntriesPaths) {
      try {
        await api.copyNote(activeNb, path, dest);
        ok++;
      } catch {
        failed++;
      }
    }
    setStatus(`copied ${ok} note(s) to ${dest}${failed ? `, ${failed} failed` : ""}`);
    closeCopyEntries();
    await refresh();
  }

  const TEMPLATE_OPTIONS = [
    { value: "default", label: "default" },
    { value: "daily", label: "daily" },
    { value: "meeting", label: "meeting" },
    { value: "bug", label: "bug" },
    { value: "spec", label: "spec" },
    { value: "review", label: "review" },
    { value: "postmortem", label: "postmortem" },
    { value: "standup", label: "standup" },
    { value: "retro", label: "retro" },
    { value: "1on1", label: "1on1" },
    { value: "weekly", label: "weekly" },
    { value: "brainstorm", label: "brainstorm" },
    { value: "", label: "(empty)" },
  ];

  const EXPORT_FORMAT_OPTIONS = [
    { value: "html", label: "HTML" },
    { value: "md", label: "Markdown" },
  ];

  // ---- export notebook (leader+x) ----
  let exportOpen = $state(false);
  let exportFormat = $state("html");

  function openExport() {
    if (!activeNb) return;
    exportFormat = "html";
    exportOpen = true;
    input.mode = "insert";
  }

  function closeExport() {
    exportOpen = false;
    input.mode = "normal";
  }

  async function publishNotebook() {
    if (!activeNb) return;
    setStatus(`publishing '${activeNb}' to PDF…`);
    try {
      const path = await api.publishNotebook(activeNb);
      setStatus(`published to ${path}`);
    } catch (e) {
      setStatus(`publish error: ${String(e)}`);
    }
  }

  async function confirmExport() {
    if (!activeNb) return;
    try {
      const path = await api.exportNotebook(activeNb, exportFormat as "html" | "md");
      setStatus(`exported to ${path}`);
      closeExport();
    } catch (e) {
      setStatus(String(e));
    }
  }

  // ---- modal state ----
  let newNoteOpen = $state(false);
  let newNoteTitle = $state("");
  let newNoteTemplate = $state("default");
  let renameOpen = $state(false);
  let renameValue = $state("");
  let confirmOpen = $state(false);
  let confirmAction = $state<() => void>(() => {});
  let confirmLabel = $state("");
  // Every existing call site is a destructive delete and never sets these,
  // so the defaults preserve that behavior exactly — only the folder-import
  // git-init confirmation (a non-destructive "yes, set this up" prompt)
  // overrides them.
  let confirmButtonLabel = $state("Delete");
  let confirmDanger = $state(true);
  let newNotebookOpen = $state(false);
  let newNotebookName = $state("");
  // An array even for a single delete — `Mode::Visual`'s batch delete needs
  // to undo the *whole* batch, same as the TUI's `last_trash: Option<Vec<TrashedEntry>>`.
  let lastDeleted: { notebook: string; path: string; trashPath: string }[] = $state([]);
  let newFolderOpen = $state(false);
  let newFolderName = $state("");
  let moveNoteOpen = $state(false);
  let moveNoteValue = $state("");
  let renameNotebookOpen = $state(false);
  let renameNotebookValue = $state("");

  // ---- scratchpad (leader+p) ----
  let scratchpadOpen = $state(false);
  let scratchpadContent = $state("");
  let scratchpadStagedContent: string | null = $state(null);

  function openScratchpad() {
    scratchpadContent = "";
    scratchpadOpen = true;
    input.mode = "edit";
    void ensureEditorPane();
  }

  function discardScratchpad() {
    scratchpadOpen = false;
    input.mode = "normal";
  }

  function stageScratchpad(content: string) {
    scratchpadOpen = false;
    input.mode = "normal";
    scratchpadStagedContent = content;
    newNoteTitle = "";
    newNoteOpen = true;
    input.mode = "insert";
  }

  const themeName = $derived(config?.theme?.name ?? "…");
  const defaultNotebook = $derived(config?.general?.default_notebook ?? "…");
  const filteredNotes = $derived(
    sortNotesBy(
      searchQuery.trim()
        ? notes.filter(
            (n) =>
              n.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
              n.tags.some((t) => t.toLowerCase().includes(searchQuery.toLowerCase())),
          )
        : notes,
      sortOrder,
    ),
  );

  const footerModeLabel = $derived(
    input.mode === "insert"
      ? "INSERT"
      : input.mode === "edit"
        ? "EDIT"
        : input.mode === "visual"
          ? `VISUAL${visualRange ? ` (${visualRange[1] - visualRange[0] + 1} selected)` : ""}`
          : null,
  );

  const footerMeta = $derived.by(() => {
    if (selectedPath && (input.focus === "notes" || input.focus === "preview") && rendered) {
      // Estimated from the rendered HTML (tags stripped), the same "good
      // enough for an estimate, not a publishing-grade counter" spirit as
      // shiki-tui's own word_count — the desktop app doesn't keep the raw
      // markdown body in memory outside of edit mode, only the rendered
      // preview, so this counts words, not markdown source.
      const text = rendered.html.replace(/<[^>]+>/g, " ");
      const words = wordCount(text);
      const wpm = (config?.general?.reading_wpm as number | undefined) ?? 200;
      return `${text.trim().length} chars · ${words} words · ${readingTimeMinutes(words, wpm)} min read`;
    }
    return `${notes.length} notes`;
  });

  const gitKind = $derived(git ? gitStatusKind(git) : "clean");
  const gitSuffix = $derived(git ? gitStatusSuffix(git) : "");
  const gitColorVar = $derived(
    gitKind === "dirty" ? "warning" : gitKind === "diverged" ? "accent" : "success",
  );

  // ---- theme (also used for the theme picker's live preview) ----
  let themeStyleEl: HTMLStyleElement | undefined;

  async function applyTheme(name?: string) {
    const css = await api.getThemeCss(name);
    if (!themeStyleEl) {
      themeStyleEl = document.createElement("style");
      themeStyleEl.setAttribute("data-shiki-theme", "active");
      document.head.appendChild(themeStyleEl);
    }
    themeStyleEl.textContent = css;
  }

  async function confirmTheme(name: string) {
    try {
      await api.setTheme(name);
      if (config) config.theme = { ...config.theme, name };
      await applyTheme(name);
      setStatus(`theme set: ${name}`);
    } catch (e) {
      setStatus(String(e));
    }
  }

  // ---- lifecycle ----
  onMount(async () => {
    try {
      await applyTheme();

      config = await api.getConfig();
      if (config?.keybindings) initKeyMaps(config.keybindings);
      await loadNotebooks();
      appVersion = await api.getAppVersion();
    } catch (e) {
      loadError = String(e);
    }
    setTimeout(checkForUpdate, 3000);
    // Idle-time preload: the editor chunk isn't needed for the initial
    // browse-notes view at all, but fetching it now (rather than only on
    // first edit) means even the *first* edit is instant instead of eating
    // the import cost — same "load the heavy thing while nothing else is
    // happening" idea as the update check above already using a delay.
    setTimeout(() => void ensureEditorPane(), 1500);
  });

  // ---- notebooks ----
  async function loadNotebooks() {
    notebooks = await api.listNotebooks();
    if (notebooks.length > 0) {
      const want = config?.general?.default_notebook ?? notebooks[0].name;
      const pick = notebooks.find((n) => n.name === want) ?? notebooks[0];
      await selectNotebook(pick.name);
    }
  }

  async function selectNotebook(name: string) {
    activeNb = name;
    selectedPath = null;
    rendered = null;
    editing = false;
    git = null;
    notebooksSelected = Math.max(0, notebooks.findIndex((n) => n.name === name));
    notesSelected = 0;
    notes = await api.listNotes(name);
    try {
      git = await api.gitStatus(name);
    } catch {
      git = null;
    }
  }

  async function newNotebook(name: string) {
    await api.createNotebook(name);
    setStatus(`notebook "${name}" created`);
    await loadNotebooks();
  }

  // ---- notes ----
  async function selectNote(path: string) {
    if (!activeNb) return;
    selectedPath = path;
    editing = false;
    previewError = "";
    notesSelected = Math.max(0, filteredNotes.findIndex((n) => n.path === path));
    try {
      rendered = await api.renderNote(activeNb, path);
      highlightCode();
    } catch (e) {
      rendered = null;
      previewError = String(e);
    }
  }

  function highlightCode() {
    requestAnimationFrame(() => {
      // Fire-and-forget: each fence's language module loads independently
      // and paints in as soon as it's ready, instead of blocking the
      // (synchronous, and far more common) image-src-rewrite + copy-button
      // wiring below on a network/disk-free but still async dynamic import.
      void highlightPreviewCode(document);
      document.querySelectorAll(".preview img").forEach((img) => {
        const src = img.getAttribute("src");
        if (src && !src.startsWith("http") && !src.startsWith("data:") && !src.startsWith("asset:")) {
          img.setAttribute("src", convertFileSrc(src));
        }
      });
      enhanceCodeBlocks();
    });
  }

  // Quick-copy for both fenced (```) and inline (`) code, mirroring what
  // GitHub/VS Code's own rendered-markdown views do — the fast, low-friction
  // way to grab a snippet without first entering edit mode and selecting it
  // by hand. `{@html rendered.html}` is raw server-rendered HTML, so this
  // wires plain DOM listeners onto it (same pattern `highlightCode` already
  // uses for hljs/image-src rewriting) rather than anything Svelte-templated.
  function enhanceCodeBlocks() {
    document.querySelectorAll(".preview pre").forEach((pre) => {
      const el = pre as HTMLElement;
      if (el.querySelector(":scope > .copy-code-btn")) return; // already wired
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "copy-code-btn";
      btn.textContent = "copy";
      btn.title = "Copy code";
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        const text = el.querySelector("code")?.textContent ?? "";
        void navigator.clipboard.writeText(text).then(() => {
          btn.textContent = "copied";
          btn.classList.add("copied");
          setTimeout(() => {
            btn.textContent = "copy";
            btn.classList.remove("copied");
          }, 1200);
        });
      });
      el.appendChild(btn);
    });

    document.querySelectorAll(".preview code").forEach((code) => {
      const el = code as HTMLElement;
      if (el.parentElement?.tagName === "PRE" || el.dataset.copyWired) return; // fenced code: handled above
      el.dataset.copyWired = "1";
      el.classList.add("copyable-inline-code");
      el.title = "Click to copy";
      el.addEventListener("click", (e) => {
        e.stopPropagation();
        void navigator.clipboard.writeText(el.textContent ?? "").then(() => {
          el.classList.add("copied-flash");
          setTimeout(() => el.classList.remove("copied-flash"), 500);
        });
      });
    });
  }

  // TUI parity: a plain click on PREVIEW enters edit mode "everywhere,
  // including on top of a wikilink" (CLAUDE.md) — real `<a>` links, the
  // copy affordances above, and (if ever wired) task checkboxes are the
  // only carve-outs, so clicking them still does their own thing instead of
  // being swallowed into "start editing."
  function handlePreviewClick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (target.closest("a, input, button, summary, .copyable-inline-code")) return;
    void openEditor();
  }

  async function openEditor() {
    if (!activeNb || !selectedPath) return;
    // Same "i opens the favorite editor instead when the setting is on"
    // behavior the TUI has — `E` (openExternalEditor) always uses the
    // statically configured editor regardless of this toggle.
    if (config?.general?.use_favorite_editor) {
      try {
        await api.openFavoriteEditor(activeNb, selectedPath);
        setStatus("opened in favorite editor");
      } catch (e) {
        setStatus(String(e));
      }
      return;
    }
    try {
      // Runs concurrently with the IPC round trip rather than after it —
      // by the time the note content comes back, the editor chunk has
      // usually already finished loading too, so there's no added delay
      // in the common case despite the import being "on demand."
      const [note] = await Promise.all([api.readNote(activeNb, selectedPath), ensureEditorPane()]);
      editorContent = note.content;
      editing = true;
      input.mode = "edit";
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function openExternalEditor() {
    if (!activeNb || !selectedPath) return;
    try {
      await api.openExternalEditor(activeNb, selectedPath);
      setStatus("opened in external editor");
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function toggleFavoriteEditor() {
    try {
      const enabled = await api.toggleFavoriteEditor();
      if (config) config.general = { ...config.general, use_favorite_editor: enabled };
      setStatus(`favorite editor ${enabled ? "on" : "off"}`);
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function saveEditor(content: string) {
    if (!activeNb || !selectedPath) return;
    try {
      await api.saveNote(activeNb, selectedPath, content);
      editing = false;
      input.mode = "normal";
      setStatus("saved ✓");
      await refresh();
      await selectNote(selectedPath);
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function createNote() {
    if (!activeNb) return;
    try {
      const tpl = newNoteTemplate || null;
      const info = await api.createNote(activeNb, newNoteTitle.trim(), tpl, null);
      newNoteOpen = false;
      newNoteTitle = "";
      // Reset here, not left to whichever branch runs next — the
      // scratchpad-staged branch below doesn't call `openEditor()` (which
      // is what used to reset it to "edit"), so mode was silently stuck on
      // "insert" after a scratchpad save with no further keys working.
      input.mode = "normal";
      await refresh();
      await selectNote(info.path);
      if (scratchpadStagedContent !== null) {
        // The scratchpad's own body replaces whatever the template just
        // wrote — Ctrl+S in the scratchpad means "this exact text becomes
        // the note," the title prompt only exists to name/place it.
        await api.saveNote(activeNb, info.path, scratchpadStagedContent);
        scratchpadStagedContent = null;
        await selectNote(info.path);
        setStatus("scratchpad saved as new note");
      } else {
        await openEditor();
      }
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function daily() {
    if (!activeNb) return;
    try {
      const info = await api.dailyNote(activeNb);
      await refresh();
      await selectNote(info.path);
      setStatus("daily note opened");
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function renameNote() {
    if (!activeNb || !selectedPath) return;
    try {
      await api.renameNote(activeNb, selectedPath, renameValue.trim());
      renameOpen = false;
      input.mode = "normal";
      setStatus("renamed (wikilinks updated)");
      await refresh();
      await selectNote(selectedPath);
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function deleteNote() {
    if (!activeNb || !selectedPath) return;
    const trashPath = await api.deleteNote(activeNb, selectedPath);
    lastDeleted = trashPath ? [{ notebook: activeNb, path: selectedPath, trashPath }] : [];
    confirmOpen = false;
    input.mode = "normal";
    selectedPath = null;
    rendered = null;
    setStatus(trashPath ? "note deleted (u to undo)" : "note deleted");
    await refresh();
  }

  async function deleteBatch(paths: string[]) {
    if (!activeNb) return;
    const entries: typeof lastDeleted = [];
    let ok = 0;
    for (const path of paths) {
      const trashPath = await api.deleteNote(activeNb, path);
      if (trashPath) entries.push({ notebook: activeNb, path, trashPath });
      ok++;
    }
    lastDeleted = entries;
    confirmOpen = false;
    input.mode = "normal";
    selectedPath = null;
    rendered = null;
    setStatus(`deleted ${ok} note(s)${entries.length ? " (u to undo)" : ""}`);
    await refresh();
  }

  async function undoDelete() {
    if (lastDeleted.length === 0) {
      setStatus("nothing to undo");
      return;
    }
    const entries = lastDeleted;
    lastDeleted = [];
    try {
      for (const { notebook, path, trashPath } of entries) {
        await api.undoDeleteNote(notebook, path, trashPath);
      }
      setStatus(`restored ${entries.length} note(s)`);
      if (entries.some((e) => e.notebook === activeNb)) {
        await refresh();
        // Re-select the restored note (single-item undo only — a batch
        // restore has no one obvious note to land on) so an immediate
        // follow-up action (edit, external editor, ...) has something to
        // act on instead of silently no-op'ing on a null selection.
        if (entries.length === 1 && entries[0].notebook === activeNb) {
          await selectNote(entries[0].path);
        }
      }
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function refresh() {
    if (!activeNb) return;
    notes = await api.listNotes(activeNb);
    invalidateWikilinkCache();
    try {
      git = await api.gitStatus(activeNb);
    } catch {
      git = null;
    }
  }

  async function gitCommit() {
    if (!activeNb) return;
    try {
      const msg = await api.gitCommit(activeNb, "desktop edit");
      setStatus(msg);
      await refresh();
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function pullNotebook() {
    if (!activeNb) return;
    try {
      setStatus(await api.pullNotebook(activeNb));
      await refresh();
    } catch (e) {
      setStatus(String(e));
    }
  }

  async function pullAllNotebooks() {
    try {
      const results = await api.pullAllNotebooks();
      const failed = results.filter((r) => !r.ok).length;
      setStatus(`pulled ${results.length} notebook(s), ${failed} failed`);
      await refresh();
    } catch (e) {
      setStatus(String(e));
    }
  }

  // ---- set remote modal ----
  let setRemoteOpen = $state(false);
  let setRemoteValue = $state("");

  function openSetRemote() {
    if (!activeNb) return;
    setRemoteValue = git?.remote ?? "";
    setRemoteOpen = true;
    input.mode = "insert";
  }

  function closeSetRemote() {
    setRemoteOpen = false;
    input.mode = "normal";
  }

  async function confirmSetRemote() {
    if (!activeNb) return;
    const url = setRemoteValue.trim();
    if (!url) return;
    try {
      await api.setNotebookRemote(activeNb, url);
      setStatus("remote set");
      closeSetRemote();
      await refresh();
    } catch (e) {
      setStatus(String(e));
    }
  }

  // ---- auto-update (same as before) ----
  let update: any = $state(null);
  let updStatus: "idle" | "checking" | "ready" | "downloading" | "installing" | "done" | "error" =
    $state("idle");
  let updProgress: number = $state(0);
  let updError: string = $state("");

  async function checkForUpdate() {
    updStatus = "checking";
    try {
      const found = await check();
      if (found) {
        update = found;
        updStatus = "ready";
      } else {
        updStatus = "idle";
      }
    } catch {
      updStatus = "idle";
    }
  }

  async function installUpdate() {
    if (!update) return;
    updStatus = "downloading";
    updProgress = 0;
    try {
      await update.downloadAndInstall((event: any) => {
        switch (event.event) {
          case "Progress": {
            const { chunkLength, contentLength } = event.data;
            if (contentLength > 0) updProgress = Math.round((chunkLength / contentLength) * 100);
            break;
          }
          case "Finished":
            updStatus = "installing";
            break;
        }
      });
      updStatus = "done";
    } catch (e) {
      updStatus = "error";
      updError = String(e);
    }
  }

  // ---- modals ----
  function openRename() {
    if (!selectedPath) return;
    const note = notes.find((n) => n.path === selectedPath);
    renameValue = note?.title ?? selectedPath.replace(/\.md$/, "");
    renameOpen = true;
    input.mode = "insert";
  }

  function askDelete() {
    if (input.mode === "visual" && visualRange) {
      const [lo, hi] = visualRange;
      const paths = filteredNotes.slice(lo, hi + 1).map((n) => n.path);
      cancelVisual();
      confirmLabel = `Delete ${paths.length} note(s)? (git history keeps them recoverable)`;
      confirmAction = () => void deleteBatch(paths);
      confirmOpen = true;
      input.mode = "insert";
      return;
    }
    if (!selectedPath) return;
    confirmLabel = "Delete this note? (git history keeps it recoverable)";
    confirmAction = deleteNote;
    confirmOpen = true;
    input.mode = "insert";
  }

  function newNotebookPrompt() {
    newNotebookName = "";
    newNotebookOpen = true;
    input.mode = "insert";
  }

  // Same detection shiki-tui's new-notebook prompt uses to decide whether a
  // pasted value is a git URL to clone rather than a plain name — the
  // desktop's single name field auto-detects this exactly like the TUI's
  // does, so pasting a URL there is enough; no separate "clone" mode to
  // switch into first.
  function looksLikeGitUrl(s: string): boolean {
    return (
      s.startsWith("http://") ||
      s.startsWith("https://") ||
      s.startsWith("git@") ||
      s.startsWith("ssh://") ||
      s.startsWith("git://")
    );
  }
  const newNotebookIsUrl = $derived(looksLikeGitUrl(newNotebookName.trim()));

  async function createNotebookFromModal() {
    const value = newNotebookName.trim();
    if (!value) return;
    if (looksLikeGitUrl(value)) {
      setStatus(`cloning '${value}'…`);
      try {
        const result = await api.createNotebookFromUrl(value);
        setStatus(result.message);
        closeNewNotebook();
        await loadNotebooks();
        await selectNotebook(result.name);
      } catch (e) {
        setStatus(String(e));
      }
      return;
    }
    await newNotebook(value);
    closeNewNotebook();
  }

  function closeNewNotebook() {
    newNotebookOpen = false;
    input.mode = "normal";
  }

  // ---- import an existing folder as a notebook ----
  async function importNotebookFolder() {
    const path = await pickFolder();
    if (!path) return;
    // Close the new-notebook modal first, not after — both it and the
    // confirm dialog are independent `<svelte:window onkeydown>`-driven
    // modals (same shape as Settings/ThemePicker), so leaving both mounted
    // at once risks the exact same double-handler collision fixed there.
    newNotebookOpen = false;
    await adoptNotebookFolder(path, false);
  }

  async function adoptNotebookFolder(path: string, initGitIfMissing: boolean) {
    try {
      const result = await api.adoptNotebookFolder(path, initGitIfMissing);
      if (result.status === "NeedsGitInitConfirm") {
        confirmLabel = `'${path}' has no git repo — initialize one and import it as a notebook?`;
        confirmButtonLabel = "Initialize & Import";
        confirmDanger = false;
        confirmAction = () => void adoptNotebookFolder(path, true);
        confirmOpen = true;
        input.mode = "insert";
        return;
      }
      setStatus(`imported '${result.name}' from ${path}`);
      closeConfirm();
      await loadNotebooks();
      await selectNotebook(result.name);
    } catch (e) {
      setStatus(String(e));
    }
  }

  function openNewFolder() {
    if (!activeNb) return;
    newFolderName = "";
    newFolderOpen = true;
    input.mode = "insert";
  }

  function closeNewFolder() {
    newFolderOpen = false;
    input.mode = "normal";
  }

  async function confirmNewFolder() {
    if (!activeNb) return;
    const name = newFolderName.trim();
    if (!name) return;
    try {
      await api.createFolder(activeNb, name);
      setStatus(`folder "${name}" created`);
      closeNewFolder();
    } catch (e) {
      setStatus(String(e));
    }
  }

  let moveNotePaths: string[] = $state([]);

  function openMoveNote() {
    if (!activeNb) return;
    if (input.mode === "visual" && visualRange) {
      const [lo, hi] = visualRange;
      moveNotePaths = filteredNotes.slice(lo, hi + 1).map((n) => n.path);
      cancelVisual();
    } else {
      if (!selectedPath) return;
      moveNotePaths = [selectedPath];
    }
    moveNoteValue = `${activeNb}/`;
    moveNoteOpen = true;
    input.mode = "insert";
  }

  function closeMoveNote() {
    moveNoteOpen = false;
    input.mode = "normal";
  }

  async function confirmMoveNote() {
    if (!activeNb || moveNotePaths.length === 0) return;
    const dest = moveNoteValue.trim();
    if (!dest) return;
    let ok = 0;
    let failed = 0;
    for (const path of moveNotePaths) {
      try {
        await api.moveNote(activeNb, path, dest);
        ok++;
      } catch {
        failed++;
      }
    }
    setStatus(`moved ${ok} note(s) to ${dest}${failed ? `, ${failed} failed` : ""}`);
    closeMoveNote();
    selectedPath = null;
    rendered = null;
    await refresh();
  }

  function openRenameNotebook() {
    if (!activeNb) return;
    renameNotebookValue = activeNb;
    renameNotebookOpen = true;
    input.mode = "insert";
  }

  function closeRenameNotebook() {
    renameNotebookOpen = false;
    input.mode = "normal";
  }

  async function confirmRenameNotebook() {
    if (!activeNb) return;
    const newName = renameNotebookValue.trim();
    if (!newName || newName === activeNb) {
      closeRenameNotebook();
      return;
    }
    try {
      await api.renameNotebook(activeNb, newName);
      closeRenameNotebook();
      setStatus(`renamed to ${newName}`);
      await loadNotebooks();
      await selectNotebook(newName);
    } catch (e) {
      setStatus(String(e));
    }
  }

  function askDeleteNotebook() {
    if (!activeNb) return;
    const name = activeNb;
    confirmLabel = `Delete notebook "${name}"? This removes it and its git history entirely.`;
    confirmAction = () => void deleteNotebookConfirmed(name);
    confirmOpen = true;
    input.mode = "insert";
  }

  async function deleteNotebookConfirmed(name: string) {
    try {
      await api.deleteNotebook(name);
      confirmOpen = false;
      input.mode = "normal";
      setStatus(`notebook "${name}" deleted`);
      activeNb = null;
      selectedPath = null;
      rendered = null;
      await loadNotebooks();
    } catch (e) {
      setStatus(String(e));
    }
  }

  function closeNewNote() {
    newNoteOpen = false;
    input.mode = "normal";
  }

  function closeRename() {
    renameOpen = false;
    input.mode = "normal";
  }

  function closeConfirm() {
    confirmOpen = false;
    confirmButtonLabel = "Delete";
    confirmDanger = true;
    input.mode = "normal";
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

{#if input.maps}
  <WhichKey maps={input.maps} onDispatch={dispatch} />
{/if}
<GlobalSearch onJump={jumpToGlobalHit} />
<Outline notebook={activeNb} path={selectedPath} onJump={jumpToOutlineHeading} />
<ThemePicker
  initialThemeName={themeName}
  onPreview={(name) => void applyTheme(name)}
  onConfirm={(name) => confirmTheme(name)}
/>
<TagsPanel {notes} onJump={jumpToNoteInCurrentNotebook} />
<GitDashboard {notebooks} />
<TasksPanel onJump={jumpToGlobalHit} />
<LinksPanel notebook={activeNb} path={selectedPath} onJump={jumpToNoteInCurrentNotebook} />
<HistoryPanel notebook={activeNb} path={selectedPath} onReverted={onNoteReverted} />
<DiffPanel lines={diffLines} />
<TreeView notebook={activeNb} onJump={jumpToNoteInCurrentNotebook} />
<MetadataPanel notebook={activeNb} path={selectedPath} onSaved={onMetadataSaved} />
<LogsPanel />
<QueryPanel onJump={jumpToGlobalHit} />
<Settings {config} notebookNames={notebooks.map((n) => n.name)} onSaved={onSettingsSaved} />

<div class="shell">
  {#if loadError}
    <div class="banner banner-error" role="alert">config problem: {loadError}</div>
  {/if}

  {#if update}
    <div class="banner banner-update" role="status">
      {#if updStatus === "ready"}
        <span><b>v{update.version}</b> available</span>
        <button type="button" class="upd-btn" onclick={installUpdate}>Update</button>
      {:else if updStatus === "downloading"}
        <span><b>v{update.version}</b> — downloading… {updProgress}%</span>
        <span class="upd-progress"><span class="upd-bar" style="width:{updProgress}%"></span></span>
      {:else if updStatus === "installing"}
        <span>Installing… shiki will relaunch.</span>
      {:else if updStatus === "done"}
        <span>Updated to v{update.version} ✓</span>
      {:else if updStatus === "error"}
        <span class="upd-error">Update failed: {updError}</span>
      {/if}
    </div>
  {/if}

  {#if notebooks.length === 0 && !loadError}
    <Onboarding onCreate={newNotebook} onImport={importNotebookFolder} />
  {:else}
    <div class="columns" class:zen={zenMode}>
      {#if !zenMode}
        <Drawer {notebooks} {activeNb} onSelect={(name) => void selectNotebook(name)} />
      {/if}
      <!-- Locked (not just visually dimmed) while editing: the TUI's Mode::Edit
           never routes a keystroke anywhere but the editor, so switching notes/
           notebooks mid-edit isn't a real option there either — blocking clicks
           here avoids silently discarding editorContent by racing a reload. -->
      <aside
        class:focused={input.focus === "notebooks"}
        class:zen-hidden={zenMode}
        class:locked={editing}
        onclick={() => !editing && setFocus("notebooks")}
      >
        <h2>NOTEBOOKS</h2>
        <ul>
          {#each notebooks as nb, i (nb.name)}
            <li class:selected={activeNb === nb.name} title={nb.path}>
              <button
                type="button"
                disabled={editing}
                onclick={() => {
                  setFocus("notebooks");
                  notebooksSelected = i;
                  selectNotebook(nb.name);
                }}
              >
                <span class="name">{nb.name}</span>
                {#if nb.encrypted}<span class="lock" title="encrypted">🔒</span>{/if}
              </button>
            </li>
          {/each}
        </ul>
        <button type="button" class="side-btn" disabled={editing} onclick={newNotebookPrompt}>+ notebook</button>
      </aside>

      <section
        class="notes-col"
        class:focused={input.focus === "notes"}
        class:zen-hidden={zenMode}
        class:locked={editing}
        onclick={() => !editing && setFocus("notes")}
      >
        <div class="notes-toolbar">
          <input
            type="search"
            placeholder="filter notes…"
            bind:value={searchQuery}
            bind:this={searchInputEl}
            title="Filter by title or tag"
            disabled={editing}
            onfocus={() => (input.mode = "insert")}
            onblur={() => (input.mode = "normal")}
          />
          <button type="button" class="tool-btn" disabled={editing} onclick={() => { newNoteOpen = true; input.mode = "insert"; }} title="New note">+ note</button>
          <button type="button" class="tool-btn" disabled={editing} onclick={daily} title="Today's daily note">daily</button>
        </div>
        <ul class="notes-list">
          {#each filteredNotes as n, i (n.path)}
            <li
              class:selected={selectedPath === n.path}
              class:visual-selected={isInVisualRange(i)}
              onclick={() => {
                if (editing) return;
                setFocus("notes");
                notesSelected = i;
                selectNote(n.path);
              }}
              ondblclick={() => !editing && openEditor()}
              title={n.tags.join(", ")}
            >
              <div class="note-title">{n.title}</div>
              {#if showDates}
                <div class="note-meta">
                  {n.date}
                  {#if n.tags.length}<span class="tags">{n.tags.slice(0, 3).map((t) => `#${t}`).join(" ")}</span>{/if}
                </div>
              {/if}
            </li>
          {:else}
            <li class="empty">no notes{searchQuery ? ` matching "${searchQuery}"` : ""} — + note</li>
          {/each}
        </ul>
      </section>

      <!-- Only this rect's content is conditional on `editing` — mirrors
           shiki-tui's draw.rs, where NOTEBOOKS/NOTES render unconditionally
           every frame and only the PREVIEW rect swaps to the editor widget. -->
      <main
        class="preview-col"
        class:focused={input.focus === "preview"}
        class:editing
        onclick={() => !editing && setFocus("preview")}
      >
        {#if scratchpadOpen && EditorPane}
          <EditorPane
            value={scratchpadContent}
            title="Scratchpad"
            onSave={stageScratchpad}
            onCancel={discardScratchpad}
            escDiscards
          />
        {:else if editing && EditorPane}
          <EditorPane
            value={editorContent}
            title={selectedPath ?? ""}
            {config}
            notebook={activeNb}
            path={selectedPath}
            onSave={saveEditor}
            onCancel={() => {
              editing = false;
              input.mode = "normal";
            }}
          />
        {:else if scratchpadOpen || editing}
          <!-- Only reachable the very first time an edit is triggered
               before the idle preload (onMount) has finished — every
               later edit already has `EditorPane` resolved by the time
               `editing`/`scratchpadOpen` flips true. -->
          <div class="placeholder">
            <p>Loading editor…</p>
          </div>
        {:else if !selectedPath}
          <div class="placeholder">
            <p>Select a note to preview it.</p>
            <p class="hint">Click a note to preview it, then click anywhere in it to edit — or press Enter on a selection.</p>
          </div>
        {:else if previewError}
          <div class="placeholder">
            <p class="error-text">{previewError}</p>
          </div>
        {:else if rendered}
          <div class="preview-toolbar">
            <button type="button" class="tool-btn primary-btn" onclick={openEditor}>✎ edit</button>
            <button type="button" class="tool-btn" onclick={openRename}>rename</button>
            <button type="button" class="tool-btn danger-btn" onclick={askDelete}>delete</button>
            <span class="spacer"></span>
            {#if git?.dirty}
              <span class="dirty" title="uncommitted changes">● {git.changed} uncommitted</span>
              <button type="button" class="tool-btn" onclick={gitCommit}>commit</button>
            {/if}
          </div>
          <div class="preview scroll" bind:this={previewEl} onclick={handlePreviewClick}>
            <!-- eslint-disable-next-line svelte/no-at-html-tags -->
            {@html rendered.html}
          </div>
        {/if}
      </main>
    </div>
  {/if}

  <footer>
    <div class="footer-left">
      {#if footerModeLabel}
        <span class="footer-mode">{footerModeLabel}</span>
        <span class="sep">│</span>
      {/if}
      <span>{activeNb ?? "-"}</span>
      <span class="sep">│</span>
      <span>{footerMeta}</span>
      {#if git}
        <span class="sep">│</span>
        <span class="footer-git" style="color: var(--{gitColorVar})">{git.branch ?? "?"}{gitSuffix}</span>
      {/if}
      {#if input.leaderPending}
        <span class="sep">│</span>
        <span class="footer-leader">leader…</span>
      {/if}
      {#if statusMsg}
        <span class="sep">│</span>
        <span>{statusMsg}</span>
      {/if}
    </div>
    <div class="footer-right">? help &nbsp;&nbsp;v{appVersion}</div>
  </footer>

  {#if newNoteOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && closeNewNote()}>
      <div class="modal">
        <h3>New note</h3>
        <input
          placeholder="Title"
          bind:value={newNoteTitle}
          onkeydown={(e) => e.key === "Enter" && createNote()}
          autofocus
        />
        <label>Template</label>
        <Dropdown bind:value={newNoteTemplate} options={TEMPLATE_OPTIONS} />
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={createNote} disabled={!newNoteTitle.trim()}>Create</button>
          <button type="button" class="tool-btn" onclick={closeNewNote}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  {#if exportOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && closeExport()}>
      <div class="modal">
        <h3>Export {activeNb} to…</h3>
        <Dropdown bind:value={exportFormat} options={EXPORT_FORMAT_OPTIONS} />
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={confirmExport}>Export</button>
          <button type="button" class="tool-btn" onclick={closeExport}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  {#if renameOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && closeRename()}>
      <div class="modal">
        <h3>Rename note</h3>
        <input
          bind:value={renameValue}
          onkeydown={(e) => e.key === "Enter" && renameNote()}
          autofocus
        />
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={renameNote}>Rename (updates wikilinks)</button>
          <button type="button" class="tool-btn" onclick={closeRename}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  {#if confirmOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && closeConfirm()}>
      <div class="modal">
        <h3>{confirmLabel}</h3>
        <div class="modal-actions">
          <button
            type="button"
            class={confirmDanger ? "danger-btn" : "primary-btn"}
            onclick={confirmAction}>{confirmButtonLabel}</button
          >
          <button type="button" class="tool-btn" onclick={closeConfirm}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  {#if newNotebookOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && closeNewNotebook()}>
      <div class="modal">
        <h3>New notebook</h3>
        <input
          placeholder="Notebook name — or paste a git URL to clone it"
          bind:value={newNotebookName}
          onkeydown={(e) => e.key === "Enter" && createNotebookFromModal()}
          autofocus
        />
        {#if newNotebookIsUrl}
          <p class="nb-hint">Will clone this repo as a new notebook.</p>
        {/if}
        <div class="modal-actions">
          <button
            type="button"
            class="primary-btn"
            onclick={createNotebookFromModal}
            disabled={!newNotebookName.trim()}>{newNotebookIsUrl ? "Clone" : "Create"}</button
          >
          <button type="button" class="tool-btn" onclick={closeNewNotebook}>Cancel</button>
        </div>
        <div class="nb-divider"><span>or</span></div>
        <button type="button" class="tool-btn nb-import-btn" onclick={importNotebookFolder}>
          Import an existing folder…
        </button>
      </div>
    </div>
  {/if}

  {#if renameNotebookOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && closeRenameNotebook()}>
      <div class="modal">
        <h3>Rename notebook</h3>
        <input
          bind:value={renameNotebookValue}
          onkeydown={(e) => e.key === "Enter" && confirmRenameNotebook()}
          autofocus
        />
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={confirmRenameNotebook} disabled={!renameNotebookValue.trim()}>Rename</button>
          <button type="button" class="tool-btn" onclick={closeRenameNotebook}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  {#if setRemoteOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && closeSetRemote()}>
      <div class="modal">
        <h3>Set git remote — {activeNb}</h3>
        <input
          placeholder="git@host:owner/repo.git"
          bind:value={setRemoteValue}
          onkeydown={(e) => e.key === "Enter" && confirmSetRemote()}
          autofocus
        />
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={confirmSetRemote} disabled={!setRemoteValue.trim()}>Set</button>
          <button type="button" class="tool-btn" onclick={closeSetRemote}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  {#if newFolderOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && closeNewFolder()}>
      <div class="modal">
        <h3>New folder</h3>
        <input
          placeholder="Folder name"
          bind:value={newFolderName}
          onkeydown={(e) => e.key === "Enter" && confirmNewFolder()}
          autofocus
        />
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={confirmNewFolder} disabled={!newFolderName.trim()}>Create</button>
          <button type="button" class="tool-btn" onclick={closeNewFolder}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  {#if moveNoteOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && closeMoveNote()}>
      <div class="modal">
        <h3>Move {moveNotePaths.length > 1 ? `${moveNotePaths.length} notes` : "to"}…</h3>
        <input
          placeholder="notebook/folder/path"
          bind:value={moveNoteValue}
          onkeydown={(e) => e.key === "Enter" && confirmMoveNote()}
          autofocus
        />
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={confirmMoveNote} disabled={!moveNoteValue.trim()}>Move</button>
          <button type="button" class="tool-btn" onclick={closeMoveNote}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  {#if copyEntriesOpen}
    <div class="modal-backdrop" onclick={(e) => e.target === e.currentTarget && closeCopyEntries()}>
      <div class="modal">
        <h3>Copy {copyEntriesPaths.length} note(s) to…</h3>
        <input
          placeholder="notebook/folder/path"
          bind:value={copyEntriesValue}
          onkeydown={(e) => e.key === "Enter" && confirmCopyEntries()}
          autofocus
        />
        <div class="modal-actions">
          <button type="button" class="primary-btn" onclick={confirmCopyEntries} disabled={!copyEntriesValue.trim()}>Copy</button>
          <button type="button" class="tool-btn" onclick={closeCopyEntries}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  footer span {
    font-size: 0.8rem;
    color: var(--muted);
  }
  .banner {
    padding: 0.5rem 1rem;
    font-size: 0.85rem;
  }
  .banner-error {
    background: var(--error);
    color: var(--bg);
  }
  .banner-update {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    background: var(--highlight);
    color: var(--fg);
    border-bottom: 1px solid var(--accent);
  }
  .banner-update b {
    color: var(--accent);
  }
  .upd-btn,
  .primary-btn {
    background: var(--accent);
    color: var(--bg);
    border: none;
    border-radius: 4px;
    padding: 0.25rem 0.9rem;
    font-weight: 600;
    cursor: pointer;
  }
  .upd-progress {
    flex: 1;
    height: 8px;
    background: var(--inactive);
    border-radius: 4px;
    overflow: hidden;
  }
  .upd-bar {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s ease;
  }
  .upd-error {
    color: var(--error);
  }
  .columns {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  /* Zen mode forces full-screen single-panel preview, hiding
     NOTEBOOKS/NOTES entirely — same "distraction-free writing" the TUI's
     ToggleZenMode does regardless of terminal size. */
  .zen-hidden {
    display: none !important;
  }
  /* NOTEBOOKS/NOTES stay visible but inert while editing, rather than being
     hidden — matches the TUI's actual constraint (only handle_edit_key sees
     keystrokes in Mode::Edit) without literally removing the panels, which
     is what "no page pops up just to write" means in a mouse-driven app. */
  aside.locked,
  .notes-col.locked {
    opacity: 0.6;
    pointer-events: none;
  }
  aside {
    width: 200px;
    border-right: 2px solid var(--border);
    overflow-y: auto;
    padding: 0.5rem 0;
    display: flex;
    flex-direction: column;
  }
  /* Focused-panel treatment mirrors the TUI's focused (Thick) vs unfocused
     (Plain) border convention, so Tab-driven navigation is legible. */
  aside.focused,
  .notes-col.focused,
  .preview-col.focused {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  /* Entering edit mode drops the ordinary focus outline in favor of a
     slim accent top edge — mirrors style_inline_editor hardcoding
     `focused = true` in the TUI (panel_block's Thick/accent border), but a
     full 4px box around the whole pane read as heavy/loud rather than
     clean; EditorPane's own tab bar (the dot + title) already carries most
     of the "you're editing" signal, so this only needs to be a quiet
     accent, not a shout. */
  .preview-col.editing {
    outline: none;
    box-shadow: inset 0 2px 0 var(--accent);
  }
  aside h2,
  .notes-col h2 {
    font-size: 0.72rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--panel-title);
    margin: 0.4rem 0.9rem;
  }
  aside ul,
  .notes-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  aside li button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 0.4rem 0.9rem;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  aside li button:hover {
    background: color-mix(in srgb, var(--fg) 8%, transparent);
  }
  aside li.selected button {
    background: var(--selection);
    border-left-color: var(--accent);
  }
  .side-btn {
    margin: auto 0.6rem 0.4rem;
    padding: 0.35rem;
    background: transparent;
    border: 1px dashed var(--border);
    border-radius: 4px;
    color: var(--muted);
    cursor: pointer;
  }
  .lock {
    font-size: 0.75rem;
  }
  .notes-col {
    width: 260px;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .notes-toolbar {
    display: flex;
    gap: 0.35rem;
    padding: 0.5rem;
    border-bottom: 1px solid var(--border);
  }
  .notes-toolbar input {
    flex: 1;
    min-width: 0;
    padding: 0.3rem 0.5rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--fg);
    outline: none;
  }
  .notes-toolbar input:focus {
    border-color: var(--accent);
  }
  .tool-btn {
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--fg);
    padding: 0.3rem 0.55rem;
    font-size: 0.78rem;
    cursor: pointer;
    white-space: nowrap;
  }
  .tool-btn:hover {
    border-color: var(--accent);
  }
  .danger-btn {
    background: transparent;
    border: 1px solid var(--error);
    border-radius: 4px;
    color: var(--error);
    padding: 0.3rem 0.55rem;
    font-size: 0.78rem;
    cursor: pointer;
  }
  .notes-list {
    flex: 1;
    overflow-y: auto;
  }
  .notes-list li {
    padding: 0.45rem 0.7rem;
    cursor: pointer;
    border-left: 2px solid transparent;
  }
  .notes-list li:hover {
    background: color-mix(in srgb, var(--fg) 8%, transparent);
  }
  .notes-list li.selected {
    background: var(--selection);
    border-left-color: var(--accent);
  }
  .notes-list li.visual-selected {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border-left-color: var(--accent);
  }
  .note-title {
    font-size: 0.9rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note-meta {
    font-size: 0.72rem;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tags {
    color: var(--tag);
  }
  li.empty {
    padding: 0.7rem;
    font-size: 0.8rem;
    color: var(--muted);
  }
  .preview-col {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  .preview-toolbar {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.5rem;
    border-bottom: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .dirty {
    font-size: 0.78rem;
    color: var(--warning);
  }
  .preview {
    flex: 1;
    overflow-y: auto;
    padding: 1rem 1.4rem;
    font-size: 0.95rem;
    line-height: 1.6;
  }
  .placeholder {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--muted);
  }
  .error-text {
    color: var(--error);
  }
  .hint {
    font-size: 0.8rem;
  }
  /* No background — shiki-tui's own status bar paints none either (spans
     just carry themed fg colors over the terminal's own background), so
     the desktop footer doesn't paint one either. */
  footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 0.35rem 1rem;
    border-top: 1px solid var(--border);
    font-size: 0.78rem;
    overflow: hidden;
    white-space: nowrap;
    color: var(--muted);
  }
  .footer-left {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .footer-right {
    color: var(--muted);
    flex-shrink: 0;
  }
  footer .sep {
    color: var(--muted);
    opacity: 0.6;
  }
  .footer-mode {
    color: var(--accent);
    font-weight: 700;
  }
  .footer-leader {
    color: var(--accent);
    font-weight: 700;
  }
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }
  .modal {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1.2rem 1.4rem;
    min-width: 320px;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .modal h3 {
    margin: 0;
    font-size: 1rem;
    color: var(--accent);
  }
  .modal input {
    padding: 0.45rem 0.6rem;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--fg);
  }
  .modal input:focus {
    border-color: var(--accent);
  }
  .modal label {
    font-size: 0.75rem;
    color: var(--muted);
  }
  .modal-actions {
    display: flex;
    gap: 0.5rem;
    justify-content: flex-end;
  }
  .nb-hint {
    margin: -0.4rem 0 0;
    font-size: 0.78rem;
    color: var(--accent);
  }
  .nb-divider {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    margin: 0.4rem 0;
    color: var(--muted);
    font-size: 0.75rem;
  }
  .nb-divider::before,
  .nb-divider::after {
    content: "";
    flex: 1;
    height: 1px;
    background: var(--border);
  }
  .nb-import-btn {
    width: 100%;
    text-align: center;
  }
</style>
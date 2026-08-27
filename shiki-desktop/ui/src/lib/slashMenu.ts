// `/`-command snippet menu for the CodeMirror editor — desktop mirror of
// shiki-tui's slash menu (`shiki-tui/src/slash_menu.rs` + `App::apply_slash_
// command`). Implemented as a second CM6 completion source (see
// `wikilinkCompletion.ts` for the same pattern) rather than a bespoke popup
// component: CM6's own completion panel already does filtered-list-with-
// live-query, arrow-key navigation, Enter-to-accept, and Esc-to-close, so
// reusing it here keeps both editor menus behaving and looking identical
// instead of maintaining two different popup implementations.
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import type { EditorView } from "@codemirror/view";

export interface SlashCommand {
  trigger: string;
  label: string;
  body: string;
}

function b(trigger: string, label: string, body: string): SlashCommand {
  return { trigger, label, body };
}

/// Verbatim port of `shiki-tui/src/slash_menu.rs::builtins()` — keep the
/// two lists in sync if the TUI's ever changes.
export function builtins(): SlashCommand[] {
  return [
    b("h1", "Heading 1", "# {{cursor}}"),
    b("h2", "Heading 2", "## {{cursor}}"),
    b("h3", "Heading 3", "### {{cursor}}"),
    b("bold", "Bold text", "**{{cursor}}**"),
    b("italic", "Italic text", "*{{cursor}}*"),
    b("code", "Code block", "```\n{{cursor}}\n```"),
    b("math", "Math block", "$$\n{{cursor}}\n$$"),
    b("table", "Table", "| Column | Column |\n| --- | --- |\n| {{cursor}} |  |\n"),
    b("check", "Checklist item", "- [ ] {{cursor}}"),
    b("quote", "Quote", "> {{cursor}}"),
    b("divider", "Divider", "---\n"),
    b("date", "Today's date", "{{date}}"),
    b("tags", "Tags line", "Tags: {{cursor}}"),
    b("frontmatter", "YAML frontmatter block", "---\ntitle: {{title}}\ndate: {{date}}\ntags: []\n---\n{{cursor}}"),
    b("bullet", "Bullet list item", "- {{cursor}}"),
    b("numbered", "Numbered list item", "1. {{cursor}}"),
    b("link", "Link", "[{{cursor}}]()"),
    b("image", "Image", "![{{cursor}}]()"),
    b("note", "Note callout", "> **Note:** {{cursor}}"),
    b("warning", "Warning callout", "> **Warning:** {{cursor}}"),
    b("details", "Collapsible section", "<details>\n<summary>{{cursor}}</summary>\n\n</details>\n"),
  ];
}

export interface SnippetConfigMap {
  [trigger: string]: { label?: string; body: string };
}

/// The user's `[snippets.<trigger>]` overrides/extends the builtins by
/// trigger, case-insensitively — same precedence the TUI gives
/// config-defined snippets over its own built-in list.
export function mergedCommands(userSnippets: SnippetConfigMap | undefined): SlashCommand[] {
  const byTrigger = new Map<string, SlashCommand>();
  for (const cmd of builtins()) byTrigger.set(cmd.trigger.toLowerCase(), cmd);
  for (const [trigger, cfg] of Object.entries(userSnippets ?? {})) {
    byTrigger.set(trigger.toLowerCase(), { trigger, label: cfg.label ?? trigger, body: cfg.body });
  }
  return [...byTrigger.values()];
}

export interface TemplateVars {
  title: string;
  date: string;
  time: string;
  notebook: string;
}

const CURSOR_MARKER = "{{cursor}}";

/// Substitutes `{{title}}`/`{{date}}`/`{{time}}`/`{{notebook}}` and strips
/// `{{cursor}}`, returning the resolved text plus the char offset the
/// marker was at (the end of the text when absent) — mirrors
/// `shiki_core::Template::render` plus the TUI's own cursor-marker handling.
export function renderSnippetBody(body: string, vars: TemplateVars): { text: string; cursorOffset: number } {
  const substituted = body
    .replaceAll("{{title}}", vars.title)
    .replaceAll("{{date}}", vars.date)
    .replaceAll("{{time}}", vars.time)
    .replaceAll("{{notebook}}", vars.notebook);
  const markerIndex = substituted.indexOf(CURSOR_MARKER);
  if (markerIndex === -1) return { text: substituted, cursorOffset: substituted.length };
  return {
    text: substituted.slice(0, markerIndex) + substituted.slice(markerIndex + CURSOR_MARKER.length),
    cursorOffset: markerIndex,
  };
}

export interface SlashSourceOptions {
  commands: () => SlashCommand[];
  vars: () => TemplateVars;
}

export function slashCompletionSource(opts: SlashSourceOptions) {
  return (context: CompletionContext): CompletionResult | null => {
    const line = context.state.doc.lineAt(context.pos);
    const beforeCursor = line.text.slice(0, context.pos - line.from);
    // Anchored at column 0 — a `/` anywhere else on a line (a file path, a
    // fraction) must not pop the menu, same as the TUI's own "`/` at
    // column 1" trigger condition.
    const m = /^\/(\S*)$/.exec(beforeCursor);
    if (!m) return null;

    const query = m[1].toLowerCase();
    const options: Completion[] = opts
      .commands()
      .filter((c) => c.trigger.toLowerCase().includes(query))
      .map((c) => ({
        label: `/${c.trigger}`,
        detail: c.label,
        type: "keyword",
        apply: (view: EditorView, _completion: Completion, from: number, to: number) => {
          const { text, cursorOffset } = renderSnippetBody(c.body, opts.vars());
          view.dispatch({
            changes: { from, to, insert: text },
            selection: { anchor: from + cursorOffset },
          });
        },
      }));

    return { from: line.from, options, filter: false };
  };
}

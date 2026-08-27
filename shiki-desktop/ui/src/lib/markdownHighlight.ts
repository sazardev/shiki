// A theme-aware replacement for `@codemirror/language`'s `defaultHighlightStyle`.
//
// `defaultHighlightStyle` hardcodes actual hex colors (`tags.contentSeparator`
// — the `---` horizontal rule / frontmatter delimiter — is `#219`, a dark
// navy; `tags.meta` is `#404740`, dark olive) tuned for a *light* background.
// None of it reads `--fg`/`--bg`/any theme var, so every one of shiki's
// (mostly dark) themes rendered the editor's own syntax coloring almost
// arbitrarily — confirmed live: `---` was nearly invisible against a dark
// theme's background. This file maps the actual tags `@lezer/markdown`
// emits (verified against `@lezer/markdown/dist/index.js`'s own
// `styleTags` call, not guessed) to the same CSS custom properties the
// rest of the app already themes with, so the editor's colors track
// whichever theme is active exactly like every other panel does — and, as
// a direct side effect, reuses the same var() family `app.css` already maps
// highlight.js classes to for the read-only PREVIEW pane, so a fenced code
// block looks the same whether you're reading or editing it.
import { HighlightStyle } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

export function markdownHighlightStyle() {
  return HighlightStyle.define([
    { tag: t.heading1, color: "var(--accent)", fontWeight: "bold", fontSize: "1.3em" },
    { tag: t.heading2, color: "var(--accent)", fontWeight: "bold", fontSize: "1.18em" },
    { tag: t.heading3, color: "var(--accent)", fontWeight: "bold", fontSize: "1.08em" },
    { tag: [t.heading4, t.heading5, t.heading6], color: "var(--accent)", fontWeight: "bold" },
    { tag: t.strong, fontWeight: "bold", color: "var(--fg)" },
    { tag: t.emphasis, fontStyle: "italic", color: "var(--fg)" },
    { tag: t.strikethrough, textDecoration: "line-through", color: "var(--muted)" },
    { tag: [t.link, t.url], color: "var(--link)" },
    { tag: t.quote, color: "var(--muted)", fontStyle: "italic" },
    { tag: t.list, color: "var(--tag)" },
    { tag: t.monospace, color: "var(--success)" },
    // The `---` horizontal rule / frontmatter delimiter — the exact tag
    // that was rendering nearly invisible before this file existed.
    { tag: t.contentSeparator, color: "var(--border)", fontWeight: "bold" },
    // The markup characters themselves (`#`, `*`, `` ` ``, `>`, `-`, link
    // brackets) — deliberately muted so the *content* reads as the
    // foreground element and the syntax marking it up recedes, the same
    // "structure quiet, content loud" hierarchy the rendered PREVIEW pane
    // has by virtue of not showing raw markup at all.
    { tag: t.processingInstruction, color: "var(--muted)" },
    { tag: t.labelName, color: "var(--tag)" },
    { tag: [t.atom, t.escape, t.character], color: "var(--warning)" },
    { tag: t.string, color: "var(--success)" },
    { tag: t.comment, color: "var(--muted)", fontStyle: "italic" },
    { tag: t.meta, color: "var(--muted)" },
    { tag: t.invalid, color: "var(--error)" },
    // Fenced-code-block embedded-language tokens (via `codeLanguages`) —
    // same var() family as `app.css`'s `.preview pre code .hljs-*` rules.
    { tag: t.keyword, color: "var(--link)" },
    { tag: t.number, color: "var(--warning)" },
    { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--accent)" },
    { tag: [t.typeName, t.className, t.namespace], color: "var(--tag)" },
    { tag: t.operator, color: "var(--fg)" },
  ]);
}

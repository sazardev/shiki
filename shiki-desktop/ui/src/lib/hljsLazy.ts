// Lazy per-language loading for the PREVIEW pane's code-fence highlighting.
//
// `highlight.js/lib/common` (what App.svelte used to import directly)
// bundles all 36 of its languages unconditionally into the app's main JS
// chunk — 2.7MB of language grammars parsed/compiled on every launch, even
// for a note with no code fences at all, or one that only ever uses a
// single language. `@codemirror/language-data` (the *editor's* own fenced-
// code highlighting) already avoids exactly this by dynamically importing
// each language's module only when a fence of that language is actually
// encountered; this mirrors that same pattern for the read-only preview's
// highlight.js path instead of leaving it as the one place still paying
// the eager-bundle cost.
import hljs from "highlight.js/lib/core";

// The exact 36 languages `highlight.js/lib/common` bundles (verified via
// `require("highlight.js/lib/common").listLanguages()`), so switching to
// lazy loading doesn't drop coverage — only common aliases people actually
// type in a fence info-string (```js, ```py, …) are added on top.
const loaders: Record<string, () => Promise<{ default: any }>> = {
  bash: () => import("highlight.js/lib/languages/bash"),
  sh: () => import("highlight.js/lib/languages/bash"),
  shell: () => import("highlight.js/lib/languages/bash"),
  zsh: () => import("highlight.js/lib/languages/bash"),
  c: () => import("highlight.js/lib/languages/c"),
  cpp: () => import("highlight.js/lib/languages/cpp"),
  "c++": () => import("highlight.js/lib/languages/cpp"),
  csharp: () => import("highlight.js/lib/languages/csharp"),
  cs: () => import("highlight.js/lib/languages/csharp"),
  css: () => import("highlight.js/lib/languages/css"),
  diff: () => import("highlight.js/lib/languages/diff"),
  go: () => import("highlight.js/lib/languages/go"),
  golang: () => import("highlight.js/lib/languages/go"),
  graphql: () => import("highlight.js/lib/languages/graphql"),
  ini: () => import("highlight.js/lib/languages/ini"),
  toml: () => import("highlight.js/lib/languages/ini"),
  java: () => import("highlight.js/lib/languages/java"),
  javascript: () => import("highlight.js/lib/languages/javascript"),
  js: () => import("highlight.js/lib/languages/javascript"),
  jsx: () => import("highlight.js/lib/languages/javascript"),
  json: () => import("highlight.js/lib/languages/json"),
  kotlin: () => import("highlight.js/lib/languages/kotlin"),
  kt: () => import("highlight.js/lib/languages/kotlin"),
  less: () => import("highlight.js/lib/languages/less"),
  lua: () => import("highlight.js/lib/languages/lua"),
  makefile: () => import("highlight.js/lib/languages/makefile"),
  make: () => import("highlight.js/lib/languages/makefile"),
  markdown: () => import("highlight.js/lib/languages/markdown"),
  md: () => import("highlight.js/lib/languages/markdown"),
  objectivec: () => import("highlight.js/lib/languages/objectivec"),
  objc: () => import("highlight.js/lib/languages/objectivec"),
  perl: () => import("highlight.js/lib/languages/perl"),
  php: () => import("highlight.js/lib/languages/php"),
  "php-template": () => import("highlight.js/lib/languages/php-template"),
  plaintext: () => import("highlight.js/lib/languages/plaintext"),
  text: () => import("highlight.js/lib/languages/plaintext"),
  python: () => import("highlight.js/lib/languages/python"),
  py: () => import("highlight.js/lib/languages/python"),
  "python-repl": () => import("highlight.js/lib/languages/python-repl"),
  r: () => import("highlight.js/lib/languages/r"),
  ruby: () => import("highlight.js/lib/languages/ruby"),
  rb: () => import("highlight.js/lib/languages/ruby"),
  rust: () => import("highlight.js/lib/languages/rust"),
  rs: () => import("highlight.js/lib/languages/rust"),
  scss: () => import("highlight.js/lib/languages/scss"),
  sql: () => import("highlight.js/lib/languages/sql"),
  swift: () => import("highlight.js/lib/languages/swift"),
  typescript: () => import("highlight.js/lib/languages/typescript"),
  ts: () => import("highlight.js/lib/languages/typescript"),
  tsx: () => import("highlight.js/lib/languages/typescript"),
  vbnet: () => import("highlight.js/lib/languages/vbnet"),
  wasm: () => import("highlight.js/lib/languages/wasm"),
  xml: () => import("highlight.js/lib/languages/xml"),
  html: () => import("highlight.js/lib/languages/xml"),
  htm: () => import("highlight.js/lib/languages/xml"),
  svg: () => import("highlight.js/lib/languages/xml"),
  yaml: () => import("highlight.js/lib/languages/yaml"),
  yml: () => import("highlight.js/lib/languages/yaml"),
};

const pending = new Map<string, Promise<void>>();

async function ensureLanguage(name: string): Promise<void> {
  if (hljs.getLanguage(name)) return;
  const loader = loaders[name];
  if (!loader) return;
  let promise = pending.get(name);
  if (!promise) {
    promise = loader().then((mod) => {
      hljs.registerLanguage(name, mod.default);
    });
    pending.set(name, promise);
  }
  return promise;
}

/// Highlights every `.preview pre code` block under `root`, loading each
/// fence's language module on demand first. A note with no fences (the
/// common case) never triggers a single language import.
export async function highlightPreviewCode(root: ParentNode): Promise<void> {
  const blocks = Array.from(root.querySelectorAll<HTMLElement>(".preview pre code"));
  await Promise.all(
    blocks.map(async (el) => {
      const cls = [...el.classList].find((c) => c.startsWith("language-"));
      const lang = cls?.slice("language-".length).toLowerCase();
      if (lang) await ensureLanguage(lang);
      hljs.highlightElement(el);
    }),
  );
}

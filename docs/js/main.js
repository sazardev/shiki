// Theme list mirrors shiki-config/src/themes/mod.rs::all() exactly (same
// order, same names — alphabetical, `default` last) — "dot" colors are each
// theme's own `accent` value
// (docs/css/styles.css has the full palette per theme; this file only
// needs enough to build/label the swatches and know which ones have a real
// screenshot). 36 of the 37 have a captured PNG in docs/assets/screenshots/
// (scripts/screenshots.sh covers every one; the release job copies each
// theme's wide-01-notebooks capture to a top-level PNG). Only `default` —
// the terminal-inherit theme — stays `screenshot: false` and renders the
// CSS-only #term-fallback mockup, since it has no fixed hex values of its
// own. `screenshot` stays a per-theme flag rather than
// being assumed true for everyone so a future theme added to shiki-config
// without a matching screenshot yet degrades to the CSS-only #term-fallback
// mockup instead of silently
// showing another theme's image under the wrong name.
const THEMES = [
  { id: "Arasaka", label: "Arasaka", dot: "#ff003c", screenshot: true },
  { id: "Blade Runner", label: "Blade Runner", dot: "#ff9e3d", screenshot: true },
  { id: "catppuccin-mocha", label: "Catppuccin Mocha", dot: "#89b4fa", screenshot: true },
  { id: "Cyberpunk 2077", label: "Cyberpunk 2077", dot: "#fcee0a", screenshot: true },
  { id: "Doom", label: "Doom", dot: "#e62525", screenshot: true },
  { id: "dracula", label: "Dracula", dot: "#bd93f9", screenshot: true },
  { id: "Fallout Terminal", label: "Fallout Terminal", dot: "#00ff00", screenshot: true },
  { id: "Ghost in the Shell", label: "Ghost in the Shell", dot: "#2dd4bf", screenshot: true },
  { id: "gruvbox-dark", label: "Gruvbox Dark", dot: "#fabd2f", screenshot: true },
  { id: "gruvbox-dark-hard", label: "Gruvbox Dark Hard", dot: "#fabd2f", screenshot: true },
  { id: "gruvbox-dark-soft", label: "Gruvbox Dark Soft", dot: "#fabd2f", screenshot: true },
  { id: "gruvbox-light", label: "Gruvbox Light", dot: "#b57614", screenshot: true },
  { id: "gruvbox-light-hard", label: "Gruvbox Light Hard", dot: "#b57614", screenshot: true },
  { id: "gruvbox-light-soft", label: "Gruvbox Light Soft", dot: "#b57614", screenshot: true },
  { id: "Halo", label: "Halo", dot: "#6fa55c", screenshot: true },
  { id: "LoL (Ahri)", label: "LoL (Ahri)", dot: "#e8448f", screenshot: true },
  { id: "LoL (Jinx)", label: "LoL (Jinx)", dot: "#ff3da5", screenshot: true },
  { id: "LoL (Teemo)", label: "LoL (Teemo)", dot: "#8bc34a", screenshot: true },
  { id: "Matrix", label: "Matrix", dot: "#00ff41", screenshot: true },
  { id: "monokai", label: "Monokai", dot: "#f92672", screenshot: true },
  { id: "Mr. Robot", label: "Mr. Robot", dot: "#ff3b3b", screenshot: true },
  { id: "nord", label: "Nord", dot: "#88c0d0", screenshot: true },
  { id: "one-dark", label: "One Dark", dot: "#61afef", screenshot: true },
  { id: "Overwatch", label: "Overwatch", dot: "#f99e1a", screenshot: true },
  { id: "Pokémon (Charizard)", label: "Pokémon (Charizard)", dot: "#ff6b35", screenshot: true },
  { id: "Pokémon (Gengar)", label: "Pokémon (Gengar)", dot: "#9d6bff", screenshot: true },
  { id: "Pokémon (Pikachu)", label: "Pokémon (Pikachu)", dot: "#f6c344", screenshot: true },
  { id: "Portal", label: "Portal", dot: "#ff8a3d", screenshot: true },
  { id: "solarized-dark", label: "Solarized Dark", dot: "#268bd2", screenshot: true },
  { id: "Stardew Valley", label: "Stardew Valley", dot: "#6aa84f", screenshot: true },
  { id: "Super Mario", label: "Super Mario", dot: "#e52521", screenshot: true },
  { id: "Super Mario (Luigi)", label: "Super Mario (Luigi)", dot: "#4a9e5c", screenshot: true },
  { id: "Synthwave", label: "Synthwave", dot: "#ff2ec4", screenshot: true },
  { id: "tokyo-night", label: "Tokyo Night", dot: "#7aa2f7", screenshot: true },
  { id: "Tron", label: "Tron", dot: "#00f6ff", screenshot: true },
  { id: "Zelda", label: "Zelda", dot: "#c6a45c", screenshot: true },
  { id: "default", label: "Default", dot: "#8b949e", screenshot: false },
];

const STORAGE_KEY = "shiki-site-theme";
const DEFAULT_THEME = "gruvbox-dark"; // matches ThemeConfig::default() as of shiki 0.8.1+

// Themes with their own hero demo video (`assets/demo/{id}.mp4`, recorded
// with that theme active — see scripts/demo-gif.sh's `THEME` env var). Any
// other theme falls back to the default gruvbox-dark recording. Kept as an
// explicit list rather than probed per-switch so the site never shows a
// stale 404'd video; add a theme here when a recording for it is committed.
const HERO_DEMO_THEMES = [
  "catppuccin-mocha",
  "tokyo-night",
  "Cyberpunk 2077",
  "LoL (Jinx)",
  "Matrix",
];

function applyTheme(themeId) {
  const theme = THEMES.find((t) => t.id === themeId) || THEMES.find((t) => t.id === DEFAULT_THEME);

  document.documentElement.setAttribute("data-theme", theme.id);

  document.querySelectorAll(".swatch").forEach((el) => {
    el.classList.toggle("active", el.dataset.themeId === theme.id);
  });

  // Hero demo video: swap to that theme's own recording when one exists,
  // otherwise the default (gruvbox-dark) demo plays. Only present on the
  // home page; `.hero-demo` is gone on every other page, so guard on it.
  const heroVideo = document.getElementById("hero-screenshot");
  if (heroVideo) {
    const themedDemo = HERO_DEMO_THEMES.includes(theme.id)
      ? `assets/demo/${theme.id}.mp4`
      : "assets/demo.mp4";
    if (heroVideo.getAttribute("src") !== themedDemo) {
      heroVideo.setAttribute("src", themedDemo);
      heroVideo.load();
      heroVideo.play().catch(() => {});
    }
  }

  // Only present on the home page's Themes section — pages like
  // documentation.html apply the chosen theme's colors via the CSS
  // variables above but have no screenshot/fallback preview to update.
  const img = document.getElementById("theme-screenshot");
  const fallback = document.getElementById("term-fallback");
  const title = document.getElementById("theme-screenshot-title");
  const caption = document.getElementById("screenshot-caption");

  if (img && fallback && caption) {
    if (theme.screenshot) {
      img.src = `assets/screenshots/${theme.id}.png`;
      img.hidden = false;
      fallback.hidden = true;
      caption.textContent = "Real screenshot, captured with this exact theme.";
    } else {
      img.hidden = true;
      fallback.hidden = false;
      caption.textContent =
        "Live CSS mockup (a real screenshot for this palette isn't captured yet) — colors are still the exact values shiki uses.";
    }
  }
  if (title) title.textContent = `shiki — theme: ${theme.id}`;

  // documentation.html's per-feature screenshot gallery (only present on
  // that page — guarded per-element, same reasoning as img/fallback/
  // caption above). Every `img[data-shot]` shows whatever theme the
  // visitor picked on the homepage, persisted via localStorage the same
  // way the rest of this function already works, rather than being pinned
  // to one hardcoded theme regardless of preference. `data-shot` holds the
  // exact filename stem scripts/screenshots.sh writes (e.g.
  // "wide-01-notebooks" or "stacked-overview") — no prefix assumed here,
  // since the wide/stacked/single tiers don't share one naming pattern.
  //
  // The HTML deliberately gives these `<img>` tags a tiny transparent
  // data-URI placeholder as `src` (a real image URL is invalid markup
  // without one) instead of a real screenshot path — this function is the
  // only thing that ever points them at an actual screenshot. A hardcoded
  // `src="…/gruvbox-dark/…"` used to sit in the markup as a "default," but
  // the browser starts fetching that the moment it parses the tag, well
  // before this script has a chance to read the saved theme from
  // `localStorage` — for any visitor whose saved theme wasn't
  // gruvbox-dark, that meant downloading both the wrong theme's image
  // *and* the right one, plus a visible flash between them. The data-URI
  // placeholder costs no network request at all (it's inline base64), so
  // this function's own fetch of the correct theme's image is the only one
  // that ever happens.
  document.querySelectorAll("img[data-shot]").forEach((img) => {
    img.src = `assets/screenshots/gallery/${theme.id}/${img.dataset.shot}.png`;
  });

  try {
    localStorage.setItem(STORAGE_KEY, theme.id);
  } catch (e) {
    // localStorage can throw in private-browsing/blocked-storage contexts —
    // theme switching still works for the current page view either way.
  }
}

function buildSwatches() {
  const container = document.getElementById("theme-swatches");
  if (!container) return; // this script is shared across pages — not every page has a switcher
  THEMES.forEach((theme) => {
    const btn = document.createElement("button");
    btn.className = "swatch";
    btn.type = "button";
    btn.dataset.themeId = theme.id;
    btn.innerHTML = `<span class="dot" style="background:${theme.dot}"></span>${theme.label}`;
    btn.addEventListener("click", () => applyTheme(theme.id));
    container.appendChild(btn);
  });
}

function initTheme() {
  let saved = null;
  try {
    saved = localStorage.getItem(STORAGE_KEY);
  } catch (e) {
    // ignore — fall through to the default
  }
  applyTheme(saved || DEFAULT_THEME);
}

// ---------------------------------------------------------------------------
// Templates: contents copied verbatim from shiki-core/src/templates.rs's
// `ensure_defaults()` list (same filename, same literal `{{title}}`/
// `{{date}}` placeholders, unsubstituted) — same "keep the marketing site's
// copy in sync with the source of truth" convention the theme palettes and
// documentation.html's reference tables already follow. If a template is
// added, removed, or its contents change there, update this array too.
// ---------------------------------------------------------------------------

const TEMPLATES = [
  // The picker's actual first entry (`open_template_picker`, shiki-tui/src/
  // key_handlers.rs) is "(blank, no template)", not the `default.md` file —
  // an empty body and `frontmatter.template` left `None`/`null`, same as
  // every other real note. This chip mirrors that, not `default.md`.
  { id: "blank", label: "Blank", exampleTitle: "Book recommendations", content: "", noTemplate: true },
  {
    id: "daily",
    label: "Daily",
    // Real daily notes are titled "{date} Daily" (shiki-core/src/daily.rs's
    // `create_or_open`) — shortened to just "Daily" here since the date
    // already appears in its own `date:` field right above. The template
    // body only ever substitutes `{{date}}`, not `{{title}}`, so the H1 and
    // the frontmatter title genuinely don't match anyway, unlike every
    // other template here.
    exampleTitle: "Daily",
    content: "# {{date}}\n\n## Tasks\n\n- [ ] \n\n## Notes\n\n",
  },
  {
    id: "meeting",
    label: "Meeting",
    exampleTitle: "Sprint planning",
    content:
      "# {{title}}\n\nDate: {{date}}\n\n## Attendees\n\n## Agenda\n\n## Notes\n\n## Action Items\n\n",
  },
  {
    id: "standup",
    label: "Standup",
    exampleTitle: "Daily standup",
    content: "# {{title}}\n\nDate: {{date}}\n\n## Yesterday\n\n## Today\n\n## Blockers\n\n",
  },
  {
    id: "retro",
    label: "Retro",
    exampleTitle: "Sprint 12 retro",
    content:
      "# {{title}}\n\nDate: {{date}}\n\n## What Went Well\n\n## What Didn't Go Well\n\n## Action Items\n\n- [ ] \n",
  },
  {
    id: "1on1",
    label: "1:1",
    exampleTitle: "1-on-1 with Alex",
    content:
      "# {{title}}\n\nDate: {{date}}\n\n## Talking Points\n\n## Notes\n\n## Action Items\n\n- [ ] \n",
  },
  {
    id: "bug",
    label: "Bug",
    exampleTitle: "Login button unresponsive on mobile",
    content:
      "# {{title}}\n\nDate: {{date}}\nSeverity: \nStatus: Open\n\n## Summary\n\n## Steps to Reproduce\n\n1. \n2. \n3. \n\n## Expected Behavior\n\n## Actual Behavior\n\n## Environment\n\n- \n\n## Fix Notes\n\n",
  },
  {
    id: "spec",
    label: "Spec",
    exampleTitle: "Notebook encryption",
    content:
      "# {{title}}\n\nDate: {{date}}\nStatus: Draft\n\n## Problem\n\n## Goals\n\n## Non-Goals\n\n## Proposal\n\n## Alternatives Considered\n\n## Open Questions\n\n",
  },
  {
    id: "postmortem",
    label: "Postmortem",
    exampleTitle: "API outage, August 3",
    content:
      "# {{title}}\n\nDate: {{date}}\nSeverity: \nStatus: Draft\n\n## Summary\n\n## Timeline\n\n## Root Cause\n\n## Impact\n\n## Action Items\n\n- [ ] \n\n## Lessons Learned\n\n",
  },
  {
    id: "review",
    label: "Review",
    exampleTitle: "Streaming export pull request",
    content:
      "# {{title}}\n\nDate: {{date}}\nPR/MR: \n\n## Summary\n\n## Comments\n\n## Decision\n\n- [ ] Approved\n- [ ] Changes requested\n",
  },
  {
    id: "weekly",
    label: "Weekly",
    exampleTitle: "Week 32 update",
    content:
      "# {{title}}\n\nWeek of: {{date}}\n\n## Highlights\n\n## Metrics\n\n## Challenges\n\n## Next Week Priorities\n\n- [ ] \n",
  },
  {
    id: "brainstorm",
    label: "Brainstorm",
    exampleTitle: "Onboarding flow ideas",
    content:
      "# {{title}}\n\nDate: {{date}}\n\n## Problem / Prompt\n\n## Ideas\n\n- \n\n## Next Steps\n\n- [ ] \n",
  },
];

const DEFAULT_TEMPLATE_ID = "meeting";
// Fixed example values a note created "right now" from one of these
// templates would actually get — matches `Frontmatter::new` (shiki-core/
// src/note.rs) plus the vars `create_note_with_template` substitutes
// (shiki-tui/src/key_handlers.rs): `tags`/`links` default to `[]`,
// `notebook` is whichever notebook is selected, and `template` is set to
// the template's own filename stem once it's actually applied.
const TEMPLATE_EXAMPLE_DATE = "2026-08-10";
const TEMPLATE_EXAMPLE_NOTEBOOK = "personal";

// Same `{{key}}` substitution `Template::render` does (shiki-core/src/
// templates.rs) — a single left-to-right pass, unknown placeholders left
// untouched. None of the 12 built-in templates contain a value that itself
// looks like `{{...}}`, so the simpler non-resubstituting-safe approach
// there isn't needed here.
function renderTemplateVars(content, vars) {
  return content.replace(/\{\{(\w+)\}\}/g, (match, key) =>
    Object.prototype.hasOwnProperty.call(vars, key) ? vars[key] : match
  );
}

// Builds the exact file shiki would write for this template — real YAML
// frontmatter (field order matches `Frontmatter`'s own declaration order:
// title, date, tags, notebook, links, template) followed by `---\n\n` and
// the rendered body, mirroring `Note::to_file_contents` byte-for-byte.
function renderNoteExample(template) {
  const vars = {
    title: template.exampleTitle,
    date: TEMPLATE_EXAMPLE_DATE,
    time: "09:30",
    notebook: TEMPLATE_EXAMPLE_NOTEBOOK,
  };
  const body = renderTemplateVars(template.content, vars);
  const frontmatter = [
    `title: ${template.exampleTitle}`,
    `date: ${TEMPLATE_EXAMPLE_DATE}`,
    `tags: []`,
    `notebook: ${TEMPLATE_EXAMPLE_NOTEBOOK}`,
    `links: []`,
    `template: ${template.noTemplate ? "null" : template.id}`,
  ].join("\n");
  return { frontmatter, body };
}

function applyTemplatePreview(templateId) {
  const template = TEMPLATES.find((t) => t.id === templateId) || TEMPLATES.find((t) => t.id === DEFAULT_TEMPLATE_ID);

  document.querySelectorAll(".template-chip").forEach((el) => {
    el.classList.toggle("active", el.dataset.templateId === template.id);
  });

  const title = document.getElementById("template-preview-title");
  if (title) title.textContent = `shiki — template: ${template.id}`;

  const pre = document.getElementById("template-preview-content");
  if (pre) {
    const { frontmatter, body } = renderNoteExample(template);
    pre.innerHTML =
      `<span class="tpl-frontmatter">---\n${escapeHtml(frontmatter)}\n---</span>` +
      `\n\n<span class="tpl-body">${escapeHtml(body)}</span>`;
  }
}

function buildTemplateChips() {
  const container = document.getElementById("template-chips");
  if (!container) return; // this script is shared across pages — not every page has the picker
  TEMPLATES.forEach((template) => {
    const btn = document.createElement("button");
    btn.className = "template-chip";
    btn.type = "button";
    btn.dataset.templateId = template.id;
    btn.textContent = template.label;
    btn.addEventListener("click", () => applyTemplatePreview(template.id));
    container.appendChild(btn);
  });
  applyTemplatePreview(DEFAULT_TEMPLATE_ID);
}

// ---------------------------------------------------------------------------
// Changelog: fetched live from CHANGELOG.md on `main` rather than duplicated
// by hand into this page, so it can never go stale relative to the repo.
// Only a small hand-rolled subset of Keep a Changelog's actual format is
// parsed here (## version headers, ### category headers, - bullets, `code`,
// **bold**, [text](url) links) — intentionally not a general markdown
// library, since this only ever needs to render shiki's own CHANGELOG.md.
// ---------------------------------------------------------------------------

const CHANGELOG_URL = "https://raw.githubusercontent.com/sazardev/shiki/main/CHANGELOG.md";
const CHANGELOG_MAX_VERSIONS = 5;
// The version-pill popover (see initVersionPopover below) shows fewer
// entries than the full on-page section — it's a quick "what's new" glance
// anchored to the pill, not a replacement for the full changelog.
const CHANGELOG_POPOVER_MAX_VERSIONS = 3;

// Both the full changelog section and the popover render the same
// CHANGELOG.md — fetched once and cached (module-level promise, not just a
// variable) so opening the popover on the homepage doesn't re-fetch what
// loadChangelog() already pulled, and two rapid popover toggles before the
// first fetch resolves don't fire a second request either.
let changelogMarkdownPromise = null;
function fetchChangelogMarkdown() {
  if (!changelogMarkdownPromise) {
    changelogMarkdownPromise = fetch(CHANGELOG_URL).then((res) => {
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return res.text();
    });
  }
  return changelogMarkdownPromise;
}

function escapeHtml(str) {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function renderInline(text) {
  let out = escapeHtml(text);
  out = out.replace(/`([^`]+)`/g, "<code>$1</code>");
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  out = out.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
  return out;
}

// The per-version pages this links to live at docs/changelog/{version}.html
// — from every other page (index.html, changelog.html, documentation.html,
// all at the docs/ root) that's reached via a "changelog/" prefix, but from
// *inside* one of those pages itself it has to be a same-directory link
// instead, or the browser would resolve it to a nonexistent
// changelog/changelog/{version}.html. main.js is shared verbatim across all
// of these pages, so this has to be resolved from the live URL rather than
// hardcoded one way.
function changelogPermalinkPrefix() {
  return /\/changelog\/[^/]+\.html$/.test(window.location.pathname) ? "" : "changelog/";
}

function renderChangelog(markdown, maxVersions = CHANGELOG_MAX_VERSIONS) {
  const lines = markdown.split("\n");
  let html = "";
  let shownVersions = 0;
  let inList = false;
  let skipping = false;
  // Continuation lines of a wrapped bullet are accumulated here as *raw*
  // markdown and only run through `renderInline` once the bullet is known
  // to be complete — CHANGELOG.md hand-wraps long bullets at ~100 columns,
  // sometimes splitting a single `code span` or **bold** run across two
  // physical lines, and matching backtick/asterisk pairs line-by-line
  // (the previous approach) can't see across that line break.
  let pendingBullet = null;
  // A version header (`## [x.y.z]`) is held here instead of appended to
  // `html` immediately — flushed only once real content (a category header
  // or a bullet) actually follows it. This is what keeps a currently-empty
  // `## [Unreleased]` from rendering as a bare, content-less heading: if
  // the *next* version header arrives before this one was ever flushed,
  // it's simply discarded, and an empty section never counts against
  // `maxVersions` either (so the cap always shows that many real entries,
  // not fewer because one slot went to an empty section).
  let pendingHeaderHtml = null;

  const flushHeader = () => {
    if (pendingHeaderHtml !== null) {
      html += pendingHeaderHtml;
      shownVersions += 1;
      pendingHeaderHtml = null;
    }
  };

  const closeList = () => {
    flushBullet();
    if (inList) {
      html += "</ul>";
      inList = false;
    }
  };

  const flushBullet = () => {
    if (pendingBullet !== null) {
      html += `<li>${renderInline(pendingBullet)}</li>`;
      pendingBullet = null;
    }
  };

  for (const rawLine of lines) {
    const line = rawLine.trimEnd();

    const versionMatch = line.match(/^##\s+\[([^\]]+)\]\s*(-\s*(.+))?/);
    if (versionMatch) {
      closeList();
      pendingHeaderHtml = null; // the previous section never got real content — drop its heading
      if (shownVersions >= maxVersions) {
        skipping = true;
        continue;
      }
      skipping = false;
      const date = versionMatch[3] ? `<span class="cl-date"> — ${escapeHtml(versionMatch[3])}</span>` : "";
      // "Unreleased" has no generated share page (scripts/generate_release_pages.py
      // only ever runs against real, tagged versions) — no permalink for it.
      const versionName = versionMatch[1];
      const permalink =
        versionName.toLowerCase() === "unreleased"
          ? ""
          : ` <a class="cl-permalink" href="${changelogPermalinkPrefix()}${encodeURIComponent(versionName)}.html" title="Permalink to this release">#</a>`;
      pendingHeaderHtml = `<h3>${escapeHtml(versionName)}${date}${permalink}</h3>`;
      continue;
    }

    if (skipping) continue;

    const categoryMatch = line.match(/^###\s+(.+)/);
    if (categoryMatch) {
      closeList();
      flushHeader();
      html += `<h4>${escapeHtml(categoryMatch[1])}</h4>`;
      continue;
    }

    const bulletMatch = line.match(/^-\s+(.+)/);
    if (bulletMatch) {
      flushBullet();
      flushHeader();
      if (!inList) {
        html += "<ul>";
        inList = true;
      }
      pendingBullet = bulletMatch[1];
      continue;
    }

    // A continuation line of a multi-line bullet (indented, no leading `-`)
    // extends the raw text of the bullet still being accumulated, instead
    // of being rendered and appended on its own.
    if (inList && pendingBullet !== null && line.trim().length > 0 && /^\s/.test(rawLine)) {
      pendingBullet += ` ${line.trim()}`;
      continue;
    }

    if (line.trim().length === 0) {
      closeList();
    }
  }
  closeList();
  return html || "<p>No changelog entries found.</p>";
}

// changelog.html's full history is a long scroll (22+ versions and growing) —
// this builds the "Jump to version" <select> next to the hero so a visitor
// looking for one specific release doesn't have to scroll past every other
// one to find it. Only changelog.html has the #version-jump container (the
// homepage teaser and the popover are both short enough not to need it), so
// this is a no-op everywhere else, same guard-on-existence convention every
// other per-page init function here already follows.
//
// It jumps straight to that version's dedicated permalink page
// (changelog/{version}.html — see scripts/generate_release_pages.py) rather
// than an in-page anchor: those pages already exist and are the one
// "canonical" place for a single version, so this doesn't need a second,
// parallel in-page-anchor navigation scheme alongside them.
function renderVersionJump(markdown) {
  const wrap = document.getElementById("version-jump");
  const select = document.getElementById("version-jump-select");
  if (!wrap || !select) return;

  const versions = [];
  const re = /^##\s+\[([^\]]+)\]\s*(?:-\s*(.+))?$/gm;
  let match;
  while ((match = re.exec(markdown))) {
    const name = match[1];
    if (name.toLowerCase() === "unreleased") continue;
    versions.push({ name, date: match[2] || "" });
  }
  if (versions.length === 0) return;

  const options = versions.map(
    (v) => `<option value="${escapeHtml(v.name)}">v${escapeHtml(v.name)}${v.date ? ` — ${escapeHtml(v.date)}` : ""}</option>`
  );
  select.innerHTML = `<option value="">Jump to version…</option>${options.join("")}`;
  select.addEventListener("change", () => {
    if (select.value) window.location.href = `changelog/${encodeURIComponent(select.value)}.html`;
  });
  wrap.hidden = false;
}

async function loadChangelog() {
  const container = document.getElementById("changelog-content");
  if (!container) return; // this script is shared across pages — not every page has a changelog
  // changelog.html sets data-full to render every version instead of the
  // CHANGELOG_MAX_VERSIONS-capped teaser other pages don't show anymore.
  const maxVersions = container.hasAttribute("data-full") ? Infinity : CHANGELOG_MAX_VERSIONS;
  try {
    const text = await fetchChangelogMarkdown();
    container.innerHTML = renderChangelog(text, maxVersions);
    renderVersionJump(text);
  } catch (err) {
    container.innerHTML = `<p class="changelog-error">Couldn't load the live changelog right now. See it directly on <a href="https://github.com/sazardev/shiki/blob/main/CHANGELOG.md">GitHub</a>.</p>`;
  }
}

// ---------------------------------------------------------------------------
// Version-pill popover — clicking the "latest: vX.Y.Z" chip in the nav (see
// loadLatestRelease below, which fills in its text) drops down a small card
// with the most recent changelog entries, instead of the chip being a dead
// label. Present on every page that includes this script (the chip itself
// is in the shared nav markup), unlike the full changelog section, which
// only exists on index.html — so this is the one place every page gets a
// real "what's new" glance, not just the homepage.
// ---------------------------------------------------------------------------

function initVersionPopover() {
  const wrap = document.getElementById("version-chip-wrap");
  const button = document.getElementById("version-pill");
  const popover = document.getElementById("version-popover");
  const content = document.getElementById("version-popover-content");
  if (!wrap || !button || !popover || !content) return null; // shared script — not every page/state has all four

  let loaded = false;

  const open = () => {
    popover.hidden = false;
    button.setAttribute("aria-expanded", "true");
    if (!loaded) {
      loaded = true;
      fetchChangelogMarkdown()
        .then((text) => {
          content.innerHTML = renderChangelog(text, CHANGELOG_POPOVER_MAX_VERSIONS);
        })
        .catch(() => {
          loaded = false; // a later open() retries instead of being stuck on the error forever
          content.innerHTML = `<p class="changelog-error">Couldn't load the live changelog right now. See it directly on <a href="https://github.com/sazardev/shiki/blob/main/CHANGELOG.md">GitHub</a>.</p>`;
        });
    }
  };

  const close = () => {
    popover.hidden = true;
    button.setAttribute("aria-expanded", "false");
  };

  button.setAttribute("aria-expanded", "false");
  button.addEventListener("click", () => (popover.hidden ? open() : close()));

  // Click-outside and Escape both close it — the same pair of dismissal
  // gestures a native <select>/menu supports, so it doesn't feel like a
  // half-built widget next to the rest of the browser's own UI.
  document.addEventListener("click", (e) => {
    if (!popover.hidden && !wrap.contains(e.target)) close();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !popover.hidden) {
      close();
      button.focus();
    }
  });

  // Exposed so `initNavToggle` can close this popover when the mobile nav
  // itself closes — otherwise `popover.hidden` only goes visually true via
  // the ancestor `<nav>` getting `display: none`, while this closure's own
  // `hidden`/`aria-expanded` state stays stale at "open", reappearing
  // already-open next time the hamburger opens and reporting a mismatched
  // expanded state to screen readers in the meantime.
  return { close };
}

// ---------------------------------------------------------------------------
// Live "latest version" + smart download — fetched from the GitHub Releases
// API on every page load rather than hardcoded, so a new tagged release
// shows up here with no site redeploy at all (the same reasoning as the
// live changelog fetch above). `.github/workflows/release.yml`'s
// `update-screenshots` job is what keeps the *screenshots* themselves
// current after each release; this is the text/link counterpart for the
// version number and the download button's target.
//
// One fetch drives both the version pill and the download button, rather
// than two separate calls that could race and stomp on each other's DOM
// writes (and to stay under GitHub's unauthenticated API rate limit).
// ---------------------------------------------------------------------------

const LATEST_RELEASE_URL = "https://api.github.com/repos/sazardev/shiki/releases/latest";

// Substrings matched against each release asset's filename (the same
// `{target}` triple release.yml's build matrix names them with). Detecting
// genuine Apple Silicon vs. Intel from the main thread isn't reliable —
// Safari/Chrome under Rosetta both still report "Intel" — so every Mac
// visitor defaults to Apple Silicon (what's shipped since late 2020); an
// Intel Mac visitor still has the explicit "Intel" pick in the Install
// section below.
const PLATFORM_ASSETS = {
  windows: { match: "x86_64-pc-windows-msvc", label: "Download for Windows" },
  linux: { match: "x86_64-unknown-linux-gnu", label: "Download for Linux" },
  macArm: { match: "aarch64-apple-darwin", label: "Download for macOS (Apple Silicon)" },
};

function detectPlatformKey() {
  const ua = navigator.userAgent || "";
  const platform = navigator.platform || "";
  if (/Win/i.test(ua) || /Win/i.test(platform)) return "windows";
  if (/Mac/i.test(ua) || /Mac|iPhone|iPad/i.test(platform)) return "macArm";
  if (/Linux|X11/i.test(ua) || /Linux/i.test(platform)) return "linux";
  return null;
}

// ---------------------------------------------------------------------------
// Hero "quick install" command — same OS detection as the download button
// above, but for the copy-paste package-manager command instead of the
// binary download link. Only macOS and Windows have a real package manager
// shiki actually ships to (Homebrew, Scoop); Linux has no single universal
// one (Arch's `yay -S shiki-bin` already gets its own card further down the
// page), so `cargo install shiki-cli` — already the one truly
// platform-agnostic install path — stays the quick-install command there
// and detection failing entirely (unknown UA) falls back to it too.
// ---------------------------------------------------------------------------

const QUICK_INSTALL = {
  windows: {
    label: "Scoop",
    command: "scoop bucket add sazardev https://github.com/sazardev/shiki\nscoop install shiki",
  },
  macArm: {
    label: "Homebrew",
    command: "brew install sazardev/shiki/shiki",
  },
};

function initQuickInstall() {
  const label = document.getElementById("quick-install-label");
  const cmd = document.getElementById("quick-install-cmd");
  const altLine = document.getElementById("install-alt-line");
  if (!cmd) return; // this script is shared across pages — not every page has the hero install block

  const preset = QUICK_INSTALL[detectPlatformKey()];
  if (preset) {
    cmd.textContent = preset.command;
    if (label) label.textContent = `Quick install (${preset.label}):`;
    // Cargo is the fallback shown as an "optional" secondary line only once
    // the primary command above has actually become something else — with
    // no override (Linux/unknown), it's already the primary, so a second
    // "or cargo install" line right under it would just repeat itself.
    if (altLine) altLine.hidden = false;
  }
}

// A hung (not failed) request otherwise never resolves or rejects — the
// existing try/catch only guards against a fetch that actually errors out or
// a non-ok status, neither of which fires for a connection that just sits
// open. 8s is generous for a single small JSON response.
const FETCH_TIMEOUT_MS = 8000;

function fetchWithTimeout(url, timeoutMs = FETCH_TIMEOUT_MS) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  return fetch(url, { signal: controller.signal }).finally(() => clearTimeout(timer));
}

async function loadLatestRelease() {
  const downloadBtn = document.getElementById("download-btn");
  const pill = document.getElementById("version-pill");

  try {
    const res = await fetchWithTimeout(LATEST_RELEASE_URL);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    const tag = data.tag_name; // e.g. "v0.8.1"
    if (!tag) return;

    if (pill) {
      pill.textContent = `latest: ${tag}`;
      pill.hidden = false;
    }

    if (!downloadBtn) return;
    downloadBtn.textContent = `Download ${tag}`;

    const platformKey = detectPlatformKey();
    const target = platformKey ? PLATFORM_ASSETS[platformKey] : null;
    const asset = target
      ? (data.assets || []).find((a) => a.name.includes(target.match))
      : null;
    if (asset) {
      // A direct link to the actual binary archive — clicking this
      // downloads the file immediately, unlike the static fallback (a link
      // to the /releases/latest *page*, which still requires picking the
      // right asset by hand).
      downloadBtn.href = asset.browser_download_url;
      downloadBtn.textContent = target.label;
    }
  } catch (err) {
    // Silent failure — the button already has a sensible static fallback
    // (GitHub's own "latest" redirect page), so an offline visitor or a
    // GitHub API rate limit just means one less convenience, not a dead
    // button.
  }
}

// ---------------------------------------------------------------------------
// Contributors — pulled live from the GitHub API (commits, PRs, issues) so
// this list can never go stale or need hand-editing, same "fetch on load"
// approach as the changelog/release sections above. The repo owner and any
// AI/bot commit authors are deliberately excluded: this section exists to
// thank *outside* contributors, not the maintainer or their tooling. A
// contributor who's only filed an issue (no commits/PRs yet) still earns a
// spot — reporting a real bug is a contribution too.
// ---------------------------------------------------------------------------

const CONTRIBUTORS_EXCLUDE = new Set(["sazardev", "claude"]);
const MIN_AVATAR_PX = 64;
const MAX_AVATAR_PX = 128;

function isExcludedContributor(login, type) {
  if (type === "Bot") return true;
  const lower = login.toLowerCase();
  return CONTRIBUTORS_EXCLUDE.has(lower) || lower.endsWith("[bot]");
}

async function fetchGithubJson(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

async function loadContributors() {
  const grid = document.getElementById("contributors-grid");
  if (!grid) return; // this script is shared across pages — not every page has this section
  const section = document.getElementById("contributors");

  try {
    // A single page of 100 comfortably covers every commit author, PR, and
    // issue this repo has today — not worth paginating until it actually
    // needs it.
    const [contributors, pulls, issues] = await Promise.all([
      fetchGithubJson("https://api.github.com/repos/sazardev/shiki/contributors?per_page=100"),
      fetchGithubJson("https://api.github.com/repos/sazardev/shiki/pulls?state=all&per_page=100"),
      fetchGithubJson("https://api.github.com/repos/sazardev/shiki/issues?state=all&per_page=100"),
    ]);

    // A rate-limited or otherwise-erroring GitHub API call can still come
    // back as a 200 with a JSON *object* (e.g. `{ message: "API rate limit
    // exceeded" }`) instead of the array these endpoints normally return —
    // `fetchGithubJson` only checks `res.ok`, not the shape of the body.
    // Without this, the `for...of` loops below would throw on a non-iterable
    // and skip the `catch` block's friendlier fallback message entirely.
    if (![contributors, pulls, issues].every(Array.isArray)) {
      throw new Error("unexpected (non-array) response shape from GitHub API");
    }

    const people = new Map(); // login -> { avatar_url, html_url, commits, prs, issues }

    for (const c of contributors) {
      if (isExcludedContributor(c.login, c.type)) continue;
      people.set(c.login, {
        avatar_url: c.avatar_url,
        html_url: c.html_url,
        commits: c.contributions,
        prs: 0,
        issues: 0,
      });
    }
    for (const pr of pulls) {
      const u = pr.user;
      if (!u || isExcludedContributor(u.login, u.type)) continue;
      const entry = people.get(u.login) || {
        avatar_url: u.avatar_url,
        html_url: u.html_url,
        commits: 0,
        prs: 0,
        issues: 0,
      };
      entry.prs += 1;
      people.set(u.login, entry);
    }
    for (const issue of issues) {
      if (issue.pull_request) continue; // the issues endpoint also returns PRs
      const u = issue.user;
      if (!u || isExcludedContributor(u.login, u.type)) continue;
      const entry = people.get(u.login) || {
        avatar_url: u.avatar_url,
        html_url: u.html_url,
        commits: 0,
        prs: 0,
        issues: 0,
      };
      entry.issues += 1;
      people.set(u.login, entry);
    }

    if (people.size === 0) {
      if (section) section.hidden = true;
      return;
    }

    const entries = Array.from(people.entries()).map(([login, v]) => ({
      login,
      ...v,
      score: v.commits + v.prs + v.issues,
    }));
    entries.sort((a, b) => b.score - a.score);

    const minScore = Math.min(...entries.map((e) => e.score));
    const maxScore = Math.max(...entries.map((e) => e.score));

    grid.innerHTML = entries
      .map((e) => {
        const size =
          maxScore === minScore
            ? MAX_AVATAR_PX
            : Math.round(
                MIN_AVATAR_PX + ((e.score - minScore) / (maxScore - minScore)) * (MAX_AVATAR_PX - MIN_AVATAR_PX)
              );
        const parts = [];
        if (e.commits > 0) parts.push(`${e.commits} commit${e.commits === 1 ? "" : "s"}`);
        if (e.prs > 0) parts.push(`${e.prs} PR${e.prs === 1 ? "" : "s"}`);
        if (e.issues > 0) parts.push(`${e.issues} issue${e.issues === 1 ? "" : "s"}`);
        return `
          <a class="contributor" href="${e.html_url}" target="_blank" rel="noopener">
            <img class="contributor-avatar" src="${e.avatar_url}" alt="${e.login}"
                 width="${size}" height="${size}" loading="lazy" />
            <span class="contributor-name">@${e.login}</span>
            <span class="contributor-stats">${parts.join(" · ")}</span>
          </a>`;
      })
      .join("");
  } catch (err) {
    grid.innerHTML = `<p class="contributors-error">Couldn't load contributors right now — see the full list on <a href="https://github.com/sazardev/shiki/graphs/contributors">GitHub</a>.</p>`;
  }
}

function initCopyButtons() {
  document.querySelectorAll(".copy-btn").forEach((btn) => {
    btn.addEventListener("click", async () => {
      // Release pages' "Copy announcement" button carries its own
      // ready-made text in a data attribute (scripts/generate_release_pages.py
      // bakes it in at generation time) instead of reading it off a sibling
      // <pre>/<code> — there's no visible code block to mirror on this page,
      // the whole point is a blurb the visitor never sees until they paste it.
      const attrText = btn.getAttribute("data-copy-text");
      const block = btn.closest(".code-block");
      const source = attrText === null ? (block ? block.querySelector("pre") : btn.previousElementSibling) : null;
      const text = attrText !== null ? attrText : source ? source.textContent.trim() : "";
      if (!text) return;
      try {
        await navigator.clipboard.writeText(text);
      } catch (err) {
        return; // Clipboard API blocked (insecure context, permissions) — command text is still visible/selectable by hand.
      }
      const original = btn.textContent;
      btn.textContent = "Copied!";
      btn.classList.add("copied");
      setTimeout(() => {
        btn.textContent = original;
        btn.classList.remove("copied");
      }, 1500);
    });
  });
}

function initNavToggle(versionPopover) {
  const toggle = document.getElementById("nav-toggle");
  const nav = document.getElementById("site-nav");
  if (!toggle || !nav) return;

  // Every link/button actually reachable while the panel is open — used
  // both to move focus into the panel on open and to trap Tab/Shift+Tab
  // inside it while it's open, so keyboard focus can't silently wander
  // into the hero content sitting hidden underneath the still-open panel.
  const focusable = () =>
    Array.from(nav.querySelectorAll("a, button")).filter(
      (el) => !el.hidden && el.offsetParent !== null
    );

  const setOpen = (open) => {
    nav.classList.toggle("nav-open", open);
    toggle.setAttribute("aria-expanded", String(open));
    toggle.textContent = open ? "✕" : "☰";
    // The version popover lives inside `nav` — closing the mobile menu
    // only hides it visually (via the ancestor's `display: none`) unless
    // its own state is closed too, so do that explicitly rather than
    // leaving it internally "open" for next time.
    if (!open && versionPopover) versionPopover.close();
    if (open) {
      focusable()[0]?.focus();
    } else if (nav.contains(document.activeElement)) {
      // Closing while focus was still inside the panel (Escape, or a link
      // navigating away) — return it to the control that opened the panel
      // instead of leaving it on a now-hidden element.
      toggle.focus();
    }
  };

  toggle.addEventListener("click", () => {
    setOpen(!nav.classList.contains("nav-open"));
  });

  // Picking a link closes the menu instead of leaving it open over the
  // section it just navigated to — otherwise the very next scroll shows a
  // half-screen nav panel obscuring the content it was just used to reach.
  nav.querySelectorAll("a").forEach((link) => {
    link.addEventListener("click", () => setOpen(false));
  });

  document.addEventListener("keydown", (e) => {
    if (!nav.classList.contains("nav-open")) return;
    if (e.key === "Escape") {
      setOpen(false);
      return;
    }
    // Focus trap: Tab past the last item (or Shift+Tab past the first)
    // wraps within the panel instead of escaping into the hero content
    // that's still in the DOM (just visually covered) behind it.
    if (e.key !== "Tab") return;
    const items = focusable();
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  });
}

document.addEventListener("DOMContentLoaded", () => {
  buildSwatches();
  initTheme();
  buildTemplateChips();
  loadChangelog();
  loadLatestRelease();
  initQuickInstall();
  loadContributors();
  initCopyButtons();
  const versionPopover = initVersionPopover();
  initNavToggle(versionPopover);
});

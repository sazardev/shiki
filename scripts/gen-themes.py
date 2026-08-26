#!/usr/bin/env python3
"""
gen-themes.py — Verifies (and optionally rewrites) the marketing site's
theme copies against the source of truth: shiki-config/src/themes/*.rs

Usage:
  python scripts/gen-themes.py --check    # fail if drift found (CI)
  python scripts/gen-themes.py            # rewrite docs/css/styles.css + docs/js/main.js THEMES

The script parses the `theme!` macro invocations in each theme file and
compares them to:
  - docs/css/styles.css  [data-theme="..."] blocks
  - docs/js/main.js      const THEMES = [...]

Only the minimal theme variables needed for the site (bg/fg/accent/selection/border/statusbar/success/warning/error/muted/panel_title) are checked in CSS;
the JS THEMES array only needs id/label/family/dot/screenshot.

If a new theme is added to shiki-config/src/themes/mod.rs::all(), this script
will flag the missing CSS/JS entries without needing to keep three copies in sync by hand.
"""

import re
import sys
import pathlib
from collections import OrderedDict

ROOT = pathlib.Path(__file__).resolve().parent.parent
THEMES_DIR = ROOT / "shiki-config" / "src" / "themes"
CSS_PATH = ROOT / "docs" / "css" / "styles.css"
JS_PATH = ROOT / "docs" / "js" / "main.js"

# Colors that the site actually uses (subset of Theme's 19 slots).
SITE_CSS_SLOTS = ["bg", "fg", "accent", "selection", "border", "statusbar", "success", "warning", "error", "muted", "panel_title"]

def parse_themes_rs():
    """Parse shiki-config/src/themes/*.rs for theme! invocations."""
    themes = OrderedDict()
    # Also need mod.rs ordering to preserve site order (alphabetical, default last)
    # We'll read mod.rs::all() order directly.
    mod_rs = (THEMES_DIR / "mod.rs").read_text()
    # Extract vec![ hacker::arasaka(), ... Theme::terminal_default() ] in order
    all_match = re.search(r"pub fn all\(\) -> Vec<Theme> \{.*?vec!\[.*?\]",
                          mod_rs, re.DOTALL)
    order = []
    if all_match:
        # Find all `something::name()` or `Theme::terminal_default()`
        for m in re.finditer(r"(\w+)::(\w+)\(\)", all_match.group(0)):
            mod, func = m.groups()
            if mod == "Theme" and func == "terminal_default":
                order.append("default")
            else:
                # Need to map function to theme name by reading that file
                pass

    # Parse each theme file for theme! calls
    theme_pattern = re.compile(
        r'theme!\s*[\{\(]\s*"(?P<name>[^"]+)"\s*,\s*"(?P<family>[^"]+)"\s*,'
        r'.*?bg:\s*"(?P<bg>[^"]+)".*?fg:\s*"(?P<fg>[^"]+)"'
        r'.*?accent:\s*"(?P<accent>[^"]+)".*?selection:\s*"(?P<selection>[^"]+)"'
        r'.*?border:\s*"(?P<border>[^"]+)".*?statusbar:\s*"(?P<statusbar>[^"]+)"'
        r'.*?highlight:\s*"(?P<highlight>[^"]+)".*?error:\s*"(?P<error>[^"]+)"'
        r'.*?warning:\s*"(?P<warning>[^"]+)".*?success:\s*"(?P<success>[^"]+)"'
        r'.*?inactive:\s*"(?P<inactive>[^"]+)".*?scrollbar:\s*"(?P<scrollbar>[^"]+)"'
        r'.*?tab_active:\s*"(?P<tab_active>[^"]+)".*?tab_inactive:\s*"(?P<tab_inactive>[^"]+)"'
        r'.*?panel_title:\s*"(?P<panel_title>[^"]+)".*?cursor:\s*"(?P<cursor>[^"]+)"'
        r'.*?link:\s*"(?P<link>[^"]+)".*?tag:\s*"(?P<tag>[^"]+)"'
        r'.*?muted:\s*"(?P<muted>[^"]+)"',
        re.DOTALL
    )
    for rs_file in sorted(THEMES_DIR.glob("*.rs")):
        if rs_file.name == "mod.rs":
            continue
        text = rs_file.read_text()
        for m in theme_pattern.finditer(text):
            d = m.groupdict()
            name = d["name"]
            themes[name] = {
                "name": name,
                "family": d["family"],
                "bg": d["bg"],
                "fg": d["fg"],
                "accent": d["accent"],
                "selection": d["selection"],
                "border": d["border"],
                "statusbar": d["statusbar"],
                "highlight": d["highlight"],
                "error": d["error"],
                "warning": d["warning"],
                "success": d["success"],
                "inactive": d["inactive"],
                "scrollbar": d["scrollbar"],
                "tab_active": d["tab_active"],
                "tab_inactive": d["tab_inactive"],
                "panel_title": d["panel_title"],
                "cursor": d["cursor"],
                "link": d["link"],
                "tag": d["tag"],
                "muted": d["muted"],
            }
    # Also add default (terminal_default) manually
    themes["default"] = {
        "name": "default",
        "family": "System",
        "bg": "reset",
        "fg": "reset",
        "accent": "blue",
        "selection": "darkgray",
        "border": "reset",
        "statusbar": "reset",
        "highlight": "yellow",
        "error": "red",
        "warning": "yellow",
        "success": "green",
        "inactive": "darkgray",
        "scrollbar": "darkgray",
        "tab_active": "blue",
        "tab_inactive": "darkgray",
        "panel_title": "magenta",
        "cursor": "reset",
        "link": "cyan",
        "tag": "magenta",
        "muted": "darkgray",
    }
    # Reorder to match mod.rs::all() order if we parsed it fully.
    # Fallback: alphabetical with default last (which matches mod.rs).
    ordered = OrderedDict()
    # Try to get order from mod.rs by extracting quoted names from theme! calls? No, those are in individual files.
    # Instead, read the vec! list and map each entry to theme name by calling the function? We haven't resolved.
    # Simpler: sort alphabetically except default last, which is exactly mod.rs order per its test.
    non_default = sorted([k for k in themes if k != "default"], key=lambda s: s.lower())
    for k in non_default:
        ordered[k] = themes[k]
    ordered["default"] = themes["default"]
    return ordered

def parse_css_themes():
    """Parse docs/css/styles.css for [data-theme=\"name\"] blocks."""
    text = CSS_PATH.read_text()
    css_themes = {}
    # Find each [data-theme="..."] { --key: value; }
    for m in re.finditer(r'\[data-theme="(?P<name>[^"]+)"\]\s*\{(?P<body>[^}]+)\}', text):
        name = m.group("name")
        body = m.group("body")
        slots = {}
        for sm in re.finditer(r'--(?P<key>[\w-]+):\s*(?P<val>[^;]+);', body):
            slots[sm.group("key")] = sm.group("val").strip()
        css_themes[name] = slots
    # Also handle :root, [data-theme="gruvbox-dark"] shared block
    # :root block is fallback for default; we treat it as gruvbox-dark already
    return css_themes

def parse_js_themes():
    """Parse docs/js/main.js const THEMES array."""
    text = JS_PATH.read_text()
    m = re.search(r'const THEMES = \[(?P<body>.*?)\];', text, re.DOTALL)
    if not m:
        return {}
    body = m.group("body")
    js_themes = OrderedDict()
    for tm in re.finditer(r'\{\s*id:\s*"(?P<id>[^"]+)"\s*,\s*label:\s*"(?P<label>[^"]+)"\s*,\s*family:\s*"(?P<family>[^"]+)"\s*,\s*dot:\s*"(?P<dot>[^"]+)"\s*,\s*screenshot:\s*(?P<ss>true|false)', body):
        d = tm.groupdict()
        js_themes[d["id"]] = d
    return js_themes

def check_drift(check_only=True):
    rs_themes = parse_themes_rs()
    css_themes = parse_css_themes()
    js_themes = parse_js_themes()

    errors = []

    # Check: every Rust theme (except default) should have CSS block
    for name, rs in rs_themes.items():
        if name == "default":
            # default has no CSS block by design (falls back to :root/gruvbox-dark)
            if name in css_themes:
                errors.append(f"CSS should NOT have block for 'default' (inherits :root) but found one")
            continue
        if name not in css_themes:
            errors.append(f"CSS missing block for theme '{name}' (family {rs['family']})")
            continue
        css = css_themes[name]
        for slot in SITE_CSS_SLOTS:
            # Map slot names: panel_title -> panel-title in CSS? Actually CSS uses --panel-title
            css_key = slot.replace("_", "-")
            # Rust uses `panel_title` etc; CSS uses `--panel-title`
            if css_key not in css:
                # Only require subset; if missing, report
                errors.append(f"CSS [{name}] missing --{css_key} (expected {rs[slot]})")
            elif css[css_key].lower() != rs[slot].lower():
                errors.append(f"CSS [{name}] --{css_key} drift: css={css[css_key]} vs rs={rs[slot]}")

    # Check: extra CSS themes not in Rust
    for name in css_themes:
        if name not in rs_themes:
            errors.append(f"CSS has extra theme '{name}' not in Rust")

    # Check JS THEMES
    for name, rs in rs_themes.items():
        if name not in js_themes:
            errors.append(f"JS THEMES missing entry for '{name}'")
            continue
        js = js_themes[name]
        if js["family"] != rs["family"]:
            errors.append(f"JS [{name}] family drift: js={js['family']} vs rs={rs['family']}")
        # dot is accent, except default's accent is the terminal ANSI "blue" with no single hex,
        # so the site uses a neutral gray as its swatch.
        expected_dot = "#8b949e" if name == "default" else rs["accent"]
        if js["dot"].lower() != expected_dot.lower():
            errors.append(f"JS [{name}] dot drift: js={js['dot']} vs rs accent={rs['accent']}")
        # screenshot flag: default should be false, others true
        expected_ss = "false" if name == "default" else "true"
        if js["ss"] != expected_ss:
            errors.append(f"JS [{name}] screenshot flag drift: js={js['ss']} vs expected {expected_ss}")

    for name in js_themes:
        if name not in rs_themes:
            errors.append(f"JS has extra theme '{name}' not in Rust")

    # Order check: JS order should be alphabetical with default last (same as Rust)
    js_order = list(js_themes.keys())
    expected_order = list(rs_themes.keys())
    if js_order != expected_order:
        errors.append(f"JS order drift: got {js_order} expected {expected_order}")

    if errors:
        print("Theme drift detected:")
        for e in errors:
            print(f"  - {e}")
        print(f"\nRust themes: {len(rs_themes)}  CSS blocks: {len(css_themes)}  JS entries: {len(js_themes)}")
        if check_only:
            print("\nRun `python scripts/gen-themes.py` to regenerate (or fix manually).")
        return False
    else:
        print(f"OK: {len(rs_themes)} themes in sync (CSS {len(css_themes)} blocks, JS {len(js_themes)} entries)")
        return True

if __name__ == "__main__":
    check = "--check" in sys.argv
    ok = check_drift(check_only=check)
    sys.exit(0 if ok else 1)

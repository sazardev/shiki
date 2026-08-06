#!/usr/bin/env python3
"""Generates per-version release pages and their OG share-card images.

For each released version in CHANGELOG.md this produces:
  - docs/changelog/{version}.html  (a real, static, crawlable page — link
    unfurlers read raw HTML and never run JS, so this can't be another
    client-side fetch-and-render like changelog.html's live view)
  - docs/assets/og/{version}.png   (1200x630 OG/Twitter card, rendered via
    headless Chromium from scripts/templates/og_card.html)

It also rewrites the RELEASE PAGES block of docs/sitemap.xml, and the whole
of docs/feed.xml (an RSS feed of every release, full content included — see
update_feed's own docstring for why that matters for dev.to's RSS import),
so every generated page is listed and syndicated.

Usage:
  scripts/generate_release_pages.py --all
  scripts/generate_release_pages.py --version 0.9.0
  scripts/generate_release_pages.py --version 0.9.0 --chromium /path/to/chromium

"Unreleased" is always skipped — there is nothing to share a permalink to
until a version actually ships. Re-running for a version that already has a
page/image regenerates both in place (idempotent), which is what lets a
release re-run or a later backfill correction just overwrite instead of
needing its own "already exists" special case.
"""

from __future__ import annotations

import argparse
import email.utils
import html
import json
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote

ROOT = Path(__file__).resolve().parent.parent
CHANGELOG_MD = ROOT / "CHANGELOG.md"
DOCS = ROOT / "docs"
OUT_PAGES_DIR = DOCS / "changelog"
OUT_OG_DIR = DOCS / "assets" / "og"
SCREENSHOTS_DIR = DOCS / "assets" / "screenshots"
TEMPLATES_DIR = Path(__file__).resolve().parent / "templates"
SITEMAP = DOCS / "sitemap.xml"
FEED_PATH = DOCS / "feed.xml"
SITE_URL = "https://sazardev.github.io/shiki"

# Every release page/OG card uses the same theme, deliberately — shiki has
# no notion of "this version's theme" (themes are a user preference, not a
# release attribute), so rotating through docs/assets/screenshots/*.png by
# chronological position (an earlier version of this script did exactly
# that) just looked arbitrary: there's no real connection between "v0.8.3"
# and "Gruvbox Light" for a reader to pick up on. One fixed theme reads as
# "this is what shiki looks like," consistently, matching the site's own
# original og-image.png and ThemeConfig::default(). Colors copied verbatim
# from docs/css/styles.css's `[data-theme="gruvbox-dark"]` block — if that
# palette changes there, update it here too (same "keep the marketing
# site's copies in sync" convention CLAUDE.md already documents).
FIXED_THEME = "gruvbox-dark"
THEME_COLORS = {
    "gruvbox-dark": ("#282828", "#ebdbb2", "#fabd2f", "Gruvbox Dark"),
}


@dataclass
class Version:
    version: str
    date: str
    categories: list[tuple[str, list[str]]]  # (category, [raw bullet markdown])


def parse_changelog(text: str) -> list[Version]:
    """Splits CHANGELOG.md into per-version records, newest first (matching
    the file's own order) — mirrors docs/js/main.js's renderChangelog, just
    collecting structured data instead of emitting HTML directly, since this
    same parse also needs to answer "what's this version's date" and "what's
    a good OG tagline" separately from rendering the body."""
    versions: list[Version] = []
    current: Version | None = None
    current_category: str | None = None
    bullets: list[str] = []
    pending_bullet: str | None = None

    def flush_bullet():
        nonlocal pending_bullet
        if pending_bullet is not None:
            bullets.append(pending_bullet)
            pending_bullet = None

    def flush_category():
        nonlocal current_category, bullets
        flush_bullet()
        if current is not None and current_category is not None and bullets:
            current.categories.append((current_category, bullets))
        current_category = None
        bullets = []

    for raw_line in text.split("\n"):
        line = raw_line.rstrip()
        version_match = re.match(r"^##\s+\[([^\]]+)\]\s*(?:-\s*(.+))?", line)
        if version_match:
            flush_category()
            name = version_match.group(1)
            if name.lower() != "unreleased":
                current = Version(version=name, date=version_match.group(2) or "", categories=[])
                versions.append(current)
            else:
                current = None
            continue

        category_match = re.match(r"^###\s+(.+)", line)
        if category_match:
            flush_category()
            current_category = category_match.group(1).strip()
            continue

        bullet_match = re.match(r"^-\s+(.+)", line)
        if bullet_match:
            flush_bullet()
            pending_bullet = bullet_match.group(1)
            continue

        if pending_bullet is not None and line.strip() and raw_line[:1].isspace():
            pending_bullet += " " + line.strip()
            continue

        if not line.strip():
            flush_bullet()

    flush_category()
    return versions


def render_inline(text: str) -> str:
    out = html.escape(text)
    out = re.sub(r"`([^`]+)`", r"<code>\1</code>", out)
    out = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", out)
    out = re.sub(r"\[([^\]]+)\]\(([^)]+)\)", r'<a href="\2" target="_blank" rel="noopener">\1</a>', out)
    return out


def strip_markdown(text: str) -> str:
    """Plain-text version of a bullet, for the OG card/meta description —
    those must never contain raw markdown syntax or HTML."""
    out = re.sub(r"`([^`]+)`", r"\1", text)
    out = re.sub(r"\*\*([^*]+)\*\*", r"\1", out)
    out = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", out)
    return out.strip()


def changelog_html(v: Version) -> str:
    parts = []
    for category, bullets in v.categories:
        parts.append(f"<h4>{html.escape(category)}</h4>")
        parts.append("<ul>")
        for bullet in bullets:
            parts.append(f"<li>{render_inline(bullet)}</li>")
        parts.append("</ul>")
    return "\n".join(parts)


def tagline_for(v: Version) -> str:
    for _category, bullets in v.categories:
        if bullets:
            text = strip_markdown(bullets[0])
            return text if len(text) <= 160 else text[:157].rstrip() + "…"
    return f"shiki v{v.version} release notes."


def page_url_for(v: Version) -> str:
    return f"{SITE_URL}/changelog/{v.version}.html"


def jsonld_for(v: Version, page_url: str, og_url: str) -> str:
    """schema.org structured data for a single release page — index.html
    already carries a SoftwareApplication block for the app as a whole
    (see its own <head>); this is `Article` instead, one per version,
    since a release page reads as a dated post about a specific release,
    not a second description of the whole app. Gives Google something to
    key a rich result (date + image) off when someone searches a specific
    version, e.g. "shiki v0.9.0"."""
    data = {
        "@context": "https://schema.org",
        "@type": "Article",
        "headline": f"shiki v{v.version} — Release notes",
        "description": tagline_for(v),
        "image": og_url,
        "url": page_url,
        "mainEntityOfPage": {"@type": "WebPage", "@id": page_url},
        "author": {"@type": "Person", "name": "sazardev", "url": "https://github.com/sazardev"},
        "publisher": {
            "@type": "Organization",
            "name": "shiki",
            "logo": {"@type": "ImageObject", "url": f"{SITE_URL}/assets/favicon-512.png"},
        },
    }
    if v.date:  # every real release has one; only a hypothetical malformed
        # CHANGELOG.md entry wouldn't — omit rather than emit an invalid
        # empty-string ISO date if that ever happens.
        data["datePublished"] = v.date
        data["dateModified"] = v.date
    # Defensive against a "</script>" substring anywhere in the tagline
    # (there isn't one today, but a JSON-LD block otherwise closes its own
    # <script> tag early if one ever appears) — the standard escape for
    # embedding JSON inside HTML <script> content.
    return json.dumps(data, ensure_ascii=False).replace("</", "<\\/")


def announcement_text(v: Version, page_url: str) -> str:
    """The "copy announcement" button's payload — a ready-to-paste blurb for
    anywhere that isn't covered by an intent link (LinkedIn's own share
    intent only accepts a URL, not pre-filled text, and there's no public
    intent link for e.g. a Discord/Slack message), so this exists for
    "paste it by hand" cases instead."""
    return (
        f"🚀 shiki v{v.version} is out — {tagline_for(v)}\n\n"
        f"{page_url}\n\n"
        "#rustlang #TUI #opensource"
    )


def share_links(v: Version, page_url: str) -> dict[str, str]:
    """Plain public "intent" URLs — every one of these just opens that
    platform's own compose/submit dialog for the *visitor* to post,
    pre-filled. No API call, no account, no token on shiki's side at all;
    the whole reason this is worth doing instead of a real API integration
    (Bluesky/Mastodon's APIs are simple, but still need a stored
    credential and someone to maintain it — this needs nothing)."""
    text = f"shiki v{v.version}: {tagline_for(v)}"

    def q(s: str) -> str:
        return quote(s, safe="")

    return {
        "x": f"https://twitter.com/intent/tweet?text={q(text)}&url={q(page_url)}",
        "bluesky": f"https://bsky.app/intent/compose?text={q(text + ' ' + page_url)}",
        "reddit": f"https://www.reddit.com/submit?url={q(page_url)}&title={q(text)}",
        "hn": f"https://news.ycombinator.com/submitlink?u={q(page_url)}&t={q(text)}",
        "whatsapp": f"https://wa.me/?text={q(text + ' ' + page_url)}",
        "telegram": f"https://t.me/share/url?url={q(page_url)}&text={q(text)}",
        "email": f"mailto:?subject={q(f'shiki v{v.version} released')}&body={q(text + chr(10) + chr(10) + page_url)}",
    }


def render_og_card(v: Version, chromium: str) -> None:
    bg, fg, accent, _label = THEME_COLORS[FIXED_THEME]
    screenshot = SCREENSHOTS_DIR / f"{FIXED_THEME}.png"
    if not screenshot.exists():
        raise SystemExit(f"missing screenshot for theme '{theme}': {screenshot}")

    template = (TEMPLATES_DIR / "og_card.html").read_text()
    rendered = (
        template.replace("{{THEME_BG}}", bg)
        .replace("{{THEME_FG}}", fg)
        .replace("{{THEME_ACCENT}}", accent)
        .replace("{{VERSION}}", html.escape(v.version))
        .replace("{{DATE}}", html.escape(v.date))
        .replace("{{TAGLINE}}", html.escape(tagline_for(v)))
        .replace("{{SCREENSHOT_PATH}}", screenshot.resolve().as_uri())
    )

    OUT_OG_DIR.mkdir(parents=True, exist_ok=True)
    out_png = OUT_OG_DIR / f"{v.version}.png"

    with tempfile.TemporaryDirectory() as tmp:
        html_path = Path(tmp) / "card.html"
        html_path.write_text(rendered)
        subprocess.run(
            [
                chromium,
                "--headless",
                "--disable-gpu",
                "--no-sandbox",
                "--hide-scrollbars",
                "--force-device-scale-factor=1",
                "--window-size=1200,630",
                f"--screenshot={out_png}",
                html_path.resolve().as_uri(),
            ],
            check=True,
            capture_output=True,
        )
    if not out_png.exists():
        raise SystemExit(f"chromium did not produce {out_png}")


def render_page(v: Version, prev_v: Version | None, next_v: Version | None) -> None:
    template = (TEMPLATES_DIR / "release_page.html").read_text()

    date_line = f"Released {v.date}." if v.date else "Release notes."

    def nav_link(other: Version | None, fallback_label: str) -> tuple[str, str, str]:
        if other is None:
            return "../changelog.html", fallback_label, " release-nav-link--disabled"
        return f"{other.version}.html", f"v{other.version}", ""

    prev_href, prev_label, prev_disabled = nav_link(prev_v, "No earlier release")
    next_href, next_label, next_disabled = nav_link(next_v, "No newer release")

    page_url = page_url_for(v)
    og_url = f"{SITE_URL}/assets/og/{v.version}.png"
    links = share_links(v, page_url)

    rendered = (
        template.replace("{{VERSION}}", html.escape(v.version))
        .replace("{{DATE_LINE}}", html.escape(date_line))
        .replace("{{TAGLINE}}", html.escape(tagline_for(v)))
        .replace("{{JSONLD}}", jsonld_for(v, page_url, og_url))
        .replace("{{THEME_ID}}", FIXED_THEME)
        .replace("{{CHANGELOG_HTML}}", changelog_html(v))
        .replace("{{PREV_HREF}}", prev_href)
        .replace("{{PREV_LABEL}}", html.escape(prev_label))
        .replace("{{PREV_DISABLED}}", prev_disabled)
        .replace("{{NEXT_HREF}}", next_href)
        .replace("{{NEXT_LABEL}}", html.escape(next_label))
        .replace("{{NEXT_DISABLED}}", next_disabled)
        # Intent links only ever need HTML-attribute escaping (they're
        # already fully percent-encoded query strings, so no further
        # escaping of the URL content itself is needed) — quote=True so an
        # eventual literal `"` inside one (there won't be, but the "email"
        # mailto: link is the shakiest one) can't break out of the
        # surrounding href="...".
        .replace("{{SHARE_X}}", html.escape(links["x"], quote=True))
        .replace("{{SHARE_BLUESKY}}", html.escape(links["bluesky"], quote=True))
        .replace("{{SHARE_REDDIT}}", html.escape(links["reddit"], quote=True))
        .replace("{{SHARE_HN}}", html.escape(links["hn"], quote=True))
        .replace("{{SHARE_WHATSAPP}}", html.escape(links["whatsapp"], quote=True))
        .replace("{{SHARE_TELEGRAM}}", html.escape(links["telegram"], quote=True))
        .replace("{{SHARE_EMAIL}}", html.escape(links["email"], quote=True))
        .replace("{{ANNOUNCEMENT_TEXT}}", html.escape(announcement_text(v, page_url), quote=True))
    )

    OUT_PAGES_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_PAGES_DIR / f"{v.version}.html").write_text(rendered)


def update_sitemap(versions: list[Version]) -> None:
    """Rebuilds the block between the RELEASE PAGES markers from scratch —
    idempotent by construction, so re-running for one version (or backfilling
    all of them) never produces duplicate <url> entries."""
    text = SITEMAP.read_text()
    begin = "<!-- BEGIN RELEASE PAGES (generated by scripts/generate_release_pages.py — do not hand-edit) -->"
    end = "<!-- END RELEASE PAGES -->"
    if begin not in text or end not in text:
        raise SystemExit(f"sitemap.xml is missing the {begin!r}/{end!r} markers")

    entries = []
    for v in versions:
        page = OUT_PAGES_DIR / f"{v.version}.html"
        if not page.exists():
            continue
        lastmod = v.date or "2026-01-01"
        entries.append(
            "  <url>\n"
            f"    <loc>{SITE_URL}/changelog/{v.version}.html</loc>\n"
            f"    <lastmod>{lastmod}</lastmod>\n"
            "    <changefreq>monthly</changefreq>\n"
            "    <priority>0.4</priority>\n"
            "  </url>"
        )

    block = begin + "\n" + "\n".join(entries) + ("\n" if entries else "") + "  " + end
    new_text = re.sub(re.escape(begin) + r".*?" + re.escape(end), block, text, flags=re.DOTALL)
    SITEMAP.write_text(new_text)


def rfc822(date_str: str) -> str:
    """RSS pubDate needs RFC 822 — CHANGELOG.md only ever records a date, no
    time of day, so every release is pinned to midnight UTC on that date
    rather than inventing a fake time. Good enough for feed readers/dev.to's
    RSS import, which sort and dedupe by this field but don't need
    minute-level precision for something that ships at most a few times a
    week."""
    dt = datetime.strptime(date_str, "%Y-%m-%d").replace(tzinfo=timezone.utc)
    return email.utils.format_datetime(dt)


def update_feed(versions: list[Version]) -> None:
    """Rewrites docs/feed.xml from scratch every run, same idempotent
    "rebuild the whole thing, don't try to patch it" approach as
    update_sitemap — an RSS feed has no natural place to surgically insert
    one new <item> without risking a stray duplicate, so this just emits
    every released version, newest first, every time.

    Each <item> carries the *full* changelog body as <content:encoded> (not
    just the tagline) specifically because dev.to's "Publish from RSS"
    feature (https://dev.to/settings/extensions) posts whatever's in that
    field as the article body — a feed with only a one-line description
    would auto-publish a nearly-empty post there. <enclosure> points at the
    version's own OG card so feed readers that show a thumbnail (Feedly,
    etc.) have something to display."""
    items = []
    for v in versions:
        page = OUT_PAGES_DIR / f"{v.version}.html"
        if not page.exists() or not v.date:
            continue
        page_url = f"{SITE_URL}/changelog/{v.version}.html"
        og_url = f"{SITE_URL}/assets/og/{v.version}.png"
        items.append(
            "    <item>\n"
            f"      <title>{html.escape(f'shiki v{v.version}')}</title>\n"
            f"      <link>{page_url}</link>\n"
            f'      <guid isPermaLink="true">{page_url}</guid>\n'
            f"      <pubDate>{rfc822(v.date)}</pubDate>\n"
            f"      <description>{html.escape(tagline_for(v))}</description>\n"
            f"      <content:encoded><![CDATA[{changelog_html(v)}]]></content:encoded>\n"
            f'      <enclosure url="{og_url}" type="image/png" />\n'
            "    </item>"
        )

    feed = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<rss version="2.0" '
        'xmlns:content="http://purl.org/rss/1.0/modules/content/" '
        'xmlns:atom="http://www.w3.org/2005/Atom">\n'
        "  <channel>\n"
        "    <title>shiki (私記) — Changelog</title>\n"
        f"    <link>{SITE_URL}/changelog.html</link>\n"
        f'    <atom:link href="{SITE_URL}/feed.xml" rel="self" type="application/rss+xml" />\n'
        "    <description>Every shiki release, in full — new features, real fixes, the occasional oops.</description>\n"
        "    <language>en-us</language>\n"
        "    <image>\n"
        f"      <url>{SITE_URL}/assets/favicon-512.png</url>\n"
        "      <title>shiki (私記) — Changelog</title>\n"
        f"      <link>{SITE_URL}/changelog.html</link>\n"
        "    </image>\n"
        + "\n".join(items)
        + "\n  </channel>\n</rss>\n"
    )
    FEED_PATH.write_text(feed)


def find_chromium(explicit: str | None) -> str:
    if explicit:
        return explicit
    for candidate in ("chromium", "chromium-browser", "google-chrome", "google-chrome-stable"):
        found = shutil.which(candidate)
        if found:
            return found
    raise SystemExit(
        "no Chromium/Chrome binary found on PATH — pass one explicitly with --chromium"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--all", action="store_true", help="generate every released version")
    group.add_argument("--version", help="generate a single version, e.g. 0.9.0")
    parser.add_argument("--chromium", help="path to a Chromium/Chrome binary")
    args = parser.parse_args()

    chromium = find_chromium(args.chromium)

    all_versions = parse_changelog(CHANGELOG_MD.read_text())  # newest first
    by_name = {v.version: v for v in all_versions}

    if args.version:
        if args.version not in by_name:
            raise SystemExit(f"version {args.version!r} not found in CHANGELOG.md")
        targets = [by_name[args.version]]
    else:
        targets = all_versions

    # newest-first index lets us resolve "prev" (older, next entry in this
    # list) / "next" (newer, previous entry in this list) for every target.
    index_in_all = {v.version: i for i, v in enumerate(all_versions)}

    for v in targets:
        i = index_in_all[v.version]
        prev_v = all_versions[i + 1] if i + 1 < len(all_versions) else None
        next_v = all_versions[i - 1] if i - 1 >= 0 else None
        print(f"generating v{v.version}...")
        render_og_card(v, chromium)
        render_page(v, prev_v, next_v)

    update_sitemap(all_versions)
    update_feed(all_versions)
    print(f"done: {len(targets)} version page(s) + OG card(s), sitemap.xml + feed.xml updated.")


if __name__ == "__main__":
    main()

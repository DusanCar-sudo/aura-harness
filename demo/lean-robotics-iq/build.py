#!/usr/bin/env python3
"""Lean Robotics IQ — tiny static site builder.

Source lives in src/ ; output is the repository root (index.html, method.html, ...).
No dependencies beyond the Python standard library.

  python3 build.py            build all pages
  python3 build.py --check    build to a temp dir and report unresolved tokens

Page front matter is a simple YAML-ish block delimited by --- lines:

  ---
  title: Page title
  description: Meta description
  nav: method
  ---

Partials are included with <!-- @include partials/name.html --> (path relative
to src/). {{tokens}} are filled from front matter + site defaults; unresolved
tokens fail the build.
"""
from __future__ import annotations

import argparse
import datetime as _dt
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SRC = ROOT / "src"
PAGES = SRC / "pages"

SITE = {
    "site_name": "Lean Robotics IQ",
    "site_url": "https://leanroboticsiq.com",
    "email": "aura@leanproiq.com",
    "year": str(_dt.date.today().year),
    "author": "Dušan Milosavljević",
    "extra_head": "",
    "body_class": "",
}

INCLUDE_RE = re.compile(r"<!--\s*@include\s+(?P<path>[\w./-]+)\s*-->")
TOKEN_RE = re.compile(r"\{\{\s*(?P<key>[a-z_][a-z0-9_]*)\s*\}\}")
FRONT_RE = re.compile(r"\A---\s*\n(?P<meta>.*?)\n---\s*\n", re.S)


def parse_front(text: str, path: Path) -> tuple[dict[str, str], str]:
    m = FRONT_RE.match(text)
    if not m:
        return {}, text
    meta: dict[str, str] = {}
    for line in m.group("meta").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if ":" not in line:
            raise SystemExit(f"{path}: bad front-matter line: {line!r}")
        key, value = line.split(":", 1)
        meta[key.strip()] = value.strip()
    return meta, text[m.end():]


def expand_includes(text: str, origin: Path) -> str:
    def sub(m: re.Match[str]) -> str:
        rel = m.group("path")
        inc = SRC / rel
        if not inc.exists():
            raise SystemExit(f"{origin}: include not found: {rel}")
        return expand_includes(inc.read_text(encoding="utf-8"), inc)

    return INCLUDE_RE.sub(sub, text)


def render(tokens: dict[str, str], text: str, origin: Path, *, strict: bool = True) -> str:
    text = expand_includes(text, origin)

    def sub(m: re.Match[str]) -> str:
        key = m.group("key")
        if key in tokens:
            return tokens[key]
        if strict:
            raise SystemExit(f"{origin}: unresolved token {{{{{key}}}}}")
        return m.group(0)

    return TOKEN_RE.sub(sub, text)


def mark_active_nav(html: str, nav: str | None) -> str:
    if not nav:
        return html
    needle = f'data-nav="{nav}"'
    if needle not in html:
        return html
    return html.replace(needle, f'data-nav="{nav}" class="is-active" aria-current="page"')


def build_page(page: Path, out_root: Path) -> Path:
    raw = page.read_text(encoding="utf-8")
    meta, body = parse_front(raw, page)
    tokens = {**SITE, **meta}

    layout = (SRC / "layout.html").read_text(encoding="utf-8")
    tokens["content"] = render(tokens, body, page)
    html = render(tokens, layout, SRC / "layout.html")
    html = mark_active_nav(html, meta.get("nav"))
    out = out_root / (page.stem + ".html")
    out.write_text(html, encoding="utf-8")
    return out


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--check", action="store_true", help="build into a temp dir; do not touch root")
    args = ap.parse_args(argv)

    pages = sorted(PAGES.glob("*.html"))
    if not pages:
        raise SystemExit("no pages found in src/pages")
    out_root = Path(tempfile.mkdtemp(prefix="lriq-build-")) if args.check else ROOT

    built = []
    for page in pages:
        out = build_page(page, out_root)
        built.append(out)
        print(f"  {'check' if args.check else 'built'}  {out.name:<14} {out.stat().st_size/1024:7.1f} KB")

    # lightweight output checks
    for out in built:
        html = out.read_text(encoding="utf-8")
        missing = TOKEN_RE.findall(html)
        if missing:
            print(f"  WARN  {out.name}: unresolved tokens {sorted(set(missing))}", file=sys.stderr)
        for need in ('id="main"', "assets/css/site.css", "assets/js/site.js"):
            if need not in html:
                print(f"  WARN  {out.name}: missing {need}", file=sys.stderr)

    print(f"done: {len(built)} page(s)" + (" in temp dir" if args.check else ""))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

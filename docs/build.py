#!/usr/bin/env python3
"""Build the web and man manuals from one source. Requires Python 3.11+ and Pandoc."""

import argparse
import difflib
import shutil
import subprocess
import sys
import tomllib
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent


class Links(HTMLParser):
    """Collect local navigation targets to catch broken generated links."""

    def __init__(self):
        super().__init__()
        self.ids = set()
        self.links = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            if attrs["id"] in self.ids:
                raise ValueError(f"duplicate HTML id: {attrs['id']}")
            self.ids.add(attrs["id"])
        for key in ("href", "src"):
            if key in attrs:
                self.links.append(attrs[key])


def build(args):
    if shutil.which(args.pandoc) is None:
        raise ValueError(
            f"Pandoc executable not found: {args.pandoc!r}.\n"
            "Install Pandoc 3.7.0.2 (the version used by CI); see docs/README.md.\n"
            "If already installed elsewhere, run make docs PANDOC=/path/to/pandoc."
        )
    site = ROOT / "target/site"
    site.mkdir(parents=True, exist_ok=True)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    common = [
        args.pandoc,
        str(ROOT / "docs/manual.md"),
        "--from=markdown-smart",
        "--standalone",
        "--fail-if-warnings",
    ]
    man = subprocess.check_output(
        common + [
            "--to=man", f"--metadata=footer:msg {version}",
            f"--lua-filter={ROOT / 'docs/man.lua'}",
        ], text=True
    )
    committed = ROOT / "man/msg.1"
    if args.check:
        previous = committed.read_text() if committed.exists() else ""
        if previous != man:
            sys.stderr.writelines(difflib.unified_diff(
                previous.splitlines(keepends=True), man.splitlines(keepends=True),
                fromfile="man/msg.1", tofile="regenerated man/msg.1",
            ))
            raise ValueError("man/msg.1 is out of date; run make docs and commit it")
    else:
        committed.parent.mkdir(parents=True, exist_ok=True)
        committed.write_text(man)

    subprocess.run(common + [
        "--to=html5", "--toc", "--toc-depth=2", "--section-divs",
        "--no-highlight",
        f"--template={ROOT / 'docs/template.html'}",
        "--metadata=title:msg manual", f"--metadata=version:{version}",
        f"--output={site / 'index.html'}",
    ], check=True)
    for source, destination in [
        (ROOT / "docs/manual.css", "manual.css"),
        (ROOT / "docs/manual.md", "manual.md"),
        (ROOT / "config.example.toml", "config.example.toml"),
        (committed, "msg.1"),
    ]:
        shutil.copyfile(source, site / destination)
    (site / ".nojekyll").touch()

    links = Links()
    links.feed((site / "index.html").read_text())
    for href in links.links:
        url = urlsplit(href)
        if url.scheme or url.netloc:
            continue
        if url.path and not (site / unquote(url.path)).is_file():
            raise ValueError(f"broken local link: {href}")
        if not url.path and url.fragment and unquote(url.fragment) not in links.ids:
            raise ValueError(f"broken section link: {href}")
    print(f"Built web manual: {site / 'index.html'}")
    print(f"{'Verified' if args.check else 'Generated'} terminal manual: {committed}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pandoc", default="pandoc", help="Pandoc executable")
    parser.add_argument("--check", action="store_true", help="fail if the man page is stale")
    try:
        build(parser.parse_args())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"docs: {error}")

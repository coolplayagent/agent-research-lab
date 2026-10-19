#!/usr/bin/env python3
"""Validate the Pages archive and every internal link against the built site."""
from html.parser import HTMLParser
from pathlib import PurePosixPath
from urllib.parse import unquote, urlsplit
import sys
import tarfile


class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = []
        self.ids = set()
        self.scripts = 0

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "script":
            self.scripts += 1
        if "id" in attrs:
            self.ids.add(attrs["id"])
        for field in ("href", "src"):
            if tag in {"a", "link", "img"} and field in attrs:
                self.links.append(attrs[field])


def main(archive, sources):
    with tarfile.open(archive, "r:gz") as package:
        members = {}
        for member in package:
            name = member.name.removeprefix("./")
            if member.isdir():
                continue
            assert member.isfile(), f"not a regular Pages file: {name}"
            assert name and ".." not in PurePosixPath(name).parts, name
            assert name not in members, f"duplicate Pages file: {name}"
            members[name] = package.extractfile(member).read()
    expected = {source.removesuffix(".md") + ".html" if source.endswith(".md") else source
                for source in sources}
    expected.update({"index.html", "index.zh-CN.html", "404.html", "styles.css"})
    assert members.keys() == expected, (
        f"missing: {sorted(expected - members.keys())}; "
        f"unexpected: {sorted(members.keys() - expected)}"
    )
    parsed = {}
    for name, content in members.items():
        if not name.endswith(".html"):
            continue
        text = content.decode("utf-8")
        assert "<!doctype html>" in text and "OutcomeHub" in text, name
        assert '<meta http-equiv="Content-Security-Policy"' in text, name
        parser = Links()
        parser.feed(text)
        assert parser.scripts == 0, name
        parsed[name] = parser
    for name, parser in parsed.items():
        for href in parser.links:
            parsed_url = urlsplit(href)
            if parsed_url.scheme in {"http", "https", "mailto"}:
                continue
            assert not parsed_url.scheme and not parsed_url.netloc, (name, href)
            if parsed_url.path:
                target = PurePosixPath(name).parent.joinpath(unquote(parsed_url.path))
                parts = []
                for part in target.parts:
                    if part == "..":
                        assert parts, (name, href)
                        parts.pop()
                    elif part != ".":
                        parts.append(part)
                target = "/".join(parts)
            else:
                target = name
            assert target in members, (name, href, target)
            if parsed_url.fragment and target.endswith(".html"):
                assert unquote(parsed_url.fragment) in parsed[target].ids, (
                    name, href, target
                )
    for name, lang in (("index.html", "en"), ("index.zh-CN.html", "zh-CN"),
                       ("docs/en/00-index.html", "en"),
                       ("docs/zh-CN/00-index.html", "zh-CN")):
        assert f'<html lang="{lang}">' in members[name].decode("utf-8"), name
    print(f"Pages archive: {len(members)} files, {len(parsed)} rendered pages, all links resolve")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2:])

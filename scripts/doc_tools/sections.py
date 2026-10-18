"""Heading-aware Markdown section extraction used by impact reports."""

from __future__ import annotations

import difflib
import re
from dataclasses import asdict, dataclass
from typing import Any

from doc_tools.github_slugger import GithubSlugger
from doc_tools.markdown import MarkdownDocument

_GENERATED_BEGIN = re.compile(r"^\s*<!-- BEGIN GENERATED: ([^ ]+) -->\s*$")
_GENERATED_END = re.compile(r"^\s*<!-- END GENERATED: ([^ ]+) -->\s*$")


@dataclass
class Section:
    section_id: str
    path: str
    heading: str
    line_start: int
    line_end: int
    text: str
    generated_only: bool
    generated_region_ids: list[str]
    source_paths: list[str]

    def as_dict(self) -> dict[str, Any]:
        return asdict(self)


def _generated_only(lines: list[str]) -> tuple[bool, list[str]]:
    active: str | None = None
    region_ids: list[str] = []
    saw_region = False
    for line in lines:
        begin, end = _GENERATED_BEGIN.match(line), _GENERATED_END.match(line)
        if begin:
            if active is not None:
                return False, []
            active = begin.group(1)
            region_ids.append(active)
            saw_region = True
        elif end:
            if active != end.group(1):
                return False, []
            active = None
        elif active is None and line.strip() and not line.lstrip().startswith("<!--") and not line.lstrip().startswith("#"):
            return False, []
    return saw_region and active is None, region_ids


def extract_sections(path: str, text: str) -> list[Section]:
    """Return stable GitHub-anchor sections and mark generated-only regions."""
    document = MarkdownDocument(path, text)
    lines = text.splitlines()
    slugger = GithubSlugger()
    headings: list[tuple[int, int, str, str]] = []
    for index, token in enumerate(document.tokens):
        if token.type != "heading_open" or index + 1 >= len(document.tokens) or not token.map:
            continue
        inline = document.tokens[index + 1]
        if inline.type != "inline":
            continue
        title = document._get_inline_plaintext(inline)
        headings.append((int(token.tag[1:]), token.map[0], title, slugger.slug(title)))
    if not headings:
        generated_only, region_ids = _generated_only(lines)
        return [Section(f"{path}#top", path, "(top)", 1, len(lines), text, generated_only, region_ids, [])]

    sections = []
    first_heading_start = headings[0][1]
    preamble_lines = lines[:first_heading_start]
    if any(line.strip() for line in preamble_lines):
        generated_only, region_ids = _generated_only(preamble_lines)
        sections.append(
            Section(
                f"{path}#top", path, "(top)", 1, first_heading_start,
                "\n".join(preamble_lines), generated_only, region_ids, [],
            )
        )
    for index, (level, start, title, anchor) in enumerate(headings):
        end = len(lines)
        for next_level, next_start, _, _ in headings[index + 1 :]:
            if next_level <= level:
                end = next_start
                break
        section_lines = lines[start:end]
        generated_only, region_ids = _generated_only(section_lines[1:])
        sections.append(
            Section(
                f"{path}#{anchor}", path, title, start + 1, end,
                "\n".join(section_lines), generated_only, region_ids, [],
            )
        )
    return sections


def changed_line_sets(old_text: str, new_text: str) -> tuple[set[int], set[int]]:
    """Return 1-based changed line numbers in each version of a text file."""
    old_lines, new_lines = old_text.splitlines(), new_text.splitlines()
    old_changed: set[int] = set()
    new_changed: set[int] = set()
    for tag, old_start, old_end, new_start, new_end in difflib.SequenceMatcher(
        None, old_lines, new_lines, autojunk=False
    ).get_opcodes():
        if tag != "equal":
            old_changed.update(range(old_start + 1, old_end + 1))
            new_changed.update(range(new_start + 1, new_end + 1))
    return old_changed, new_changed


def deepest_sections_for_lines(sections: list[Section], line_numbers: set[int]) -> list[Section]:
    """Choose the most specific heading section for each changed line."""
    selected: dict[str, Section] = {}
    for line in sorted(line_numbers):
        candidates = [
            section
            for section in sections
            if section.line_start <= line <= section.line_end
        ]
        if candidates:
            section = max(candidates, key=lambda item: (item.line_start, -item.line_end))
            selected[section.section_id] = section
    return [selected[key] for key in sorted(selected)]

#!/usr/bin/env python3
"""Find numeric literals that are not named constants (AGENTS.md section 5).

A magic number is a literal whose meaning a reader cannot recover: a window, a
threshold, a duration, a retry count, a file mode. The harness states the rule
in prose ("Named constants over magic numbers"); this is its executable half.

The rule, so it stays reviewable:

* Scanned: every ``*.rs`` under the root (``target/``, ``vendor/``, and the
  local state tree pruned).
* Ignored inside a line: comments, string/char literals (including raw
  strings), and the argument span of an attribute (``#[...]``).
* Allowed anywhere: ``0``, ``1``, ``2`` and their float forms -- indices,
  increments, and the conventional small counts.
* Allowed on a line that declares the value: ``const``/``static`` items, which
  is where a value earns its name.

Everything else is a finding. The sensor runs at ``severity = "warn"`` with a
blessed findings baseline (``plans/baselines.json``), so existing occurrences are
grandfathered and any *increase* is a hard regression: the rule applies to new
code from the day it lands, without a tree-wide rewrite.

Usage: ``check_magic_numbers.py [ROOT]``; prints one ``file:line: literal`` per
finding plus a final ``FINDINGS: <n>`` line, and exits 1 when n > 0.
"""

from __future__ import annotations

import pathlib
import re
import sys

#: Findings printed before the report switches to a count: a 1900-finding
#: baseline must not flood a CI log, and the ratchet reads FINDINGS.
PRINT_LIMIT = 25

#: `--limit 0` prints every finding: for a tree-wide survey, not for CI.

#: Conventional values: indices, increments, small counts, success codes.
ALLOWED_VALUES = frozenset({"0", "1", "2", "0.0", "1.0", "2.0"})

#: Directories whose Rust sources are not this repository's to police.
PRUNED_DIRS = frozenset({"target", ".git", ".do-harness", "node_modules", "vendor"})

#: A numeric literal token: decimal, radix-prefixed, or a float. Not adjacent to
#: identifier characters, not part of a `..` range, and not a field access.
NUMBER = re.compile(r"(?<![\w.$])(0[oObB][0-9a-fA-F_]+|0[xX][0-9a-fA-F_]+|\d+(?:\.\d+)?)(?![\w.])")

#: `const NAME: T = ...` / `static NAME: T = ...`: where a value earns its name.
DECLARATION = re.compile(r"\b(?:const|static)\s+[A-Z_][A-Z0-9_]*\s*:")


def blank(count: int) -> str:
    """Spaces of `count` characters, keeping column positions intact."""
    return " " * max(count, 0)


def skip_raw_string(line: str, start: int) -> int | None:
    """Index just past a raw string that starts at `start`, or None if unclosed."""
    hashes = 0
    index = start + 1
    while index < len(line) and line[index] == "#":
        hashes += 1
        index += 1
    if index >= len(line) or line[index] != '"':
        return start + 1  # not a raw string after all
    terminator = '"' + "#" * hashes
    close = line.find(terminator, index + 1)
    return None if close == -1 else close + len(terminator)


def skip_string(line: str, start: int) -> int | None:
    """Index just past the string literal, or None when it runs past this line."""
    index = start + 1
    while index < len(line):
        if line[index] == "\\":
            index += 2
            continue
        if line[index] == '"':
            return index + 1
        index += 1
    return None


def strip_comments_and_literals(line: str) -> tuple[str, bool]:
    """Returns `line` blanked out, plus whether it ends inside a string literal.

    A multi-line literal (a `"\n…"` panic message, a raw string) continues on
    the next line, where a naive line scanner would read its contents as code.
    """
    out: list[str] = []
    index = 0
    length = len(line)
    while index < length:
        if line.startswith("//", index):
            break
        char = line[index]
        if char == "r" and index + 1 < length and line[index + 1] in '"#':
            end = skip_raw_string(line, index)
            if end is None:
                out.append(blank(length - index))
                return "".join(out), True
            if end != index + 1:
                out.append(blank(end - index))
                index = end
                continue
        if char == '"':
            end = skip_string(line, index)
            if end is None:
                out.append(blank(length - index))
                return "".join(out), True
            out.append(blank(end - index))
            index = end
            continue
        if char == "'":
            close = line.find("'", index + 1, index + 4)
            if close != -1:
                out.append(blank(close + 1 - index))
                index = close + 1
                continue
        if char == "#" and index + 1 < length and line[index + 1] == "[":
            close = line.find("]", index)
            if close != -1:
                out.append(blank(close + 1 - index))
                index = close + 1
                continue
        out.append(char)
        index += 1
    return "".join(out), False


def findings_in(path: pathlib.Path) -> list[str]:
    """Returns `line: literal` findings for one file."""
    found: list[str] = []
    inside_literal = False
    for number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if inside_literal:
            inside_literal = raw.count('"') % 2 == 0
            continue
        if DECLARATION.search(raw):
            continue
        stripped, inside_literal = strip_comments_and_literals(raw)
        for literal in NUMBER.findall(stripped):
            if literal not in ALLOWED_VALUES:
                found.append(f"{number}: {literal}")
    return found


def rust_sources(root: pathlib.Path) -> list[pathlib.Path]:
    sources: list[pathlib.Path] = []
    for path in sorted(root.rglob("*.rs")):
        if PRUNED_DIRS.intersection(path.relative_to(root).parts):
            continue
        sources.append(path)
    return sources


def main(argv: list[str]) -> int:
    limit = PRINT_LIMIT
    arguments = list(argv[1:])
    if "--limit" in arguments:
        index = arguments.index("--limit")
        limit = int(arguments[index + 1])
        del arguments[index : index + 2]
    root = pathlib.Path(arguments[0] if arguments else ".").resolve()
    total = 0
    printed = 0
    for path in rust_sources(root):
        hits = findings_in(path)
        if hits:
            relative = path.relative_to(root)
            for hit in hits:
                if printed < limit or limit == 0:
                    print(f"{relative}:{hit}")
                    printed += 1
            total += len(hits)
    if limit and total > printed:
        print(f"... {total - printed} more (see plans/baselines.json for the ceiling)")
    print(f"FINDINGS: {total}")
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

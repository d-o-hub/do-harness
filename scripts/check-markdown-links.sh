#!/usr/bin/env bash
# check-markdown-links.sh — hermetic relative-link validation for markdown.
#
# Sensor: scripts/check-markdown-links.sh
# Coverage:
#   - every tracked or untracked-but-not-ignored *.md file
#
# markdownlint-cli2 checks structure, not link targets, and the
# repository-relative paths documents cite (`../crates/types`,
# `scripts/check-deps.sh`, `.github/workflows/verify.yml`) become dead links
# silently when a file moves or is renamed. This walks every markdown file,
# extracts inline links `[text](target)` and reference definitions
# `[label]: target`, and fails when a *relative* target does not exist on disk.
#
# Deliberately network-free: `http(s)://`, `mailto:`, `tel:`, `ftp://`, `data:`,
# protocol-relative `//`, root-absolute `/` site paths, and `#anchor`-only links
# are skipped. Fenced code blocks and inline code spans are stripped first, so a
# documented link example is not mistaken for a real one. Symlinked documents
# (`README.md` and `assets/compliance.md` in `crates/do-harness/` are symlinks)
# are read through their canonical file and relative links are resolved against
# it: the content is authored for the real location.
#
# Enforcement policy (fail-open locally, fail-closed on demand):
#   - python3 missing AND (CI=true OR DO_HARNESS_REQUIRE_TOOLS=1) -> FAIL.
#   - python3 missing otherwise -> SKIP (the scan itself needs no other tool).
#   - zero markdown files discovered -> FAIL (no vacuous gate).
set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v python3 >/dev/null 2>&1; then
    if require_tools; then
        echo "FAIL: python3 is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: python3 not installed; skipping the markdown relative-link check (install python3)."
    exit 0
fi

mapfile -t files < <(
    if [[ -d "$ROOT/.git" ]]; then
        # Tracked and untracked alike, so a new document fails here before it is
        # staged. Ignored trees (target/, node_modules/) stay excluded.
        git -C "$ROOT" ls-files --cached --others --exclude-standard '*.md'
    else
        find "$ROOT" -name '*.md' -type f
    fi | sort
)

if [[ "${#files[@]}" -eq 0 ]]; then
    echo "FAIL: no markdown files found to scan; a gate that checks nothing is not a pass. Restore the tracked markdown files or fix the discovery root." >&2
    exit 1
fi

# The script arrives on stdin (heredoc), so the file list is passed as argv.
python3 - "$ROOT" "${files[@]}" <<'PY'
import os
import re
import sys
import urllib.parse

root = os.path.abspath(sys.argv[1])
files = sys.argv[2:]

# An inline link or image: [text](target), ![alt](target), with an optional
# <angle-bracketed> target and an optional "title".
INLINE = re.compile(r"!?\[[^\]]*\]\(\s*<?([^)\s<>]*?)\s*>?(?:\s+[\"'][^\"']*[\"'])?\s*\)")
# A reference definition: [label]: target ("title").
REFDEF = re.compile(r"^ {0,3}\[[^\]]+\]:\s*<?([^\s<>]+)>?", re.M)
# Fenced code blocks (``` or ~~~), replaced by bare newlines so reported line
# numbers still match the source file.
FENCED = re.compile(r"^ {0,3}(?:```|~~~).*?^ {0,3}(?:```|~~~)", re.M | re.S)
# Inline code spans. A span cannot be a link, and documentation that shows the
# syntax `[text](path)` must not be read as a real link.
CODE_SPAN = re.compile(r"`+[^`\n]*`+")
# URI schemes and non-relative targets that are never checked on disk.
EXTERNAL = ("http://", "https://", "mailto:", "tel:", "ftp://", "data:", "//")

failures = []
checked = 0
for rel_path in files:
    real_path = os.path.realpath(os.path.join(root, rel_path))
    directory = os.path.dirname(real_path)
    try:
        with open(real_path, "r", encoding="utf-8") as handle:
            text = handle.read()
    except (OSError, UnicodeDecodeError) as err:
        failures.append((rel_path, 0, f"<unreadable source: {err}>", ""))
        continue

    body = FENCED.sub(lambda match: "\n" * match.group(0).count("\n"), text)
    body = CODE_SPAN.sub("", body)

    seen = set()
    for match in list(INLINE.finditer(body)) + list(REFDEF.finditer(body)):
        dest = (match.group(1) or "").strip()
        if not dest or dest in seen:
            continue
        seen.add(dest)
        if dest.startswith("#") or dest.startswith(EXTERNAL) or dest.startswith("/"):
            continue
        target = dest.split("#", 1)[0].split("?", 1)[0]
        if not target:
            continue
        checked += 1
        resolved = os.path.normpath(
            os.path.join(directory, urllib.parse.unquote(target))
        )
        if not os.path.exists(resolved):
            line = body.count("\n", 0, match.start()) + 1
            failures.append((rel_path, line, dest, os.path.relpath(resolved, root)))

if failures:
    for rel_path, line, dest, resolved in failures:
        print(
            f"FAIL: {rel_path}:{line}: relative link `{dest}` does not exist "
            f"(resolved: {resolved})",
            file=sys.stderr,
        )
    print(
        f"FAIL: {len(failures)} broken relative markdown link(s) across "
        f"{len(files)} file(s).",
        file=sys.stderr,
    )
    sys.exit(1)

print(
    f"check-markdown-links OK: {len(files)} markdown file(s) scanned, "
    f"{checked} relative link(s) resolved."
)
PY

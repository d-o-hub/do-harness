#!/usr/bin/env bash
# check-markdownlint.sh — markdownlint gate for documentation and markdown files.
#
# Sensor: scripts/check-markdownlint.sh
# Coverage:
#   - **/*.md
# Enforcement policy (fail-open locally, fail-closed on demand):
#   - markdownlint-cli2/markdownlint missing AND (CI=true OR DO_HARNESS_REQUIRE_TOOLS=1) -> FAIL.
#   - markdownlint-cli2/markdownlint missing otherwise -> SKIP.
#   - zero markdown files discovered -> FAIL (no vacuous gate).

set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if command -v markdownlint-cli2 >/dev/null 2>&1; then
    RUNNER=(markdownlint-cli2)
elif command -v markdownlint >/dev/null 2>&1; then
    RUNNER=(markdownlint)
elif command -v npx >/dev/null 2>&1 && npx --no-install markdownlint-cli2 --version >/dev/null 2>&1; then
    RUNNER=(npx --no-install markdownlint-cli2)
else
    if require_tools; then
        echo "FAIL: markdownlint-cli2 or markdownlint is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: markdownlint-cli2/markdownlint not installed; skipping markdown lint."
    exit 0
fi

mapfile -t files < <(
    if [[ -d "$ROOT/.git" ]]; then
        # See check-yamllint.sh: untracked files are linted too, so a new
        # document fails here rather than after it is staged.
        git -C "$ROOT" ls-files --cached --others --exclude-standard '*.md'
    else
        find "$ROOT" -name '*.md' -type f
    fi | sort
)

if [[ "${#files[@]}" -eq 0 ]]; then
    echo "FAIL: no markdown files found to lint; a gate that checks nothing is not a pass." >&2
    exit 1
fi

"${RUNNER[@]}" "${files[@]}"
echo "check-markdownlint OK: ${#files[@]} markdown file(s) linted."

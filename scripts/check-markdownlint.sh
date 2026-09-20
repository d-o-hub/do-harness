#!/usr/bin/env bash
# check-markdownlint.sh — markdownlint gate for documentation and markdown files.
#
# Sensor: scripts/check-markdownlint.sh
# Coverage:
#   - **/*.md
# Enforcement policy (fail-open locally, fail-closed on demand):
#   - markdownlint-cli2/markdownlint missing AND (CI=true OR DO_HARNESS_REQUIRE_TOOLS=1) -> FAIL.
#   - markdownlint-cli2/markdownlint missing otherwise -> SKIP.

set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

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
        git -C "$ROOT" ls-files '*.md'
    else
        find "$ROOT" -name '*.md' -type f
    fi | sort
)

if [[ "${#files[@]}" -eq 0 ]]; then
    echo "check-markdownlint OK: no markdown files found to lint."
    exit 0
fi

"${RUNNER[@]}" "${files[@]}"
echo "check-markdownlint OK: ${#files[@]} markdown file(s) linted."

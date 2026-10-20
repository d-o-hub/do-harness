#!/usr/bin/env bash
# check-shell.sh — shellcheck gate for repo scripts, skill scripts, and
# shipped templates.
#
# Sensor: scripts/check-shell.sh
# Coverage:
#   - scripts/**/*.sh and crates/do-harness/templates/scripts/**/*.sh
#   - .agents/skills/**/*.sh (skill walkthroughs and helpers)
#   - crates/do-harness/templates/skills/**/*.sh (shipped skill fixtures)
#   - tests/**/*.sh (integration helpers and fixtures)
# Enforcement policy (fail-open locally, fail-closed on demand):
#   - shellcheck missing AND (CI=true OR DO_HARNESS_REQUIRE_TOOLS=1) -> FAIL.
#   - shellcheck missing otherwise -> SKIP (keeps offline
#     `do-harness verify --fail-fast` usable; CI lints shell explicitly).
set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v shellcheck >/dev/null 2>&1; then
    if require_tools; then
        echo "FAIL: shellcheck is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: shellcheck not installed; skipping shell lint (install shellcheck: https://www.shellcheck.net)."
    exit 0
fi

mapfile -t files < <(
    {
        find "$ROOT/scripts" "$ROOT/crates/do-harness/templates/scripts" \
            -name '*.sh' -print
        find "$ROOT/.agents/skills" "$ROOT/crates/do-harness/templates/skills" \
            -name '*.sh' -print
        # Integration helpers and fixtures are runnable entry points even when
        # no workflow invokes them (the selection-impact suite is manual after
        # #359), so they are linted too — the old `-maxdepth 1` silently skipped
        # both.
        if [[ -d "$ROOT/tests" ]]; then
            find "$ROOT/tests" -name '*.sh' -print
        fi
    } | sort
)

if [[ "${#files[@]}" -eq 0 ]]; then
    echo "FAIL: no shell scripts found to lint. Restore the scripts/ tree or pass a complete checkout." >&2
    exit 1
fi

shellcheck "${files[@]}"
echo "check-shell OK: ${#files[@]} script(s) linted."

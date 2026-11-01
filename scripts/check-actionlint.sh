#!/usr/bin/env bash
# check-actionlint.sh — actionlint gate for GitHub Actions workflow files.
#
# Sensor: scripts/check-actionlint.sh
# Coverage:
#   - .github/workflows/*.yml and .github/workflows/*.yaml
#
# check-yamllint.sh only proves the workflow files are well-formed YAML; it
# cannot see a misspelled key, an undefined `${{ matrix.foo }}`, a
# non-existent action input, or a `run:` script with a shell syntax error.
# actionlint parses the workflow schema plus the embedded shell, so those become
# CI failures instead of silently ignored steps.
#
# Only `.github/workflows/` is scanned: actionlint treats every file it is
# handed as a workflow, so passing `.yamllint.yml`, `.codacy.yml`, or
# `.pre-commit-config.yaml` reports a schema error for a file that is not a
# workflow at all.
#
# Enforcement policy (fail-open locally, fail-closed on demand):
#   - actionlint missing AND (CI=true OR DO_HARNESS_REQUIRE_TOOLS=1) -> FAIL.
#   - actionlint missing otherwise -> SKIP (keeps an offline dev checkout
#     usable; CI and the release preflight install it).
#   - zero workflow files discovered -> FAIL (no vacuous gate).
set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

ROOT="$(dh_repo_root)"
cd "$ROOT"

if ! command -v actionlint >/dev/null 2>&1; then
    if dh_require_tools; then
        echo "FAIL: actionlint is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: actionlint not installed; skipping workflow lint (install actionlint 1.7.7: https://github.com/rhysd/actionlint/releases/tag/v1.7.7)."
    exit 0
fi

mapfile -t files < <(
    if [[ -d "$ROOT/.git" ]]; then
        # `--others --exclude-standard`: a new workflow is linted before it is
        # staged, which is exactly when a schema error is cheapest to catch.
        git -C "$ROOT" ls-files --cached --others --exclude-standard \
            '.github/workflows/*.yml' '.github/workflows/*.yaml'
    else
        find "$ROOT/.github/workflows" \( -name '*.yml' -o -name '*.yaml' \) -type f \
            | sed "s|^$ROOT/||"
    fi | sort
)

if [[ "${#files[@]}" -eq 0 ]]; then
    echo "FAIL: no workflow files found to lint; a gate that checks nothing is not a pass. Restore .github/workflows/ or fix the discovery root." >&2
    exit 1
fi

actionlint "${files[@]}"
echo "check-actionlint OK: ${#files[@]} workflow file(s) linted."

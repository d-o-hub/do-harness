#!/usr/bin/env bash
# check-yamllint.sh — yamllint gate for workflow files, configs, and YAML files.
#
# Sensor: scripts/check-yamllint.sh
# Coverage:
#   - **/*.yml, **/*.yaml
# Enforcement policy (fail-open locally, fail-closed on demand):
#   - yamllint missing AND (CI=true OR DO_HARNESS_REQUIRE_TOOLS=1) -> FAIL.
#   - yamllint missing otherwise -> SKIP.
#   - zero yaml files discovered -> FAIL (no vacuous gate).

set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v yamllint >/dev/null 2>&1; then
    if require_tools; then
        echo "FAIL: yamllint is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: yamllint not installed; skipping yaml lint."
    exit 0
fi

mapfile -t files < <(
    if [[ -d "$ROOT/.git" ]]; then
        # `--others --exclude-standard`: a new workflow or config is unlinted
        # until it is staged, which is exactly when a syntax error is cheapest
        # to catch. Ignored trees (target/, fixtures) stay excluded.
        git -C "$ROOT" ls-files --cached --others --exclude-standard '*.yml' '*.yaml'
    else
        find "$ROOT" \( -name '*.yml' -o -name '*.yaml' \) -type f
    fi | sort
)

if [[ "${#files[@]}" -eq 0 ]]; then
    echo "FAIL: no yaml files found to lint; a gate that checks nothing is not a pass." >&2
    exit 1
fi

# Pass configuration file if present
CMD=(yamllint)
if [[ -f "$ROOT/.yamllint.yml" ]]; then
    CMD+=("-c" "$ROOT/.yamllint.yml")
elif [[ -f "$ROOT/.yamllint" ]]; then
    CMD+=("-c" "$ROOT/.yamllint")
fi

"${CMD[@]}" "${files[@]}"
echo "check-yamllint OK: ${#files[@]} yaml file(s) linted."

#!/usr/bin/env bash
# check-magic-numbers.sh — numeric literals must be named constants.
#
# Sensor: scripts/check-magic-numbers.sh
# What it proves: no Rust source in this repository carries a numeric literal
# that is neither a named constant nor one of the conventional values (0, 1, 2).
# AGENTS.md section 5 states the rule ("Named constants over magic numbers");
# `scripts/check_magic_numbers.py` is its executable half and documents exactly
# what is scanned, ignored, and allowed.
#
# Findings are reported, not fixed: the sensor runs at warn severity with a
# blessed baseline (`plans/baselines.json`), so the existing occurrences are
# grandfathered and any *increase* is a hard regression. That is what makes the
# rule enforceable on new code without a tree-wide rewrite.
#
# Enforcement policy (fail-open on a missing tool, fail-closed when required):
#   - python3 missing AND (CI=true OR DO_HARNESS_REQUIRE_TOOLS=1) -> FAIL.
#   - python3 missing otherwise -> SKIP with the marker the runner surfaces.
# Portability: POSIX shell, coreutils, python3 (3.9+ for the checker's typing).
set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v python3 >/dev/null 2>&1; then
    if require_tools; then
        echo "FAIL: python3 is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: python3 not installed; skipping the magic-number check."
    exit 0
fi

python3 "$ROOT/scripts/check_magic_numbers.py" "$ROOT"

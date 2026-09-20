#!/usr/bin/env bash
# check-coverage.sh — measures line coverage via cargo-llvm-cov and nextest.
#
# Sensor: scripts/check-coverage.sh
# Severity: warn
# Target threshold: 70% line coverage (codecov-style project target).
# Output: generates lcov.info and reports `FINDINGS: <deficit>` for the blessed ratchet.

set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v cargo-llvm-cov >/dev/null 2>&1 || ! command -v cargo-nextest >/dev/null 2>&1; then
    if require_tools; then
        echo "FAIL: cargo-llvm-cov and cargo-nextest are required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: cargo-llvm-cov or cargo-nextest not installed; skipping coverage check."
    exit 0
fi

TARGET_PCT=70

if ! cov_output="$(cargo llvm-cov nextest --lcov --output-path lcov.info 2>&1)"; then
    echo "$cov_output"
    echo "FAIL: cargo llvm-cov nextest failed."
    exit 1
fi

echo "$cov_output"

LINE_PCT="$(echo "$cov_output" | awk '/^TOTAL[[:space:]]/ {for(i=1;i<=NF;i++) if ($i ~ /^[0-9]+\.[0-9]+%$/) p=$i} END {print p}' | tr -d '%')"

if [[ -z "$LINE_PCT" ]]; then
    echo "WARN: Could not parse coverage percentage from cargo-llvm-cov output."
    echo "FINDINGS: 1"
    exit 0
fi

PCT_INT="${LINE_PCT%%.*}"
if [[ -z "$PCT_INT" ]]; then
    PCT_INT=0
fi

if (( PCT_INT < TARGET_PCT )); then
    DEFICIT=$(( TARGET_PCT - PCT_INT ))
    echo "WARN: Line coverage is ${LINE_PCT}% (target ${TARGET_PCT}%, deficit ${DEFICIT}%)."
    echo "FINDINGS: ${DEFICIT}"
else
    echo "check-coverage OK: Line coverage is ${LINE_PCT}% (>= ${TARGET_PCT}%)."
    echo "FINDINGS: 0"
fi

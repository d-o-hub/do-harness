#!/usr/bin/env bash
# check-audit.sh — RustSec advisory scan for this workspace.
#
# Sensor: scripts/check-audit.sh
# Missing cargo-audit fails closed when CI=true or DO_HARNESS_REQUIRE_TOOLS=1,
# and is a WARN skip otherwise so offline local runs stay usable.
set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! cargo audit --version >/dev/null 2>&1; then
    if require_tools; then
        echo "FAIL: cargo-audit is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "WARN: cargo-audit not installed; skipping RustSec scan."
    exit 0
fi

cargo audit --deny warnings

#!/usr/bin/env bash
# check-audit.sh — scans dependencies against the RustSec advisory DB.
#
# Sensor: scripts/check-audit.sh
# Enforcement policy (fail-open locally, fail-closed in CI):
#   - cargo-audit missing AND CI=true  -> FAIL (CI must install it).
#   - cargo-audit missing otherwise    -> WARN skip (keeps offline
#     `do-harness verify --fail-fast` pre-pushes usable; advisory scanning
#     is still enforced by the CI pipeline).
# Note: cargo-deny (deps sensor) also checks advisories; this sensor is the
# independent second gate using cargo-audit's own DB handling.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! cargo audit --version >/dev/null 2>&1; then
    if [[ "${CI:-}" == "true" ]]; then
        echo "FAIL: cargo-audit is required when CI=true."
        exit 1
    fi
    echo "WARN: cargo-audit not installed; skipping RustSec scan."
    exit 0
fi

cargo audit

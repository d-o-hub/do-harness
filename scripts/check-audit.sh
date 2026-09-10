#!/usr/bin/env bash
# check-audit.sh — scans dependencies against the RustSec advisory DB.
#
# Sensor: scripts/check-audit.sh
# Enforcement policy (fail-open locally, fail-closed on demand):
#   - cargo-audit missing AND (CI=true OR DO_HARNESS_REQUIRE_TOOLS=1) -> FAIL.
#   - cargo-audit missing otherwise -> WARN skip (keeps offline
#     `do-harness verify --fail-fast` usable; advisory scanning is still
#     enforced by the CI pipeline).
# NOTE: a WARN skip exits 0 and is indistinguishable from a pass downstream.
# Set DO_HARNESS_REQUIRE_TOOLS=1 in any context where a skip must not
# masquerade as green (pre-push, release gates). CI sets it explicitly in
# .github/workflows/verify.yml alongside CI=true.
# Note: cargo-deny (deps sensor) also checks advisories; this sensor is the
# independent second gate using cargo-audit's own DB handling.

set -euo pipefail

# Fail-closed when the caller demands real gates: CI or an explicit opt-in.
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

# Ignore RUSTSEC-2026-0097 (rand 0.8.5 unsound warning in agent-governance dev/optional tree; agent-governance 3.2.2 is pinned until GA).
cargo audit --deny warnings --ignore RUSTSEC-2026-0097

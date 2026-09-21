#!/usr/bin/env bash
# check-machete.sh — checks for unused dependencies across workspace manifests using cargo-machete.
#
# Sensor: scripts/check-machete.sh
# Identifies unused dependencies across workspace manifests.
# Exact-pin policy: pins stay exact (=3.2.2 for agent-governance, =3.3.0 for rmcp);
# this sensor verifies no unused dependencies exist.
#
# Enforcement policy (fail-open locally with SKIP marker, fail-closed on demand):
#   - If cargo-machete is available: runs cargo machete
#   - If cargo-machete is missing AND require_tools (CI=true or DO_HARNESS_REQUIRE_TOOLS=1):
#       echo "FAIL: cargo-machete is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1." and exit 1
#   - If cargo-machete is missing otherwise:
#       prints SKIP: marker and exits 0.

set -euo pipefail

# Fail-closed when the caller demands real gates: CI or an explicit opt-in.
require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if command -v cargo-machete >/dev/null 2>&1 || cargo machete --version >/dev/null 2>&1; then
    cargo machete
else
    if require_tools; then
        echo "FAIL: cargo-machete is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: cargo-machete not installed; skipping unused dependency check."
    exit 0
fi

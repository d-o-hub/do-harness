#!/usr/bin/env bash
# check-powerset.sh — runs feature-powerset check over guardian-proxy crate.
#
# Sensor: scripts/check-powerset.sh
# Checks feature combinations of guardian-proxy (--features agt-governance / mcp-surface on/off).
# Exact-pin policy: pins stay exact (=3.2.2 for agent-governance, =3.3.0 for rmcp);
# this sensor verifies feature combinations build.
#
# Enforcement policy (fail-open locally with SKIP marker, fail-closed on demand):
#   - If cargo-hack is available: runs cargo hack --feature-powerset check -p guardian-proxy
#   - If cargo-hack is missing AND require_tools (CI=true or DO_HARNESS_REQUIRE_TOOLS=1):
#       echo "FAIL: cargo-hack is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1." and exit 1
#   - If cargo-hack is missing otherwise:
#       prints SKIP: marker and falls back to running cargo check over explicit powerset if cargo is present.

set -euo pipefail

# Fail-closed when the caller demands real gates: CI or an explicit opt-in.
require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if command -v cargo-hack >/dev/null 2>&1 || cargo hack --version >/dev/null 2>&1; then
    cargo hack --feature-powerset check -p guardian-proxy
else
    if require_tools; then
        echo "FAIL: cargo-hack is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: cargo-hack not installed; running cargo check over explicit powerset fallback."
    if command -v cargo >/dev/null 2>&1; then
        cargo check -p guardian-proxy --no-default-features
        cargo check -p guardian-proxy --no-default-features --features agt-governance
        cargo check -p guardian-proxy --no-default-features --features mcp-surface
        cargo check -p guardian-proxy --no-default-features --features agt-governance,mcp-surface
    fi
fi

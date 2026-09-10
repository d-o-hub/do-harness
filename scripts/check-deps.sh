#!/usr/bin/env bash
# check-deps.sh — enforces dependency direction and runs cargo-deny.
#
# Sensor: scripts/check-deps.sh
# Rule: do-harness-types must NOT depend on storage or adapters.
# Also runs `cargo deny check` when cargo-deny is installed.
# Enforcement policy (fail-open locally, fail-closed on demand), mirroring
# scripts/check-audit.sh: a missing cargo-deny must not silently green-light
# the sensor where real gates are required. Set DO_HARNESS_REQUIRE_TOOLS=1
# (or CI=true) to turn a missing tool into a FAIL instead of a WARN skip.

set -euo pipefail

# Fail-closed when the caller demands real gates: CI or an explicit opt-in.
require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TYPES_MANIFEST="$ROOT/crates/types/Cargo.toml"
FAIL=0

if grep -qE 'do-harness-(db|core|adapters|cli)' "$TYPES_MANIFEST"; then
    echo "FAIL: do-harness-types must not depend on storage or adapters."
    FAIL=1
fi

if command -v cargo-deny >/dev/null 2>&1; then
    (cd "$ROOT" && cargo deny check) || FAIL=1
elif require_tools; then
    echo "FAIL: cargo-deny is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
    FAIL=1
else
    echo "WARN: cargo-deny not installed; skipping deny check."
fi

if (( FAIL )); then
    exit 1
fi

echo "check-deps OK."
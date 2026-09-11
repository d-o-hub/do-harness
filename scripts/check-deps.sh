#!/usr/bin/env bash
# check-deps.sh — enforces dependency direction and runs cargo-deny.
#
# Sensor: scripts/check-deps.sh
# Rules:
#   1. do-harness-types (schema) must NOT depend on storage/adapters.
#   2. guardian-proxy (adjacent adapter) must NOT depend on the do-harness CLI.
# Both are checked over the real `cargo tree` closure, not just the manifest.
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

# Real closure check: cargo tree resolves the full normal-dependency graph.
# A tree failure is a hard error under required-tools mode, a WARN otherwise.
if closure="$(cd "$ROOT" && cargo tree -p do-harness-types --edges normal --prefix none 2>/dev/null)"; then
    if echo "$closure" | grep -qE '^(do-harness-db|do-harness|guardian-proxy|libsql)( |$)'; then
        echo "FAIL: do-harness-types closure must not contain storage or adapters."
        FAIL=1
    fi
elif require_tools; then
    echo "FAIL: cargo tree failed under required-tools mode."
    FAIL=1
else
    echo "WARN: cargo tree unavailable; skipping transitive closure check."
fi

if closure="$(cd "$ROOT" && cargo tree -p guardian-proxy --edges normal --prefix none 2>/dev/null)"; then
    if echo "$closure" | grep -qE '^do-harness( |$)'; then
        echo "FAIL: guardian-proxy must not depend on the do-harness CLI."
        FAIL=1
    fi
elif require_tools; then
    echo "FAIL: cargo tree failed under required-tools mode."
    FAIL=1
else
    echo "WARN: cargo tree unavailable; skipping guardian-proxy closure check."
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
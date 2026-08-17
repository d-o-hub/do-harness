#!/usr/bin/env bash
# check-deps.sh — enforces dependency direction and runs cargo-deny.
#
# Sensor: scripts/check-deps.sh
# Rule: do-harness-types must NOT depend on storage or adapters.
# Also runs `cargo deny check` when cargo-deny is installed.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TYPES_MANIFEST="$ROOT/crates/types/Cargo.toml"
FAIL=0

if grep -qE 'do-harness-(db|core|adapters|cli)' "$TYPES_MANIFEST"; then
    echo "FAIL: do-harness-types must not depend on storage or adapters."
    FAIL=1
fi

if command -v cargo-deny >/dev/null 2>&1; then
    (cd "$ROOT" && cargo deny check) || FAIL=1
else
    echo "WARN: cargo-deny not installed; skipping deny check."
fi

if (( FAIL )); then
    exit 1
fi

echo "check-deps OK."
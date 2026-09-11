#!/usr/bin/env bash
# check-deps.sh — cargo-deny policy check for this workspace.
#
# Sensor: scripts/check-deps.sh
# Without deny.toml the policy is unconfigured, so the sensor WARN-skips rather
# than failing a greenfield scaffold that has not adopted a deny policy yet.
# Missing cargo-deny fails closed when CI=true or DO_HARNESS_REQUIRE_TOOLS=1.
set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ ! -f "$ROOT/deny.toml" ]]; then
    echo "WARN: deny.toml not found; skipping cargo-deny policy check."
    exit 0
fi

if ! command -v cargo-deny >/dev/null 2>&1; then
    if require_tools; then
        echo "FAIL: cargo-deny is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "WARN: cargo-deny not installed; skipping deny check."
    exit 0
fi

cargo deny check

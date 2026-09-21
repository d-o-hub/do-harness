#!/usr/bin/env bash
# check-audit.sh — RustSec advisory scan for this workspace.
#
# Sensor: scripts/check-audit.sh
# Missing cargo-audit fails closed when CI=true or DO_HARNESS_REQUIRE_TOOLS=1,
# and is a WARN skip otherwise so offline local runs stay usable.
# Note: cargo-deny (deps sensor) also checks advisories; this sensor is the
# independent second gate using cargo-audit's own DB handling.
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

run_audit() {
    cargo audit --deny warnings "$@"
}

if ! run_audit; then
    # If cargo audit fails (e.g. due to a corrupted/truncated advisory-db file
    # on runner hosts), clear the local advisory-db cache and retry once.
    CARGO_HOME_DIR="${CARGO_HOME:-${HOME:-/tmp}/.cargo}"
    if [ -d "$CARGO_HOME_DIR/advisory-db" ]; then
        echo "WARN: cargo audit failed; clearing advisory-db and retrying..."
        rm -rf "$CARGO_HOME_DIR/advisory-db"
        run_audit
    else
        exit 1
    fi
fi

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

if ! run_audit "$@"; then
    # Retry once before clearing anything: the advisory-db under CARGO_HOME is
    # shared with any other sandbox verify running at the same time (parallel
    # dogfood runs update the same database), so a lock or contention failure
    # need not mean the cache is unusable, and clearing it would sabotage a
    # sibling process.
    sleep "${AUDIT_RETRY_DELAY_SECONDS:-2}"
    if ! run_audit "$@"; then
        # Fall back to the original recovery for a corrupted or truncated
        # advisory-db: clear the cache and retry once more.
        CARGO_HOME_DIR="${CARGO_HOME:-${HOME:-/tmp}/.cargo}"
        if [ -d "$CARGO_HOME_DIR/advisory-db" ]; then
            echo "WARN: cargo audit failed twice; clearing advisory-db and retrying..."
            # Best effort by design: files can be locked or read-only while
            # another sandbox verify uses the database, and under
            # `set -euo pipefail` a failing `rm -rf` would abort the retry this
            # branch exists to run (measured twice on the Windows runner, where
            # this copy killed the sensor before the retry).
            chmod -R u+w "$CARGO_HOME_DIR/advisory-db" 2>/dev/null || true
            rm -rf "$CARGO_HOME_DIR/advisory-db" || true
            run_audit "$@"
        else
            exit 1
        fi
    fi
fi

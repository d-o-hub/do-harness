#!/usr/bin/env bash
# check-package-contract.sh — verifies package contents and publish constraints for workspace crates.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAIL=0

if ! command -v cargo >/dev/null 2>&1; then
    if [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; then
        echo "FAIL: cargo is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    else
        echo "SKIP: cargo unavailable; skipping package contract check."
        exit 0
    fi
fi

# 1. Verify guardian-proxy remains publish = false
PROXY_MANIFEST="$ROOT/crates/guardian-proxy/Cargo.toml"
if ! grep -qE '^publish *= *false' "$PROXY_MANIFEST"; then
    echo "FAIL: crates/guardian-proxy/Cargo.toml must specify publish = false"
    FAIL=1
fi

check_crate_package() {
    local crate="$1"
    shift
    local required_files=("$@")

    echo "Checking package contents for $crate..."
    local list
    if ! list="$(cd "$ROOT" && cargo package --locked --allow-dirty --list -p "$crate" 2>/dev/null)"; then
        echo "FAIL: cargo package --locked --allow-dirty --list -p $crate failed"
        FAIL=1
        return
    fi

    # Require declared files
    for req in "${required_files[@]}"; do
        if ! echo "$list" | grep -qxF "$req"; then
            echo "FAIL: $crate package is missing required file: $req"
            FAIL=1
        fi
    done

    # Reject accidental inclusion of local state, target output, secrets
    local forbidden_regex='(\.do-harness/|\.git/|\.env|target/|secrets)'
    if echo "$list" | grep -qE "$forbidden_regex"; then
        echo "FAIL: $crate package contains forbidden files:"
        echo "$list" | grep -E "$forbidden_regex"
        FAIL=1
    fi
}

check_crate_package do-harness-types LICENSE
check_crate_package do-harness-db LICENSE
check_crate_package do-harness LICENSE README.md assets/compliance.md assets/methods.json assets/nextest.toml templates/AGENTS.md

if (( FAIL )); then
    echo "FAIL: package contract check failed."
    exit 1
fi

echo "check-package-contract OK."

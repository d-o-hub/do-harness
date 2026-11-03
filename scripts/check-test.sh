#!/usr/bin/env bash
# check-test.sh — the `test` sensor: run the workspace suite, never vacuously.
#
# Prefer cargo-nextest when available. The fallback keeps local verification
# useful on a clean toolchain and deliberately excludes doctests because the
# separate `doctest` sensor owns those examples.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if cargo nextest --version >/dev/null 2>&1; then
    exec cargo nextest run --workspace
fi

if ! listing="$(cargo test --workspace --lib --bins --tests -- --list 2>&1)"; then
    printf '%s\n' "$listing"
    echo "FAIL: cargo test --list failed; the suite could not be enumerated"
    exit 1
fi
count="$(printf '%s\n' "$listing" | grep -c ': test$' || true)"
if [ "${count:-0}" -eq 0 ]; then
    echo "FAIL: the workspace has zero tests; a green run would be vacuous"
    exit 1
fi

cargo test --workspace --lib --bins --tests

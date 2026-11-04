#!/usr/bin/env bash
# check-test.sh — the `test` sensor: run the workspace suite, never vacuously.
#
# Prefers cargo-nextest, which fails a run that executes zero tests. Falls back
# to `cargo test` when nextest is absent, and applies the same zero-test guard
# itself: `cargo test` exits 0 when a workspace has no tests, so the fallback
# would otherwise be a green gate that executed nothing (AGENTS.md section 4,
# "No Vacuous Gates"). Keep doctests out of this sensor: `doctest` owns those
# examples, and overlapping them makes the fast fallback do duplicate work.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if cargo nextest --version >/dev/null 2>&1; then
    exec cargo nextest run --workspace
fi

# `--list` enumerates the tests without running them. A compile failure must
# surface as itself rather than as "zero tests", so its status is checked first.
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

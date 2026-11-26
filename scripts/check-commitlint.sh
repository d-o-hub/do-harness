#!/usr/bin/env bash
# check-commitlint.sh — enforces conventional commits with lowercase subjects.
#
# Sensor: scripts/check-commitlint.sh
# Thin delegating wrapper: the implementation is the native
# `do-harness helpers commitlint` subcommand. The wrapper keeps sensor argv
# rows like ["bash", "scripts/check-commitlint.sh"] working in existing
# adopter configs and resolves the same binary the managed hooks do
# (DO_HARNESS_BIN, then PATH, then target/release/do-harness[.exe]).
#
# Exit codes match the native command: 0 pass, 1 a subject failed lint, 2
# usage/environment errors.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Resolve the binary the way the managed hooks do. A relative DO_HARNESS_BIN
# resolves against this repository rather than the caller's working directory,
# and the checkout's own release build outranks an installed binary on PATH
# when it is newer — otherwise a development checkout lints with the release it
# installed months ago.
BIN="${DO_HARNESS_BIN:-}"
if [[ -n "$BIN" && "$BIN" != /* && ! -x "$BIN" && -x "$ROOT/$BIN" ]]; then
    BIN="$ROOT/$BIN"
fi
if [[ -z "${DO_HARNESS_BIN:-}" ]]; then
    REPO_BIN="$ROOT/target/release/do-harness"
    [[ -x "$REPO_BIN" ]] || { [[ -x "$REPO_BIN.exe" ]] && REPO_BIN="$REPO_BIN.exe"; }
    PATH_BIN="$(command -v do-harness 2>/dev/null || true)"
    if [[ -z "$PATH_BIN" ]] || { [[ -x "$REPO_BIN" ]] && [[ "$REPO_BIN" -nt "$PATH_BIN" ]]; }; then
        BIN="${REPO_BIN:-$PATH_BIN}"
    else
        BIN="$PATH_BIN"
    fi
fi
if [[ -z "$BIN" || ! -x "$BIN" ]]; then
    echo "check-commitlint: do-harness binary not found. Set DO_HARNESS_BIN, add do-harness to PATH, or build with: cargo build --release -p do-harness" >&2
    exit 2
fi

exec "$BIN" --root "$ROOT" helpers commitlint "$@"

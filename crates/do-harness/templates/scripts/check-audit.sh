#!/usr/bin/env bash
# check-audit.sh — scans dependencies against the RustSec advisory DB.
#
# Sensor: scripts/check-audit.sh
# Thin delegating wrapper: the implementation is the native
# `do-harness helpers audit` subcommand. The wrapper keeps sensor argv rows
# like ["bash", "scripts/check-audit.sh"] working in existing adopter configs
# and resolves the same binary the managed hooks do (DO_HARNESS_BIN, then
# PATH, then target/release/do-harness[.exe]).
#
# Exit codes match the native command: 0 pass, 1 the scan found issues, 2
# usage/environment errors.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BIN="${DO_HARNESS_BIN:-}"
if [[ -z "$BIN" ]]; then
    BIN="$(command -v do-harness 2>/dev/null || true)"
fi
if [[ -z "$BIN" ]]; then
    BIN="$ROOT/target/release/do-harness"
    if [[ ! -x "$BIN" && -x "$BIN.exe" ]]; then
        BIN="$BIN.exe"
    fi
    if [[ ! -x "$BIN" ]]; then
        echo "check-audit: do-harness binary not found. Set DO_HARNESS_BIN, add do-harness to PATH, or build with: cargo build --release -p do-harness" >&2
        exit 2
    fi
fi

exec "$BIN" --root "$ROOT" helpers audit "$@"

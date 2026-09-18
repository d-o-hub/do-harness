#!/usr/bin/env bash
# check-dora.sh — DORA threshold sensor (thin shim over `do-harness dora`).
#
# Sensor: scripts/check-dora.sh
# The derivation lives in Rust (`crates/do-harness/src/dora/`); this script only
# resolves the binary the way the managed hooks do and forwards the sensor
# verdict.
#
# Exit-code contract: `do-harness dora` exits 1 on a threshold breach and 2 on a
# usage/config/discovery error. Exit 1 is mapped to sensor success because the
# gate for a count that must only go down is the blessed `FINDINGS:` ratchet,
# not the process exit code: a warn-severity sensor that keeps failing is
# quarantined after 3 consecutive strikes and then never runs again, so
# propagating exit 1 would make a permanently-breaching metric silently stop
# being measured within three `--record` runs (and the managed hooks always
# pass `--record`). Exit 2 is propagated as a hard failure: a collector that
# cannot read git history must never read as healthy.
#
# Binary resolution order matches hook_script.rs RESOLVE_BIN: an explicit
# DO_HARNESS_BIN, then PATH, then the repo-local release build (including the
# .exe suffix so it works under Git Bash on Windows).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -n "${DO_HARNESS_BIN:-}" && -x "$DO_HARNESS_BIN" ]]; then
    BIN="$DO_HARNESS_BIN"
elif command -v do-harness >/dev/null 2>&1; then
    BIN="$(command -v do-harness)"
else
    BIN="$ROOT/target/release/do-harness"
    [[ -x "$BIN" ]] || { [[ -x "$BIN.exe" ]] && BIN="$BIN.exe"; }
fi
if [[ ! -x "$BIN" ]]; then
    echo "FAIL: do-harness binary not found. Set DO_HARNESS_BIN, add do-harness to PATH, or build with: cargo build --release -p do-harness" >&2
    exit 1
fi

# Stream output directly (not through a substitution) so the FINDINGS: and
# COVERAGE: markers keep their order in the captured sensor output.
set +e
"$BIN" --root "$ROOT" dora "$@"
code=$?
set -e

# 0 = no breach, 1 = breach (advisory; gated by the blessed findings ratchet).
if [[ "$code" -eq 0 || "$code" -eq 1 ]]; then
    exit 0
fi
exit "$code"

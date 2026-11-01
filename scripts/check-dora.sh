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
# Binary resolution order matches the documented resolution order of the managed
# hooks' RESOLVE_BIN block: an explicit DO_HARNESS_BIN, then the workspace
# release build (`${CARGO_TARGET_DIR:-target}/release/do-harness`, including the
# .exe suffix so it works under Git Bash on Windows), then PATH.
#
# The workspace build is preferred whenever it exists: a PATH-installed or
# cargo-installed `do-harness` can predate the checkout, and running it makes
# `verify` dogfood an old CLI and report failures the working tree does not
# have. The old `-nt` tie-break existed only to let the newer of the two win;
# with the workspace build ranked above PATH that comparison has nothing left to
# decide, while the reverse case — a stale *workspace* build — is exactly what
# the "older than workspace sources" warning below reports.
#
# `${CARGO_TARGET_DIR}` is honoured, and a relative value resolves against the
# repository root: a build redirected away from `target/` otherwise leaves this
# shim executing a binary that silently predates the schema (AGENTS.md, Hooks).
#
# This shim deliberately does not source scripts/lib.sh, unlike the sibling
# sensors: the DORA acceptance fixtures copy it on its own into a scratch
# repository (crates/do-harness/tests/support/dora.rs), so it has to run with
# nothing but itself. That is why the root and the resolver are written out here
# rather than shared.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

TARGET_DIR="${CARGO_TARGET_DIR:-target}"
case "$TARGET_DIR" in
    /* | [A-Za-z]:[\\/]*) ;;
    *) TARGET_DIR="$ROOT/$TARGET_DIR" ;;
esac
REPO_BIN="$TARGET_DIR/release/do-harness"
[[ -x "$REPO_BIN" ]] || { [[ -x "$REPO_BIN.exe" ]] && REPO_BIN="$REPO_BIN.exe"; }

if [[ -n "${DO_HARNESS_BIN:-}" && -x "$DO_HARNESS_BIN" ]]; then
    BIN="$DO_HARNESS_BIN"
elif [[ -x "$REPO_BIN" ]]; then
    BIN="$REPO_BIN"
elif command -v do-harness >/dev/null 2>&1; then
    BIN="$(command -v do-harness)"
else
    echo "FAIL: do-harness binary not found. Set DO_HARNESS_BIN, add do-harness to PATH, or build with: cargo build --release -p do-harness" >&2
    # Discovery failure, not a metric verdict: exit 2 so it is a hard failure
    # rather than the advisory breach code (exit 1 would be indistinguishable
    # from a threshold breach and would accrue strikes toward quarantine).
    exit 2
fi

if [[ -n "$(find "$ROOT/crates" -name '*.rs' -newer "$BIN" -print -quit 2>/dev/null)" ]]; then
    echo "warning: $BIN is older than workspace sources; rebuild with: cargo build --release -p do-harness (or set DO_HARNESS_BIN)" >&2
fi

# Stream output directly (not through a substitution) so the FINDINGS: and
# COVERAGE: markers keep their order in the captured sensor output.
set +e
"$BIN" --root "$ROOT" dora "$@"
code=$?
set -e

# 0 = no breach, 1 = breach (advisory; gated by the blessed findings ratchet).
if [[ "$code" -eq 0 || "$code" -eq 1 ]]; then
    # The ratchet is the gate, so a missing ceiling is worth saying out loud:
    # with no `dora` entry in plans/baselines.json a breach is reported but
    # nothing turns it into a gate. That is the correct report-only behavior
    # before the first bless, and silence here would hide it after one.
    if [[ "$code" -eq 1 && ! -f "$ROOT/plans/baselines.json" ]]; then
        echo "note: plans/baselines.json is absent, so this breach is reported without a ratchet ceiling; run 'verify --record --only dora --bless' to pin it" >&2
    fi
    exit 0
fi
exit "$code"

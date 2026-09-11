#!/usr/bin/env bash
# check-audit.sh — scans dependencies against the RustSec advisory DB.
#
# Sensor: scripts/check-audit.sh
# Enforcement policy (fail-open locally, fail-closed on demand):
#   - cargo-audit missing AND (CI=true OR DO_HARNESS_REQUIRE_TOOLS=1) -> FAIL.
#   - cargo-audit missing otherwise -> WARN skip (keeps offline
#     `do-harness verify --fail-fast` usable; advisory scanning is still
#     enforced by the CI pipeline).
# NOTE: a WARN skip exits 0 and is indistinguishable from a pass downstream.
# Set DO_HARNESS_REQUIRE_TOOLS=1 in any context where a skip must not
# masquerade as green (pre-push, release gates). CI sets it explicitly in
# .github/workflows/verify.yml alongside CI=true.
# Note: cargo-deny (deps sensor) also checks advisories; this sensor is the
# independent second gate using cargo-audit's own DB handling.

set -euo pipefail

# Fail-closed when the caller demands real gates: CI or an explicit opt-in.
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

# Ignore RUSTSEC-2026-0097 (rand 0.8.5 unsound: `thread_rng` aliasing, only
# triggerable with a custom logger calling back into ThreadRng during reseed;
# advisory: https://rustsec.org/advisories/RUSTSEC-2026-0097, alias
# GHSA-cq8v-f236-94qc, patched in rand >= 0.8.6).
# Scope justification: rand 0.8.5 is pulled ONLY by the optional, pre-GA
# agent-governance dep (verified via Cargo.lock reverse-deps), never by
# first-party code. Revisit when agent-governance revs past 3.2.2
# (`cargo update -p agent-governance --precise <ver>`) or at GA promotion
# (plans/agt-governance-epic.md), when the pin — and this ignore — go away.
# Advisory-DB pin decision: intentionally unpinned. RustSec advisories are the
# point of the scan, and a pinned snapshot would hide post-pin disclosures;
# reproducibility comes from Cargo.lock (deps) and `cargo audit --deny warnings`.
cargo audit --deny warnings --ignore RUSTSEC-2026-0097

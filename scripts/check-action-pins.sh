#!/usr/bin/env bash
# check-action-pins.sh — immutable-reference gate for GitHub Actions.
#
# Sensor: scripts/check-action-pins.sh
# Coverage:
#   - .github/workflows/**/*.yml and .github/workflows/**/*.yaml
#   - scripts/check-action-pins.sh
#   - scripts/check-action-pins.py
#
# What it proves: every external action, reusable workflow, and Docker action
# image referenced by a workflow is pinned to an immutable identifier — a full
# 40-character commit SHA for actions and reusable workflows, a sha256 digest
# for Docker images. `actionlint` proves the workflow is syntactically valid;
# it does not say the reference cannot move under you.
#
# The check is offline: it never calls the GitHub API and never resolves a
# tag, so it proves the *shape* of a reference, not that a commit exists.
#
# Enforcement policy (fail-open locally, fail-closed on demand), mirroring
# scripts/check-actionlint.sh:
#   - python3 missing AND (CI=true OR DO_HARNESS_REQUIRE_TOOLS=1) -> FAIL.
#   - python3 missing otherwise -> SKIP (keeps an offline dev checkout
#     usable; CI and the release preflight install it).
#   - PyYAML missing -> the same policy: SKIP locally, FAIL when required.
#   - zero workflow files discovered -> FAIL (no vacuous gate).
set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v python3 >/dev/null 2>&1; then
    if require_tools; then
        echo "FAIL: python3 is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: python3 not installed; skipping the immutable action-reference check (install python3)."
    exit 0
fi

python3 "$ROOT/scripts/check-action-pins.py" --root "$ROOT"

#!/usr/bin/env bash
# lib.sh — shared helpers for the repository's shell scripts.
#
# Source it; do not execute it:
#
#   # shellcheck disable=SC1091 # resolved at runtime from this script's directory
#   source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
#
# Three properties matter, because every caller runs under `set -euo pipefail`:
#   - sourcing defines functions and nothing else: no output, no variable
#     assignment, no `cd`, no dependency on the caller's working directory;
#   - every path is derived from this file's own location (`BASH_SOURCE`), never
#     from `$PWD`, so a caller works when invoked from anywhere;
#   - each helper keeps its callers' existing semantics exactly, including the
#     fail-open/fail-closed policy (SKIP in a dev checkout, FAIL when the caller
#     demands real gates).
#
# Policy stays with the caller. This file holds only the mechanics the scripts
# were duplicating verbatim; the SKIP/FAIL message, its remediation hint, and
# whether a missing tool is fatal are decisions each script still makes, because
# they are its contract, not shared code.
#
# Deliberately NOT a caller of this file: the scripts that ship or run as
# standalone copies — `install.sh` (piped into POSIX `sh`), `check-dora.sh`,
# `check-artifact-provenance.sh`, `check-rust-binary-provenance.sh` and
# `generate-large-fixtures.sh` (each copied on its own into a fixture tree by an
# integration test), and the `init` templates under
# `crates/do-harness/templates/scripts/` (scaffolded into an adopting
# repository, which has no `scripts/lib.sh`). Sourcing from any of them would
# break a supported invocation to save a few lines.

# dh_repo_root — absolute path to the repository root, derived from this file's
# own location. Prints the path; it never changes the caller's directory.
dh_repo_root() {
    local root
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)" || return 1
    printf '%s\n' "$root"
}

# dh_require_tools — is a missing external tool a hard failure in this run?
#
# The shared fail-open/fail-closed policy: a dev checkout SKIPs (exit 0 with a
# SKIP marker the runner surfaces as a warned sensor), while CI and any caller
# that sets DO_HARNESS_REQUIRE_TOOLS=1 turn the same condition into a FAIL.
# A script that widens the policy (e.g. its own `--strict` flag, as
# check-rust-binary-provenance.sh does) keeps a local predicate instead.
dh_require_tools() {
    [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]
}

# dh_sha256_of <file> — lowercase hex SHA-256 of <file>, or empty when no
# hashing tool is installed. The fallback order is the portable one
# (coreutils, then macOS/Perl `shasum`, then `openssl`).
#
# An empty digest is not a verdict: callers decide what it means (a FAIL, a
# SKIP, a "cannot compute" note), so the failure mode stays local policy.
dh_sha256_of() {
    local file="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file" | awk '{print $1}'
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "$file" | awk '{print $NF}'
    fi
}

# dh_workspace_version [root] — the `[workspace.package].version` value from
# <root>/Cargo.toml (`root` defaults to this repository).
#
# The whole quoted value is printed, not a numeric prefix: a malformed
# `2.1.0-beta` must reach the caller's validator instead of being read as
# `2.1.0`. A caller that needs exactly one value appends `head -1`.
dh_workspace_version() {
    local root="${1:-}"
    if [[ -z "$root" ]]; then
        root="$(dh_repo_root)" || return 1
    fi
    sed -n '/^\[workspace.package\]/,/^\[/ s/^version *= *"\([^"]*\)"/\1/p' \
        "$root/Cargo.toml"
}

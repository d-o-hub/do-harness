#!/usr/bin/env bash
# check-rust-binary-provenance.sh — Rust binary provenance sensor (cargo audit bin).
#
# Verifies already-built Rust binaries: each artifact must carry the auditable
# dependency metadata cargo-auditable embeds, and `cargo audit bin` must report
# no vulnerability in what those binaries embed. The sensor never builds an
# artifact and never fetches anything on its own (cargo-audit manages its own
# advisory database).
#
# Inputs via environment or command-line arguments:
#   ARTIFACT_PATHS: File path(s) to verify (space or comma separated). Required.
#   EXPECTED_DIGEST: Optional expected hex SHA-256 digest, applied to each artifact.
#   CARGO_AUDIT_BIN: Optional cargo-audit binary override; invoked the way cargo
#                    invokes a subcommand, i.e. `<bin> audit bin <paths...>`.
#   STRICT_MODE / --strict: Fail (instead of SKIP) when cargo-audit is unavailable.
#
# Evidence: one `COVERAGE: <json>` line plus a `FINDINGS: <n>` marker. `findings`
# counts advisory ids (RUSTSEC-YYYY-NNNN), so tool/setup failures stay
# distinguishable from vulnerability findings.

set -euo pipefail

# cargo-auditable stores dependency metadata in a link-time section: .dep-v0 for
# ELF and PE, __dep for Mach-O. Section names are stored verbatim in the file's
# section table, so a raw scan is format-agnostic and needs no per-platform
# reader. The tool's own signals cannot stand in for this probe: with
# cargo-audit >= 0.22 a binary without auditable metadata still exits 0 after
# recovering a partial dependency list from panic messages, and "was not built
# with 'cargo auditable'" is warning prose, not a machine signal.
has_auditable_metadata() {
    LC_ALL=C grep -qFa -e '.dep-v0' -e '__dep' -- "$1"
}

compute_sha256() {
    local file="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file" | awk '{print $1}'
    elif command -v openssl >/dev/null 2>&1; then
        openssl dgst -sha256 "$file" | awk '{print $NF}'
    else
        echo "unsupported"
    fi
}

# Escape a value for a single-line JSON string.
json_escape() {
    local value="$1"
    value="${value//\\/\\\\}"
    value="${value//\"/\\\"}"
    printf '%s' "$value"
}

require_tools() {
    [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" || "${STRICT_MODE:-}" == "true" ]]
}

# Resolve the cargo-audit invocation. cargo runs a subcommand as
# `<cargo-audit> audit <subcommand>`, so a standalone binary needs the leading
# `audit`; `cargo audit …` supplies it through cargo.
AUDIT_CMD=()
TOOL_VER=""
resolve_cargo_audit() {
    if [[ -n "${CARGO_AUDIT_BIN:-}" ]]; then
        if command -v "$CARGO_AUDIT_BIN" >/dev/null 2>&1; then
            AUDIT_CMD=("$CARGO_AUDIT_BIN" audit)
            TOOL_VER="$("$CARGO_AUDIT_BIN" --version 2>/dev/null || printf '%s' "$CARGO_AUDIT_BIN")"
            return 0
        fi
        return 1
    fi
    if command -v cargo >/dev/null 2>&1 && cargo audit --version >/dev/null 2>&1; then
        AUDIT_CMD=(cargo audit)
        # `cargo audit --version` reports "cargo-audit-audit <v>" because cargo
        # supplies the subcommand name; prefer the binary's own version.
        if command -v cargo-audit >/dev/null 2>&1; then
            TOOL_VER="$(cargo-audit --version 2>/dev/null || printf 'cargo-audit')"
        else
            TOOL_VER="$(cargo audit --version 2>/dev/null || printf 'cargo audit')"
        fi
        return 0
    fi
    if command -v cargo-audit >/dev/null 2>&1; then
        AUDIT_CMD=(cargo-audit audit)
        TOOL_VER="$(cargo-audit --version 2>/dev/null || printf 'cargo-audit')"
        return 0
    fi
    return 1
}

POSITIONAL_ARGS=()
STRICT_MODE=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact|-a)
            ARTIFACT_PATHS="${2:-}"
            shift 2
            ;;
        --digest|-d)
            EXPECTED_DIGEST="${2:-}"
            shift 2
            ;;
        --strict)
            STRICT_MODE=true
            shift
            ;;
        *)
            POSITIONAL_ARGS+=("$1")
            shift
            ;;
    esac
done

if [[ ${#POSITIONAL_ARGS[@]} -gt 0 && -z "${ARTIFACT_PATHS:-}" ]]; then
    ARTIFACT_PATHS="${POSITIONAL_ARGS[0]}"
fi

ARTIFACT_PATHS="${ARTIFACT_PATHS:-}"
EXPECTED_DIGEST="${EXPECTED_DIGEST:-}"

if [[ -z "$ARTIFACT_PATHS" ]]; then
    echo "FAIL: ARTIFACT_PATHS is required."
    printf 'COVERAGE: {"artifact": "", "digest": null, "status": "fail", "reason": "missing ARTIFACT_PATHS input", "tool": null, "auditable_metadata": false, "findings": 0, "artifacts": []}\n'
    echo "FINDINGS: 0"
    exit 1
fi

if ! resolve_cargo_audit; then
    if require_tools; then
        echo "FAIL: cargo-audit is required (--strict or CI) but unavailable."
        printf 'COVERAGE: {"artifact": "%s", "digest": null, "status": "fail", "reason": "cargo-audit tool unavailable", "tool": null, "auditable_metadata": false, "findings": 0, "artifacts": []}\n' \
            "$(json_escape "$ARTIFACT_PATHS")"
        echo "FINDINGS: 0"
        exit 1
    fi
    echo "SKIP: cargo-audit is unavailable; skipping the rust binary provenance scan."
    printf 'COVERAGE: {"artifact": "%s", "digest": null, "status": "warn", "reason": "cargo-audit tool unavailable", "tool": null, "auditable_metadata": false, "findings": 0, "artifacts": []}\n' \
        "$(json_escape "$ARTIFACT_PATHS")"
    echo "FINDINGS: 0"
    exit 0
fi

IFS=', ' read -r -a PATHS <<< "$ARTIFACT_PATHS"

FAILURES=0
FIRST_REASON=""
LAST_ART=""
LAST_DIGEST="null"
PROBED=0
WITH_METADATA=0
TOTAL_FINDINGS=0
ARTIFACTS_JSON=""
AUDITABLE_PATHS=()

fail() {
    FAILURES=$((FAILURES + 1))
    if [[ -z "$FIRST_REASON" ]]; then
        FIRST_REASON="$1"
    fi
    echo "FAIL: $1"
    return 0
}

add_artifact() {
    ARTIFACTS_JSON+="{\"path\": \"$(json_escape "$1")\", \"digest\": \"$2\", \"auditable_metadata\": $3}, "
}

for path in "${PATHS[@]}"; do
    [[ -z "$path" ]] && continue
    LAST_ART="$path"

    if [[ ! -f "$path" ]]; then
        LAST_DIGEST="null"
        fail "artifact file not found: $path"
        continue
    fi

    digest="$(compute_sha256 "$path")"
    if [[ "$digest" == "unsupported" ]]; then
        LAST_DIGEST="null"
        fail "no sha256 tool available to hash $path"
        continue
    fi
    LAST_DIGEST="\"$digest\""
    PROBED=$((PROBED + 1))

    if [[ -n "$EXPECTED_DIGEST" && "$digest" != "$EXPECTED_DIGEST" ]]; then
        add_artifact "$path" "$digest" false
        fail "digest mismatch for $path: expected $EXPECTED_DIGEST, got $digest"
        continue
    fi

    if ! has_auditable_metadata "$path"; then
        add_artifact "$path" "$digest" false
        fail "auditable dependency metadata missing in $path; build it with 'cargo auditable build'"
        continue
    fi

    add_artifact "$path" "$digest" true
    WITH_METADATA=$((WITH_METADATA + 1))
    AUDITABLE_PATHS+=("$path")
done

if [[ ${#AUDITABLE_PATHS[@]} -gt 0 ]]; then
    AUDIT_OUT=""
    AUDIT_STATUS=0
    AUDIT_OUT="$("${AUDIT_CMD[@]}" bin "${AUDITABLE_PATHS[@]}" 2>&1)" || AUDIT_STATUS=$?
    if [[ $AUDIT_STATUS -eq 0 ]]; then
        echo "PASS: ${#AUDITABLE_PATHS[@]} artifact(s) audited with ${AUDIT_CMD[*]} (no advisories)"
    else
        TOTAL_FINDINGS="$(printf '%s\n' "$AUDIT_OUT" | grep -oE 'RUSTSEC-[0-9]{4}-[0-9]{4}' | sort -u | wc -l | tr -d ' ' || true)"
        if [[ -n "$TOTAL_FINDINGS" && "$TOTAL_FINDINGS" -gt 0 ]]; then
            fail "security vulnerabilities found in the embedded dependencies of ${AUDITABLE_PATHS[*]} (${TOTAL_FINDINGS} advisory finding(s))"
        else
            fail "cargo audit bin failed for ${AUDITABLE_PATHS[*]}: $(printf '%s\n' "$AUDIT_OUT" | head -n 1)"
        fi
    fi
fi

AUDITABLE_BOOL=false
if [[ $PROBED -gt 0 && $WITH_METADATA -eq $PROBED ]]; then
    AUDITABLE_BOOL=true
fi

ARTIFACTS_JSON="${ARTIFACTS_JSON%, }"

if [[ $FAILURES -gt 0 ]]; then
    printf 'COVERAGE: {"artifact": "%s", "digest": %s, "status": "fail", "reason": "%s", "tool": "%s", "auditable_metadata": %s, "findings": %s, "artifacts": [%s]}\n' \
        "$(json_escape "$LAST_ART")" "$LAST_DIGEST" "$(json_escape "$FIRST_REASON")" \
        "$(json_escape "$TOOL_VER")" "$AUDITABLE_BOOL" "$TOTAL_FINDINGS" "$ARTIFACTS_JSON"
    echo "FINDINGS: $TOTAL_FINDINGS"
    exit 1
fi

printf 'COVERAGE: {"artifact": "%s", "digest": %s, "status": "pass", "reason": null, "tool": "%s", "auditable_metadata": %s, "findings": 0, "artifacts": [%s]}\n' \
    "$(json_escape "$LAST_ART")" "$LAST_DIGEST" "$(json_escape "$TOOL_VER")" "$AUDITABLE_BOOL" "$ARTIFACTS_JSON"
echo "FINDINGS: 0"
exit 0

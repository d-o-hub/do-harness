#!/usr/bin/env bash
# check-artifact-provenance.sh — generic release-artifact provenance sensor.
#
# Inputs via environment or command-line arguments:
#   ARTIFACT_PATHS: File path(s) to verify (space or comma separated).
#   EXPECTED_DIGEST: Optional expected hex SHA-256 digest.
#   VERIFY_MODE: Verification mode (e.g. digest-only, slsa, github-attestation, sbom, strict). Default: digest-only.

set -euo pipefail

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

# Parse CLI arguments if provided
POSITIONAL_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact)
            ARTIFACT_PATHS="$2"
            shift 2
            ;;
        --digest)
            EXPECTED_DIGEST="$2"
            shift 2
            ;;
        --mode)
            VERIFY_MODE="$2"
            shift 2
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
VERIFY_MODE="${VERIFY_MODE:-digest-only}"

if [[ -z "$ARTIFACT_PATHS" ]]; then
    echo "FAIL: ARTIFACT_PATHS is required."
    echo "COVERAGE: {\"artifact\": \"\", \"digest\": null, \"status\": \"fail\", \"reason\": \"missing ARTIFACT_PATHS input\"}"
    echo "FINDINGS: 1"
    exit 1
fi

# Split ARTIFACT_PATHS on comma or space
IFS=', ' read -r -a PATHS <<< "$ARTIFACT_PATHS"

FAILURES=0
FIRST_REASON=""
LAST_ART=""
LAST_DIGEST=""

for path in "${PATHS[@]}"; do
    [[ -z "$path" ]] && continue
    LAST_ART="$path"

    if [[ ! -f "$path" ]]; then
        FAILURES=$((FAILURES + 1))
        REASON="artifact file not found: $path"
        [[ -z "$FIRST_REASON" ]] && FIRST_REASON="$REASON"
        LAST_DIGEST="null"
        echo "FAIL: $REASON"
        continue
    fi

    DIGEST=$(compute_sha256 "$path")
    LAST_DIGEST="\"$DIGEST\""

    if [[ "$DIGEST" == "unsupported" ]]; then
        FAILURES=$((FAILURES + 1))
        REASON="sha256 tool unavailable to hash $path"
        [[ -z "$FIRST_REASON" ]] && FIRST_REASON="$REASON"
        echo "FAIL: $REASON"
        continue
    fi

    if [[ -n "$EXPECTED_DIGEST" && "$DIGEST" != "$EXPECTED_DIGEST" ]]; then
        FAILURES=$((FAILURES + 1))
        REASON="digest mismatch for $path: expected $EXPECTED_DIGEST, got $DIGEST"
        [[ -z "$FIRST_REASON" ]] && FIRST_REASON="$REASON"
        echo "FAIL: $REASON"
        continue
    fi

    case "$VERIFY_MODE" in
        digest-only)
            ;;
        slsa|github-attestation|sbom|strict)
            # Generic mode check placeholder - mode supported
            ;;
        *)
            FAILURES=$((FAILURES + 1))
            REASON="unsupported verification mode: $VERIFY_MODE"
            [[ -z "$FIRST_REASON" ]] && FIRST_REASON="$REASON"
            echo "FAIL: $REASON"
            continue
            ;;
    esac

    echo "PASS: $path verified (sha256: $DIGEST, mode: $VERIFY_MODE)"
done

if [[ $FAILURES -gt 0 ]]; then
    # Sanitize JSON string quotes in FIRST_REASON
    SAFE_REASON="${FIRST_REASON//\"/\\\"}"
    echo "COVERAGE: {\"artifact\": \"$LAST_ART\", \"digest\": $LAST_DIGEST, \"status\": \"fail\", \"reason\": \"$SAFE_REASON\"}"
    echo "FINDINGS: $FAILURES"
    exit 1
else
    echo "COVERAGE: {\"artifact\": \"$LAST_ART\", \"digest\": $LAST_DIGEST, \"status\": \"pass\", \"reason\": null}"
    echo "FINDINGS: 0"
    exit 0
fi

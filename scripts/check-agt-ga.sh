#!/usr/bin/env bash
# check-agt-ga.sh — informational AGT SDK GA watch (not a verify sensor).
#
# Queries crates.io + the toolkit's latest GitHub release and prints a
# verdict for the `chore-agt-promotion` gate in plans/agt-governance-epic.md:
#   VERDICT=GA      description dropped "preview" AND release notes declare GA
#   VERDICT=NOT_GA  both sources fetched, GA criteria not met (current state)
#   VERDICT=UNKNOWN network or parse failure (never GA on error)
#
# Crate name: the Rust SDK was renamed `agent-governance` -> `agentmesh`.
# The old crate is frozen at 3.2.2 (2026-04-22) and is no longer published, so
# watching it could never report GA. The live crate is `agentmesh`; the retired
# name is queried too, as evidence, and never gates the verdict.
#
# Exit 0 on any verdict; exit 2 on usage errors or missing jq/curl.
# Fixture mode (hermetic, no network):
#   check-agt-ga.sh --crate-json FILE --release-json FILE
#   check-agt-ga.sh --crate-json FILE --release-json FILE --legacy-crate-json FILE
#
# Spike finding: crates.io returns 403 without a User-Agent header.

set -euo pipefail

USER_AGENT="do-harness-ga-watch/1.0"
CRATE="agentmesh"
LEGACY_CRATE="agent-governance"
CRATE_URL="https://crates.io/api/v1/crates/$CRATE"
LEGACY_CRATE_URL="https://crates.io/api/v1/crates/$LEGACY_CRATE"
RELEASE_URL="https://api.github.com/repos/microsoft/agent-governance-toolkit/releases/latest"
CRATE_JSON=""
LEGACY_CRATE_JSON=""
RELEASE_JSON=""

while (( $# )); do
    case "$1" in
        --crate-json)
            CRATE_JSON="${2:-}"; shift 2 ;;
        --legacy-crate-json)
            LEGACY_CRATE_JSON="${2:-}"; shift 2 ;;
        --release-json)
            RELEASE_JSON="${2:-}"; shift 2 ;;
        --help|-h)
            sed -n '2,22p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *)
            echo "check-agt-ga: unknown argument: $1 (see --help)" >&2; exit 2 ;;
    esac
done

for dep in curl jq; do
    if ! command -v "$dep" >/dev/null 2>&1; then
        echo "check-agt-ga: missing required command: $dep" >&2
        exit 2
    fi
done

fetch() {
    curl --fail --silent --show-error --max-time 20 \
        --retry 3 --retry-connrefused --retry-delay 1 \
        -A "$USER_AGENT" -H "Accept: application/json" "$1"
}

# The sources are printed, not just used: the predecessor of this script
# watched a crate name upstream had retired, so every run reported NOT_GA and
# the promotion gate could never fire. Naming the queried URLs makes that class
# of silent blindness visible in the CI log.
echo "source: crate=$CRATE_URL"
echo "source: legacy=$LEGACY_CRATE_URL"
echo "source: release=$RELEASE_URL"

unknown() {
    echo "crate: ${1:-unavailable}"
    echo "release: ${2:-unavailable}"
    echo "VERDICT=UNKNOWN"
}

legacy_raw=""
if [[ -n "$CRATE_JSON" || -n "$RELEASE_JSON" || -n "$LEGACY_CRATE_JSON" ]]; then
    if [[ -z "$CRATE_JSON" || -z "$RELEASE_JSON" ]]; then
        echo "check-agt-ga: --crate-json and --release-json must be passed together" >&2
        exit 2
    fi
    crate_raw="$(cat "$CRATE_JSON")"
    release_raw="$(cat "$RELEASE_JSON")"
    if [[ -n "$LEGACY_CRATE_JSON" ]]; then
        legacy_raw="$(cat "$LEGACY_CRATE_JSON")"
    fi
else
    if ! crate_raw="$(fetch "$CRATE_URL" 2>/dev/null)"; then
        unknown "fetch failed" "skipped"
        exit 0
    fi
    if ! release_raw="$(fetch "$RELEASE_URL" 2>/dev/null)"; then
        unknown "fetched" "fetch failed"
        exit 0
    fi
    # Supplementary evidence only: the retired crate cannot gate the verdict, so
    # a fetch failure here is reported and ignored rather than degrading to
    # UNKNOWN.
    if ! legacy_raw="$(fetch "$LEGACY_CRATE_URL" 2>/dev/null)"; then
        legacy_raw=""
    fi
fi

if ! desc="$(echo "$crate_raw" | jq -r '.crate.description // empty' 2>/dev/null)" \
    || [[ -z "$desc" ]]; then
    unknown "parse failed" "parse failed"
    exit 0
fi
version="$(echo "$crate_raw" | jq -r '.crate.max_stable_version // "unknown"' 2>/dev/null || echo unknown)"
tag="$(echo "$release_raw" | jq -r '.tag_name // "unknown"' 2>/dev/null || echo unknown)"
prerelease="$(echo "$release_raw" | jq -r '.prerelease // false' 2>/dev/null || echo false)"
body="$(echo "$release_raw" | jq -r '.body // ""' 2>/dev/null || echo "")"
name="$(echo "$release_raw" | jq -r '.name // ""' 2>/dev/null || echo "")"

echo "crate: name=$CRATE version=$version description=$desc"
if [[ -n "$legacy_raw" ]]; then
    legacy_version="$(echo "$legacy_raw" | jq -r '.crate.max_stable_version // "unknown"' 2>/dev/null || echo unknown)"
    echo "legacy: name=$LEGACY_CRATE version=$legacy_version (retired name, evidence only)"
fi
echo "release: tag=$tag prerelease=$prerelease"

if echo "$desc" | grep -qi "preview"; then
    echo "VERDICT=NOT_GA"
    exit 0
fi
if [[ "$prerelease" == "true" ]]; then
    echo "VERDICT=NOT_GA"
    exit 0
fi
if echo "$name $body" | grep -qiE "generally available|(^|[^a-z])ga([^a-z]|$)"; then
    echo "VERDICT=GA"
    exit 0
fi
echo "VERDICT=NOT_GA"

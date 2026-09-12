#!/usr/bin/env bash
# check-deps.sh — dependency-direction + cargo-deny policy check.
#
# Sensor: scripts/check-deps.sh
# Rules:
#   1. The conventional schema crate (`crates/types/Cargo.toml`, checked only
#      when present) must not depend on storage, adapter, or CLI layers.
#      Layers are recognized by package-name suffix (db, storage,
#      adapters/adapter, cli), which generalizes the do-harness types-manifest
#      rule without hardcoding project crate names. Override the suffix ERE
#      with DO_HARNESS_FORBIDDEN_LAYERS.
#   2. `cargo deny check` runs when deny.toml is configured and covers the
#      transitive closure; without deny.toml the policy is unconfigured, so
#      the sensor WARN-skips rather than failing a greenfield scaffold.
# Missing cargo-deny fails closed when CI=true or DO_HARNESS_REQUIRE_TOOLS=1.
set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
FAIL=0

TYPES_MANIFEST="$ROOT/crates/types/Cargo.toml"
FORBIDDEN_LAYERS="${DO_HARNESS_FORBIDDEN_LAYERS:-(^|[-_])(db|storage|adapters?|cli)$}"
if [[ -f "$TYPES_MANIFEST" ]]; then
    while IFS= read -r dep; do
        [[ -z "$dep" ]] && continue
        if [[ "$dep" =~ $FORBIDDEN_LAYERS ]]; then
            echo "FAIL: schema crate must not depend on a storage/adapter layer: $dep"
            FAIL=1
        fi
    done < <(
        awk '/^\[/ { in_deps = ($0 ~ /dependencies\]$/) } in_deps { print }' "$TYPES_MANIFEST" \
            | grep -oE '^[[:space:]]*"?[A-Za-z0-9_.-]+' \
            | tr -d ' "'
    )
fi

if [[ -f "$ROOT/deny.toml" ]]; then
    if ! command -v cargo-deny >/dev/null 2>&1; then
        if require_tools; then
            echo "FAIL: cargo-deny is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
            FAIL=1
        else
            echo "WARN: cargo-deny not installed; skipping deny check."
        fi
    else
        cargo deny check || FAIL=1
    fi
else
    echo "WARN: deny.toml not found; skipping cargo-deny policy check."
fi

if (( FAIL )); then
    exit 1
fi

echo "check-deps OK."

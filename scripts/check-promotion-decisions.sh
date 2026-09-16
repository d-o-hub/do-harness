#!/usr/bin/env bash
# check-promotion-decisions.sh — hermetic gate for recorded promotion decisions.
#
# Sensor: scripts/check-promotion-decisions.sh
#
# This checks the one thing a promotion decision can silently get wrong: the
# epic says "keep off by default" while `Cargo.toml` ships the feature on (or
# the reverse). Both artifacts are in the repository, so the check is hermetic —
# no network, no registry, no clock.
#
# It deliberately does NOT re-derive GA. That verdict needs crates.io and the
# upstream release feed, and `scripts/check-agt-ga.sh` is explicitly documented
# as "not a verify sensor": it prints VERDICT=GA / NOT_GA / UNKNOWN and exits 0
# on every verdict. A sensor wrapping it would therefore always pass, which is
# worse than no gate at all, and calling it over the network would make routine
# verification fail on a fetch error rather than on a real defect. The dated
# live verdict stays where it belongs: recorded epic evidence.
#
# What this sensor enforces instead — coherence only, not GA:
#   1. Every `*-promotion` decision row resolves to a `hold` or `promote`
#      verdict.
#   2. A `hold` verdict means the feature is off by default in Cargo.
#   3. A `promote` verdict means the feature is on by default in Cargo.
#   4. The AGT decision is mirrored in plans/invariants.json, so the recorded
#      policy has one machine-readable home rather than two.
#
# It does NOT verify that the recorded verdict is *correct*. A hand-written
# `hold` with no GA evaluation satisfies it, which is intentional: the
# authoritative GA verdict is a dated, network-dependent observation recorded
# in the epic, and this gate's job is to catch the two artifacts drifting apart
# — not to re-derive the verdict. The gate a promotion decision must pass for
# substance is the epic's own recorded evidence, reviewed by a human.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/crates/guardian-proxy/Cargo.toml"
FAIL=0

fail() {
    echo "FAIL: $*" >&2
    FAIL=1
}

# Default feature list from a crate manifest, then prove a named feature is
# or is not in it. `default = ["a", "b"]` on one line or wrapped over several.
default_features() {
    local manifest="$1"
    sed -n '/^\[features\]/,/^\[/p' "$manifest" |
        sed -n 's/^default[[:space:]]*=[[:space:]]*\[\(.*\)\].*/\1/p' |
        tr -d ' \n"'
}

check_decision() {
    local name="$1" feature="$2" epic="$3"
    local row
    row="$(grep -F "| \`$name\` |" "$epic" 2>/dev/null | head -1 || true)"
    if [[ -z "$row" ]]; then
        fail "$name: no decision row found in ${epic#"$ROOT"/}"
        return
    fi

    # The verdict may live in the table row or in the epic's status line: a
    # decision recorded before the table convention settled says
    # "(keep off-by-default)" up top and leaves the row descriptive.
    #
    # Read both rather than editing an epic to add a marker. A decision record
    # is a historical artifact owned by its own epic; reshaping another epic's
    # prose so this parser can read it is scope creep with a real cost, and it
    # is unnecessary — `chore-mcp-promotion` resolves correctly from its status
    # line alone.
    local verdict="" haystack
    haystack="$(printf '%s\n%s\n' "$row" "$(sed -n '1,5p' "$epic")")"
    if grep -qiE '\*\*(hold|promote)\*\*' <<<"$haystack"; then
        verdict="$(grep -oiE '\*\*(hold|promote)\*\*' <<<"$haystack" | head -1 | tr -d '*')"
        verdict="$(tr '[:upper:]' '[:lower:]' <<<"$verdict")"
    elif grep -qi 'keep off-by-default\|off-by-default' <<<"$haystack"; then
        verdict="hold"
    elif grep -qiE 'decided [0-9-]+ \(promote' <<<"$haystack"; then
        verdict="promote"
    fi
    if [[ -z "$verdict" ]]; then
        fail "$name: no 'hold' or 'promote' verdict found in the row or epic status"
        return
    fi

    local defaults on_default="no"
    defaults="$(default_features "$MANIFEST")"
    [[ ",$defaults," == *",$feature,"* ]] && on_default="yes"

    if [[ "$verdict" == "hold" && "$on_default" == "yes" ]]; then
        fail "$name: recorded a hold but '$feature' is in guardian-proxy default features"
    elif [[ "$verdict" == "promote" && "$on_default" == "no" ]]; then
        fail "$name: recorded a promote but '$feature' is absent from guardian-proxy defaults"
    else
        echo "OK: $name -> $verdict; '$feature' default=$on_default (coherent)"
    fi
}

check_invariant_mirror() {
    local label="$1"
    if ! grep -q "chore-agt-promotion" "$ROOT/plans/invariants.json" 2>/dev/null; then
        fail "$label: the AGT decision is not mirrored in plans/invariants.json"
    else
        echo "OK: $label decision is mirrored in plans/invariants.json"
    fi
}

check_decision "chore-agt-promotion" "agt-governance" "$ROOT/plans/agt-governance-epic.md"
check_decision "chore-mcp-promotion" "mcp-surface" "$ROOT/plans/mcp-conformance-epic.md"
check_invariant_mirror "AGT"

if ((FAIL)); then
    echo "check-promotion-decisions: FAILED (a recorded decision disagrees with shipped defaults)." >&2
    exit 1
fi

echo "check-promotion-decisions OK."

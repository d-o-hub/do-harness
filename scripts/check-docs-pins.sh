#!/usr/bin/env bash
# Verifies that every documented install pin matches the workspace version.
#
# The README and the adoption guide tell adopters to install a release by tag.
# Nothing regenerated those tags, so the snippets silently pinned v0.2.1 for
# five minor releases while `verify --status --since` and the empty-change
# refusal landed. This sensor is the feedback half; `release-manager.sh
# prepare` rewrites the pins so a release does not have to remember.
#
# `docs/releasing.md` is deliberately absent: its `v0.2.1` examples are a
# historical verification transcript, not an install instruction.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Maintained install snippets, relative to the workspace root.
files=(README.md docs/adoption.md)

workspace_version() {
    sed -n '/^\[workspace.package\]/,/^\[/ s/^version *= *"\(.*\)"/\1/p' "$1/Cargo.toml"
}

# Checks one workspace root; returns 1 when any documented pin is stale.
check() {
    local root="$1" version findings=0 file line pin
    version="$(workspace_version "$root")"
    if [ -z "$version" ]; then
        printf 'FAIL: cannot read [workspace.package].version from %s/Cargo.toml\n' "$root" >&2
        return 1
    fi
    for file in "${files[@]}"; do
        [ -f "$root/$file" ] || continue
        while IFS= read -r line; do
            pin="$(printf '%s\n' "$line" | grep -o 'v[0-9][0-9]*\(\.[0-9][0-9]*\)*' | head -n1 || true)"
            [ -n "$pin" ] || continue
            if [ "$pin" != "v$version" ]; then
                printf 'FAIL: %s:%s pins %s but the workspace is %s\n' \
                    "$file" "${line%%:*}" "$pin" "$version"
                findings=$((findings + 1))
            fi
        done < <(grep -n -- '--version v[0-9]' "$root/$file" || true)
    done
    printf 'FINDINGS: %s\n' "$findings"
    [ "$findings" -eq 0 ]
}

# Proves both directions: a matching pin passes, a stale pin fails.
SELF_TEST_DIR=""
self_test() {
    local tmp
    tmp="$(mktemp -d)"
    SELF_TEST_DIR="$tmp"
    trap 'rm -rf "${SELF_TEST_DIR:-}"' EXIT
    mkdir -p "$tmp/docs"
    printf '[workspace.package]\nversion = "9.9.9"\n' >"$tmp/Cargo.toml"
    printf 'curl ... | sh -s -- --version v9.9.9\n' >"$tmp/README.md"
    if ! check "$tmp" >/dev/null; then
        printf 'self-test: a matching pin was rejected\n' >&2
        exit 1
    fi
    printf 'curl ... | sh -s -- --version v0.0.1\n' >"$tmp/README.md"
    if check "$tmp" >/dev/null; then
        printf 'self-test: a stale pin was accepted\n' >&2
        exit 1
    fi
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    printf 'check-docs-pins: self-test passed\n'
    exit 0
fi
# Every ordinary run proves both directions first: the sensor must not be able
# to pass because it silently stopped matching anything.
self_test
check "$root"

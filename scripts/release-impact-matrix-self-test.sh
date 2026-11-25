#!/usr/bin/env bash
# shellcheck disable=SC2034 # Shared settings are read by the sourcing script.
# Hermetic fixtures sourced by release-impact-matrix.sh --self-test.

# The self-test's sandbox; a global because the EXIT trap runs after the
# function has returned and a `local` would already be out of scope.
WORK=""
self_test() {
    local repo failures=0
    WORK="$(mktemp -d)"
    repo="$WORK/repo"
    trap 'rm -rf "$WORK"' EXIT

    expect_die() {
        local want="$1" description="$2" out
        shift 2
        if out="$("$@" 2>&1)"; then
            printf 'FAIL %s: exited 0\n' "$description" >&2
            failures=$((failures + 1))
        elif [[ "$out" == *"$want"* ]]; then
            printf 'ok   %s\n' "$description"
        else
            printf 'FAIL %s: failed without "%s": %s\n' "$description" "$want" "$out" >&2
            failures=$((failures + 1))
        fi
    }
    expect_matrix() { # expect_matrix <tag> <group> <expected-impact> <description>
        local tag="$1" group="$2" want="$3" description="$4" out row
        out="$(matrix "$tag")" || {
            printf 'FAIL %s: matrix exited %s\n' "$description" "$?" >&2
            failures=$((failures + 1))
            return
        }
        row="$(printf '%s' "$out" | grep -o "\"group\":\"$group\"[^}]*}" || true)"
        if [[ "$row" == *"\"impact\":\"$want\""* ]]; then
            printf 'ok   %s (%s=%s)\n' "$description" "$group" "$want"
        else
            printf 'FAIL %s: %s row expected %s, got %s\n' "$description" "$group" "$want" "$row" >&2
            failures=$((failures + 1))
        fi
        for g in "${GROUP_IDS[@]}"; do
            printf '%s' "$out" | grep -q "\"group\":\"$g\"" || {
                printf 'FAIL %s: the matrix is missing the %s row\n' "$description" "$g" >&2
                failures=$((failures + 1))
            }
        done
    }

    # A fixture repository with one file per consumer group. Commit bodies are
    # written to real files so each commit carries changed paths.
    mkdir -p "$repo/crates/do-harness/src/cli" \
        "$repo/crates/do-harness/templates/scripts" \
        "$repo/integrations/npm/platforms" \
        "$repo/docs" \
        "$repo/.agents/skills/harness"
    : >"$repo/crates/do-harness/src/cli/status.rs"
    : >"$repo/crates/do-harness/templates/scripts/check-python-test.sh"
    : >"$repo/integrations/npm/package.json"
    : >"$repo/docs/releasing.md"
    : >"$repo/.agents/skills/harness/SKILL.md"
    : >"$repo/do-harness.toml"
    git -C "$repo" init -q -b main
    git -C "$repo" config user.email self-test@example.invalid
    git -C "$repo" config user.name self-test
    git -C "$repo" add -A
    git -C "$repo" commit -qm 'chore: seed'
    git -C "$repo" tag v0.4.5

    # Every derivation below reads the fixture repository, never this checkout.
    ROOT="$repo"
    # The assertions parse the JSON shape.
    FORMAT=json

    edit() { # edit <path> <content> <subject>
        printf '%s\n' "$2" >>"$repo/$1"
        git -C "$repo" add "$1"
        git -C "$repo" commit -q -m "$3"
    }

    # A breaking commit touching the CLI marks the CLI row breaking.
    edit crates/do-harness/src/cli/status.rs 'cmd' 'feat(cli)!: drop the legacy status flag'
    git -C "$repo" tag v0.4.6
    expect_matrix v0.4.6 cli breaking 'a breaking commit touching the CLI marks the CLI row breaking'
    expect_matrix v0.4.6 packs none 'the breaking CLI commit left the sensor-pack row untouched'
    expect_matrix v0.4.6 npm none 'the breaking CLI commit left the npm row untouched'

    # A sensor-pack-only change touches only the packs row.
    edit crates/do-harness/templates/scripts/check-python-test.sh 'ruff' 'fix(init): bump the python pack'
    git -C "$repo" tag v0.4.7
    expect_matrix v0.4.7 packs fix 'a sensor-pack-only change marks the packs row fix'
    expect_matrix v0.4.7 cli none 'a sensor-pack-only change left the CLI row untouched'
    expect_matrix v0.4.7 guides none 'a sensor-pack-only change left the guides row untouched'

    # A docs-only commit marks the guides row, not anything else.
    edit docs/releasing.md 'note' 'docs: explain the release gate'
    git -C "$repo" tag v0.4.8
    expect_matrix v0.4.8 guides updated 'a docs-only change marks the guides row updated'
    expect_matrix v0.4.8 contract none 'a docs-only change left the contract row untouched'

    mkdir -p "$repo/crates/do-harness/src/config"
    edit Cargo.lock dependency 'fix(deps): update a shipped dependency'
    edit crates/do-harness/src/config/mod.rs schema 'feat(config)!: change the adoption schema'
    git -C "$repo" tag v0.4.9
    expect_matrix v0.4.9 contract breaking 'a breaking parser edit reaches the contract row'
    expect_matrix v0.4.9 npm fix 'a dependency-only fix reaches the npm binary row'

    printf '%s\n' 'Consumer-Impact: contract | Run do-harness migrate --write' >"$WORK/actions"
    ACTIONS="$WORK/actions"
    FORMAT=plain
    if [[ "$(matrix v0.4.9)" != *'Required — Run do-harness migrate --write'* ]]; then
        printf 'FAIL the matrix dropped the required migration\n' >&2
        failures=$((failures + 1))
    fi
    ACTIONS=""
    FORMAT=json

    git -C "$repo" tag v0.4.10
    expect_die 'empty range' 'an empty range fails closed' matrix v0.4.10
    expect_die 'cannot resolve ref' 'an unresolvable ref fails closed' matrix no-such-ref

    if [ "$failures" -ne 0 ]; then
        printf 'release-impact-matrix: %s self-test failures\n' "$failures" >&2
        exit 1
    fi
    printf 'release-impact-matrix: self-test passed\n'
}

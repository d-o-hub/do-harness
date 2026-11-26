#!/usr/bin/env bash
# Hermetic can-fail suite for release-notes.sh.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck disable=SC1090,SC1091 # path resolved at runtime
source "$SCRIPT_DIR/release-notes.sh"
self_test() {
    local generated="$WORK/generated" composed="$WORK/composed.md" dump="$WORK/dump.md"
    local expected reason

    # The Consumer Impact Matrix fixture: what `release-impact-matrix.sh matrix`
    # emits for a release. The compose/check contract is about placement and
    # shape, so a fabricated file proves it without needing git history.
    cat >"$WORK/impact.md" <<'EOF'
## Consumer Impact

| Consumer group | Impact | Action required |
|---|---|---|
| CLI binaries | Breaking | Required — breaking changes to this artifact are detailed above. |
| npm packages | None | None. |
EOF
    cat >"$generated" <<'EOF'
<!-- Release notes generated using configuration in .github/release.yml at main -->

## What's Changed
### Other changes
* feat(cli): add a status command by @d-o-hub in https://github.com/d-o-hub/do-harness/pull/1
* fix(db): close the write lock by @d-o-hub in https://github.com/d-o-hub/do-harness/pull/2
* docs: explain the release gate by @d-o-hub in https://github.com/d-o-hub/do-harness/pull/3
* refactor: split the notes composer by @d-o-hub in https://github.com/d-o-hub/do-harness/pull/4
* chore(deps): bump sha2 by @d-o-hub in https://github.com/d-o-hub/do-harness/pull/5
* feat(api)!: drop the legacy sensor name by @d-o-hub in https://github.com/d-o-hub/do-harness/pull/6
* Merge branch 'main' into topic by @d-o-hub in https://github.com/d-o-hub/do-harness/pull/7

## New Contributors
* @newcomer made their first contribution in https://github.com/d-o-hub/do-harness/pull/8

**Full Changelog**: https://github.com/d-o-hub/do-harness/compare/v0.1.2...v0.1.3
EOF

    compose "v0.1.3" "$generated" "$composed" "" "$WORK/impact.md"
    check "v0.1.3" "$composed" >/dev/null ||
        fail "self-test: notes composed from the template failed the check"

    # The matrix sits between the curated header and the change list.
    if ! grep -qxF '## Consumer Impact' "$composed"; then
        fail "self-test: composed notes have no Consumer Impact section"
    fi
    impact_line="$(grep -n '^## Consumer Impact$' "$composed" | cut -d: -f1)"
    changes_line="$(grep -n '^## Changes$' "$composed" | cut -d: -f1)"
    if [[ -z "$impact_line" ]] || [[ -z "$changes_line" ]] || (( impact_line > changes_line )); then
        fail "self-test: the Consumer Impact section is not before the change list"
    fi

    for expected in \
        "### Breaking changes" "### Features" "### Fixes" "### Documentation" \
        "### Refactors" "### Maintenance" "### Other changes" "### New Contributors"; do
        grep -qxF "$expected" "$composed" ||
            fail "self-test: composed notes are missing '$expected'"
    done
    # The artifacts table must surface every independently-versioned artifact,
    # starting with the CLI release itself.
    grep -qxF "## Artifacts" "$composed" ||
        fail "self-test: composed notes are missing the Artifacts section"
    grep -qF "| CLI (binary, release tag) | v0.1.3 |" "$composed" ||
        fail "self-test: the artifacts table does not name the CLI release"
    grep -qF "| Adoption contract (do-harness.toml) |" "$composed" ||
        fail "self-test: the artifacts table does not name the adoption contract"
    grep -qF "| Sensor pack: rust |" "$composed" ||
        fail "self-test: the artifacts table does not name the sensor packs"
    grep -qF "| Guide: AGENTS.md (root) |" "$composed" ||
        fail "self-test: the artifacts table does not name the guides"
    # The artifacts section must sit before the change list, not inside it.
    local changes_at artifacts_at
    changes_at="$(grep -n '^## Changes$' "$composed" | head -n 1 | cut -d: -f1)"
    artifacts_at="$(grep -n '^## Artifacts$' "$composed" | head -n 1 | cut -d: -f1)"
    if (( artifacts_at >= changes_at )); then
        fail "self-test: the artifacts section follows the change list"
    fi
    grep -q '^\* feat(api)!: drop the legacy sensor name by @d-o-hub' "$composed" ||
        fail "self-test: a breaking bullet lost its title, author, or link"
    if awk '/^### Features$/,/^$/' "$composed" | grep -q 'api)!'; then
        fail "self-test: a breaking change was also listed under its type section"
    fi
    # A contributor line is not a change: it belongs under its own section and
    # nowhere else, and it must still survive the regrouping.
    if ! awk '/^### New Contributors$/,/^$/' "$composed" | grep -q 'made their first contribution'; then
        fail "self-test: a contributor line did not reach the New Contributors section"
    fi
    if awk '/^### Other changes$/,/^$/' "$composed" | grep -q 'made their first contribution'; then
        fail "self-test: a contributor line was filed as a change"
    fi
    if [[ "$(count_bullets "$generated")" != "$(count_bullets "$composed")" ]]; then
        fail "self-test: the composed body lost pull requests"
    fi

    # Negative control: the bare generator output is exactly the shape a release
    # must not ship, so the check has to reject it and name what is missing.
    cp "$generated" "$dump"
    if check "v0.1.3" "$dump" >/dev/null 2>&1; then
        fail "self-test: a bare generated dump passed the check"
    fi
    reason="$(check "v0.1.3" "$dump" 2>&1 || true)"
    case "$reason" in
        *"## Install"*) ;;
        *) fail "self-test: the rejection did not name the missing sections" ;;
    esac

    # Negative control: a placeholder that survived substitution.
    sed 's/v0\.1\.3/{{TAG}}/' "$composed" >"$WORK/placeholder.md"
    if check "v0.1.3" "$WORK/placeholder.md" >/dev/null 2>&1; then
        fail "self-test: an unsubstituted placeholder passed the check"
    fi

    # Negative control: a required section that exists but says nothing. The
    # control empties a required section on purpose — emptying an optional one
    # proves nothing about the check.
    awk '
        $0 == "## Breaking changes" { print; skip = 1; next }
        skip && /^## / { skip = 0 }
        skip { next }
        { print }
    ' "$composed" >"$WORK/empty-section.md"
    if check "v0.1.3" "$WORK/empty-section.md" >/dev/null 2>&1; then
        fail "self-test: an emptied required section passed the check"
    fi
    reason="$(check "v0.1.3" "$WORK/empty-section.md" 2>&1 || true)"
    case "$reason" in
        *"'## Breaking changes' is empty"*) ;;
        *) fail "self-test: the rejection did not name the empty section" ;;
    esac
    # Negative control: a body composed without the impact matrix is missing a
    # required section, and the rejection names it.
    compose "v0.1.3" "$generated" "$WORK/no-impact.md"
    if check "v0.1.3" "$WORK/no-impact.md" >/dev/null 2>&1; then
        fail "self-test: a body without the Consumer Impact matrix passed the check"
    fi
    reason="$(check "v0.1.3" "$WORK/no-impact.md" 2>&1 || true)"
    case "$reason" in
        *"## Consumer Impact"*) ;;
        *) fail "self-test: the rejection did not name the missing matrix section" ;;
    esac

    # Negative control: a matrix heading with no data rows is not a matrix.
    cat >"$WORK/empty-impact.md" <<'EOF'
## Consumer Impact

| Consumer group | Impact | Action required |
|---|---|---|
EOF
    compose "v0.1.3" "$generated" "$WORK/empty-matrix.md" "" "$WORK/empty-impact.md"
    if check "v0.1.3" "$WORK/empty-matrix.md" >/dev/null 2>&1; then
        fail "self-test: a header-only impact matrix passed the check"
    fi
    reason="$(check "v0.1.3" "$WORK/empty-matrix.md" 2>&1 || true)"
    case "$reason" in
        *"has no data rows"*) ;;
        *) fail "self-test: the rejection did not name the empty matrix" ;;
    esac

    # Negative control: a body without the Artifacts section is rejected and
    # the rejection names it — the table is part of the release contract.
    awk '
        $0 == "## Artifacts" { skip = 1; next }
        skip && /^## / { skip = 0 }
        skip { next }
        { print }
    ' "$composed" >"$WORK/no-artifacts.md"
    if check "v0.1.3" "$WORK/no-artifacts.md" >/dev/null 2>&1; then
        fail "self-test: a body without the Artifacts section passed the check"
    fi
    reason="$(check "v0.1.3" "$WORK/no-artifacts.md" 2>&1 || true)"
    case "$reason" in
        *"have no '## Artifacts' section"*) ;;
        *) fail "self-test: the rejection did not name the empty Artifacts section" ;;
    esac
    # `seed` controls: the stub is byte-identical to the template (an untouched
    # stub must publish the same body), a second seed cannot destroy an edit made
    # while the release pull request is open, a malformed tag is rejected without
    # writing anything, and an unedited stub already passes the check the release
    # job runs.
    local stub="$WORK/stub.md" seeded="$WORK/seeded.md"
    seed "v0.1.3" "$stub" >/dev/null
    cmp -s "$stub" "$TEMPLATE" ||
        fail "self-test: the seeded stub is not byte-identical to the template"
    seed "v0.1.3" "$seeded" >/dev/null
    printf 'edited during review\n' >>"$seeded"
    seed "v0.1.3" "$seeded" >/dev/null
    [[ "$(tail -n 1 "$seeded")" == "edited during review" ]] ||
        fail "self-test: a second seed overwrote an edited header"
    # A subshell on purpose: `seed` rejects a bad tag by exiting, and a function
    # call in an `if` condition runs in this shell, so the control would end the
    # self-test instead of reporting a failure.
    if (seed "0.1.3" "$WORK/no-v-prefix.md" >/dev/null 2>&1); then
        fail "self-test: seed accepted a tag without the v prefix"
    fi
    if [[ -e "$WORK/no-v-prefix.md" ]]; then
        fail "self-test: a rejected tag still wrote a file"
    fi
    compose "v0.1.3" "$generated" "$WORK/stubbed.md" "$stub" "$WORK/impact.md"
    check "v0.1.3" "$WORK/stubbed.md" >/dev/null ||
        fail "self-test: an unedited seeded stub failed the check"

    echo "release-notes.sh self-test OK (regroups by type, files contributors apart, keeps every bullet, places the impact matrix before the changes, seeds without overwriting, rejects a bare dump, an empty section, and a missing or empty matrix)"
}
self_test

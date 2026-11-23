#!/usr/bin/env bash
# check-artifact-versions.sh — multi-artifact version discipline.
#
# Sensor: scripts/check-artifact-versions.sh
#
# The repository versions the CLI (git tag), the adoption contract
# (`contract_version` in do-harness.toml), each sensor pack (`version` in
# crates/do-harness/templates/packs/<lang>/manifest.toml), and the guides
# (frontmatter `version` in AGENTS.md and USE_RESTRICTIONS.md) independently.
# This guard fails when an artifact's shipped files changed since the merge
# base without a strict semver increase in that artifact's own version field —
# so a pack patch no longer forces a CLI release, and a silent contract or
# guide change is a red PR instead of a release surprise.
#
#   bash scripts/check-artifact-versions.sh             # compare HEAD vs base
#   bash scripts/check-artifact-versions.sh --self-test  # hermetic can-fail proof
#
# Version fields are plain MAJOR.MINOR.PATCH (the same shape the release
# pipeline accepts). An absent version at the base reads as 0.0.0, so adding
# versioning to a previously-unversioned artifact is itself a bump.
#
# Base resolution is fail-closed: $CHECK_ARTIFACT_VERSIONS_BASE if set, else
# origin/main, else main, else HEAD~1. When no base resolves, or any git read
# fails, the check exits 2 — it never passes vacuously on an unreadable tree.
set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

ROOT="${CHECK_ARTIFACT_VERSIONS_ROOT:-}"
BASE_REF="${CHECK_ARTIFACT_VERSIONS_BASE:-}"
SELF_TEST=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --self-test)
            SELF_TEST=1
            shift
            ;;
        --root)
            ROOT="$2"
            shift 2
            ;;
        *)
            echo "usage: check-artifact-versions.sh [--self-test] [--root DIR]" >&2
            exit 2
            ;;
    esac
done
if [[ -z "$ROOT" ]]; then
    ROOT="$(dh_repo_root)" || exit 2
fi

FAIL=0

fail_closed() {
    echo "check-artifact-versions: $*" >&2
    exit 2
}

# resolve_base — prints the merge base of HEAD and the resolved base ref.
# Exits 2 when the git state is unreadable (no base resolves, or merge-base
# fails). HEAD itself must resolve, or there is nothing to compare.
resolve_base() {
    local candidate mb
    git -C "$ROOT" rev-parse -q --verify 'HEAD^{commit}' >/dev/null 2>&1 ||
        fail_closed "HEAD does not resolve; not a git checkout?"
    for candidate in "${BASE_REF:-}" origin/main main HEAD~1; do
        [[ -n "$candidate" ]] || continue
        if git -C "$ROOT" rev-parse -q --verify "$candidate^{commit}" >/dev/null 2>&1; then
            if mb="$(git -C "$ROOT" merge-base HEAD "$candidate" 2>&1)"; then
                printf '%s\n' "$mb"
                return 0
            fi
            fail_closed "git merge-base failed against '$candidate': $mb"
        fi
    done
    fail_closed "no base ref resolves (set CHECK_ARTIFACT_VERSIONS_BASE)"
}

# extract_toml_from_text <key> <text> — value of `<key> = "…"` in TOML text.
extract_toml_from_text() {
    local key="$1" text="$2"
    printf '%s\n' "$text" |
        sed -n "s/^${key} *= *\"\([^\"]*\)\"/\1/p" |
        tail -n 1
}

# extract_frontmatter_from_text <text> — `version:` value in YAML frontmatter.
extract_frontmatter_from_text() {
    printf '%s\n' "$1" | awk '
        NR == 1 && $0 == "---" { fm = 1; next }
        fm && $0 == "---" { exit }
        fm && /^version:/ {
            sub(/^version:[[:space:]]*/, "")
            gsub(/"/, "")
            if (length($0)) print
            exit
        }
    '
}

# version_from_file <file> <mode> [key] — version parsed from the working-tree
# file. Prints empty (exit 0) when the file or key is absent or not parseable;
# the caller decides what an empty value means for the artifact under test.
version_from_file() {
    local file="$1" mode="$2" key="${3:-}" text
    [[ -f "$file" ]] || return 0
    text="$(cat "$file")" || return 0
    case "$mode" in
        toml) extract_toml_from_text "$key" "$text" ;;
        frontmatter) extract_frontmatter_from_text "$text" ;;
    esac
}

# version_from_ref <ref> <file> <mode> [key] — version parsed from a git ref.
# Exit 1: file absent at the ref (the base case: unversioned artifact).
# Exit 2: the ref is unreadable (fail closed — never guess).
version_from_ref() {
    local ref="$1" file="$2" mode="$3" key="${4:-}" out
    if ! git -C "$ROOT" cat-file -e "$ref:$file" >/dev/null 2>&1; then
        return 1
    fi
    if ! out="$(git -C "$ROOT" show "$ref:$file" 2>&1)"; then
        echo "unreadable version file $file at $ref: $out" >&2
        return 2
    fi
    case "$mode" in
        toml) extract_toml_from_text "$key" "$out" ;;
        frontmatter) extract_frontmatter_from_text "$out" ;;
    esac
}

# ver_cmp <a> <b> — prints 1 when a > b, -1 when a < b, 0 when equal.
# Plain MAJOR.MINOR.PATCH only; anything else exits 2 (malformed).
ver_cmp() {
    local a="$1" b="$2" ia ib i
    IFS='.' read -r -a ia <<< "$a"
    IFS='.' read -r -a ib <<< "$b"
    if [[ ${#ia[@]} -ne 3 || ${#ib[@]} -ne 3 ]]; then
        return 2
    fi
    for i in 0 1 2; do
        if [[ ! "${ia[$i]}" =~ ^[0-9]+$ || ! "${ib[$i]}" =~ ^[0-9]+$ ]]; then
            return 2
        fi
        if ((10#${ia[$i]} > 10#${ib[$i]})); then
            echo 1
            return 0
        fi
        if ((10#${ia[$i]} < 10#${ib[$i]})); then
            echo -1
            return 0
        fi
    done
    echo 0
}

# check_artifact <name> <version-file> <mode> [key] [owner paths…]
# The owner paths are the artifact's shipped files; when any of them changed
# since the merge base, the version field must have strictly increased.
check_artifact() {
    local name="$1" vfile="$2" mode="$3" key="${4:-}"
    shift 4
    local -a owners=("$@")
    local head_version base_version changed cmp

    head_version="$(version_from_file "$ROOT/$vfile" "$mode" "$key")"
    if [[ -z "$head_version" ]]; then
        echo "FAIL: $name has no readable version at HEAD ($vfile)" >&2
        FAIL=$((FAIL + 1))
        return 1
    fi

    if ! changed="$(git -C "$ROOT" diff --name-only "$BASE_COMMIT" HEAD -- "${owners[@]}" 2>&1)"; then
        echo "FAIL: git diff failed while checking $name" >&2
        echo "$changed" >&2
        FAIL=$((FAIL + 1))
        return 1
    fi
    # Uncommitted edits are part of the gate too: a working-tree change to an
    # owner path without a version bump must fail the same way a committed one
    # does. `git status --porcelain -- <paths>` covers staged and unstaged
    # changes (including new files) for exactly the owner paths.
    local status_changes
    if ! status_changes="$(git -C "$ROOT" status --porcelain -- "${owners[@]}" 2>&1)"; then
        echo "FAIL: git status failed while checking $name" >&2
        echo "$status_changes" >&2
        FAIL=$((FAIL + 1))
        return 1
    fi
    changed="${changed}${status_changes:+$'\n'$status_changes}"
    if [[ -z "$changed" ]]; then
        echo "OK: $name unchanged (version $head_version)"
        return 0
    fi

    base_version="$(version_from_ref "$BASE_COMMIT" "$vfile" "$mode" "$key")"
    case $? in
        0) ;;
        1) base_version="" ;;
        2) FAIL=$((FAIL + 1)); return 1 ;;
    esac
    if [[ -z "$base_version" ]]; then
        base_version="0.0.0"
    fi

    if ! cmp="$(ver_cmp "$head_version" "$base_version")"; then
        echo "FAIL: $name version '${head_version}' is malformed (want MAJOR.MINOR.PATCH)" >&2
        FAIL=$((FAIL + 1))
        return 1
    fi
    if [[ "$cmp" != "1" ]]; then
        echo "FAIL: $name changed without a version bump ($base_version -> $head_version)" >&2
        echo "$changed" | sed 's/^/  changed: /' >&2
        FAIL=$((FAIL + 1))
        return 1
    fi
    echo "OK: $name bumped $base_version -> $head_version"
}

# run_checks — the artifact registry. Owner paths are the shipped surface of
# each artifact; version files are the machine-readable location of its version.
run_checks() {
    # Adoption contract: the parsing surface of do-harness.toml. The config
    # schema files are the contract's definition; the config tests are not.
    check_artifact "adoption contract" "do-harness.toml" toml contract_version \
        "do-harness.toml" \
        "crates/do-harness/templates/do-harness.toml.generic" \
        "crates/do-harness/templates/do-harness.toml.web" \
        "crates/do-harness/src/config/mod.rs" \
        "crates/do-harness/src/config/validate.rs" || true
    # Sensor packs: each pack's manifest is its version file; the owner paths
    # are the template files `init` ships for that pack.
    check_artifact "sensor pack: rust" \
        "crates/do-harness/templates/packs/rust/manifest.toml" toml version \
        "crates/do-harness/templates/packs/rust/manifest.toml" \
        "crates/do-harness/templates/crate/**" \
        "crates/do-harness/templates/scripts/check-loc.sh" \
        "crates/do-harness/templates/scripts/check-deps.sh" \
        "crates/do-harness/templates/scripts/check-audit.sh" \
        "crates/do-harness/templates/scripts/check-coverage.sh" \
        "crates/do-harness/templates/scripts/check-test.sh" \
        "crates/do-harness/templates/scripts/check-release-preflight.sh" || true
    check_artifact "sensor pack: generic" \
        "crates/do-harness/templates/packs/generic/manifest.toml" toml version \
        "crates/do-harness/templates/packs/generic/manifest.toml" \
        "crates/do-harness/templates/do-harness.toml.generic" || true
    check_artifact "sensor pack: web" \
        "crates/do-harness/templates/packs/web/manifest.toml" toml version \
        "crates/do-harness/templates/packs/web/manifest.toml" \
        "crates/do-harness/templates/do-harness.toml.web" \
        "crates/do-harness/templates/scripts/web-ui/**" \
        "crates/do-harness/templates/scripts/viewport-audit.mjs" \
        "crates/do-harness/templates/scripts/a11y-audit.mjs" \
        "crates/do-harness/templates/scripts/console-audit.mjs" \
        "crates/do-harness/templates/scripts/perf-audit.mjs" \
        "crates/do-harness/templates/scripts/visual-audit.mjs" \
        "crates/do-harness/templates/scripts/i18n-audit.mjs" || true
    check_artifact "sensor pack: node" \
        "crates/do-harness/templates/packs/node/manifest.toml" toml version \
        "crates/do-harness/templates/packs/node/manifest.toml" || true
    check_artifact "sensor pack: python" \
        "crates/do-harness/templates/packs/python/manifest.toml" toml version \
        "crates/do-harness/templates/packs/python/manifest.toml" \
        "crates/do-harness/templates/scripts/check-python-test.sh" || true
    check_artifact "sensor pack: go" \
        "crates/do-harness/templates/packs/go/manifest.toml" toml version \
        "crates/do-harness/templates/packs/go/manifest.toml" \
        "crates/do-harness/templates/scripts/check-go-vet.sh" \
        "crates/do-harness/templates/scripts/check-go-test.sh" \
        "crates/do-harness/templates/scripts/check-go-staticcheck.sh" || true
    # Guides: each guide's frontmatter is its version file.
    check_artifact "guide AGENTS.md (root)" "AGENTS.md" frontmatter "" \
        "AGENTS.md" || true
    check_artifact "guide AGENTS.md (template)" \
        "crates/do-harness/templates/AGENTS.md" frontmatter "" \
        "crates/do-harness/templates/AGENTS.md" || true
    check_artifact "guide USE_RESTRICTIONS.md" \
        "crates/do-harness/templates/USE_RESTRICTIONS.md" frontmatter "" \
        "crates/do-harness/templates/USE_RESTRICTIONS.md" || true
}

# run_self_test — hermetic cannot-fail proof: red/green both directions, a
# malformed version, a missing version file, an artifact introduced with a
# version, and fail-closed on an unreadable git state.
run_self_test() {
    local work failures=0
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' RETURN
    git -C "$work" init -q
    git -C "$work" config user.email test@example.com
    git -C "$work" config user.name test
    mkdir -p "$work/crates/do-harness/templates/packs/rust" \
        "$work/crates/do-harness/templates/packs/generic" \
        "$work/crates/do-harness/templates/packs/web" \
        "$work/crates/do-harness/templates/packs/node" \
        "$work/crates/do-harness/templates/packs/python" \
        "$work/crates/do-harness/templates/packs/go" \
        "$work/crates/do-harness/templates/scripts" \
        "$work/crates/do-harness/templates"
    cat >"$work/do-harness.toml" <<'EOF'
contract_version = "0.1.0"
EOF
    cat >"$work/crates/do-harness/templates/packs/rust/manifest.toml" <<'EOF'
name = "rust"
version = "0.1.0"
EOF
    cat >"$work/crates/do-harness/templates/packs/generic/manifest.toml" <<'EOF'
name = "generic"
version = "0.1.0"
EOF
    cat >"$work/crates/do-harness/templates/packs/web/manifest.toml" <<'EOF'
name = "web"
version = "0.1.0"
EOF
    cat >"$work/crates/do-harness/templates/packs/node/manifest.toml" <<'EOF'
name = "node"
version = "0.1.0"
EOF
    cat >"$work/crates/do-harness/templates/packs/python/manifest.toml" <<'EOF'
name = "python"
version = "0.1.0"
EOF
    cat >"$work/crates/do-harness/templates/packs/go/manifest.toml" <<'EOF'
name = "go"
version = "0.1.0"
EOF
    cat >"$work/crates/do-harness/templates/scripts/check-loc.sh" <<'EOF'
# rust pack helper
EOF
    cat >"$work/crates/do-harness/templates/AGENTS.md" <<'EOF'
---
version: 0.1.0
---
EOF
    cat >"$work/crates/do-harness/templates/USE_RESTRICTIONS.md" <<'EOF'
---
version: 0.1.0
---
EOF
    cat >"$work/AGENTS.md" <<'EOF'
---
version: 0.1.0
---
EOF
    git -C "$work" add -A
    git -C "$work" commit -qm base
    git -C "$work" branch base

    self_check() { # <expect-exit-code> <label>
        local expected="$1" label="$2" actual=0
        env CHECK_ARTIFACT_VERSIONS_ROOT="$work" \
            CHECK_ARTIFACT_VERSIONS_BASE=base \
            bash "${BASH_SOURCE[0]}" >/dev/null 2>&1 || actual=$?
        if [[ "$actual" != "$expected" ]]; then
            echo "self-test: $label exited $actual, expected $expected" >&2
            failures=$((failures + 1))
        fi
    }

    # 1. Red: an uncommitted owner change without a version bump fails too —
    # the gate judges the working tree, not only committed history.
    printf '# helper v2\n' >>"$work/crates/do-harness/templates/scripts/check-loc.sh"
    self_check 1 "an uncommitted owner change without a version bump was NOT rejected"
    git -C "$work" checkout -q -- crates/do-harness/templates/scripts/check-loc.sh

    # 2. Red: an owner file changed (committed) without a version bump.
    printf '# helper v2\n' >>"$work/crates/do-harness/templates/scripts/check-loc.sh"
    git -C "$work" add -A
    git -C "$work" commit -qm no-bump
    self_check 1 "a changed artifact without a version bump was NOT rejected"

    # 3. Green: the same change ships with a bumped manifest version.
    sed -i 's/version = "0.1.0"/version = "0.1.1"/' \
        "$work/crates/do-harness/templates/packs/rust/manifest.toml"
    git -C "$work" add -A
    git -C "$work" commit -qm bump
    self_check 0 "a bumped artifact was rejected"

    # 4. Red: a malformed version is rejected even without further changes.
    sed -i 's/version = "0.1.1"/version = "1.2"/' \
        "$work/crates/do-harness/templates/packs/rust/manifest.toml"
    git -C "$work" add -A
    git -C "$work" commit -qm bad-version
    self_check 1 "a malformed version was NOT rejected"

    # 5. Red: a version file that disappears from HEAD fails closed.
    git -C "$work" rm -q crates/do-harness/templates/packs/rust/manifest.toml
    git -C "$work" commit -qm removed-manifest
    self_check 1 "a missing version file at HEAD was NOT rejected"

    # 6. Green: restoring a version file with a higher version passes — an
    # absent version at the base reads as 0.0.0, so introducing (or
    # re-introducing) a versioned artifact is itself a bump.
    mkdir -p "$work/crates/do-harness/templates/packs/rust"
    cat >"$work/crates/do-harness/templates/packs/rust/manifest.toml" <<'EOF'
name = "rust"
version = "0.1.1"
EOF
    git -C "$work" add -A
    git -C "$work" commit -qm restore-manifest
    self_check 0 "a restored artifact with a bumped version was rejected"

    # 7. Fail closed: an unreadable git state exits 2, never a vacuous pass.
    local unsigned
    unsigned="$(mktemp -d)"
    if env CHECK_ARTIFACT_VERSIONS_ROOT="$unsigned" \
        bash "${BASH_SOURCE[0]}" >/dev/null 2>&1; then
        echo "self-test: a non-git root passed instead of failing closed" >&2
        failures=$((failures + 1))
    fi
    rm -rf "$unsigned"

    if ((failures > 0)); then
        echo "check-artifact-versions: self-test FAILED" >&2
        return 1
    fi
    echo "check-artifact-versions: self-test passed (red/green, malformed, missing, new artifact, fail-closed)"
}

if ((SELF_TEST)); then
    run_self_test
else
    BASE_COMMIT="$(resolve_base)"
    run_checks
    if ((FAIL > 0)); then
        echo "check-artifact-versions FAIL: $FAIL artifact(s) changed without a version bump" >&2
        exit 1
    fi
    echo "check-artifact-versions OK: every changed artifact raised its version"
fi

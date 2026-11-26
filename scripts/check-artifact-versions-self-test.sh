#!/usr/bin/env bash
# Hermetic can-fail suite for check-artifact-versions.sh.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_SCRIPT="$SCRIPT_DIR/check-artifact-versions.sh"
# shellcheck disable=SC1090,SC1091 # path resolved at runtime
source "$CHECK_SCRIPT"
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
            bash "$CHECK_SCRIPT" >/dev/null 2>&1 || actual=$?
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
    self_check 2 "a malformed version was NOT rejected"

    # 5. Red: a version file that disappears from HEAD fails closed.
    git -C "$work" rm -q crates/do-harness/templates/packs/rust/manifest.toml
    git -C "$work" commit -qm removed-manifest
    self_check 2 "a missing version file at HEAD was NOT rejected"

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
    # 7. Version metadata is validated even when no owner path changed. The
    # adoption contract's version file is not itself a contract definition.
    sed -i 's/contract_version = "0.1.0"/contract_version = "1.2"/' "$work/do-harness.toml"
    git -C "$work" add -A
    git -C "$work" commit -qm malformed-contract-version
    self_check 2 "malformed contract version without an owner change was NOT rejected"

    # 8. Fail closed: an unreadable git state exits 2, never a vacuous pass.
    local unsigned
    unsigned="$(mktemp -d)"
    if env CHECK_ARTIFACT_VERSIONS_ROOT="$unsigned" \
        bash "$CHECK_SCRIPT" >/dev/null 2>&1; then
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
run_self_test

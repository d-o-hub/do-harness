#!/usr/bin/env bash
# check-contract-first.sh — contract-first gate for the adoption-contract schema.
#
# Sensor: scripts/check-contract-first.sh
#
# The adoption contract is the `do-harness.toml` schema adopters depend on.
# Its definition lives in the parser (`crates/do-harness/src/config/`), the
# versioned migrations (`crates/do-harness/src/migrate.rs`), the doctor's
# schema-version checks (`crates/do-harness/src/doctor/checks.rs`), and the
# scaffolded contract instances
# (`crates/do-harness/templates/do-harness.toml.generic` / `.web`). A change
# to any of these must declare the new contract version before it lands;
# otherwise adopters cannot pin to a version or know when their config is
# affected.
#
# Checks:
#   1. Agreement (always): `CONFIG_SCHEMA_VERSION` in `config/mod.rs`, the
#      `schema-version` key in every template, and the repository's own
#      `do-harness.toml` must all agree. A drift fails even without a pending
#      change.
#   2. Bump-or-annotation (when a contract-definition file changed against the
#      merge base): the declared schema version must differ from the base's,
#      or the change must carry the explicit additive-only annotation
#      `Contract-schema: additive-only` as a commit trailer. Otherwise FAIL.
#
# The repository's own `do-harness.toml` is an instance of the contract, not
# a definition, so editing it alone never demands a bump; the agreement check
# still keeps its `schema-version` honest.
#
# `--self-test` proves both directions on hermetic fixtures, and every
# ordinary run proves them first, so a sensor that stopped matching anything
# can never report green. Bump rules and the additive-only convention are
# documented in `docs/versioning.md`.

set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

# Contract-definition paths, relative to the repository root. These are the
# files whose content defines the schema; each entry also works as a git
# pathspec for the diff and untracked-file selection below.
CONTRACT_PATHS=(
    "crates/do-harness/src/config"
    "crates/do-harness/src/migrate.rs"
    "crates/do-harness/src/doctor/checks.rs"
    "crates/do-harness/templates/do-harness.toml.generic"
    "crates/do-harness/templates/do-harness.toml.web"
)

# Prints `CONFIG_SCHEMA_VERSION` from config/mod.rs (or stdin when no file is
# given); empty when absent.
config_version_of() {
    local file="${1:-}"
    if [[ -n "$file" ]]; then
        sed -n 's/^pub const CONFIG_SCHEMA_VERSION: u32 = \([0-9][0-9]*\);/\1/p' "$file" | head -n 1
    else
        sed -n 's/^pub const CONFIG_SCHEMA_VERSION: u32 = \([0-9][0-9]*\);/\1/p' | head -n 1
    fi
}

# Prints the `schema-version` value of a do-harness.toml-style file (or stdin
# when no file is given); empty when absent.
toml_version_of() {
    local file="${1:-}"
    if [[ -n "$file" ]]; then
        sed -n 's/^schema-version *= *\([0-9][0-9]*\)/\1/p' "$file" | head -n 1
    else
        sed -n 's/^schema-version *= *\([0-9][0-9]*\)/\1/p' | head -n 1
    fi
}

# Agreement check: the constant must equal every template and the repository's
# own file. Fails closed on any drift.
agree() {
    local root="$1" current="" bad=0 version="" file=""
    current="$(config_version_of "$root/crates/do-harness/src/config/mod.rs")"
    if [[ -z "$current" ]]; then
        echo "FAIL: cannot read CONFIG_SCHEMA_VERSION from crates/do-harness/src/config/mod.rs" >&2
        return 1
    fi
    for file in "$root"/crates/do-harness/templates/do-harness.toml.*; do
        [[ -f "$file" ]] || continue
        version="$(toml_version_of "$file")"
        if [[ "$version" != "$current" ]]; then
            echo "FAIL: ${file#"$root"/} declares schema-version $version but CONFIG_SCHEMA_VERSION is $current" >&2
            bad=1
        fi
    done
    version="$(toml_version_of "$root/do-harness.toml")"
    if [[ -n "$version" && "$version" != "$current" ]]; then
        echo "FAIL: do-harness.toml declares schema-version $version but CONFIG_SCHEMA_VERSION is $current" >&2
        bad=1
    fi
    [[ "$bad" -eq 0 ]]
}

# Base ref for the committed range: origin/main when resolvable, else the PR
# base remote when present, else HEAD (working-tree-only mode).
base_ref() {
    local root="$1"
    if git -C "$root" rev-parse -q --verify refs/remotes/origin/main >/dev/null 2>&1; then
        printf '%s\n' refs/remotes/origin/main
    elif [[ -n "${GITHUB_BASE_REF:-}" ]] \
        && git -C "$root" rev-parse -q --verify "refs/remotes/origin/$GITHUB_BASE_REF" >/dev/null 2>&1; then
        printf '%s\n' "refs/remotes/origin/$GITHUB_BASE_REF"
    else
        printf '%s\n' HEAD
    fi
}

# Prints changed contract-definition paths: the committed range from the merge
# base plus tracked and untracked working-tree changes.
changed_contract_files() {
    local root="$1" base="$2" out="" mbase=""
    if [[ "$base" != HEAD ]]; then
        mbase="$(git -C "$root" merge-base "$base" HEAD 2>/dev/null || true)"
        if [[ -n "$mbase" ]]; then
            out+="$(git -C "$root" diff --name-only "$mbase"..HEAD -- "${CONTRACT_PATHS[@]}" 2>/dev/null || true)"
        fi
    fi
    out+="$(git -C "$root" diff --name-only HEAD -- "${CONTRACT_PATHS[@]}" 2>/dev/null || true)"
    out+="$(git -C "$root" ls-files --others --exclude-standard -- "${CONTRACT_PATHS[@]}" 2>/dev/null || true)"
    printf '%s' "$out"
}

# True when any commit in the range carries the additive-only trailer.
has_annotation() {
    local root="$1" mbase="$2"
    git -C "$root" log --format=%B "$mbase"..HEAD 2>/dev/null \
        | grep -Eq '^Contract-schema: +additive-only[[:space:]]*$'
}

# Bump-or-annotation check. A declared version change (constant vs the base's)
# passes; otherwise the explicit additive-only trailer is required.
bump_or_annotation() {
    local root="$1" base="$2" mbase="" current="" base_version=""
    if [[ "$base" != HEAD ]]; then
        mbase="$(git -C "$root" merge-base "$base" HEAD 2>/dev/null || true)"
    fi
    current="$(config_version_of "$root/crates/do-harness/src/config/mod.rs")"
    if [[ -n "$mbase" ]]; then
        base_version="$(git -C "$root" show "$mbase:crates/do-harness/src/config/mod.rs" 2>/dev/null | config_version_of || true)"
    fi
    # A base without the constant predates versioning: treat it as version 0.
    [[ -z "$base_version" ]] && base_version=0
    if [[ "$current" != "$base_version" ]]; then
        echo "contract schema-version bumped $base_version -> $current"
        return 0
    fi
    if [[ -n "$mbase" ]] && has_annotation "$root" "$mbase"; then
        echo "contract change annotated additive-only (trailer: Contract-schema: additive-only)"
        return 0
    fi
    echo "FAIL: contract-definition files changed but schema-version ($current) did not bump" >&2
    if [[ -z "$mbase" ]]; then
        echo "      no base range found; uncommitted-only changes cannot carry a commit-trailer annotation" >&2
    fi
    echo "      bump CONFIG_SCHEMA_VERSION and every template's schema-version together," >&2
    echo "      or mark the change additive-only with the documented commit trailer." >&2
    return 1
}

# Runs the full gate for one repository root.
check() {
    local root="$1" base="" changed=""
    agree "$root" || return 1
    if ! git -C "$root" rev-parse --git-dir >/dev/null 2>&1; then
        echo "check-contract-first OK: not a git checkout; agreement verified"
        return 0
    fi
    base="$(base_ref "$root")"
    changed="$(changed_contract_files "$root" "$base")"
    if [[ -z "$changed" ]]; then
        echo "check-contract-first OK: no contract-definition change"
        return 0
    fi
    echo "contract-definition files changed:"
    printf '%s\n' "$changed" | sed 's/^/  /'
    bump_or_annotation "$root" "$base"
}

# Proves both directions on hermetic fixtures: agreement pass/fail, bump
# pass/fail, and the additive-only annotation pass.
SELF_TEST_DIR=""
self_test() {
    local tmp src initial
    tmp="$(mktemp -d)"
    SELF_TEST_DIR="$tmp"
    trap 'rm -rf "${SELF_TEST_DIR:-}"' EXIT
    src="$tmp/src"
    mkdir -p "$src/crates/do-harness/src/config" "$src/crates/do-harness/templates"
    cat >"$src/crates/do-harness/src/config/mod.rs" <<'EOF'
pub const CONFIG_SCHEMA_VERSION: u32 = 1;
EOF
    printf 'schema-version = 1\n' >"$src/crates/do-harness/templates/do-harness.toml.generic"
    printf 'schema-version = 1\n' >"$src/crates/do-harness/templates/do-harness.toml.web"
    printf 'schema-version = 1\n' >"$src/do-harness.toml"
    git -C "$src" init -q
    git -C "$src" config user.email contract-first@example.com
    git -C "$src" config user.name contract-first
    git -C "$src" add -A
    git -C "$src" commit -qm initial
    initial="$(git -C "$src" rev-parse HEAD)"
    git -C "$src" update-ref refs/remotes/origin/main "$initial"

    # 1. A consistent contract passes.
    if ! check "$src" >/dev/null 2>&1; then
        echo "self-test: a consistent contract was rejected" >&2
        exit 1
    fi
    # 2. A drifting template fails.
    printf 'schema-version = 2\n' >"$src/crates/do-harness/templates/do-harness.toml.web"
    if check "$src" >/dev/null 2>&1; then
        echo "self-test: a drifting template was accepted" >&2
        exit 1
    fi
    printf 'schema-version = 1\n' >"$src/crates/do-harness/templates/do-harness.toml.web"

    # 3. A contract change without a bump and without an annotation fails.
    printf '# extend the schema\n' >>"$src/crates/do-harness/src/config/mod.rs"
    git -C "$src" add -A
    git -C "$src" commit -qm "feat(config): extend schema"
    if check "$src" >/dev/null 2>&1; then
        echo "self-test: an unbumped contract change was accepted" >&2
        exit 1
    fi
    # 4. The same change passes once the version bumps everywhere.
    sed -i 's/CONFIG_SCHEMA_VERSION: u32 = 1/CONFIG_SCHEMA_VERSION: u32 = 2/' "$src/crates/do-harness/src/config/mod.rs"
    printf 'schema-version = 2\n' >"$src/crates/do-harness/templates/do-harness.toml.generic"
    printf 'schema-version = 2\n' >"$src/crates/do-harness/templates/do-harness.toml.web"
    printf 'schema-version = 2\n' >"$src/do-harness.toml"
    git -C "$src" add -A
    git -C "$src" commit -qm "feat(config): bump contract version"
    if ! check "$src" >/dev/null 2>&1; then
        echo "self-test: a bumped contract change was rejected" >&2
        exit 1
    fi
    # 5. Without a bump, the explicit additive-only annotation passes.
    git -C "$src" reset -q --hard "$initial"
    printf '# document an existing field\n' >>"$src/crates/do-harness/src/config/mod.rs"
    git -C "$src" add -A
    git -C "$src" commit -qm "docs(config): clarify field" -m "Contract-schema: additive-only"
    if ! check "$src" >/dev/null 2>&1; then
        echo "self-test: an annotated additive-only change was rejected" >&2
        exit 1
    fi
}

if [[ "${1:-}" == "--self-test" ]]; then
    self_test
    printf 'check-contract-first: self-test passed\n'
    exit 0
fi
# Every ordinary run proves both directions first: the sensor must not be able
# to pass because it silently stopped matching anything.
self_test
check "$(dh_repo_root)"

#!/usr/bin/env bash
# release-manager.sh — one canonical release entry point.
#
#   status
#       Workspace version, newest release tag, commits since it, and the bump
#       those commits imply. Read-only.
#
#   next [--ref <rev>] [--format plain|json]
#       The machine-readable form of `status`. `bump` is one of:
#         major|minor|patch  the commits since the last tag warrant a release
#         pending            main carries a version with no tag: the missing
#                            step is a tag, not another bump
#         none               nothing releasable landed
#
#   prepare <version> [--no-lock] [--no-man]
#       Apply <version> to every managed manifest: `[workspace.package].version`
#       in Cargo.toml, Cargo.lock (workspace members are pins in it), the six npm
#       manifests (the meta version plus its five platform pins), and the
#       generated man page, whose header announces the semver.
#
#   --self-test
#       Hermetic controls in a throwaway repository: every commit class maps to
#       its bump, a `BREAKING CHANGE:` footer counts as breaking, a merge commit
#       is ignored, an untagged workspace version reports `pending`, a manifest
#       behind the newest tag fails closed, and `prepare` rewrites each managed
#       manifest. A check that cannot fail is decoration.
#
# Bump policy — the rules live here, the table and the reasoning for readers are
# in docs/releasing.md ("Cutting a release"):
#
#   breaking                 -> major at >=1.0, minor at 0.x, patch at 0.0.x
#   feat                     -> minor at >=1.0, patch at 0.x
#   fix, perf                -> patch
#   another conventional type (docs, chore, test, refactor, style, ci, build)
#                            -> nothing on its own
#   not a conventional subject -> patch
#
# Mirrored from `next_version`, the crate release-plz uses
# (<https://docs.rs/next_version>), rather than invented here. Pre-1.0 it takes a
# breaking change as the *minor* because "the bump from 0.x.y to 0.(x+1).0
# indicates a breaking change", and SemVer §4 is what permits the choice ("Major
# version zero (0.y.z) is for initial development. Anything MAY change at any
# time.") — a feature-driven minor would make breaking changes invisible. It
# takes a patch for a subject that is not a conventional commit, because a missed
# release is the failure this pipeline exists to prevent. `feat` -> minor and
# `fix` -> patch otherwise follow Conventional Commits, and §15's case rule is
# honoured: the type is matched case-insensitively while the `BREAKING CHANGE`
# footer token stays uppercase (§16's BREAKING-CHANGE synonym included). Merge
# commits are excluded — their subject is git's ("Merge branch 'x'"), so counting
# it would cut a release per merge.
#
# Tagging is deliberately not here: `main` requires pull requests, so the bump
# lands as a reviewed PR and the tag is created by
# `.github/workflows/auto-release.yml` once that PR is merged. See
# docs/releasing.md ("Cutting a release").

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
readonly REPO_ROOT
ROOT="$REPO_ROOT"
REF="HEAD"
FORMAT="plain"
DO_LOCK=true
DO_MAN=true

# Set by derive(): CURRENT, BASIS, COMMITS, BREAKING, FEATURE, FIX,
# NONCONVENTIONAL, BUMP, NEXT.
CURRENT="" BASIS="" COMMITS=0 BREAKING=false FEATURE=false FIX=false
NONCONVENTIONAL=false BUMP="" NEXT=""

# The self-test's sandbox. A global, because the EXIT trap runs after that
# function has returned and a `local` would already be out of scope.
WORK=""

die() {
    printf 'release-manager: %s\n' "$1" >&2
    exit 1
}

usage() {
    cat <<'EOF'
usage: release-manager.sh <command> [options]

  status                     version, newest tag, commits since it, derived bump
  next                       the same, as key=value or --format json
  prepare <version>          apply <version> to every managed manifest
  --self-test                hermetic controls for the derivation and the bump

options:
  --root <dir>               repository to operate on (default: this checkout)
  --ref <rev>                revision to derive from (default: HEAD)
  --format plain|json        output shape for `next` (default: plain)
  --no-lock                  skip the Cargo.lock refresh in `prepare`
  --no-man                   skip regenerating man/do-harness.1 in `prepare`
EOF
}

workspace_version() {
    sed -n '/^\[workspace.package\]/,/^\[/ s/^version *= *"\([0-9][0-9.]*\)"/\1/p' \
        "$ROOT/Cargo.toml"
}

# The highest release tag reachable from <ref>, by version order, so v0.10.0
# outranks v0.9.0. Prints nothing when the ref carries no release tag.
latest_tag() {
    git -C "$ROOT" tag --merged "$1" --list 'v[0-9]*' --sort=-v:refname 2>/dev/null \
        | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -1 || true
}

range_for() {
    if [ -n "$BASIS" ]; then
        printf '%s..%s' "$BASIS" "$1"
    else
        printf '%s' "$1"
    fi
}

# Sets BREAKING/FEATURE/FIX/NONCONVENTIONAL from the subjects and bodies in
# <range>. Matched with bash's own ERE rather than `grep -q` in a pipeline:
# `grep -q` exits at the first match, and under `pipefail` the resulting SIGPIPE
# on the producer would make a matching commit read as a non-match. The patterns
# live in variables because `[[:space:]]` ends the `[[ ]]` test when written
# inline.
readonly RE_BREAKING='^[a-zA-Z]+(\([^)]*\))?!:'      # feat(cli)!:
readonly RE_FEAT='^feat(\([^)]*\))?:'                # feat(cli):
readonly RE_FIX='^(fix|perf)(\([^)]*\))?:'           # fix(cli):, perf(core):
readonly RE_CONVENTIONAL='^[a-zA-Z]+(\([^)]*\))?!?:' # any conventional type
classify() {
    local range="$1" subject lowered line
    BREAKING=false
    FEATURE=false
    FIX=false
    NONCONVENTIONAL=false
    while IFS= read -r subject; do
        [ -n "$subject" ] || continue
        # §15: the type is case-insensitive (the footer token below is not).
        lowered="$(printf '%s' "$subject" | tr '[:upper:]' '[:lower:]')"
        if [[ "$lowered" =~ $RE_BREAKING ]]; then
            BREAKING=true
        elif [[ "$lowered" =~ $RE_FEAT ]]; then
            FEATURE=true
        elif [[ "$lowered" =~ $RE_FIX ]]; then
            FIX=true
        fi
        if [[ ! "$lowered" =~ $RE_CONVENTIONAL ]]; then
            NONCONVENTIONAL=true
        fi
    done < <(git -C "$ROOT" log --no-merges --format=%s "$range")
    while IFS= read -r line; do
        case "$line" in
            'BREAKING CHANGE:'* | 'BREAKING-CHANGE:'*) BREAKING=true ;;
        esac
    done < <(git -C "$ROOT" log --no-merges --format=%b "$range")
}

decide_bump() {
    local current="$1" major minor
    major="${current%%.*}"
    minor="${current#*.}"
    minor="${minor%%.*}"
    # `0.0.x` is the pre-API phase: next_version keeps it on patches so that the
    # step to 0.1.0 stays a decision rather than an automatic side effect.
    if [ "$major" -eq 0 ] && [ "$minor" -eq 0 ]; then
        printf 'patch'
        return 0
    fi
    if [ "$BREAKING" = true ]; then
        if [ "$major" -ge 1 ]; then
            printf 'major'
        else
            printf 'minor'
        fi
        return 0
    fi
    if [ "$FEATURE" = true ]; then
        if [ "$major" -ge 1 ]; then
            printf 'minor'
        else
            printf 'patch'
        fi
        return 0
    fi
    if [ "$FIX" = true ] || [ "$NONCONVENTIONAL" = true ]; then
        printf 'patch'
        return 0
    fi
    printf 'none'
}

next_version() {
    local current="$1" bump="$2" major minor patch
    IFS=. read -r major minor patch <<<"$current"
    case "$bump" in
        major) printf '%s.0.0' "$((major + 1))" ;;
        minor) printf '%s.%s.0' "$major" "$((minor + 1))" ;;
        patch) printf '%s.%s.%s' "$major" "$minor" "$((patch + 1))" ;;
        *) printf '%s' "$current" ;;
    esac
}

# Numeric version comparison, component by component. `sort -V` would be shorter
# but it is a GNU extension that BSD sort (macOS) rejects, and a command
# substitution that fails inside a `[ ... ]` test reads as a non-match: the
# guards below would quietly stop firing instead of erroring.
version_lt() { # version_lt <a> <b>: true when a < b
    local a="$1" b="$2" i ai bi
    local -a ap bp
    IFS=. read -r -a ap <<<"$a"
    IFS=. read -r -a bp <<<"$b"
    for i in 0 1 2; do
        ai="${ap[i]:-0}"
        bi="${bp[i]:-0}"
        if [ "$ai" -lt "$bi" ]; then
            return 0
        fi
        if [ "$ai" -gt "$bi" ]; then
            return 1
        fi
    done
    return 1
}

derive() {
    local ref="$1" range
    CURRENT="$(workspace_version)"
    [ -n "$CURRENT" ] || die "cannot read [workspace.package].version from $ROOT/Cargo.toml"
    BASIS="$(latest_tag "$ref")"
    range="$(range_for "$ref")"
    COMMITS="$(git -C "$ROOT" rev-list --count "$range" 2>/dev/null || printf '0')"
    classify "$range"
    # A manifest behind the newest release tag derives a bump from the wrong
    # baseline, so refuse the inconsistency instead of reporting a plausible
    # number: the version has to be applied before the tag, never after.
    local basis_version=""
    if [ -n "$BASIS" ]; then
        basis_version="${BASIS#v}"
    fi
    if [ -n "$basis_version" ] && [ "$basis_version" != "$CURRENT" ] &&
        version_lt "$CURRENT" "$basis_version"; then
        die "workspace version $CURRENT is behind the newest release tag $BASIS; apply the bump first"
    fi
    # An untagged workspace version is a release waiting for its tag. Deriving a
    # bump from the workspace version here would double-bump a version that a
    # merged release PR already applied.
    if git -C "$ROOT" rev-parse -q --verify "refs/tags/v$CURRENT" >/dev/null; then
        BUMP="$(decide_bump "$CURRENT")"
    else
        BUMP="pending"
    fi
    NEXT="$(next_version "$CURRENT" "$BUMP")"
}

print_plain() {
    printf 'current=%s\nbasis=%s\ncommits=%s\nbreaking=%s\nfeature=%s\nfix=%s\nnonconventional=%s\nbump=%s\nnext=%s\n' \
        "$CURRENT" "${BASIS:-none}" "$COMMITS" "$BREAKING" "$FEATURE" "$FIX" \
        "$NONCONVENTIONAL" "$BUMP" "$NEXT"
}

print_json() {
    printf '{"current":"%s","basis":"%s","commits":%s,"breaking":%s,"feature":%s,"fix":%s,"nonconventional":%s,"bump":"%s","next":"%s"}\n' \
        "$CURRENT" "${BASIS:-none}" "$COMMITS" "$BREAKING" "$FEATURE" "$FIX" \
        "$NONCONVENTIONAL" "$BUMP" "$NEXT"
}

# Rewrites one file through sed, then moves it into place: `sed -i` takes a
# different argument on GNU and BSD, and `mv` inside the same directory is atomic.
# `mktemp` creates its file 0600, so the original mode is captured and restored
# before the move — otherwise every write would quietly leave the manifest
# owner-only. `stat -c` is GNU, `stat -f` is BSD (macOS).
substitute() {
    local expression="$1" file="$2" tmp mode
    if mode="$(stat -c '%a' "$file" 2>/dev/null)"; then :; else
        mode="$(stat -f '%Lp' "$file")"
    fi
    tmp="$(mktemp "${file}.XXXXXX")"
    sed -e "$expression" "$file" >"$tmp"
    chmod "$mode" "$tmp"
    mv "$tmp" "$file"
}

# The mode of every manifest `prepare` rewrites, so that a write which changes
# one is caught by the self-test rather than by the next reader's `ls -l`.
manifest_modes() {
    local root="$1" file mode out=""
    for file in "$root/Cargo.toml" "$root/integrations/npm/package.json" \
        "$root"/integrations/npm/platforms/*/package.json; do
        [ -f "$file" ] || continue
        if mode="$(stat -c '%a' "$file" 2>/dev/null)"; then :; else
            mode="$(stat -f '%Lp' "$file")"
        fi
        out="$out$mode $file"$'\n'
    done
    printf '%s' "$out"
}

prepare_cargo() {
    local version="$1"
    substitute "/^\[workspace.package\]/,/^\[/ s/^version *= *\"[0-9][0-9.]*\"/version = \"$version\"/" \
        "$ROOT/Cargo.toml"
}

prepare_npm() {
    local version="$1" meta="$ROOT/integrations/npm/package.json" path
    [ -f "$meta" ] || die "missing $meta"
    substitute "s/^\([[:space:]]*\"version\": \)\"[0-9][0-9.]*\"/\1\"$version\"/" "$meta"
    # The platform pins carry the workspace version too:
    # check-package-contract.sh fails the release when they disagree.
    substitute "s/^\([[:space:]]*\"do-harness-[a-z0-9-]*\": \)\"[0-9][0-9.]*\"/\1\"$version\"/" "$meta"
    for path in "$ROOT"/integrations/npm/platforms/*/package.json; do
        [ -f "$path" ] || continue
        substitute "s/^\([[:space:]]*\"version\": \)\"[0-9][0-9.]*\"/\1\"$version\"/" "$path"
    done
}

prepare_lock() {
    # Workspace members are pins in Cargo.lock, and the release pipeline builds
    # with --locked, so a bump that leaves the lock stale fails the tag preflight.
    (cd "$ROOT" && cargo metadata --format-version 1 >/dev/null)
}

prepare_man() {
    # The page header announces the semver; man_sync.rs compares the committed
    # copy against the CLI and normalises only the build fingerprint.
    (cd "$ROOT" && cargo run --quiet -p do-harness --bin do-harness -- man man)
}

cmd_prepare() {
    local version="${1:-}"
    [ -n "$version" ] || die "prepare needs a version (try: release-manager.sh next)"
    case "$version" in
        [0-9]*.[0-9]*.[0-9]*) : ;;
        *) die "'$version' is not a MAJOR.MINOR.PATCH version" ;;
    esac
    local current
    current="$(workspace_version)"
    [ -n "$current" ] || die "cannot read [workspace.package].version from $ROOT/Cargo.toml"
    # Idempotent: re-running with the version already applied is a no-op rather
    # than an error, so a retried job does not fail on its own earlier work.
    if [ "$version" = "$current" ]; then
        printf 'workspace is already at %s\n' "$version"
    else
        [ "$version" = "$current" ] || version_lt "$current" "$version" ||
            die "'$version' does not supersede the current $current"
        prepare_cargo "$version"
        prepare_npm "$version"
        printf 'applied %s (was %s)\n' "$version" "$current"
    fi
    if [ "$DO_LOCK" = true ]; then
        prepare_lock
    fi
    if [ "$DO_MAN" = true ]; then
        prepare_man
    fi
}

self_test() {
    local repo failures=0 modes_before modes_after
    WORK="$(mktemp -d)"
    repo="$WORK/repo"
    trap 'rm -rf "$WORK"' EXIT

    mkdir -p "$repo/integrations/npm/platforms/linux-x64"
    cat >"$repo/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/do-harness"]

[workspace.package]
version = "0.3.0"
edition = "2021"
EOF
    cat >"$repo/integrations/npm/package.json" <<'EOF'
{
  "name": "do-harness",
  "version": "0.3.0",
  "optionalDependencies": {
    "do-harness-linux-x64": "0.3.0",
    "do-harness-darwin-arm64": "0.3.0"
  }
}
EOF
    cat >"$repo/integrations/npm/platforms/linux-x64/package.json" <<'EOF'
{
  "name": "do-harness-linux-x64",
  "version": "0.3.0"
}
EOF
    git -C "$repo" init -q -b main
    git -C "$repo" config user.email self-test@example.invalid
    git -C "$repo" config user.name self-test
    git -C "$repo" add -A
    git -C "$repo" commit -qm 'chore: seed'
    git -C "$repo" tag v0.3.0

    # Every derivation below reads the fixture repository, never this checkout.
    ROOT="$repo"

    commit() { git -C "$repo" commit -q --allow-empty -m "$1"; }
    # release <version>: apply the derived version the way the release PR would,
    # commit it, and tag it, so the next rule is measured from a clean baseline.
    release() {
        substitute "s/^version *= *\"[0-9][0-9.]*\"/version = \"$1\"/" "$repo/Cargo.toml"
        commit "chore(release): v$1"
        git -C "$repo" tag "v$1"
    }
    expect() { # expect <expected-bump> <expected-next> <description>
        local want_bump="$1" want_next="$2" description="$3"
        derive "$REF"
        if [ "$BUMP" = "$want_bump" ] && [ "$NEXT" = "$want_next" ]; then
            printf 'ok   %s (bump=%s next=%s)\n' "$description" "$BUMP" "$NEXT"
        else
            printf 'FAIL %s: expected bump=%s next=%s, got bump=%s next=%s\n' \
                "$description" "$want_bump" "$want_next" "$BUMP" "$NEXT" >&2
            failures=$((failures + 1))
        fi
    }

    commit 'docs: explain the release gate'
    commit 'test(cli): cover the new flag'
    commit 'refactor(core): simplify the payload decoder'
    expect none 0.3.0 'docs, tests and refactors alone cut no release'

    commit 'fix(db): close the write lock'
    expect patch 0.3.1 'a fix cuts a patch'
    release 0.3.1

    commit 'feat(cli): add a status command'
    expect patch 0.3.2 'pre-1.0 a feature cuts a patch, not the minor'
    release 0.3.2

    commit 'perf(core): drop a full-tree scan'
    expect patch 0.3.3 'perf cuts a patch'
    release 0.3.3

    commit 'Revert "feat(cli): add a status command"'
    expect patch 0.3.4 'a non-conventional subject cuts a patch'
    release 0.3.4

    commit 'FIX(core): mixed-case types still count'
    expect patch 0.3.5 'the type is case-insensitive (spec §15)'
    release 0.3.5

    commit 'docs: another note'
    expect none 0.3.5 'a conventional non-bumping type still cuts nothing'

    # A merge commit's subject is generated by git; only branch commits count.
    git -C "$repo" checkout -q -b side
    commit 'docs: side note'
    git -C "$repo" checkout -q main
    git -C "$repo" merge -q --no-ff -m "Merge branch 'side'" side
    expect none 0.3.5 'a merge commit is not a release change'

    commit 'refactor(core)!: drop the legacy payload'
    expect minor 0.4.0 'pre-1.0 breaking cuts the minor, not the major'
    release 0.4.0

    commit 'fix(core): repair the payload decoder'
    commit "$(printf 'fix(core): tighten the decoder\n\nBREAKING CHANGE: the decoder rejects truncated payloads')"
    expect minor 0.5.0 'a BREAKING CHANGE footer is breaking'
    release 0.5.0

    release 1.2.3
    commit 'feat(core): add a payload format'
    expect minor 1.3.0 'a feature past 1.0 cuts the minor'
    release 1.3.0

    commit 'feat(core)!: replace the payload format'
    expect major 2.0.0 'breaking past 1.0 cuts the major, outranking a feature'
    release 2.0.0

    # `0.0.x` cannot be reached by a linear fixture that already tagged 2.0.0,
    # so drive the pure rule directly.
    BREAKING=true
    FEATURE=true
    FIX=false
    NONCONVENTIONAL=false
    if [ "$(decide_bump 0.0.4)" = patch ]; then
        printf 'ok   0.0.x stays on patches even for breaking changes\n'
    else
        printf 'FAIL 0.0.x derived %s\n' "$(decide_bump 0.0.4)" >&2
        failures=$((failures + 1))
    fi

    substitute 's/^version *= *"2\.0\.0"/version = "2.1.0"/' "$repo/Cargo.toml"
    expect pending 2.1.0 'an untagged workspace version is pending'

    # A manifest behind the newest tag is an inconsistency, not a bump.
    substitute 's/^version *= *"2\.1\.0"/version = "1.2.0"/' "$repo/Cargo.toml"
    if (derive HEAD) >/dev/null 2>&1; then
        printf 'FAIL a manifest behind the newest tag was accepted\n' >&2
        failures=$((failures + 1))
    else
        printf 'ok   a manifest behind the newest tag fails closed\n'
    fi
    substitute 's/^version *= *"1\.2\.0"/version = "2.1.0"/' "$repo/Cargo.toml"

    DO_LOCK=false
    DO_MAN=false
    # The fixture is written under the developer's umask, so pin its mode first:
    # the assertion below has to fail for a mode change, not for a umask.
    chmod 644 "$repo/Cargo.toml"
    modes_before="$(manifest_modes "$repo")"
    cmd_prepare 2.1.1 >/dev/null
    local check file expected
    for check in \
        "$repo/Cargo.toml:version = \"2.1.1\"" \
        "$repo/integrations/npm/package.json:\"version\": \"2.1.1\"" \
        "$repo/integrations/npm/package.json:\"do-harness-linux-x64\": \"2.1.1\"" \
        "$repo/integrations/npm/package.json:\"do-harness-darwin-arm64\": \"2.1.1\"" \
        "$repo/integrations/npm/platforms/linux-x64/package.json:\"version\": \"2.1.1\""
    do
        file="${check%%:*}"
        expected="${check#*:}"
        if grep -qF "$expected" "$file"; then
            printf 'ok   prepare wrote %s into %s\n' "$expected" "$file"
        else
            printf 'FAIL prepare left %s out of %s\n' "$expected" "$file" >&2
            failures=$((failures + 1))
        fi
    done
    if grep -qF '"0.3.0"' "$repo/integrations/npm/package.json"; then
        printf 'FAIL prepare left a stale version in the npm meta manifest\n' >&2
        failures=$((failures + 1))
    else
        printf 'ok   prepare left no stale npm version\n'
    fi
    modes_after="$(manifest_modes "$repo")"
    if [ "$modes_before" = "$modes_after" ]; then
        printf 'ok   prepare preserves the manifest modes\n'
    else
        printf 'FAIL prepare changed a manifest mode:\nbefore:\n%s after:\n%s\n' \
            "$modes_before" "$modes_after" >&2
        failures=$((failures + 1))
    fi

    # Idempotent: a retried job must not fail on its own earlier work.
    if [ "$(cmd_prepare 2.1.1)" = "workspace is already at 2.1.1" ]; then
        printf 'ok   prepare is idempotent for an already-applied version\n'
    else
        printf 'FAIL prepare is not idempotent\n' >&2
        failures=$((failures + 1))
    fi

    # The guard both fail-closed checks above call. 0.9.0 < 0.10.0 by component
    # and 0.9.0 > 0.10.0 lexically, so this pins the comparison to the former.
    if version_lt 0.9.0 0.10.0 && ! version_lt 0.10.0 0.9.0 &&
        ! version_lt 1.2.3 1.2.3; then
        printf 'ok   version_lt orders by component, not lexically\n'
    else
        printf 'FAIL version_lt mis-orders versions\n' >&2
        failures=$((failures + 1))
    fi

    if [ "$failures" -ne 0 ]; then
        printf 'release-manager: %s self-test failures\n' "$failures" >&2
        exit 1
    fi
    printf 'release-manager: self-test passed\n'
}

main() {
    local command="${1:-}" version=""
    if [ $# -gt 0 ]; then
        shift
    fi
    while [ $# -gt 0 ]; do
        case "$1" in
            --root)
                ROOT="${2:-}"
                shift 2
                ;;
            --ref)
                REF="${2:-}"
                shift 2
                ;;
            --format)
                FORMAT="${2:-}"
                shift 2
                ;;
            --no-lock)
                DO_LOCK=false
                shift
                ;;
            --no-man)
                DO_MAN=false
                shift
                ;;
            -h | --help)
                usage
                exit 0
                ;;
            -*)
                die "unknown option: $1"
                ;;
            *)
                [ -z "$version" ] || die "unexpected argument: $1"
                version="$1"
                shift
                ;;
        esac
    done
    case "$command" in
        status | next)
            case "$FORMAT" in
                plain | json) : ;;
                *) die "--format takes plain or json" ;;
            esac
            derive "$REF"
            if [ "$command" = status ] || [ "$FORMAT" = plain ]; then
                print_plain
            fi
            if [ "$command" = next ] && [ "$FORMAT" = json ]; then
                print_json
            fi
            ;;
        prepare)
            cmd_prepare "$version"
            ;;
        --self-test)
            self_test
            ;;
        "")
            usage
            exit 1
            ;;
        *)
            die "unknown command: $command"
            ;;
    esac
}

main "$@"

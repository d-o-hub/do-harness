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
#       in Cargo.toml, the intra-workspace requirement pins in the member
#       manifests (`do-harness-* = { path = …, version = … }`), Cargo.lock
#       (workspace members are pins in it), the six npm manifests (the meta
#       version plus its five platform pins), and the generated man page, whose
#       header announces the semver.
#
#   --self-test
#       Hermetic controls in a throwaway repository: every commit class maps to
#       its bump, a `BREAKING CHANGE:` footer counts as breaking, a merge commit
#       is ignored, an untagged workspace version reports `pending`, a manifest
#       behind the newest tag fails closed, a malformed version or an unresolvable
#       ref is refused, and `prepare` rewrites every managed manifest, the member
#       pins and both lock/man steps through a fake cargo. A check that cannot
#       fail is decoration.
#
#   --e2e [<version>]
#       The real preparation path with the real tools, in a scratch `git worktree`
#       of this repository: `prepare` bumps every manifest, refreshes Cargo.lock,
#       and the bumped tree must still resolve under `cargo metadata --locked`.
#       The caller's own checkout is never written to. Needs cargo; opt-in.
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

# Force the C locale for the whole run. Under a UTF-8 locale, bash's `[0-9]` in
# `=~` is not an ASCII class: it also matches Arabic-Indic (`١`) and fullwidth
# (`１`) digits, so `require_version` would accept a version that cargo rejects.
# C also makes every glob, sort and comparison byte-deterministic.
export LC_ALL=C

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

# The --e2e scratch worktree and the checkout it was cut from. Globals for the
# same reason: the EXIT trap that removes the worktree runs after cmd_e2e has
# returned, whether it returned or died.
E2E_CALLER="" E2E_SCRATCH=""

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
  --e2e [<version>]          real prepare in a scratch worktree (needs cargo)

options:
  --root <dir>               repository to operate on (default: this checkout)
  --ref <rev>                revision to derive from (default: HEAD)
  --format plain|json        output shape for `next` (default: plain)
  --no-lock                  skip the Cargo.lock refresh in `prepare`
  --no-man                   skip regenerating man/do-harness.1 in `prepare`
EOF
}

# A full-string numeric MAJOR.MINOR.PATCH, and nothing else. The shell glob this
# replaced (`[0-9]*.[0-9]*.[0-9]*`) accepts `1.2.3.4`, `1.2.3-rc1` and
# `1.2.3; rm -rf /` — `*` is a wildcard, not a quantifier — and the accepted
# value is then interpolated into the `sed` expression that rewrites a manifest.
# Pre-release and build metadata (`1.2.3-rc1`, `1.2.3+build`) are out of scope on
# purpose: this pipeline publishes plain MAJOR.MINOR.PATCH only.
#
# `0|[1-9][0-9]*` per component, not `[0-9]+`: a leading zero (`01.2.3`) is not a
# semver component, is written into every manifest, and then breaks the
# `$((…))` arithmetic in `next_version` (bash reads `08` as octal and errors).
# The class is ASCII-only because the script pins `LC_ALL=C` above.
readonly RE_VERSION='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'

# Every version that reaches a manifest passes through here first — the command
# line, the derivation, and Cargo.toml included. The check is `=~` against an
# anchored pattern rather than `case`, so a value a human or git supplied cannot
# pass by matching a prefix.
require_version() {
    local version="$1" origin="$2"
    if [[ ! "$version" =~ $RE_VERSION ]]; then
        die "'$version' is not a MAJOR.MINOR.PATCH version (from $origin)"
    fi
}

workspace_version() {
    # The whole quoted value, not a numeric prefix: a stale `2.1.0-beta` has to
    # reach require_version and fail there, rather than be read as `2.1.0`.
    sed -n '/^\[workspace.package\]/,/^\[/ s/^version *= *"\([^"]*\)"/\1/p' \
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

# The number of commits in <range>, or a hard failure. The `|| printf '0'` this
# replaced made a revision git cannot walk indistinguishable from an empty
# history: the run then reported a plausible `bump=none` instead of the broken
# range that caused it. A count that comes from nowhere is a guess, not a fact.
commit_count() {
    local range="$1" count
    if ! count="$(git -C "$ROOT" rev-list --count "$range")"; then
        die "cannot count commits in range '$range' in $ROOT"
    fi
    printf '%s' "$count"
}

# Sets BREAKING/FEATURE/FIX/NONCONVENTIONAL from the subjects and bodies in
# <range>. Both logs are read before classifying: a process substitution hands
# the loop whatever the producer wrote and throws the producer's exit status
# away, so a range git rejects would classify as "nothing landed" — the same
# false `bump=none` the commit count used to produce.
#
# Matched with bash's own ERE rather than `grep -q` in a pipeline:
# `grep -q` exits at the first match, and under `pipefail` the resulting SIGPIPE
# on the producer would make a matching commit read as a non-match. The patterns
# live in variables because `[[:space:]]` ends the `[[ ]]` test when written
# inline.
readonly RE_BREAKING='^[a-zA-Z]+(\([^)]*\))?!:'      # feat(cli)!:
readonly RE_FEAT='^feat(\([^)]*\))?:'                # feat(cli):
readonly RE_FIX='^(fix|perf)(\([^)]*\))?:'           # fix(cli):, perf(core):
readonly RE_CONVENTIONAL='^[a-zA-Z]+(\([^)]*\))?!?:' # any conventional type
classify() {
    local range="$1" subject lowered line subjects bodies
    BREAKING=false
    FEATURE=false
    FIX=false
    NONCONVENTIONAL=false
    if ! subjects="$(git -C "$ROOT" log --no-merges --format=%s "$range")"; then
        die "cannot read commit subjects in range '$range' in $ROOT"
    fi
    if ! bodies="$(git -C "$ROOT" log --no-merges --format=%b "$range")"; then
        die "cannot read commit bodies in range '$range' in $ROOT"
    fi
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
    done <<<"$subjects"
    while IFS= read -r line; do
        case "$line" in
            'BREAKING CHANGE:'* | 'BREAKING-CHANGE:'*) BREAKING=true ;;
        esac
    done <<<"$bodies"
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
    # A ref that does not resolve makes every git query below fail in a way that
    # reads as "no commits" (see commit_count): refuse it before anything is read.
    git -C "$ROOT" rev-parse --verify --quiet "$ref^{commit}" >/dev/null ||
        die "cannot resolve ref '$ref' as a commit in $ROOT"
    CURRENT="$(workspace_version)"
    [ -n "$CURRENT" ] || die "cannot read [workspace.package].version from $ROOT/Cargo.toml"
    # `decide_bump` subtracts from these components and `next_version` increments
    # them, so the value has to be numeric before either runs — and a malformed
    # one must not be interpolated into the `sed` expressions in `prepare`.
    require_version "$CURRENT" "[workspace.package].version in $ROOT/Cargo.toml"
    BASIS="$(latest_tag "$ref")"
    range="$(range_for "$ref")"
    COMMITS="$(commit_count "$range")"
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
    # NEXT is arithmetic on a validated CURRENT, so this cannot fire today; it is
    # the assertion that keeps it that way if the bump rules ever change.
    require_version "$NEXT" "the derived next version (current=$CURRENT bump=$BUMP)"
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
    # `$version` is safe to interpolate into this sed expression because
    # require_version has already proven it to be digits and dots only. An
    # unvalidated value could otherwise change the expression's meaning (a `/`
    # or `&`, say) instead of just its replacement text.
    substitute "/^\[workspace.package\]/,/^\[/ s/^version *= *\"[^\"]*\"/version = \"$version\"/" \
        "$ROOT/Cargo.toml"
}

prepare_pins() {
    # Every intra-workspace requirement names the workspace version literally
    # (`do-harness-types = { path = "../types", version = "0.3.0" }` in
    # crates/db, crates/do-harness and crates/guardian-proxy), and cargo reads
    # that as `^0.3.0` — a range that excludes the next minor. Cargo has no
    # "inherit the workspace version" form for a path dependency, so a minor or
    # major bump that stopped at `[workspace.package].version` would leave the
    # tree unresolvable: `cargo metadata --locked` (the `prepare_lock` step and
    # the tag preflight) fails with "failed to select a version for the
    # requirement". This is the same edit every hand-cut release made (see
    # `git log -S'do-harness-types' -- crates/db/Cargo.toml`), now applied by
    # `prepare`. Patch bumps do not need it (`^0.3.0` contains 0.3.1), which is
    # why the omission survived the 0.3.0 cut.
    #
    # Only keys shaped `do-harness-*` are touched: the vendored `libsql` alias
    # (`{ package = "do-harness-libsql", version = "0.9.30" }`) carries its own
    # version line and must keep it. A `dev-dependencies` pin is the same
    # requirement and is rewritten with the same pattern.
    local version="$1" file
    for file in "$ROOT"/crates/*/Cargo.toml; do
        [ -f "$file" ] || continue
        substitute \
            "s/^\(do-harness-[a-z0-9-]* *= *{[^}]*version *= *\)\"[^\"]*\"/\1\"$version\"/" \
            "$file"
    done
}

prepare_npm() {
    local version="$1" meta="$ROOT/integrations/npm/package.json" path
    [ -f "$meta" ] || die "missing $meta"
    # Same validated value, so the same reasoning as prepare_cargo applies.
    substitute "s/^\([[:space:]]*\"version\": \)\"[^\"]*\"/\1\"$version\"/" "$meta"
    # The platform pins carry the workspace version too:
    # check-package-contract.sh fails the release when they disagree.
    substitute "s/^\([[:space:]]*\"do-harness-[a-z0-9-]*\": \)\"[^\"]*\"/\1\"$version\"/" "$meta"
    for path in "$ROOT"/integrations/npm/platforms/*/package.json; do
        [ -f "$path" ] || continue
        substitute "s/^\([[:space:]]*\"version\": \)\"[^\"]*\"/\1\"$version\"/" "$path"
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
    require_version "$version" "the prepare argument"
    local current
    current="$(workspace_version)"
    [ -n "$current" ] || die "cannot read [workspace.package].version from $ROOT/Cargo.toml"
    # A malformed value in Cargo.toml must stop the run here rather than travel
    # into the sed expressions below or into `version_lt`'s arithmetic.
    require_version "$current" "[workspace.package].version in $ROOT/Cargo.toml"
    # Idempotent: re-running with the version already applied is a no-op rather
    # than an error, so a retried job does not fail on its own earlier work.
    if [ "$version" = "$current" ]; then
        printf 'workspace is already at %s\n' "$version"
    else
        [ "$version" = "$current" ] || version_lt "$current" "$version" ||
            die "'$version' does not supersede the current $current"
        prepare_cargo "$version"
        prepare_pins "$version"
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

# Removes the --e2e scratch worktree, on success and on failure alike. `die`
# exits and the EXIT trap is the only thing that still runs then, so the caller's
# repository must never be left with a registered worktree pointing at a deleted
# directory — or a leftover copy of its sources under /tmp.
cleanup_e2e() {
    if [ -n "$E2E_SCRATCH" ]; then
        # --force: `prepare` has modified tracked files, which a plain
        # `worktree remove` refuses to discard.
        git -C "$E2E_CALLER" worktree remove --force "$E2E_SCRATCH" 2>/dev/null || true
        git -C "$E2E_CALLER" worktree prune 2>/dev/null || true
    fi
    if [ -n "$WORK" ]; then
        rm -rf "$WORK"
    fi
}

# The real preparation path with the real tools, in a scratch `git worktree` of
# this repository. The hermetic self-test fakes cargo, so it cannot see a stale
# Cargo.lock — and a stale lock after a version bump is exactly the failure the
# tag preflight (`cargo build --locked`) hits. This mode is the integration half:
# opt-in, because it needs cargo and a moment of real dependency resolution.
cmd_e2e() {
    local version="${1:-}" expected actual file missing base add_out failures=0
    command -v cargo >/dev/null 2>&1 ||
        die "--e2e needs cargo on PATH: the lock refresh and the --locked check are the point"
    E2E_CALLER="$(cd "$ROOT" && pwd)"
    # `prepare` rewrites tracked files, so it runs in a worktree, never in the
    # caller's checkout — whose own status is asserted byte-identical below.
    base="$(git -C "$E2E_CALLER" status --porcelain)"
    WORK="$(mktemp -d)"
    E2E_SCRATCH="$WORK/repo"
    trap 'cleanup_e2e' EXIT
    # git's progress line is noise in a sensor run, so it is captured rather than
    # printed — and shown when the worktree cannot be created, which is when it
    # is the only diagnostic there is.
    if ! add_out="$(git -C "$E2E_CALLER" worktree add --detach "$E2E_SCRATCH" "$REF" 2>&1)"; then
        die "--e2e cannot add a scratch worktree at '$REF' under $E2E_CALLER: $add_out"
    fi
    ROOT="$E2E_SCRATCH"
    DO_LOCK=true
    # The man step is skipped on purpose: regenerating man/do-harness.1 means a
    # cold `cargo run` of the whole CLI (a build), which a release gate cannot
    # pay for. The committed page is covered by the `man_sync` test and by the
    # release pull request's own diff; this mode covers the manifests and the
    # lock, the part a bump gets silently wrong.
    DO_MAN=false
    if [ -z "$version" ]; then
        derive "$REF"
        case "$BUMP" in
            major | minor | patch) version="$NEXT" ;;
            # `none` and `pending` leave NEXT equal to CURRENT, and `prepare`
            # would then be a no-op: a check that cannot fail. Ask for a patch so
            # the run always has a change to make.
            *) version="$(next_version "$CURRENT" patch)" ;;
        esac
    fi
    require_version "$version" "the --e2e argument"
    printf 'e2e: preparing %s in %s\n' "$version" "$E2E_SCRATCH"
    # Called plainly, never inside `if !`: errexit is suppressed inside an `if`
    # condition and that suppression reaches this function body, so a failing
    # lock step would stop being fatal — the exact swallowing this mode exists to
    # catch. `prepare`'s own lock step is the first gate: the member crates pin
    # `^<minor>.0`, so `prepare_pins` has to rewrite those requirements alongside
    # the manifests, and a bump that leaves one stale fails there with cargo's
    # exit code, in the scratch worktree and never in the caller's checkout.
    cmd_prepare "$version"

    # (1) The lock the bump just refreshed must still satisfy --locked. This is
    # the reviewer's `cargo metadata --locked`: a stale lock fails right here.
    if (cd "$E2E_SCRATCH" && cargo metadata --locked --format-version 1 >/dev/null); then
        printf 'ok   e2e: the bumped tree resolves with cargo metadata --locked\n'
    else
        printf 'FAIL e2e: cargo metadata --locked failed after the bump (stale Cargo.lock?)\n' >&2
        failures=$((failures + 1))
    fi

    # (2) Exactly the managed surface, and nothing else, moved. The platform
    # manifests are discovered in the worktree, so the assertion follows the
    # repository instead of a list that can rot. A member manifest counts only
    # when it actually carries an intra-workspace pin, which is exactly when
    # `prepare_pins` rewrites it — so the expected set is derived the same way
    # the edit is made, rather than listing crates that have no pin.
    expected="$({
        printf '%s\n' Cargo.toml Cargo.lock integrations/npm/package.json
        for file in "$E2E_SCRATCH"/integrations/npm/platforms/*/package.json; do
            [ -f "$file" ] || continue
            printf '%s\n' "${file#"$E2E_SCRATCH"/}"
        done
        for file in "$E2E_SCRATCH"/crates/*/Cargo.toml; do
            [ -f "$file" ] || continue
            grep -qE '^do-harness-[a-z0-9-]* *= *\{[^}]*version *= *"' "$file" || continue
            printf '%s\n' "${file#"$E2E_SCRATCH"/}"
        done
    } | sort)"
    actual="$(git -C "$E2E_SCRATCH" diff --name-only | sort)"
    if [ "$actual" = "$expected" ]; then
        printf 'ok   e2e: prepare touched exactly the managed manifests and the lock\n'
    else
        printf 'FAIL e2e: unexpected diff\n--- expected ---\n%s\n--- actual ---\n%s\n' \
            "$expected" "$actual" >&2
        failures=$((failures + 1))
    fi

    # (3) "The expected files" is a weaker claim than "the expected files carry
    # the expected version", so check both.
    missing=""
    while IFS= read -r file; do
        [ -n "$file" ] || continue
        grep -qF -- "$version" "$E2E_SCRATCH/$file" || missing="$missing $file"
    done <<<"$expected"
    if [ -z "$missing" ]; then
        printf 'ok   e2e: every managed file carries %s\n' "$version"
    else
        printf 'FAIL e2e: %s missing %s\n' "$missing" "$version" >&2
        failures=$((failures + 1))
    fi

    # (4) The invoking checkout's git status is untouched: no new, modified or
    # deleted tracked file appears. The worktree is the whole reason this mode can
    # run `prepare` at all when the caller has uncommitted work of their own.
    # (Porcelain status, not a byte comparison: it does not see ignored files or
    # `.git/worktrees` registrations, which the cleanup trap also removes.)
    actual="$(git -C "$E2E_CALLER" status --porcelain)"
    if [ "$actual" = "$base" ]; then
        printf 'ok   e2e: the invoking checkout is unchanged\n'
    else
        printf 'FAIL e2e: the invoking checkout changed\n--- before ---\n%s\n--- after ---\n%s\n' \
            "$base" "$actual" >&2
        failures=$((failures + 1))
    fi

    if [ "$failures" -ne 0 ]; then
        printf 'release-manager: %s e2e failures\n' "$failures" >&2
        exit 1
    fi
    printf 'release-manager: e2e passed\n'
}

self_test() {
    local repo failures=0 modes_before modes_after name bad real want count fakebin
    local deps="" sep=""
    local -a platforms=()
    WORK="$(mktemp -d)"
    repo="$WORK/repo"
    trap 'rm -rf "$WORK"' EXIT

    mkdir -p "$repo/integrations/npm/platforms"
    cat >"$repo/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/do-harness"]

[workspace.package]
version = "0.3.0"
edition = "2021"
EOF
    # A member manifest with the two shapes that matter: an intra-workspace
    # requirement pin (`do-harness-*`, rewritten by `prepare`) and the vendored
    # `libsql` alias, whose version line is a different crate's and must be left
    # alone. Without this fixture the pin rewrite would be untested, and without
    # the alias line a pattern that rewrote every `version =` would pass.
    mkdir -p "$repo/crates/do-harness"
    cat >"$repo/crates/do-harness/Cargo.toml" <<'EOF'
[package]
name = "do-harness"
version.workspace = true

[dependencies]
do-harness-types = { path = "../types", version = "0.3.0" }
libsql = { package = "do-harness-libsql", version = "0.9.30" }
EOF
    # The fixture mirrors every platform manifest this checkout ships, so a
    # platform added later cannot land without this test covering it. Enumerating
    # REPO_ROOT's directory names is a listdir: no cargo, no network, and the
    # fixture itself stays hermetic.
    for real in "$REPO_ROOT"/integrations/npm/platforms/*/package.json; do
        [ -f "$real" ] || continue
        name="$(basename "$(dirname "$real")")"
        platforms+=("$name")
        mkdir -p "$repo/integrations/npm/platforms/$name"
        cat >"$repo/integrations/npm/platforms/$name/package.json" <<EOF
{
  "name": "do-harness-$name",
  "version": "0.3.0"
}
EOF
        deps="$deps$sep    \"do-harness-$name\": \"0.3.0\""
        sep=$',\n'
    done
    [ "${#platforms[@]}" -gt 0 ] ||
        die "no platform manifests under $REPO_ROOT/integrations/npm/platforms"
    cat >"$repo/integrations/npm/package.json" <<EOF
{
  "name": "do-harness",
  "version": "0.3.0",
  "optionalDependencies": {
$deps
  }
}
EOF
    # The fixture must mirror every platform the repository ships: a fixture that
    # quietly shrank would make the platform assertions below pass vacuously. The
    # real set is discovered again here, independently of the loop above, so
    # pinning that loop to one directory fails here instead of just reducing the
    # assertion count.
    for real in "$REPO_ROOT"/integrations/npm/platforms/*/package.json; do
        [ -f "$real" ] || continue
        name="$(basename "$(dirname "$real")")"
        if [ -f "$repo/integrations/npm/platforms/$name/package.json" ]; then
            printf 'ok   the fixture covers the %s platform manifest\n' "$name"
        else
            printf 'FAIL the fixture is missing the %s platform manifest\n' "$name" >&2
            failures=$((failures + 1))
        fi
    done
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
    # expect_die <message-fragment> <description> <command...>: the command must
    # fail *for the stated reason*. "It exited non-zero somehow" would also be
    # satisfied by an unrelated crash, and the guards here exist to make a bad
    # input loud rather than to make the run fail in any way at all.
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
    wrote() { # wrote <file> <expected line>
        if grep -qF -- "$2" "$1"; then
            printf 'ok   prepare wrote %s into %s\n' "$2" "$1"
        else
            printf 'FAIL prepare left %s out of %s\n' "$2" "$1" >&2
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
    expect_die 'behind the newest release tag' \
        'a manifest behind the newest tag fails closed' derive HEAD
    substitute 's/^version *= *"1\.2\.0"/version = "2.1.0"/' "$repo/Cargo.toml"

    # A fake cargo stands in for the real one: the fast self-test must not build
    # anything. It logs its argv — that log is what proves `prepare` really
    # reached the lock and the man steps, which `DO_LOCK=false`/`DO_MAN=false`
    # used to hide — and it writes the page the CLI would write, so the man
    # assertion is about the invocation rather than about a compile.
    fakebin="$WORK/bin"
    mkdir -p "$fakebin"
    export FAKE_CARGO_LOG="$WORK/cargo.log"
    : >"$FAKE_CARGO_LOG"
    cat >"$fakebin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FAKE_CARGO_LOG"
# `cargo run ... -- man man` has the CLI write man/do-harness.1 itself.
if [ "${1:-}" = run ]; then
    mkdir -p man
    printf '.TH do-harness 1 "do-harness 2.1.1"\n' >man/do-harness.1
fi
EOF
    chmod +x "$fakebin/cargo"
    export PATH="$fakebin:$PATH"

    DO_LOCK=true
    DO_MAN=true
    # The fixture is written under the developer's umask, so pin its mode first:
    # the assertion below has to fail for a mode change, not for a umask.
    chmod 644 "$repo/Cargo.toml"
    modes_before="$(manifest_modes "$repo")"
    cmd_prepare 2.1.1 >/dev/null
    wrote "$repo/Cargo.toml" 'version = "2.1.1"'
    wrote "$repo/integrations/npm/package.json" '"version": "2.1.1"'
    # Every platform manifest and every pin, discovered rather than listed: a new
    # platform directory enters the fixture above and is asserted on here.
    for name in "${platforms[@]}"; do
        wrote "$repo/integrations/npm/package.json" "\"do-harness-$name\": \"2.1.1\""
        wrote "$repo/integrations/npm/platforms/$name/package.json" '"version": "2.1.1"'
    done
    if grep -qF '"0.3.0"' "$repo/integrations/npm/package.json"; then
        printf 'FAIL prepare left a stale version in the npm meta manifest\n' >&2
        failures=$((failures + 1))
    else
        printf 'ok   prepare left no stale npm version\n'
    fi
    # The member pin is rewritten; the vendored alias on the next line is not.
    wrote "$repo/crates/do-harness/Cargo.toml" \
        'do-harness-types = { path = "../types", version = "2.1.1" }'
    wrote "$repo/crates/do-harness/Cargo.toml" \
        'libsql = { package = "do-harness-libsql", version = "0.9.30" }'
    # The lock and the man page were part of the run: each invocation exactly
    # once, and nothing else built along the way.
    for want in \
        'metadata --format-version 1' \
        'run --quiet -p do-harness --bin do-harness -- man man'
    do
        count="$(grep -cF -- "$want" "$FAKE_CARGO_LOG" || true)"
        if [ "$count" -eq 1 ]; then
            printf 'ok   prepare invoked cargo "%s" exactly once\n' "$want"
        else
            printf 'FAIL prepare invoked cargo "%s" %s time(s)\n' "$want" "$count" >&2
            failures=$((failures + 1))
        fi
    done
    count="$(wc -l <"$FAKE_CARGO_LOG" | tr -d ' ')"
    if [ "$count" -eq 2 ]; then
        printf 'ok   prepare built nothing beyond the lock and the man page\n'
    else
        printf 'FAIL prepare made %s cargo invocation(s), expected 2\n' "$count" >&2
        failures=$((failures + 1))
    fi
    if [ -f "$repo/man/do-harness.1" ]; then
        printf "ok   the man step ran the CLI as 'man man' in the repository root\n"
    else
        printf 'FAIL the man step never wrote man/do-harness.1\n' >&2
        failures=$((failures + 1))
    fi
    modes_after="$(manifest_modes "$repo")"
    if [ "$modes_before" = "$modes_after" ]; then
        printf 'ok   prepare preserves the manifest modes\n'
    else
        printf 'FAIL prepare changed a manifest mode:\nbefore:\n%s after:\n%s\n' \
            "$modes_before" "$modes_after" >&2
        failures=$((failures + 1))
    fi

    # Idempotent: a retried job must not fail on its own earlier work. The lock
    # and man steps are switched off in a subshell, so this stays an assertion
    # about the version comparison rather than about re-running cargo.
    if [ "$(DO_LOCK=false DO_MAN=false; cmd_prepare 2.1.1)" = "workspace is already at 2.1.1" ]; then
        printf 'ok   prepare is idempotent for an already-applied version\n'
    else
        printf 'FAIL prepare is not idempotent\n' >&2
        failures=$((failures + 1))
    fi

    # The version validator. The glob this replaced accepted `1.2.3.4`,
    # `1.2.3-rc1`, `1.2.3+build` and `1.2.3; rm -rf /`, and an accepted value
    # then travelled into the sed expression that rewrites every manifest. The
    # last two sit *above* the current version, which is the case the supersede
    # guard below cannot catch: without the validator they are written into every
    # manifest, not refused. `01.2.3`/`1.2.08` are the leading-zero shapes (not
    # semver, and `08` later breaks `$((…))`), and `١.٢.٣` is the Arabic-Indic
    # shape a UTF-8 locale's `[0-9]` used to accept — refused only because the
    # script pins `LC_ALL=C`.
    # `\$(...)` is a literal command substitution in the input, not one this
    # script runs: the point is that the argument reaches prepare as text.
    for bad in 1.2 1.2.3.4 1.2.x 1.2.3-rc1 1.2.3+build '1.2.3; rm -rf /' \
        2.2.0-rc1 '2.1.2; rm -rf /' "\$(echo hi).2.3" 01.2.3 1.2.08 ١.٢.٣; do
        expect_die 'is not a MAJOR.MINOR.PATCH version' \
            "prepare rejects '$bad'" cmd_prepare "$bad"
    done
    # A malformed value already in the manifest must stop the run too, rather
    # than be read as its numeric prefix.
    substitute 's/^version *= *"2\.1\.1"/version = "2.1.0-beta"/' "$repo/Cargo.toml"
    expect_die 'is not a MAJOR.MINOR.PATCH version' \
        'a malformed [workspace.package].version is refused' derive HEAD
    substitute 's/^version *= *"2\.1\.0-beta"/version = "2.1.1"/' "$repo/Cargo.toml"

    # An unresolvable ref used to read as "nothing since nothing".
    for bad in no-such-ref 'HEAD^{tree}'; do
        expect_die 'cannot resolve ref' \
            "derive refuses the unresolvable ref '$bad'" derive "$bad"
    done
    # A bogus range used to become a commit count of 0 (`|| printf '0'`) and, in
    # the log read, an empty history — a plausible `bump=none` either way.
    expect_die 'cannot count commits in range' \
        'a bogus range fails closed on the commit count' \
        commit_count 'v0.3.0..no-such-ref'
    expect_die 'cannot read commit subjects in range' \
        'a bogus range fails closed in the log parsing' \
        classify 'v0.3.0..no-such-ref'

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
        --e2e)
            cmd_e2e "$version"
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

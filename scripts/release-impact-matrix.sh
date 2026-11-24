#!/usr/bin/env bash
# release-impact-matrix.sh — derive the Consumer Impact Matrix for a release.
#
# The release body is composed by `.github/workflows/release.yml`, and every
# body must tell each consumer group what the release does to it. This script
# derives the matrix from the conventional commits and the changed paths since
# the previous release tag, offline and deterministically:
#
#   matrix <ref> [--out <file>] [--format plain|json]
#       Derive the matrix for <ref> (normally the release tag `vX.Y.Z`). The
#       basis is the previous release tag reachable from <ref>; with no
#       previous tag the whole reachable history is the range. Prints the
#       matrix as markdown (plain) or JSON; fails closed on an empty range or
#       an unresolvable ref.
#
#   --self-test
#       Hermetic controls in a throwaway repository: a breaking commit touching
#       the CLI marks the CLI row breaking, a sensor-pack-only change touches
#       only the packs row, and an empty range fails closed.
#
# One path can feed multiple shipped groups, such as a config template that
# feeds both the adoption contract and the sensor packs. Explicit PR metadata
# adds consumer upgrade instructions to the Markdown report.
#
# Impact precedence: breaking > feature > fix > updated > none.

set -euo pipefail

# Force the C locale for the whole run, matching release-manager.sh: under a
# UTF-8 locale bash's `[0-9]` in `=~` is not an ASCII class.
export LC_ALL=C

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
readonly REPO_ROOT
ROOT="$REPO_ROOT"
FORMAT="plain"
OUT=""
ACTIONS=""
PREVIOUS_TAG=""
PREVIOUS_SET=false

# The consumer groups, in table row order. The row label is what a reader sees;
# the `group` id is the JSON key and the lookup key for per-group results.
readonly GROUP_IDS=(cli npm packs guides contract)
readonly GROUP_LABELS=(
    "CLI binaries (Linux x86_64/arm64, macOS x86_64/arm64, Windows x86_64)"
    "npm packages (meta + platform packages)"
    "Sensor packs (scaffolded by do-harness init)"
    "Guides & skills (docs, .agents/skills)"
    "do-harness.toml contract"
)

# The action-required text per impact class. A breaking change demands an
# upgrade; a fix is recommended; a feature is optional; none is none.
impact_action() {
    case "$1" in
        breaking) printf 'Required — breaking changes to this artifact are detailed above.' ;;
        feature) printf 'Optional — new capability; upgrade when you want it.' ;;
        updated) printf 'Review the updated guidance.' ;;
        fix) printf 'Recommended — upgrade to receive the fixes.' ;;
        *) printf 'None.' ;;
    esac
}

# The change class of one commit, from its subject and body. The regexes are
# release-manager.sh's (spec §15 type case-insensitivity, §16 footer token
# uppercase); breaking outranks feature and fix, and any other conventional
# type is `none`.
readonly RE_BREAKING='^[a-zA-Z]+(\([^)]*\))?!:'      # feat(cli)!:
readonly RE_FEAT='^feat(\([^)]*\))?:'                # feat(cli):
readonly RE_DOCS='^docs(\([^)]*\))?:'
readonly RE_FIX='^(fix|perf)(\([^)]*\))?:'           # fix(cli):, perf(core):
class_of() {
    local subject="$1" body="$2" lowered line
    lowered="$(printf '%s' "$subject" | tr '[:upper:]' '[:lower:]')"
    if [[ "$lowered" =~ $RE_BREAKING ]]; then
        printf 'breaking'
        return 0
    fi
    while IFS= read -r line; do
        case "$line" in
            'BREAKING CHANGE:'* | 'BREAKING-CHANGE:'*)
                printf 'breaking'
                return 0
                ;;
        esac
    done <<<"$body"
    if [[ "$lowered" =~ $RE_FEAT ]]; then
        printf 'feature'
        return 0
    fi
    if [[ "$lowered" =~ $RE_FIX ]]; then
        printf 'fix'
        return 0
    fi
    if [[ "$lowered" =~ $RE_DOCS ]]; then
        printf 'updated'
        return 0
    fi
    printf 'none'
}

# ordering: none < fix < feature < breaking; returns the higher of the two.
max_class() {
    local a="$1" b="$2"
    case "$a:$b" in
        breaking:* | *:breaking) printf 'breaking' ;;
        feature:* | *:feature) printf 'feature' ;;
        fix:* | *:fix) printf 'fix' ;;
        updated:* | *:updated) printf 'updated' ;;
        *) printf 'none' ;;
    esac
}

die() {
    printf 'release-impact-matrix: %s\n' "$1" >&2
    exit 1
}

usage() {
    cat <<'EOF'
usage: release-impact-matrix.sh <command> [options]

  matrix <ref>            derive the Consumer Impact Matrix for <ref>
                          (default ref: HEAD; default format: plain markdown)
  --self-test             hermetic controls in a throwaway repository

options:
  --root <dir>            repository to operate on (default: this checkout)
  --previous <tag>           use the same basis as the generated changelog
  --actions <file>           inputs containing Consumer-Impact metadata
  --out <file>            write the matrix to <file> instead of stdout
  --format plain|json     output shape (default: plain)
EOF
}

# The highest release tag reachable from <ref>, by version order. Mirrors
# release-manager.sh's latest_tag; an absent basis means whole-history range.
latest_tag() {
    git -C "$ROOT" tag --merged "$1" --list 'v[0-9]*' --sort=-v:refname 2>/dev/null \
        | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -1 || true
}

# The commit count in <range>, or a hard failure: a revision git cannot walk
# must not read as an empty history.
commit_count() {
    local range="$1" count
    if ! count="$(git -C "$ROOT" rev-list --count "$range")"; then
        die "cannot count commits in range '$range' in $ROOT"
    fi
    printf '%s' "$count"
}

# The changed paths in the range. With a basis it is the two-point diff; with
# no basis (a first release) the whole tree at <ref> is new.
changed_paths() {
    local ref="$1" basis="$2"
    if [ -n "$basis" ]; then
        git -C "$ROOT" diff --name-only "$basis..$ref" \
            || die "cannot diff range '$basis..$ref' in $ROOT"
    else
        git -C "$ROOT" ls-tree -r --name-only "$ref" \
            || die "cannot list the tree at '$ref' in $ROOT"
    fi
}

impact_title() {
    case "$1" in
        breaking) printf 'Breaking' ;;
        feature) printf 'Feature' ;;
        fix) printf 'Fix' ;;
        updated) printf 'Updated' ;;
        *) printf 'None' ;;
    esac
}

# Some shipped inputs feed multiple consumers. Repository-local configuration
# is an instance of the adoption contract, whose definitions are listed here.
groups_for_path() {
    case "$1" in
        crates/do-harness/templates/do-harness.toml.*) printf '%s\n' packs contract ;;
        crates/do-harness/templates/*) printf '%s\n' packs ;;
        crates/do-harness/src/config/* | crates/do-harness/src/migrate.rs | crates/do-harness/src/doctor/checks.rs)
            printf '%s\n' cli contract ;;
        Cargo.lock) printf '%s\n' cli npm ;;
        integrations/npm/*) printf '%s\n' npm ;;
        crates/* | vendor/*) printf '%s\n' cli ;;
        AGENTS.md | docs/* | .agents/skills/*) printf '%s\n' guides ;;
    esac
}

# Sets CLASS_OF[hash]=class for every non-merge commit in the range, read as
# one record per commit so a git failure fails the run instead of reading as
# an empty history.
declare -A CLASS_OF
classify_range() {
    local range="$1" records record hash rest subject body
    if ! records="$(git -C "$ROOT" log --no-merges --format='%H%x1f%s%x1f%b%x1e' "$range")"; then
        die "cannot read commit subjects and bodies in range '$range' in $ROOT"
    fi
    while IFS= read -r -d $'\x1e' record; do
        # `git log --format` appends its normal newline after the record
        # separator; it becomes the first byte of every record after the first.
        record="${record#$'\n'}"
        [ -n "$record" ] || continue
        hash="${record%%$'\x1f'*}"
        rest="${record#*$'\x1f'}"
        subject="${rest%%$'\x1f'*}"
        body="${rest#*$'\x1f'}"
        CLASS_OF["$hash"]="$(class_of "$subject" "$body")"
    done <<<"$records"
}

# Runs `matrix` with the current ROOT/REF/FORMAT/OUT and prints the result.
matrix() {
    local ref="$1" basis range group hash impact path paths hashes line action i
    local -A GROUP_FILES IMPACT REQUIRED_ACTION
    local action_pattern='^Consumer-Impact: (cli|npm|packs|guides|contract) [|] ([^|<>]+)$'
    if [ -n "$ACTIONS" ]; then
        while IFS= read -r line || [ -n "$line" ]; do
            line="${line%$'\r'}"
            case "$line" in
                'Consumer-Impact:'*)
                    [[ "$line" =~ $action_pattern ]] || die "invalid Consumer-Impact entry"
                    group="${BASH_REMATCH[1]}"
                    action="${BASH_REMATCH[2]}"
                    REQUIRED_ACTION["$group"]="${REQUIRED_ACTION[$group]:+${REQUIRED_ACTION[$group]}<br>}$action"
                    ;;
            esac
        done <"$ACTIONS"
    fi
    # A ref that does not resolve makes every git query below fail in a way
    # that reads as "no commits": refuse it before anything is read.
    git -C "$ROOT" rev-parse --verify --quiet "$ref^{commit}" >/dev/null ||
        die "cannot resolve ref '$ref' as a commit in $ROOT"
    if [ "$PREVIOUS_SET" = true ]; then
        basis="$PREVIOUS_TAG"
        if [ -n "$basis" ]; then
            git -C "$ROOT" merge-base --is-ancestor "refs/tags/$basis" "$ref" ||
                die "previous tag is missing or outside the release history: $basis"
        fi
    else
        basis="$(latest_tag "$ref")"
    fi
    # When <ref> is itself a release tag, `tag --merged` counts it as merged
    # into itself; the basis is the next release tag strictly below it.
    if [ "$PREVIOUS_SET" = false ] && [ -n "$basis" ] && [ "$basis" = "$ref" ]; then
        basis="$(git -C "$ROOT" tag --merged "$ref" --list 'v[0-9]*' --sort=-v:refname 2>/dev/null \
            | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | grep -vxF "$ref" | head -1 || true)"
    fi
    if [ -n "$basis" ]; then
        range="$basis..$ref"
    else
        range="$ref"
    fi
    # An empty range is a broken release, not a "nothing changed" release: a
    # body with nothing to report would pass vacuously.
    if [ "$(commit_count "$range")" -eq 0 ]; then
        die "empty range '$range' (no commits since the previous release tag); nothing to report"
    fi
    classify_range "$range"
    # Partition the changed paths into per-group file lists.
    paths="$(changed_paths "$ref" "$basis")" || return 1
    while IFS= read -r path; do
        [ -n "$path" ] || continue
        while IFS= read -r group; do
            [ -n "$group" ] || continue
            GROUP_FILES["$group"]+=$'\n'"$path"
        done <<<"$(groups_for_path "$path")"
    done <<<"$paths"
    # One row per group: the highest change class among the commits that touch
    # its files. A group no commit touches is `none`.
    for i in "${!GROUP_IDS[@]}"; do
        group="${GROUP_IDS[i]}"
        local -a files=() kept=()
        if [ -n "${GROUP_FILES[$group]:-}" ]; then
            mapfile -t files <<<"${GROUP_FILES[$group]}"
            # The here-string's trailing newline leaves one empty element;
            # drop it so no empty pathspec reaches git.
            for path in "${files[@]}"; do
                [ -n "$path" ] || continue
                kept+=("$path")
            done
            files=("${kept[@]}")
        fi
        impact=none
        if [ "${#files[@]}" -gt 0 ]; then
            if ! hashes="$(git -C "$ROOT" --literal-pathspecs log --no-merges --format='%H' "$range" -- "${files[@]}")"; then
                die "cannot read impact history for $group"
            fi
            while IFS= read -r hash; do
                [ -n "$hash" ] || continue
                impact="$(max_class "$impact" "${CLASS_OF[$hash]:-none}")"
            done <<<"$hashes"
        fi
        if [ "$impact" = none ] && [ -n "${REQUIRED_ACTION[$group]:-}" ]; then
            impact=updated
        fi
        IMPACT["$group"]="$impact"
    done
    # Emit the matrix in the fixed group order.
    if [ "$FORMAT" = json ]; then
        printf '{"tag":"%s","basis":"%s","commits":%s,"rows":[' \
            "$ref" "${basis:-null}" "$(commit_count "$range")"
        local sep=""
        for i in "${!GROUP_IDS[@]}"; do
            group="${GROUP_IDS[i]}"
            printf '%s{"group":"%s","label":"%s","impact":"%s"}' \
                "$sep" "$group" "${GROUP_LABELS[i]}" "${IMPACT[$group]}"
            sep=","
        done
        printf ']}\n'
    else
        printf '## Consumer Impact\n\n'
        printf '| Consumer group | Impact | Action required |\n'
        printf '|---|---|---|\n'
        for i in "${!GROUP_IDS[@]}"; do
            group="${GROUP_IDS[i]}"
            action="$(impact_action "${IMPACT[$group]}")"
            if [ -n "${REQUIRED_ACTION[$group]:-}" ]; then
                action="Required — ${REQUIRED_ACTION[$group]}"
            fi
            printf '| %s | %s | %s |\n' \
                "${GROUP_LABELS[i]}" \
                "$(impact_title "${IMPACT[$group]}")" \
                "$action"
        done
    fi
}

main() {
    require_arg() {
        [ $# -ge 2 ] || die "$1 requires a value"
    }
    local command="${1:-}"
    if [ $# -gt 0 ]; then
        shift
    fi
    while [ $# -gt 0 ]; do
        case "$1" in
            --root)
                require_arg "$@"
                ROOT="${2:-}"
                shift 2
                ;;
            --previous)
                require_arg "$@"
                PREVIOUS_TAG="$2"
                PREVIOUS_SET=true
                shift 2
                ;;
            --actions)
                require_arg "$@"
                ACTIONS="$2"
                shift 2
                ;;
            --out)
                require_arg "$@"
                OUT="${2:-}"
                shift 2
                ;;
            --format)
                require_arg "$@"
                FORMAT="${2:-}"
                shift 2
                ;;
            -h | --help)
                usage
                exit 0
                ;;
            -*)
                die "unknown option: $1"
                ;;
            *)
                if [ -z "${MATRIX_REF:-}" ]; then
                    MATRIX_REF="$1"
                else
                    die "unexpected argument: $1"
                fi
                shift
                ;;
        esac
    done
    case "$command" in
        matrix)
            case "$FORMAT" in
                plain | json) : ;;
                *) die "--format takes plain or json" ;;
            esac
            local output
            output="$(matrix "${MATRIX_REF:-HEAD}")"
            if [ -n "$OUT" ]; then
                printf '%s\n' "$output" >"$OUT"
            else
                printf '%s\n' "$output"
            fi
            ;;
        --self-test)
            # shellcheck source=scripts/release-impact-matrix-self-test.sh
            source "$SCRIPT_DIR/release-impact-matrix-self-test.sh"
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

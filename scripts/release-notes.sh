#!/usr/bin/env bash
# release-notes.sh — compose and validate the GitHub Release body.
#
# The `release` job routes the published notes through this script:
#
#   compose <tag> <generated> <out> [<curated>]
#       A curated header followed by the merged pull requests since the
#       previous tag, regrouped by the conventional-commit type in each pull
#       request title. The header is `docs/releases/<tag>.md` when that file
#       exists at the tagged commit, otherwise `.github/release-notes-template.md`
#       with `{{TAG}}`/`{{VERSION}}` substituted.
#
#   check <tag> <file>
#       Fail unless the composed body carries the sections a reader needs.
#
#   seed <tag> [<out>]
#       Write the curated-header stub for one release: a byte-identical copy of
#       `.github/release-notes-template.md`. Byte-identical is the point — `compose`
#       substitutes `{{TAG}}`/`{{VERSION}}` in a curated header too, so an untouched
#       stub publishes exactly the body the template alone would, and an edited one
#       is used verbatim. `auto-release.yml` seeds it inside the release pull
#       request, which is what puts the header in the *tagged* tree the `release`
#       job composes from; run it by hand only when a release is cut without the
#       automation. An existing file is never overwritten.
#
#   --self-test
#       Hermetic controls: a compliant body passes, a bare release dump is
#       rejected, and a bullet the composer fails to place is reported. A check
#       that cannot fail is decoration.
#
# Why the regrouping lives here instead of in `.github/release.yml`: GitHub's
# generated changelog groups by pull-request label, and this repository labels no
# pull requests (0 of the 74 merged between v0.1.1 and v0.1.2 carried any label),
# so every entry landed in one catch-all bucket — the categories were dead
# configuration. Conventional-commit subjects are enforced by the commitlint
# sensor, so they are the grouping that actually exists.
#
# A bullet carrying `!` (a breaking change) is listed under "Breaking changes"
# instead of its type section, so no entry appears twice. `compose` fails if the
# regrouped list does not keep every bullet it was given: a silently dropped
# change is worse than a badly categorized one.
set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

ROOT="$(dh_repo_root)"
TEMPLATE="$ROOT/.github/release-notes-template.md"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# Section order for the generated side of the body. "Breaking changes" is first
# because it is the only section a reader must not miss.
CATEGORY_ORDER=(
    "Breaking changes"
    "Features"
    "Fixes"
    "Performance"
    "Documentation"
    "Refactors"
    "Tests"
    "Reverts"
    "Maintenance"
    "Other changes"
    # The generator's `## New Contributors` section is not a change list, so its
    # bullets are bucketed by section rather than by title and reported once, at
    # the end, instead of being filed as an untyped change.
    "New Contributors"
)

fail() {
    echo "release-notes.sh: $*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Compose and validate the GitHub Release body.

Usage:
  release-notes.sh compose <tag> <generated-notes> <out> [<curated-notes>] [<impact-matrix>]
  release-notes.sh check <tag> <file>
  release-notes.sh seed <tag> [<out>]
  release-notes.sh --self-test
EOF
}

# Regroups a generated changelog. Reads stdin, writes the change sections.
# Keeps each bullet verbatim (title, author, pull-request link) and the
# "Full Changelog" trailer; drops the generator's own headings, its label-based
# category names, and its provenance comment.
regroup() {
    awk -v order="$(printf '%s\n' "${CATEGORY_ORDER[@]}" | paste -sd '|' -)" '
        function bucket_for(type) {
            if (type == "feat") return "Features"
            if (type == "fix") return "Fixes"
            if (type == "perf") return "Performance"
            if (type == "docs") return "Documentation"
            if (type == "refactor") return "Refactors"
            if (type == "test") return "Tests"
            if (type == "revert") return "Reverts"
            if (type == "chore" || type == "ci" || type == "build" || type == "style") return "Maintenance"
            return "Other changes"
        }
        # The conventional type is the leading [a-z]+ before a scope, a bang, or
        # the colon; anything else is not a conventional subject.
        function type_of(title,   t) {
            if (title !~ /^[a-z]+[(!:]/) return ""
            t = title
            sub(/[(!:].*$/, "", t)
            return t
        }
        function bucket(title,   type) {
            type = type_of(title)
            if (type == "") return "Other changes"
            if (title ~ /^[a-z]+(\([^)]*\))?!:/ || title ~ /^[a-z]+!:/) {
                return "Breaking changes"
            }
            # A title with no conventional type (a release merge, a hand-written
            # subject) is kept rather than dropped.
            return bucket_for(type)
        }
        {
            line = $0
            # The generator emits its own top-level sections, and only the
            # section a bullet sits in decides how a contributor line is filed.
            # Every heading is still dropped from the output, so the body keeps
            # the category order defined above.
            if (line ~ /^## /) {
                section = line
                sub(/^## /, "", section)
                next
            }
            if (line ~ /^\* /) {
                body = substr(line, 3)
                title = body
                trailer = ""
                if (match(body, / by @[^ ]+ in https?:\/\/[^ ]+$/)) {
                    title = substr(body, 1, RSTART - 1)
                    trailer = substr(body, RSTART)
                }
                if (section == "New Contributors") {
                    buckets["New Contributors"] = buckets["New Contributors"] "* " title trailer "\n"
                } else {
                    buckets[bucket(title)] = buckets[bucket(title)] "* " title trailer "\n"
                }
                next
            }
            if (line ~ /^\*\*Full Changelog\*\*/) {
                trailerline = line
            }
        }
        END {
            n = split(order, names, "|")
            for (i = 1; i <= n; i++) {
                name = names[i]
                if (buckets[name] == "") continue
                print "### " name
                printf "%s\n", buckets[name]
            }
            if (trailerline != "") print trailerline
        }
    '
}

count_bullets() {
    grep -c '^\* ' "$1" || true
}

# Artifact-version readers (adoption contract, sensor packs, guides) live in
# release-artifacts.sh so this file stays under the 500-LOC ceiling.
# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/release-artifacts.sh"

compose() {
    local tag="$1" generated="$2" out="$3" curated="${4:-}" impact="${5:-}"
    local version="${tag#v}" header changes="$WORK/changes.md" want have

    [[ -f "$generated" ]] || fail "generated notes not found: $generated"
    if [[ -n "$impact" ]]; then
        [[ -f "$impact" ]] || fail "impact matrix not found: $impact"
    fi
    if [[ -n "$curated" ]]; then
        [[ -f "$curated" ]] || fail "curated notes not found: $curated"
        header="$curated"
    elif [[ -f "$ROOT/docs/releases/$tag.md" ]]; then
        header="$ROOT/docs/releases/$tag.md"
    else
        header="$TEMPLATE"
    fi

    regroup <"$generated" >"$changes"
    want="$(count_bullets "$generated")"
    have="$(count_bullets "$changes")"
    [[ "$want" == "$have" ]] ||
        fail "regrouping kept $have of $want pull requests; every change must reach the body"

    {
        sed -e "s/{{TAG}}/$tag/g" -e "s/{{VERSION}}/$version/g" "$header"
        if [[ -n "$impact" ]]; then
            # The Consumer Impact Matrix sits between the curated header and the
            # change list: it is the first thing a consumer reads after the
            # curated sections. `release-impact-matrix.sh matrix` emits the
            # whole section, heading included.
            printf '\n'
            cat "$impact"
        fi
        # Every independently-versioned artifact ships its version in the body:
        # the CLI tag, the adoption contract, each sensor pack, and the guides.
        # A version file missing from the tree prints `not declared`.
        printf '\n## Artifacts\n\n'
        printf '| Artifact | Version |\n| :--- | :--- |\n'
        artifact_version_table "$tag"
        printf '\n## Changes\n\n'
        cat "$changes"
    } >"$out"
}
# Fails when a section a reader needs is missing or empty. Reports every problem,
# not just the first, so one release run names all of them.
check() {
    local tag="$1" file="$2"
    local version="${tag#v}" problems=0 section body
    local required=("Breaking changes" "Install" "Verify the download" "Consumer Impact" "Artifacts" "Changes")

    [[ -f "$file" ]] || fail "release notes not found: $file"

    if grep -q '{{TAG}}\|{{VERSION}}' "$file"; then
        echo "FAIL: unsubstituted placeholder remains in the release notes"
        problems=$((problems + 1))
    fi
    # The generated trailer names the tag too, so the version is required in the
    # curated part: a reader must see which release they are looking at before
    # the change list starts.
    if ! awk '/^## Changes$/ { exit } { print }' "$file" | grep -qF "$version"; then
        echo "FAIL: the curated header does not name the released version ($version)"
        problems=$((problems + 1))
    fi

    for section in "${required[@]}"; do
        if ! grep -qxF "## $section" "$file"; then
            echo "FAIL: release notes have no '## $section' section"
            problems=$((problems + 1))
            continue
        fi
        # Section body: the lines after the heading, up to the next heading.
        body="$(awk -v want="## $section" '
            $0 == want { inside = 1; next }
            inside && /^## / { exit }
            inside { print }
        ' "$file" | grep -vE '^[[:space:]]*$' || true)"
        if [[ -z "$body" ]]; then
            echo "FAIL: '## $section' is empty"
            problems=$((problems + 1))
        fi
        if [[ "$section" == "Consumer Impact" ]]; then
            # The matrix is a table with the three fixed columns and at least
            # one data row: a heading alone is the same empty-section failure.
            if ! printf '%s\n' "$body" | grep -q '^| Consumer group | Impact | Action required |$'; then
                echo "FAIL: '## Consumer Impact' lacks the '| Consumer group | Impact | Action required |' header"
                problems=$((problems + 1))
            fi
            if [[ "$(printf '%s\n' "$body" | grep -c '^| ')" -lt 3 ]]; then
                echo "FAIL: '## Consumer Impact' has no data rows"
                problems=$((problems + 1))
            fi
        fi
    done

    if ((problems > 0)); then
        echo "release-notes.sh: $problems problem(s) in the release body" >&2
        return 1
    fi
    echo "release notes OK: ${#required[@]} sections present, version $version named"
    return 0
}

# Writes the curated-header stub for one release, and never overwrites: an edit
# made while the release pull request is open outranks a re-run of the seeding
# step. The default destination is outside this script's own working tree, so the
# caller decides when the repository is touched at all.
seed() {
    local tag="$1" out="${2:-}"
    [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
        fail "seed needs a release tag like v1.2.3, got '$tag'"
    out="${out:-$ROOT/docs/releases/$tag.md}"
    [[ -f "$TEMPLATE" ]] || fail "release-notes template not found: $TEMPLATE"
    if [[ -e "$out" ]]; then
        echo "release-notes.sh: $out exists; leaving it unchanged"
        return 0
    fi
    mkdir -p "$(dirname "$out")"
    cp "$TEMPLATE" "$out"
    echo "release-notes.sh: seeded $out — edit it before the release pull request merges"
}


if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
case "${1:-}" in
    compose)
        [[ $# -ge 4 ]] || {
            usage
            exit 2
        }
        compose "$2" "$3" "$4" "${5:-}" "${6:-}"
        ;;
    check)
        [[ $# -eq 3 ]] || {
            usage
            exit 2
        }
        check "$2" "$3"
        ;;
    seed)
        [[ $# -ge 2 ]] || {
            usage
            exit 2
        }
        seed "$2" "${3:-}"
        ;;
    --self-test)
        bash "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/release-notes-self-test.sh"
        ;;
    -h | --help)
        usage
        ;;
    *)
        usage
        exit 2
        ;;
esac
fi

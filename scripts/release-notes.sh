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

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT/.github/release-notes-template.md"
# Set by `seed --carry-from <file>`: a copy of the notes header taken from the
# release branch being replaced, so a curated header survives a re-derivation.
CARRY_FROM=""
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
  release-notes.sh compose <tag> <generated-notes> <out> [<curated-notes>]
  release-notes.sh check <tag> <file>
  release-notes.sh seed <tag> [<out>] [--carry-from <file>]
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

compose() {
    local tag="$1" generated="$2" out="$3" curated="${4:-}"
    local version="${tag#v}" header changes="$WORK/changes.md" want have

    [[ -f "$generated" ]] || fail "generated notes not found: $generated"
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
        printf '\n## Changes\n\n'
        cat "$changes"
    } >"$out"
}

# Fails when a section a reader needs is missing or empty. Reports every problem,
# not just the first, so one release run names all of them.
check() {
    local tag="$1" file="$2"
    local version="${tag#v}" problems=0 section body
    local required=("Breaking changes" "Install" "Verify the download" "Changes")

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
    # A refresh force-pushes the release branch, so an edit made in the pull
    # request would be discarded. The notes header is the one file the pull request
    # exists to edit, so a copy that differs from the template is carried forward
    # rather than re-seeded; a copy identical to it is the stub, which the seed
    # writes anyway.
    if [[ -n "$CARRY_FROM" ]]; then
        [[ -f "$CARRY_FROM" ]] || fail "seed: --carry-from needs an existing file, got '$CARRY_FROM'"
        if ! cmp -s "$CARRY_FROM" "$TEMPLATE"; then
            mkdir -p "$(dirname "$out")"
            cp "$CARRY_FROM" "$out"
            echo "release-notes.sh: carried the curated $out forward from $CARRY_FROM"
            return 0
        fi
    fi
    mkdir -p "$(dirname "$out")"
    cp "$TEMPLATE" "$out"
    echo "release-notes.sh: seeded $out — edit it before the release pull request merges"
}

self_test() {
    local generated="$WORK/generated" composed="$WORK/composed.md" dump="$WORK/dump.md"
    local expected reason

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

    compose "v0.1.3" "$generated" "$composed"
    check "v0.1.3" "$composed" >/dev/null ||
        fail "self-test: notes composed from the template failed the check"

    for expected in \
        "### Breaking changes" "### Features" "### Fixes" "### Documentation" \
        "### Refactors" "### Maintenance" "### Other changes" "### New Contributors"; do
        grep -qxF "$expected" "$composed" ||
            fail "self-test: composed notes are missing '$expected'"
    done
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

    # seed: the header a maintainer wrote is the one thing a refresh must not
    # discard, while a copy that is still the template is just the stub.
    local seed_dir="$WORK/seed" curated="$WORK/curated.md" stub_copy="$WORK/stub.md"
    mkdir -p "$seed_dir"
    cp "$TEMPLATE" "$stub_copy"
    CARRY_FROM="$stub_copy"
    seed "v0.3.9" "$seed_dir/notes.md" >/dev/null
    cmp -s "$TEMPLATE" "$seed_dir/notes.md" ||
        fail "self-test: a template copy was not treated as the stub"
    rm -f "$seed_dir/notes.md"
    cp "$TEMPLATE" "$curated"
    printf '\n## Highlights\n\nEdited by a maintainer.\n' >>"$curated"
    CARRY_FROM="$curated"
    seed "v0.3.9" "$seed_dir/notes.md" >/dev/null
    grep -q 'Edited by a maintainer' "$seed_dir/notes.md" ||
        fail "self-test: a curated header was not carried forward"
    CARRY_FROM=""
    seed "v0.3.9" "$seed_dir/notes.md" >/dev/null
    grep -q 'Edited by a maintainer' "$seed_dir/notes.md" ||
        fail "self-test: seed overwrote an existing header"
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
    compose "v0.1.3" "$generated" "$WORK/stubbed.md" "$stub"
    check "v0.1.3" "$WORK/stubbed.md" >/dev/null ||
        fail "self-test: an unedited seeded stub failed the check"

    echo "release-notes.sh self-test OK (regroups by type, files contributors apart, keeps every bullet, seeds without overwriting, carries a curated header forward, rejects a bare dump and an empty section)"
}

case "${1:-}" in
    compose)
        [[ $# -ge 4 ]] || {
            usage
            exit 2
        }
        compose "$2" "$3" "$4" "${5:-}"
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
        seed_tag="$2"
        shift 2
        seed_out=""
        if [[ $# -gt 0 && "${1:-}" != --* ]]; then
            seed_out="$1"
            shift
        fi
        while [[ $# -gt 0 ]]; do
            case "$1" in
                --carry-from)
                    CARRY_FROM="${2:-}"
                    [[ -n "$CARRY_FROM" ]] || {
                        usage
                        exit 2
                    }
                    shift 2
                    ;;
                *)
                    usage
                    exit 2
                    ;;
            esac
        done
        seed "$seed_tag" "$seed_out"
        ;;
    --self-test)
        self_test
        ;;
    -h | --help)
        usage
        ;;
    *)
        usage
        exit 2
        ;;
esac

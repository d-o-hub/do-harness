#!/usr/bin/env bash
# verify-published.sh — prove a published release is complete on every channel.
#
# Sensor: bash scripts/verify-published.sh --self-test
#
# The pipeline already orders publication (the `release` job needs crates.io,
# npm and both registry validations, so a GitHub Release never claims a version
# that is missing elsewhere). What no job can prove from the inside is what a
# *consumer* sees: that the assets on the Release are the ones that were built,
# that the installer's result reports the released version, and that npm and
# crates.io actually carry it. That verdict lives here.
#
# The split is deliberate: this script owns the policy and the digest arithmetic
# and is hermetic (`--self-test` needs no network and no release), while
# `.github/workflows/release.yml` owns the network I/O and writes the facts this
# script judges. A test that needs the network is not a test.
#
# Usage:
#   verify-published.sh --tag <vX.Y.Z> [--dir <assets>] [--facts <file>]
#                       [--root <dir>] [--require-immutable] [--report-only]
#   verify-published.sh --self-test
#
# `--dir` holds the release's downloaded assets and its `checksums.txt`.
# `--facts` is key=value, written by the workflow from its own queries:
#
#   release=present|missing      gh release view
#   npm=<version>|missing        npm view do-harness@<version> version
#   npm_provenance=ok|missing    npm view do-harness@<version> dist.attestations
#   npm_latest=<version>         npm view do-harness dist-tags.latest (advisory)
#   crates=<name>=<version>,…    crates.io API for the version-tracking crates
#                                (do-harness, -db, -types); do-harness-libsql is
#                                a vendored fork on its own version and is advisory
#   install=<version>|missing    `do-harness --version` of an installed binary
#   attestation=ok|missing       gh attestation verify <archive> --repo …
#   immutable=ok|missing         gh release verify <tag> (advisory by default)
#
# A fact that is absent is `missing`, never "fine": every required check reads
# its fact positively, so a workflow bug cannot turn a check into a pass by
# omitting it. Two empty values comparing equal is how a digest gate passes
# vacuously — every comparison here rejects an empty side first.
#
# Exit: 0 every required check passed; 1 a required check failed; 2 usage error.
# With `--report-only` the exit code is always 0 and the summary says REPORTED,
# which is the ratchet's first position: visible, never silent, not yet blocking.
set -euo pipefail

export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

TAG=""
DIR=""
FACTS=""
REQUIRE_IMMUTABLE=false
REPORT_ONLY=false
FAILURES=0
PASSES=0

usage() {
    sed -n 's/^# \{0,1\}//; /^Usage:/,/^#$/p' "${BASH_SOURCE[0]}" | sed '/^#$/d' >&2
}

die() {
    printf 'verify-published: %s\n' "$1" >&2
    exit 2
}

pass() {
    PASSES=$((PASSES + 1))
    printf 'PASS\t%s\t%s\n' "$1" "$2"
}

fail() {
    FAILURES=$((FAILURES + 1))
    printf 'FAIL\t%s\t%s\n' "$1" "$2"
}

note() {
    printf 'NOTE\t%s\t%s\n' "$1" "$2"
}

# digest_of <file>: SHA-256 hex, or empty when no tool is available.
digest_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

# fact <key>: the value of <key> in the facts file, empty when absent.
fact() {
    local key="$1"
    [ -n "$FACTS" ] && [ -f "$FACTS" ] || return 0
    sed -n "s/^$key=//p" "$FACTS" | tail -1
}

# The version the tagged tree claims, from its own manifests. A release whose
# tag disagrees with the tree it was cut from is not the release it says it is.
tag_version() {
    printf '%s' "${TAG#v}"
}

tree_version() {
    sed -n '/^\[workspace.package\]/,/^\[/ s/^version *= *"\([^"]*\)"/\1/p' \
        "$ROOT/Cargo.toml" | head -1
}

# The vendored libsql fork is versioned independently, so its expected version
# is the pin the released tree carries, not the workspace version.
libsql_version() {
    sed -n 's/.*package *= *"do-harness-libsql", *version *= *"\([^"]*\)".*/\1/p' \
        "$ROOT/crates/db/Cargo.toml" | head -1
}

check_tree() {
    local want tree
    want="$(tag_version)"
    tree="$(tree_version)"
    if [ -z "$want" ] || [ -z "$tree" ]; then
        fail tree "cannot compare tag '$TAG' with the workspace version (tag='$want' tree='$tree')"
        return
    fi
    if [ "$want" != "$tree" ]; then
        fail tree "tag $TAG names $want but the tree says $tree"
        return
    fi
    pass tree "tag $TAG matches the workspace version $tree"
}

check_release() {
    local state
    state="$(fact release)"
    if [ "$state" = "present" ]; then
        pass release "the GitHub Release for $TAG exists"
    else
        fail release "no published GitHub Release for $TAG (fact: '${state:-missing}')"
    fi
}

# Every asset the checksums name must be present and match, and every file
# present must be named: a Release carrying an unlisted asset is an asset no
# consumer can verify.
check_assets() {
    local checksums="$DIR/checksums.txt" name want got listed=0 verified=0
    if [ -z "$DIR" ] || [ ! -d "$DIR" ]; then
        fail checksums "no asset directory given (--dir)"
        return
    fi
    if [ ! -f "$checksums" ]; then
        fail checksums "no checksums.txt in $DIR"
        return
    fi
    if [ ! -s "$checksums" ]; then
        fail checksums "checksums.txt in $DIR is empty"
        return
    fi
    while read -r want name; do
        [ -n "$name" ] || continue
        listed=$((listed + 1))
        if [ -z "$want" ]; then
            fail checksums "$name is listed without a digest"
            continue
        fi
        if [ ! -f "$DIR/$name" ]; then
            fail checksums "$name is listed in checksums.txt but missing from the Release"
            continue
        fi
        got="$(digest_of "$DIR/$name")"
        if [ -z "$got" ]; then
            fail checksums "cannot compute a digest for $name (no sha256sum or shasum)"
            continue
        fi
        if [ "$want" != "$got" ]; then
            fail checksums "$name digest mismatch (expected $want, got $got)"
            continue
        fi
        verified=$((verified + 1))
    done <"$checksums"
    if [ "$listed" -eq 0 ]; then
        fail checksums "checksums.txt names no asset"
        return
    fi
    local file
    while IFS= read -r file; do
        name="$(basename "$file")"
        [ "$name" = "checksums.txt" ] && continue
        if ! grep -qE "[[:space:]]${name//./\\.}\$" "$checksums"; then
            fail checksums "$name is on the Release but not in checksums.txt"
        fi
    done < <(find "$DIR" -maxdepth 1 -type f | sort)
    if [ "$verified" -eq "$listed" ]; then
        pass checksums "$verified of $listed asset(s) match checksums.txt"
    fi
}

check_npm() {
    local version provenance
    version="$(fact npm)"
    provenance="$(fact npm_provenance)"
    if [ "$version" = "$(tag_version)" ]; then
        pass npm "npm serves do-harness $version"
    else
        fail npm "npm serves '${version:-missing}' for do-harness, expected $(tag_version)"
    fi
    if [ "$provenance" = "ok" ]; then
        pass npm_provenance "npm carries SLSA provenance for $version"
    else
        fail npm_provenance "npm has no provenance for '${version:-missing}' (fact: '${provenance:-missing}')"
    fi
    local latest
    latest="$(fact npm_latest)"
    if [ -n "$latest" ] && [ "$latest" != "$(tag_version)" ]; then
        note npm_latest "dist-tags.latest is $latest, not $(tag_version) (a backfill keeps latest ahead)"
    fi
}

# crates=<name>=<version>,… compared against the version the released tree
# carries. The three crates that inherit `[workspace.package].version` must match
# the tag. The vendored `do-harness-libsql` fork is versioned on its own and
# published idempotently — a release never moves it — so it is reported against
# its pin and never fails the gate.
check_crates() {
    local entries entry name actual expected=0 ok=0
    entries="$(fact crates)"
    if [ -z "$entries" ]; then
        fail crates "no crates.io facts (fact: 'crates' absent)"
        return
    fi
    local -A want=(
        [do-harness]="$(tree_version)"
        [do-harness-db]="$(tree_version)"
        [do-harness-types]="$(tree_version)"
        [do-harness-libsql]="$(libsql_version)"
    )
    if [ -z "${want[do-harness]}" ]; then
        fail crates "cannot read the expected crate versions from $ROOT"
        return
    fi
    local IFS=,
    for entry in $entries; do
        name="${entry%%=*}"
        actual="${entry#*=}"
        [ -n "$name" ] || continue
        if [ -z "${want[$name]:-}" ]; then
            fail crates "unexpected crate '$name' in the facts"
            continue
        fi
        if [ "$name" = "do-harness-libsql" ]; then
            if [ "$actual" = "${want[$name]}" ]; then
                note crates "the vendored fork is published at its pin $actual"
            else
                note crates "the vendored fork reports '${actual:-missing}', its pin is ${want[$name]} (advisory)"
            fi
            continue
        fi
        expected=$((expected + 1))
        if [ -z "$actual" ] || [ "$actual" = "missing" ]; then
            fail crates "$name is not published"
            continue
        fi
        if [ "$actual" != "${want[$name]}" ]; then
            fail crates "$name is at $actual, expected ${want[$name]}"
            continue
        fi
        ok=$((ok + 1))
    done
    if [ "$expected" -lt 3 ]; then
        fail crates "only $expected of the 3 version-tracking crates reported a version"
    elif [ "$ok" -eq "$expected" ]; then
        pass crates "all $ok version-tracking crates carry the released version"
    fi
}

check_install() {
    local version
    version="$(fact install)"
    if [ "$version" = "$(tag_version)" ]; then
        pass install "the installed binary reports $version"
    else
        fail install "the installed binary reports '${version:-missing}', expected $(tag_version)"
    fi
}

check_attestation() {
    local state
    state="$(fact attestation)"
    if [ "$state" = "ok" ]; then
        pass attestation "the release archive verifies against its build provenance"
    else
        fail attestation "no verifiable build provenance (fact: '${state:-missing}')"
    fi
}

# Immutability is advisory until the repository enables it: `gh release verify`
# fails on every tag cut before that switch, so requiring it early would block
# every release for a setting only a human can change.
check_immutable() {
    local state
    state="$(fact immutable)"
    if [ "$state" = "ok" ]; then
        pass immutable "the Release is immutable and attestation-verified"
    elif [ "$REQUIRE_IMMUTABLE" = true ]; then
        fail immutable "the Release is not immutable (fact: '${state:-missing}')"
    else
        note immutable "not immutable yet (fact: '${state:-missing}'); advisory until Settings enables it"
    fi
}

summarize() {
    local required="tree release checksums npm npm_provenance crates install attestation"
    if [ "$REQUIRE_IMMUTABLE" = true ]; then
        required="$required immutable"
    fi
    if [ "$FAILURES" -gt 0 ]; then
        if [ "$REPORT_ONLY" = true ]; then
            printf 'summary\tREPORTED\t%s\t%d failed, %d passed\n' "$TAG" "$FAILURES" "$PASSES"
            return 0
        fi
        printf 'summary\tFAIL\t%s\t%d failed, %d passed\n' "$TAG" "$FAILURES" "$PASSES"
        printf 'required\t%s\n' "$required"
        return 1
    fi
    printf 'summary\tPASS\t%s\t%d checks\n' "$TAG" "$PASSES"
    return 0
}

# Set at the top of self_test and kept global so the EXIT trap can see it: a
# trap fires after the function returns, when its `local`s are gone.
SELF_TEST_WORK=""

self_test() {
    local failures=0
    SELF_TEST_WORK="$(mktemp -d)"
    work="$SELF_TEST_WORK"
    trap 'rm -rf "$SELF_TEST_WORK"' EXIT
    local root="$work/root" assets="$work/assets" facts="$work/facts"

    # A fixture tree whose versions the controls own, so the self-test does not
    # depend on this checkout's version.
    mkdir -p "$root/crates/db"
    cat >"$root/Cargo.toml" <<'TOML'
[workspace]
members = ["crates/do-harness"]

[workspace.package]
version = "9.9.9"
edition = "2021"
TOML
    cat >"$root/crates/db/Cargo.toml" <<'TOML'
[package]
name = "do-harness-db"
version.workspace = true

[dependencies]
libsql = { package = "do-harness-libsql", version = "0.9.30" }
TOML

    mkfixture() {
        rm -rf "$assets"
        mkdir -p "$assets"
        printf 'linux\n' >"$assets/do-harness-v9.9.9-x86_64-unknown-linux-musl.tar.gz"
        printf 'macos\n' >"$assets/do-harness-v9.9.9-aarch64-apple-darwin.tar.gz"
        (cd "$assets" && sha256sum do-harness-v9.9.9-* >checksums.txt)
    }
    goodfacts() {
        cat >"$facts" <<'FACTS'
release=present
npm=9.9.9
npm_provenance=ok
npm_latest=9.9.9
crates=do-harness=9.9.9,do-harness-db=9.9.9,do-harness-types=9.9.9,do-harness-libsql=0.9.30
install=9.9.9
attestation=ok
immutable=missing
FACTS
    }
    run() {
        bash "${BASH_SOURCE[0]}" --tag v9.9.9 --root "$root" --dir "$assets" \
            --facts "$facts" "$@"
    }
    # Every control starts from a complete, correct release. Without this, a
    # control that damages a fixture leaks into the next one and the failure
    # it reports belongs to the previous control.
    reset() {
        mkfixture
        goodfacts
    }
    expect() { # expect <exit> <description> [extra args...]
        local want="$1" description="$2" got
        shift 2
        set +e
        out="$(run "$@" 2>&1)"
        got=$?
        set -e
        if [ "$got" -eq "$want" ]; then
            printf 'ok   %s (exit=%s)\n' "$description" "$got"
        else
            printf 'FAIL %s: expected exit=%s got %s\n%s\n' "$description" "$want" "$got" "$out" >&2
            failures=$((failures + 1))
        fi
    }
    expect_summary() { # expect_summary <PASS|FAIL|REPORTED> <description>
        local want="$1" description="$2"
        if printf '%s' "$out" | grep -q "summary	$want	"; then
            printf 'ok   %s (summary=%s)\n' "$description" "$want"
        else
            printf 'FAIL %s: expected summary %s\n%s\n' "$description" "$want" "$out" >&2
            failures=$((failures + 1))
        fi
    }
    expect_fail_line() { # expect_fail_line <check> <description>
        local want="$1" description="$2"
        if printf '%s' "$out" | grep -q "FAIL	$want	"; then
            printf 'ok   %s (FAIL %s)\n' "$description" "$want"
        else
            printf 'FAIL %s: expected a FAIL line for %s\n%s\n' "$description" "$want" "$out" >&2
            failures=$((failures + 1))
        fi
    }

    reset
    out=""
    expect 0 'a complete release passes'
    expect_summary PASS 'a complete release reports PASS'

    # A digest that does not match is the failure the Release exists to prevent.
    mkfixture
    printf 'tampered\n' >>"$assets/do-harness-v9.9.9-x86_64-unknown-linux-musl.tar.gz"
    expect 1 'a tampered asset fails'
    expect_fail_line checksums 'a tampered asset fails the checksum check'

    # A listed asset that never reached the Release.
    mkfixture
    rm "$assets/do-harness-v9.9.9-aarch64-apple-darwin.tar.gz"
    expect 1 'a missing asset fails'
    expect_fail_line checksums 'a missing asset fails the checksum check'

    # An asset on the Release that checksums.txt does not name.
    mkfixture
    printf 'stray\n' >"$assets/do-harness-v9.9.9-x86_64-pc-windows-msvc.zip"
    expect 1 'an unlisted asset fails'
    expect_fail_line checksums 'an unlisted asset fails the checksum check'

    # Empty values must never compare equal (the vacuous-pass hazard).
    mkfixture
    : >"$assets/checksums.txt"
    expect 1 'an empty checksums.txt fails'
    expect_fail_line checksums 'an empty checksums.txt fails the checksum check'

    # npm: version and provenance.
    reset
    sed -i 's/^npm=9.9.9$/npm=9.9.8/' "$facts"
    expect 1 'an npm version mismatch fails'
    expect_fail_line npm 'an npm version mismatch fails the npm check'
    reset
    sed -i 's/^npm_provenance=ok$/npm_provenance=missing/' "$facts"
    expect 1 'missing npm provenance fails'
    reset
    sed -i '/^npm_latest=/d' "$facts"
    expect 0 'an absent npm_latest is advisory only'

    # poll_npm argument forwarding and provenance regression test.
    mkdir -p "$work/bin"
    cat >"$work/bin/npm" <<'MOCK_NPM'
#!/usr/bin/env bash
if [ "$1" = "view" ]; then
    shift
    if [ $# -eq 2 ]; then
        case "$1:$2" in
            "do-harness@9.9.9:version")
                echo "9.9.9"
                return 0
                ;;
            "do-harness@9.9.9:dist.attestations.url")
                return 0
                ;;
            "do-harness-prov@9.9.9:dist.attestations.url")
                echo "https://registry.npmjs.org/-/npm/v1/attestations/do-harness-prov@9.9.9"
                return 0
                ;;
        esac
    elif [ $# -eq 1 ]; then
        echo "do-harness@9.9.9 | MIT | deps: none"
        return 0
    fi
fi
return 1
MOCK_NPM
    chmod +x "$work/bin/npm"

    poll_npm_test() {
        local PATH="$work/bin:$PATH"
        poll_npm() {
            local value=""
            for _ in $(seq 1 3); do
                value="$(npm view "$@" 2>/dev/null || true)"
                if [ -n "$value" ]; then
                    printf '%s\n' "$value"
                    return 0
                fi
                sleep 0.1
            done
            return 1
        }

        local v prov_missing prov_ok
        v="$(poll_npm "do-harness@9.9.9" version || true)"
        prov_missing="$(poll_npm "do-harness@9.9.9" dist.attestations.url || true)"
        prov_ok="$(poll_npm "do-harness-prov@9.9.9" dist.attestations.url || true)"

        if [ "$v" = "9.9.9" ] && [ -z "$prov_missing" ] && [ -n "$prov_ok" ]; then
            printf 'ok   poll_npm forwards field arguments and handles missing provenance\n'
        else
            printf 'FAIL poll_npm regression test failed: v="%s" prov_missing="%s" prov_ok="%s"\n'                 "$v" "$prov_missing" "$prov_ok" >&2
            failures=$((failures + 1))
        fi
    }
    poll_npm_test

    # crates.io: every published crate, including the independently versioned fork.
    reset
    sed -i 's/do-harness-types=9.9.9/do-harness-types=9.9.8/' "$facts"
    expect 1 'a crate at the wrong version fails'
    expect_fail_line crates 'a crate at the wrong version fails the crates check'
    reset
    sed -i 's/do-harness-libsql=0.9.30/do-harness-libsql=0.9.29/' "$facts"
    expect 0 'the vendored fork only advises against its own pin'

    # The installed binary must report the released version.
    reset
    sed -i 's/^install=9.9.9$/install=9.9.8/' "$facts"
    expect 1 'an installed binary at the wrong version fails'
    expect_fail_line install 'an installed binary at the wrong version fails the install check'

    # Provenance.
    reset
    sed -i 's/^attestation=ok$/attestation=missing/' "$facts"
    expect 1 'missing build provenance fails'
    expect_fail_line attestation 'missing build provenance fails the attestation check'

    # The release itself.
    reset
    sed -i 's/^release=present$/release=missing/' "$facts"
    expect 1 'a missing GitHub Release fails'
    expect_fail_line release 'a missing GitHub Release fails the release check'

    # Tag and tree must agree: a tag that names another version is not this tree.
    reset
    set +e
    out="$(bash "${BASH_SOURCE[0]}" --tag v9.9.8 --root "$root" --dir "$assets" \
        --facts "$facts" 2>&1)"
    got=$?
    set -e
    if [ "$got" -eq 1 ] && printf '%s' "$out" | grep -q "FAIL	tree	"; then
        printf 'ok   a tag that disagrees with the tree fails\n'
    else
        printf 'FAIL a tag that disagrees with the tree fails (exit=%s)\n%s\n' "$got" "$out" >&2
        failures=$((failures + 1))
    fi

    # Immutability: advisory by default, required when asked for.
    reset
    expect 0 'a not-yet-immutable release passes while advisory'
    expect 1 'a not-yet-immutable release fails under --require-immutable' --require-immutable
    expect_summary FAIL 'the immutable failure names FAIL'
    reset
    sed -i 's/^immutable=missing$/immutable=ok/' "$facts"
    expect 0 'an immutable release passes under --require-immutable' --require-immutable

    # The ratchet's first position: visible, never silent, not yet blocking.
    reset
    sed -i 's/^npm=9.9.9$/npm=9.9.8/' "$facts"
    expect 0 'a failure under --report-only exits 0' --report-only
    expect_summary REPORTED 'a failure under --report-only says REPORTED'

    # A fact that is absent is missing, never fine.
    mkfixture
    : >"$facts"
    expect 1 'an empty facts file fails closed'

    # Usage.
    set +e
    bash "${BASH_SOURCE[0]}" >/dev/null 2>&1
    got=$?
    set -e
    if [ "$got" -eq 2 ]; then
        printf 'ok   a missing --tag is a usage error (exit=2)\n'
    else
        printf 'FAIL a missing --tag should exit 2, got %s\n' "$got" >&2
        failures=$((failures + 1))
    fi

    if [ "$failures" -gt 0 ]; then
        printf 'verify-published: %d self-test control(s) failed\n' "$failures" >&2
        return 1
    fi
    printf 'verify-published: self-test passed\n'
}

main() {
    if [ $# -eq 0 ]; then
        die "missing --tag (see the header for usage)"
    fi
    while [ $# -gt 0 ]; do
        case "$1" in
            --tag) TAG="${2:?--tag needs a value}"; shift 2 ;;
            --dir) DIR="${2:?--dir needs a value}"; shift 2 ;;
            --facts) FACTS="${2:?--facts needs a value}"; shift 2 ;;
            --root) ROOT="${2:?--root needs a value}"; shift 2 ;;
            --require-immutable) REQUIRE_IMMUTABLE=true; shift ;;
            --report-only) REPORT_ONLY=true; shift ;;
            --self-test) self_test; exit $? ;;
            -h | --help) usage; exit 0 ;;
            *) die "unknown argument: $1" ;;
        esac
    done
    [ -n "$TAG" ] || die "--tag is required"
    # A tag is `v<digits>.<digits>.<digits>`; anything else cannot name a release
    # this pipeline cut, and the version reaches a shell comparison below.
    case "$TAG" in
        v[0-9]*.[0-9]*.[0-9]*) : ;;
        *) die "--tag takes a release tag such as v1.2.3, got '$TAG'" ;;
    esac
    TAG="$(printf '%s' "$TAG" | sed -n 's/^\(v[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)$/\1/p')"
    [ -n "$TAG" ] || die "--tag takes a plain vMAJOR.MINOR.PATCH tag"

    check_tree
    check_release
    check_assets
    check_npm
    check_crates
    check_install
    check_attestation
    check_immutable
    summarize
}

main "$@"

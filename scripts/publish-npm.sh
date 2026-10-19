#!/usr/bin/env bash
# publish-npm.sh — assemble and publish the npm wrapper packages.
#
# The meta package `do-harness` depends on five platform packages; each
# platform package contains the prebuilt binary from a release tarball.
# Publishing is idempotent (versions already on npm are skipped) and ordered
# platform-first so the meta package's optionalDependencies resolve.
#
# Usage:
#   scripts/publish-npm.sh --dist <dir> [--version X.Y.Z] [--tag latest] [--dry-run]
#
# --dist points at a directory of `do-harness-v<version>-<target>.tar.gz`
# release artifacts. --dry-run assembles every package and runs
# `npm publish --dry-run` without requiring a token (used in CI).
#
# --self-test exercises the version validation and the dist-tag decision
# hermetically — no network, no npm, no --dist.
set -euo pipefail

# ASCII-only matching and byte-deterministic output: under a UTF-8 locale bash's
# `[0-9]` also matches Arabic-Indic and fullwidth digits, so `valid_version`
# would accept a version npm cannot publish.
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NPM_DIR="$ROOT/integrations/npm"

VERSION=""
DIST=""
DRY_RUN=0
TAG="latest"
OTP=""
SELF_TEST=0

# `actions/setup-node` with `registry-url` always exports this exact dummy value
# when no token is configured, and writes it to the temp `.npmrc` as
# `//registry.npmjs.org/:_authToken=${NODE_AUTH_TOKEN}`:
#
#   core.exportVariable('NODE_AUTH_TOKEN',
#     process.env.NODE_AUTH_TOKEN || 'XXXXX-XXXXX-XXXXX-XXXXX');
#
# Treating it as a real credential would shadow GitHub's OIDC exchange, because
# the token branch below is checked first — making the trusted-publishing branch
# unreachable in exactly the CI job that is supposed to use it. It must count as
# unset, and be cleared before publishing so `.npmrc` resolves it empty.
NODE_AUTH_TOKEN_PLACEHOLDER="XXXXX-XXXXX-XXXXX-XXXXX"

die() {
    echo "publish-npm.sh: $*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Assemble and publish the do-harness npm wrapper packages.

Usage: publish-npm.sh --dist <DIR> [OPTIONS]

Options:
  --dist <DIR>       Directory with do-harness-v<version>-<target>.tar.gz.
  --version <X.Y.Z>  npm version to publish. Default: workspace version.
  --tag <TAG>        npm dist-tag. Default: latest.
  --dry-run          Assemble and `npm publish --dry-run` (no token needed).
  --otp <CODE>       One-time password for a 2FA account (bootstrap publish).
  --self-test        Run the hermetic self-test (no network, no npm).
  -h, --help         Show this help.
EOF
}

# The version must be exactly three numeric components. Unanchored, the pattern
# this replaced (`^[0-9]+\.[0-9]+\.[0-9]+`) accepted `1.2.3/x` and passed the
# trailing text to npm and to the component comparison in `dist_tag_for`. A
# leading zero (`01.2.3`) is not a semver component and npm rejects it, so each
# component is `0|[1-9][0-9]*` rather than `[0-9]+`. The class is ASCII-only
# because the script pins `LC_ALL=C` above.
valid_version() { # valid_version <version>
    [[ "$1" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]
}

# Numeric version comparison, component by component. `sort -V` is a GNU
# extension that BSD sort (macOS) rejects, and this script runs on both, so the
# two validated three-component versions are split with `IFS=.` and compared with
# arithmetic tests. Returns 0 when a < b.
version_lt() { # version_lt <a> <b>
    local a="$1" b="$2" i ai bi
    local -a ap bp
    IFS=. read -r -a ap <<<"$a"
    IFS=. read -r -a bp <<<"$b"
    for i in 0 1 2; do
        ai="${ap[i]:-0}"
        bi="${bp[i]:-0}"
        if ((ai < bi)); then
            return 0
        fi
        if ((ai > bi)); then
            return 1
        fi
    done
    return 1
}

# Picks the dist-tag to publish with. `npm publish --tag` defaults to `latest`,
# and the registry serves `latest` for a plain `npm install`, so publishing an
# OLDER version without an explicit tag would move `latest` backwards and hand
# every installer the older binaries. A backfilled release therefore publishes
# under `backfill` — still installable explicitly (`npm install
# do-harness@backfill`) but it leaves `latest` where it is. Pure: the registry's
# answer is an argument, so `--self-test` exercises it without npm or network.
dist_tag_for() { # dist_tag_for <version> <registry_latest> <requested_tag>
    local version="$1" registry_latest="$2" requested="$3"
    if [[ "$requested" != "latest" ]]; then
        printf '%s' "$requested"
        return 0
    fi
    if [[ -n "$registry_latest" ]] && version_lt "$version" "$registry_latest"; then
        printf '%s' "backfill"
        return 0
    fi
    printf '%s' "latest"
}

# The registry's current `dist-tags.latest` for a package. Exactly two answers
# are trusted: the published `latest` parsed from a 200 packument, and "no such
# package" (404 — a first publish legitimately takes `latest`). Everything else
# is a failure, never an empty answer: `curl -sS -L` exits 0 on an HTTP error, so
# a 5xx error body carries no `dist-tags` and used to be indistinguishable from a
# first publish. That sent an OLDER version to `latest` and moved the pointer
# backwards for every installer — the exact regression the `backfill` tag exists
# to prevent. Read with `curl` + `sed` so no jq/node dependency is assumed beyond
# what the job already installs.
#
# Defined above `self_test` because the `--self-test` dispatch below runs before
# the publish helpers: the hermetic stub `curl` exercises this function, so it has
# to exist by then.
registry_latest_version() {
    local pkg="$1"
    local registry="${NPM_CONFIG_REGISTRY:-https://registry.npmjs.org}"
    local out status body dist_tags latest
    out="$(curl -sS --retry 2 --retry-delay 1 -L -w $'\n%{http_code}' \
        "${registry%/}/$pkg")" || die "could not query npm registry for $pkg"
    # The status code is the last line curl wrote; the body is everything before.
    status="${out##*$'\n'}"
    body="${out%$'\n'*}"
    case "$status" in
        200) : ;;
        404) return 0 ;;
        *)
            die "npm registry returned HTTP $status for $pkg; refusing to guess the dist-tag"
            ;;
    esac
    dist_tags="$(printf '%s' "$body" | tr -d '\n' |
        sed -n 's/.*"dist-tags"[[:space:]]*:[[:space:]]*{\([^}]*\)}.*/\1/p')"
    latest="$(printf '%s' "$dist_tags" |
        sed -n 's/.*"latest"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
    # A published package whose packument names no usable `latest` is not a first
    # publish, and a prerelease latest must not reach `version_lt`'s arithmetic.
    # Refuse both rather than move the pointer by accident.
    [[ -n "$latest" ]] ||
        die "npm registry returned a packument for $pkg with no dist-tags.latest"
    valid_version "$latest" ||
        die "npm registry reports latest=$latest for $pkg, which is not MAJOR.MINOR.PATCH"
    printf '%s' "$latest"
}

self_test() {
    local failures=0 bad

    # Malformed versions must be refused before they reach npm or the
    # comparison: `1.2.3/x` and `1.2.3.4` are the shapes the unanchored pattern
    # accepted, `01.2.3`/`1.2.08` are leading-zero shapes npm rejects, and
    # `١.٢.٣` is the Arabic-Indic shape a UTF-8 locale's `[0-9]` used to accept.
    for bad in "1.2.3/x" "1.2.3.4" "1.2" "1.2.x" "v1.2.3" "" "1.2.3 " "01.2.3" "1.2.08" "١.٢.٣"; do
        if valid_version "$bad"; then
            echo "FAIL valid_version accepted '${bad}'"
            failures=$((failures + 1))
        else
            echo "ok   valid_version rejects '${bad}'"
        fi
    done
    if valid_version "1.2.3"; then
        echo "ok   valid_version accepts 1.2.3"
    else
        echo "FAIL valid_version rejected 1.2.3"
        failures=$((failures + 1))
    fi

    # The decision that keeps `latest` from moving backwards.
    if [[ "$(dist_tag_for 1.1.0 1.2.0 latest)" == "backfill" ]]; then
        echo "ok   an older version publishes under backfill"
    else
        echo "FAIL an older version did not fall back to backfill"
        failures=$((failures + 1))
    fi
    if [[ "$(dist_tag_for 1.3.0 1.2.0 latest)" == "latest" ]]; then
        echo "ok   a newer version keeps latest"
    else
        echo "FAIL a newer version did not keep latest"
        failures=$((failures + 1))
    fi
    # Component order, not lexical: 0.9.0 is older than 0.10.0.
    if [[ "$(dist_tag_for 0.9.0 0.10.0 latest)" == "backfill" ]]; then
        echo "ok   the comparison is numeric, not lexical (0.9.0 vs 0.10.0)"
    else
        echo "FAIL the comparison went lexical (0.9.0 vs 0.10.0)"
        failures=$((failures + 1))
    fi
    # A package with no `latest` yet (first publish) must take it.
    if [[ "$(dist_tag_for 1.0.0 "" latest)" == "latest" ]]; then
        echo "ok   a first publish takes latest"
    else
        echo "FAIL a first publish did not take latest"
        failures=$((failures + 1))
    fi
    # Equal is not older; the already-published check handles that version.
    if [[ "$(dist_tag_for 1.2.0 1.2.0 latest)" == "latest" ]]; then
        echo "ok   an equal version keeps latest"
    else
        echo "FAIL an equal version was rewritten"
        failures=$((failures + 1))
    fi
    # A requested non-latest tag is left alone, whatever the registry has.
    if [[ "$(dist_tag_for 1.1.0 1.2.0 next)" == "next" ]]; then
        echo "ok   an explicit non-latest tag is left alone"
    else
        echo "FAIL an explicit non-latest tag was rewritten"
        failures=$((failures + 1))
    fi

    # `registry_latest_version` must FAIL CLOSED. `curl -sS -L` exits 0 on an
    # HTTP error, so a 5xx error body has no `dist-tags` and, before this, read
    # as "first publish" — sending an older version to `latest`. A stub `curl`
    # drives each status without touching the network.
    local stub got
    stub="$(mktemp -d)"
    cat >"$stub/curl" <<'STUB'
#!/usr/bin/env bash
printf '%s' "${STUB_BODY:-}"
printf '\n%s' "${STUB_CODE:-200}"
STUB
    chmod +x "$stub/curl"
    export PATH="$stub:$PATH"
    export STUB_CODE STUB_BODY

    STUB_CODE=500 STUB_BODY='{"error":"upstream boom"}'
    if (registry_latest_version do-harness) >/dev/null 2>&1; then
        echo "FAIL an HTTP 500 packument did not fail closed"
        failures=$((failures + 1))
    else
        echo "ok   an HTTP 500 packument fails closed instead of taking latest"
    fi
    STUB_CODE=200 STUB_BODY='{"name":"do-harness"}'
    if (registry_latest_version do-harness) >/dev/null 2>&1; then
        echo "FAIL a packument with no dist-tags did not fail closed"
        failures=$((failures + 1))
    else
        echo "ok   a packument with no dist-tags fails closed"
    fi
    STUB_CODE=200 STUB_BODY='{"dist-tags":{"latest":"1.2.0-rc.1"}}'
    if (registry_latest_version do-harness) >/dev/null 2>&1; then
        echo "FAIL a prerelease latest was accepted"
        failures=$((failures + 1))
    else
        echo "ok   a prerelease latest is refused before version_lt"
    fi
    # A 404 is the one "not published yet" answer, and it yields empty so the
    # first publish legitimately takes `latest`.
    STUB_CODE=404 STUB_BODY='{"error":"Not found"}'
    if got="$(registry_latest_version do-harness)" && [[ -z "$got" ]]; then
        echo "ok   a 404 (not published yet) yields empty, so latest is taken"
    else
        echo "FAIL a 404 did not yield empty (got '$got')"
        failures=$((failures + 1))
    fi
    # The happy path still parses the published latest out of the packument.
    STUB_CODE=200 STUB_BODY='{"name":"do-harness","dist-tags":{"latest":"1.2.0","next":"2.0.0"}}'
    if [[ "$(registry_latest_version do-harness)" == "1.2.0" ]]; then
        echo "ok   the published latest is parsed from the packument"
    else
        echo "FAIL the packument latest was not parsed"
        failures=$((failures + 1))
    fi
    rm -rf "$stub"

    if ((failures > 0)); then
        echo "publish-npm.sh: $failures self-test failure(s)" >&2
        return 1
    fi
    echo "publish-npm.sh self-test OK (version anchoring, backfill for older versions, explicit tags preserved, registry status handled fail-closed)"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dist)
            DIST="${2:?--dist requires a value}"
            shift 2
            ;;
        --version)
            VERSION="${2:?--version requires a value}"
            shift 2
            ;;
        --tag)
            TAG="${2:?--tag requires a value}"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --otp)
            OTP="${2:?--otp requires a value}"
            shift 2
            ;;
        --self-test)
            SELF_TEST=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            die "unknown argument: $1 (see --help)"
            ;;
    esac
done

# Hermetic controls run before anything touches --dist, Cargo.toml, the network,
# or npm.
if ((SELF_TEST)); then
    self_test || exit 1
    exit 0
fi

[[ -n "$DIST" ]] || die "--dist is required"
[[ -d "$DIST" ]] || die "dist directory not found: $DIST"
if [[ -z "$VERSION" ]]; then
    VERSION="$(sed -n '/^\[workspace.package\]/,/^\[/ s/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml")"
fi
VERSION="${VERSION#v}"
valid_version "$VERSION" || die "invalid version: $VERSION"

if (( ! DRY_RUN )); then
    TOKEN="${NODE_AUTH_TOKEN:-${NPM_TOKEN:-}}"
    # A bare `setup-node` placeholder is not a credential; ignore it so the OIDC
    # branch below is reachable. Also export it as empty, so the temp `.npmrc`
    # that setup-node generated (`_authToken=${NODE_AUTH_TOKEN}`) does not
    # resolve to the literal dummy string and get sent as Bearer auth.
    if [[ "$TOKEN" == "$NODE_AUTH_TOKEN_PLACEHOLDER" ]]; then
        echo "Ignoring the setup-node placeholder NODE_AUTH_TOKEN; using OIDC"
        TOKEN=""
        export NODE_AUTH_TOKEN=""
    fi
    if [[ -n "$TOKEN" ]]; then
        export NODE_AUTH_TOKEN="$TOKEN"
    elif [[ -n "${ACTIONS_ID_TOKEN_REQUEST_URL:-}" &&
        -n "${ACTIONS_ID_TOKEN_REQUEST_TOKEN:-}" ]]; then
        # npm 11.5.1+ exchanges the GitHub Actions OIDC token during publish.
        # `npm whoami` cannot validate this mode before the publish operation.
        echo "Using GitHub Actions OIDC trusted publishing"
    elif ! npm whoami >/dev/null 2>&1; then
        die "not authenticated: configure npm trusted publishing or set NODE_AUTH_TOKEN"
    fi
fi

# target triple : platform dir : npm package : archive : binary
TARGETS=(
    "x86_64-unknown-linux-musl:linux-x64:do-harness-linux-x64:tar.gz:do-harness"
    "aarch64-unknown-linux-musl:linux-arm64:do-harness-linux-arm64:tar.gz:do-harness"
    "x86_64-apple-darwin:darwin-x64:do-harness-darwin-x64:tar.gz:do-harness"
    "aarch64-apple-darwin:darwin-arm64:do-harness-darwin-arm64:tar.gz:do-harness"
    "x86_64-pc-windows-msvc:win32-x64:do-harness-win32-x64:zip:do-harness.exe"
)

# Platform packages the npm registry refuses are listed in UNAVAILABLE_PKGS.
# They are skipped with an explicit log line instead of aborting the run: the
# 403 previously killed the loop, so every tag push reported a red npm-publish
# job that looked like a release failure while actually being the documented
# decision. The meta package still publishes — its pin on an unavailable
# package is filtered by `os`/`cpu` before npm fetches anything, so it is inert
# on the platforms that do have a package. See plans/distribution-epic.md.
#
# Keep in sync with `UNAVAILABLE_PACKAGES` in integrations/npm/lib/platform.js
# (the shim uses it for run-time guidance) and docs/releasing.md.
UNAVAILABLE_PKGS=(
    "do-harness-win32-x64"
)

is_unavailable() {
    local pkg="$1" entry
    for entry in "${UNAVAILABLE_PKGS[@]}"; do
        [[ "$entry" == "$pkg" ]] && return 0
    done
    return 1
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
package_version_exists() {
    local pkg="$1" version="$2"
    local registry="${NPM_CONFIG_REGISTRY:-https://registry.npmjs.org}"
    local status
    status="$(curl -sS --retry 2 --retry-delay 1 -L \
        -o /dev/null -w '%{http_code}' "${registry%/}/$pkg/$version")" || {
        die "could not query npm registry for $pkg@$version"
    }
    case "$status" in
        200)
            return 0
            ;;
        404)
            return 1
            ;;
        *)
            die "npm registry returned HTTP $status for $pkg@$version"
            ;;
    esac
}
publish_dir() {
    local dir="$1" pkg="$2"
    if package_version_exists "$pkg" "$VERSION"; then
        echo "$pkg@$VERSION is already on npm; skipping"
        return 0
    fi
    # Decide the dist-tag before publishing: with an explicit `latest`, an older
    # version must not move the registry's `latest` pointer backwards.
    local tag="$TAG"
    if [[ "$TAG" == "latest" ]]; then
        local registry_latest
        registry_latest="$(registry_latest_version "$pkg")"
        tag="$(dist_tag_for "$VERSION" "$registry_latest" "$TAG")"
        if [[ "$tag" != "$TAG" ]]; then
            echo "$pkg: $VERSION is older than the registry's latest ($registry_latest); publishing with --tag $tag so latest is not moved backwards"
        fi
    fi
    local log="$TMP/publish-$pkg.log"
    local -a cmd=(npm publish --tag "$tag")
    (( DRY_RUN )) && cmd=(npm publish --dry-run --tag "$tag")
    [[ -n "$OTP" ]] && cmd=(npm publish --tag "$tag" --otp "$OTP")
    # `if !` keeps errexit out of the way, and the log replays npm's output
    # unchanged on both paths.
    if ! (cd "$dir" && "${cmd[@]}") >"$log" 2>&1; then
        cat "$log"
        # npm's answer when the trusted publisher is configured for staged
        # publishing only (the default for connections created after
        # 2026-09-03) reads like a credential problem but is a missing
        # permission on the action. Name the remedy here: the failure surfaces
        # in a tag run, where the operator has the least context.
        if grep -q "OIDC permission denied" "$log"; then
            die "npm denied the OIDC publish of $pkg@$VERSION: its trusted publisher is not granted direct publishing.
Grant it per package (interactive 2FA), then re-run this job; publishing is idempotent:
  npm trust github $pkg --file release.yml --repo ${GITHUB_REPOSITORY:-d-o-hub/do-harness} --allow-publish
See docs/releasing.md (npm publishing)."
        fi
        die "npm publish failed for $pkg@$VERSION (see the output above)"
    fi
    cat "$log"
}

for entry in "${TARGETS[@]}"; do
    IFS=: read -r target platform pkg archive binary <<<"$entry"
    if is_unavailable "$pkg"; then
        echo "$pkg is unavailable on npm (registry name rejection); Windows ships as a GitHub release zip"
        continue
    fi
    stage="$TMP/$pkg"
    mkdir -p "$stage/bin"
    cp "$NPM_DIR/platforms/$platform/package.json" "$stage/package.json"
    if [[ "$archive" == "zip" ]]; then
        artifact="$DIST/do-harness-v${VERSION}-${target}.zip"
        [[ -f "$artifact" ]] || die "missing release artifact: $artifact"
        unzip -p "$artifact" "do-harness-v${VERSION}-${target}/${binary}" \
            >"$stage/bin/${binary}"
    else
        artifact="$DIST/do-harness-v${VERSION}-${target}.tar.gz"
        [[ -f "$artifact" ]] || die "missing release artifact: $artifact"
        tar -xzf "$artifact" -C "$TMP" "do-harness-v${VERSION}-${target}/${binary}"
        cp "$TMP/do-harness-v${VERSION}-${target}/${binary}" "$stage/bin/${binary}"
    fi
    chmod 0755 "$stage/bin/${binary}"
    (cd "$stage" && npm pkg set version="$VERSION" >/dev/null)
    publish_dir "$stage" "$pkg"
done

stage="$TMP/do-harness"
mkdir -p "$stage"
cp -R "$NPM_DIR/bin" "$NPM_DIR/lib" "$stage/"
cp "$NPM_DIR/package.json" "$NPM_DIR/README.md" "$stage/"
if [[ -f "$NPM_DIR/LICENSE" ]]; then
    cp -L "$NPM_DIR/LICENSE" "$stage/LICENSE"
fi
(
    cd "$stage"
    npm pkg set version="$VERSION" \
        "optionalDependencies.do-harness-linux-x64=$VERSION" \
        "optionalDependencies.do-harness-linux-arm64=$VERSION" \
        "optionalDependencies.do-harness-darwin-x64=$VERSION" \
        "optionalDependencies.do-harness-darwin-arm64=$VERSION" \
        "optionalDependencies.do-harness-win32-x64=$VERSION" >/dev/null
)

# The meta package pins every platform package, including ones npm refuses.
# Those pins are *inert* where they cannot resolve: npm filters an optional
# dependency by its `os`/`cpu` before fetching it, so a Linux/macOS install
# never touches the Windows pin (verified: clean install and clean `npm ls`),
# and on Windows the absent package is skipped silently and the shim exits with
# release-zip guidance. Keeping the pin means the moment npm clears the name,
# Windows users get the package with no further release change.
#
# So the meta package publishes even while a pinned platform package is
# unavailable — withholding it broke the documented primary install path
# (`npx do-harness`) for every Linux/macOS user. `UNAVAILABLE_PKGS` therefore
# affects only which platform packages are uploaded.
publish_dir "$stage" "do-harness"

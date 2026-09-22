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
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NPM_DIR="$ROOT/integrations/npm"

VERSION=""
DIST=""
DRY_RUN=0
TAG="latest"
OTP=""

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
  -h, --help         Show this help.
EOF
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
        -h | --help)
            usage
            exit 0
            ;;
        *)
            die "unknown argument: $1 (see --help)"
            ;;
    esac
done

[[ -n "$DIST" ]] || die "--dist is required"
[[ -d "$DIST" ]] || die "dist directory not found: $DIST"
if [[ -z "$VERSION" ]]; then
    VERSION="$(sed -n '/^\[workspace.package\]/,/^\[/ s/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml")"
fi
VERSION="${VERSION#v}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+ ]] || die "invalid version: $VERSION"

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
    # `--retry` alone does not retry a connection reset ("Recv failure:
    # Connection reset by peer"), which reddened the verify job on main once;
    # `--retry-all-errors` covers it, while an HTTP 404 stays a definitive
    # answer because it is a status, not an error (no `-f`).
    status="$(curl -sS --retry 3 --retry-delay 2 --retry-all-errors -L \
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
    if (( DRY_RUN )); then
        (cd "$dir" && npm publish --dry-run --tag "$TAG")
    elif [[ -n "$OTP" ]]; then
        (cd "$dir" && npm publish --tag "$TAG" --otp "$OTP")
    else
        (cd "$dir" && npm publish --tag "$TAG")
    fi
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

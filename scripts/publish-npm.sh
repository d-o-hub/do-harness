#!/usr/bin/env bash
# publish-npm.sh — assemble and publish the npm wrapper packages.
#
# The meta package `do-harness` depends on four platform packages; each
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
    [[ -n "$TOKEN" ]] || die "NODE_AUTH_TOKEN or NPM_TOKEN is not configured"
    export NODE_AUTH_TOKEN="$TOKEN"
fi

# target triple : platform package directory : npm package name
TARGETS=(
    "x86_64-unknown-linux-musl:linux-x64:do-harness-linux-x64"
    "aarch64-unknown-linux-musl:linux-arm64:do-harness-linux-arm64"
    "x86_64-apple-darwin:darwin-x64:do-harness-darwin-x64"
    "aarch64-apple-darwin:darwin-arm64:do-harness-darwin-arm64"
)

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

publish_dir() {
    local dir="$1" pkg="$2"
    if (( ! DRY_RUN )) && npm view "$pkg@$VERSION" version >/dev/null 2>&1; then
        echo "$pkg@$VERSION is already on npm; skipping"
        return 0
    fi
    if (( DRY_RUN )); then
        (cd "$dir" && npm publish --dry-run --tag "$TAG")
    else
        (cd "$dir" && npm publish --tag "$TAG")
    fi
}

for entry in "${TARGETS[@]}"; do
    IFS=: read -r target platform pkg <<<"$entry"
    tarball="$DIST/do-harness-v${VERSION}-${target}.tar.gz"
    [[ -f "$tarball" ]] || die "missing release artifact: $tarball"
    stage="$TMP/$pkg"
    mkdir -p "$stage/bin"
    cp "$NPM_DIR/platforms/$platform/package.json" "$stage/package.json"
    tar -xzf "$tarball" -C "$TMP" "do-harness-v${VERSION}-${target}/do-harness"
    cp "$TMP/do-harness-v${VERSION}-${target}/do-harness" "$stage/bin/do-harness"
    chmod 0755 "$stage/bin/do-harness"
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
        "optionalDependencies.do-harness-darwin-arm64=$VERSION" >/dev/null
)
publish_dir "$stage" "do-harness"

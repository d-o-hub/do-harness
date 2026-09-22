#!/bin/sh
# install.sh — install a prebuilt do-harness release binary.
#
# POSIX sh on purpose: the documented invocation pipes this script into `sh`,
# which is dash on Debian/Ubuntu. Do not use bashisms ([[ ]], local, pipefail).

# Reproducibility: pin DO_HARNESS_VERSION (or --version). Without a pin the
# latest release tag is resolved from the GitHub `releases/latest` redirect
# (no API call, no rate limit).
#
# Integrity: the SHA-256 checksum comes from `checksums.txt` on the same
# origin as the artifact, so it detects corruption and truncated downloads,
# not a compromised release origin. For a stronger guarantee, verify the
# checksums out of band before installing.
# pipefail is deliberately absent (POSIX): the only pipes feed awk from the
# checksum tools, and a failed checksum tool yields an empty digest that the
# explicit mismatch guard below rejects.
set -eu

REPO="${DO_HARNESS_REPO:-d-o-hub/do-harness}"
DEFAULT_BASE_URL="https://github.com/${REPO}/releases/download"
LATEST_URL="https://github.com/${REPO}/releases/latest"

BASE_URL="${DO_HARNESS_BASE_URL:-$DEFAULT_BASE_URL}"
VERSION="${DO_HARNESS_VERSION:-}"
BIN_DIR="${DO_HARNESS_INSTALL_DIR:-$HOME/.local/bin}"

usage() {
    cat <<'EOF'
Install a prebuilt do-harness release binary.

Usage: install.sh [OPTIONS]

Options:
  --version <TAG>    Release tag to install (e.g. v0.1.2). Default: latest.
  --bin-dir <DIR>    Install directory. Default: $HOME/.local/bin.
  --base-url <URL>   Release download base URL (mirrors, tests).
  -h, --help         Show this help.

Environment:
  DO_HARNESS_VERSION      Same as --version.
  DO_HARNESS_INSTALL_DIR  Same as --bin-dir.
  DO_HARNESS_BASE_URL     Same as --base-url.
  DO_HARNESS_REPO         GitHub owner/repo for latest resolution.
  DO_HARNESS_TARGET       Override the detected release target (tests).
EOF
}

die() {
    echo "install.sh: $*" >&2
    exit 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --version)
            VERSION="${2:?--version requires a value}"
            shift 2
            ;;
        --bin-dir)
            BIN_DIR="${2:?--bin-dir requires a value}"
            shift 2
            ;;
        --base-url)
            BASE_URL="${2:?--base-url requires a value}"
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

detect_target() {
    uname_os="$(uname -s)"
    uname_arch="$(uname -m)"
    case "$uname_os" in
        Linux)
            case "$uname_arch" in
                x86_64 | amd64) echo "x86_64-unknown-linux-musl" ;;
                aarch64 | arm64) echo "aarch64-unknown-linux-musl" ;;
                *) return 1 ;;
            esac
            ;;
        Darwin)
            case "$uname_arch" in
                x86_64) echo "x86_64-apple-darwin" ;;
                arm64 | aarch64) echo "aarch64-apple-darwin" ;;
                *) return 1 ;;
            esac
            ;;
        MINGW* | MSYS* | CYGWIN* | Windows_NT)
            case "$uname_arch" in
                x86_64 | amd64) echo "x86_64-pc-windows-msvc" ;;
                *) return 1 ;;
            esac
            ;;
        *) return 1 ;;
    esac
}

resolve_latest() {
    latest_url="$(curl -fsSL -o /dev/null -w '%{url_effective}' "$LATEST_URL")" ||
        die "could not resolve the latest release from ${LATEST_URL}"
    latest_tag="${latest_url##*/}"
    case "$latest_tag" in
        v*) ;;
        *) die "unexpected latest release URL: ${latest_url}" ;;
    esac
    printf '%s\n' "$latest_tag"
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        die "neither sha256sum nor shasum is available for checksum verification"
    fi
}

command -v curl >/dev/null 2>&1 || die "curl is required to download releases"

if [ -n "${DO_HARNESS_TARGET:-}" ]; then
    target="$DO_HARNESS_TARGET"
else
    target="$(detect_target)" ||
        die "unsupported platform $(uname -s)/$(uname -m); install from source with 'cargo install --path crates/do-harness'"
fi

if [ -z "$VERSION" ]; then
    VERSION="$(resolve_latest)"
fi
case "$VERSION" in
    v*) ;;
    *) VERSION="v${VERSION}" ;;
esac

case "$target" in
    x86_64-pc-windows-msvc) archive_ext="zip"; bin_name="do-harness.exe" ;;
    *) archive_ext="tar.gz"; bin_name="do-harness" ;;
esac
asset="do-harness-${VERSION}-${target}.${archive_ext}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

curl -fsSL -o "$tmp/$asset" "${BASE_URL}/${VERSION}/${asset}" ||
    die "download failed: ${BASE_URL}/${VERSION}/${asset}"
curl -fsSL -o "$tmp/checksums.txt" "${BASE_URL}/${VERSION}/checksums.txt" ||
    die "download failed: ${BASE_URL}/${VERSION}/checksums.txt"

expected="$(awk -v name="$asset" '$2 == name { print $1 }' "$tmp/checksums.txt")"
[ -n "$expected" ] || die "${asset} is not listed in checksums.txt"
actual="$(sha256_of "$tmp/$asset")"
[ "$actual" = "$expected" ] ||
    die "checksum mismatch for ${asset} (expected ${expected}, got ${actual})"

if [ "$archive_ext" = "zip" ]; then
    if command -v unzip >/dev/null 2>&1; then
        unzip -q "$tmp/$asset" -d "$tmp" ||
            die "failed to extract ${asset} (unzip)"
    elif command -v 7z >/dev/null 2>&1; then
        7z x "-o$tmp" "$tmp/$asset" >/dev/null ||
            die "failed to extract ${asset} (7z)"
    else
        die "neither unzip nor 7z is available to extract ${asset}"
    fi
else
    tar -xzf "$tmp/$asset" -C "$tmp"
fi
src="$tmp/do-harness-${VERSION}-${target}/${bin_name}"
[ -f "$src" ] || die "archive ${asset} did not contain ${bin_name}"

mkdir -p "$BIN_DIR"
install -m 0755 "$src" "$BIN_DIR/$bin_name"

echo "Installed do-harness ${VERSION} (${target}) to ${BIN_DIR}/${bin_name}"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "Add it to PATH: export PATH=\"${BIN_DIR}:\$PATH\"" ;;
esac
"$BIN_DIR/$bin_name" version

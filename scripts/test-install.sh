#!/usr/bin/env bash
# test-install.sh — end-to-end test for scripts/install.sh.
#
# Packages a locally built binary as a release artifact in a temp directory,
# serves it over file://, and asserts the installer verifies, extracts, and
# installs it. Also asserts a tampered artifact is rejected before install.
#
# Usage: scripts/test-install.sh [--bin PATH] [--version vX.Y.Z]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${ROOT}/target/release/do-harness"
VERSION=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --bin)
            BIN="${2:?--bin requires a value}"
            shift 2
            ;;
        --version)
            VERSION="${2:?--version requires a value}"
            shift 2
            ;;
        *)
            echo "test-install.sh: unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

[[ -x "$BIN" ]] || {
    echo "FAIL: binary not found or not executable: $BIN" >&2
    exit 1
}

if [[ -z "$VERSION" ]]; then
    VERSION="v$(sed -n '/^\[workspace.package\]/,/^\[/ s/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml")"
fi
[[ "$VERSION" == v* ]] || VERSION="v${VERSION}"

case "$(uname -s)/$(uname -m)" in
    Linux/x86_64) target="x86_64-unknown-linux-musl" ;;
    Linux/aarch64) target="aarch64-unknown-linux-musl" ;;
    Darwin/x86_64) target="x86_64-apple-darwin" ;;
    Darwin/arm64) target="aarch64-apple-darwin" ;;
    *)
        echo "SKIP: unsupported host platform $(uname -s)/$(uname -m)"
        exit 0
        ;;
esac

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

dist="$tmp/dist/$VERSION"
base="file://$tmp/dist"
name="do-harness-${VERSION}-${target}"
mkdir -p "$dist" "$tmp/pkg/$name"
cp "$BIN" "$tmp/pkg/$name/do-harness"
tar -czf "$dist/${name}.tar.gz" -C "$tmp/pkg" "$name"
printf '%s  %s\n' "$(sha256_of "$dist/${name}.tar.gz")" "${name}.tar.gz" >"$dist/checksums.txt"

install_ok() {
    local bin_dir="$1" log="$2"
    bash "$ROOT/scripts/install.sh" --version "$VERSION" --base-url "$base" --bin-dir "$bin_dir" >"$log" 2>&1
}

if ! install_ok "$tmp/bin" "$tmp/install.log"; then
    cat "$tmp/install.log" >&2
    echo "FAIL: installer exited non-zero" >&2
    exit 1
fi
[[ -x "$tmp/bin/do-harness" ]] || {
    echo "FAIL: installer did not place an executable at $tmp/bin/do-harness" >&2
    exit 1
}
got="$("$tmp/bin/do-harness" version --format json | sed -n 's/.*"version": "\([^"]*\)".*/\1/p')"
[[ "v${got}" == "$VERSION" ]] || {
    echo "FAIL: installed version '${got}' does not match ${VERSION}" >&2
    exit 1
}

# Documented curl-pipeline contract: the README pipes the script into `sh`,
# which is dash on Debian/Ubuntu. Exercise that exact execution mode (script
# on stdin, arguments after -s --) to catch bashisms the file-mode run above
# cannot see.
pipe_ok() {
    local bin_dir="$1" log="$2"
    sh -s -- --version "$VERSION" --base-url "$base" --bin-dir "$bin_dir" <"$ROOT/scripts/install.sh" >"$log" 2>&1
}

if ! pipe_ok "$tmp/bin-piped" "$tmp/piped.log"; then
    cat "$tmp/piped.log" >&2
    echo "FAIL: piped (sh -s) installation exited non-zero" >&2
    exit 1
fi
[[ -x "$tmp/bin-piped/do-harness" ]] || {
    echo "FAIL: piped install did not place an executable at $tmp/bin-piped/do-harness" >&2
    exit 1
}

# Tampering with the artifact must fail checksum verification before install.
printf 'tamper' >>"$dist/${name}.tar.gz"
if install_ok "$tmp/bin-tampered" "$tmp/tamper.log"; then
    echo "FAIL: tampered artifact was accepted" >&2
    exit 1
fi
grep -q "checksum mismatch" "$tmp/tamper.log" || {
    cat "$tmp/tamper.log" >&2
    echo "FAIL: expected a checksum mismatch diagnostic" >&2
    exit 1
}
[[ ! -e "$tmp/bin-tampered/do-harness" ]] || {
    echo "FAIL: tampered artifact installed a binary" >&2
    exit 1
}

echo "test-install OK: verified, installed (bash file mode + sh pipeline mode), and rejected a tampered artifact"

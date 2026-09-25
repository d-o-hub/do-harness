#!/usr/bin/env bash
# check-release-preflight.sh — read-only release preflight for this repository.
#
# Sensor: scripts/check-release-preflight.sh (written by `do-harness init`)
#
# Two checks, both read-only. This script never tags, pushes, or publishes:
#   1. every version pin agrees — `VERSION`, `Cargo.toml`
#      (`[workspace.package]` first, then `[package]`), and every tracked
#      `package.json` that declares its own top-level version and is not
#      `"private": true` (`--no-package-json` drops the manifest pins when
#      they are stamped at publish time instead);
#   2. with `--release`, the target version is not already published as a
#      GitHub Release, and the latest release is printed for context.
#
# The default invocation never touches the network, so `feedback` and
# `verification` runs stay offline and deterministic; comparing against
# published releases is opt-in at release time.
#
# Degrading is loud but never flaky: when `gh` is missing, unauthenticated,
# or offline the comparison prints `WARN` and exits 0 — unless `CI=true` or
# `DO_HARNESS_REQUIRE_TOOLS=1`, where a skipped guard is a `FAIL` (the
# shipped rust-pack convention: fail open locally, fail closed on demand).
#
# Environment:
#   GH_REPO               `owner/name` to query (default: the `origin` remote)
#   RELEASE_PREFLIGHT_GH  `gh` binary to use (default: `gh`)
#
# Exit codes: 0 pass or degraded, 1 finding, 2 usage error.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

GH="${RELEASE_PREFLIGHT_GH:-gh}"

# Releases are listed in full so an already-published target is found even when
# it is older than the newest release; the cap only bounds the API page size.
RELEASE_TAGS_LIMIT=100

usage() {
    echo "usage: ${0} [--release] [--no-package-json]"
    echo "  --release          also fail when the target version is already published"
    echo "  --no-package-json  ignore package.json version pins"
}

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

degraded() {
    if require_tools; then
        echo "FAIL: release preflight could not compare against published releases ($1), and CI=true or DO_HARNESS_REQUIRE_TOOLS=1 requires it."
        exit 1
    fi
    echo "WARN: release comparison skipped: $1."
    exit 0
}

release=0
package_json=1
for arg in "$@"; do
    case "$arg" in
        --release) release=1 ;;
        --no-package-json) package_json=0 ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "FAIL: unknown argument '$arg'" >&2
            usage >&2
            exit 2
            ;;
    esac
done

# Version pins are collected as `path|value`; the first one is the target.
pins=()
add_pin() { pins+=("$1|$2"); }

# Prints the `version` value in a Cargo.toml: `[workspace.package]` wins over
# `[package]`, and an inherited `version.workspace = true` never matches.
cargo_toml_version() {
    awk '
        /^\[workspace\.package\]/ { section = "workspace"; next }
        /^\[package\]/            { section = "package";   next }
        /^\[/                     { section = "";          next }
        section == "workspace" && /^[[:space:]]*version[[:space:]]*=/ { workspace = $0 }
        section == "package" && /^[[:space:]]*version[[:space:]]*=/ && package_version == "" { package_version = $0 }
        END { print (workspace != "" ? workspace : package_version) }
    ' "$1" | sed -E 's/^[^=]*=[[:space:]]*//' | tr -d '[:space:]"'
}

# Every package.json the repository tracks, one path per line. Falls back to a
# pruned walk outside a git work tree (the script must work in either).
tracked_package_json() {
    if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        git ls-files -z -- '*package.json' | tr '\0' '\n'
    else
        find . -name package.json -not -path './.git/*' -not -path '*/node_modules/*' \
            -not -path '*/target/*' -not -path '*/dist/*' -print | sed 's#^\./##'
    fi
}

# Prints a JSON manifest's own top-level `version` string and `private`
# literal, tab-separated. Only keys at brace depth 1 count: a dependency,
# override, or `publishConfig` sub-object carries its own `version` key, and
# treating that as a release pin would fail a correct repository.
json_toplevel_fields() {
    awk '
        function flush() {
            if (capture != "" && depth == 1) {
                if (capture == "version") { out_version = buf } else { out_private = buf }
            }
            capture = ""
            buf = ""
        }
        {
            line = $0
            for (i = 1; i <= length(line); i++) {
                c = substr(line, i, 1)
                if (in_str) {
                    if (esc) { buf = buf c; esc = 0; continue }
                    if (c == "\\") { esc = 1; continue }
                    if (c == "\"") {
                        in_str = 0
                        if (is_key) {
                            key = buf
                            buf = ""
                            if (depth == 1 && (key == "version" || key == "private")) { capture = key }
                        } else {
                            flush()
                        }
                        continue
                    }
                    buf = buf c
                    continue
                }
                if (c == "\"") { in_str = 1; buf = ""; is_key = expect_key; expect_key = 0; continue }
                if (c == "{" || c == "[") { flush(); depth++; if (depth == 1) { expect_key = 1 } continue }
                if (c == "}" || c == "]") { flush(); depth--; continue }
                if (c == ",") { flush(); if (depth == 1) { expect_key = 1 } continue }
                if (c == ":") { continue }
                if (capture != "") { buf = buf c }
            }
        }
        END { printf "%s\t%s\n", out_version, out_private }
    ' "$1"
}

if [[ -f VERSION ]]; then
    add_pin "VERSION" "$(head -n 1 VERSION | tr -d '[:space:]' | sed 's/^v//')"
fi
if [[ -f Cargo.toml ]]; then
    cargo_version="$(cargo_toml_version Cargo.toml)"
    if [[ -n "$cargo_version" ]]; then
        add_pin "Cargo.toml" "$cargo_version"
    fi
fi
if [[ "$package_json" -eq 1 ]]; then
    while IFS= read -r manifest; do
        [[ -n "$manifest" ]] || continue
        IFS=$'\t' read -r manifest_version manifest_private < <(json_toplevel_fields "$manifest")
        # Private packages are free to carry a placeholder version (npm
        # workspaces commonly park them at 0.0.0), so they are not release pins.
        if [[ "$(printf '%s' "${manifest_private:-}" | tr -d '[:space:]')" == "true" ]]; then
            continue
        fi
        manifest_version="$(printf '%s' "${manifest_version:-}" | tr -d '[:space:]')"
        if [[ -n "$manifest_version" ]]; then
            add_pin "$manifest" "$manifest_version"
        fi
    done < <(tracked_package_json)
fi

if [[ "${#pins[@]}" -eq 0 ]]; then
    echo "WARN: no version pin found (VERSION, Cargo.toml, package.json); release preflight skipped."
    exit 0
fi

target="${pins[0]#*|}"
mismatch=0
for pin in "${pins[@]}"; do
    pin_path="${pin%%|*}"
    pin_value="${pin#*|}"
    if [[ "$pin_value" != "$target" ]]; then
        echo "FAIL: $pin_path declares $pin_value, but the target version is $target."
        mismatch=1
    fi
done
if [[ "$mismatch" -eq 1 ]]; then
    echo "FIX: bump every pin in one release-prep PR so they all read $target, then re-run this preflight."
    exit 1
fi
echo "OK: ${#pins[@]} version pin(s) agree at $target."

if [[ "$release" -eq 0 ]]; then
    echo "note: release comparison skipped (offline); run with --release before tagging or dispatching a release."
    exit 0
fi

if ! command -v "$GH" >/dev/null 2>&1; then
    degraded "$GH is not installed"
fi

repo="${GH_REPO:-}"
if [[ -z "$repo" ]]; then
    if url="$(git remote get-url origin 2>/dev/null)"; then
        repo="$(printf '%s' "$url" | sed -E 's#^git@github\.com:##; s#^https?://github\.com/##; s#\.git$##')"
    fi
fi
if [[ -z "$repo" ]]; then
    degraded "cannot determine the GitHub repository (set GH_REPO or add an origin remote)"
fi

if ! raw="$("$GH" release list --repo "$repo" --limit "$RELEASE_TAGS_LIMIT" --json tagName 2>/dev/null)"; then
    degraded "$GH could not list releases for $repo (no network, or not authenticated)"
fi
tags="$(printf '%s' "$raw" | grep -o '"tagName":"[^"]*"' | sed -E 's/^"tagName":"//; s/"$//' || true)"
latest="$(printf '%s\n' "$tags" | head -n 1)"

while IFS= read -r tag; do
    [[ -n "$tag" ]] || continue
    if [[ "${tag#v}" == "$target" ]]; then
        echo "FAIL: $repo already has a GitHub Release for $target ($tag)."
        echo "FIX: bump every version pin in a release-prep PR, merge it, then re-run this preflight before tagging or dispatching."
        exit 1
    fi
done <<< "$tags"

if [[ -z "$latest" ]]; then
    echo "OK: $repo has no published releases; $target is unreleased."
else
    echo "OK: $target is not published yet (latest $repo release: $latest)."
fi

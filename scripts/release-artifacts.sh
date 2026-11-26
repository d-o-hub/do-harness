#!/usr/bin/env bash
# release-artifacts.sh — read the independently-versioned artifact versions.
#
# Source it from release-notes.sh; do not execute it (it defines functions and
# nothing else):
#
#   # shellcheck disable=SC1091 # resolved at runtime from this script's directory
#   source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/release-artifacts.sh"
#
# The artifact versions live in the version files documented in
# docs/versioning.md (the adoption contract's `contract_version`, each sensor
# pack manifest's `version`, and the guides' frontmatter `version`). A version
# file that is absent from the tree reads as empty; callers label what empty
# means (`not declared` in the release body rather than a guessed value).
set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

# toml_string_value <file> <key> — the `<key> = "…"` value in a TOML file.
# Empty when the file or key is absent; the caller labels what empty means.
toml_string_value() {
    sed -n "s/^${2} *= *\"\([^\"]*\)\"/\1/p" "$1" | tail -n 1
}

# frontmatter_version <file> — the `version:` value in YAML frontmatter.
# Empty when the file is absent or has no frontmatter version.
frontmatter_version() {
    awk '
        NR == 1 && $0 == "---" { fm = 1; next }
        fm && $0 == "---" { exit }
        fm && /^version:/ {
            sub(/^version:[[:space:]]*/, "")
            gsub(/"/, "")
            if ($0 != "") print
            exit
        }
    ' "$1"
}

# artifact_version_table <tag> — one markdown row per independently-versioned
# artifact, read from the tree the release is cut from. A version file absent
# from that tree (a release predating multi-artifact versioning) prints
# `not declared` — an honest record rather than a guessed value.
artifact_version_table() {
    local tag="$1" version
    version="$(dh_workspace_version | head -n 1)"
    printf '| CLI (binary, release tag) | %s |\n' "$tag"
    version="$(toml_string_value "$ROOT/do-harness.toml" contract_version)"
    printf '| Adoption contract (do-harness.toml) | %s |\n' "${version:-not declared}"
    local manifest
    for manifest in "$ROOT"/crates/do-harness/templates/packs/*/manifest.toml; do
        [[ -f "$manifest" ]] || continue
        local pack
        pack="$(toml_string_value "$manifest" name)"
        version="$(toml_string_value "$manifest" version)"
        printf '| Sensor pack: %s | %s |\n' "${pack:-?}" "${version:-not declared}"
    done
    local guide label
    for pair in \
        "$ROOT/AGENTS.md:AGENTS.md (root)" \
        "$ROOT/crates/do-harness/templates/AGENTS.md:AGENTS.md (template)" \
        "$ROOT/crates/do-harness/templates/USE_RESTRICTIONS.md:USE_RESTRICTIONS.md"; do
        guide="${pair%%:*}"
        label="${pair#*:}"
        version="$(frontmatter_version "$guide")"
        printf '| Guide: %s | %s |\n' "$label" "${version:-not declared}"
    done
}

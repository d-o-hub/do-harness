#!/usr/bin/env bash
# check-package-contract.sh — verifies package contents and publish constraints for workspace crates.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAIL=0

if ! command -v cargo >/dev/null 2>&1; then
    if [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; then
        echo "FAIL: cargo is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    else
        echo "SKIP: cargo unavailable; skipping package contract check."
        exit 0
    fi
fi

# 1. Verify guardian-proxy remains publish = false
PROXY_MANIFEST="$ROOT/crates/guardian-proxy/Cargo.toml"
if ! grep -qE '^publish *= *false' "$PROXY_MANIFEST"; then
    echo "FAIL: crates/guardian-proxy/Cargo.toml must specify publish = false"
    FAIL=1
fi

check_crate_package() {
    local crate="$1"
    shift
    local required_files=("$@")

    echo "Checking package contents for $crate..."
    local list
    if ! list="$(cd "$ROOT" && cargo package --locked --allow-dirty --list -p "$crate" 2>/dev/null)"; then
        echo "FAIL: cargo package --locked --allow-dirty --list -p $crate failed"
        FAIL=1
        return
    fi

    # Require declared files
    for req in "${required_files[@]}"; do
        if ! echo "$list" | grep -qxF "$req"; then
            echo "FAIL: $crate package is missing required file: $req"
            FAIL=1
        fi
    done

    # Reject accidental inclusion of local state, target output, secrets
    local forbidden_regex='(\.do-harness/|\.git/|\.env|target/|secrets)'
    if echo "$list" | grep -qE "$forbidden_regex"; then
        echo "FAIL: $crate package contains forbidden files:"
        echo "$list" | grep -E "$forbidden_regex"
        FAIL=1
    fi
}

check_crate_package do-harness-types LICENSE
check_crate_package do-harness-db LICENSE
check_crate_package do-harness LICENSE README.md assets/compliance.md assets/methods.json assets/nextest.toml templates/AGENTS.md

# 2. npm wrapper manifests: the publish job stamps the workspace version into
#    the staged copies at release time, so the checked-in files must already
#    agree with the workspace version and with each other. A mismatch means a
#    release commit bumped some manifests and not others, and in-tree readers
#    (or local fixtures) pin a version no release ever shipped.
npm_report="$(python3 - "$ROOT" <<'PY'
import json
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1])
cargo = root / "Cargo.toml"
match = re.search(
    r'^\[workspace\.package\]$.*?^version\s*=\s*"([^"]+)"',
    cargo.read_text(encoding="utf-8"),
    re.M | re.S,
)
if not match:
    print("FAIL: cannot read [workspace.package] version from Cargo.toml")
    sys.exit(1)
version = match.group(1)
problems = []
meta_path = root / "integrations" / "npm" / "package.json"
meta = json.loads(meta_path.read_text(encoding="utf-8"))
if meta.get("version") != version:
    problems.append(
        f"{meta_path.relative_to(root)} version {meta.get('version')} != workspace {version}"
    )
for dep, pinned in meta.get("optionalDependencies", {}).items():
    if pinned != version:
        problems.append(
            f"{meta_path.relative_to(root)} pins {dep}={pinned} != workspace {version}"
        )
for path in sorted((root / "integrations" / "npm" / "platforms").glob("*/package.json")):
    pinned = json.loads(path.read_text(encoding="utf-8")).get("version")
    if pinned != version:
        problems.append(f"{path.relative_to(root)} version {pinned} != workspace {version}")
if problems:
    print("\n".join(f"FAIL: {problem}" for problem in problems))
    sys.exit(1)
print(f"npm manifests agree with workspace version {version}.")
PY
)" || FAIL=1
echo "$npm_report"

if (( FAIL )); then
    echo "FAIL: package contract check failed."
    exit 1
fi

echo "check-package-contract OK."

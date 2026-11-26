#!/usr/bin/env bash
# check-sdk-pack.sh — hermetic packaging validation for the SDK packages.
#
# Sensor: scripts/check-sdk-pack.sh
#
# Validates that both SDK packages build into publishable artifacts without
# touching any registry, credentials, or network:
#   - Python (integrations/sdk-python/): builds an sdist and a wheel into a
#     scratch directory (never in-tree), then verifies the wheel contains the
#     package modules.
#   - TypeScript (integrations/sdk-ts/): runs `npm pack --dry-run` and
#     verifies the packed manifest name/version and the shipped files.
#
# This is the offline half of the publish automation: the actual `pip
# publish` / `npm publish` steps need registry credentials and stay out of
# the repository (see docs/adoption.md, "SDK packages"). Exit code 0 means
# both packages are ready to publish from the current tree.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT


# --- Python: sdist + wheel from a scratch copy (no in-tree artifacts) ------
PY_DIR="$ROOT/integrations/sdk-python"
PY_BUILD="$TMP/pybuild"
PY_DIST="$TMP/pydist"
mkdir -p "$PY_BUILD" "$PY_DIST"
cp -R "$PY_DIR"/. "$PY_BUILD/"

# Build sdist + wheel directly through setuptools' PEP 517 backend: this
# needs only setuptools (no `build` orchestrator, no network).
python3 - "$PY_BUILD" "$PY_DIST" <<'PY' >/dev/null 2>&1
import os
import sys

from setuptools import build_meta

project, dist = sys.argv[1], sys.argv[2]
os.chdir(project)
build_meta.build_wheel(dist)
build_meta.build_sdist(dist)
PY
WHEEL="$(find "$PY_DIST" -maxdepth 1 -name '*.whl' -print -quit)"
SDIST="$(find "$PY_DIST" -maxdepth 1 -name '*.tar.gz' -print -quit)"
[ -n "$WHEEL" ] || { echo "FINDINGS: 1 (python wheel missing)"; exit 1; }
[ -n "$SDIST" ] || { echo "FINDINGS: 1 (python sdist missing)"; exit 1; }

python3 - "$WHEEL" <<'PY'
import sys
import zipfile

wheel = sys.argv[1]
with zipfile.ZipFile(wheel) as zf:
    names = zf.namelist()
for required in (
    "do_harness_sdk/__init__.py",
    "do_harness_sdk/client.py",
    "do_harness_sdk/models.py",
    "do_harness_sdk/langchain_tool.py",
):
    if required not in names:
        raise SystemExit(f"wheel missing module: {required}")
print(f"python wheel ok: {wheel.split('/')[-1]} ({len(names)} files)")
PY

# --- TypeScript: npm pack --dry-run ---------------------------------------

(
    cd "$ROOT/integrations/sdk-ts"
    npm pack --dry-run --json > "$TMP/npm.json"
)

node - "$TMP/npm.json" <<'JS'
const fs = require("fs");
const raw = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
// npm >= 12 prints the `--json` report keyed by package name; npm <= 11
// printed a one-element array. Accept both, so that a locally upgraded npm
// reads as a tooling difference rather than a packaging failure.
const packages = Array.isArray(raw) ? raw : Object.values(raw);
if (packages.length !== 1) {
  throw new Error(`pack result must contain exactly one package (got ${packages.length})`);
}
const [pkg] = packages;
if (pkg.name !== "do-harness-sdk") throw new Error(`unexpected package: ${pkg.name}`);
if (pkg.version !== "0.4.8") throw new Error(`unexpected version: ${pkg.version}`);
const paths = pkg.files.map((f) => f.path);
for (const file of ["lib/sdk.mjs", "index.d.ts", "README.md"]) {
  if (!paths.includes(file)) throw new Error(`packed file missing: ${file}`);
}
console.log(`npm pack ok: ${pkg.filename} (${pkg.size} bytes)`);
JS

echo "sdk pack validation passed"

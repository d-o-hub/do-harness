#!/usr/bin/env bash
# ci_regression_matrix_test.sh — signature-match test for plans/regression-matrix.json
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
matrix_path="$repo_root/plans/regression-matrix.json"
fixtures_dir="$repo_root/tests/fixtures/regression-matrix"
clean_fixture="$fixtures_dir/clean.log"

if [[ ! -f "$matrix_path" ]]; then
  echo "FAIL: $matrix_path does not exist" >&2
  exit 1
fi

if [[ ! -f "$clean_fixture" ]]; then
  echo "FAIL: $clean_fixture does not exist" >&2
  exit 1
fi

python3 - "$matrix_path" "$fixtures_dir" "$clean_fixture" << 'EOF'
import json
import re
import sys
import os

matrix_path, fixtures_dir, clean_fixture = sys.argv[1], sys.argv[2], sys.argv[3]

with open(matrix_path, "r", encoding="utf-8") as f:
    matrix = json.load(f)

if not isinstance(matrix, list) or len(matrix) < 5:
    print(f"FAIL: expected at least 5 entries in regression-matrix.json, got {len(matrix)}", file=sys.stderr)
    sys.exit(1)

with open(clean_fixture, "r", encoding="utf-8") as f:
    clean_text = f.read()

failures = 0
for entry in matrix:
    entry_id = entry.get("id")
    symptom = entry.get("symptom")
    signature = entry.get("signature")
    fix_hint = entry.get("fix_hint")
    sensor = entry.get("sensor")

    if not all([entry_id, symptom, signature, fix_hint, sensor]):
        print(f"FAIL: entry missing required fields: {entry}", file=sys.stderr)
        failures += 1
        continue

    # 1. Positive match test against fixture log
    fixture_file = os.path.join(fixtures_dir, f"{entry_id}.log")
    if not os.path.isfile(fixture_file):
        print(f"FAIL [{entry_id}]: missing positive fixture file {fixture_file}", file=sys.stderr)
        failures += 1
        continue

    with open(fixture_file, "r", encoding="utf-8") as f:
        fixture_text = f.read()

    pattern = re.compile(signature)
    if not pattern.search(fixture_text):
        print(f"FAIL [{entry_id}]: signature '{signature}' did not match positive fixture {fixture_file}", file=sys.stderr)
        failures += 1
    else:
        print(f"OK [{entry_id}]: signature matched positive fixture")

    # 2. Negative match test against clean log
    if pattern.search(clean_text):
        print(f"FAIL [{entry_id}]: signature '{signature}' falsely matched clean fixture {clean_fixture}", file=sys.stderr)
        failures += 1

if failures > 0:
    print(f"ci_regression_matrix_test FAIL: {failures} assertion(s) failed", file=sys.stderr)
    sys.exit(1)

print("ci_regression_matrix_test OK: all signatures matched positive fixtures and rejected clean output")
EOF

#!/usr/bin/env bash
# check-invariants.sh — content guard for the policy record (plans/invariants.json).
#
# Sensor: scripts/check-invariants.sh
# Coverage:
#   - plans/invariants.json
#   - scripts/check-invariants.sh
#
# The DecisionHeader schema check runs only inside `do-harness seed`/`init`, and
# a structural JSON parse accepts any well-formed array, so a stale or
# hand-edited entry can sit here unnoticed: this is the record that tells every
# other decision where its sensor lives. This sensor enforces what the record
# itself promises:
#   1. plans/invariants.json parses as a JSON array of decision headers with
#      non-empty invariant/rationale/sensor/category strings.
#   2. Every `scripts/...` path referenced by a `sensor` field exists.
#   3. No two entries repeat the same `invariant` text.
#
# It deliberately does not re-derive whether a decision is still *true* — that
# is the owning epic's reviewed evidence, not something a hermetic script can
# read.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 - "$ROOT" <<'PY'
import json
import os
import re
import sys

root = sys.argv[1]
path = os.path.join(root, "plans", "invariants.json")
rel = os.path.relpath(path, root)

failures = []


def fail(message):
    failures.append(message)
    print(f"FAIL: {message}", file=sys.stderr)


try:
    with open(path, "r", encoding="utf-8") as handle:
        entries = json.load(handle)
except FileNotFoundError:
    fail(f"{rel} does not exist")
    entries = None
except json.JSONDecodeError as err:
    fail(f"{rel} is not valid JSON: {err}")
    entries = None

if entries is not None:
    if not isinstance(entries, list):
        fail(f"{rel} must be a JSON array of decision headers")
        entries = []

    required = ("invariant", "rationale", "sensor", "category")
    seen_invariants = {}
    # Full repository-relative path, not a `scripts/...` suffix of a longer
    # one: entries reference `scripts/*.sh` and `.agents/skills/**/scripts/*.sh`
    # alike, and both must resolve from the root.
    script_ref = re.compile(r"(?:[A-Za-z0-9_.-]+/)*scripts/[A-Za-z0-9_.-]+\.(?:sh|py)")

    for index, entry in enumerate(entries):
        if not isinstance(entry, dict):
            fail(f"{rel}[{index}] is not an object")
            continue
        for field in required:
            value = entry.get(field)
            if not isinstance(value, str) or not value.strip():
                fail(f"{rel}[{index}] has an empty or missing `{field}` field")
        invariant = entry.get("invariant")
        if isinstance(invariant, str):
            if invariant in seen_invariants:
                fail(
                    f"{rel} repeats invariant text at entries "
                    f"{seen_invariants[invariant]} and {index}: {invariant!r}"
                )
            else:
                seen_invariants[invariant] = index
        sensor = entry.get("sensor")
        if isinstance(sensor, str):
            for match in script_ref.findall(sensor):
                if ".." in match:
                    continue
                referenced = os.path.join(root, match)
                if not os.path.exists(referenced):
                    fail(
                        f"{rel}[{index}] references missing script `{match}` "
                        f"(sensor: {sensor!r})"
                    )

    if not failures:
        print(f"check-invariants OK: {len(entries)} decision header(s) verified.")
        sys.exit(0)

sys.exit(1)
PY

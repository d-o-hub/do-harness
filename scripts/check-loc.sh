#!/usr/bin/env bash
# check-loc.sh — enforces the 500 LOC per file invariant.
#
# Sensor: scripts/check-loc.sh
# Scope decision: all `.rs` files under `crates/` count, including test
# modules and the `templates/crate` scaffold (it ships to consumers), because
# reviewability does not depend on a file's role. Out of scope: root `tests/`
# (spike residue), `.agents/skills/` (markdown/python), `scripts/` (shell),
# and non-Rust template assets.
# Fails if any file exceeds MAX lines; warns at the decomposition threshold.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MAX="${1:-500}"
THRESHOLD=450
FAIL=0

while IFS= read -r file; do
    lines="$(wc -l < "$file")"
    if (( lines > MAX )); then
        echo "FAIL: $file has $lines lines (max $MAX)"
        FAIL=1
    elif (( lines >= THRESHOLD )); then
        echo "WARN: $file is nearing the limit: $lines lines"
    fi
done < <(find "$ROOT/crates" -name '*.rs' -not -path '*/target/*')

if (( FAIL )); then
    echo "LOC ceiling violated."
    exit 1
fi

echo "check-loc OK: all source files under $MAX lines."
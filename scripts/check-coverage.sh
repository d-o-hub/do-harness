#!/usr/bin/env bash
# check-coverage.sh — behavior inventory plus line/branch coverage thresholds.
#
# Sensor: scripts/check-coverage.sh
# Severity: warn
# Target threshold: 70% line coverage (codecov-style project target).
# Output: prints the behavior inventory, generates lcov.info, and reports
# `FINDINGS: <deficit>` — the line-percentage deficit against TARGET_PCT — for
# the blessed ratchet. The branch percentage is printed when the report carries
# one and never affects the ratchet number.
#
# Methodology:
# - inventory (fast path): unique compiled behavior as unique(file, fn) per layer —
#   crates/*/src (owners), crates/*/tests (crate integration), src/ (root facade),
#   tests/ (root integration). No test execution, python3 only.
# - llvm-cov (proof path): workspace line and branch percentages over the targets
#   that actually run. Both come from the lcov report itself (LH/LF for lines,
#   BRH/BRF for branches), never from a stdout summary: `--lcov` prints none.
# - Test-LOC counts replace neither layer.
#
# Usage: check-coverage.sh [inventory|llvm-cov] [dir]
# The script measures the workspace that contains it unless `dir` is given.

set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

MODE="all"
TARGET_DIR="$REPO_DIR"
case "${1:-}" in
    inventory)
        MODE="inventory"
        TARGET_DIR="${2:-$REPO_DIR}"
        ;;
    llvm-cov)
        MODE="llvm-cov"
        TARGET_DIR="${2:-$REPO_DIR}"
        ;;
    '') ;;
    *)
        printf 'check-coverage.sh: unknown argument: %s (see header)\n' "$1" >&2
        exit 2
        ;;
esac

cd "$TARGET_DIR"

# Behavior map: unique test function declarations per canonical layer, sorted
# deterministically. No test execution and no toolchain beyond python3.
print_inventory() {
    if ! command -v python3 >/dev/null 2>&1; then
        echo "SKIP: python3 not installed; behavior inventory unavailable."
        return 0
    fi
    python3 - <<'PY'
import os
import re

LAYER_LABEL = {
    "crates/*/src": "crates/*/src",
    "crates/*/tests": "crates/*/tests",
    "src/": "src/",
    "tests/": "tests/",
}
SKIP_DIRS = {"target", ".git", ".do-harness", ".agents", "node_modules"}
ATTR_RE = re.compile(r"#\s*\[\s*(?:[\w:]+::)?\w*test\b")
FN_RE = re.compile(r"\bfn\s+([a-zA-Z0-9_]+)")


def classify(relative):
    parts = relative.split("/")
    if len(parts) >= 3 and parts[0] == "crates":
        if parts[2] == "src":
            return "crates/*/src"
        if parts[2] == "tests" and "fixtures" not in parts:
            return "crates/*/tests"
    elif len(parts) >= 2 and parts[0] == "src":
        return "src/"
    elif len(parts) >= 2 and parts[0] == "tests" and "fixtures" not in parts:
        return "tests/"
    return None


def test_function(lines, index):
    """First `fn` name at most 15 lines below a test attribute, or None."""
    for following in lines[index + 1 : index + 15]:
        match = FN_RE.search(following)
        if match:
            return match.group(1)
        stripped = following.strip()
        if stripped and not stripped.startswith(("#[", "//", "/*", "*")):
            return None
    return None


counts = {layer: {"files": 0, "tests": 0, "unique": set()} for layer in LAYER_LABEL}

for root, dirs, files in os.walk("."):
    dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
    for name in sorted(files):
        if not name.endswith(".rs"):
            continue
        relative = os.path.relpath(os.path.join(root, name)).replace("\\", "/")
        layer = classify(relative)
        if layer is None:
            continue
        counts[layer]["files"] += 1
        try:
            with open(relative, encoding="utf-8", errors="ignore") as source:
                lines = source.readlines()
        except OSError:
            continue
        for index, line in enumerate(lines):
            if not ATTR_RE.search(line):
                continue
            counts[layer]["tests"] += 1
            name = test_function(lines, index)
            if name:
                counts[layer]["unique"].add((relative, name))

print("layer          | files | tests | unique(file, fn)")
print("---------------+-------+-------+------------------")
for layer in sorted(counts):
    entry = counts[layer]
    print(f"{layer:<14} | {entry['files']:>5} | {entry['tests']:>5} | {len(entry['unique']):>16}")
PY
}

print_inventory

if [[ "$MODE" == "inventory" ]]; then
    exit 0
fi

if ! command -v cargo-llvm-cov >/dev/null 2>&1 || ! command -v cargo-nextest >/dev/null 2>&1; then
    if require_tools; then
        echo "FAIL: cargo-llvm-cov and cargo-nextest are required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
        exit 1
    fi
    echo "SKIP: cargo-llvm-cov or cargo-nextest not installed; skipping coverage check."
    exit 0
fi

TARGET_PCT=70

if ! cov_output="$(cargo llvm-cov nextest --lcov --output-path lcov.info 2>&1)"; then
    echo "$cov_output"
    echo "FAIL: cargo llvm-cov nextest failed."
    exit 1
fi

echo "$cov_output"

if [[ ! -f lcov.info ]]; then
    echo "FAIL: cargo llvm-cov reported success but produced no lcov.info."
    exit 1
fi

# `--lcov` prints no TOTAL summary table, so both percentages are derived from
# the report: LH (lines hit) over LF (lines found), BRH over BRF for branches.
percentages="$(
    awk -F: '
        /^LF:/ { lines_found += $2 }
        /^LH:/ { lines_hit += $2 }
        /^BRF:/ { branches_found += $2 }
        /^BRH:/ { branches_hit += $2 }
        END {
            if (lines_found > 0) printf "%.2f ", 100 * lines_hit / lines_found
            else printf " "
            if (branches_found > 0) printf "%.2f", 100 * branches_hit / branches_found
        }
    ' lcov.info
)"
LINE_PCT="${percentages%% *}"
BRANCH_PCT="${percentages#* }"

if [[ -z "$LINE_PCT" ]]; then
    echo "WARN: Could not derive line coverage from lcov.info."
    echo "FINDINGS: 1"
    exit 0
fi

PCT_INT="${LINE_PCT%%.*}"
if [[ -z "$PCT_INT" ]]; then
    PCT_INT=0
fi

BRANCH_MSG=""
if [[ -n "$BRANCH_PCT" ]]; then
    BRANCH_MSG=", Branch coverage is ${BRANCH_PCT}%"
fi

if (( PCT_INT < TARGET_PCT )); then
    DEFICIT=$(( TARGET_PCT - PCT_INT ))
    echo "WARN: Line coverage is ${LINE_PCT}% (target ${TARGET_PCT}%, deficit ${DEFICIT}%)${BRANCH_MSG}."
    echo "FINDINGS: ${DEFICIT}"
else
    echo "check-coverage OK: Line coverage is ${LINE_PCT}% (>= ${TARGET_PCT}%)${BRANCH_MSG}."
    echo "FINDINGS: 0"
fi

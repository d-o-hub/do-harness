#!/usr/bin/env bash
# check-coverage.sh — behavior-based coverage inventory alongside line/branch threshold.
#
# Sensor: scripts/check-coverage.sh
# Severity: warn
# Target threshold: 70% line coverage (codecov-style project target).
# Output: generates lcov.info and reports `FINDINGS: <deficit>` for the blessed ratchet.
#
# Methodology:
# - inventory (fast path): unique compiled behavior map as unique(file, fn) per layer:
#   crates/*/src (owners), crates/*/tests (crate integration), src/ (root facade), tests/ (root integration).
# - llvm-cov (proof path): workspace line + branch percentages over targets.
# - test-LOC counts do not replace inventory or llvm-cov metrics.

set -euo pipefail

require_tools() { [[ "${CI:-}" == "true" || "${DO_HARNESS_REQUIRE_TOOLS:-}" == "1" ]]; }

MODE="all"
TARGET_DIR="."

if [[ "${1:-}" == "inventory" ]]; then
    MODE="inventory"
    TARGET_DIR="${2:-.}"
elif [[ "${1:-}" == "llvm-cov" ]]; then
    MODE="llvm-cov"
    TARGET_DIR="${2:-.}"
elif [[ -n "${1:-}" ]]; then
    TARGET_DIR="$1"
fi

cd "$TARGET_DIR"

print_inventory() {
    if command -v python3 >/dev/null 2>&1; then
        python3 -c '
import os, re

layers = {
    "crates/*/src": {"files": 0, "tests": 0, "unique": set()},
    "crates/*/tests": {"files": 0, "tests": 0, "unique": set()},
    "src/": {"files": 0, "tests": 0, "unique": set()},
    "tests/": {"files": 0, "tests": 0, "unique": set()},
}

def classify_layer(path):
    p = path.replace("\\", "/")
    if p.startswith("./"):
        p = p[2:]
    parts = p.split("/")
    if len(parts) >= 3 and parts[0] == "crates":
        if parts[2] == "src":
            return "crates/*/src"
        elif parts[2] == "tests" and "fixtures" not in parts:
            return "crates/*/tests"
    elif len(parts) >= 2 and parts[0] == "src":
        return "src/"
    elif len(parts) >= 2 and parts[0] == "tests" and "fixtures" not in parts:
        return "tests/"
    return None

attr_re = re.compile(r"#\s*\[\s*(?:[\w:]+::)?\w*test\b")
fn_re = re.compile(r"\bfn\s+([a-zA-Z0-9_]+)")

for root, dirs, files in os.walk("."):
    r_norm = root.replace("\\", "/")
    if "/target" in r_norm or "/.git" in r_norm or "/.do-harness" in r_norm:
        continue
    for f in sorted(files):
        if f.endswith(".rs"):
            rel = os.path.normpath(os.path.join(root, f)).replace("\\", "/")
            if rel.startswith("./"):
                rel = rel[2:]
            layer = classify_layer(rel)
            if not layer:
                continue
            layers[layer]["files"] += 1

            try:
                with open(rel, "r", encoding="utf-8", errors="ignore") as fobj:
                    lines = fobj.readlines()
            except Exception:
                continue

            for i, line in enumerate(lines):
                if attr_re.search(line):
                    layers[layer]["tests"] += 1
                    fn_name = None
                    for j in range(i + 1, min(i + 15, len(lines))):
                        m = fn_re.search(lines[j])
                        if m:
                            fn_name = m.group(1)
                            break
                        stripped = lines[j].strip()
                        if stripped and not stripped.startswith("#[") and not stripped.startswith("//") and not stripped.startswith("///") and not stripped.startswith("/*") and not stripped.startswith("*"):
                            break
                    if fn_name:
                        layers[layer]["unique"].add((rel, fn_name))

print("layer          | files | tests | unique(file, fn)")
print("---------------+-------+-------+------------------")
for l in sorted(layers.keys()):
    f_cnt = layers[l]["files"]
    t_cnt = layers[l]["tests"]
    u_cnt = len(layers[l]["unique"])
    print(f"{l:<14} | {f_cnt:>5} | {t_cnt:>5} | {u_cnt:>16}")
'
    else
        echo "layer          | files | tests | unique(file, fn)"
        echo "---------------+-------+-------+------------------"
        echo "crates/*/src   |     0 |     0 |                0"
        echo "crates/*/tests |     0 |     0 |                0"
        echo "src/           |     0 |     0 |                0"
        echo "tests/         |     0 |     0 |                0"
    fi
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

parsed_percentages="$(echo "$cov_output" | python3 -c '
import sys, re

cov_text = sys.stdin.read()
total_line = ""
for line in cov_text.splitlines():
    if line.startswith("TOTAL "):
        total_line = line
        break

pcts = re.findall(r"(\d+\.\d+)%", total_line)
line_pct = ""
branch_pct = ""

if len(pcts) >= 4:
    line_pct = pcts[2]
    branch_pct = pcts[3]
elif len(pcts) == 3:
    line_pct = pcts[2]
elif len(pcts) >= 1:
    line_pct = pcts[-1]

print(f"{line_pct},{branch_pct}")
' 2>/dev/null || echo ",")"

LINE_PCT="$(echo "$parsed_percentages" | cut -d',' -f1)"
BRANCH_PCT="$(echo "$parsed_percentages" | cut -d',' -f2)"

if [[ -z "$LINE_PCT" ]]; then
    LINE_PCT="$(echo "$cov_output" | awk '/^TOTAL[[:space:]]/ {for(i=1;i<=NF;i++) if ($i ~ /^[0-9]+\.[0-9]+%$/) p=$i} END {print p}' | tr -d '%')"
fi

if [[ -z "$LINE_PCT" ]]; then
    echo "WARN: Could not parse coverage percentage from cargo-llvm-cov output."
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

#!/usr/bin/env bash
# check-loc.sh — enforces the 500 LOC per file invariant.
#
# Sensor: scripts/check-loc.sh
# Fails if any .rs file under src/ or crates/ exceeds MAX lines.
# Writes a warning when a file is at or above the decomposition threshold.
#
# The output is feedforward, not a bare verdict: every over-limit file lists
# its largest top-level item spans (`SPAN:`) and every file at or above the
# threshold reports its code/test line split (`CODE:`/`TEST:`), so a candidate
# split point is visible without re-reading the file. Boundary detection is a
# line-based, column-0 heuristic — nested items are attributed to their
# enclosing top-level item, and attribute/doc lines count toward the item they
# annotate. A planning aid, not an AST.
#
# `FINDINGS: <n>` reports the over-limit file count so the blessed ratchet
# (`do-harness verify --record --bless`) can pin it.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
MAX="${1:-500}"
THRESHOLD=450
FAIL_COUNT=0

# How many of the largest top-level items to report per over-limit file.
SPAN_LIMIT=5

# Top-level item starts, column 0 only: a nested item belongs to its owner.
ITEM_RE='^(fn |pub fn |pub\(crate\) fn |impl |mod |struct |enum |trait |const |static |type |macro_rules!)'

# Line number of the first inline `#[cfg(test)]` marker, or 0 when absent.
test_boundary() {
    local file="$1" match
    match="$(grep -nE '^[[:space:]]*#\[cfg\(test\)\]' "$file" | head -n 1 || true)"
    if [[ -z "$match" ]]; then
        printf '0'
    else
        printf '%s' "${match%%:*}"
    fi
}

# Emits `  CODE: <n>  TEST: <n>` when the file has an inline test module:
# everything before the marker is production code, the rest is fixtures.
report_code_test() {
    local file="$1" total="$2" boundary code
    boundary="$(test_boundary "$file")"
    if (( boundary > 0 )); then
        code="$(( boundary - 1 ))"
        printf '  CODE: %d  TEST: %d\n' "$code" "$(( total - code ))"
    fi
}

# Emits `  SPAN: <item>: <n> lines (<start>..<end>)` for the largest top-level
# items, then the code/test split. A span runs from the item's first
# attribute/doc line through the line before the next top-level item (or EOF).
report_density() {
    local file="$1" total="$2" row
    local -a rows=() top=()

    mapfile -t rows < <(awk -v item_re="$ITEM_RE" '
        { line[NR] = $0 }
        END {
            n = 0
            for (i = 1; i <= NR; i++) {
                if (line[i] !~ item_re) continue
                start = i
                while (start > 1 &&
                       (line[start - 1] ~ /^[[:space:]]*#\[/ ||
                        line[start - 1] ~ /^[[:space:]]*\/\/\//)) {
                    start--
                }
                if (n > 0 && start <= prev_start) continue
                n++
                prev_start = start
                prev_item[n] = i
                starts[n] = start
            }
            for (k = 1; k <= n; k++) {
                end = (k < n) ? starts[k + 1] - 1 : NR
                if (end < starts[k]) continue
                label = line[prev_item[k]]
                sub(/^pub\(crate\) /, "", label)
                sub(/^pub /, "", label)
                sub(/[[:space:]]*\(.*$/, "", label)
                sub(/[[:space:]]*\{.*$/, "", label)
                sub(/[[:space:]]*=.*$/, "", label)
                sub(/[[:space:]]*;.*$/, "", label)
                sub(/[[:space:]]+$/, "", label)
                if (length(label) > 64) label = substr(label, 1, 64)
                printf "%08d|%s|%s|%s\n", end - starts[k] + 1, starts[k], end, label
            }
        }
    ' "$file")
    if (( ${#rows[@]} > 0 )); then
        # No `head` here: truncating a pipeline would trip `pipefail`.
        mapfile -t top < <(printf '%s\n' "${rows[@]}" | sort -r)
        for row in "${top[@]:0:$SPAN_LIMIT}"; do
            IFS='|' read -r span start end label <<<"$row"
            printf '  SPAN: %s: %d lines (%d..%d)\n' \
                "$label" "$(( 10#$span ))" "$(( 10#$start ))" "$(( 10#$end ))"
        done
    fi
    report_code_test "$file" "$total"
}

while IFS= read -r file; do
    lines="$(wc -l < "$file")"
    if (( lines > MAX )); then
        echo "FAIL: $file has $lines lines (max $MAX)"
        report_density "$file" "$lines"
        FAIL_COUNT=$(( FAIL_COUNT + 1 ))
    elif (( lines >= THRESHOLD )); then
        echo "WARN: $file is nearing the limit: $lines lines"
        report_code_test "$file" "$lines"
    fi
done < <(find "$ROOT/src" "$ROOT/crates" -name '*.rs' -not -path '*/target/*' 2>/dev/null)

if (( FAIL_COUNT > 0 )); then
    echo "LOC ceiling violated."
    echo "FINDINGS: $FAIL_COUNT"
    exit 1
fi

echo "check-loc OK: all source files under $MAX lines."
echo "FINDINGS: 0"

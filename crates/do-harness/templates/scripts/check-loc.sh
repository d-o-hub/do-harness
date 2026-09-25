#!/usr/bin/env bash
# check-loc.sh — enforces the per-file LOC invariant.
#
# Sensor: scripts/check-loc.sh
#
# Scope: every scanned file counts, whatever its role — extraction of a test
# module does not make a file reviewable. `--root` and `--ext` widen or narrow
# the scan (default: `src/` and `crates/`, `*.rs`), so a repository with a
# front-end can enforce the same ceiling on it from the config, without forking
# this script:
#
#   [[sensors]]
#   name = "loc"
#   argv = ["bash", "scripts/check-loc.sh", "--root", "web", "--ext", "ts,tsx"]
#   when-changed = ["**/*.rs", "web/**/*.ts", "web/**/*.tsx"]
#
# `--max` sets the ceiling and `--warn` the decomposition threshold. Generated
# trees (`node_modules`, `target`, `dist`, `build`, `coverage`, `vendor`,
# `.next`, `out`, `.git`) are pruned and cannot be re-included.
#
# Fails if any file exceeds MAX lines; warns at the decomposition threshold.
#
# The output is feedforward, not a bare verdict: every over-limit Rust file
# lists its largest top-level item spans (`SPAN:`) and every Rust file at or
# above the threshold reports its code/test line split (`CODE:`/`TEST:`), so a
# candidate split point is visible without re-reading the file. That heuristic
# is line-based, column-0, and Rust-shaped; other extensions report the verdict
# alone.
#
# `FINDINGS: <n>` reports the over-limit file count so the blessed ratchet
# (`do-harness verify --record --bless`) can pin it.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"

MAX=500
THRESHOLD=450
FAIL_COUNT=0
declare -a ROOTS=() EXTS=()
ROOTS_EXPLICIT=0

# How many of the largest top-level items to report per over-limit file.
SPAN_LIMIT=5

# Directory names never scanned: build output and vendored trees, not sources.
PRUNE_NAMES=(node_modules target dist build coverage vendor .next out .git)

# Top-level item starts, column 0 only: a nested item belongs to its owner.
ITEM_RE='^(fn |pub fn |pub\(crate\) fn |impl |mod |struct |enum |trait |const |static |type |macro_rules!)'

usage() {
    cat <<'EOF'
Usage: check-loc.sh [--max N] [--warn N] [--root DIR]... [--ext EXT]... [MAX]

  --max N        Per-file ceiling (default 500)
  --warn N       Decomposition threshold (default 450)
  --root DIR     Scan this directory, relative to the repository root; a
                 configured root that does not exist is an error, so a typo
                 cannot pass as a vacuous green
                 (repeatable, comma-separated; default src, crates)
  --ext EXT      Scan this extension (repeatable, comma-separated; default rs).
                 A leading dot is accepted.
  MAX            Legacy positional ceiling, equivalent to --max

Every over-limit file reports a FINDINGS line so `verify --record --bless`
can pin the count.
EOF
}

while (( $# > 0 )); do
    case "$1" in
        --max) MAX="${2:?--max needs a value}"; shift 2 ;;
        --max=*) MAX="${1#*=}"; shift ;;
        --warn) THRESHOLD="${2:?--warn needs a value}"; shift 2 ;;
        --warn=*) THRESHOLD="${1#*=}"; shift ;;
        --root | --root=*)
            value="${1#*=}"
            [[ "$1" == "--root" ]] && { value="${2:?--root needs a value}"; shift; }
            IFS=, read -r -a parts <<<"$value"
            ROOTS+=("${parts[@]}")
            ROOTS_EXPLICIT=1
            shift
            ;;
        --ext | --ext=*)
            value="${1#*=}"
            [[ "$1" == "--ext" ]] && { value="${2:?--ext needs a value}"; shift; }
            IFS=, read -r -a parts <<<"$value"
            EXTS+=("${parts[@]}")
            shift
            ;;
        -h | --help) usage; exit 0 ;;
        [0-9]*) MAX="$1"; shift ;;
        *) usage >&2; exit 2 ;;
    esac
done

(( ${#ROOTS[@]} > 0 )) || ROOTS=(src crates)
(( ${#EXTS[@]} > 0 )) || EXTS=(rs)

# A configured root that does not exist fails closed: a typo must not turn the
# ceiling into a silent vacuous pass. The default roots are optional, because a
# repository need not have both `src/` and `crates/`.
if (( ROOTS_EXPLICIT )); then
    for root in "${ROOTS[@]}"; do
        if [[ ! -d "$ROOT/$root" ]]; then
            echo "check-loc: --root $root does not exist under $ROOT" >&2
            exit 2
        fi
    done
fi

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

# Every scanned file, relative to `$ROOT`, one per line.
scan_files() {
    local ext root
    local -a not_path=()
    for name in "${PRUNE_NAMES[@]}"; do
        not_path+=(-not -path "*/$name/*")
    done
    for ext in "${EXTS[@]}"; do
        ext="${ext#.}"
        for root in "${ROOTS[@]}"; do
            [[ -d "$ROOT/$root" ]] || continue
            find "$ROOT/$root" -type f -name "*.$ext" "${not_path[@]}" 2>/dev/null
        done
    done | sort -u
}

while IFS= read -r file; do
    [[ -n "$file" ]] || continue
    lines="$(wc -l < "$file")"
    if (( lines > MAX )); then
        echo "FAIL: $file has $lines lines (max $MAX)"
        case "$file" in
            *.rs) report_density "$file" "$lines" ;;
        esac
        FAIL_COUNT=$(( FAIL_COUNT + 1 ))
    elif (( lines >= THRESHOLD )); then
        echo "WARN: $file is nearing the limit: $lines lines"
        case "$file" in
            *.rs) report_code_test "$file" "$lines" ;;
        esac
    fi
done < <(scan_files)

if (( FAIL_COUNT > 0 )); then
    echo "LOC ceiling violated."
    echo "FINDINGS: $FAIL_COUNT"
    exit 1
fi

echo "check-loc OK: all source files under $MAX lines."
echo "FINDINGS: 0"

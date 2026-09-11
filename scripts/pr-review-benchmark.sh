#!/usr/bin/env bash
# Measure `do-harness pr review` input reduction over recent first-parent
# commits, as the phase-4 evidence for plans/pr-triage-skill-epic.md.
#
# Usage: pr-review-benchmark.sh [COUNT]
# Env: DO_HARNESS_BIN overrides the binary (default: target/debug/do-harness).
#
# Stdout (TSV): <sha> <t_raw> <t_res> <verdict> <skipped> <subject>
# Stderr summary: ordinary (no skipped units) reduced/no-go counts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${DO_HARNESS_BIN:-$ROOT/target/debug/do-harness}"
COUNT="${1:-20}"

if [[ ! -x "$BIN" ]]; then
  echo "pr-review-benchmark: binary not found: $BIN" >&2
  echo "build with: cargo build -p do-harness (or set DO_HARNESS_BIN)" >&2
  exit 2
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "pr-review-benchmark: jq is required" >&2
  exit 2
fi

ordinary=0
ordinary_reduced=0
ordinary_no_go=0

while IFS= read -r sha; do
  parent="$(git -C "$ROOT" rev-parse --verify --quiet "${sha}^")" || continue
  json="$("$BIN" --root "$ROOT" pr review --base "$parent" --head "$sha" --recompute --format json)"
  t_raw="$(jq -r '.measurement.t_raw' <<<"$json")"
  t_res="$(jq -r '.measurement.t_res' <<<"$json")"
  verdict="$(jq -r '.measurement.verdict' <<<"$json")"
  skipped="$(jq -r '.skipped | length' <<<"$json")"
  subject="$(git -C "$ROOT" log -1 --pretty=%s "$sha")"
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$sha" "$t_raw" "$t_res" "$verdict" "$skipped" "$subject"
  if [[ "$skipped" -eq 0 ]]; then
    ordinary=$((ordinary + 1))
    if [[ "$verdict" == "reduced" ]]; then
      ordinary_reduced=$((ordinary_reduced + 1))
    else
      ordinary_no_go=$((ordinary_no_go + 1))
    fi
  fi
done < <(git -C "$ROOT" log --first-parent --pretty=%H -n "$COUNT")

printf 'ordinary=%d reduced=%d no_go=%d\n' "$ordinary" "$ordinary_reduced" "$ordinary_no_go" >&2

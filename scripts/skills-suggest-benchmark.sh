#!/usr/bin/env bash
# Measure progressive-disclosure skill selection over the fixture catalog.
#
# Usage: skills-suggest-benchmark.sh [--root DIR] [--tasks FILE]
# Env: DO_HARNESS_BIN overrides the binary (default: target/debug/do-harness).
#      DO_HARNESS_SKILL_SELECTOR / DO_HARNESS_SKILL_SELECTOR_TIMEOUT arm the
#      optional semantic selector.
#
# Three arms are measured over the same catalog and task set so a selector that
# reorders without improving selection is visible rather than assumed:
#   load-all                  every SKILL.md read for every task (baseline)
#   deterministic-only        metadata index + one loaded skill per task
#   deterministic+selector    the same, with DO_HARNESS_SKILL_SELECTOR armed
#
# Stdout (TSV): <arm> <task> <expected_top1> <ranked-top1> <top1> <top3> <top5>
#               <metadata-bytes> <context-bytes> <loaded-skills>
# Stderr summary: per-case rows summarize to counts and byte totals.
#
# The selector can only reorder the deterministic shortlist, so the selector arm
# always reports its own in/out byte volume next to the deterministic arm.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURES="$ROOT/tests/fixtures/skill-catalog"
TASKS=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      FIXTURES="${2:?--root requires a directory}"
      shift 2
      ;;
    --tasks)
      TASKS="${2:?--tasks requires a file}"
      shift 2
      ;;
    *)
      echo "skills-suggest-benchmark: unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

BIN="${DO_HARNESS_BIN:-$ROOT/target/debug/do-harness}"
if [[ ! -x "$BIN" ]]; then
  echo "skills-suggest-benchmark: binary not found: $BIN" >&2
  echo "build with: cargo build -p do-harness (or set DO_HARNESS_BIN)" >&2
  exit 2
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "skills-suggest-benchmark: jq is required" >&2
  exit 2
fi
if [[ ! -d "$FIXTURES/catalog" ]]; then
  echo "skills-suggest-benchmark: no catalog under $FIXTURES/catalog" >&2
  exit 2
fi
TASKS="${TASKS:-$FIXTURES/tasks.json}"
if [[ ! -f "$TASKS" ]]; then
  echo "skills-suggest-benchmark: no task set at $TASKS" >&2
  exit 2
fi

# Hermetic workspace: the fixture catalog becomes the root's skill tree, so the
# benchmark never measures the caller's own skills or cache.
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/.agents"
cp -R "$FIXTURES/catalog" "$work/.agents/skills"
rm -rf "$work/.do-harness"

limit=5
task_count="$(jq -r '.tasks | length' "$TASKS")"
selector="${DO_HARNESS_SKILL_SELECTOR:-}"

# Full-context bytes per task for the load-all arm: every body in the catalog.
catalog_bytes=0
catalog_skills=0
while IFS= read -r -d '' skill_md; do
  catalog_bytes=$((catalog_bytes + $(wc -c < "$skill_md" | tr -d ' ')))
  catalog_skills=$((catalog_skills + 1))
done < <(find "$work/.agents/skills" -name 'SKILL.md' -print0)

pct() {  # <numerator> <denominator>
  awk -v n="$1" -v d="$2" 'BEGIN { if (d == 0) { print "0.0" } else { printf "%.1f", (n / d) * 100 } }'
}

# In-list test for the top-k prefix of the ranked names.
has_in_top() {  # <names> <k> <expected>
  printf '%s\n' "$1" | tr ' ' '\n' | head -"$2" | grep -qx "$3"
}

# Runs one arm, echoing per-task TSV rows and reporting aggregates to fd 2.
run_arm() {  # <arm>
  local arm="$1"
  local top1_hits=0 top3_hits=0 top5_hits=0
  local metadata_total=0 context_total=0 latency_total_ms=0

  for index in $(seq 0 $((task_count - 1))); do
    local query expected start_ms end_ms json names top1 metadata_bytes
    query="$(jq -r ".tasks[$index].query" "$TASKS")"
    expected="$(jq -r ".tasks[$index].expected_top1" "$TASKS")"

    start_ms="$(date +%s%3N)"
    json="$("$BIN" --root "$work" skills suggest \
      --query "$query" --limit "$limit" --format json)"
    end_ms="$(date +%s%3N)"
    latency_total_ms=$((latency_total_ms + end_ms - start_ms))

    names="$(jq -r '.candidates | map(.name) | join(" ")' <<<"$json")"
    top1="$(jq -r '.candidates[0].name // ""' <<<"$json")"
    metadata_bytes="$(printf '%s' "$json" | wc -c | tr -d ' ')"

    local top1_flag=no top3_flag=no top5_flag=no
    if [[ "$top1" == "$expected" ]]; then
      top1_flag=yes
      top1_hits=$((top1_hits + 1))
    fi
    has_in_top "$names" 3 "$expected" && top3_flag=yes
    has_in_top "$names" 5 "$expected" && top5_flag=yes
    [[ "$top3_flag" == "yes" ]] && top3_hits=$((top3_hits + 1))
    [[ "$top5_flag" == "yes" ]] && top5_hits=$((top5_hits + 1))

    # Progressive disclosure: only the selected skill's body is read this task.
    local context_bytes=0 loaded_skills=0
    if [[ -f "$work/.agents/skills/$top1/SKILL.md" ]]; then
      context_bytes="$(wc -c < "$work/.agents/skills/$top1/SKILL.md" | tr -d ' ')"
      loaded_skills=1
    fi

    metadata_total=$((metadata_total + metadata_bytes))
    context_total=$((context_total + context_bytes))

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$arm" "$index" "$expected" "$top1" "$top1_flag" "$top3_flag" "$top5_flag" \
      "$metadata_bytes" "$context_bytes" "$loaded_skills"
  done

  {
    echo "arm=$arm tasks=$task_count catalog_skills=$catalog_skills"
    echo "  top1_accuracy=$(pct "$top1_hits" "$task_count") top3_recall=$(pct "$top3_hits" "$task_count") top5_recall=$(pct "$top5_hits" "$task_count")"
    echo "  unnecessary_skill_load_rate=$(pct $((task_count - top1_hits)) "$task_count")"
    echo "  metadata_bytes_per_task=$(awk -v t="$metadata_total" -v n="$task_count" 'BEGIN { printf "%.1f", t / n }')"
    echo "  loaded_context_bytes_per_task=$(awk -v t="$context_total" -v n="$task_count" 'BEGIN { printf "%.1f", t / n }')"
    echo "  catalog_bytes_per_task=$catalog_bytes"
    echo "  context_reduction_vs_load_all=$(awk -v a="$catalog_bytes" -v b="$context_total" -v n="$task_count" 'BEGIN { if (a == 0 || n == 0) { print "0.000" } else { printf "%.3f", 1 - ((b / n) / a) } }')"
    echo "  latency_ms_per_task=$(awk -v t="$latency_total_ms" -v n="$task_count" 'BEGIN { printf "%.1f", t / n }')"
  } >&2
}

# Arm 2 (and 3 when configured) always run with the selector explicitly removed
# first, so an inherited variable cannot silently turn both arms into one.
if [[ -n "$selector" ]]; then
  echo "selector=$selector selector_bytes_in_per_task=$(printf '%s' "$(jq -c '.tasks' "$TASKS")" | wc -c | tr -d ' ') (candidate metadata only)" >&2
fi

unset DO_HARNESS_SKILL_SELECTOR
run_arm deterministic-only

if [[ -n "$selector" ]]; then
  export DO_HARNESS_SKILL_SELECTOR="$selector"
  run_arm deterministic+selector
else
  echo "arm=deterministic+selector SKIPPED: set DO_HARNESS_SKILL_SELECTOR to an executable to measure it" >&2
fi

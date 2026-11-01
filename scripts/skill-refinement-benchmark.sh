#!/usr/bin/env bash
# Benchmark script for multi-candidate skill refinement experiment.
# Compares baseline (Canonical SKILL.md) against candidate guidance variants
# (Concise, Verbose, Checklist) across representative skill evaluation suites.
set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

ROOT="$(dh_repo_root)"
BIN="${DO_HARNESS_BIN:-$ROOT/target/debug/do-harness}"
FIXTURES="$ROOT/tests/fixtures/skill-refinement"
ITERATIONS="${ITERATIONS:-5}"
OUT_FILE="${1:-}"

if [[ ! -x "$BIN" ]]; then
  echo "Building do-harness binary..." >&2
  cargo build -p do-harness --bin do-harness >&2
fi

SKILLS=("htn-planner" "tokio-performance" "skill-creator")
VARIANTS=("baseline" "candidate_1_concise" "candidate_2_verbose" "candidate_3_checklist")

results_json="[]"

temp_work="$(mktemp -d)"
trap 'rm -rf "$temp_work"' EXIT

# Prepare sandbox workspace base once
mkdir -p "$temp_work/.agents/skills"
for skill in "${SKILLS[@]}"; do
  cp -R "$ROOT/.agents/skills/$skill" "$temp_work/.agents/skills/"
done

echo "=== Starting Multi-Candidate Skill Refinement Benchmark ($ITERATIONS iterations) ===" >&2

for iter in $(seq 1 "$ITERATIONS"); do
  for skill in "${SKILLS[@]}"; do
    for variant in "${VARIANTS[@]}"; do
      # Set up SKILL.md for variant
      if [[ "$variant" == "baseline" ]]; then
        cp "$ROOT/.agents/skills/$skill/SKILL.md" "$temp_work/.agents/skills/$skill/SKILL.md"
      else
        cp "$FIXTURES/$skill/${variant}.md" "$temp_work/.agents/skills/$skill/SKILL.md"
      fi

      # Run eval, capturing stdout and stderr
      eval_err_file="$(mktemp)"
      eval_out_file="$(mktemp)"

      if ! "$BIN" --root "$temp_work" eval --skill "$skill" --format json >"$eval_out_file" 2>"$eval_err_file"; then
        eval_status="failed"
        err_msg="$(cat "$eval_err_file")"
      else
        eval_status="success"
        err_msg=""
      fi

      eval_out="$(cat "$eval_out_file")"
      rm -f "$eval_err_file" "$eval_out_file"

      if [[ -z "$eval_out" || "$eval_out" == "[]" ]]; then
        echo "Error: Eval returned empty output for $skill ($variant) on iteration $iter: $err_msg" >&2
        exit 1
      fi

      pass_rate="$(jq -r '.[0].pass_rate // 0.0' <<<"$eval_out")"
      graded="$(jq -r '.[0].graded // 0' <<<"$eval_out")"
      passed="$(jq -r '.[0].passed // 0' <<<"$eval_out")"
      lift="$(jq -r '.[0].lift // 0.0' <<<"$eval_out")"
      words="$(jq -r '.[0].skill_words // 0' <<<"$eval_out")"
      walk_secs="$(jq -r '.[0].walk_secs // 0.0' <<<"$eval_out")"
      cases="$(jq -c '.[0].cases // []' <<<"$eval_out")"

      # Efficiency ratio = pass_rate / words * 1000
      eff_ratio="$(awk -v p="$pass_rate" -v w="$words" 'BEGIN { if (w == 0) print "0.0"; else printf "%.4f", (p / w) * 1000 }')"

      entry="$(jq -n \
        --arg iter "$iter" \
        --arg skill "$skill" \
        --arg variant "$variant" \
        --arg words "$words" \
        --arg pass_rate "$pass_rate" \
        --arg graded "$graded" \
        --arg passed "$passed" \
        --arg lift "$lift" \
        --arg walk_secs "$walk_secs" \
        --arg eff "$eff_ratio" \
        --arg status "$eval_status" \
        --arg err "$err_msg" \
        --argjson cases "$cases" \
        '{iteration: ($iter|tonumber), skill: $skill, variant: $variant, words: ($words|tonumber), pass_rate: ($pass_rate|tonumber), graded: ($graded|tonumber), passed: ($passed|tonumber), lift: ($lift|tonumber), walk_secs: ($walk_secs|tonumber), efficiency_ratio: ($eff|tonumber), status: $status, error: $err, cases: $cases}')"

      results_json="$(jq --argjson e "$entry" '. + [$e]' <<<"$results_json")"
    done
  done
done

# Compute aggregates and write summary report JSON
report="$(python3 - <<< "
import json, sys, math

true = True
false = False
null = None

data = $results_json

by_key = {}
for r in data:
    key = (r['skill'], r['variant'])
    if key not in by_key:
        by_key[key] = []
    by_key[key].append(r)

summary = []
for (skill, variant), runs in by_key.items():
    n = len(runs)
    words = runs[0]['words']
    pass_rates = [r['pass_rate'] for r in runs]
    lifts = [r['lift'] for r in runs]
    walks = [r['walk_secs'] for r in runs]
    effs = [r['efficiency_ratio'] for r in runs]

    avg_pass = sum(pass_rates) / n
    avg_lift = sum(lifts) / n
    avg_walk = sum(walks) / n
    avg_eff = sum(effs) / n

    # variance of pass_rate
    var_pass = sum((x - avg_pass) ** 2 for x in pass_rates) / n if n > 1 else 0.0

    summary.append({
        'skill': skill,
        'variant': variant,
        'words': words,
        'avg_pass_rate': round(avg_pass, 4),
        'pass_rate_variance': round(var_pass, 6),
        'avg_lift': round(avg_lift, 4),
        'avg_walk_secs': round(avg_walk, 4),
        'avg_efficiency_ratio': round(avg_eff, 4),
        'iterations': n
    })

meta = {
    'schema_version': 1,
    'evaluator_mode': 'deterministic',
    'total_runs_recorded': len(data),
    'expected_runs': len(by_key) * ${ITERATIONS}
}

print(json.dumps({'metadata': meta, 'runs': data, 'summary': summary}, indent=2))
")"

if [[ -n "$OUT_FILE" ]]; then
  echo "$report" > "$OUT_FILE"
fi

echo "$report"

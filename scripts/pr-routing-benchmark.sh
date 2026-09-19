#!/usr/bin/env bash
# End-to-end routing-cost and route-safety benchmark for semantic PR routing.
#
# Usage: pr-routing-benchmark.sh [COUNT] [--fixtures DIR] [--router MODE]
#   COUNT            Replay the last COUNT first-parent commits of this repo
#                    (default fixtures mode when --fixtures is absent).
#   --fixtures DIR   Materialize the seeded corpus at DIR (default
#                    $ROOT/tests/fixtures/pr-routing) instead of git history.
#   --router MODE    FAKE_ROUTER_MODE for every case (default: per-class mode).
# Env: DO_HARNESS_BIN   Harness binary (default: target/debug/do-harness).
#      PR_TRIAGE_ROUTER Fake provider path; defaults to the fixture provider.
#      PR_TRIAGE_ROUTER_TIMEOUT seconds (default: 5).
#
# Stdout: one JSON row per case (the machine-readable source of truth).
# Stderr: aggregates plus two recommendations — the conservative `VERDICT`
# (review payload, ignoring routing) and the route-aware `ROUTED_VERDICT`
# (the payload the route actually selects) — and a `FINDINGS: <n>` marker
# counting oracle failures.
#
# Cost models:
#   conservative  total = router_input + review_input   (routing always adds)
#   route-aware   total = router_input + routed_review  where the route picks
#                 the payload: `cheap` reads nothing (0), `focused` reads the
#                 residual, `deep` reads the full raw diff, and a no-effect
#                 case spends no model bytes at all.
#
# Byte fields are byte proxies for deterministic CI. They are never tokens and
# must not be presented as exact token counts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${DO_HARNESS_BIN:-$ROOT/target/debug/do-harness}"
FIXTURES="$ROOT/tests/fixtures/pr-routing"
COUNT=""
ROUTER_MODE=""
FIXTURES_EXPLICIT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --fixtures)
      FIXTURES="${2:?--fixtures requires a directory}"
      FIXTURES_EXPLICIT="yes"
      shift 2
      ;;
    --router)
      ROUTER_MODE="${2:?--router requires a mode}"
      shift 2
      ;;
    *)
      COUNT="$1"
      shift
      ;;
  esac
done

if [[ ! -x "$BIN" ]]; then
  echo "pr-routing-benchmark: binary not found: $BIN" >&2
  echo "build with: cargo build -p do-harness (or set DO_HARNESS_BIN)" >&2
  exit 2
fi
# The router wrapper runs with the case repository as its cwd, so a relative
# DO_HARNESS_BIN would stop resolving after the first case and silently
# degrade every route to the `deep` fallback.
BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"
if ! command -v jq >/dev/null 2>&1; then
  echo "pr-routing-benchmark: jq is required" >&2
  exit 2
fi
# Absolute paths: the benchmark changes directory per case, so a relative
# `--fixtures` would stop resolving after the first `cd`.
FIXTURES="$(cd "$FIXTURES" 2>/dev/null && pwd)" || {
  echo "pr-routing-benchmark: no corpus under $FIXTURES" >&2
  exit 2
}
# An explicitly requested corpus must be a real corpus. Falling through to
# historical replay would silently benchmark the caller's own repository.
if [[ "$FIXTURES_EXPLICIT" == "yes" && ! -f "$FIXTURES/manifest.json" ]]; then
  echo "pr-routing-benchmark: $FIXTURES has no manifest.json" >&2
  echo "an explicitly requested corpus must not fall back to git history" >&2
  exit 2
fi

ROUTER="${PR_TRIAGE_ROUTER:-$FIXTURES/fake-router.sh}"
if [[ ! -x "$ROUTER" ]]; then
  echo "pr-routing-benchmark: router is not executable: $ROUTER" >&2
  exit 2
fi
export PR_TRIAGE_ROUTER="$ROUTER"
export PR_TRIAGE_ROUTER_TIMEOUT="${PR_TRIAGE_ROUTER_TIMEOUT:-5}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The harness command needs an isolated repository per case so `--base/--head`
# resolve inside the fixture, never inside the caller's checkout.
export GIT_DIR=""
export GIT_WORK_TREE=""
unset GIT_DIR GIT_WORK_TREE

# Route ordering for the seeded oracle. A route is acceptable when it is at
# least as deep as the case's declared minimum.
rank() {
  case "$1" in
    cheap) echo 0 ;;
    focused) echo 1 ;;
    deep) echo 2 ;;
    *) echo -1 ;;
  esac
}

# Per-class provider mode: the corpus declares the class, so the deterministic
# policy is what is under test rather than provider accuracy.
mode_for() {
  case "$1" in
    docs-only) echo docs ;;
    tests-only) echo tests ;;
    lockfile-only) echo dependency ;;
    internal-refactor) echo refactor ;;
    behavior-change|error-handling) echo behavior ;;
    public-api-break) echo api ;;
    security) echo security ;;
    persistence-schema) echo schema ;;
    concurrency) echo concurrency ;;
    mixed-buried) echo misleading ;;
    router-invalid) echo malformed ;;
    *) echo misleading ;;
  esac
}

# Materializes one corpus case as a two-commit repository.
materialize() {  # <case-dir> <dest>
  local case_dir="$1" dest="$2"
  mkdir -p "$dest"
  cp -R "$case_dir/base/." "$dest/" 2>/dev/null || true
  (
    cd "$dest"
    git init -q -b main
    git config user.email "bench@example.com"
    git config user.name "Bench"
    git add -A
    git commit -q -m base
    git checkout -q -b feature
    # Replace the tree wholesale with the head state.
    find . -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
    cp -R "$case_dir/head/." .
    git add -A
    git commit -q -m head
  )
}

total_bytes=0
baseline_total=0
routed_total_bytes=0
case_count=0
oracle_failures=0
downgrades=0
fallbacks=0
router_calls=0
zero_model_cases=0
reduced=0
no_go=0
savings_sum=0
routed_savings_sum=0
median_list="$work/savings.txt"
: > "$median_list"
routed_median_list="$work/routed_savings.txt"
: > "$routed_median_list"

run_case() {  # <case> <expected_min_route> <repo>
  local case_name="$1" expected="$2" repo="$3"

  local review route_json route source
  review="$("$BIN" --root "$repo" pr review --base main --head feature \
    --recompute --format json)"
  local t_raw t_res verdict
  t_raw="$(jq -r '.measurement.t_raw' <<<"$review")"
  t_res="$(jq -r '.measurement.t_res' <<<"$review")"
  verdict="$(jq -r '.measurement.verdict' <<<"$review")"

  # Step 3-4: current source selection, then the semantic route over it.
  if [[ "$verdict" == "reduced" ]]; then
    source="residual"
    review_input_bytes="$t_res"
    reduced=$((reduced + 1))
  else
    source="raw"
    review_input_bytes="$t_raw"
    no_go=$((no_go + 1))
  fi

  local mode="${ROUTER_MODE:-$(mode_for "$case_name")}"
  local router_in_file="$work/router_in_$case_name"
  rm -f "$router_in_file"
  route_json="$(cd "$repo" && FAKE_ROUTER_MODE="$mode" \
    FAKE_ROUTER_INPUT_BYTES="$router_in_file" DO_HARNESS_BIN="$BIN" \
    bash "$ROOT/.agents/skills/pr-triage/scripts/semantic-route.sh" \
    --base main --head feature)"

  route="$(jq -r '.route' <<<"$route_json")"
  local status reason
  status="$(jq -r '.status' <<<"$route_json")"
  reason="$(jq -r '.reason // ""' <<<"$route_json")"

  # Byte accounting. The router sees exactly the payload the reviewer would
  # have seen, and its own output is tracked separately. A zero-effect case
  # spends no model bytes at all.
  local router_input_bytes=0 router_output_bytes=0
  if [[ "$status" == "no-effect" ]]; then
    zero_model_cases=$((zero_model_cases + 1))
    review_input_bytes=0
  else
    router_calls=$((router_calls + 1))
    if [[ -f "$router_in_file" ]]; then
      router_input_bytes="$(tr -d ' ' < "$router_in_file")"
    else
      # Fallback modes never reach the provider, but the wrapper still built
      # the payload; the review source is the router's input in that case.
      router_input_bytes="$review_input_bytes"
    fi
    router_output_bytes="$(printf '%s' "$route_json" | wc -c | tr -d ' ')"
  fi
  case "$reason" in
    router_timeout|malformed_json_output|invalid_judgment_schema|router_process_failed|executable_not_found_or_not_executable|router_not_configured)
      fallbacks=$((fallbacks + 1))
      ;;
    *) ;;
  esac

  local total_bytes_case baseline
  total_bytes_case=$((router_input_bytes + review_input_bytes))
  # Baseline B: the same probe with no router at all.
  baseline="$review_input_bytes"

  # Route-aware arm: the payload the chosen route actually reads. `cheap`
  # trusts the classification and skips the diff, `focused` reviews the
  # residual, `deep` reviews the full raw diff, and a no-effect case reads
  # nothing. This is the model that can show a routing win, because the
  # conservative model above charges for review input on every route.
  local routed_review_input routed_total
  case "$route" in
    cheap) routed_review_input=0 ;;
    focused) routed_review_input="$t_res" ;;
    deep) routed_review_input="$t_raw" ;;
    *)
      echo "pr-routing-benchmark: unknown route name (route=$route)" >&2
      exit 2
      ;;
  esac
  if [[ "$status" == "no-effect" ]]; then
    routed_review_input=0
  fi
  routed_total=$((router_input_bytes + routed_review_input))

  local expected_rank actual_rank oracle_pass
  expected_rank="$(rank "$expected")"
  actual_rank="$(rank "$route")"
  if [[ "$expected_rank" -lt 0 || "$actual_rank" -lt 0 ]]; then
    echo "pr-routing-benchmark: unknown route name (expected=$expected route=$route)" >&2
    exit 2
  fi
  oracle_pass=false
  if [[ "$actual_rank" -ge "$expected_rank" ]]; then
    oracle_pass=true
  else
    oracle_failures=$((oracle_failures + 1))
    downgrades=$((downgrades + 1))
  fi

  local savings
  savings="$(awk -v t="$total_bytes_case" -v b="$baseline" \
    'BEGIN { if (b == 0) { print "0.000" } else { printf "%.3f", 1 - (t / b) } }')"
  local routed_savings
  routed_savings="$(awk -v t="$routed_total" -v b="$baseline" \
    'BEGIN { if (b == 0) { print "0.000" } else { printf "%.3f", 1 - (t / b) } }')"

  jq -n -c \
    --arg case "$case_name" \
    --argjson t_raw "$t_raw" \
    --argjson t_res "$t_res" \
    --arg review_source "$source" \
    --argjson router_input_bytes "$router_input_bytes" \
    --argjson router_output_bytes "$router_output_bytes" \
    --argjson review_input_bytes "$review_input_bytes" \
    --argjson total "$total_bytes_case" \
    --argjson baseline "$baseline" \
    --argjson savings "$savings" \
    --argjson routed_review "$routed_review_input" \
    --argjson routed_total "$routed_total" \
    --argjson routed_savings "$routed_savings" \
    --arg route "$route" \
    --arg expected "$expected" \
    --argjson oracle_pass "$oracle_pass" \
    '{schema_version:1, case:$case, t_raw_bytes:$t_raw, t_res_bytes:$t_res,
      review_source:$review_source, router_input_bytes:$router_input_bytes,
      router_output_bytes:$router_output_bytes, review_input_bytes:$review_input_bytes,
      total_model_input_bytes:$total, baseline_model_input_bytes:$baseline,
      input_savings_ratio:$savings, routed_review_input_bytes:$routed_review,
      routed_total_model_input_bytes:$routed_total,
      routed_savings_ratio:$routed_savings, route:$route,
      expected_min_route:$expected, oracle_pass:$oracle_pass}'

  total_bytes=$((total_bytes + total_bytes_case))
  baseline_total=$((baseline_total + baseline))
  routed_total_bytes=$((routed_total_bytes + routed_total))
  savings_sum="$(awk -v s="$savings_sum" -v v="$savings" 'BEGIN { printf "%.6f", s + v }')"
  routed_savings_sum="$(awk -v s="$routed_savings_sum" -v v="$routed_savings" 'BEGIN { printf "%.6f", s + v }')"
  printf '%s\n' "$savings" >> "$median_list"
  printf '%s\n' "$routed_savings" >> "$routed_median_list"
  case_count=$((case_count + 1))
}

if [[ -d "$FIXTURES" && -f "$FIXTURES/manifest.json" ]]; then
  case_total="$(jq -r '.cases | length' "$FIXTURES/manifest.json")"
  for index in $(seq 0 $((case_total - 1))); do
    case_name="$(jq -r ".cases[$index].case" "$FIXTURES/manifest.json")"
    expected="$(jq -r ".cases[$index].expected_min_route" "$FIXTURES/manifest.json")"
    repo="$work/$case_name"
    materialize "$FIXTURES/$case_name" "$repo"
    run_case "$case_name" "$expected" "$repo"
  done
else
  count="${COUNT:-10}"
  for sha in $(git -C "$ROOT" log --first-parent --pretty=%H -n "$count"); do
    parent="$(git -C "$ROOT" rev-parse --verify --quiet "${sha}^")" || continue
    repo="$work/$sha"
    mkdir -p "$repo"
    (
      cd "$repo"
      git init -q -b main
      git config user.email "bench@example.com"
      git config user.name "Bench"
      git fetch -q "$ROOT" "$parent" 2>/dev/null || true
      git fetch -q "$ROOT" "$sha"
      git checkout -q FETCH_HEAD
      git update-ref refs/heads/main "$parent"
      git checkout -q -b feature
      git reset -q --hard "$sha"
    ) || continue
    run_case "$sha" "focused" "$repo"
  done
fi

if [[ "$case_count" -eq 0 ]]; then
  echo "pr-routing-benchmark: no cases were materialized" >&2
  exit 2
fi

median="$(sort -n "$median_list" | awk '{ a[NR] = $1 } END { if (NR % 2 == 1) { print a[(NR + 1) / 2] } else { printf "%.3f", (a[NR / 2] + a[NR / 2 + 1]) / 2 } }')"
p95="$(sort -n "$median_list" | awk '{ a[NR] = $1 } END { idx = int(NR * 0.95); if (idx < 1) { idx = 1 } print a[idx] }')"
mean_savings="$(awk -v s="$savings_sum" -v n="$case_count" 'BEGIN { printf "%.3f", s / n }')"
routed_median="$(sort -n "$routed_median_list" | awk '{ a[NR] = $1 } END { if (NR % 2 == 1) { print a[(NR + 1) / 2] } else { printf "%.3f", (a[NR / 2] + a[NR / 2 + 1]) / 2 } }')"
routed_mean_savings="$(awk -v s="$routed_savings_sum" -v n="$case_count" 'BEGIN { printf "%.3f", s / n }')"

pct() { awk -v n="$1" -v d="$2" 'BEGIN { if (d == 0) { print "0.0" } else { printf "%.1f", (n / d) * 100 } }'; }

{
  echo "cases=$case_count router=$ROUTER"
  echo "total_model_input_bytes=$total_bytes baseline_model_input_bytes=$baseline_total"
  echo "mean_savings_ratio=$mean_savings median_savings_ratio=$median p95_savings_ratio=$p95"
  echo "routed_total_model_input_bytes=$routed_total_bytes"
  echo "routed_mean_savings_ratio=$routed_mean_savings routed_median_savings_ratio=$routed_median"
  echo "reduced_vs_baseline_b=$(pct "$reduced" "$case_count") no_go_vs_baseline_b=$(pct "$no_go" "$case_count")"
  echo "router_calls=$router_calls router_calls_per_case=$(awk -v c="$router_calls" -v n="$case_count" 'BEGIN { printf "%.2f", c / n }')"
  echo "zero_model_cases=$zero_model_cases"
  echo "seeded_oracle_failures=$oracle_failures downgrades=$downgrades"
  echo "timeout_or_invalid_fallbacks=$fallbacks"
  if [[ "$total_bytes" -lt "$baseline_total" && "$oracle_failures" -eq 0 ]]; then
    echo "VERDICT go: total model input is lower than Baseline B with zero seeded escalation regressions"
  else
    echo "VERDICT no-go: total model input is not lower than Baseline B, or seeded escalations regressed"
  fi
  # Route-aware verdict. The conservative VERDICT above stays as the
  # routing-is-pure-overhead upper bound; this one answers the question the
  # router is actually for: does reading only what the route selects cost less
  # than reading the review payload unconditionally?
  if [[ "$routed_total_bytes" -lt "$baseline_total" && "$oracle_failures" -eq 0 ]]; then
    echo "ROUTED_VERDICT go: routed total is lower than Baseline B with zero escalation regressions"
  else
    echo "ROUTED_VERDICT no-go: routed total is not lower than Baseline B, or seeded escalations regressed"
  fi
  echo "FINDINGS: $oracle_failures"
} >&2

# A downgrade is a release-blocking regression, so it fails the benchmark
# rather than merely appearing in the summary.
if [[ "$downgrades" -gt 0 ]]; then
  exit 1
fi

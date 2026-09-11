#!/usr/bin/env bash
# Classify all checks and commit statuses for a PR's current head commit.
#
# Usage: checks.sh <pr-number>
# Output (TSV): <PASS|FAIL|PENDING|SKIP> name source detail
# Summary line: summary<TAB><PASS|FAIL|PENDING|NONE|UNKNOWN>
# Exit: 0 no FAIL/PENDING, 1 FAIL, 2 PENDING, 3 cannot read.
set -euo pipefail

pr="${1:?usage: checks.sh <pr-number>}"

head=$(gh pr view "$pr" --json headRefOid --jq .headRefOid 2>/dev/null || true)
if [ -z "$head" ]; then
  printf 'summary\tUNKNOWN\n'
  exit 3
fi

runs=$(gh api "repos/{owner}/{repo}/commits/$head/check-runs?per_page=100" \
  --jq '.check_runs[] | [.name, .status, (.conclusion // ""), (.app.slug // "")] | @tsv' 2>/dev/null || true)
statuses=$(gh api "repos/{owner}/{repo}/commits/$head/status" \
  --jq '.statuses[] | [.context, .state, "", "status"] | @tsv' 2>/dev/null || true)

rows=$(printf '%s\n%s\n' "$runs" "$statuses" | sed '/^$/d' | awk -F'\t' '
  function emit(v, n, src, d) { printf "%s\t%s\t%s\t%s\n", v, n, src, d }
  {
    name=$1; status=$2; conclusion=$3; app=$4
    if (app == "status") {
      if (status == "success") emit("PASS", name, "status", status)
      else if (status == "pending") emit("PENDING", name, "status", status)
      else emit("FAIL", name, "status", status)
      next
    }
    if (status != "completed") { emit("PENDING", name, "check", status); next }
    if (conclusion == "success") emit("PASS", name, "check", conclusion)
    else if (conclusion == "skipped" || conclusion == "neutral") emit("SKIP", name, "check", conclusion)
    else emit("FAIL", name, "check", conclusion)
  }
')

if [ -z "$rows" ]; then
  printf 'summary\tNONE\n'
  exit 0
fi
printf '%s\n' "$rows"

if printf '%s\n' "$rows" | grep -q '^FAIL'; then
  printf 'summary\tFAIL\n'
  exit 1
fi
if printf '%s\n' "$rows" | grep -q '^PENDING'; then
  printf 'summary\tPENDING\n'
  exit 2
fi
printf 'summary\tPASS\n'

#!/usr/bin/env bash
# Deterministic `do-harness` stub for the pr-triage walkthrough.
#
# Answers only `pr no-effect <n> --format json`. The report mode is chosen by
# FAKE_DO_HARNESS_MODE; without it, PR 7 defaults to a real tree change
# (TREE_EFFECT) and PR 8 to an empty commit with a verified base
# (NO_TREE_EFFECT). The no-effect.sh wrapper re-reads both OIDs via fake gh, so
# the no-tree-effect case also passes the movement check.
set -euo pipefail

[ "${1:-}" = "pr" ] && [ "${2:-}" = "no-effect" ] || {
  echo "fake do-harness: only 'pr no-effect <n> --format json' supported" >&2
  exit 1
}
pr="${3:-}"
mode="${FAKE_DO_HARNESS_MODE:-}"
if [ -z "$mode" ]; then
  case "$pr" in
    7) mode=tree-effect ;;
    8) mode=no-tree-effect ;;
    *) echo "fake do-harness: unknown PR $pr" >&2; exit 1 ;;
  esac
fi
head_sha=$(git rev-parse "refs/pull/$pr/head")
base_sha=$(git rev-parse refs/heads/main)
report_base_sha="$base_sha"
report_head_sha="$head_sha"
verdict=TREE_EFFECT
closure=false
base_verified=true
case "$mode" in
  tree-effect) verdict=TREE_EFFECT; closure=false ;;
  no-tree-effect) verdict=NO_TREE_EFFECT; closure=true ;;
  unverified-base) verdict=NO_TREE_EFFECT; closure=false; base_verified=false ;;
  missing-oids) verdict=NO_TREE_EFFECT; closure=true; report_base_sha=""; report_head_sha="" ;;
  unknown-verdict) verdict=UNKNOWN; closure=false ;;
  moved-head) verdict=NO_TREE_EFFECT; closure=true; report_head_sha="ffffffffffffffffffffffffffffffffffffffff" ;;
  *) echo "fake do-harness: unknown mode $mode" >&2; exit 1 ;;
esac
{
  printf '{\n  "schema_version": 2,\n  "mode": "pr",\n  "pr": %s,\n' "$pr"
  printf '  "base": "main",\n  "head": "%s",\n' "$head_sha"
  printf '  "effective_change": true,\n  "method": "git",\n'
  printf '  "merge_impact": {\n    "verdict": "%s",\n' "$verdict"
  printf '    "method": "merge-tree",\n    "base": "main",\n    "head": "%s",\n' "$head_sha"
  printf '    "base_sha": "%s",\n    "head_sha": "%s",\n' "$report_base_sha" "$report_head_sha"
  printf '    "result_tree": "%s",\n    "base_verified": %s,\n' "$base_sha" "$base_verified"
  printf '    "evidence": "result tree equals base tree",\n    "reason": null,\n    "warnings": []\n  },\n'
  printf '  "closure_recommendation": %s\n}\n' "$closure"
} >&1

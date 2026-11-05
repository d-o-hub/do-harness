#!/usr/bin/env bash
# Report whether a PR introduces any effective change, using the harness's
# prospective merge-impact evidence. Only a proven, base-verified no-tree-effect
# verdict recommends closure; the historical branch diff is never treated as
# merge-impact proof.
#
# Usage: no-effect.sh <pr-number>
# Output (TSV): <no-effect|has-effect|unknown> <pr> <head_sha>
# Exit: 0 determined, 3 unknown (caller must not close).
set -euo pipefail

pr="${1:?usage: no-effect.sh <pr-number>}"

if ! command -v jq >/dev/null 2>&1; then
  printf 'unknown\t%s\t\n' "$pr"
  exit 3
fi

# Resolve the harness binary (git-hook order; mirror do-harness-optional.md).
harness_bin="${DO_HARNESS_BIN:-}"
if [ -z "$harness_bin" ] && command -v do-harness >/dev/null 2>&1; then
  harness_bin="$(command -v do-harness)"
fi
if [ -z "$harness_bin" ]; then
  printf 'unknown\t%s\t\n' "$pr"
  exit 3
fi

# The harness emits the merge-impact report; without it there is no second
# merge-tree algorithm to fall back to.
report="$("$harness_bin" pr no-effect "$pr" --format json 2>/dev/null)" || {
  printf 'unknown\t%s\t\n' "$pr"
  exit 3
}

extract() { # <jq-filter> — empty on missing/ill-formed output
  printf '%s' "$report" | jq -r "$1 // empty" 2>/dev/null || true
}

verdict=$(extract '.merge_impact.verdict')
head_sha=$(extract '.merge_impact.head_sha')

# A proven tree change is reported as has-effect.
if [ "$verdict" = "TREE_EFFECT" ]; then
  printf 'has-effect\t%s\t%s\n' "$pr" "$head_sha"
  exit 0
fi

# Closure requires the full no-tree-effect evidence: verdict, verified base, the
# derived recommendation, and resolved OIDs.
closure=$(extract '.closure_recommendation')
base_verified=$(extract '.merge_impact.base_verified')
base_sha=$(extract '.merge_impact.base_sha')
if [ "$verdict" != "NO_TREE_EFFECT" ] || [ "$closure" != "true" ] || \
   [ "$base_verified" != "true" ] || [ -z "$base_sha" ] || [ -z "$head_sha" ]; then
  printf 'unknown\t%s\t%s\n' "$pr" "$head_sha"
  exit 3
fi

# Re-read both OIDs immediately before signalling closure; if either moved, the
# evidence no longer belongs to the exact revisions.
read -r cur_base cur_head < <(gh pr view "$pr" --json baseRefOid,headRefOid \
  --jq '[.baseRefOid, .headRefOid] | @tsv') || {
  printf 'unknown\t%s\t%s\n' "$pr" "$head_sha"
  exit 3
}
if [ "$cur_base" != "$base_sha" ] || [ "$cur_head" != "$head_sha" ]; then
  printf 'unknown\t%s\t%s\n' "$pr" "$head_sha"
  exit 3
fi

printf 'no-effect\t%s\t%s\n' "$pr" "$head_sha"

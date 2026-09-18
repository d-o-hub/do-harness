#!/usr/bin/env bash
# List open, non-draft PRs in merge order: stacked PRs first, then oldest-first.
# Output (TSV): number base head head_sha merge_state author
set -euo pipefail

jq_filter=$(cat <<'JQ'
. as $all
| ($all | map(.headRefName)) as $heads
| $all
| map(select(.isDraft | not))
| map(select(([.labels[].name] | any(. == "hold" or . == "wip" or . == "do-not-merge")) | not))
| sort_by((if (.baseRefName as $b | ($heads | index($b))) != null then 0 else 1 end), .createdAt)
| .[]
| [.number, .baseRefName, .headRefName, .headRefOid, .mergeStateStatus, (.author.login // "")] | @tsv
JQ
)

gh pr list --state open --limit 200 \
  --json number,isDraft,baseRefName,headRefName,headRefOid,mergeStateStatus,author,createdAt,labels \
  --jq "$jq_filter"

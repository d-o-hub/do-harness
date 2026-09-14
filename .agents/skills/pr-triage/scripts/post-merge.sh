#!/usr/bin/env bash
# Verify the default-branch runs for a merged PR's commit with bounded polling.
#
# Usage: post-merge.sh <pr-number|merge-sha> [timeout-seconds]
# Output (TSV): <PASS|FAIL|PENDING|SKIP> workflow conclusion
# Summary line: summary<TAB><PASS|FAIL|PENDING|NONE>
# Exit: 0 all runs success, 1 any failure, 2 pending or not yet registered at
# the deadline, 3 cannot resolve the merge commit.
#
# The wait is bounded in this script (default 900 seconds, 15 second interval;
# 1 second when the timeout is smaller) so the agent never sleeps between
# calls. An empty run list means the runs are not registered yet, never done.
set -euo pipefail

target="${1:?usage: post-merge.sh <pr-number|merge-sha> [timeout-seconds]}"
timeout_secs="${2:-900}"
case "$timeout_secs" in
  ''|*[!0-9]*)
    printf 'post-merge.sh: timeout must be an integer number of seconds\n' >&2
    exit 3
    ;;
esac

sha="$target"
case "$target" in
  *[!0-9]*) ;;
  *)
    sha=$(gh pr view "$target" --json mergeCommit --jq '.mergeCommit.oid // empty' 2>/dev/null || true)
    if [ -z "$sha" ]; then
      printf 'post-merge.sh: PR %s has no merge commit (not merged?)\n' "$target" >&2
      exit 3
    fi
    ;;
esac

interval=15
if [ "$timeout_secs" -lt 15 ]; then
  interval=1
fi
deadline=$(( $(date +%s) + timeout_secs ))

rows=""
code=2
while :; do
  rows=$(gh run list --commit "$sha" --limit 50 \
    --json workflowName,status,conclusion \
    --jq '.[] | [(if .status != "completed" then "PENDING" elif .conclusion == "success" then "PASS" elif (.conclusion == "skipped" or .conclusion == "neutral") then "SKIP" else "FAIL" end), .workflowName, (.conclusion // .status)] | @tsv' \
    2>/dev/null || true)

  if [ -n "$rows" ] && printf '%s\n' "$rows" | grep -q '^FAIL'; then
    code=1
    break
  fi
  if [ -n "$rows" ] && ! printf '%s\n' "$rows" | grep -q '^PENDING'; then
    code=0
    break
  fi

  now=$(date +%s)
  if [ "$now" -ge "$deadline" ]; then
    code=2
    break
  fi
  sleep "$interval"
done

if [ -z "$rows" ]; then
  printf 'summary\tNONE\n'
  exit 2
fi
printf '%s\n' "$rows"
case "$code" in
  0) printf 'summary\tPASS\n' ;;
  1) printf 'summary\tFAIL\n' ;;
  *) printf 'summary\tPENDING\n' ;;
esac
exit "$code"

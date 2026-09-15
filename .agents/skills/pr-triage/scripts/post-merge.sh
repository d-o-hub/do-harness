#!/usr/bin/env bash
# Verify the default-branch runs for a merged PR's commit with bounded polling.
#
# Usage: post-merge.sh <pr-number|merge-sha> [timeout-seconds] [--events-url URL]
# Output (TSV): <PASS|FAIL|PENDING|SKIP> workflow conclusion
# Summary line: summary<TAB><PASS|FAIL|PENDING|NONE>
# Exit: 0 all runs success, 1 any failure, 2 pending or not yet registered at
# the deadline, 3 cannot resolve the merge commit.
#
# The wait is bounded in this script (default 900 seconds, 15 second interval;
# 1 second when the timeout is smaller) so the agent never sleeps between
# calls. An empty run list means the runs are not registered yet, never done.
#
# Fast path: when a URL resolves (--events-url or PR_TRIAGE_EVENTS_URL) and a
# webhook receiver answers /health with "ok", the wait long-polls
# /wait?since=<cursor>&events=workflow_run instead of sleeping; it
# re-classifies after every wake and after a safety chunk without events
# (PR_TRIAGE_EVENT_CHUNK, default 300s, clamped 5-600). Deliveries only
# shorten the wait - the run classification stays authoritative, and a
# missing receiver silently degrades to the polling loop.
set -euo pipefail

target="${1:?usage: post-merge.sh <pr-number|merge-sha> [timeout-seconds] [--events-url URL]}"
shift || true

timeout_secs=900
events_url="${PR_TRIAGE_EVENTS_URL:-}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --events-url)
      if [ "$#" -lt 2 ]; then
        printf 'post-merge.sh: --events-url needs a URL\n' >&2
        exit 3
      fi
      events_url="$2"
      shift 2
      ;;
    *)
      case "$1" in
        ''|*[!0-9]*)
          printf 'post-merge.sh: timeout must be an integer number of seconds\n' >&2
          exit 3
          ;;
        *)
          timeout_secs="$1"
          shift
          ;;
      esac
      ;;
  esac
done

event_chunk="${PR_TRIAGE_EVENT_CHUNK:-300}"
case "$event_chunk" in
  ''|*[!0-9]*) event_chunk=300 ;;
esac
if [ "$event_chunk" -lt 5 ]; then
  event_chunk=5
fi
if [ "$event_chunk" -gt 600 ]; then
  event_chunk=600
fi

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

# Arm the webhook fast path only for a bounded wait with a URL; a missing
# receiver, curl, or "ok" body falls back to polling without failing the run.
events_armed=0
event_cursor=0
if [ "$timeout_secs" -gt 0 ] && [ -n "$events_url" ]; then
  health=""
  if command -v curl >/dev/null 2>&1; then
    health=$(curl -s --max-time 2 "$events_url/health" 2>/dev/null || true)
  fi
  if [ "$health" = "ok" ]; then
    events_armed=1
    printf 'post-merge.sh: event wait armed (url=%s cursor=%s chunk=%ss)\n' \
      "$events_url" "$event_cursor" "$event_chunk" >&2
  else
    printf 'post-merge.sh: event wait unavailable; falling back to polling\n' >&2
  fi
fi

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
  if [ "$events_armed" -eq 1 ]; then
    wait_for="$event_chunk"
    remaining=$(( deadline - now ))
    if [ "$remaining" -lt "$wait_for" ]; then
      wait_for="$remaining"
    fi
    if [ "$wait_for" -lt 1 ]; then
      wait_for=1
    fi
    curl_rc=0
    resp=$(curl -s --max-time $(( wait_for + 5 )) \
      "$events_url/wait?since=$event_cursor&timeout=$wait_for&events=workflow_run" 2>/dev/null) || curl_rc=$?
    new_cursor=$(printf '%s' "$resp" | sed -n 's/.*"cursor":\([0-9]*\).*/\1/p')
    if [ "$curl_rc" -ne 0 ] || [ -z "$new_cursor" ]; then
      events_armed=0
      printf 'post-merge.sh: event wait unavailable; falling back to polling\n' >&2
    else
      event_cursor="$new_cursor"
      case "$resp" in
        *'"woke":true'*)
          printf 'post-merge.sh: event wake (cursor=%s)\n' "$event_cursor" >&2
          ;;
      esac
      # The wait consumed the pacing time (event or safety chunk): re-classify
      # now instead of sleeping again.
      continue
    fi
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

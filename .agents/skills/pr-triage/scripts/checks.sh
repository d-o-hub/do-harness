#!/usr/bin/env bash
# Classify all checks and commit statuses for a PR's current head commit.
#
# Usage: checks.sh <pr-number> [--wait [seconds]] [--events-url URL]
# Output (TSV): <PASS|FAIL|PENDING|SKIP> name source detail
# Summary line: summary<TAB><PASS|FAIL|PENDING|NONE|UNKNOWN>
# Exit: 0 no FAIL/PENDING, 1 FAIL, 2 PENDING, 3 cannot read.
#
# `--wait` polls every 30 seconds until no check is PENDING or the deadline
# (default 1200 seconds) passes. The wait is bounded in this script so the
# agent makes one call instead of sleeping between calls; an empty check list
# is NONE, never a pass while waiting.
#
# Fast path: when a URL resolves (--events-url or PR_TRIAGE_EVENTS_URL) and a
# webhook receiver answers /health with "ok", the wait long-polls
# /wait?since=<cursor> instead of sleeping; it re-classifies after every wake
# and after a safety chunk without events (PR_TRIAGE_EVENT_CHUNK, default
# 300s, clamped 5-600). Deliveries only shorten the wait - the GitHub
# classification stays authoritative, and a missing receiver silently
# degrades to the polling loop.
set -euo pipefail

pr="${1:?usage: checks.sh <pr-number> [--wait [seconds]] [--events-url URL]}"
shift || true

wait_secs=0
events_url="${PR_TRIAGE_EVENTS_URL:-}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --wait)
      if [ "$#" -ge 2 ] && [ "${2#--}" = "$2" ]; then
        wait_secs="$2"
        shift 2
      else
        wait_secs=1200
        shift
      fi
      ;;
    --events-url)
      if [ "$#" -lt 2 ]; then
        printf 'checks.sh: --events-url needs a URL\n' >&2
        exit 3
      fi
      events_url="$2"
      shift 2
      ;;
    *)
      printf 'checks.sh: unknown argument %s\n' "$1" >&2
      exit 3
      ;;
  esac
done
case "$wait_secs" in
  ''|*[!0-9]*)
    printf 'checks.sh: --wait needs an integer number of seconds\n' >&2
    exit 3
    ;;
esac

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

# Prints the classified rows plus the summary line; returns the verdict code.
classify() {
  local head runs statuses rows
  head=$(gh pr view "$pr" --json headRefOid --jq .headRefOid 2>/dev/null || true)
  if [ -z "$head" ]; then
    printf 'summary\tUNKNOWN\n'
    return 3
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
    return 0
  fi
  printf '%s\n' "$rows"

  if printf '%s\n' "$rows" | grep -q '^FAIL'; then
    printf 'summary\tFAIL\n'
    return 1
  fi
  if printf '%s\n' "$rows" | grep -q '^PENDING'; then
    printf 'summary\tPENDING\n'
    return 2
  fi
  printf 'summary\tPASS\n'
  return 0
}

# Arm the webhook fast path only for a bounded wait with a URL; a missing
# receiver, curl, or "ok" body falls back to polling without failing the run.
events_armed=0
event_cursor=0
if [ "$wait_secs" -gt 0 ] && [ -n "$events_url" ]; then
  health=""
  if command -v curl >/dev/null 2>&1; then
    health=$(curl -s --max-time 2 "$events_url/health" 2>/dev/null || true)
  fi
  if [ "$health" = "ok" ]; then
    events_armed=1
    printf 'checks.sh: event wait armed (url=%s cursor=%s chunk=%ss)\n' \
      "$events_url" "$event_cursor" "$event_chunk" >&2
  else
    printf 'checks.sh: event wait unavailable; falling back to polling\n' >&2
  fi
fi

deadline=$(( $(date +%s) + wait_secs ))
interval=30
if [ "$wait_secs" -lt 30 ]; then
  interval=1
fi
result=""
code=0
while :; do
  code=0
  result=$(classify) || code=$?
  if [ "$code" -ne 2 ]; then
    break
  fi
  now=$(date +%s)
  if [ "$now" -ge "$deadline" ]; then
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
      "$events_url/wait?since=$event_cursor&timeout=$wait_for" 2>/dev/null) || curl_rc=$?
    new_cursor=$(printf '%s' "$resp" | sed -n 's/.*"cursor":\([0-9]*\).*/\1/p')
    if [ "$curl_rc" -ne 0 ] || [ -z "$new_cursor" ]; then
      events_armed=0
      printf 'checks.sh: event wait unavailable; falling back to polling\n' >&2
    else
      event_cursor="$new_cursor"
      case "$resp" in
        *'"woke":true'*)
          printf 'checks.sh: event wake (cursor=%s)\n' "$event_cursor" >&2
          ;;
      esac
      # The wait consumed the pacing time (event or safety chunk): re-classify
      # now instead of sleeping again.
      continue
    fi
  fi
  sleep "$interval"
done

printf '%s\n' "$result"
exit "$code"

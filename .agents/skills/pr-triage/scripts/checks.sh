#!/usr/bin/env bash
# Thin wrapper over `do-harness pr checks`: the Rust CLI is the single source
# of CI truth. This script only paces the bounded wait and renders the legacy
# TSV rows, so the skill never keeps a second classifier.
#
# Usage: checks.sh <pr-number> [--wait [seconds]] [--events-url URL]
# Output (TSV): <PASS|FAIL|PENDING|SKIP|UNKNOWN> name source detail
# Summary line: summary<TAB><PASS|FAIL|PENDING|UNKNOWN>
# Exit: 0 PASS, 1 FAIL, 2 PENDING, 3 UNKNOWN or unreadable.
#
# Fail closed: when the harness binary is unavailable, or the classifier cannot
# read the check state, the verdict is UNKNOWN and the exit is nonzero. There
# is no shell fallback parser.
#
# `--wait` polls every 30 seconds until no check is PENDING or the deadline
# (default 1200 seconds) passes. The wait is bounded in this script so the
# agent makes one call instead of sleeping between calls.
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

# Resolve the harness binary exactly as the git hooks do.
harness_bin="${DO_HARNESS_BIN:-}"
if [ -z "$harness_bin" ] && command -v do-harness >/dev/null 2>&1; then
  harness_bin="$(command -v do-harness)"
fi
if [ -z "$harness_bin" ]; then
  repo_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
  for candidate in "$repo_root/target/release/do-harness" \
    "$repo_root/target/release/do-harness.exe"; do
    [ -x "$candidate" ] && harness_bin="$candidate" && break
  done
fi
if [ -z "$harness_bin" ] || ! "$harness_bin" pr checks --help >/dev/null 2>&1; then
  printf 'checks.sh: do-harness is required to classify CI; refusing to guess\n' >&2
  printf 'summary\tUNKNOWN\n'
  exit 3
fi
if ! command -v jq >/dev/null 2>&1; then
  printf 'checks.sh: jq is required to render the CI report\n' >&2
  printf 'summary\tUNKNOWN\n'
  exit 3
fi

# Prints the classified rows plus the summary line; returns the verdict code.
classify() {
  local json verdict
  json=$("$harness_bin" pr checks "$pr" --format json 2>/dev/null) || true
  if [ -z "$json" ]; then
    printf 'summary\tUNKNOWN\n'
    return 3
  fi
  verdict=$(printf '%s' "$json" | jq -r '.verdict // "UNKNOWN"')
  printf '%s' "$json" |
    jq -r '.checks[] | [.verdict, .name, .source, (.conclusion // .state)] | @tsv'
  printf 'summary\t%s\n' "$verdict"
  case "$verdict" in
    PASS) return 0 ;;
    FAIL) return 1 ;;
    PENDING) return 2 ;;
    *) return 3 ;;
  esac
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

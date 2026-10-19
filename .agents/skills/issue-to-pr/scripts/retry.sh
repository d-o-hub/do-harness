#!/usr/bin/env bash
# Retry a `gh` (or any) command across transient GitHub API failures.
#
# Usage: retry.sh [--attempts N] [--delay S] -- <command...>
# Defaults: 5 attempts, 15 second fixed delay. The delay is bounded in this
# script so the agent makes one call instead of sleeping between calls.
#
# Retries only transient signatures (HTTP 502/503/504, GraphQL internal
# errors, connection resets, HTTP 429 rate limits). Deterministic failures
# (HTTP 400/401/403/404/422, GraphQL deprecation responses such as the
# Projects-classic projectCards error, "already exists", rerun-forbidden)
# return immediately with the command's exit code — retrying those can
# duplicate side effects (e.g. a second PR) without ever succeeding.
# On success the command's stdout is printed and attempt diagnostics go to
# stderr; on final failure both captured streams are replayed (stdout, then
# stderr) so the cause survives. Exit: the final attempt's exit code.
set -euo pipefail

attempts=5
delay=15
while [ $# -gt 0 ]; do
  case "$1" in
    --attempts)
      attempts="${2:?--attempts requires a value}"
      shift 2
      ;;
    --delay)
      delay="${2:?--delay requires a value}"
      shift 2
      ;;
    --)
      shift
      break
      ;;
    *)
      printf 'retry.sh: unknown argument: %s (see header)\n' "$1" >&2
      exit 2
      ;;
  esac
done
case "$attempts" in
  '' | *[!0-9]* | 0)
    printf 'retry.sh: --attempts needs a positive integer\n' >&2
    exit 2
    ;;
esac
case "$delay" in
  '' | *[!0-9]*)
    printf 'retry.sh: --delay needs an integer number of seconds\n' >&2
    exit 2
    ;;
esac
if [ $# -eq 0 ]; then
  printf 'retry.sh: no command given (see header)\n' >&2
  exit 2
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

attempt=1
while true; do
  set +e
  "$@" >"$tmp/stdout" 2>"$tmp/stderr"
  code=$?
  set -e
  if [ "$code" -eq 0 ]; then
    cat "$tmp/stdout"
    exit 0
  fi
  output="$(cat "$tmp/stdout" "$tmp/stderr")"
  transient=0
  case "$output" in
    *'HTTP 502'* | *'HTTP 503'* | *'HTTP 504'* | *'Bad Gateway'* | *'Service Unavailable'* | *'Gateway Time-out'* | *'Gateway Timeout'*)
      transient=1
      ;;
    *'Something went wrong while executing your query'* | *'Failed to connect'* | *'Connection reset'* | *'connection refused'* | *'timed out'* | *'HTTP 429'* | *'rate limit'* | *'Rate limit'*)
      transient=1
      ;;
  esac
  if [ "$transient" -eq 0 ] || [ "$attempt" -ge "$attempts" ]; then
    printf 'retry.sh: giving up after %s attempt(s) (exit %s)\n' "$attempt" "$code" >&2
    cat "$tmp/stdout"
    cat "$tmp/stderr" >&2
    exit "$code"
  fi
  printf 'retry.sh: attempt %s/%s failed transiently (exit %s); retrying in %ss\n' \
    "$attempt" "$attempts" "$code" "$delay" >&2
  sleep "$delay"
  attempt=$((attempt + 1))
done

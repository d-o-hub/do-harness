#!/usr/bin/env bash
# Merge one PR through the authorizing receipt boundary (#466).
#
# Usage: merge.sh PR REPORT_FILE
#
# REPORT_FILE is the full `pr ready --format json` document that authorized this
# head: `"$DO_HARNESS_BIN" pr ready PR --format json > "$receipt_file"`.
#
# The receipt is never trusted as a token. The script:
#   1. runs `pr ready PR --expected-receipt FILE --format json`, which recomputes
#      the whole snapshot and accepts the file only when nothing moved and the
#      60-second lifetime has not elapsed;
#   2. requires exit 0 and authorization.decision == MERGE_ALLOWED;
#   3. issues exactly `gh pr merge PR --squash --match-head-commit HEAD_SHA`.
#
# Exit: 0 merged, 1 refused/blocked, 2 cannot be judged (unknown or bad receipt),
# 3 no usable do-harness binary.
#
# This script never uses --auto, --admin, or a weaker retry. It does not clear a
# draft, a hold label, or a reviewer's decision: those are the author's calls.
set -euo pipefail

pr="${1:?usage: merge.sh <pr-number> <report-file>}"
report="${2:?usage: merge.sh <pr-number> <report-file>}"

fail() {
  printf 'merge: %s\n' "$1" >&2
  exit "${2:-1}"
}

[ -f "$report" ] || fail "report file '$report' not found" 2

bin="${DO_HARNESS_BIN:-}"
if [ -z "$bin" ]; then
  if command -v do-harness >/dev/null 2>&1; then
    bin="$(command -v do-harness)"
  else
    fail "no do-harness binary; set DO_HARNESS_BIN (refusing to merge without the typed authorization)" 3
  fi
fi
[ -x "$bin" ] || fail "do-harness binary '$bin' is not executable" 3

set +e
out="$("$bin" pr ready "$pr" --expected-receipt "$report" --format json 2>"/tmp/merge-stderr.$$")"
code=$?
set -e
stderr="$(cat "/tmp/merge-stderr.$$" 2>/dev/null || true)"
rm -f "/tmp/merge-stderr.$$"

if [ "$code" -ne 0 ]; then
  printf '%s\n' "$stderr" >&2
  case "$code" in
    1) fail "PR $pr is blocked; not merging" 1 ;;
    2) fail "PR $pr cannot be judged (unknown state or rejected receipt); not merging" 2 ;;
    *) fail "pr ready exited $code; not merging" 2 ;;
  esac
fi

parsed="$(printf '%s' "$out" | jq -r '[.authorization.decision // "", .authorization.receipt.head_ref_oid // "", .authorization.receipt.base_ref_oid // ""] | @tsv' 2>/dev/null || true)"
[ -n "$parsed" ] || fail "the readiness report could not be parsed; not merging" 2
IFS="$(printf '\t')" read -r decision head base <<EOF
$parsed
EOF

case "$decision" in
  MERGE_ALLOWED) ;;
  BLOCKED)
    printf '%s' "$out" | jq -r '.authorization.reasons[]? | "  - [\(.code)] \(.message)"' >&2 || true
    fail "PR $pr is blocked; not merging" 1
    ;;
  UNKNOWN)
    printf '%s' "$out" | jq -r '.authorization.reasons[]? | "  - [\(.code)] \(.message)"' >&2 || true
    fail "PR $pr cannot be judged; not merging" 2
    ;;
  *) fail "authorization is '${decision:-missing}', not MERGE_ALLOWED" 2 ;;
esac
case "$head" in
  '' | null) fail "authorization carried no head SHA" 2 ;;
esac
case "$base" in
  '' | null) fail "authorization carried no base SHA" 2 ;;
esac

# Re-read the live refs immediately before merging: the receipt proved the
# snapshot it sealed, not the head that exists now.
live="$(gh pr view "$pr" --json headRefOid,state,isDraft 2>/dev/null || true)"
live_head="$(printf '%s' "$live" | jq -r '.headRefOid // empty' 2>/dev/null || true)"
live_state="$(printf '%s' "$live" | jq -r '.state // empty' 2>/dev/null || true)"
# `false // empty` is empty in jq, so the flag is stringified, not coerced.
live_draft="$(printf '%s' "$live" | jq -r 'if .isDraft == null then "" else (.isDraft|tostring) end' 2>/dev/null || true)"

[ -n "$live_head" ] || fail "could not re-read the live head SHA; refusing to merge" 2
[ "$live_head" = "$head" ] || fail "head moved from $head to $live_head; re-authorize" 2
[ "$live_state" = "OPEN" ] || fail "PR $pr is '$live_state', not OPEN" 1
[ "$live_draft" = "false" ] || fail "PR $pr is a draft (isDraft=$live_draft)" 1

printf 'merge: authorizing PR %s at %s (base %s)\n' "$pr" "$head" "$base" >&2
exec gh pr merge "$pr" --squash --match-head-commit "$head"

#!/usr/bin/env bash
# The pre-merge stop: refuse anything that is not an open, non-draft, unheld PR.
#
# Usage: premerge.sh <pr-number>
# Output (TSV): <MERGEABLE|DRAFT|HELD|NOT-OPEN|NOT-MERGEABLE|BEHIND><TAB><detail>
# Summary line: summary<TAB><MERGEABLE|DRAFT|HELD|NOT-OPEN|NOT-MERGEABLE|BEHIND>
# Exit: 0 the PR may be merged; 1 it must not (draft, held, not open, not
#       mergeable, or behind its base);
#       2 the PR cannot be read, or the API did not return a field this check
#       needs (fail closed); 3 usage error.
#
# Why the sweep cares: a draft is the author's "not ready" signal and a
# `hold`/`wip`/`do-not-merge` label is the same statement for reviewers, but
# both were withdrawn by hand once and merged seconds later (#345, 2026-10-19:
# `gh pr ready`, then a merge ten seconds after it). Un-drafting is the author's
# decision, so this script makes the mistake unreachable for a sweep: it refuses
# the draft instead of clearing the flag, and only explicit human approval
# recorded in the session lifts the stop. Run it immediately before the merge
# command, on the same head that was validated.
set -euo pipefail

pr="${1:?usage: premerge.sh <pr-number>}"

case "$pr" in
  '' | *[!0-9]*)
    printf 'premerge.sh: PR must be a number, got %s\n' "$pr" >&2
    exit 3
    ;;
esac

json="$(gh pr view "$pr" --json isDraft,state,labels,mergeable,mergeStateStatus 2>/dev/null)" || {
  printf 'premerge.sh: cannot read PR %s\n' "$pr" >&2
  exit 2
}

# A field the API did not return is not a "no": an unknown draft state must stop
# the merge, not pass it.
for field in isDraft state labels mergeable mergeStateStatus; do
  if ! printf '%s' "$json" | jq -e "has(\"$field\")" >/dev/null 2>&1; then
    printf 'premerge.sh: PR %s is missing the %s field\n' "$pr" "$field" >&2
    exit 2
  fi
done

state="$(printf '%s' "$json" | jq -r '.state')"
draft="$(printf '%s' "$json" | jq -r '.isDraft')"
held="$(printf '%s' "$json" | jq -r '[.labels[].name]
  | map(select(test("^(hold|wip|do-not-merge|do not merge|draft)$"; "i"))) | join(",")')"
mergeable="$(printf '%s' "$json" | jq -r '.mergeable')"
merge_state="$(printf '%s' "$json" | jq -r '.mergeStateStatus')"

if [ "$state" != "OPEN" ]; then
  printf 'NOT-OPEN\thard stop: state=%s\n' "$state"
  printf 'summary\tNOT-OPEN\n'
  exit 1
fi

if [ "$draft" = "true" ]; then
  printf 'DRAFT\thard stop: the author marked it not-ready; only explicit human approval merges one\n'
  printf 'summary\tDRAFT\n'
  exit 1
fi

if [ -n "$held" ]; then
  printf 'HELD\thard stop: label=%s\n' "$held"
  printf 'summary\tHELD\n'
  exit 1
fi

# Mergeability is part of the stop, not a detail to leave to the merge command:
# GitHub computes it asynchronously after a force-push, and a sweep that merges
# on `mergeable=UNKNOWN` is merging a state it never validated. The merge state
# catches the strict-up-to-date rule, where GitHub reports MERGEABLE and then
# refuses the merge because the branch is behind.
if [ "$mergeable" != "MERGEABLE" ]; then
  printf 'NOT-MERGEABLE\thard stop: GitHub reports mergeable=%s (merge state %s); wait or rebase, never merge blind\n' \
    "$mergeable" "$merge_state"
  printf 'summary\tNOT-MERGEABLE\n'
  exit 1
fi

if [ "$merge_state" = "BEHIND" ]; then
  printf 'BEHIND\thard stop: the branch is behind its base; rebase before merging\n'
  printf 'summary\tBEHIND\n'
  exit 1
fi

printf 'MERGEABLE\t-\n'
printf 'summary\tMERGEABLE\n'
exit 0

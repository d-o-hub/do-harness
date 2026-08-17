#!/usr/bin/env bash
# check-commitlint.sh — enforces conventional commits with lowercase subjects.
#
# Sensor: scripts/check-commitlint.sh
# Fails if any of the most recent COMMITS commit messages do not match:
#   type(optional scope): lowercase subject
# where type is one of feat/fix/docs/chore/refactor/test/build/ci.
# Pass --count <N> or set DO_HARNESS_COMMITLINT_COUNT to size the window.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COUNT="${1:-}"
COUNT="${COUNT:-${DO_HARNESS_COMMITLINT_COUNT:-10}}"

if [[ ! -d "$ROOT/.git" ]]; then
    echo "check-commitlint skipped: no .git directory"
    exit 0
fi

FAIL=0
mapfile -t subjects < <(git -C "$ROOT" log -n "$COUNT" --pretty=format:%s)

for subject in "${subjects[@]}"; do
    if ! [[ "$subject" =~ ^(feat|fix|docs|chore|refactor|test|build|ci|perf|revert)(\([a-z0-9._/-]+\))?:.+\ .+$ ]]; then
        echo "FAIL: non-conventional commit subject: $subject"
        FAIL=1
        continue
    fi
    # Drop the "type(scope): " prefix, then require the subject to be lowercase.
    body="${subject#*: }"
    if [[ "$body" != "${body,,}" ]]; then
        echo "FAIL: commit subject is not lowercase: $subject"
        FAIL=1
    fi
done

if (( FAIL )); then
    echo "Conventional-commit invariant violated in the last $COUNT commit(s)."
    echo "Use: 'type(scope): lowercase subject' e.g. 'feat(workflow): gate advance on beats'"
    exit 1
fi

echo "check-commitlint OK: last $COUNT commit(s) use conventional lowercase subjects."

#!/usr/bin/env bash
# check-commitlint.sh — enforces conventional commits with lowercase subjects.
#
# Sensor: scripts/check-commitlint.sh
# Fails if any inspected commit messages do not match:
#   type(optional scope): lowercase subject
# where type is one of feat/fix/docs/chore/refactor/test/build/ci/perf/revert.
#
# Modes:
#   (no args)             lint the last COUNT commit subjects (sensor default).
#   --count <N>           override the history window (env: DO_HARNESS_COMMITLINT_COUNT).
#   --message <FILE>      lint the single subject in FILE (git commit-msg hook).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Lints a single subject line against the conventional-commit + lowercase rules.
# Returns nonzero (and prints a FAIL line) when the subject is invalid.
lint_subject() {
    local subject="$1"
    if [[ "$subject" =~ ^Merge[[:space:]] ]]; then
        return 0
    fi
    if ! [[ "$subject" =~ ^(feat|fix|docs|chore|refactor|test|build|ci|perf|revert)(\([a-z0-9._/-]+\))?:\ ?.+$ ]]; then
        echo "FAIL: non-conventional commit subject: $subject"
        return 1
    fi
    # Drop the "type(scope): " prefix, then require the subject to be lowercase.
    local body="${subject#*: }"
    if [[ "$body" != "${body,,}" ]]; then
        echo "FAIL: commit subject is not lowercase: $subject"
        return 1
    fi
}

# commit-msg hook mode: lint a single prepared message file.
if [[ "${1:-}" == "--message" ]]; then
    MESSAGE_FILE="${2:-}"
    if [[ -z "$MESSAGE_FILE" ]]; then
        echo "check-commitlint: --message requires a file path" >&2
        exit 2
    fi
    # Commit-msg files may carry leading '#' comment lines (git's template);
    # take the first non-comment, non-empty line as the subject.
    subject="$(awk 'NF && $0 !~ /^#/ { print; exit }' "$MESSAGE_FILE" || true)"
    if [[ -z "$subject" ]]; then
        echo "check-commitlint: no commit message found in $MESSAGE_FILE" >&2
        exit 2
    fi
    if ! lint_subject "$subject"; then
        echo "Conventional-commit invariant violated."
        echo "Use: 'type(scope): lowercase subject' e.g. 'fix: typo'"
        exit 1
    fi
    echo "check-commitlint OK: subject is conventional and lowercase."
    exit 0
fi

if [[ ! -d "$ROOT/.git" ]]; then
    echo "check-commitlint skipped: no .git directory"
    exit 0
fi

COUNT="${1:-}"
COUNT="${COUNT:-${DO_HARNESS_COMMITLINT_COUNT:-10}}"

FAIL=0
mapfile -t subjects < <(git -C "$ROOT" log --no-merges -n "$COUNT" --pretty=format:%s)

for subject in "${subjects[@]}"; do
    if ! lint_subject "$subject"; then
        FAIL=1
    fi
done

if (( FAIL )); then
    echo "Conventional-commit invariant violated in the last $COUNT commit(s)."
    echo "Use: 'type(scope): lowercase subject' e.g. 'feat(workflow): gate advance on beats'"
    exit 1
fi

echo "check-commitlint OK: last $COUNT commit(s) use conventional lowercase subjects."

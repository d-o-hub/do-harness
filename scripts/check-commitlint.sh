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

# History window: --count <N> flag overrides the env default of 10.
COUNT="${DO_HARNESS_COMMITLINT_COUNT:-10}"
while (( $# )); do
    case "$1" in
        --count)
            if [[ $# -lt 2 ]]; then
                echo "check-commitlint: --count requires a positive integer" >&2
                exit 2
            fi
            COUNT="$2"
            shift 2
            ;;
        --count=*)
            COUNT="${1#--count=}"
            shift
            ;;
        *)
            echo "check-commitlint: unknown argument: $1" >&2
            exit 2
            ;;
    esac
done
if ! [[ "$COUNT" =~ ^[1-9][0-9]*$ ]]; then
    echo "check-commitlint: count must be a positive integer, got: $COUNT" >&2
    exit 2
fi

FAIL=0
LINTED=0
# Read the subjects through a while loop (not mapfile) so this also runs on
# bash 3.2 / macOS, and so an empty history simply lints nothing.
while IFS= read -r subject; do
    LINTED=$((LINTED + 1))
    if ! lint_subject "$subject"; then
        FAIL=1
    fi
done < <(git -C "$ROOT" log --no-merges -n "$COUNT" --pretty=format:%s)

if (( FAIL )); then
    echo "Conventional-commit invariant violated in the last $COUNT commit(s)."
    echo "Use: 'type(scope): lowercase subject' e.g. 'feat(workflow): gate advance on beats'"
    exit 1
fi

echo "check-commitlint OK: last $LINTED commit(s) use conventional lowercase subjects."

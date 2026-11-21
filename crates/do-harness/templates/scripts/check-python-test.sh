#!/usr/bin/env bash
# check-python-test.sh — the `test` sensor for the Python pack: run pytest and
# treat "zero tests collected" (pytest exit 5) as a pass so a freshly scaffolded
# project with no tests yet is not a false red. Any other failure stays a
# failure, and a run that collects zero tests in a project that declares tests
# is still reported so it is never silently green.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT" || exit 1

pytest -q
status=$?

# pytest exits 5 when it collects zero tests. A green gate must never pass for
# nothing: a repository with a Python pack declares pytest as its test gate, so
# zero collected tests is a red run, not a skip (the no-vacuous-tests rule).
if [ "$status" -eq 5 ]; then
    echo "pytest: zero tests collected; the test gate measured nothing" >&2
    exit 1
fi

exit "$status"

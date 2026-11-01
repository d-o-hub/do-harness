#!/usr/bin/env bash
# check-untrusted-ingest.sh — proves untrusted-ingest skills keep text as data.
#
# Sensor: scripts/check-untrusted-ingest.sh
#
# Two skills read attacker-controlled text by design: pr-triage (PR titles,
# bodies, comments, review threads, diffs, and webhook payloads) and
# issue-to-pr (issue titles, bodies, comments, and plan-task text). Their
# SKILL.md contracts say that text is data and is never executed, but per
# AGENTS.md section 7 an architecture rule that lives only in prose is not
# enforced. This sensor is the executable half of those contracts: it fails
# when an ingest script gains a dynamic-execution sink that would let a
# crafted PR, comment, or issue run code.
#
# Scope decision: the ingest skills are enumerated in SKILL_ROOTS below. Every
# other skill reads repository content the developer already trusts. Widening
# the scope means adding a root here, not rewriting the check.
#
# Deliberately NOT a taint tracker: it cannot prove the absence of every path
# from untrusted bytes to a sink, only that the known dangerous sinks are
# absent from the ingest surface. That is the honest, checkable claim, and
# plans/invariants.json records it as such.

set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

ROOT="$(dh_repo_root)"
# Skills whose input is attacker-controlled by design.
SKILL_ROOTS=("pr-triage" "issue-to-pr")
FAIL_COUNT=0
PRESENT=0
TOTAL_SCRIPTS=0

fail() {
    echo "FAIL: $1"
    FAIL_COUNT=$((FAIL_COUNT + 1))
}

for skill_name in "${SKILL_ROOTS[@]}"; do
    SKILL_DIR="$ROOT/.agents/skills/$skill_name"

    # A skill is repo-only: when it is entirely absent (a published crate or a
    # template context), there is nothing to check for that root. When the
    # skill IS present its wrappers must be too — otherwise renaming
    # `scripts/` would silently suppress both the sink lint and the contract
    # check below.
    if [[ ! -d "$SKILL_DIR" ]]; then
        echo "note: $SKILL_DIR is absent (repo-only skill not present in this checkout)."
        continue
    fi
    PRESENT=$((PRESENT + 1))

    # The wrappers that call `gh` and parse its output, at any depth under
    # `scripts/`: a helper moved into `scripts/lib/` is still ingest surface,
    # so scoping the lint to the top level would let a sink hide one directory
    # down. `evals/` is exempt on purpose: a walkthrough feeds a hostile
    # payload through the scripts to prove nothing executes, so a sink there
    # would be the test, not the defect.
    SCRIPTS_DIR="$SKILL_DIR/scripts"
    scripts=()
    if [[ ! -d "$SCRIPTS_DIR" ]]; then
        fail "$SCRIPTS_DIR is missing (the $skill_name wrappers must exist when the skill is present)."
    else
        mapfile -t scripts < <(find "$SCRIPTS_DIR" -type f \
            -not -path '*/evals/*' \
            \( -name '*.sh' -o -name '*.mjs' -o -name '*.js' \) -print | sort)
        if [[ "${#scripts[@]}" -eq 0 ]]; then
            fail "no ingest scripts under $SCRIPTS_DIR (expected the $skill_name wrappers)."
        fi
    fi

    for script in "${scripts[@]:-}"; do
        [[ -n "$script" ]] || continue
        TOTAL_SCRIPTS=$((TOTAL_SCRIPTS + 1))
        rel="${script#"$ROOT"/}"

        # Strip full-line comments before matching. Documentation that *names*
        # a sink ("never run eval on a PR body") is the thing that stops the
        # next person from adding one, so the sensor must not punish writing
        # it down. Only whole-line comments are dropped: a sink inside an
        # inline comment still trips, which is the safe direction to be wrong
        # in.
        code="$(grep -vE '^[[:space:]]*(#|//)' "$script" || true)"

        # Shell dynamic execution. Each pattern is anchored so an unrelated
        # identifier (`evaluate`, `my_eval`) cannot trip it.
        if grep -nE '(^|[^[:alnum:]_])eval([[:space:]]|$)' <<<"$code" >/dev/null; then
            fail "$rel uses 'eval'; untrusted tracker text must never reach a shell evaluation."
        fi
        if grep -nE '(^|[^[:alnum:]_])(bash|sh|zsh|dash|ksh)[[:space:]]+-c([[:space:]]|$)' \
            <<<"$code" >/dev/null; then
            fail "$rel passes a command string to a shell; use a fixed argv instead."
        fi

        # Node dynamic execution: only meaningful once the module imports it.
        if grep -nE "from[[:space:]]+['\"]node:child_process['\"]" <<<"$code" >/dev/null; then
            if grep -nE '(^|[^[:alnum:]_.])exec(Sync)?[[:space:]]*\(' <<<"$code" >/dev/null; then
                fail "$rel uses child_process.exec, which interprets its argument as a shell command."
            fi
            if grep -nE 'shell[[:space:]]*:[[:space:]]*true' <<<"$code" >/dev/null; then
                fail "$rel enables shell interpretation in a spawn call."
            fi
        fi
    done

    # The contract must stay stated where an agent reads it: a sensor
    # enforcing a rule the guide no longer teaches is a rule nobody can
    # follow.
    if ! grep -qiE 'untrusted data, never as' "$SKILL_DIR/SKILL.md"; then
        fail "$SKILL_DIR/SKILL.md no longer states the untrusted-data contract."
    fi
done

if [[ "$PRESENT" -eq 0 ]]; then
    echo "SKIP: no untrusted-ingest skill is present in this checkout."
    exit 0
fi

if (( FAIL_COUNT > 0 )); then
    echo "untrusted-ingest contract violated."
    echo "FINDINGS: $FAIL_COUNT"
    exit 1
fi

echo "check-untrusted-ingest OK: $TOTAL_SCRIPTS ingest script(s) hold no dynamic-execution sink."
echo "FINDINGS: 0"

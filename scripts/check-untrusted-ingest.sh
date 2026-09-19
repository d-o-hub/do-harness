#!/usr/bin/env bash
# check-untrusted-ingest.sh — proves pr-triage's untrusted ingest stays data.
#
# Sensor: scripts/check-untrusted-ingest.sh
#
# The pr-triage skill reads attacker-controlled text: PR titles, bodies,
# comments, review threads, diffs, and webhook payloads. Its SKILL.md contract
# says that text is data and is never executed, but per AGENTS.md section 7 an
# architecture rule that lives only in prose is not enforced. This sensor is
# the executable half of that contract: it fails when an ingest script gains a
# dynamic-execution sink that would let a crafted PR or comment run code.
#
# Scope decision: only `.agents/skills/pr-triage/` is checked. That skill is
# the one that ingests hostile third-party text by design; the harness's other
# skills read repository content the developer already trusts. Widening the
# scope means adding a root here, not rewriting the check.
#
# Deliberately NOT a taint tracker: it cannot prove the absence of every path
# from untrusted bytes to a sink, only that the known dangerous sinks are
# absent from the ingest surface. That is the honest, checkable claim, and
# plans/invariants.json records it as such.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SKILL_DIR="$ROOT/.agents/skills/pr-triage"
# The wrappers that call `gh` and parse its output. `evals/walkthrough.sh` is
# exempt on purpose: it feeds a hostile payload through the router to prove
# nothing executes, so a sink there would be the test, not the defect.
SCRIPTS_DIR="$SKILL_DIR/scripts"
FAIL_COUNT=0

fail() {
    echo "FAIL: $1"
    FAIL_COUNT=$((FAIL_COUNT + 1))
}

if [[ ! -d "$SCRIPTS_DIR" ]]; then
    echo "SKIP: $SCRIPTS_DIR is absent (repo-only skill not present in this checkout)."
    exit 0
fi

mapfile -t scripts < <(find "$SCRIPTS_DIR" -maxdepth 1 -type f \
    \( -name '*.sh' -o -name '*.mjs' -o -name '*.js' \) -print | sort)

if [[ "${#scripts[@]}" -eq 0 ]]; then
    fail "no ingest scripts under $SCRIPTS_DIR (expected the pr-triage wrappers)."
fi

for script in "${scripts[@]:-}"; do
    [[ -n "$script" ]] || continue
    rel="${script#"$ROOT"/}"

    # Strip full-line comments before matching. Documentation that *names* a
    # sink ("never run eval on a PR body") is the thing that stops the next
    # person from adding one, so the sensor must not punish writing it down.
    # Only whole-line comments are dropped: a sink inside an inline comment
    # still trips, which is the safe direction to be wrong in.
    code="$(grep -vE '^[[:space:]]*(#|//)' "$script" || true)"

    # Shell dynamic execution. Each pattern is anchored so an unrelated
    # identifier (`evaluate`, `my_eval`) cannot trip it.
    if grep -nE '(^|[^[:alnum:]_])eval([[:space:]]|$)' <<<"$code" >/dev/null; then
        fail "$rel uses 'eval'; untrusted PR text must never reach a shell evaluation."
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

# The contract must stay stated where an agent reads it: a sensor enforcing a
# rule the guide no longer teaches is a rule nobody can follow.
if ! grep -qiE 'untrusted data, never as' "$SKILL_DIR/SKILL.md"; then
    fail "$SKILL_DIR/SKILL.md no longer states the untrusted-data contract."
fi

if (( FAIL_COUNT > 0 )); then
    echo "untrusted-ingest contract violated."
    echo "FINDINGS: $FAIL_COUNT"
    exit 1
fi

echo "check-untrusted-ingest OK: ${#scripts[@]} ingest script(s) hold no dynamic-execution sink."
echo "FINDINGS: 0"

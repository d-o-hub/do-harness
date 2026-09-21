#!/usr/bin/env bash
# pr-triage walkthrough: exercises the deterministic skill scripts against a
# fake gh and a local git repo. Proves merge ordering, no-effect detection,
# check classification, thread listing/resolution, state upsert/clear, and the
# harness `pr no-effect` command. Leaves graded residue under the sandbox root.
set -euo pipefail

root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
skill="$root/.agents/skills/pr-triage"

command -v jq >/dev/null 2>&1 || {
  echo "pr-triage evals require jq" >&2
  exit 1
}

# Fake gh on PATH; fixtures ship with the skill.
mkdir -p "$root/bin"
cp "$skill/evals/fake_gh.sh" "$root/bin/gh"
chmod +x "$root/bin/gh"
export PATH="$root/bin:$PATH"
export FAKE_GH_FIXTURES="$skill/evals/fixtures"

# Fixture repository with a change branch and an empty branch.
repo="$root/repo"
mkdir -p "$repo"
cd "$repo"
git init -q -b main
git config user.email "eval@example.com"
git config user.name "Eval"
printf 'one\n' > a.txt
git add a.txt
git commit -q -m base
git remote add origin "$repo"

git switch -q -c feature
printf 'two\n' > a.txt
git commit -qam change
git update-ref refs/pull/7/head feature
git update-ref refs/pull/9/head feature

git switch -q -c empty main
git commit -q --allow-empty -m empty
git update-ref refs/pull/8/head empty

# 1. Merge ordering: stacks first, oldest-first, drafts and hold excluded.
"$skill/scripts/list-prs.sh" > "$root/pr_list.tsv"
python3 - "$root/pr_list.tsv" "$root/pr_list_order.txt" <<'PY'
import sys

rows = [line.split("\t") for line in open(sys.argv[1]).read().splitlines() if line]
order = [row[0] for row in rows]
bases = [row[1] for row in rows]
assert order == ["2", "5", "1"], f"merge order: {order}"
assert bases == ["feature-a", "main", "main"], f"bases: {bases}"
open(sys.argv[2], "w").write(",".join(order))
PY

# 2. No-effect: real change vs empty commit.
"$skill/scripts/no-effect.sh" 7 > "$root/no_effect_has.txt"
"$skill/scripts/no-effect.sh" 8 > "$root/no_effect_empty.txt"

# 3. Check classification exits 0/1/2 for pass/fail/pending.
set +e
FAKE_GH_CHECKS=pass "$skill/scripts/checks.sh" 9 > "$root/checks_pass.out"
pass_exit=$?
FAKE_GH_CHECKS=fail "$skill/scripts/checks.sh" 9 > "$root/checks_fail.out"
fail_exit=$?
FAKE_GH_CHECKS=pending "$skill/scripts/checks.sh" 9 > "$root/checks_pending.out"
pending_exit=$?
set -e
printf 'pass_exit=%s\nfail_exit=%s\npending_exit=%s\n' \
  "$pass_exit" "$fail_exit" "$pending_exit" > "$root/checks_summary.txt"

# 4. Conversations: list unresolved threads and resolve one.
"$skill/scripts/threads.sh" list 9 > "$root/threads.tsv"
"$skill/scripts/threads.sh" resolve T_1 > "$root/thread_resolve.txt"

# 5. State helpers: upsert, read, clear.
"$skill/scripts/state.sh" set 42 abc def merged >/dev/null
"$skill/scripts/state.sh" get 42 > "$root/state_get.txt"
"$skill/scripts/state.sh" clear 42
if [ -z "$("$skill/scripts/state.sh" get 42)" ]; then
  echo cleared > "$root/state_cleared.txt"
fi

# 5b. Bounded waits: `checks.sh --wait` and `post-merge.sh` classify
# pass/fail/pending/not-yet-registered without unbounded sleeps.
set +e
FAKE_GH_CHECKS=pending "$skill/scripts/checks.sh" 9 --wait 1 > "$root/checks_wait.out"
checks_wait_exit=$?
FAKE_GH_RUNS=pass "$skill/scripts/post-merge.sh" 7 > "$root/post_merge_pass.out"
post_merge_pass_exit=$?
FAKE_GH_RUNS=fail "$skill/scripts/post-merge.sh" 7 > "$root/post_merge_fail.out"
post_merge_fail_exit=$?
FAKE_GH_RUNS=pending "$skill/scripts/post-merge.sh" 7 1 > "$root/post_merge_pending.out"
post_merge_pending_exit=$?
FAKE_GH_RUNS=none "$skill/scripts/post-merge.sh" 7 1 > "$root/post_merge_none.out"
post_merge_none_exit=$?
set -e
printf 'checks_wait_exit=%s\npost_merge_pass_exit=%s\npost_merge_fail_exit=%s\npost_merge_pending_exit=%s\npost_merge_none_exit=%s\n' \
  "$checks_wait_exit" "$post_merge_pass_exit" "$post_merge_fail_exit" "$post_merge_pending_exit" "$post_merge_none_exit" \
  > "$root/wait_summary.txt"

# 5c. Retry helper: transient HTTP 502 replays until success; a deterministic
# API failure returns after exactly one attempt with its exit code. Attempt
# counts travel through RETRY_COUNTER in the environment.
export RETRY_COUNTER="$root/retry_counter"
rm -f "$RETRY_COUNTER"
cat > "$root/bin/flaky" <<'EOF'
#!/bin/sh
n=$(cat "$RETRY_COUNTER" 2>/dev/null || echo 0)
n=$((n + 1))
echo "$n" > "$RETRY_COUNTER"
if [ "$n" -lt 3 ]; then echo "HTTP 502: 502 Bad Gateway" >&2; exit 1; fi
echo created
EOF
chmod +x "$root/bin/flaky"
cat > "$root/bin/deterministic" <<'EOF'
#!/bin/sh
n=$(cat "$RETRY_COUNTER" 2>/dev/null || echo 0)
n=$((n + 1))
echo "$n" > "$RETRY_COUNTER"
echo "GraphQL: Projects (classic) is being deprecated" >&2
exit 1
EOF
chmod +x "$root/bin/deterministic"
set +e
"$skill/scripts/retry.sh" --attempts 5 --delay 0 -- flaky > "$root/retry_flaky.out" 2> "$root/retry_flaky.err"
flaky_exit=$?
flaky_runs=$(cat "$RETRY_COUNTER")
rm -f "$RETRY_COUNTER"
"$skill/scripts/retry.sh" --attempts 5 --delay 0 -- deterministic > "$root/retry_det.out" 2> "$root/retry_det.err"
det_exit=$?
det_runs=$(cat "$RETRY_COUNTER" 2>/dev/null || echo 0)
set -e
printf 'flaky_exit=%s\nflaky_out=%s\nflaky_runs=%s\ndet_exit=%s\ndet_runs=%s\n' \
  "$flaky_exit" "$(cat "$root/retry_flaky.out")" "$flaky_runs" "$det_exit" "$det_runs" \
  > "$root/retry_summary.txt"

# 5d. Auto-merge guard: a pre-armed request is detected (exit 1 proves the
# caller must act), disarmed through --disable-auto, and then reads clean.
export FAKE_GH_AUTO_MERGE=armed
export FAKE_GH_AUTO_MERGE_LOG="$root/auto_merge_disable.log"
rm -f "$FAKE_GH_AUTO_MERGE_LOG"
set +e
"$skill/scripts/auto-merge.sh" 9 > "$root/auto_merge_armed.out"
auto_merge_armed_exit=$?
set -e
"$skill/scripts/auto-merge.sh" 9 --disable > "$root/auto_merge_disabled.out"
"$skill/scripts/auto-merge.sh" 9 > "$root/auto_merge_after.out"
printf 'armed_exit=%s\n' "$auto_merge_armed_exit" > "$root/auto_merge_summary.txt"
unset FAKE_GH_AUTO_MERGE FAKE_GH_AUTO_MERGE_LOG

# 5e. Webhook fast path: a real receiver answers /health, persists signed
# deliveries (rejecting unsigned ones), wakes long-polls, and drives the wait
# scripts with events instead of sleeps; an unreachable receiver falls back to
# polling. Node and curl are required for this section (present in CI).
command -v node >/dev/null 2>&1 || {
  echo "pr-triage evals require node for the webhook fast path" >&2
  exit 1
}
command -v curl >/dev/null 2>&1 || {
  echo "pr-triage evals require curl for the webhook fast path" >&2
  exit 1
}

webhook_root="$root/webhook"
mkdir -p "$webhook_root"
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')
node "$skill/scripts/webhook-receiver.mjs" --port "$port" \
  --events "$webhook_root/events.jsonl" --secret evalsecret \
  > "$webhook_root/receiver.out" 2> "$webhook_root/receiver.err" &
receiver_pid=$!
trap 'kill "$receiver_pid" 2>/dev/null || true' EXIT

up=""
for _ in $(seq 1 50); do
  if [ "$(curl -s --max-time 1 "http://127.0.0.1:$port/health")" = "ok" ]; then
    up=1
    break
  fi
  sleep 0.1
done
if [ -z "$up" ]; then
  echo "webhook receiver failed to become healthy" >&2
  cat "$webhook_root/receiver.err" >&2
  exit 1
fi
events_url="http://127.0.0.1:$port"
health_status=$(curl -s -o /dev/null -w '%{http_code}' "$events_url/health")

check_payload='{"action":"completed","check_run":{"id":7,"name":"CI","conclusion":"success","check_suite":{"head_sha":"abc","pull_requests":[{"number":9}]}}}'
sign() {
  printf '%s' "$1" | python3 -c 'import hmac,hashlib,sys; print("sha256="+hmac.new(b"evalsecret", sys.stdin.buffer.read(), hashlib.sha256).hexdigest())'
}
woke_value() {
  case "$1" in
    *'"woke":true'*) printf 'true' ;;
    *'"woke":false'*) printf 'false' ;;
    *) printf 'unknown' ;;
  esac
}

signed_status=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$events_url/events" \
  -H "X-GitHub-Event: check_run" -H "X-Hub-Signature-256: $(sign "$check_payload")" -d "$check_payload")
unsigned_status=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$events_url/events" \
  -H "X-GitHub-Event: check_run" -d "$check_payload")
event_lines=$(wc -l < "$webhook_root/events.jsonl" | tr -d ' ')

# Idle wait (cursor past the one persisted event) times out without a wake; a
# wait started first is then woken by a matching signed delivery.
wait_idle_woke=$(woke_value "$(curl -s "$events_url/wait?since=1&timeout=1")")
curl -s "$events_url/wait?since=1&timeout=5" > "$webhook_root/wait_wake.json" &
wait_pid=$!
sleep 0.5
curl -s -o /dev/null -X POST "$events_url/events" \
  -H "X-GitHub-Event: check_run" -H "X-Hub-Signature-256: $(sign "$check_payload")" -d "$check_payload"
wait "$wait_pid"
wait_wake_woke=$(woke_value "$(cat "$webhook_root/wait_wake.json")")

# Event-driven wait: pending first, then a signed delivery wakes the wait and
# the re-classification sees pass; stderr must show the wake, not a fallback.
export FAKE_GH_SEQ_FILE="$root/fake_gh_sequence.state"
rm -f "$FAKE_GH_SEQ_FILE"
checks_event_start=$(python3 -c 'import time; print(int(time.time()*1000))')
set +e
FAKE_GH_CHECKS_SEQUENCE=pending,pass "$skill/scripts/checks.sh" 9 --wait 20 --events-url "$events_url" \
  > "$webhook_root/checks_event.out" 2> "$root/webhook_checks_event.err" &
checks_event_pid=$!
sleep 0.5
curl -s -o /dev/null -X POST "$events_url/events" \
  -H "X-GitHub-Event: check_run" -H "X-Hub-Signature-256: $(sign "$check_payload")" -d "$check_payload"
wait "$checks_event_pid"
checks_event_exit=$?
set -e
checks_event_elapsed_ms=$(( $(python3 -c 'import time; print(int(time.time()*1000))') - checks_event_start ))
checks_event_summary=$(awk -F'\t' '$1 == "summary" { print $2 }' "$webhook_root/checks_event.out")
if grep -q 'falling back' "$root/webhook_checks_event.err"; then
  echo "checks.sh fell back to polling during the event-driven run" >&2
  cat "$root/webhook_checks_event.err" >&2
  exit 1
fi

# Unreachable receiver: the same pending-then-pass sequence still finishes by
# polling, and post-merge.sh is unaffected.
rm -f "$FAKE_GH_SEQ_FILE"
set +e
FAKE_GH_CHECKS_SEQUENCE=pending,pass "$skill/scripts/checks.sh" 9 --wait 5 --events-url "http://127.0.0.1:9" \
  > "$webhook_root/checks_fallback.out" 2> "$root/webhook_checks_fallback.err"
checks_fallback_exit=$?
FAKE_GH_RUNS=pass "$skill/scripts/post-merge.sh" 7 --events-url "$events_url" \
  > "$webhook_root/post_merge_event.out" 2> "$webhook_root/post_merge_event.err"
post_merge_event_exit=$?
set -e
checks_fallback_summary=$(awk -F'\t' '$1 == "summary" { print $2 }' "$webhook_root/checks_fallback.out")

kill -TERM "$receiver_pid" 2>/dev/null || true
wait "$receiver_pid" 2>/dev/null || true
trap - EXIT

cat > "$root/webhook_summary.txt" <<EOF
health_status=$health_status
signed_status=$signed_status
unsigned_status=$unsigned_status
event_lines=$event_lines
wait_idle_woke=$wait_idle_woke
wait_wake_woke=$wait_wake_woke
checks_event_exit=$checks_event_exit
checks_event_summary=$checks_event_summary
checks_event_elapsed_ms=$checks_event_elapsed_ms
checks_fallback_exit=$checks_fallback_exit
checks_fallback_summary=$checks_fallback_summary
post_merge_event_exit=$post_merge_event_exit
EOF

# 5f. Semantic router: drive the optional typed router wrapper against a
# hermetic fake provider. Every scenario writes residue under $root so the
# policy (fallbacks, schema rejection, uncertainty escalation, head-bound cache,
# zero-effect bypass, and diff text that must never be executed) is graded.
unset PR_TRIAGE_ROUTER
unset PR_TRIAGE_CONFIDENCE_THRESHOLD
unset PR_TRIAGE_ROUTER_TIMEOUT

cat > "$root/bin/fake-router" <<'ROUTER'
#!/usr/bin/env bash
# Deterministic router stub: reads one JSON object on stdin and emits the
# judgment named by FAKE_ROUTER_MODE. Each invocation appends to
# FAKE_ROUTER_COUNTER so the walkthrough can observe cache and bypass paths.
set -euo pipefail
cat >/dev/null
if [ -n "${FAKE_ROUTER_COUNTER:-}" ]; then
  printf 'call\n' >> "$FAKE_ROUTER_COUNTER"
fi
case "${FAKE_ROUTER_MODE:?FAKE_ROUTER_MODE required}" in
  docs-cheap)
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  api-deep)
    printf '%s\n' '{"schema_version":1,"change_kind":"public-api","behavior_change":{"answer":"yes","confidence":0.95},"public_contract_change":{"answer":"yes","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  security-deep)
    printf '%s\n' '{"schema_version":1,"change_kind":"security","behavior_change":{"answer":"yes","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"yes","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  low-confidence)
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.60},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.60}'
    ;;
  malformed)
    printf 'this is not json\n'
    ;;
  bad-schema)
    printf '%s\n' '{"schema_version":1,"change_kind":"prose","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  out-of-range)
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":1.5},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  missing-fields)
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  misleading)
    printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"yes","confidence":0.99},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
    ;;
  *)
    printf 'unknown FAKE_ROUTER_MODE: %s\n' "$FAKE_ROUTER_MODE" >&2
    exit 1
    ;;
esac
ROUTER
chmod 0755 "$root/bin/fake-router"

# Router fixture repo. The gate policy proves `src/generated/**` mechanical, so
# a branch that rewrites the generated file *and* one unproven source line has a
# residual smaller than its raw diff (`reduced`), while a branch that touches
# only the unproven line does not (the serialized residual is larger than the
# diff). Distinct branches give every scenario a distinct head sha, so the
# head-bound route cache cannot cross-serve one scenario's decision to the next.
router_repo="$root/router_repo"
mkdir -p "$router_repo/src/generated"
cd "$router_repo"
git init -q -b main
git config user.email "eval@example.com"
git config user.name "Eval"
mkdir -p .github
printf '[proof]\nmechanical = ["src/generated/**"]\n' > .github/pr-gate.toml
seq 1 200 | sed 's/^/generated line /' > src/generated/bulk.txt
printf 'alpha\nbeta\ngamma\n' > src/real.txt
python3 - <<'PY'
lines = [f'{{"key_{i}": "va\\"lue \\\\ with \\"quotes\\"", "n": {i}}}' for i in range(60)]
open('src/config.json', 'w').write('\n'.join(lines) + '\n')
PY
git add .
git commit -q -m base

# Rewrites the proven generated file (dropped from the residual) plus one
# unproven source line (kept), leaving a residual smaller than the raw diff.
make_routed_change() {
  seq 201 400 | sed 's/^/generated line /' > src/generated/bulk.txt
  printf 'changed\n' >> src/real.txt
  git add src/generated/bulk.txt src/real.txt
  git commit -qm routed
}

# The malicious payload carries text that would create a file or delete the
# sandbox root if any code path ever interpreted diff content as a command.
git switch -qc malicious main
cat > src/payload.txt <<'PAYLOAD'
$(touch __SANDBOX_ROOT__/pwned)
; rm -rf __SANDBOX_ROOT__
PAYLOAD
sed -i "s|__SANDBOX_ROOT__|$root|g" src/payload.txt
git add src/payload.txt
git commit -qm malicious

# No-go branch: escaping-heavy JSON makes the serialized residual larger than
# the raw diff, so the wrapper must select the raw diff instead.
git switch -qc no_go main
python3 - <<'PY'
lines = [f'{{"key_{i}": "CHANGED \\"value\\" \\\\ more", "n": {i}}}' for i in range(60)]
open('src/config.json', 'w').write('\n'.join(lines) + '\n')
PY
git commit -qam no-go

# Do-nothing branch: an empty commit has no diff at all.
git switch -qc empty main
git commit -q --allow-empty -m empty

# Per-judgment branches; each name mirrors the FAKE_ROUTER_MODE it exercises.
for spec in docs-cheap api-deep security-deep low-confidence malformed bad-schema out-of-range missing-fields misleading; do
  git switch -qc "case-$spec" main
  make_routed_change
done

# Cache branch: a routed change whose content a later empty commit leaves intact.
git switch -qc cache-case main
make_routed_change

router_counter="$root/router_calls"
route() {  # <residue-case> <FAKE_ROUTER_MODE> <head>
  local case_name="$1" mode="$2" head="$3"
  FAKE_ROUTER_MODE="$mode" FAKE_ROUTER_COUNTER="$router_counter" \
    DO_HARNESS_BIN="$DO_HARNESS_BIN" PR_TRIAGE_ROUTER="$root/bin/fake-router" \
    "$skill/scripts/semantic-route.sh" --base main --head "$head" \
    > "$root/route_${case_name}.json" 2> "$root/route_${case_name}.err" || true
}

# Unconfigured and non-executable routers never reach a provider.
DO_HARNESS_BIN="$DO_HARNESS_BIN" \
  "$skill/scripts/semantic-route.sh" --base main --head case-docs-cheap \
  > "$root/route_unset.json" 2> "$root/route_unset.err" || true
DO_HARNESS_BIN="$DO_HARNESS_BIN" PR_TRIAGE_ROUTER="$root/bin/absent-router" \
  "$skill/scripts/semantic-route.sh" --base main --head case-docs-cheap \
  > "$root/route_missing.json" 2> "$root/route_missing.err" || true

# Schema rejections: malformed JSON, unknown enum, out-of-range confidence,
# below-threshold confidence, and a missing required subfield.
route malformed malformed case-malformed
route bad_schema bad-schema case-bad-schema
route out_of_range out-of-range case-out-of-range
route low_confidence low-confidence case-low-confidence
route missing_fields missing-fields case-missing-fields

# Valid judgments. `misleading` claims docs but asserts a public contract
# change, so the atomic answer wins and the route is deep.
route docs_cheap docs-cheap case-docs-cheap
route api_deep api-deep case-api-deep
route security_deep security-deep case-security-deep
route misleading misleading case-misleading

# The no-go case reviews the raw diff rather than a residual.
route no_go docs-cheap no_go

# Cache identity: an unchanged head reuses the decision; advancing the head
# re-invokes the provider.
: > "$router_counter"
route cache_first docs-cheap cache-case
cache_first_calls=$(wc -l < "$router_counter" | tr -d ' ')
route cache_second docs-cheap cache-case
cache_second_calls=$(wc -l < "$router_counter" | tr -d ' ')
git commit -q --allow-empty -m advance
route cache_third docs-cheap cache-case
cache_third_calls=$(wc -l < "$router_counter" | tr -d ' ')
cache_cached=no
cache_reinvoked=no
[ "$cache_second_calls" -eq "$cache_first_calls" ] && cache_cached=yes
[ "$cache_third_calls" -gt "$cache_second_calls" ] && cache_reinvoked=yes
printf 'cache_first_calls=%s\ncache_second_calls=%s\ncache_third_calls=%s\ncache_cached=%s\ncache_reinvoked=%s\n' \
  "$cache_first_calls" "$cache_second_calls" "$cache_third_calls" "$cache_cached" "$cache_reinvoked" \
  > "$root/route_summary.txt"

# Diff text stays data: routing the malicious payload executes nothing.
rm -f "$root/pwned"
route malicious docs-cheap malicious
if [ -e "$root/pwned" ]; then
  printf 'executed\n' > "$root/route_malicious.txt"
else
  printf 'not-executed\n' > "$root/route_malicious.txt"
fi

# Timeout: a provider that outlives the bound fails toward depth.
cat > "$root/bin/fake-router-slow" <<'SLOW'
#!/usr/bin/env bash
set -euo pipefail
cat >/dev/null
sleep 5
printf '%s\n' '{"schema_version":1,"change_kind":"docs","behavior_change":{"answer":"no","confidence":0.95},"public_contract_change":{"answer":"no","confidence":0.98},"security_sensitive":{"answer":"no","confidence":0.99},"needs_repository_context":{"answer":"no","confidence":0.90},"confidence":0.95}'
SLOW
chmod 0755 "$root/bin/fake-router-slow"
FAKE_ROUTER_MODE=docs-cheap DO_HARNESS_BIN="$DO_HARNESS_BIN" \
  PR_TRIAGE_ROUTER="$root/bin/fake-router-slow" PR_TRIAGE_ROUTER_TIMEOUT=1 \
  "$skill/scripts/semantic-route.sh" --base main --head case-docs-cheap \
  > "$root/route_timeout.json" 2> "$root/route_timeout.err" || true

# Zero-effect: an empty diff bypasses the provider entirely.
: > "$router_counter"
route no_effect docs-cheap empty
zero_calls=$(wc -l < "$router_counter" | tr -d ' ')
printf 'provider_calls=%s\n' "$zero_calls" > "$root/route_zero_calls.txt"

git switch -q main
cd "$root"

# 6. Harness command: a git repo at the sandbox root for `cli:` assertions.
cd "$root"
git init -q "$root"
git config user.email "eval@example.com"
git config user.name "Eval"
printf 'x\n' > root_file.txt
git add root_file.txt
git commit -q -m root

# 7. Negative case: out-of-scope requests never touch PR machinery.
cat > "$root/pr_negative.txt" << 'TXT'
out-of-scope: non-PR requests never touch the sweep, merge, or thread machinery
TXT

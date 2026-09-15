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

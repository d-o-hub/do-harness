#!/usr/bin/env bash
# harness walkthrough: proves greenfield adoption end-to-end with the real
# binary — init scaffolds the workspace plus a minimal crate, the full rust
# sensor suite must actually pass (no assumed green), then a broken workspace
# drives the fail-fast error-signature lifecycle (record -> list) as residue.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
bin="${DO_HARNESS_BIN:-do-harness}"

# Greenfield adoption: the full suite (fmt/check/clippy/test/loc/commitlint)
# runs against the scaffolded crate and must exit 0.
#
# `--minimal` is load-bearing, not cosmetic: without it `init` re-scaffolds
# `.agents/skills/harness/SKILL.md` + `references/`, which is the guidance the
# without-skill baseline strips. The baseline would then grade a regenerated
# copy of the guidance it was supposed to measure without, and Skill Lift is
# reported as contaminated instead of measured. Skill scaffolding is already
# covered by the repository's own skills-sync sensor, so this walkthrough only
# needs the workspace + crate. Keep this comment free of bare tree tokens: the
# eval sandbox mirrors any repo path named here, and a mirrored non-hidden
# entry would make `init` detect the generic pack instead of greenfield Rust.
"$bin" --root "$root" init --minimal >/dev/null
"$bin" --root "$root" verify

# Break the crate: the test sensor must fail and record its signature.
rm -rf "$root/Cargo.toml" "$root/src"
"$bin" --root "$root" verify --only test --record >/dev/null 2>&1 || true

"$bin" --root "$root" errors list

# Self-correction: minimal fix (restore the crate), then prove green again.
"$bin" --root "$root" init --minimal >/dev/null
"$bin" --root "$root" verify

# Remote advancement fixture in a separate subdirectory:
freshness_dir="$root/freshness-fixture"
mkdir -p "$freshness_dir"
origin="$freshness_dir/origin.git"
seeder="$freshness_dir/seeder"
consumer="$freshness_dir/consumer"

git init -q --bare -b main "$origin"
git clone -q "$origin" "$seeder"
(
  cd "$seeder"
  git config user.email "eval@example.com"
  git config user.name "Eval"
  echo "commit A" > README.md
  git add README.md
  git commit -q -m "init A"
  git push -q origin main
)
git -C "$origin" symbolic-ref HEAD refs/heads/main

git clone -q "$origin" "$consumer"
(
  cd "$consumer"
  git config user.email "eval@example.com"
  git config user.name "Eval"
)

# Advance origin via seeder to commit B
(
  cd "$seeder"
  echo "commit B" > README.md
  git add README.md
  git commit -q -m "advance B"
  git push -q origin main
)
commit_b="$(git -C "$seeder" rev-parse HEAD)"

helper="$root/.agents/skills/harness/scripts/latest-main.sh"

head_before="$(git -C "$consumer" rev-parse HEAD)"
origin_main_before="$(git -C "$consumer" rev-parse refs/remotes/origin/main)"
cp "$consumer/.git/HEAD" "$freshness_dir/head.before"
cp "$consumer/.git/index" "$freshness_dir/index.before"
cp "$consumer/README.md" "$freshness_dir/readme.before"

bash "$helper" --root "$consumer" --read-only > "$root/freshness_ro_sha.txt" 2> "$root/freshness_ro.err"
if [ "$(cat "$root/freshness_ro_sha.txt")" = "$commit_b" ] \
  && [ "$(git -C "$consumer" rev-parse HEAD)" = "$head_before" ] \
  && [ "$(git -C "$consumer" rev-parse refs/remotes/origin/main)" = "$origin_main_before" ] \
  && ! git -C "$consumer" cat-file -e "${commit_b}^{commit}" 2>/dev/null \
  && cmp -s "$consumer/.git/HEAD" "$freshness_dir/head.before" \
  && cmp -s "$consumer/.git/index" "$freshness_dir/index.before" \
  && cmp -s "$consumer/README.md" "$freshness_dir/readme.before"; then
  printf 'preserved\n'
else
  printf 'changed\n'
fi > "$root/freshness_ro_state.txt"

bash "$helper" --root "$consumer" > "$root/freshness_sha.txt" 2> "$root/freshness.err"
if [ "$(cat "$root/freshness_sha.txt")" = "$commit_b" ] \
  && [ "$(git -C "$consumer" rev-parse refs/remotes/origin/main)" = "$commit_b" ] \
  && [ "$(git -C "$consumer" rev-parse HEAD)" = "$head_before" ] \
  && cmp -s "$consumer/.git/HEAD" "$freshness_dir/head.before" \
  && cmp -s "$consumer/.git/index" "$freshness_dir/index.before" \
  && cmp -s "$consumer/README.md" "$freshness_dir/readme.before"; then
  printf 'match\n'
else
  printf 'mismatch\n'
fi > "$root/freshness_result.txt"

# Negative case: outside a git worktree
non_git_dir="$root/not-a-git-repo"
mkdir -p "$non_git_dir"
set +e
bash "$helper" --root "$non_git_dir" > "$root/outside_git.out" 2> "$root/outside_git.err"
outside_exit=$?
set -e
printf 'exit=%s\n' "$outside_exit" > "$root/outside_git_exit.txt"
if [ ! -s "$root/outside_git.out" ]; then
  rm -f "$root/outside_git.out"
fi

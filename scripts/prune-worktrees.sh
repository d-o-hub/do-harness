#!/usr/bin/env bash
# prune-worktrees.sh — reclaim the checkouts of branches whose work already landed.
#
# A squash merge puts the change in `main` but leaves the branch's linked
# worktree behind: the checkout keeps its files, its target directory, and the
# registration in `.git/worktrees`. Nothing removes it automatically, because
# only the author knows whether the worktree still holds work in progress.
#
# This script decides from evidence, and refuses anything it cannot prove:
#
#   - the repository's own checkout is never a candidate;
#   - a worktree with uncommitted changes is never removed (`git status` must be
#     empty), so an edit in flight is not destroyed;
#   - the work must be provably landed: a merged pull request for the head ref,
#     no commits ahead of `origin/main`, a tree identical to `origin/main`, or
#     every commit patch-equivalent upstream (`git cherry` reports no `+`);
#   - removal never uses `--force`, and the branch ref is kept — deleting the
#     checkout does not delete the branch, so a mistake stays recoverable.
#
# A clean worktree whose work is *not* provably landed is reported as
# `local-only` when its branch exists neither on `origin` nor in any pull
# request. Those checkouts are redundant (the commits live in the branch ref),
# but they are only removed with `--unmerged-checkouts`, because only the author
# knows whether the work is abandoned or still being finished.
#
# Default is a dry run: it prints the verdict per worktree and changes nothing.
#
#   scripts/prune-worktrees.sh                          # report only
#   scripts/prune-worktrees.sh --apply                  # remove landed worktrees
#   scripts/prune-worktrees.sh --apply --unmerged-checkouts
#                                                       # also drop checkouts of
#                                                       # local-only branches,
#                                                       # keeping the branches
#
# Run it after a merge: the sweep's last step is exactly this check.

set -euo pipefail

# shellcheck disable=SC1091 # resolved at runtime from this script's directory
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

ROOT="$(dh_repo_root)"
BASE_REF="origin/main"
APPLY=0
UNMERGED_CHECKOUTS=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --apply) APPLY=1; shift ;;
        --unmerged-checkouts) UNMERGED_CHECKOUTS=1; shift ;;
        --base) BASE_REF="${2:?--base needs a ref}"; shift 2 ;;
        -h | --help)
            sed -n '2,28p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "usage: prune-worktrees.sh [--apply] [--unmerged-checkouts] [--base REF]" >&2
            exit 2
            ;;
    esac
done

git -C "$ROOT" rev-parse -q --verify "$BASE_REF^{commit}" >/dev/null 2>&1 || {
    echo "prune-worktrees: base ref '$BASE_REF' does not resolve; fetch first" >&2
    exit 2
}

# Merged pull-request head refs, when `gh` is usable. Missing tooling degrades to
# the commit/tree tests below instead of failing the report.
merged_heads=""
if command -v gh >/dev/null 2>&1; then
    merged_heads="$(gh pr list --state merged --limit 500 --json headRefName \
        --jq '.[] | .headRefName' 2>/dev/null || true)"
fi
is_merged_pr() {
    [[ -n "$merged_heads" ]] && printf '%s\n' "$merged_heads" | grep -qxF "$1"
}

removed=0
detached=0
kept=0
skipped=0

while IFS= read -r worktree; do
    [[ -n "$worktree" ]] || continue
    if [[ "$worktree" == "$ROOT" ]]; then
        echo "skip   $worktree (the repository checkout)"
        skipped=$((skipped + 1))
        continue
    fi
    if [[ ! -d "$worktree" ]]; then
        echo "skip   $worktree (missing directory; run: git worktree prune)"
        skipped=$((skipped + 1))
        continue
    fi

    branch="$(git -C "$worktree" rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
    dirty="$(git -C "$worktree" status --porcelain 2>/dev/null | wc -l)"
    if [[ "$dirty" != "0" ]]; then
        echo "keep   $worktree [$branch] ($dirty uncommitted change(s))"
        kept=$((kept + 1))
        continue
    fi

    ahead="$(git -C "$worktree" rev-list --count "$BASE_REF..HEAD" 2>/dev/null || echo "?")"
    tree_diff="$(git -C "$worktree" diff --stat "$BASE_REF...HEAD" 2>/dev/null | tail -n 1)"
    unapplied="$(git -C "$worktree" cherry "$BASE_REF" HEAD 2>/dev/null | grep -c '^+' || true)"
    landed=""
    if [[ "$branch" != "HEAD" ]] && is_merged_pr "$branch"; then
        landed="merged pull request for $branch"
    elif [[ "$ahead" == "0" ]]; then
        landed="no commits ahead of $BASE_REF"
    elif [[ -z "$tree_diff" ]]; then
        landed="tree identical to $BASE_REF"
    elif [[ "$ahead" != "?" ]] && [[ "$unapplied" == "0" ]]; then
        landed="every commit patch-equivalent upstream"
    fi

    if [[ -z "$landed" ]]; then
        detail="${tree_diff:-no diff summary}"
        remote_branch=""
        if [[ "$branch" != "HEAD" ]]; then
            remote_branch="$(git -C "$ROOT" ls-remote --heads origin "$branch" 2>/dev/null | wc -l)"
        fi
        has_pr=0
        if [[ "$branch" != "HEAD" ]] && command -v gh >/dev/null 2>&1; then
            has_pr="$(gh pr list --head "$branch" --state all --limit 1 --json number --jq 'length' 2>/dev/null || echo 0)"
        fi
        if ((UNMERGED_CHECKOUTS)) && [[ "$remote_branch" == "0" ]] && [[ "$has_pr" == "0" ]]; then
            if ((APPLY)); then
                if git -C "$ROOT" worktree remove "$worktree" 2>/dev/null; then
                    echo "detach $worktree [$branch] (local-only; checkout removed, branch kept)"
                    detached=$((detached + 1))
                else
                    echo "keep   $worktree [$branch] (removal failed)"
                    kept=$((kept + 1))
                fi
            else
                echo "would detach $worktree [$branch] (local-only; checkout removed, branch kept)"
                detached=$((detached + 1))
            fi
            continue
        fi
        echo "keep   $worktree [$branch] ($ahead commit(s) ahead, $unapplied not upstream; $detail)"
        kept=$((kept + 1))
        continue
    fi

    if ((APPLY)); then
        if git -C "$ROOT" worktree remove "$worktree" 2>/dev/null; then
            echo "remove $worktree [$branch] ($landed)"
            removed=$((removed + 1))
        else
            echo "keep   $worktree [$branch] (removal failed; git refuses a dirty or locked tree)"
            kept=$((kept + 1))
        fi
    else
        echo "would remove $worktree [$branch] ($landed)"
        removed=$((removed + 1))
    fi
done < <(git -C "$ROOT" worktree list --porcelain | awk '/^worktree /{print substr($0, 10)}')

git -C "$ROOT" worktree prune

if ((APPLY)); then
    echo "prune-worktrees: removed $removed, detached $detached, kept $kept, skipped $skipped"
else
    echo "prune-worktrees: $removed removable, $detached detachable, $kept kept, $skipped skipped (dry run; pass --apply)"
fi

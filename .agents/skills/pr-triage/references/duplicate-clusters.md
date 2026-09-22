# Duplicate clusters

Bot swarms and parallel agents file near-identical PRs. Treat a set of PRs as
one cluster when all three hold:

1. they touch the same file (or the same file set for one change);
2. they branch from the same base blob for that file — compare
   `gh pr view PR --json files,baseRefOid` against
   `gh api repos/{owner}/{repo}/contents/<path>?ref=<baseRefOid> --jq .sha`;
3. they have the same effect on the tree — `scripts/no-effect.sh` on each
   revision range is the cheap proxy when two branches look identical.

Measure before believing a difference in wording or commit messages: identical
diffs with different prose are one change.

## Keep exactly one

Apply the first rule that separates the cluster:

- **(a) green CI beats red.** A member whose checks pass keeps over one that is
  failing, pending, or unknown.
- **(b) fewest unrelated files.** Prefer the member whose diff touches only the
  intended files; a PR carrying drive-by edits loses even when it is older.
- **(c) backward-compat wrapper beats signature break.** The change that keeps a
  deprecated path working wins over one that removes it, so callers can migrate
  on their own schedule.
- **(d) newest branch rebases cleanest.** When the rules above tie, keep the most
  recently rebased branch: it has the fewest conflicts against the current base.

If two members are still indistinguishable, escalate with the evidence you
measured (`baseRefOid` per member, `no-effect` output, diffstat) instead of
guessing. The user owns the product decision.

## Close the losers

Close every non-keeper with the cluster reason and a link to the keeper:

```bash
gh pr close LOSER --comment "Superseded by #KEEPER: same <file> change on the
same base blob, kept by rule (a|b|c|d) — <one line of evidence>."
```

Record the outcome with
`scripts/state.sh set LOSER HEAD_SHA BASE_SHA closed-superseded` so the next
sweep skips it, and report the cluster in the sweep summary (`KEEPER` merged,
each `LOSER` closed-superseded).

## Never merge two members

Merging a second member lands the same behavior twice: the diff conflicts, or it
silently re-applies code the keeper already replaced. One keeper per cluster,
always — and the keeper is merged through the normal loop
([merge-order.md](merge-order.md)), not on the strength of the cluster decision.

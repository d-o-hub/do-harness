# Queue discovery

`scripts/queue.sh` builds the ordered work list and prints one TSV row per
actionable item on stdout:

    kind<TAB>id<TAB>rank<TAB>title

Every excluded candidate is named on stderr with its reason, and a summary
line always prints, so an empty queue is explainable rather than mysterious.

## Sources

1. **Open GitHub issues** — `gh issue list --state open`, the public work
   contract.
2. **Pending plan tasks** — `do-harness task list --format json`, filtered to
   status `pending`. Tasks already started (`in_progress`), done, or failed
   are excluded: an in-progress task resumes on its branch, and terminal
   states are not queue material.

If no harness binary resolves, the task source degrades to a warning and the
issue list still prints; a task-only queue is not silently invented.

## Ranking

| Rank | Signal |
|------|--------|
| 0-3  | `P0`..`P3` label, or a `[P0]`..`[P3]` title prefix (label wins) |
| 4    | unprioritized issues |
| 5    | plan tasks |

Within a rank, ascending id. Issues rank above tasks so the public contract
drains first.

## Exclusions (issues)

| Signal | Reason |
|--------|--------|
| label `hold`, `wip`, `do-not-merge`, `wontfix`, `duplicate`, `blocked` | explicit hold |
| an open PR whose head is `issue-ID-...` or whose body contains `Closes #ID` | already covered |

## Exclusions (tasks)

- status other than `pending` (see Sources).

## Selectors

`--issue N` and `--task ID` narrow stdout to that item. They do not bypass any
exclusion: `start-item.sh` re-validates the item and refuses held, covered, or
non-pending items even when selected directly.

## Freshness

The queue reads live tracker and database state; there is nothing to cache.
Re-run it after any item opens so the next selection sees the new PR and the
updated task state.

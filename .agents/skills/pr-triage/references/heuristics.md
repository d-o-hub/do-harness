# Heuristics
- **rebase a stacked child with git rebase --onto origin/DEFAULT OLD_PARENT_SHA after any parent merge, then force-push and re-validate**: applies when a parent PR merges (squash or rebase), its branch auto-deletes, and the child auto-retargets to the default branch showing CONFLICTING/DIRTY (from trace 18)

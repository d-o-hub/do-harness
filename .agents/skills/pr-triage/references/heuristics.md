# Heuristics
- **rebase a stacked child with git rebase --onto origin/DEFAULT OLD_PARENT_SHA after any parent merge, then force-push and re-validate**: applies when a parent PR merges (squash or rebase), its branch auto-deletes, and the child auto-retargets to the default branch showing CONFLICTING/DIRTY (from trace 18)
- **a skill that must run at a lifecycle boundary (after opening a PR) needs that trigger in its description; agents load skills from descriptions, not from remembered plans**: skill authoring: lifecycle triggers versus request-only phrasing (from trace 37)
- **optional-tool probes in skills must honor DO_HARNESS_BIN so dev-checkout binaries are found instead of silently falling back to the token-heavy path**: do-harness-optional probes and preflight checks (from trace 37)

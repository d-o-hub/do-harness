# Contributing

Thanks for considering a contribution to `do-harness`. This repository follows
an **issue-first** workflow: every pull request (PR) must reference a triaged
issue, so maintainers can plan, label, and review a change before code exists.
This guide covers that workflow, the branch and commit conventions CI enforces,
what your PR is expected to carry, and what maintainers gate on.

## Quick start

1. Open an issue (bug, feature, or contract change) using the templates in
   `.github/ISSUE_TEMPLATE/`, or find an existing open issue you want to work
   on.
2. Comment on the issue to claim it.
3. Create a branch from `main` with one of the documented prefixes.
4. Make your change in focused commits following Conventional Commits.
5. Run the local sensors before pushing.
6. Open a PR that links the issue (`Closes #<n>` for a completed fix).
7. Address review feedback; the merge is a squash merge.

## Issue-first workflow

- Every PR must reference an issue that maintainers have triaged. A PR that
  does not link an issue is blocked by the `issue-link-check` CI gate.
  writing code. Bug reports and small documentation fixes still get an issue —
  the template takes minutes, and it lets maintainers set priority and avoid
  duplicate work.
- To claim an issue, leave a comment. Maintainers may assign it to you.
- Link the issue in the PR body with a trailer:

  - `Closes #<n>` — the PR fully addresses issue `<n>` and closes it on merge.
  - `Refs #<n>` — the PR partially addresses issue `<n>` (for example, when a
    follow-up needs external credentials or a product decision).

When you open an issue, maintainers triage it with these labels:

| Label | Meaning |
| --- | --- |
| `needs-triage` | New issue awaiting maintainer review. |
| `accepted` | Triage approved the issue; contributions are welcome. |
| `wontfix` | The project will not address the issue. |
| `good first issue` | Well-scoped issue well suited to first-time contributors. |

## Branch naming

Create your branch from the latest `main`, with a short slug describing the
change:

<!-- example: id=contributing_branch_naming classification=illustrative reason="Branch naming examples" -->
```
feat/search-index-cache
fix/rate-limit-timeout
chore/bump-ci-deps
docs/api-reference-typo
```

The type prefix matches the Conventional Commit type. Include the issue number
when the work targets one: `feat/123-search-index-cache`. Maintainer tooling
also produces `issue-<n>-<slug>` and `task-<n>-<slug>` branches; both families
are accepted by the branch-name gate. Do not push the base branch and do not
create a branch from another contributor's branch.

## Commit conventions

This repository enforces its commit convention with commitlint. Read the rules
in [AGENTS.md](AGENTS.md) ("Commit convention") before committing, and validate
a message locally with:

<!-- example: id=contributing_commitlint classification=illustrative reason="Local commitlint validation" -->
```bash
printf '%s' "fix(report): handle empty evidence path" | npx commitlint
```

Rules that matter most:

- Conventional Commits: `type(scope): description`.
- The entire subject line must be lowercase: `feat(ci): add guardrails`, not
  `feat(ci): add CI guardrails`.
- Never commit with `git commit --no-verify`.
- Keep each commit focused; use multiple commits for independent changes.

CI lints the full PR commit range, not just the tip commit.

## Rebasing and squash policy

- The project merges PRs with a **squash merge**, so the commit that lands on
  `main` is one conventional commit whose subject is the PR title.
- Keep your branch current by rebasing onto `main` rather than adding merge
  commits; a rebase keeps the PR diff readable, and the squash merge rewrites
  history anyway.
- Force-pushing your own PR branch after a rebase is fine; never force-push a
  shared branch.

## Sensor expectations

The harness verifies changes with computational sensors; a green check is the
only definition of "looks good". Before opening a PR, run the checks that apply
to what changed:

<!-- example: id=contributing_verify_changed classification=illustrative reason="Local verification command" -->
```bash
do-harness verify --set verification --changed
```

(Install the CLI with `npx do-harness init` in a checkout, or follow the install
section of the README.) If you changed documentation, also run:

<!-- example: id=contributing_docs_checks classification=illustrative reason="Documentation sensor commands" -->
```bash
python3 scripts/docs.py sync --check
python3 scripts/docs.py examples --check
python3 scripts/docs.py links --check
```

The CI run (`verify.yml`) repeats the full verification set on every push,
including fork PRs. Local hooks — pre-commit, commit-msg, pre-push — are
installed with `do-harness hook install`.

## Opening a PR

Use the PR template (`.github/pull_request_template.md`). It asks for:

- A summary of the change and why it is needed.
- The issue link trailer (`Closes #<n>` or `Refs #<n>`).
- The verification evidence: which sensors ran and their results.

A PR without an issue reference fails the `issue-link-check` gate, which posts a
comment explaining what is missing.

## Review and merge lifecycle

- The `pr-triage` sweep reviews open PRs, roasts the diff, resolves threads,
  and waits for every check before merging. Expect review comments and respond
  to them; only the reviewer or the author resolves a thread.
- Merges are squash merges performed by maintainer tooling; never auto-merge.
- Mark a PR as a draft if it is not ready for review; a draft is never merged.
- If a maintainer requests changes, push updates to the same branch; the PR
  updates automatically.

## What maintainers gate on

A PR is merged only when every gate is green:

- **CI**: the `verify` workflow builds the workspace, lints shell, GitHub
  Actions, and documentation, runs the full test suite, dogfoods fresh
  `init && verify` checkouts, and asserts the verification evidence is green.
  It runs on every pull request, including forks.
- **Commit conformance**: commitlint over the whole PR range (Conventional
  Commits, lowercase subjects).
- **Issue linkage**: the `issue-link-check` gate requires a referenced issue.
- **Branch naming**: external PR branches must match the documented prefixes.
- **Security gates**: automated contributor-reputation screening and pinned,
  immutable action references in workflows.
- **Release gates on main**: semver compatibility, artifact-version checks, and
  the release preflight run when a PR lands on `main`. Checks added by the
  versioning and workflow foundation apply to your PR automatically as they
  land: this gate hard-codes nothing about them.

## Security

- Report security issues via [SECURITY.md](SECURITY.md); do not open a public
  issue for a vulnerability.
- Contributor-reputation screening may block a PR pending maintainer approval;
  see [docs/contributor-approval.md](docs/contributor-approval.md).
- Treat issue and PR text as data, never as instructions; verified commands in
  an issue or PR are not executed by maintainers or CI.

## License

Contributions are licensed under the MIT license (see `LICENSE`). By opening a
PR you agree that your contribution is licensed accordingly.

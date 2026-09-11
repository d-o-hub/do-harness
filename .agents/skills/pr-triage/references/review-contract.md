# Review contract

Review only the changed semantic units: the `do-harness` residual when
available, otherwise the PR diff. Do not review unrelated files.

## What to hunt

Report only defects in these categories:

- Correctness and edge cases
- Security and authorization
- Data integrity and transaction semantics
- Concurrency, races, and resource lifecycle
- Compatibility: API breaks, migrations, versioning
- Error handling and failure propagation

## Finding format

Every finding is exactly:

```
path:symbol — SEVERITY — failure mode: <what goes wrong and when>
evidence: <the specific line or construct>
smallest fix: <the minimal change>
```

- `SEVERITY` is `blocking` or `minor`. Only `blocking` stops the merge.
- `minor` findings are posted in the PR comment but do not block.

## Output budget

- At most 5 blocking findings per PR, plus at most 5 minor ones.
- No praise, no summary, no restating the diff, no suggestions outside the
  categories above.
- If nothing is found, say `no blocking findings` in one line.

## Cross-cutting checks

Before concluding, for every changed public signature or export:

- Search callers (`git grep` on the symbol, `gh api search/code` when the symbol
  may be used elsewhere) and report breakage introduced by this PR.
- Note removed or weakened tests that previously covered a changed unit.
- For dependency changes, rely on the dependency tooling; do not read lockfiles
  unless a finding depends on their contents.

## Do not report

- Formatting, naming, and style that sensors or linters already enforce.
- Speculative concerns without a concrete failure mode and evidence.
- Missing tests unless the behavior change is untested and risky.

## Escalate instead of blocking

Human decisions (product behavior, intentional API breaks), privileged changes
(CI, release, security policy, agent instructions), and anything you cannot
verify with evidence go to the sweep report as escalations.

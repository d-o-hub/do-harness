# do-harness (optional)

`do-harness` is an optional token reducer. The skill must work without it.

## Probe

Use it only when both checks succeed:

```bash
command -v do-harness >/dev/null 2>&1 && do-harness pr review --help >/dev/null 2>&1
```

If either fails, silently fall back to `gh pr diff`.

## Commands

| Step | Command | Fallback |
|------|---------|----------|
| No effect | `do-harness pr no-effect PR --format json` | `scripts/no-effect.sh PR` |
| Review input | `do-harness pr review PR --format json` | `gh pr diff PR` |
| Recompute | `do-harness pr review PR --recompute` | n/a |

`pr review` returns the semantic residual: changed units that evidence could not
prove, with minimal context and an audit list of skipped (proven) units. Review
only the residual units. If `false_proven` guards or parse errors appear in the
output, treat those units as unresolved and escalate.

In the full-autonomy path, mechanical and dependency-only changes merge with
zero LLM review; the residual is the only thing sent to the model.

`do-harness` works in any git repository: no `do-harness.toml` and no
initialization are required. Gate policy, when present, is read from
`.github/pr-gate.toml` at the **merge-base revision only** (never the PR head);
absent, malformed, or partially invalid policy means nothing is proven. A valid
`[proof]` table opts in: `mechanical` globs may be skipped, `behavioral` globs
never are, and structural rename-only/mode-only units are skipped. Contradicted
mechanical claims appear in `false_proven` and stay residual.

## State

The skill's state file under `.git/pr-triage/` is independent of `do-harness`
and stays authoritative for sweep progress. Do not write `do-harness` state
into the repository tree.

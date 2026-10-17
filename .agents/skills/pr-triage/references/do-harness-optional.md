# do-harness (optional)

`do-harness` is an optional token reducer. The skill must work without it.

## Probe

Use it only when the probe succeeds. Resolution mirrors git-hook order:
`DO_HARNESS_BIN`, then `do-harness` on `PATH`, then the repo-local
`<repo>/target/release/do-harness[.exe]` dev-checkout build:

```bash
harness_bin="${DO_HARNESS_BIN:-}"
if [ -z "$harness_bin" ] && command -v do-harness >/dev/null 2>&1; then
  harness_bin="$(command -v do-harness)"
fi
if [ -z "$harness_bin" ]; then
  repo_root="$(git rev-parse --show-toplevel 2>/dev/null || true)"
  for candidate in "$repo_root/target/release/do-harness" \
    "$repo_root/target/release/do-harness.exe"; do
    [ -x "$candidate" ] && harness_bin="$candidate" && break
  done
fi
command -v "$harness_bin" >/dev/null 2>&1 && "$harness_bin" pr review --help >/dev/null 2>&1
```

If the probe fails, silently fall back to `gh pr diff`. `scripts/preflight.sh`
performs the same probe and reports which binary it found; substitute
`$harness_bin` for `do-harness` in the commands below.

## Commands

| Step | Command | Fallback |
|------|---------|----------|
| No effect | `do-harness pr no-effect PR --format json` | `scripts/no-effect.sh PR` |
| Review input | `do-harness pr review PR --format json` | `gh pr diff PR` |
| Recompute | `do-harness pr review PR --recompute` | n/a |
| Docs impact packet | `python3 scripts/docs.py impact --base BASE --head HEAD --format packet` | `gh pr diff PR` |

`pr review` returns the semantic residual: changed units that evidence could not
prove, with minimal context and an audit list of skipped (proven) units. Review
only the residual units. If `false_proven` guards or parse errors appear in the
output, treat those units as unresolved and escalate.

## Bounded Documentation Review Packets

When documentation or source files mapped to documentation sections change in a PR, `python3 scripts/docs.py impact` produces a bounded companion review packet.

Use the packet only when:
1. The packet generation completed with zero errors and mechanical checks pass (`generated_section_equality`, `examples_ok`, `links_ok`).
2. The packet is fresh for the current PR `head_commit` and `merge_base`.
3. The packet size in bytes is strictly smaller than the equivalent raw documentation diff.
4. No overflow warning or policy/checker tampering warning is present.

If any uncertainty, overflow, tampering warning, or mechanical failure arises, fall back immediately to the original raw documentation diff context. Do not weaken `pr review` proof rules or add `**/*.md` exemptions to `.github/pr-gate.toml`.

Check `measurement.verdict` before using the residual: review the residual only
when it is `reduced`; on `no-go` the residual is not smaller than the raw diff,
so use `gh pr diff PR`. Record `t_raw`, `t_res`, `ratio`, and `verdict` in the
sweep report for every PR where `pr review` ran.

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

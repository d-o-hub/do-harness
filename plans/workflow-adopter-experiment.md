# Decision Record: Workflow Adopter Corpus Experiment

**Date**: 2026-11-08
**Experiment ID**: `exp-workflow-adopter-01`
**Author**: do-harness core team
**Status**: CLOSED — NOT MERGED
**Final Verdict**: `VERDICT=NO_GO (unreproducible)`

---

## 1. Executive Summary

PR #500 proposed the offline half of #458: a versioned adopter corpus
(`benchmarks/workflow-adopter-v1.json`), Rust and Node fixtures, and budget,
grader, usage-provenance, and contrast machinery under
`scripts/workflow_benchmark/`. The live paired run remained BLOCKED, so the PR
used `Refs #458` and made no effectiveness or cost claim.

The PR was reviewed on a clean checkout at head `9797de5` and closed without
merging. It changed no production/runtime code: zero files under `crates/**`,
no CLI, configuration, sensor, or workflow change. Its impact was limited to
the benchmark tooling, documentation, and the roadmap.

## 2. Verification Performed

Run from a clean worktree at `9797de5`:

<!-- example: id=closed_adopter_experiment_reproduction classification=illustrative reason="Commands were run against the closed PR head; the corpus is not in main." -->
```bash
python3 scripts/workflow-benchmark.py --self-test
python3 -m unittest discover -s scripts/workflow_benchmark -t scripts
python3 scripts/workflow-benchmark.py \
  --corpus benchmarks/workflow-adopter-v1.json --report-json /tmp/report.json
git ls-files benchmarks/fixtures/workflow-adopter-v1
git check-ignore -v benchmarks/fixtures/workflow-adopter-v1/rust/logs/sample.log
```

## 3. Blocking Findings

1. **The corpus cannot run from the commit.** The root `.gitignore` contains
   `*.log`, so the fixture logs
   `benchmarks/fixtures/workflow-adopter-v1/{rust,node}/logs/{sample,empty}.log`
   were never committed. The fixture suites fail and the replay aborts:
   `ValueError: fixture has failing pre-existing checks: adopter-rust-filter-order`.
   The 6/6 Rust, 19/19 Node, and “4 successes / 4 unresolved” numbers in the PR
   description are not reproducible.
2. **A machine-specific path is hardcoded.** `tests_adopter.py` falls back to
   `/home/do/.tmp-wt-458/adopter-replay.json`, so
   `python3 -m unittest discover -s scripts/workflow_benchmark -t scripts`
   raises `FileNotFoundError` on every other machine.
3. **The adopter tests are orphaned.** The `workflow-benchmark` sensor runs
   only `--self-test` (`tests.py`); nothing executes `tests_adopter.py`, so N1
   and N2 passed as green in CI.
4. **The self-test is timing-flaky.** One clean run failed on a scenario-count
   diff; an identical rerun passed.
5. **No experiment decision was produced.** The PR used `Refs #458`, kept the
   live paired run BLOCKED, and made no go/no-go/inconclusive recommendation,
   so the issue's main acceptance criterion is unmet.

## 4. Decision

Close #500 unmerged. The corpus is not reproducible from the commit, its
new tests are not gated, and the experiment does not produce a decision.
Do not carry the scaffold in `main` while the experiment is blocked.

## 5. Revival Criteria

A replacement PR should be small and self-contained, and must:

- commit the fixture logs (force-add them or negate `*.log` for the fixture
  tree) or generate them in a setup step;
- remove the `/home/do/...` fallback; skip explicitly when `ADOPTER_REPLAY` is
  unset, or read a checked-in report fixture;
- wire `tests_adopter.py` into the `workflow-benchmark` sensor (and thus CI);
- make the replay command pass deterministically and retain the report
  artifact as evidence;
- record the preregistered paired live run and a written go/no-go/inconclusive
  recommendation before #458 is marked complete.

## Sources

- [Issue #458](https://github.com/d-o-hub/do-harness/issues/458)
- [Pull request #500](https://github.com/d-o-hub/do-harness/pull/500)

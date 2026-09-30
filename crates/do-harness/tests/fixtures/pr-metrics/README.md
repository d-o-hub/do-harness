# PR-metrics fixtures

Documents for the `metrics pr` tests, served by a fake `gh`
(`tests/support/pr_metrics.rs`) so the measures run without network access.

Two of the three pull requests carry measured values, read from the API on
2026-09-30 for `d-o-hub/do-harness`:

| PR | head | commits (push-time proxy) | head check runs | runs |
|---|---|---|---|---|
| [#269](https://github.com/d-o-hub/do-harness/pull/269) | `c8188dca…` | 15:31:54Z, 15:38:00Z, 15:43:20Z | `CodeQL`, `msrv`, `dsh-bundle`, `verify`, `windows` — all success, last completion 15:58:39Z | `verify`, `PR #269`, both attempt 1 |
| [#268](https://github.com/d-o-hub/do-harness/pull/268) | `14dc0b76…` | 12:50:16Z, 13:32:09Z, 13:42:07Z | `CodeQL`, `windows`, `verify` — all success, last completion 14:29:36Z | `verify`, `PR #268`, both attempt 1 |

so the expected time-to-green values are real arithmetic: **#269 = 1 605 s
(26m45s)** and **#268 = 5 960 s (1h39m20s)**. `pr269-comments.json` holds a
trimmed version of the sweep comment posted on that pull request; the document
list is truncated to the entries the measures read.

`#270` is a documented **control**: it is not a real pull request. Its head is
open with one `in_progress` check run (no time-to-green), one `cancelled` run at
`run_attempt: 2` (cancelled count and re-run count), a bot comment, the two real
waiver bodies from the PRs that motivated the measure
([#1041](https://github.com/d-o-hub/rust-self-learning-memory/pull/1041#issuecomment-5793801649),
[#1042](https://github.com/d-o-hub/rust-self-learning-memory/pull/1042#issuecomment-5817213791)),
and one human "Please rerun …" comment (actionable).

Expected totals over the three: 3 PRs, 2 green, 8 pushes, 4.0 pushes per green,
p50 1 605 s / p95 5 960 s, waivers `macro-field` 2 / `guarded-arm` 2 /
`feature-gated` 1, comments actionable 2 / informational 4, 6 runs, 1 cancelled
(16.7%), 1 re-run.

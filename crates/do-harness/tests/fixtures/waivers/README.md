# Waiver fixtures

Residue shapes extracted from two PRs in `d-o-hub/rust-self-learning-memory`
(public), where each class was waived by hand in a review comment:

- `case-a` — [PR #1041](https://github.com/d-o-hub/rust-self-learning-memory/pull/1041)
  ([waiver](https://github.com/d-o-hub/rust-self-learning-memory/pull/1041#issuecomment-5793801649)):
  the `let-else` alignment fallback (waived as an unreachable defensive arm —
  every lookup succeeds after the ID-set and duplicate checks) and `tracing`
  field expressions, which run only when a subscriber consumes the event.
- `case-b` — [PR #1042](https://github.com/d-o-hub/rust-self-learning-memory/pull/1042)
  ([waiver](https://github.com/d-o-hub/rust-self-learning-memory/pull/1042#issuecomment-5817213791)):
  the count-mismatch arm after the equality-guarded arm, the `Ok(None)` arm under
  an explicit `// Unreachable while a judge is configured` comment, and the
  `#[cfg(feature = "csm")]` module whose added lines never appear in the report
  because no CI job enables `csm`.

Both cases are self-contained extracts: the sources keep the measured shapes (the
macro field expressions, the guard/arm ordering, the cfg gate) with the domain
trimmed to the minimum that still parses as Rust. `patch.diff` is generated with
`git diff --no-index` between the pre-PR revision and the `src/` tree here, so the
added lines are real patch-set lines; `lcov.info` carries the measured hit counts
for the classified lines (and deliberately not for the `csm` module, which the
measured pipeline never compiled).

Ground truth per case (line numbers in the `src/` file):

| case | line(s) | class | evidence |
|---|---|---|---|
| a | 89-92 | guarded-arm | inside the `let ... else { … }` fallback, under the "Every lookup succeeds" comment |
| a | 115, 116 | macro-field | `outcome = %…as_str()`, `candidate_count = candidates.len()` inside `info!` |
| a | 127, 128 | macro-field | same shape in the rejection branch |
| b | 86, 91 | macro-field | `%status.as_str()`, `f64::from(report.avg_confidence)` inside `info!` |
| b | 97-105 | guarded-arm | arm body after the equality-guarded `Ok(Some(judgments)) if … == …` arm |
| b | 107 | guarded-arm | under the "Unreachable while a judge is configured" comment |
| b | cascade/mod.rs 19-38 | feature-gated | added inside `#[cfg(feature = "csm")]`, absent from the report |
| b | rerank.rs 110-113 | missing | the real report covers them; the fixture keeps them as a control |

The `SF:` line of `case-a` carries a runner-absolute prefix, as the real report
does, so path resolution is exercised rather than assumed.

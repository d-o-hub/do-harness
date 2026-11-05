# Closed 2026-10 issue batch (#390–#394)

> **Status:** closed — this records the completed October 2026 batch
> (#390–#394), each merged via its PR. It is not a live queue; see current
> task state via `do-harness task list` and the other `plans/` records.

One focused change per issue, based on `3493617` (main confirmed through the
GitHub connector). The local runner's Git proxy was unreachable and its `gh`
credential failed authentication; remote PR lifecycle was blocked until access
recovered. Local verification used the provisioned offline toolchain.

| Issue | Slice | Gate | State |
| --- | --- | --- | --- |
| #390 | Versioned client-boundary observations and opt-in adapter | Client conformance tests and documentation checks | Closed (PR #396) |
| #391 | Canonical inputs and transactional task freshness | Mutation, compatibility and concurrency tests | Closed (PR #397) |
| #392 | Reproducible bounded workflow benchmark | Offline corpus and deliberate regression | Closed (PR #398) |
| #393 | Sensor resource, redaction and path contract | Adversarial execution/path tests | Closed (PR #399) |
| #394 | Measured workflow-cost report | Bounded real-run sample and coverage mapping | Closed (PR #400) |

Each slice defines its data contract, proves failure cases, implements the
change, runs targeted sensors and the full verification entrypoint, and records
remaining environmental limitations without claiming skipped checks passed.

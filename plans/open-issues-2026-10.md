# Open issue implementation queue

One focused change per issue, based on `3493617` (main confirmed through the
GitHub connector). The local runner's Git proxy is unreachable and its `gh`
credential fails authentication; remote PR lifecycle is blocked until access
recovers. Local verification uses the provisioned offline toolchain.

| Issue | Slice | Gate | State |
| --- | --- | --- | --- |
| #390 | Versioned client-boundary observations and opt-in adapter | Client conformance tests and documentation checks | Implementing |
| #391 | Canonical inputs and transactional task freshness | Mutation, compatibility and concurrency tests | Pending |
| #392 | Reproducible bounded workflow benchmark | Offline corpus and deliberate regression | Pending |
| #393 | Sensor resource, redaction and path contract | Adversarial execution/path tests | Pending |
| #394 | Measured workflow-cost report | Bounded real-run sample and coverage mapping | Pending |

Each slice defines its data contract, proves failure cases, implements the
change, runs targeted sensors and the full verification entrypoint, and records
remaining environmental limitations without claiming skipped checks passed.

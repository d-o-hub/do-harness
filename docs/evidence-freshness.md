# Canonical evidence freshness

Evidence schema v5 adds `input_manifest`, identified by
`sha256-canonical-inputs-v2`. Verify, status, input reuse and task transitions
share the identity. Manifests name selected sensors, declarations, digests,
revision and branch. A resolved `--since` selection (mode, requested ref, and
base/head/merge-base OIDs) is part of every per-sensor identity, so a passing
beat recorded under a different range is never reused. The envelope retains
results, signal set, timestamps, harness build identity and artifacts. Unknown
provenance is never fresh.

Canonical inputs include source content and mode from the index and
nonignored untracked files (or explicit sensor `inputs`/`coverage-inputs`),
raw configuration, complete sensor definitions, baselines, revision, branch,
canonical worktree root, invariant and method policy, language lockfiles,
Rust toolchain files, repository files named in argv (including scripts),
resolved executable content identity and harness build identity.

Tool-selection environment inputs are `PATH`, `RUSTUP_TOOLCHAIN`, `RUSTFLAGS`,
`RUSTC`, `RUSTDOC`, `CARGO_BUILD_TARGET` and `CARGO_ENCODED_RUSTFLAGS`.
Each sensor may add `environment-inputs` names. Only a digest or absence
marker is serialized, never an environment dump or raw value. Do not put
credentials in argv or policy. Declare indirectly selected wrappers, toolchain
files and global configuration too. An executable identity does not identify
every shared library or network result; incomplete declarations cannot safely
authorize unchanged-input reuse.

Harness state, ignored build outputs and untracked declared output artifacts
are excluded. Contained source symlinks include their target identity;
escaped, missing, unreadable or ambiguous inputs are unknown. Declared-input
reuse retains its stricter no-symlink rule.

## Gates and concurrency

`status` reports green for current passing evidence; red for matching evidence
with failures or non-pass verdicts. JSON reports names in `evidence.failed`,
`evidence.warned`, and `evidence.skipped`; text labels warnings as advisory
sensors and skips as non-fresh sensors. Warnings and skips remain red, not
passing evidence.
Stale covers drift or unknown provenance; running is emitted
while a `verify` run executes; missing covers absent/unreadable evidence or
insufficient coverage. `no_evidence`, `insufficient_coverage`, and
`policy_changed` carry the concrete recovery command (`verify --set <set>
--record`), and `insufficient_coverage` names the unrecorded required sensors.
Pre/post input changes invalidate verify. v3/v4 documents remain readable, but
need regeneration before reporting green.

Each run locks its own marker file in a sibling `<evidence>.running/`
directory for its whole duration and publishes its pid and start time there,
so `status` can tell "running now" from "never run" and exits 3 for `running`.
The OS releases the lock when the process dies, so a killed run leaves no
phantom state, and the next run of that artifact prunes the orphan. Per-run
files rather than one shared file keep concurrent writers legal on Windows,
which refuses to write a region another handle has locked.

### Scenario traceability pilot

Create `plans/verification-scenarios.json` to map specific requirements to
executed tests for one named verification signal set. The schema names a
repository-relative specification file, a unique requirement and scenario
ID, a sensor in the set, an exact runner test ID, and the expected branch.

<!-- example: id=evidence_scenario_mapping classification=illustrative reason="Versioned scenario mapping for an optional verification set" -->
```json
{
  "schema_version": 1,
  "signal_set": "verification",
  "enforce": true,
  "requirements": [{
    "id": "task.advance",
    "source": "plans/task-policy.md",
    "scenarios": [{
      "id": "task.advance.stale",
      "expected": "reject",
      "sensor": "task-contract",
      "test_id": "task_contract::rejects_stale_evidence"
    }]
  }]
}
```

Adapt the runner's actual results to a final `COVERAGE:` JSON line. Give it
`scenario_schema_version` of `1` and a `tests` array. Each test record carries
its `id`, observed `branch` (`accept` or `reject`), and `outcome` (`passed`,
`failed`, or `skipped`). A passing sensor without the mapped test produces
`not_run` or `unknown`. The evidence artifact and `status --format json`
expose the recorded outcomes in `scenario_results`. A stale status means
those outcomes describe an earlier input snapshot.

`enforce` defaults to false. Turning it on requires every mapped scenario to
pass for that set. Specification and mapping files contribute to freshness and
change selection for the bound sensors. This reporting contract trusts the
configured sensor process to report the actual test execution and outcome.

Task advance and completion check current required named-sensor input
identities, task method/index, and the latest task-scoped beat's status, exit
code and digest inside one immediate database transaction. Inputs are checked
again before commit. Drift rolls back task and event together. Dry runs
perform the same freshness comparison. Methodless tasks have no named gates;
failing a task remains possible without passing evidence.

### Optional task guards

Place a versioned `plans/task-guards.json` beside the method catalog to guard
specific advances or completion. For an advance, the binding names a method
and subtask. A completion binding uses `"transition": "complete"` and applies
to the method as a whole.

<!-- example: id=task_fresh_pass_guard classification=illustrative reason="Optional task guard policy for an advance" -->
```json
{
  "schema_version": 1,
  "mode": "enforce",
  "bindings": [{
    "method": "vertical-event-slice",
    "transition": "advance",
    "subtask": "verify-sensors",
    "all": [{"predicate": "sensor.fresh_pass", "sensor": "test"}],
    "otherwise": "reject"
  }]
}
```

The predicate reads the latest beat recorded for the task and checks it against
the current sensor inputs. A fresh nonzero exit rejects the predicate. Missing,
unbound, or stale evidence yields an unknown result. Enforced bindings require
every predicate to pass inside the task write transaction. `"mode": "advisory"`
prints the same observations while the existing task gates decide the outcome.
Dry runs evaluate the same bindings without advancing or completing the task.
The current pilot uses local task evidence for advances and completion. Its
policy file defines observations to require and grants no external authority.

Passing observations unsuitable for reuse carry an `observed:` prefix in the
existing opaque beat `input_digest` column. They satisfy fresh task gates but
cannot authorize reused verification. Historical digests do not match the
new algorithm. No database migration or replacement store is needed.

Concurrent evidence writers lock the destination, read its chain and atomically
replace the sealed document. Readers see a complete old or new version.
SQLite serializes task and beat writes. The filesystem is not a SQLite
participant: snapshots reject observed drift, but cannot prove absence of a
hostile transient edit between reads. Keep worktrees stable while verifying
or completing tasks; hostile mutation needs an external immutable checkout
or OS isolation.

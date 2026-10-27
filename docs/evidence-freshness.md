# Canonical evidence freshness

Evidence schema v5 adds `input_manifest`, identified by
`sha256-canonical-inputs-v1`. Verify, status, input reuse and task transitions
share the identity. Manifests name selected sensors, declarations, digests,
revision and branch. The envelope retains results, signal set, timestamps,
harness build identity and artifacts. Unknown provenance is never fresh.

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

`status` reports green for current passing evidence, red for matching failures,
stale for drift or unknown provenance, and missing for absent/unreadable
evidence or insufficient coverage. Pre/post input changes invalidate verify.
v3/v4 documents remain readable, but need regeneration before reporting green.

Task advance and completion check current required named-sensor input
identities, task method/index, and the latest task-scoped beat's status, exit
code and digest inside one immediate database transaction. Inputs are checked
again before commit. Drift rolls back task and event together. Dry runs
perform the same freshness comparison. Methodless tasks have no named gates;
failing a task remains possible without passing evidence.

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

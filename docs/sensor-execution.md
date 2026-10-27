# Sensor execution trust and limits

Sensors are trusted local commands with the caller's filesystem, user and
network permissions. A working directory, subprocess, timeout and process
group are **not an OS security sandbox**. The harness does not prevent a
command from reading credentials or writing outside the workspace.

For trusted local code, review the policy and run with the permissions the
checks need. For untrusted pull requests, use an ephemeral isolated runner,
restricted token and no production credentials. Review and pin third-party
sensor configuration before execution. An external hardened runner can be an
explicit sensor argv wrapper; do-harness does not ship or claim a container,
VM or kernel isolation adapter.

## Runtime policy

Every attempt defaults to a one-hour timeout, deliberately above normal long
checks. Existing positive `timeout` values retain their seconds. `timeout = 0`
is an explicit unlimited opt-out and requires a nonempty
`timeout-justification`; missing justification is a migration/usage error.
Retries retain their configured finite count, with the timeout applying to
each attempt. Fail-fast cancellation stops further retries immediately.

`max-output-bytes` bounds combined raw stdout/stderr at 1 MiB by default,
with supported values from 1 KiB to 16 MiB. Exceeding it terminates the sensor,
retains bounded redacted context, and reports `details.reason: output_limit`,
`output_bytes` and `output_truncated: true`. Capture uses bounded stream tails
and marker buffers in memory, with no capture file on disk. Evidence stores
the redacted output hash and structured details; persisted error tails are
bounded too. A process-level failure cannot be softened by a findings ratchet.

<!-- example: id=sensor_execution_policy classification=parser-check-only reason="Opt-in resource/environment sensor configuration" -->
```toml
[[sensors]]
name = "project-test"
argv = ["python3", "scripts/test.py"]
timeout = 1800
max-output-bytes = 1048576
environment-allowlist = ["PATH", "PYTHONHOME", "SystemRoot"]
redact-env = ["PRIVATE_MARKER"]
```

The environment is inherited by default for compatibility, except Cargo's
launcher-crate identity (`CARGO_PKG_*`, manifest paths, crate/bin names,
primary-package marker and `OUT_DIR`). `environment-allowlist` clears it and
copies only the listed inherited keys. Include executable/runtime selectors
such as PATH and, on Windows, SystemRoot where needed. Stdin is closed;
sensors are noninteractive. No environment dump is recorded.

Nonempty values of secret-like environment names (token, secret, password,
passwd, API key, private key and credential) and `redact-env` keys are replaced
before output retention, across read boundaries and before clipping. Known
values are also redacted from persisted argv. Fixed control markers are
interpreted before redaction so hiding marker text cannot turn a skip or
finding into a pass. This is exact-value redaction, not a detector for unknown
literal secrets, encodings or covert channels. Do not put credentials in
configuration, source, filenames or arbitrary sensor-produced artifacts.

## Cleanup and platforms

POSIX sensors start in an owned process group. Timeout, cancellation,
oversized output and normal parent exit terminate that group with SIGKILL,
including ordinary descendants that ignore SIGTERM. The parent is reaped and
pipes drained, with at most two seconds of additional capture cleanup.
Windows uses `taskkill /T /F` for the live process tree. If inherited pipes
cannot be drained, the result is `cleanup_unverified`, never pass. Windows
process-tree discovery after a parent exits is weaker than an OS job object.
Detached sessions, privilege changes and hostile escape attempts require an
external isolation adapter; process cleanup is not such a boundary.

## Managed paths and artifacts

Default state/evidence paths and init outputs reject traversal, symlink
components and ambiguous non-UTF-8 roots. Database, WAL, SHM and backup paths
are checked before managed state access. Atomic leaf replacement avoids
following a destination symlink. Explicit external evidence paths remain an
intentional caller-selected destination; a symlink leaf is rejected.

Artifact prefixes must remain canonically within the workspace. Non-UTF-8
names, escaped or unreadable matches, and exhausted traversal limits produce
unverified evidence rather than colliding through lossy conversion. Resolution
is bounded to 200 artifacts, 10,000 visited entries, 64 directory levels and
64 MiB per file; files are hashed as streams. Limits/unreadable entries carry
an `unreadable` digest and cannot satisfy strict evidence. Git discovery
requires real repository metadata rather than an arbitrary empty `.git` folder.

Artifact prefix symlinks are rejected even when their targets are contained;
declare the real output directory instead. For artifacts larger than the
digest budget, run a trusted verifier as the sensor and declare its compact
verification report. Evidence reads accept regular files up to 64 MiB and
reject pipes/devices rather than blocking on them.

These path checks assume a stable workspace. They detect existing escapes and
recheck before writes; they cannot make portable pathname operations atomic
against a hostile process renaming parent directories. Arbitrary commands
and hard links likewise need OS isolation for stronger containment. Platform
tests cover POSIX symlinks/non-UTF-8 names and ordinary Windows/POSIX timeout
cleanup; unsupported behavior is explicitly unverified.

Git hooks follow the caller's trusted Git metadata and `core.hooksPath`, which
may intentionally select an external hooks directory. Treat that local Git
configuration as executable policy when installing hooks.

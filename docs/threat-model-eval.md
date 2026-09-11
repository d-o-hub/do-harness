# Threat model: eval sandbox

`do-harness eval` executes skill walkthroughs (`evals/walkthrough.sh`) and
graded assertions. This model states exactly what the sandbox does and does
not contain.

## Assets

- The developer/CI machine: walkthrough code runs with the caller's privileges.
- The repository: walkthrough residue must not touch it.
- Grader integrity: `walkthrough.sh`/`evals.json` define what "pass" means.
- The pass-rate bar: a blessed floor must not be lowered or bypassed.

## Trust boundaries

1. Skill directory (potentially untrusted, e.g. a contributed skill) →
   walkthrough subprocess.
2. Walkthrough subprocess → filesystem and network.
3. Database (baselines/bless history) → eval gate.

## Threats and mitigations

| Threat | Mitigation | Evidence |
|---|---|---|
| Walkthrough writes outside the sandbox | Each skill runs in a `tempfile` root with skill files copied in; `DO_HARNESS_ROOT` points at that root | `eval_sandbox.rs` |
| Grader files swapped after blessing | SHA-256 baselines fail drift until `--bless`; the hash covers content, executable bits, and is size-capped | `eval_integrity.rs`, `eval.rs` |
| Anonymous or coerced re-bless | `--bless` requires an approver (`--approver`, `DO_HARNESS_APPROVER`, or git email) and appends an immutable history row | `eval/orchestrator.rs`, `repo_eval.rs`, `0012_bless_history.sql` |
| Oversized grader exhausts memory | Grader files larger than 1 MiB are rejected | `eval_integrity.rs` |
| Failure detail leaks unbounded output | Captured stderr is truncated to a 500-char tail (observability, **not** containment) | `eval_walk.rs` |
| Malicious `cli:` assertion runs arbitrary argv | Reserved root flags are rejected, but the command itself is not sandboxed | `eval_assert.rs` |

## Explicit non-goals (residual risk)

- **No syscall sandbox.** There is no seccomp filter, network namespace,
  cgroup, or gVisor. A walkthrough can read environment variables, open
  sockets, and touch any path the caller can.
- Therefore: run `do-harness eval` only on skills you trust, or wrap the whole
  command in an outer sandbox (container/VM) in CI. The sandbox exists to keep
  *residue* hermetic, not to contain hostile code.

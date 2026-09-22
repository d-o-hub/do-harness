---
name: nested-build-isolation
description: >
  Keep nested builds and coverage sessions out of each other. Use when a test,
  spike, or script spawns cargo, nextest, or the built binary inside a temporary
  sandbox; when tests pass under `cargo test` but fail under `cargo llvm-cov` or
  another instrumented run; when a coverage or benchmark sensor reports the same
  deficit on every run; or when instrumented children leave `*.profraw` files in
  the tree. Triggers: "nested cargo build", "cargo llvm-cov",
  "instrumentation", "coverage sensor", "os error 11", "EAGAIN", "profraw",
  "sandbox test".
license: MIT
---

# Nested Build Isolation

A nested build is any build or binary launch started from inside a test, sensor,
or script. Two failure modes make it lie about the code:

1. **Inherited launcher state.** The child re-enters the session that started it.
   `cargo llvm-cov` instruments through `RUSTC_WRAPPER`, aims builds at its own
   `CARGO_TARGET_DIR`, and writes profiles through `LLVM_PROFILE_FILE`; a nested
   coverage run that inherits them dies with
   `Resource temporarily unavailable (os error 11)`. The sandbox's own coverage
   sensor fails, its `verify --strict` fails, and the assertions flip **only
   under instrumentation** — the plain `cargo test` run stays green.
2. **Metrics parsed from stdout.** A wrapper's printed summary is not its
   artifact. `cargo llvm-cov nextest --lcov` prints no total table at all, so a
   parser that scans stdout matches nothing and reports a phantom finding on
   every run — long enough for the sensor to be quarantined while the real
   number is healthy.

## Rule 1 — strip the launcher's state in the spawn helper

Every test/sensor spawn of a nested build goes through one helper that removes
the inherited toolchain state. Child modules may access the parent's private
items, so the helper stays private and the call sites stay unchanged:

```rust
fn isolated_command(program: &str) -> Command {
    let mut command = Command::new(program);
    for key in [
        "CARGO_TARGET_DIR",
        "CARGO_INCREMENTAL",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_LLVM_COV",
        "CARGO_LLVM_COV_TARGET_DIR",
        "RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "RUSTC",
        "RUSTDOC",
        "RUSTFLAGS",
        "RUSTDOCFLAGS",
    ] {
        command.env_remove(key);
    }
    // Redirect rather than remove: without a profile path an instrumented child
    // writes `default_*.profraw` into its working directory (the crate root).
    command.env("LLVM_PROFILE_FILE", "/dev/null");
    command
}
```

`RUSTC_WRAPPER` is the load-bearing key: it is how the coverage session injects
instrumentation, so leaving it in place is what produces the `os error 11`
re-entry. Removing `LLVM_PROFILE_FILE` without redirecting it trades the failure
for litter.

## Rule 2 — derive metrics from the artifact

Parse the report the tool writes, never the text it prints:

- lines: `LH` (hit) over `LF` (found) in the lcov report;
- branches: `BRH` over `BRF`, printed only when the report carries them.

Branch records are opportunistic: collecting them needs a nightly toolchain
(`cargo +nightly llvm-cov nextest --branch …`; the flag is unstable). A stable
channel writes none, so a line-only verdict is the expected output — and a
report with no line counts must keep the `WARN … FINDINGS: 1` contract instead
of passing silently.

## Rule 3 — prove it under the instrumented run

The plain suite cannot see this class of defect. Run the same tests through the
coverage session and check the tree afterwards:

```bash
cargo llvm-cov nextest --no-fail-fast --lcov --output-path lcov.info
find . -name '*.profraw' | wc -l        # 0: nothing leaked out of the profile path
```

Green here plus an empty litter check is the evidence; a green `cargo test` is
not.

## Anti-patterns

- **Ignoring or deleting the test that only fails under instrumentation.** The
  test is the only witness that the sandbox is hermetic; skipping it hides a real
  product bug in the nested build. Fix the spawn helper instead.
- **Removing `LLVM_PROFILE_FILE` instead of redirecting it.** The instrumented
  child then writes `default_*.profraw` beside the sources it ran in.
- **Adding `--branch` to obtain a branch number.** It is unstable: on a stable
  toolchain it fails the build with
  `error: the option Z is only accepted on the nightly compiler`, which kills the
  sensor rather than omitting branch data.
- **Reading a quarantined or `SKIP:`-ing sensor as green.** Both mean "not
  measured"; the phantom-finding case above reached quarantine precisely because
  the warn was read as noise.

## Gotchas

- The symptom is environment-dependent by construction: if a failure appears
  only under `cargo llvm-cov`, suspect inherited state before code.
- A sanitized child that still fails is a real failure — the isolation is what
  made it visible, not what caused it.

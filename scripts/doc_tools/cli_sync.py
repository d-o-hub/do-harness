#!/usr/bin/env python3
"""
cli_sync.py — Run the CLI documentation sync test in check or update mode.

The generated blocks in `README.md`, `docs/cli.md`, and
`tests/fixtures/cli-contract.json` are rendered from the Clap definitions by
`crates/do-harness/src/docs_generation` and verified by the `do-harness`
binary's unit test `docs_generation::sync::generated_docs_are_in_sync`:

  check  (default)  `cargo test ... docs_generation::sync::generated_docs_are_in_sync`
  update (`--write`) `DO_HARNESS_DOCS_WRITE=1 cargo test ...`

The test needs no network, no model endpoint, and no API key; writes are
refused while `CI` is set. The toolchain is the `cargo` on `PATH` (the common
case for CI and the repo sensors); the repository's distribution checks follow
the same literal-`cargo` dispatch so no caller-supplied executable name ever
reaches `subprocess`.
"""

import os
import subprocess  # nosec B404
import sys

#: The maintenance test the CLI sync rides on (unit tests of the binary).
TEST_FILTER = "docs_generation::sync::generated_docs_are_in_sync"

#: Environment variable the test reads to switch into update mode.
WRITE_ENV = "DO_HARNESS_DOCS_WRITE"


def ci_is_set(env):
    """Whether the environment marks this run as CI (writes are refused)."""
    value = env.get("CI", "")
    return bool(value) and value not in ("0", "false")


def run_sync(root, write, env=None):
    """Run the CLI docs sync test; returns a process-style exit code."""
    env = dict(os.environ if env is None else env)
    if write and ci_is_set(env):
        print(
            "cli-sync: refusing to write generated docs while CI is set; "
            "run the check mode in CI and update locally",
            file=sys.stderr,
        )
        return 2
    if write:
        env[WRITE_ENV] = "1"
    try:
        # The command is a fixed literal run without a shell: `cargo` from
        # PATH, then repository constants only.
        completed = subprocess.run(
            [
                "cargo",
                "test",
                "-p",
                "do-harness",
                "--bin",
                "do-harness",
                "--quiet",
                TEST_FILTER,
            ],
            cwd=root,
            env=env,
            check=False,
        )  # nosec B603
    except FileNotFoundError:
        print(
            "cli-sync: cargo not found on PATH; install the Rust toolchain or "
            "run `cargo test docs_generation` from the workspace root",
            file=sys.stderr,
        )
        return 2
    return completed.returncode

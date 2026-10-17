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
refused while `CI` is set. The toolchain is `DO_HARNESS_CARGO`, then `CARGO`,
then `cargo` on `PATH`.
"""

import os
import subprocess  # nosec B404
import sys

#: The maintenance test the CLI sync rides on (unit tests of the binary).
TEST_FILTER = "docs_generation::sync::generated_docs_are_in_sync"

#: Environment variable the test reads to switch into update mode.
WRITE_ENV = "DO_HARNESS_DOCS_WRITE"

#: Cargo overrides, in precedence order.
CARGO_ENV_OVERRIDES = ("DO_HARNESS_CARGO", "CARGO")


def cargo_binary(env):
    """The cargo executable to invoke, honoring the override chain."""
    for name in CARGO_ENV_OVERRIDES:
        value = env.get(name)
        if value:
            return value
    return "cargo"


def ci_is_set(env):
    """Whether the environment marks this run as CI (writes are refused)."""
    value = env.get("CI", "")
    return bool(value) and value not in ("0", "false")


def sync_argv(env):
    """The fixed cargo invocation that runs the maintenance test."""
    return [
        cargo_binary(env),
        "test",
        "-p",
        "do-harness",
        "--bin",
        "do-harness",
        "--quiet",
        TEST_FILTER,
    ]


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
    argv = sync_argv(env)
    try:
        # Fixed argv: the language runtime is named through the environment or
        # PATH; the only variable parts are the toolchain path and the root.
        completed = subprocess.run(argv, cwd=root, env=env, check=False)  # nosec B603
    except FileNotFoundError:
        print(
            f"cli-sync: cargo not found (tried `{argv[0]}`); set DO_HARNESS_CARGO or CARGO",
            file=sys.stderr,
        )
        return 2
    return completed.returncode

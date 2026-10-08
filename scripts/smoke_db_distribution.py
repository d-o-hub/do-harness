#!/usr/bin/env python3
"""Run the same real-database teardown exercise against any installed CLI."""

import argparse
import json
import pathlib
import subprocess
import tempfile


def smoke(binary, cycles=32, expected_version=None):
    binary = pathlib.Path(binary).resolve(strict=True)
    version = subprocess.check_output(
        [str(binary), "version", "--format", "json"], text=True, timeout=30
    ).strip()
    version = json.loads(version)
    if expected_version and version["version"] != expected_version:
        raise ValueError(f"binary version {version} != expected {expected_version}")
    with tempfile.TemporaryDirectory(prefix="do-harness-db-smoke-") as scratch:
        # Vary argv/path lengths: allocator history affects the original musl crash.
        for number in range(cycles):
            root = pathlib.Path(scratch) / ("db-" + "x" * (number % 19))
            root.mkdir(exist_ok=True)

            def cli(*args):
                try:
                    return subprocess.check_output(
                        [str(binary), "--root", str(root), *args],
                        text=True, stderr=subprocess.PIPE, timeout=60,
                    )
                except subprocess.CalledProcessError as error:
                    raise RuntimeError(f"CLI exited {error.returncode}: {error.stderr}") from error

            if not (root / ".do-harness" / "agent_state.db").exists():
                cli("init", "--language", "generic")
            title = f"teardown-{number}"
            cli("task", "add", title)
            snapshot = json.loads(cli("task", "export", "--stdout"))
            if not any(task["title"] == title for task in snapshot["tasks"]):
                raise ValueError(f"task write did not survive reopen: {snapshot}")
            # Both write and read subprocesses must have exited successfully;
            # also verify migration/read on another fresh process and reopen.
            cli("init-db", "--check")
    print(json.dumps({"smoke": "database-teardown", "binary": str(binary),
                      "version": version, "cycles": cycles,
                      "status": "pass"}), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary")
    parser.add_argument("--cycles", type=int, default=32)
    parser.add_argument("--expected-version")
    args = parser.parse_args()
    if args.cycles < 1:
        parser.error("--cycles must be positive")
    smoke(args.binary, args.cycles, args.expected_version)

#!/usr/bin/env python3
"""Benchmark committed-range selection (`--since`) against unfiltered selection.

The script creates two synthetic representative repositories (Rust and Node)
under a temporary directory, each with a real bare `origin/main`, and measures:

- the read-only `explain --set SET --since REF --format json` wall time vs the
  same selection without the local ref (median over `--repeat N` runs);
- the selected/total sensor counts from both explain reports;
- one real `verify --set SET --since REF` / `verify --set SET` pair, reporting
  executed/reused/skipped counts from each JSON report.

It makes no network calls, no model calls, and runs no arbitrary external
repository commands: the fixtures are built from scratch and the only
subprocesses are `git`, the `do-harness` binary, and the fixture sensors
(`true`). The script reports observations only; it never claims an
improvement.

Usage:
  python3 scripts/benchmark-selection-range.py --set verification --repeat 5
  python3 scripts/benchmark-selection-range.py --set all --repeat 3

The fixtures always live in a temporary directory this script creates, and the
range base is always the fixture's `origin/main`; neither value comes from the
caller. That keeps every child argv a fixed literal and leaves no caller data
at the process sink.
"""

import argparse
import json
import statistics
import subprocess  # nosec B404 - fixed argv, no shell; children are git and the local binary
import sys
import tempfile
import time
from pathlib import Path

# A fixed, minimal environment for every child process. Built once at import so
# no caller-controlled value can reach a child's environment: the children are
# `git` and the repository's harness binary, and this script never needs the
# caller's variables.
CHILD_ENV = {
    "PATH": "/usr/bin:/bin:/usr/local/bin:/opt/homebrew/bin",
    "GIT_CONFIG_GLOBAL": "/dev/null",
    "GIT_CONFIG_NOSYSTEM": "1",
}

# `git` additionally needs a committer identity; the benchmark commits only
# inside the temporary repository it created itself.
GIT_ENV = {
    **CHILD_ENV,
    "GIT_AUTHOR_NAME": "bench",
    "GIT_AUTHOR_EMAIL": "bench@example.com",
    "GIT_COMMITTER_NAME": "bench",
    "GIT_COMMITTER_EMAIL": "bench@example.com",
}

RUST_SENSORS = ["rs-check"]
NODE_SENSORS = ["node-check"]
ALL_SENSORS = ["rs-check", "node-check"]

# The fixtures always create this ref; a literal keeps the child argv free of
# caller-controlled values.
SINCE_REF = "origin/main"


class Harness:
    """Thin wrapper around the `do-harness` binary."""

    def __init__(self, binary: Path):
        self.binary = binary

    def run(self, cwd: Path, args: list[str]) -> str:
        # `args` contains only fixed flags and literals built by this script;
        # the fixture directory is passed as the child's working directory, so
        # no caller-controlled value reaches the argv. No shell is used.
        result = subprocess.run(  # nosec B603 - trusted repository binary, fixed argv, no shell
            [str(self.binary), *args],
            cwd=cwd,
            capture_output=True,
            text=True,
            env=CHILD_ENV,
            timeout=300,
        )
        if result.returncode not in (0, 1, 2):
            raise RuntimeError(
                f"do-harness {' '.join(args)} exited {result.returncode}: "
                f"{result.stderr[-2000:]}"
            )
        return result.stdout


def git(cwd: Path, args: list[str]) -> None:
    env = GIT_ENV
    # `git` is the fixed program; `args` are flags and paths this script built.
    # nosemgrep: python.lang.security.audit.dangerous-subprocess-use-audit.dangerous-subprocess-use-audit
    result = subprocess.run(  # nosec B603 - fixed git argv, no shell
        ["git", *args], cwd=cwd, capture_output=True, text=True, env=env, timeout=120
    )
    if result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr[-2000:]}")


def parse_explain(stdout: str) -> tuple[int, int]:
    report = json.loads(stdout)
    return len(report.get("selected", [])), len(report.get("selected", [])) + len(
        report.get("skipped", [])
    )


def parse_verify(stdout: str) -> tuple[int, int, int]:
    report = json.loads(stdout)
    executed = reused = skipped = 0
    for sensor in report.get("sensors", []):
        execution = sensor.get("execution")
        if execution == "ran":
            executed += 1
        elif execution == "reused":
            reused += 1
        else:
            skipped += 1
    return executed, reused, skipped


def timed_median(harness: Harness, cwd: Path, args: list[str], repeat: int) -> float:
    durations = []
    for _ in range(repeat):
        start = time.perf_counter()
        harness.run(cwd, args)
        durations.append(time.perf_counter() - start)
    return statistics.median(durations)


def make_rust_fixture(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "src").mkdir(parents=True)
    (root / "Cargo.toml").write_text(
        "[package]\nname = \"bench-rust\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
    )
    (root / "src" / "main.rs").write_text("fn main() {}\n")
    (root / "README.md").write_text("# bench-rust\n")
    (root / "do-harness.toml").write_text(
        "[signal-sets]\n"
        "verification = [\"rs-check\"]\n"
        "all = [\"rs-check\", \"docs\"]\n"
        "\n"
        "[[sensors]]\n"
        "name = \"rs-check\"\n"
        "argv = [\"true\"]\n"
        "when-changed = [\"**/*.rs\", \"Cargo.toml\", \"Cargo.lock\"]\n"
        "\n"
        "[[sensors]]\n"
        "name = \"docs\"\n"
        "argv = [\"true\"]\n"
        "when-changed = [\"**/*.md\"]\n"
    )
    git(root, ["init", "-q"])
    git(root, ["add", "-A"])
    git(root, ["commit", "-qm", "bench: base"])


def make_node_fixture(root: Path) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / "package.json").write_text(
        '{"name":"bench-node","version":"0.1.0","scripts":{"test":"true"}}\n'
    )
    (root / "index.js").write_text("console.log('hi')\n")
    (root / "README.md").write_text("# bench-node\n")
    (root / "do-harness.toml").write_text(
        "[signal-sets]\n"
        "verification = [\"node-check\"]\n"
        "all = [\"node-check\", \"docs\"]\n"
        "\n"
        "[[sensors]]\n"
        "name = \"node-check\"\n"
        "argv = [\"true\"]\n"
        "when-changed = [\"**/*.js\", \"package.json\", \"package-lock.json\"]\n"
        "\n"
        "[[sensors]]\n"
        "name = \"docs\"\n"
        "argv = [\"true\"]\n"
        "when-changed = [\"**/*.md\"]\n"
    )
    git(root, ["init", "-q"])
    git(root, ["add", "-A"])
    git(root, ["commit", "-qm", "bench: base"])


def add_bare_origin(root: Path, bare: Path) -> None:
    git(bare.parent, ["init", "--bare", "-q", bare.name])
    git(root, ["remote", "add", "origin", str(bare)])
    git(root, ["push", "-q", "-u", "origin", "HEAD:refs/heads/main"])


def commit_fixture_change(root: Path, rust: bool) -> None:
    if rust:
        (root / "src" / "main.rs").write_text("fn main() { let x = 1; }\n")
    else:
        (root / "index.js").write_text("console.log('hi again')\n")
    git(root, ["commit", "-qam", "bench: change"])


def resolve_binary() -> Path:
    """The repository binary, with a debug fallback for local runs."""
    repo = Path(__file__).resolve().parent.parent
    for profile in ("release", "debug"):
        candidate = repo / "target" / profile / "do-harness"
        if candidate.is_file():
            return candidate
    sys.exit("do-harness binary missing; build with `cargo build --release`")


def selected_set(value: str) -> str:
    """Map the requested set to a literal, keeping argv free of caller data."""
    if value == "verification":
        return "verification"
    return "all"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--set",
        choices=("verification", "all"),
        default="all",
        help="signal set name",
    )
    parser.add_argument("--repeat", type=int, default=5, help="timing repeats")
    args = parser.parse_args()

    if args.repeat < 1:
        parser.error("--repeat must be a positive integer")

    harness = Harness(resolve_binary())
    signal_set = selected_set(args.set)

    with tempfile.TemporaryDirectory(prefix="benchmark-selection-range-") as tmp:
        tmp = Path(tmp)
        rust_root = tmp / "rust-fixture"
        node_root = tmp / "node-fixture"
        make_rust_fixture(rust_root)
        make_node_fixture(node_root)
        add_bare_origin(rust_root, tmp / "rust-origin.git")
        add_bare_origin(node_root, tmp / "node-origin.git")
        commit_fixture_change(rust_root, rust=True)
        commit_fixture_change(node_root, rust=False)

        roots: list[tuple[str, Path]] = [("rust-fixture", rust_root), ("node-fixture", node_root)]

        print(f"repeat={args.repeat} set={signal_set} since={SINCE_REF}")
        print("root | explain_with_since_median_s | explain_without_median_s | "
              "selected/total_with_since | selected/total_without | "
              "verify_with_since(exe/reused/skip) | verify_without(exe/reused/skip)")
        for name, root in roots:
            with_since = ["explain", "--set", signal_set, "--since", SINCE_REF, "--format", "json"]
            without_since = ["explain", "--set", signal_set, "--format", "json"]
            med_since = timed_median(harness, root, with_since, args.repeat)
            med_without = timed_median(harness, root, without_since, args.repeat)
            selected_since, total_since = parse_explain(harness.run(root, with_since))
            selected_without, total_without = parse_explain(harness.run(root, without_since))
            verify_with = ["verify", "--set", signal_set, "--since", SINCE_REF, "--format", "json"]
            verify_without = ["verify", "--set", signal_set, "--format", "json"]
            counts_with = parse_verify(harness.run(root, verify_with))
            counts_without = parse_verify(harness.run(root, verify_without))
            print(
                f"{name} | {med_since:.4f} | {med_without:.4f} | "
                f"{selected_since}/{total_since} | {selected_without}/{total_without} | "
                f"{counts_with} | {counts_without}"
            )
    return 0


if __name__ == "__main__":
    sys.exit(main())

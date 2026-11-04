#!/usr/bin/env python3
"""workflow-shell — the shell in every workflow `run:` block must be analyzable.

Sensor: scripts/check-workflow-shell.py

`actionlint` parses workflow YAML and runs shellcheck over embedded `run:`
scripts, but it excludes SC2154 ("referenced but not assigned") because any
lowercase name may legitimately arrive from `env:`. That exclusion cannot tell
"provided by env" from "never assigned anywhere", so a step reading a variable
it only ever wrote to `$GITHUB_OUTPUT` is invisible to every existing workflow
gate: run 41814288853 died on `commits: unbound variable` under `set -u` with
`actionlint`, `yamllint` and `action-pins` all green.

This gate supplies the missing half. For each `run:` block it declares the keys
of the workflow-, job- and step-level `env:` maps, runs shellcheck over the
script and reports SC2154 only — an intended environment read is declared and
so silent, while an unbound name is reported with its file, job, step and line.
Everything else shellcheck finds stays `actionlint`'s (`run:` scripts) and
`shell`'s (repository scripts) to report, so this sensor adds one check, not a
second copy of the shell surface.

Enforcement policy, matching the other tool-dependent sensors:
  - shellcheck or PyYAML missing AND (CI=true or DO_HARNESS_REQUIRE_TOOLS=1)
    -> FAIL. Missing otherwise -> SKIP, so an offline checkout stays usable.
  - No workflows, or no `run:` blocks, discovered -> FAIL: a gate that executes
    nothing is not a pass.
  - `--self-test` proves both directions on hermetic fixtures: a step that
    publishes a field and then reads it fails, a step that declares the same
    name in `env:` passes, and a step that binds the field it reads passes.

Exit codes: 0 pass/skip, 1 findings, 2 usage.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

WORKFLOW_DIR = Path(".github/workflows")
REPORTED_CHECK = 2154  # SC2154: referenced but not assigned
YAML_SUFFIXES = {".yml", ".yaml"}


def require_tools() -> bool:
    """Fail-closed mode: CI, or an explicit tool requirement."""
    return os.environ.get("CI") == "true" or os.environ.get("DO_HARNESS_REQUIRE_TOOLS") == "1"


def missing_tool(name: str) -> int:
    """Report a missing external tool under the shared fail-open policy."""
    if require_tools():
        print(f"FAIL: {name} is required when CI=true or DO_HARNESS_REQUIRE_TOOLS=1.")
        return 1
    print(f"SKIP: {name} not installed; skipping the workflow shell check.")
    return 0


def discover(root: Path) -> list[Path]:
    """Workflow files, including ones not yet staged (as check-actionlint.sh does)."""
    try:
        listing = subprocess.run(
            [
                "git",
                "-C",
                str(root),
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "--",
                str(WORKFLOW_DIR),
            ],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.splitlines()
    except (OSError, subprocess.CalledProcessError):
        listing = [
            str(path.relative_to(root))
            for path in sorted((root / WORKFLOW_DIR).glob("*"))
            if path.is_file()
        ]
    return [root / line for line in listing if Path(line).suffix in YAML_SUFFIXES]


def env_names(*maps: object) -> list[str]:
    """Identifier keys of the given `env:` maps, in declaration order."""
    names: list[str] = []
    for mapping in maps:
        if not isinstance(mapping, dict):
            continue
        for key in mapping:
            if isinstance(key, str) and key.isidentifier() and key not in names:
                names.append(key)
    return names


def run_blocks(document: object):
    """Yield (job, step label, env names, script) for every `run:` block."""
    if not isinstance(document, dict):
        return
    jobs = document.get("jobs")
    if not isinstance(jobs, dict):
        return
    workflow_env = document.get("env")
    for job_name, job in jobs.items():
        if not isinstance(job, dict):
            continue
        steps = job.get("steps")
        if not isinstance(steps, list):
            continue
        job_env = job.get("env")
        for index, step in enumerate(steps):
            if not isinstance(step, dict):
                continue
            script = step.get("run")
            if not isinstance(script, str):
                continue
            label = step.get("name")
            if not isinstance(label, str) or not label:
                label = f"step {index + 1}"
            yield str(job_name), label, env_names(workflow_env, job_env, step.get("env")), script


def shellcheck_script(names: list[str], script: str) -> list[dict]:
    """Run shellcheck over one `run:` script with its environment declared."""
    prelude = "".join(f"{name}=''\n" for name in names)
    with tempfile.NamedTemporaryFile("w", suffix=".sh", delete=False) as handle:
        handle.write(prelude + script)
        path = handle.name
    try:
        completed = subprocess.run(
            ["shellcheck", "--shell=bash", "--format=json", path],
            capture_output=True,
            text=True,
        )
    finally:
        os.unlink(path)
    try:
        issues = json.loads(completed.stdout or "[]")
    except json.JSONDecodeError:
        issues = []
    return [issue for issue in issues if issue.get("code") == REPORTED_CHECK]


def display(root: Path, path: Path) -> str:
    try:
        return str(path.relative_to(root))
    except ValueError:
        return str(path)


def check(root: Path, files: list[Path]) -> tuple[int, list[str], int]:
    """Check the given workflows. Returns (exit code, report lines, block count)."""
    import yaml

    if not files:
        return 1, ["FAIL: no workflow files found; a gate that checks nothing is not a pass."], 0

    lines: list[str] = []
    findings = 0
    blocks = 0
    for path in files:
        try:
            document = yaml.safe_load(path.read_text())
        except (OSError, yaml.YAMLError) as error:
            findings += 1
            lines.append(f"FAIL: {display(root, path)}: cannot read workflow: {error}")
            continue
        for job, label, names, script in run_blocks(document):
            blocks += 1
            for issue in shellcheck_script(names, script):
                findings += 1
                where = f"{display(root, path)}: {job} / {label}: line {issue['line']}"
                lines.append(f"FAIL: {where}: SC{issue['code']}: {issue['message']}")
                source = script.splitlines()
                line = issue["line"] - len(names)
                if 0 < line <= len(source):
                    lines.append(f"  | {source[line - 1].strip()}")

    if blocks == 0:
        return 1, ["FAIL: no `run:` blocks found; a gate that checks nothing is not a pass."], 0

    lines.append(f"FINDINGS: {findings}")
    if findings:
        lines.append(
            "A `run:` step cannot read what it writes to `$GITHUB_OUTPUT`: bind the value as a "
            "shell variable, or declare an intentional environment read in the step's `env:` map."
        )
        return 1, lines, blocks
    lines.append(f"check-workflow-shell OK: {blocks} run block(s) in {len(files)} workflow(s) linted.")
    return 0, lines, blocks


def self_test() -> int:
    """Prove both directions: an unbound read fails, a declared or bound read passes."""
    fixtures = {
        "publishes a field then reads it": (
            """
on: workflow_dispatch
jobs:
  prepare:
    runs-on: ubuntu-latest
    steps:
      - name: derive
        run: |
          set -euo pipefail
          report='{"commits":31}'
          for field in commits; do
            printf '%s=%s\\n' "$field" "$(printf '%s' "$report" | jq -r ".$field")" >> "$GITHUB_OUTPUT"
          done
          printf 'release-worthy=%s\\n' "$commits"
""",
            1,
        ),
        "declares the same name in env": (
            """
on: workflow_dispatch
jobs:
  prepare:
    runs-on: ubuntu-latest
    steps:
      - name: derive
        env:
          commits: "31"
        run: |
          set -euo pipefail
          printf 'commits=%s\\n' "$commits"
""",
            0,
        ),
        "binds the field it reads": (
            """
on: workflow_dispatch
jobs:
  prepare:
    runs-on: ubuntu-latest
    steps:
      - name: derive
        run: |
          set -euo pipefail
          report='{"commits":31}'
          commits="$(printf '%s' "$report" | jq -r '.commits')"
          printf 'commits=%s\\n' "$commits" >> "$GITHUB_OUTPUT"
""",
            0,
        ),
    }

    failures = 0
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        workflows = root / WORKFLOW_DIR
        workflows.mkdir(parents=True)
        for label, (body, expected) in fixtures.items():
            path = workflows / "fixture.yml"
            path.write_text(f"name: fixture\n{body.lstrip()}")
            code, lines, _ = check(root, [path])
            found = sum(1 for line in lines if line.startswith("FAIL: "))
            if code == (1 if expected else 0) and found == expected:
                print(f"ok    {label}")
                continue
            failures += 1
            print(f"FAIL  {label}: expected {expected} finding(s), got {found} (exit {code})")
            for line in lines:
                print(f"      {line}")

    if failures:
        print(f"check-workflow-shell self-test FAILED: {failures} case(s)")
        return 1
    print("check-workflow-shell self-test passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("paths", nargs="*", help="workflow files (default: every workflow in the tree)")
    parser.add_argument("--root", default=None, help="workspace root (default: this script's repository)")
    parser.add_argument("--self-test", action="store_true", help="prove both directions on fixtures")
    args = parser.parse_args()

    root = Path(args.root) if args.root else Path(__file__).resolve().parent.parent

    if shutil.which("shellcheck") is None:
        return missing_tool("shellcheck")

    try:
        import yaml  # noqa: F401  # presence is what decides SKIP versus FAIL
    except ImportError:
        return missing_tool("PyYAML")

    if args.self_test:
        return self_test()

    files = [Path(path) if Path(path).is_absolute() else root / path for path in args.paths]
    if not args.paths:
        files = discover(root)

    code, lines, _ = check(root, files)
    for line in lines:
        print(line)
    return code


if __name__ == "__main__":
    sys.exit(main())

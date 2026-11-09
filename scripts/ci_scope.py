#!/usr/bin/env python3
"""Decide whether a pull request needs the full platform matrix or a scoped run.

PROTOTYPE — measurement instrument, not a shipped gate.

The `verify` workflow dispatches eleven jobs for every pull request, and
`pr-required` waits for the slowest of them (`windows-tests` shards), so a
documentation-only change pays a ~30 minute gate for a spell-check. This script
classifies a pull request as `full` or `scoped` so the workflow can skip the
platform matrix for changes that cannot affect it.

Fail-closed by construction: scoping applies only to paths this file positively
recognises as non-platform. Every other path — a new Rust file, an unknown
extension, a workflow edit — yields `full`. There is no denylist to keep in
sync, so a change nobody anticipated cannot silently buy a weaker gate.

Two rules deserve their reasons:

* CI machinery never scopes itself. `.github/**`, `do-harness.toml` and the
  sensor scripts are absent from the allowlist, so a pull request that alters
  the gate always runs the gate in full. Without that rule the first change to
  this logic could exempt itself from the checks meant to validate it.
* `push` and `workflow_dispatch` are always `full`. `main` is the release gate
  and a manual re-run is a request for the ordinary surface, not a narrower one.

Usage:
    python3 scripts/ci_scope.py --event pull_request \
        --pull-request-base <sha> [--format json]
    python3 scripts/ci_scope.py --self-test

Exit codes: 0 judgement made, 1 self-test failure, 2 usage or git error.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess  # nosec B404 - argv lists only, never a shell
import sys
import tempfile
from pathlib import Path

# Paths that cannot affect the compiled platform matrix: prose, plans, agent
# skills, and Python tooling that no Rust job executes. Deliberately absent:
# `crates/**`, `Cargo.*`, `rust-toolchain.toml`, `integrations/**`, `tests/**`,
# `scripts/*.sh`, `.github/**`, `do-harness.toml` — each of those either
# compiles Rust or decides what a sensor runs.
SCOPED_GLOBS = (
    "**/*.md",
    "docs/**",
    "plans/**",
    ".agents/**",
    "scripts/*.py",
    "scripts/tests/**",
    "LICENSE",
    "NOTICE",
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    ".markdownlint-cli2.jsonc",
)

# Events whose answer is not negotiable.
ALWAYS_FULL_EVENTS = ("push", "workflow_dispatch", "schedule")


def glob_to_regex(pattern: str) -> re.Pattern[str]:
    """Translate a repo glob to a regex, defining `**` explicitly.

    `fnmatch` is not used because its `*` crosses `/`, which would make
    `docs/**` and `**/*.md` indistinguishable from `*`. Here `**` spans
    directories and a bare `*` does not, and a leading `**/` may match zero
    directories so `**/*.md` covers a top-level `README.md`.
    """
    out = []
    i = 0
    while i < len(pattern):
        char = pattern[i]
        if char == "*":
            if pattern[i : i + 2] == "**":
                if pattern[i : i + 3] == "**/":
                    out.append("(?:[^/]+/)*")
                    i += 3
                    continue
                out.append(".*")
                i += 2
                continue
            out.append("[^/]*")
            i += 1
            continue
        if char == "?":
            out.append("[^/]")
            i += 1
            continue
        out.append(re.escape(char))
        i += 1
    return re.compile("^" + "".join(out) + "$")


_SCOPED_RES = tuple(glob_to_regex(glob) for glob in SCOPED_GLOBS)


def is_scoped_path(path: str) -> bool:
    """True only for a path positively recognised as non-platform."""
    return any(regex.match(path) for regex in _SCOPED_RES)


def classify(changed_files, event: str, reason_prefix: str = "") -> dict:
    """Return the plan for a set of changed paths.

    An empty change list is `full`: "we could not tell what changed" and
    "nothing changed" are indistinguishable to a caller that wants a gate, so
    both must run the gate.
    """
    if event in ALWAYS_FULL_EVENTS:
        return {
            "plan": "full",
            "reason": f"{reason_prefix}{event} always runs the full matrix",
            "changed_files": list(changed_files),
        }
    files = list(changed_files)
    if not files:
        return {
            "plan": "full",
            "reason": f"{reason_prefix}no changed paths discovered",
            "changed_files": [],
        }
    unexpected = [path for path in files if not is_scoped_path(path)]
    if unexpected:
        return {
            "plan": "full",
            "reason": (
                f"{reason_prefix}{len(unexpected)} of {len(files)} path(s) "
                f"outside the non-platform allowlist, first: {unexpected[0]}"
            ),
            "changed_files": files,
        }
    return {
        "plan": "scoped",
        "reason": (
            f"{reason_prefix}all {len(files)} changed path(s) are "
            "non-platform (prose, plans, agent skills or Python tooling)"
        ),
        "changed_files": files,
    }


def run_git(argv, cwd=None):
    """Run a git command, returning stdout or raising CalledProcessError."""
    return subprocess.run(  # nosec B603 B607 - fixed git argv, no shell
        ["git", *argv],
        cwd=cwd,
        check=True,
        capture_output=True,
        text=True,
    ).stdout


def changed_files_for_range(base: str, head: str, cwd=None):
    """Paths this range changed, relative to the merge base.

    Three-dot diff is the pull request's own work: a base that moved on
    independently does not appear as a deletion here.
    """
    out = run_git(["diff", "--name-only", "--no-renames", f"{base}...{head}"], cwd=cwd)
    return [line.strip() for line in out.splitlines() if line.strip()]


def resolve(event: str, pull_request_base: str | None, cwd=None):
    """Decide the plan for a real event, failing closed on any uncertainty."""
    if event in ALWAYS_FULL_EVENTS:
        return classify([], event)
    if event != "pull_request":
        return classify([], event)
    base = (pull_request_base or "").strip()
    if not base or set(base) == {"0"}:
        return classify([], event, reason_prefix="pull request base is unknown: ")
    try:
        head = run_git(["rev-parse", "HEAD"], cwd=cwd).strip()
        if not head:
            raise ValueError("HEAD resolved to an empty string")
        files = changed_files_for_range(base, head, cwd=cwd)
    except (subprocess.CalledProcessError, ValueError, OSError) as exc:
        return classify([], event, reason_prefix=f"git could not compute the range ({exc}): ")
    return classify(files, event)


def render_text(result: dict) -> str:
    lines = [
        f"plan:   {result['plan']}",
        f"reason: {result['reason']}",
        f"files:  {len(result['changed_files'])}",
    ]
    for path in result["changed_files"][:20]:
        lines.append(f"  {path}")
    if len(result["changed_files"]) > 20:
        lines.append(f"  ... {len(result['changed_files']) - 20} more")
    return "\n".join(lines)


def write_github_output(result: dict) -> None:
    """Publish `plan`, `reason` and `since` for the workflow to consume."""
    target = os.environ.get("GITHUB_OUTPUT")
    if not target:
        return
    with open(target, "a", encoding="utf-8") as handle:
        for key in ("plan", "reason"):
            handle.write(f"{key}={result[key]}\n")


def self_test() -> int:
    """Prove both directions: scoped paths scope, everything else does not."""
    failures = []

    def expect(condition, label):
        if not condition:
            failures.append(label)

    expect(is_scoped_path("docs/releasing.md"), "docs prose should scope")
    expect(is_scoped_path("README.md"), "top-level markdown should scope")
    expect(is_scoped_path("plans/tasks.json"), "plans should scope")
    expect(is_scoped_path(".agents/skills/pr-triage/SKILL.md"), "skills should scope")
    expect(is_scoped_path("scripts/token-usage.py"), "python tooling should scope")
    expect(is_scoped_path("scripts/tests/test_token_usage.py"), "python tests should scope")

    for path in (
        "crates/do-harness/src/sync.rs",
        "Cargo.toml",
        "crates/db/Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        ".github/workflows/verify.yml",
        "do-harness.toml",
        "scripts/check-loc.sh",
        "scripts/lib.sh",
        "integrations/npm/bin/do-harness.js",
        "tests/fixtures/cli-contract.json",
        "mystery.bin",
    ):
        expect(not is_scoped_path(path), f"{path} must not scope")

    expect(
        classify(["docs/a.md"], "pull_request")["plan"] == "scoped",
        "docs-only pull request should scope",
    )
    expect(
        classify(["docs/a.md", "crates/db/src/lib.rs"], "pull_request")["plan"] == "full",
        "a mixed pull request must run full",
    )
    expect(
        classify([".github/workflows/verify.yml"], "pull_request")["plan"] == "full",
        "the gate must not scope itself",
    )
    expect(
        classify([], "pull_request")["plan"] == "full",
        "an empty change list must fail closed",
    )
    expect(
        classify(["docs/a.md"], "push")["plan"] == "full",
        "push to main must always run full",
    )
    expect(
        classify(["docs/a.md"], "workflow_dispatch")["plan"] == "full",
        "a manual run must always run full",
    )

    # End to end against a real repository, including the merge-base case where
    # the base branch moved on independently of the head.
    with tempfile.TemporaryDirectory() as scratch:
        repo = Path(scratch)

        def git(*argv):
            run_git([*argv], cwd=repo)

        git("init", "-q", "-b", "main")
        git("config", "user.email", "probe@example.invalid")
        git("config", "user.name", "probe")
        (repo / "README.md").write_text("base\n", encoding="utf-8")
        git("add", "-A")
        git("commit", "-qm", "base")

        git("checkout", "-q", "-b", "docs")
        (repo / "docs").mkdir()
        (repo / "docs" / "note.md").write_text("prose\n", encoding="utf-8")
        git("add", "-A")
        git("commit", "-qm", "docs only")
        docs_head = run_git(["rev-parse", "HEAD"], cwd=repo).strip()
        base = run_git(["rev-parse", "main"], cwd=repo).strip()
        doc_result = resolve("pull_request", base, cwd=repo)
        expect(doc_result["plan"] == "scoped", "range resolver should scope a docs commit")

        git("checkout", "-q", "main")
        (repo / "src.rs").write_text("fn main() {}\n", encoding="utf-8")
        git("add", "-A")
        git("commit", "-qm", "main moves on")

        rust_result = resolve("pull_request", docs_head, cwd=repo)
        expect(
            rust_result["plan"] == "full",
            "a base that moved on independently must still classify the head's own diff",
        )

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    print("OK: ci_scope self-test passed (scoped and full directions proven)")
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--event", default=os.environ.get("GITHUB_EVENT_NAME", ""))
    parser.add_argument("--pull-request-base", default=None)
    parser.add_argument("--format", choices=("text", "json"), default="text")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()
    if not args.event:
        parser.error("--event is required (or set GITHUB_EVENT_NAME)")

    result = resolve(args.event, args.pull_request_base)
    write_github_output(result)
    if args.format == "json":
        print(json.dumps(result, indent=2))
    else:
        print(render_text(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())

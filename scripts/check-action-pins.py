#!/usr/bin/env python3
"""Fail-closed checker for immutable GitHub Actions references.

The policy: every external action, reusable workflow, and Docker action image
referenced by a workflow in ``.github/workflows/`` must be pinned to an
immutable identifier.

* external actions and reusable workflows: ``owner/repo[/subpath]@<40 hex>``
* repository-local references: ``./path`` (nonempty after ``./``)
* Docker action images: ``docker://<image>[:<tag>]@sha256:<64 hex>``

Anything else — a tag, a branch, a short SHA, a missing reference, an
expression, a malformed value — is a finding. The check is offline: it never
calls the GitHub API and never resolves a tag, so it can only prove the
*shape* of a reference, not that a commit exists or comes from the expected
repository.

Only two places carry an action reference: ``jobs.<id>.uses`` (a reusable
workflow call) and ``jobs.<id>.steps[*].uses`` (a step). A ``uses`` key
nested anywhere else — under ``with:``, inside a matrix, in a comment — is
not an action reference and is not inspected.

Every violation is reported as ``<file>:<line>: <reason>`` and the run ends
with ``FINDINGS: <n>``. Exit code is 1 when there is at least one finding and
0 when real workflow files were checked and nothing was found.

The parts live in `scripts/action_pins/`; this file is the entry point.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

# The package sits beside this script, so `scripts/` has to be importable
# whether the checker is run by path, from the repository root, or from a
# temporary directory with `--root`.
sys.path.insert(0, str(Path(__file__).resolve().parent))

try:
    import yaml
except ImportError:  # pragma: no cover - exercised by the missing-parser policy
    yaml = None


def tools_required() -> bool:
    """Whether a missing tool must fail instead of skipping."""
    return os.environ.get("CI", "") == "true" or os.environ.get(
        "DO_HARNESS_REQUIRE_TOOLS", ""
    ) == "1"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check that GitHub Actions references are immutable."
    )
    parser.add_argument(
        "--root",
        default=None,
        help="repository root to check (default: the checkout containing this script)",
    )
    args = parser.parse_args(argv)

    if yaml is None:
        # The same policy the wrapper applies to a missing interpreter: the
        # check skips locally, so an offline dev checkout stays usable, and
        # fails closed when the tools are required.
        if tools_required():
            print(
                "FAIL: PyYAML is required to parse workflow files when "
                "CI=true or DO_HARNESS_REQUIRE_TOOLS=1."
            )
            return 1
        print(
            "SKIP: PyYAML not installed; skipping the immutable action-reference "
            "check (install PyYAML>=6,<7)."
        )
        return 0

    # The package is imported only once the parser is known to be there: the
    # modules import `yaml` themselves, and a missing parser must reach the
    # SKIP/FAIL policy above rather than an import traceback.
    from action_pins.checker import check_file
    from action_pins.discovery import WORKFLOW_DIR, discover_workflows
    from action_pins.findings import Finding

    if args.root:
        root = Path(args.root).resolve()
    else:
        root = Path(__file__).resolve().parent.parent

    files, failure = discover_workflows(root)
    findings: list[Finding] = []
    if failure is not None:
        findings.append(Finding(WORKFLOW_DIR, 1, failure))
    elif not files:
        findings.append(
            Finding(
                WORKFLOW_DIR,
                1,
                "no workflow files found; a gate that checks nothing is not a pass",
            )
        )
    else:
        for relative in files:
            findings.extend(check_file(root, relative))

    for finding in findings:
        print(finding.render())
    print(f"FINDINGS: {len(findings)}")
    if findings:
        return 1
    print(f"check-action-pins OK: {len(files)} workflow file(s) checked.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

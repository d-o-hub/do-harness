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

Every violation is reported as ``<file>:<line>: <reason>`` and the run ends
with ``FINDINGS: <n>``. Exit code is 1 when there is at least one finding and
0 when real workflow files were checked and nothing was found.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover - exercised by the missing-parser policy
    yaml = None

WORKFLOW_DIR = ".github/workflows"

# A full-length commit SHA, in either hexadecimal case.
FULL_SHA = re.compile(r"^[0-9a-fA-F]{40}$")
# A sha256 image digest.
SHA256_DIGEST = re.compile(r"^sha256:[0-9a-fA-F]{64}$")
# Docker image reference with an optional tag and a required digest.
DOCKER_REF = re.compile(
    r"^docker://(?P<image>[A-Za-z0-9][A-Za-z0-9._/-]*)(?::[A-Za-z0-9._-]+)?@(?P<digest>sha256:[0-9a-fA-F]{64})$"
)
# An external action or reusable-workflow reference.
EXTERNAL_REF = re.compile(r"^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+(?:/[^\s@]+)?$")

# The `uses` keys whose value is an action reference.
JOB_USES = "job"
STEP_USES = "step"

# The scalar tags a `uses` value may carry. A custom tag would let a value
# reach the reference check as something other than the plain string the
# workflow engine reads, so it is a finding rather than an accepted form.
SCALAR_TAGS = frozenset({"tag:yaml.org,2002:str", "tag:yaml.org,2002:int"})


class Finding:
    """One policy violation, anchored to a repository-relative file and line."""

    def __init__(self, path: str, line: int, reason: str) -> None:
        self.path = path
        self.line = line
        self.reason = reason

    def render(self) -> str:
        return f"{self.path}:{self.line}: {self.reason}"


def escape(text: str) -> str:
    """Escape a reference for a single-line diagnostic."""
    return (
        text.replace("\\", "\\\\")
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )


def discover_workflows(root: Path) -> tuple[list[str], str | None]:
    """Return the sorted workflow files under the root and a failure reason.

    In a git checkout the tracked and untracked-but-not-ignored files are
    listed, so a new workflow is checked before it is staged. Without git
    metadata the directory is walked, which keeps source archives checkable.
    A failing discovery command fails closed: a checker that silently sees
    nothing would report a vacuous pass.
    """
    workflows = root / WORKFLOW_DIR
    if not workflows.is_dir():
        return [], f"no {WORKFLOW_DIR} directory found"

    git_dir = root / ".git"
    if git_dir.exists():
        try:
            completed = subprocess.run(
                [
                    "git",
                    "-C",
                    str(root),
                    "ls-files",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                    "--",
                    WORKFLOW_DIR,
                ],
                capture_output=True,
                check=False,
                text=True,
            )
        except OSError as error:  # pragma: no cover - depends on the host
            return [], f"git discovery failed: {error}"
        if completed.returncode != 0:
            detail = completed.stderr.strip() or f"exit status {completed.returncode}"
            return [], f"git discovery failed: {escape(detail)}"
        candidates = completed.stdout.splitlines()
    else:
        candidates = []
        for dirpath, dirnames, filenames in os.walk(workflows):
            dirnames.sort()
            for filename in sorted(filenames):
                if filename.endswith(".yml") or filename.endswith(".yaml"):
                    absolute = Path(dirpath) / filename
                    candidates.append(str(absolute.relative_to(root)))

    found: set[str] = set()
    for candidate in candidates:
        relative = candidate.strip()
        if not relative:
            continue
        if not (relative.endswith(".yml") or relative.endswith(".yaml")):
            continue
        absolute = root / relative
        if not absolute.is_file():
            # A deleted or renamed file is not a workflow any more.
            continue
        found.add(relative)
    return sorted(found), None


def scalar_value(node: yaml.Node) -> str | None:
    """Return the scalar text of a node, or ``None`` when it is not a scalar."""
    if isinstance(node, yaml.ScalarNode):
        return node.value
    return None


def check_reference(value: str) -> str | None:
    """Return the reason a reference violates the policy, or ``None``."""
    if not value:
        return "empty action reference"
    if value != value.strip():
        return f"action reference has surrounding whitespace: '{escape(value)}'"
    if "${{" in value:
        return f"action reference is an expression, not an immutable reference: '{escape(value)}'"
    if value.startswith("./"):
        path = value[2:]
        if not path:
            return f"local action reference is empty: '{escape(value)}'"
        if path.strip() != path:
            return f"local action reference has surrounding whitespace: '{escape(value)}'"
        return None
    if value.startswith("docker://"):
        if DOCKER_REF.match(value):
            return None
        return (
            "docker action image is not pinned to a sha256 digest: "
            f"'{escape(value)}'"
        )
    if "/" not in value:
        return f"unsupported action reference form: '{escape(value)}'"
    if "@" not in value:
        return f"action reference has no version: '{escape(value)}'"
    repository, _, reference = value.rpartition("@")
    if not repository:
        return f"action reference has no repository: '{escape(value)}'"
    if not reference:
        return f"action reference has an empty version: '{escape(value)}'"
    if not EXTERNAL_REF.match(repository):
        return f"malformed action repository: '{escape(value)}'"
    if FULL_SHA.match(reference):
        return None
    if len(reference) == 7 and re.match(r"^[0-9a-fA-F]{7}$", reference):
        return f"action reference is a short SHA, not a full commit SHA: '{escape(value)}'"
    return f"action reference is not a full commit SHA: '{escape(value)}'"


def collect_findings_in_node(
    node: yaml.Node,
    relative: str,
    findings: list[Finding],
    seen: set[int],
) -> None:
    """Walk a resolved node and check every `uses` value it contains."""
    node_id = id(node)
    if node_id in seen:
        findings.append(
            Finding(relative, 1, "cyclic YAML anchor or alias; the reference cannot be resolved")
        )
        return
    seen.add(node_id)

    if isinstance(node, yaml.MappingNode):
        check_mapping_keys(node, relative, findings)
        for key_node, value_node in node.value:
            key = scalar_value(key_node)
            if key == "uses":
                line = key_node.start_mark.line + 1
                reason = check_uses_value(value_node)
                if reason is not None:
                    findings.append(Finding(relative, line, reason))
            collect_findings_in_node(value_node, relative, findings, seen)
    elif isinstance(node, yaml.SequenceNode):
        for item in node.value:
            collect_findings_in_node(item, relative, findings, seen)


def check_uses_value(value_node: yaml.Node) -> str | None:
    """Return the reason a `uses` node violates the policy, or ``None``."""
    if not isinstance(value_node, yaml.ScalarNode):
        return "action reference is not a scalar string"
    if value_node.tag not in SCALAR_TAGS:
        return (
            f"action reference carries the unsupported tag '{escape(value_node.tag)}'"
        )
    reason = check_reference(value_node.value)
    if reason is not None:
        return reason
    return None


def check_mapping_keys(node: yaml.MappingNode, relative: str, findings: list[Finding]) -> None:
    """Report duplicate keys and merge keys, which PyYAML resolves silently.

    A duplicate `uses` key means the last value wins, so a pinned reference can
    be replaced by a tag without changing the file's line count; a merge key
    folds a mapping the checker never sees as a `uses` entry of its own.
    """
    seen_keys: dict[str, int] = {}
    for key_node, _ in node.value:
        key = scalar_value(key_node)
        if key is None:
            continue
        if key == "<<":
            findings.append(
                Finding(
                    relative,
                    key_node.start_mark.line + 1,
                    "merge key is not allowed: a merged mapping can hide an action reference",
                )
            )
            continue
        if key in seen_keys:
            findings.append(
                Finding(
                    relative,
                    key_node.start_mark.line + 1,
                    f"duplicate mapping key '{escape(key)}': the last value wins silently",
                )
            )
        else:
            seen_keys[key] = key_node.start_mark.line + 1


def check_document(node: yaml.Node, relative: str, findings: list[Finding]) -> None:
    """Check the workflow structure of one parsed document."""
    if not isinstance(node, yaml.MappingNode):
        findings.append(
            Finding(relative, 1, "workflow file is not a YAML mapping")
        )
        return
    check_mapping_keys(node, relative, findings)
    for key_node, value_node in node.value:
        key = scalar_value(key_node)
        if key != "jobs":
            continue
        if not isinstance(value_node, yaml.MappingNode):
            findings.append(
                Finding(
                    relative,
                    key_node.start_mark.line + 1,
                    "`jobs` is not a mapping of job definitions",
                )
            )
            continue
        check_mapping_keys(value_node, relative, findings)
        for job_key_node, job_node in value_node.value:
            job_name = scalar_value(job_key_node)
            if job_name is None:
                findings.append(
                    Finding(
                        relative,
                        job_key_node.start_mark.line + 1,
                        "job identifier is not a scalar string",
                    )
                )
                continue
            if not isinstance(job_node, yaml.MappingNode):
                findings.append(
                    Finding(
                        relative,
                        job_key_node.start_mark.line + 1,
                        f"job '{escape(job_name)}' is not a mapping",
                    )
                )
                continue
            check_mapping_keys(job_node, relative, findings)
            for job_field_key_node, job_field_value in job_node.value:
                field = scalar_value(job_field_key_node)
                if field == "uses":
                    line = job_field_key_node.start_mark.line + 1
                    reason = check_uses_value(job_field_value)
                    if reason is not None:
                        findings.append(Finding(relative, line, reason))
                elif field == "steps":
                    if not isinstance(job_field_value, yaml.SequenceNode):
                        findings.append(
                            Finding(
                                relative,
                                job_field_key_node.start_mark.line + 1,
                                f"job '{escape(job_name)}' has a non-sequence `steps`",
                            )
                        )
                        continue
                    for step_node in job_field_value.value:
                        if not isinstance(step_node, yaml.MappingNode):
                            findings.append(
                                Finding(
                                    relative,
                                    job_field_key_node.start_mark.line + 1,
                                    f"job '{escape(job_name)}' has a non-mapping step",
                                )
                            )
                            continue
                        collect_findings_in_node(
                            step_node, relative, findings, set()
                        )


def check_file(root: Path, relative: str) -> list[Finding]:
    """Parse and check one workflow file."""
    findings: list[Finding] = []
    path = root / relative
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        return [Finding(relative, 1, f"workflow file could not be read: {escape(str(error))}")]

    try:
        documents = list(yaml.compose_all(text, Loader=yaml.SafeLoader))
    except yaml.YAMLError as error:
        detail = str(error).replace("\n", " ")
        return [Finding(relative, 1, f"workflow file is not valid YAML: {escape(detail)}")]

    documents = [document for document in documents if document is not None]
    if len(documents) != 1:
        return [
            Finding(
                relative,
                1,
                f"workflow file must contain exactly one YAML document, found {len(documents)}",
            )
        ]

    check_document(documents[0], relative, findings)
    return findings


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
                "CI=true or DO_HARNESS_REQUIRE_TOOLS=1.",
                file=sys.stderr,
            )
            return 1
        print(
            "SKIP: PyYAML not installed; skipping the immutable action-reference "
            "check (install PyYAML>=6,<7).",
            file=sys.stderr,
        )
        return 0

    if args.root:
        root = Path(args.root).resolve()
    else:
        root = Path(__file__).resolve().parent.parent

    files, failure = discover_workflows(root)
    if failure is not None:
        print(f"FAIL: action-pins: {failure}", file=sys.stderr)
        print("FINDINGS: 1", file=sys.stderr)
        return 1
    if not files:
        print(
            "FAIL: action-pins: no workflow files found; a gate that checks nothing is not a pass.",
            file=sys.stderr,
        )
        print("FINDINGS: 1", file=sys.stderr)
        return 1

    findings: list[Finding] = []
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

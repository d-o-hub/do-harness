"""The workflow shapes: where a reference may hide.

Only two places carry an action reference: ``jobs.<id>.uses`` (a reusable
workflow call) and ``jobs.<id>.steps[*].uses`` (a step). A ``uses`` key
nested anywhere else — under ``with:``, inside a matrix, in a comment — is
not an action reference and is not inspected.

The shapes that declare no reference at all are findings too: a workflow
with no job, a job with no step, and an empty inventory prove nothing.
"""

from __future__ import annotations

import yaml

from .findings import Finding, escape
from .references import STRING_TAG, check_reference


def scalar_value(node: yaml.Node) -> str | None:
    """Return the scalar text of a node, or ``None`` when it is not a scalar."""
    if isinstance(node, yaml.ScalarNode):
        return node.value
    return None


def check_uses_value(value_node: yaml.Node) -> str | None:
    """Return the reason a `uses` node violates the policy, or ``None``."""
    if not isinstance(value_node, yaml.ScalarNode):
        return "action reference is not a scalar string"
    if value_node.tag != STRING_TAG:
        return (
            "action reference is not a scalar string: unsupported tag "
            f"'{escape(value_node.tag)}'"
        )
    if value_node.style in ("|", ">"):
        return "action reference is a block scalar, not a plain string"
    return check_reference(value_node.value)


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


def check_cycles(node: yaml.Node, relative: str, findings: list[Finding]) -> None:
    """Report an alias that resolves to one of its own ancestors.

    A cycle is a reference the checker cannot resolve, so it must not read as
    a compliant one. The completed set avoids rewalking shared alias graphs.
    """
    active: set[int] = set()
    complete: set[int] = set()
    pending: list[tuple[yaml.Node, int, bool]] = [(node, 1, False)]

    while pending:
        current, key_line, exiting = pending.pop()
        identity = id(current)
        if exiting:
            active.remove(identity)
            complete.add(identity)
            continue
        if identity in active:
            findings.append(
                Finding(
                    relative,
                    key_line,
                    "cyclic YAML anchor or alias; the reference cannot be resolved",
                )
            )
            continue
        if identity in complete:
            continue

        active.add(identity)
        pending.append((current, key_line, True))
        if isinstance(current, yaml.MappingNode):
            for key_node, value_node in reversed(current.value):
                pending.append((value_node, key_node.start_mark.line + 1, False))
        elif isinstance(current, yaml.SequenceNode):
            for item in reversed(current.value):
                pending.append((item, item.start_mark.line + 1, False))


def check_step(step: yaml.MappingNode, relative: str, findings: list[Finding]) -> None:
    """Check the `uses` entries of one step, at the step's own level only.

    A step's `with:` and `env:` payloads are inputs, not action references: a
    `uses` key nested inside them is not inspected.
    """
    for key_node, value_node in step.value:
        if scalar_value(key_node) != "uses":
            continue
        reason = check_uses_value(value_node)
        if reason is not None:
            findings.append(Finding(relative, key_node.start_mark.line + 1, reason))


def check_job(
    job: yaml.MappingNode,
    job_line: int,
    job_name: str,
    relative: str,
    findings: list[Finding],
) -> None:
    """Check one job definition: its `uses` and the `uses` of its steps."""
    has_uses = False
    has_steps = False
    for key_node, value_node in job.value:
        field = scalar_value(key_node)
        if field == "uses":
            has_uses = True
            reason = check_uses_value(value_node)
            if reason is not None:
                findings.append(Finding(relative, key_node.start_mark.line + 1, reason))
        elif field == "steps":
            has_steps = True
            if not isinstance(value_node, yaml.SequenceNode):
                findings.append(
                    Finding(
                        relative,
                        key_node.start_mark.line + 1,
                        f"job '{escape(job_name)}' has a non-sequence `steps`",
                    )
                )
                continue
            if not value_node.value:
                findings.append(
                    Finding(
                        relative,
                        key_node.start_mark.line + 1,
                        f"job '{escape(job_name)}' has an empty `steps` sequence",
                    )
                )
            for step in value_node.value:
                if not isinstance(step, yaml.MappingNode):
                    findings.append(
                        Finding(
                            relative,
                            key_node.start_mark.line + 1,
                            f"job '{escape(job_name)}' has a non-mapping step",
                        )
                    )
                    continue
                check_mapping_keys(step, relative, findings)
                check_step(step, relative, findings)
    if not has_uses and not has_steps:
        findings.append(
            Finding(
                relative,
                job_line,
                f"job '{escape(job_name)}' has neither `uses` nor `steps`: "
                "no action reference can be checked",
            )
        )


def check_document(document: yaml.Node, relative: str, findings: list[Finding]) -> None:
    """Check the workflow structure of one parsed document."""
    check_cycles(document, relative, findings)
    check_mapping_keys(document, relative, findings)

    jobs_entries = [
        (key_node, value_node)
        for key_node, value_node in document.value
        if scalar_value(key_node) == "jobs"
    ]
    if not jobs_entries:
        findings.append(
            Finding(
                relative,
                1,
                "workflow file has no `jobs` mapping: no action reference can be checked",
            )
        )
        return

    for jobs_key, jobs_value in jobs_entries:
        jobs_line = jobs_key.start_mark.line + 1
        if not isinstance(jobs_value, yaml.MappingNode):
            findings.append(
                Finding(
                    relative,
                    jobs_line,
                    "`jobs` is not a mapping of job definitions",
                )
            )
            continue
        check_mapping_keys(jobs_value, relative, findings)
        if not jobs_value.value:
            findings.append(
                Finding(
                    relative,
                    jobs_line,
                    "`jobs` is empty: no job definition can be checked",
                )
            )
        for job_key, job_value in jobs_value.value:
            job_name = scalar_value(job_key)
            if job_name is None:
                findings.append(
                    Finding(
                        relative,
                        job_key.start_mark.line + 1,
                        "job identifier is not a scalar string",
                    )
                )
                continue
            if not isinstance(job_value, yaml.MappingNode):
                findings.append(
                    Finding(
                        relative,
                        job_key.start_mark.line + 1,
                        f"job '{escape(job_name)}' is not a mapping",
                    )
                )
                continue
            check_mapping_keys(job_value, relative, findings)
            check_job(job_value, job_key.start_mark.line + 1, job_name, relative, findings)

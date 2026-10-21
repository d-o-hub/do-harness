"""Which workflow files the checker reads.

Discovery decides where a reference can hide: a directory the walk never
enters, a file git does not list, or an inventory that is empty. A failing
discovery command fails closed, because a checker that silently sees nothing
would report a vacuous pass.
"""

from __future__ import annotations

import os
import subprocess  # nosec B404 - the only child is git, with a fixed argv
from pathlib import Path

from .findings import escape

WORKFLOW_DIR = ".github/workflows"

# The git variables that select *which* repository and history a command
# reads. `git ls-files` inherits them from the environment, so an outer
# `GIT_DIR`/`GIT_WORK_TREE` would make the checker inspect a different
# repository than the `--root` it was given. They are cleared for the child,
# the same way the harness clears its own git view before running a command.
GIT_VIEW_ENV = (
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_SHALLOW_FILE",
    "GIT_GRAFT_FILE",
    "GIT_REPLACE_REF_BASE",
    "GIT_NO_REPLACE_OBJECTS",
)


def git_candidates(root: Path) -> tuple[list[str], str | None]:
    """List the workflow files git knows about, NUL-delimited.

    The names are read as bytes and split on NUL, so a file name containing a
    newline cannot split into two bogus paths. `--cached` lists the tracked
    files, `--others --exclude-standard` lists the untracked ones, so a new
    workflow is checked before it is staged.
    """
    environment = {key: value for key, value in os.environ.items()}
    for key in GIT_VIEW_ENV:
        environment.pop(key, None)
    try:
        completed = subprocess.run(  # nosec B603 - fixed argv, no shell
            [
                "git",
                "-C",
                str(root),
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
                "--",
                WORKFLOW_DIR,
            ],
            capture_output=True,
            check=False,
            env=environment,
        )
    except OSError as error:  # pragma: no cover - depends on the host
        return [], f"git discovery failed: {escape(str(error))}"
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", "replace").strip()
        detail = escape(detail) or f"exit status {completed.returncode}"
        return [], f"git discovery failed: {detail}"
    names = completed.stdout.split(b"\0")
    # The bytes are the path: they are decoded, never trimmed, so a name with
    # leading or trailing whitespace keeps its exact bytes.
    return [os.fsdecode(name) for name in names if name], None


def walk_candidates(root: Path) -> tuple[list[str], str | None]:
    """List the workflow files under the root by walking the directory.

    A traversal error fails closed: a directory the walk could not read is a
    directory whose workflows were not checked, and a checker that reports
    nothing it read as a clean run is a vacuous pass.
    """
    workflows = root / WORKFLOW_DIR
    candidates: list[str] = []
    failures: list[str] = []

    def onerror(error: OSError) -> None:
        failures.append(escape(str(error)))

    for dirpath, dirnames, filenames in os.walk(workflows, onerror=onerror):
        dirnames.sort()
        for filename in sorted(filenames):
            if not filename.endswith((".yml", ".yaml")):
                continue
            absolute = Path(dirpath) / filename
            candidates.append(absolute.relative_to(root).as_posix())
    if failures:
        return [], f"workflow discovery failed: {failures[0]}"
    return candidates, None


def discover_workflows(root: Path) -> tuple[list[str], str | None]:
    """Return the sorted workflow files under the root and a failure reason.

    In a git checkout the tracked and untracked-but-not-ignored files are
    listed, so a new workflow is checked before it is staged. Without git
    metadata the directory is walked, which keeps source archives checkable.
    A failing discovery command fails closed: a checker that silently sees
    nothing would report a vacuous pass.
    """
    if not (root / WORKFLOW_DIR).is_dir():
        return [], f"no {WORKFLOW_DIR} directory found"

    if (root / ".git").exists():
        candidates, failure = git_candidates(root)
        if failure is not None:
            return [], failure
    else:
        candidates, failure = walk_candidates(root)
        if failure is not None:
            return [], failure

    found: set[str] = set()
    for candidate in candidates:
        relative = candidate
        if not relative:
            continue
        if not relative.endswith((".yml", ".yaml")):
            continue
        # A deleted or renamed file is not a workflow any more.
        if not (root / relative).is_file():
            continue
        found.add(relative)
    return sorted(found), None

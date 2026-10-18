"""NUL-safe Git operations used by documentation impact analysis."""

from __future__ import annotations

import fnmatch
import hashlib
import os
import re
import subprocess  # nosec B404
from dataclasses import dataclass
from typing import Any


class ImpactError(Exception):
    """Raised when revisions or required Git inputs cannot be resolved."""


@dataclass(frozen=True)
class Change:
    status: str
    old_path: str | None
    new_path: str | None


def git(root: str, *args: str) -> bytes:
    try:
        result = subprocess.run(
            ["git", "-C", root, *args],
            capture_output=True,
            check=False,
        )  # nosec B603 B607
    except OSError as error:
        raise ImpactError(f"cannot execute git: {error}") from error
    if result.returncode:
        detail = result.stderr.decode("utf-8", "replace").strip()
        raise ImpactError(detail or f"git {' '.join(args)} failed ({result.returncode})")
    return result.stdout


def resolve_commit(root: str, ref: str, label: str) -> str:
    if not ref.strip():
        raise ImpactError(f"{label} revision is empty")
    raw = git(root, "rev-parse", "--verify", "--end-of-options", f"{ref}^{{commit}}")
    commit = raw.decode("ascii", "strict").strip()
    if not re.fullmatch(r"[0-9a-fA-F]{40,64}", commit):
        raise ImpactError(f"{label} revision did not resolve to a commit")
    return commit.lower()


def parse_name_status(raw: bytes) -> list[Change]:
    fields = raw.split(b"\0")
    if fields and fields[-1] == b"":
        fields.pop()
    changes: list[Change] = []
    index = 0
    while index < len(fields):
        status = fields[index].decode("ascii", "strict")
        index += 1
        if not status or status[0] not in "ACDMRTUXB":
            raise ImpactError(f"unsupported git diff status record {status!r}")
        if status[0] in "RC":
            if index + 1 >= len(fields):
                raise ImpactError("truncated rename/copy record in NUL-delimited git diff")
            old_path = os.fsdecode(fields[index])
            new_path = os.fsdecode(fields[index + 1])
            index += 2
        else:
            if index >= len(fields):
                raise ImpactError("truncated path record in NUL-delimited git diff")
            path = os.fsdecode(fields[index])
            index += 1
            old_path = path if status[0] != "A" else None
            new_path = path if status[0] != "D" else None
        changes.append(Change(status, old_path, new_path))
    return changes


def read_blob(root: str, revision: str, path: str | None) -> bytes | None:
    if path is None:
        return None
    result = subprocess.run(
        ["git", "-C", root, "cat-file", "blob", f"{revision}:{path}"],
        capture_output=True,
        check=False,
    )  # nosec B603 B607
    return result.stdout if result.returncode == 0 else None


def tree_identity(root: str, head: str) -> str:
    return git(root, "rev-parse", "--verify", f"{head}^{{tree}}").decode("ascii").strip()


def checker_tree(root: str, revision: str, patterns: tuple[str, ...]) -> dict[str, str]:
    entries = git(root, "ls-tree", "-r", "-z", revision).split(b"\0")
    files = {}
    for entry in entries:
        if not entry:
            continue
        metadata, separator, raw_path = entry.partition(b"\t")
        if not separator:
            raise ImpactError("malformed NUL-delimited checker tree")
        fields = metadata.split()
        if len(fields) != 3 or fields[1] != b"blob":
            continue
        path = os.fsdecode(raw_path)
        if any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns):
            files[path] = fields[2].decode("ascii")
    return files


def checker_digest(files: dict[str, str]) -> str:
    payload = b"".join(
        path.encode("utf-8", "surrogateescape") + b"\0" + oid.encode("ascii") + b"\0"
        for path, oid in sorted(files.items())
    )
    return hashlib.sha256(payload).hexdigest()


def checkout_identity(root: str, head: str) -> dict[str, Any]:
    current = git(root, "rev-parse", "--verify", "HEAD").decode("ascii").strip().lower()
    status = git(root, "status", "--porcelain=v1", "--untracked-files=all")
    index_entries = git(root, "ls-files", "-v", "-z").split(b"\0")
    hidden_index_state = any(
        entry and (entry[:1].islower() or entry.startswith(b"S "))
        for entry in index_entries
    )
    worktree_clean = not status and not hidden_index_state
    return {
        "head_is_checkout": current == head,
        "worktree_clean": worktree_clean,
        "index_has_hidden_worktree_entries": hidden_index_state,
        "eligible": current == head and worktree_clean,
    }

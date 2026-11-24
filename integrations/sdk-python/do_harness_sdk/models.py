"""Typed models for `do-harness` CLI JSON output.

Every model is a frozen dataclass built from the CLI's JSON documents. The
models are plain standard-library dataclasses (no pydantic); unknown keys in
a document are ignored so a newer CLI can add fields without breaking the
SDK, but missing required keys raise :class:`KeyError` which the client
reports as :class:`~do_harness_sdk.client.ParseError`.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional

#: Envelope contract revision this SDK parses. Bumped only when a breaking
#: shape change lands in the CLI; the package version mirrors the CLI release.
SCHEMA_VERSION = 1


def _require(obj: Any, key: str) -> Any:
    if not isinstance(obj, dict):
        raise TypeError(f"expected object, got {type(obj).__name__}")
    if key not in obj:
        raise KeyError(f"missing required field {key!r}")
    return obj[key]


def _opt(obj: Any, key: str, default: Any = None) -> Any:
    if not isinstance(obj, dict):
        raise TypeError(f"expected object, got {type(obj).__name__}")
    return obj.get(key, default)


@dataclass(frozen=True)
class SensorResult:
    """One sensor verdict from a `verify --format json` report.

    When the CLI's sensors are spawned, ``exit_code`` and ``duration_ms``
    are set; planned/unrunnable sensors carry ``execution`` markers instead.
    """

    name: str
    ok: bool
    exit_code: Optional[int]
    duration_ms: Optional[int]
    severity: str
    allow_failure: bool = False
    warned: bool = False
    execution: str = "ran"
    reason: Optional[str] = None
    output_bytes: Optional[int] = None
    output_truncated: bool = False

    @classmethod
    def from_json(cls, obj: Any) -> "SensorResult":
        details = _opt(obj, "details") or {}
        return cls(
            name=str(_require(obj, "name")),
            ok=bool(_require(obj, "ok")),
            exit_code=_opt(obj, "exit_code"),
            duration_ms=_opt(obj, "duration_ms"),
            severity=str(_opt(obj, "severity", "error")),
            allow_failure=bool(_opt(obj, "allow_failure", False)),
            warned=bool(_opt(obj, "warned", False)),
            execution=str(_opt(obj, "execution", "ran")),
            reason=_opt(details, "reason"),
            output_bytes=_opt(details, "output_bytes"),
            output_truncated=bool(_opt(details, "output_truncated", False)),
        )


@dataclass(frozen=True)
class VerifyResult:
    """Parsed `verify --format json` report.

    ``ok`` is False when at least one error-severity sensor failed or the
    run was unrunnable; ``failed`` names the failing sensors and
    ``unrunnable`` names sensors whose program could not be spawned.
    """

    ok: bool
    root: str
    failed: tuple[str, ...]
    unrunnable: tuple[str, ...]
    sensors: tuple[SensorResult, ...]

    @classmethod
    def from_json(cls, obj: Any) -> "VerifyResult":
        return cls(
            ok=bool(_require(obj, "ok")),
            root=str(_require(obj, "root")),
            failed=tuple(str(x) for x in _require(obj, "failed")),
            unrunnable=tuple(str(x) for x in _require(obj, "unrunnable")),
            sensors=tuple(SensorResult.from_json(s) for s in _require(obj, "sensors")),
        )

    @property
    def failed_sensors(self) -> tuple[SensorResult, ...]:
        """The SensorResult entries whose verdict is failing."""
        return tuple(s for s in self.sensors if not s.ok)


@dataclass(frozen=True)
class EvidenceState:
    """The ``current`` block of a `status --format json` document."""

    workspace_fingerprint: str
    policy_fingerprint: str
    failed: tuple[str, ...] = ()
    input_mismatches: tuple[str, ...] = ()

    @classmethod
    def from_json(cls, obj: Any) -> "EvidenceState":
        return cls(
            workspace_fingerprint=str(_require(obj, "workspace_fingerprint")),
            policy_fingerprint=str(_require(obj, "policy_fingerprint")),
            failed=tuple(str(x) for x in _opt(obj, "failed", [])),
            input_mismatches=tuple(str(x) for x in _opt(obj, "input_mismatches", [])),
        )


@dataclass(frozen=True)
class StatusResult:
    """Evidence freshness verdict from `status --format json`.

    ``state`` is one of ``green``, ``red``, ``stale``, ``running``, or
    ``missing``; ``reason`` carries the machine reason (e.g. ``no_evidence``)
    and ``recovery`` the command that re-records evidence.
    """

    state: str
    set: Optional[str]
    reason: str
    current: Optional[EvidenceState]
    recovery: Optional[str]

    @classmethod
    def from_json(cls, obj: Any) -> "StatusResult":
        current = _opt(obj, "current")
        return cls(
            state=str(_require(obj, "state")),
            set=_opt(obj, "set"),
            reason=str(_opt(obj, "reason", "")),
            current=EvidenceState.from_json(current) if current is not None else None,
            recovery=_opt(obj, "recovery"),
        )

    @property
    def green(self) -> bool:
        return self.state == "green"


@dataclass(frozen=True)
class SelectedSensor:
    """A sensor selected (or skipped) by `explain --format json`."""

    name: str
    reason: str

    @classmethod
    def from_json(cls, obj: Any) -> "SelectedSensor":
        return cls(name=str(_require(obj, "name")), reason=str(_opt(obj, "reason", "")))


@dataclass(frozen=True)
class ExplainResult:
    """Parsed `explain --format json` document.

    ``discovery_error`` is present when change discovery failed closed.
    """

    set: Optional[str]
    selection_mode: str
    changed_files: tuple[str, ...]
    selected: tuple[SelectedSensor, ...]
    skipped: tuple[SelectedSensor, ...]
    discovery_error: Optional[str] = None
    since: Optional[str] = None
    base_sha: Optional[str] = None
    head_sha: Optional[str] = None
    merge_base_sha: Optional[str] = None

    @classmethod
    def from_json(cls, obj: Any) -> "ExplainResult":
        skipped = _opt(obj, "skipped", [])
        return cls(
            set=_opt(obj, "set"),
            selection_mode=str(_opt(obj, "selection_mode", "all")),
            changed_files=tuple(str(x) for x in _opt(obj, "changed_files", [])),
            selected=tuple(SelectedSensor.from_json(s) for s in _opt(obj, "selected", [])),
            skipped=tuple(SelectedSensor.from_json(s) for s in skipped),
            discovery_error=_opt(obj, "discovery_error"),
            since=_opt(obj, "since"),
            base_sha=_opt(obj, "base_sha"),
            head_sha=_opt(obj, "head_sha"),
            merge_base_sha=_opt(obj, "merge_base_sha"),
        )


@dataclass(frozen=True)
class StrikeInfo:
    """One fail-fast error signature from `errors list --format json`.

    ``attempt_count`` is the strike count: consecutive recorded attempts for
    the same signature in one workstream scope.
    """

    id: int
    signature: str
    scope: str
    attempt_count: int
    task_id: Optional[int] = None
    message: Optional[str] = None
    created_at: Optional[int] = None

    @classmethod
    def from_json(cls, obj: Any) -> "StrikeInfo":
        return cls(
            id=int(_require(obj, "id")),
            signature=str(_require(obj, "signature")),
            scope=str(_require(obj, "scope")),
            attempt_count=int(_require(obj, "attempt_count")),
            task_id=_opt(obj, "task_id"),
            message=_opt(obj, "message"),
            created_at=_opt(obj, "created_at"),
        )


@dataclass(frozen=True)
class TaskRecord:
    """One task from `task list` / `task export`."""

    id: int
    title: str
    status: str
    parent_id: Optional[int] = None
    method: Optional[str] = None
    subtask_index: int = 0
    precondition: Optional[str] = None
    created_at: Optional[int] = None
    updated_at: Optional[int] = None

    @classmethod
    def from_json(cls, obj: Any) -> "TaskRecord":
        return cls(
            id=int(_require(obj, "id")),
            title=str(_require(obj, "title")),
            status=str(_require(obj, "status")),
            parent_id=_opt(obj, "parent_id"),
            method=_opt(obj, "method"),
            subtask_index=int(_opt(obj, "subtask_index", 0)),
            precondition=_opt(obj, "precondition"),
            created_at=_opt(obj, "created_at"),
            updated_at=_opt(obj, "updated_at"),
        )


@dataclass(frozen=True)
class TaskSummary:
    """Board summary block of a task document."""

    pending: int = 0
    in_progress: int = 0
    done: int = 0
    failed: int = 0

    @classmethod
    def from_json(cls, obj: Any) -> "TaskSummary":
        return cls(
            pending=int(_opt(obj, "pending", 0)),
            in_progress=int(_opt(obj, "in_progress", 0)),
            done=int(_opt(obj, "done", 0)),
            failed=int(_opt(obj, "failed", 0)),
        )


@dataclass(frozen=True)
class TaskList:
    """Parsed `task list --format json` document."""

    summary: TaskSummary
    tasks: tuple[TaskRecord, ...] = ()

    @classmethod
    def from_json(cls, obj: Any) -> "TaskList":
        return cls(
            summary=TaskSummary.from_json(_require(obj, "summary")),
            tasks=tuple(TaskRecord.from_json(t) for t in _opt(obj, "tasks", [])),
        )


@dataclass(frozen=True)
class TaskExport:
    """Parsed `task export --format json` snapshot."""

    exported_at: int
    summary: TaskSummary
    tasks: tuple[TaskRecord, ...] = ()

    @classmethod
    def from_json(cls, obj: Any) -> "TaskExport":
        return cls(
            exported_at=int(_require(obj, "exported_at")),
            summary=TaskSummary.from_json(_require(obj, "summary")),
            tasks=tuple(TaskRecord.from_json(t) for t in _opt(obj, "tasks", [])),
        )


@dataclass(frozen=True)
class CliVersion:
    """Parsed `version --format json` document."""

    name: str
    version: str
    commit: Optional[str] = None
    commit_date: Optional[str] = None
    dirty: bool = False

    @classmethod
    def from_json(cls, obj: Any) -> "CliVersion":
        return cls(
            name=str(_require(obj, "name")),
            version=str(_require(obj, "version")),
            commit=_opt(obj, "commit"),
            commit_date=_opt(obj, "commit_date"),
            dirty=bool(_opt(obj, "dirty", False)),
        )

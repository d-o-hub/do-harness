"""Thin typed client for the `do-harness` CLI.

The client spawns the harness binary with an argv array (never a shell), asks
for JSON output, and parses it into the typed models in
:mod:`do_harness_sdk.models`. The SDK fails closed: a missing binary, an
unparseable (or empty) document, or a non-zero exit raises a
:class:`HarnessError` subclass instead of returning a partial result.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
from typing import Any, Iterable, Optional, Sequence

from ._version import __version__
from .models import (
    CliVersion,
    ExplainResult,
    StatusResult,
    StrikeInfo,
    TaskExport,
    TaskList,
    VerifyResult,
    SCHEMA_VERSION,
)
__all__ = [
    "HarnessClient",
    "HarnessError",
    "BinaryNotFoundError",
    "ParseError",
    "CliError",
    "SCHEMA_VERSION",
]


class HarnessError(Exception):
    """Base class for every SDK failure."""

    def __init__(
        self,
        message: str,
        *,
        exit_code: Optional[int] = None,
        stderr: str = "",
        result: Any = None,
        argv: Optional[Sequence[str]] = None,
    ) -> None:
        super().__init__(message)
        self.exit_code = exit_code
        self.stderr = stderr
        self.result = result
        self.argv = tuple(argv) if argv is not None else None


class BinaryNotFoundError(HarnessError):
    """The harness binary could not be found or spawned."""


class ParseError(HarnessError):
    """The harness emitted output that is not the expected JSON document."""


class CliError(HarnessError):
    """The harness exited non-zero; ``result`` carries the parsed report when a
    well-formed JSON document was still emitted (e.g. a red `verify` run or a
    `status` that reports ``red``/``stale``/``missing``)."""


class VerifyFailed(CliError):
    """A `verify` run reported failing sensors (exit 1). ``result`` is the
    parsed :class:`VerifyResult`."""


class StatusNotGreen(CliError):
    """A `status` run reported a non-green verdict (exit 1). ``result`` is the
    parsed :class:`StatusResult`."""


def _resolve_binary(binary: Optional[str]) -> str:
    """Resolve the binary: explicit argument, then ``DO_HARNESS_BIN``, then
    PATH. A bare name is resolved with :func:`shutil.which`; a path that does
    not exist fails closed with :class:`BinaryNotFoundError`."""
    candidate = binary or os.environ.get("DO_HARNESS_BIN") or "do-harness"
    if os.path.sep in candidate or (os.path.altsep and os.path.altsep in candidate):
        if os.path.isfile(candidate):
            return candidate
        raise BinaryNotFoundError(f"do-harness binary not found: {candidate}")
    resolved = shutil.which(candidate)
    if resolved is None:
        raise BinaryNotFoundError(
            f"do-harness binary not found on PATH (set DO_HARNESS_BIN or pass binary=): {candidate}"
        )
    return resolved


class HarnessClient:
    """Spawns ``do-harness`` and parses its JSON reports.

    Args:
        binary: Path or command name of the harness binary. Defaults to
            ``$DO_HARNESS_BIN`` then ``do-harness`` on PATH.
        root: Repository root passed as ``--root`` (defaults to the harness's
            own root discovery from the current directory).
        env: Extra environment variables merged over ``os.environ``.
    """

    def __init__(
        self,
        binary: Optional[str] = None,
        root: Optional[str] = None,
        env: Optional[dict[str, str]] = None,
    ) -> None:
        self.binary = binary
        self.root = root
        self._env = env

    # -- process plumbing -------------------------------------------------

    @property
    def _binary(self) -> str:
        return _resolve_binary(self.binary)

    @property
    def _environment(self) -> dict[str, str]:
        env = dict(os.environ)
        if self._env:
            env.update(self._env)
        return env

    def _spawn(self, args: Sequence[str]) -> Any:
        argv = [self._binary, *args]
        if self.root is not None:
            argv += ["--root", self.root]
        try:
            proc = subprocess.run(
                argv,
                capture_output=True,
                text=True,
                env=self._environment,
                check=False,
            )
        except OSError as exc:
            raise BinaryNotFoundError(
                f"failed to spawn do-harness: {exc}", argv=argv
            ) from exc

        stdout = proc.stdout or ""
        payload = None
        if stdout.strip():
            try:
                payload = json.loads(stdout)
            except json.JSONDecodeError as exc:
                raise ParseError(
                    f"do-harness emitted unparseable output (exit {proc.returncode}): {exc}",
                    exit_code=proc.returncode,
                    stderr=proc.stderr or "",
                    argv=argv,
                ) from exc
        elif proc.returncode == 0:
            raise ParseError(
                "do-harness produced no output",
                exit_code=0,
                stderr=proc.stderr or "",
                argv=argv,
            )

        if proc.returncode != 0:
            raise CliError(
                f"do-harness exited {proc.returncode}",
                exit_code=proc.returncode,
                stderr=proc.stderr or "",
                result=payload,
                argv=argv,
            )
        return payload

    def _parse(self, kind: str, payload: Any, model: Any) -> Any:
        try:
            return model.from_json(payload)
        except (KeyError, TypeError, ValueError) as exc:
            raise ParseError(
                f"{kind} document is missing fields or malformed: {exc}",
                result=payload,
            ) from exc

    # -- commands ----------------------------------------------------------

    def verify(
        self,
        *,
        signal_set: Optional[str] = None,
        strict: bool = False,
        changed: bool = False,
        since: Optional[str] = None,
        fail_fast: bool = False,
        only: Optional[Iterable[str]] = None,
        exclude: Optional[Iterable[str]] = None,
        jobs: Optional[int] = None,
        extra: Optional[Sequence[str]] = None,
    ) -> VerifyResult:
        """Run ``verify --format json``.

        Raises:
            VerifyFailed: the run reported failing sensors (exit 1); the
                parsed :class:`VerifyResult` is attached as ``result``.
            CliError: exit 2 usage/config errors (e.g. all sensors unrunnable).
        """
        args = ["verify", "--format", "json"]
        if signal_set is not None:
            args += ["--set", signal_set]
        if strict:
            args.append("--strict")
        if changed:
            args.append("--changed")
        if since is not None:
            args += ["--since", since]
        if fail_fast:
            args.append("--fail-fast")
        for name in only or ():
            args += ["--only", name]
        for name in exclude or ():
            args += ["--exclude", name]
        if jobs is not None:
            args += ["--jobs", str(jobs)]
        if extra:
            args += list(extra)
        try:
            payload = self._spawn(args)
        except CliError as exc:
            if exc.exit_code == 1 and isinstance(exc.result, dict):
                raise VerifyFailed(
                    exc.args[0],
                    exit_code=exc.exit_code,
                    stderr=exc.stderr,
                    result=self._parse("verify", exc.result, VerifyResult),
                    argv=exc.argv,
                ) from None
            raise
        return self._parse("verify", payload, VerifyResult)

    def status(
        self,
        *,
        signal_set: Optional[str] = None,
        evidence: Optional[str] = None,
        since: Optional[str] = None,
        extra: Optional[Sequence[str]] = None,
    ) -> StatusResult:
        """Read ``status --format json`` evidence freshness.

        Raises:
            StatusNotGreen: ``red``/``stale``/``missing`` verdicts (exit 1)
                with the parsed :class:`StatusResult` attached as ``result``.
        """
        args = ["status", "--format", "json"]
        if signal_set is not None:
            args += ["--set", signal_set]
        if evidence is not None:
            args += ["--evidence", evidence]
        if since is not None:
            args += ["--since", since]
        if extra:
            args += list(extra)
        try:
            payload = self._spawn(args)
        except CliError as exc:
            if exc.exit_code == 1 and isinstance(exc.result, dict):
                raise StatusNotGreen(
                    exc.args[0],
                    exit_code=exc.exit_code,
                    stderr=exc.stderr,
                    result=self._parse("status", exc.result, StatusResult),
                    argv=exc.argv,
                ) from None
            raise
        return self._parse("status", payload, StatusResult)

    def explain(
        self,
        *,
        signal_set: Optional[str] = None,
        changed: bool = False,
        since: Optional[str] = None,
        extra: Optional[Sequence[str]] = None,
    ) -> ExplainResult:
        """Explain the sensor selection without executing anything."""
        args = ["explain", "--format", "json"]
        if signal_set is not None:
            args += ["--set", signal_set]
        if changed:
            args.append("--changed")
        if since is not None:
            args += ["--since", since]
        if extra:
            args += list(extra)
        return self._parse("explain", self._spawn(args), ExplainResult)

    def errors_list(
        self,
        *,
        task_id: Optional[str] = None,
        scope: Optional[str] = None,
        extra: Optional[Sequence[str]] = None,
    ) -> tuple[StrikeInfo, ...]:
        """List fail-fast error signatures (strikes) from ``errors list``."""
        args = ["errors", "list", "--format", "json"]
        if task_id is not None:
            args += ["--task", task_id]
        if scope is not None:
            args += ["--scope", scope]
        if extra:
            args += list(extra)
        payload = self._spawn(args)
        if not isinstance(payload, list):
            raise ParseError("errors list document must be an array", result=payload)
        try:
            return tuple(StrikeInfo.from_json(item) for item in payload)
        except (KeyError, TypeError, ValueError) as exc:
            raise ParseError(f"errors list item is malformed: {exc}", result=payload) from exc

    def task_list(
        self,
        *,
        status: Optional[str] = None,
        method: Optional[str] = None,
        parent: Optional[str] = None,
        extra: Optional[Sequence[str]] = None,
    ) -> TaskList:
        """Read ``task list --format json``."""
        args = ["task", "list", "--format", "json"]
        if status is not None:
            args += ["--status", status]
        if method is not None:
            args += ["--method", method]
        if parent is not None:
            args += ["--parent", parent]
        if extra:
            args += list(extra)
        return self._parse("task list", self._spawn(args), TaskList)

    def task_export(self, *, extra: Optional[Sequence[str]] = None) -> TaskExport:
        """Read the ``task export --stdout --format json`` snapshot."""
        args = ["task", "export", "--stdout", "--format", "json"]
        if extra:
            args += list(extra)
        return self._parse("task export", self._spawn(args), TaskExport)

    def version(self) -> CliVersion:
        """Read ``version --format json``."""
        return self._parse("version", self._spawn(["version", "--format", "json"]), CliVersion)

    # -- compatibility -----------------------------------------------------

    def assert_schema_compatible(self) -> CliVersion:
        """Fail closed when the installed CLI is from a different release line.

        The SDK's package version mirrors the CLI release (``0.4.x``); a CLI
        whose reported version does not share that line may emit documents this
        SDK does not know, so a mismatch raises :class:`CliError` instead of
        risking a misparse.
        """
        cli = self.version()
        sdk_major_minor = ".".join(__version__.split(".")[:2])
        cli_major_minor = ".".join(cli.version.split(".")[:2])
        if cli_major_minor != sdk_major_minor:
            raise CliError(
                f"SDK {__version__} targets do-harness {sdk_major_minor}.x; "
                f"installed CLI reports {cli.version}",
                result=cli,
            )
        return cli


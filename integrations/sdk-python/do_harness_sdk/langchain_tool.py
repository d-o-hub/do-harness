"""LangChain adapter: a ``Tool`` that runs `verify` and returns a structured
observation.

The adapter is optional: the core package imports nothing outside the
standard library. Importing this module is safe, but calling
:func:`make_langchain_verify_tool` requires ``langchain-core`` (install with
``pip install 'do-harness-sdk[langchain]'``). The tool reuses the SDK's
fail-closed client, so a red gate raises :class:`VerifyFailed` with the
parsed :class:`~do_harness_sdk.models.VerifyResult` attached, and an
unparseable CLI or missing binary raises the SDK's error classes.
"""

from __future__ import annotations

from typing import Any, Optional, Sequence

from .client import HarnessClient
from .models import VerifyResult


def verify_result_to_dict(result: VerifyResult) -> dict[str, Any]:
    """Serialize a VerifyResult to a plain dict (LangChain observation)."""
    return {
        "ok": result.ok,
        "root": result.root,
        "failed": list(result.failed),
        "unrunnable": list(result.unrunnable),
        "sensors": [
            {
                "name": s.name,
                "ok": s.ok,
                "exit_code": s.exit_code,
                "duration_ms": s.duration_ms,
                "severity": s.severity,
                "execution": s.execution,
            }
            for s in result.sensors
        ],
    }


def make_langchain_verify_tool(
    client: Optional[HarnessClient] = None,
    *,
    name: str = "do-harness-verify",
    description: Optional[str] = None,
    **verify_defaults: Any,
) -> Any:
    """Build a LangChain ``StructuredTool`` that runs ``client.verify()``.

    Args:
        client: A :class:`HarnessClient` (defaults to a fresh client that
            resolves the binary the usual way).
        name: Tool name handed to LangChain.
        description: Tool description; defaults to a standard summary.
        **verify_defaults: Keyword arguments forwarded to
            :meth:`HarnessClient.verify` (e.g. ``signal_set="verification"``,
            ``strict=True``). The tool's runtime arguments override these.

    The returned tool takes ``signal_set`` (str, optional) and ``strict``
    (bool, default False) arguments and returns the serialized
    :class:`VerifyResult` observation.
    """
    try:
        from langchain_core.tools import StructuredTool
    except ImportError as exc:  # pragma: no cover - env-dependent
        raise RuntimeError(
            "langchain-core is not installed; "
            "install it with `pip install 'do-harness-sdk[langchain]'`"
        ) from exc

    def _run(signal_set: Optional[str] = None, strict: bool = False) -> dict[str, Any]:
        kwargs: dict[str, Any] = dict(verify_defaults)
        if signal_set is not None:
            kwargs["signal_set"] = signal_set
        if strict:
            kwargs["strict"] = True
        harness = client or HarnessClient()
        return verify_result_to_dict(harness.verify(**kwargs))

    default_description = (
        "Run do-harness verify and return the sensor report as a structured "
        "JSON object (ok, root, failed, unrunnable, sensors)."
    )
    return StructuredTool.from_function(
        func=_run,
        name=name,
        description=description or default_description,
    )

"""Tests for the LangChain adapter (langchain-core stubbed out)."""

from __future__ import annotations

import sys
import types

import pytest

from do_harness_sdk import VerifyFailed
from do_harness_sdk.langchain_tool import make_langchain_verify_tool, verify_result_to_dict


class _StubStructuredTool:
    """Minimal stand-in for langchain_core.tools.StructuredTool."""

    def __init__(self, func, name, description):
        self.func = func
        self.name = name
        self.description = description
        self.last_kwargs = None

    def invoke(self, kwargs):
        self.last_kwargs = dict(kwargs)
        return self.func(**kwargs)

    @classmethod
    def from_function(cls, func, *, name, description):
        return cls(func, name, description)


@pytest.fixture
def stub_langchain(monkeypatch):
    """Inject a fake langchain_core.tools module for the factory to import."""
    module = types.ModuleType("langchain_core.tools")
    module.StructuredTool = _StubStructuredTool
    monkeypatch.setitem(sys.modules, "langchain_core.tools", module)
    return module


def test_tool_runs_verify_and_returns_observation(stub_langchain, fake_harness) -> None:
    tool = make_langchain_verify_tool(signal_set="verification")
    observation = tool.invoke({"signal_set": "verification"})
    assert observation["ok"] is True
    assert observation["root"] == "/repo"
    assert observation["failed"] == []
    assert observation["sensors"][0]["name"] == "fake-check"
    assert tool.description.startswith("Run do-harness verify")


def test_tool_passes_defaults_and_overrides(stub_langchain, fake_harness) -> None:
    tool = make_langchain_verify_tool(signal_set="verification", strict=False)
    observation = tool.invoke({"strict": True})
    assert tool.last_kwargs == {"strict": True}
    assert observation["ok"] is True


def test_tool_fail_closed_on_red_gate(stub_langchain, fake_harness, monkeypatch) -> None:
    monkeypatch.setenv("FAKE_SCENARIO", "verify-fail")
    tool = make_langchain_verify_tool()
    with pytest.raises(VerifyFailed) as excinfo:
        tool.invoke({})
    assert excinfo.value.result is not None
    assert excinfo.value.result.ok is False


def test_verify_result_to_dict_round_trip() -> None:
    from do_harness_sdk.models import SensorResult, VerifyResult

    result = VerifyResult(
        ok=False,
        root="/r",
        failed=("fails",),
        unrunnable=(),
        sensors=(
            SensorResult(
                name="fails",
                ok=False,
                exit_code=1,
                duration_ms=19,
                severity="error",
            ),
        ),
    )
    dumped = verify_result_to_dict(result)
    assert dumped["ok"] is False
    assert dumped["failed"] == ["fails"]
    assert dumped["sensors"][0]["execution"] == "ran"


def test_missing_langchain_raises(monkeypatch) -> None:
    monkeypatch.delitem(sys.modules, "langchain_core.tools", raising=False)
    with pytest.raises(RuntimeError, match="langchain-core is not installed"):
        make_langchain_verify_tool()

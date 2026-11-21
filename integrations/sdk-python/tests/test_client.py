"""HarnessClient tests: parsing, fail-closed error paths, and argv arrays."""

from __future__ import annotations

import json

import pytest

from do_harness_sdk import (
    BinaryNotFoundError,
    CliError,
    HarnessClient,
    ParseError,
    StatusNotGreen,
    VerifyFailed,
)
from do_harness_sdk.client import SCHEMA_VERSION
from do_harness_sdk.models import StatusResult, StrikeInfo, TaskList, VerifyResult

from .fixtures_json import ERRORS, EXPLAIN, STATUS_GREEN, VERIFY_FAIL, VERIFY_OK, VERSION


@pytest.fixture
def client(fake_harness) -> HarnessClient:
    return HarnessClient()


def read_spy(spy_file) -> list[str]:
    return json.loads(spy_file.read_text(encoding="utf-8"))


def test_verify_parses_ok(client: HarnessClient, spy_file) -> None:
    result = client.verify(signal_set="verification")
    assert isinstance(result, VerifyResult)
    assert result.ok is True
    assert result.root == "/repo"
    assert result.failed == ()
    assert result.unrunnable == ()
    assert len(result.sensors) == 1
    sensor = result.sensors[0]
    assert sensor.name == "fake-check"
    assert sensor.exit_code == 0
    assert sensor.severity == "error"
    assert sensor.execution == "ran"
    assert result.failed_sensors == ()
    assert read_spy(spy_file) == ["verify", "--format", "json", "--set", "verification"]


def test_verify_passes_extra_flags(client: HarnessClient, spy_file) -> None:
    client.verify(strict=True, changed=True, since="origin/main", fail_fast=True)
    assert read_spy(spy_file) == [
        "verify",
        "--format",
        "json",
        "--strict",
        "--changed",
        "--since",
        "origin/main",
        "--fail-fast",
    ]


def test_verify_failure_raises_with_report(fake_harness, monkeypatch) -> None:
    monkeypatch.setenv("FAKE_SCENARIO", "verify-fail")
    with pytest.raises(VerifyFailed) as excinfo:
        HarnessClient().verify()
    exc = excinfo.value
    assert exc.exit_code == 1
    assert isinstance(exc.result, VerifyResult)
    assert exc.result.ok is False
    assert exc.result.failed == ("fails",)
    assert [s.name for s in exc.result.failed_sensors] == ["fails", "warns"]


def test_status_parses_green(client: HarnessClient) -> None:
    result = client.status(signal_set="verification")
    assert isinstance(result, StatusResult)
    assert result.state == "green"
    assert result.green


def test_status_red_raises_with_report(fake_harness, monkeypatch) -> None:
    monkeypatch.setenv("FAKE_SCENARIO", "status-red")
    with pytest.raises(StatusNotGreen) as excinfo:
        HarnessClient().status()
    exc = excinfo.value
    assert exc.exit_code == 1
    assert isinstance(exc.result, StatusResult)
    assert exc.result.state == "red"
    assert exc.result.reason == "sensor_failed"
    assert exc.result.current is not None
    assert exc.result.current.failed == ("fails",)
    assert exc.result.recovery == "do-harness verify --set verification --record"


def test_explain_parses(client: HarnessClient) -> None:
    result = client.explain()
    assert result.selection_mode == "all"
    assert result.selected[0].name == "fake-check"
    assert "always applicable" in result.selected[0].reason


def test_errors_list_parses(client: HarnessClient) -> None:
    strikes = client.errors_list(scope="branch:main")
    assert len(strikes) == 2
    assert all(isinstance(s, StrikeInfo) for s in strikes)
    first = strikes[0]
    assert first.signature == "sensor:clippy"
    assert first.scope == "branch:main"
    assert first.attempt_count == 2
    assert first.task_id is None
    assert strikes[1].task_id == 7


def test_task_list_parses(client: HarnessClient) -> None:
    board = client.task_list()
    assert isinstance(board, TaskList)
    assert board.summary.pending == 1
    assert board.tasks[0].title == "demo task"
    assert board.tasks[0].status == "pending"


def test_task_export_parses(client: HarnessClient) -> None:
    snapshot = client.task_export()
    assert snapshot.exported_at == 1795277534
    assert snapshot.tasks[0].id == 1


def test_version_parses(client: HarnessClient) -> None:
    info = client.version()
    assert info.name == "do-harness"
    assert info.version == "0.4.8"
    assert info.dirty is False


def test_schema_compatible_matches(fake_harness) -> None:
    assert HarnessClient().assert_schema_compatible().version == "0.4.8"


def test_schema_compatible_mismatch(fake_harness, monkeypatch) -> None:
    monkeypatch.setenv("FAKE_SCENARIO", "version-mismatch")
    with pytest.raises(CliError, match="installed CLI reports 0.5.0"):
        HarnessClient().assert_schema_compatible()


def test_root_flag_passed(fake_harness, spy_file) -> None:
    HarnessClient(root="/srv/repo").status()
    assert read_spy(spy_file) == ["status", "--format", "json", "--root", "/srv/repo"]


def test_missing_binary_raises(monkeypatch) -> None:
    monkeypatch.delenv("DO_HARNESS_BIN", raising=False)
    with pytest.raises(BinaryNotFoundError):
        HarnessClient(binary="/nonexistent/do-harness").version()
    monkeypatch.setenv("PATH", "")
    with pytest.raises(BinaryNotFoundError):
        HarnessClient().version()  # neither PATH nor DO_HARNESS_BIN


def test_unparseable_output_raises(fake_harness, monkeypatch) -> None:
    monkeypatch.setenv("FAKE_SCENARIO", "garbage")
    with pytest.raises(ParseError, match="unparseable"):
        HarnessClient().verify()


def test_unparseable_output_raises_even_on_exit_zero(fake_harness, monkeypatch) -> None:
    monkeypatch.setenv("FAKE_SCENARIO", "garbage")
    with pytest.raises(ParseError):
        HarnessClient().status()


def test_empty_output_raises(fake_harness, monkeypatch) -> None:
    monkeypatch.setenv("FAKE_SCENARIO", "empty")
    with pytest.raises(ParseError, match="no output"):
        HarnessClient().verify()


def test_usage_error_exit2_raises(fake_harness, monkeypatch) -> None:
    monkeypatch.setenv("FAKE_SCENARIO", "usage-error")
    with pytest.raises(CliError) as excinfo:
        HarnessClient().explain(signal_set="nope")
    assert excinfo.value.exit_code == 2
    assert "unknown signal set" in excinfo.value.stderr


def test_exit2_with_report_raises_carrying_result(fake_harness, monkeypatch) -> None:
    monkeypatch.setenv("FAKE_SCENARIO", "exit2-with-report")
    with pytest.raises(CliError) as excinfo:
        HarnessClient().verify()
    assert excinfo.value.exit_code == 2
    assert excinfo.value.result["ok"] is False


def test_extra_args_appended(client: HarnessClient, spy_file) -> None:
    client.explain(extra=["--since", "HEAD~3"])
    assert read_spy(spy_file) == ["explain", "--format", "json", "--since", "HEAD~3"]


def test_schema_version_constant() -> None:
    assert SCHEMA_VERSION == 1

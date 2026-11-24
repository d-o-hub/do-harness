"""Model parsing tests: required-field strictness and unknown-key tolerance."""

from __future__ import annotations

import pytest

from do_harness_sdk.models import (
    CliVersion,
    EvidenceState,
    ExplainResult,
    SensorResult,
    StatusResult,
    StrikeInfo,
    TaskExport,
    TaskList,
    VerifyResult,
)

from .fixtures_json import (
    ERRORS,
    EXPLAIN,
    STATUS_GREEN,
    TASK_EXPORT,
    TASK_LIST,
    VERIFY_OK,
    VERSION,
)


def test_verify_result_parses_fixture() -> None:
    result = VerifyResult.from_json(VERIFY_OK)
    assert result.ok
    assert len(result.sensors) == 1
    assert result.sensors[0].output_bytes == 3


def test_verify_result_rejects_missing_required_field() -> None:
    with pytest.raises(KeyError):
        VerifyResult.from_json({"ok": True})


def test_sensor_result_rejects_missing_name() -> None:
    with pytest.raises(KeyError):
        SensorResult.from_json({"ok": True})


def test_sensor_result_ignores_unknown_keys() -> None:
    sensor = SensorResult.from_json(
        {"name": "x", "ok": True, "exit_code": 0, "duration_ms": 1, "future_field": 42}
    )
    assert sensor.name == "x"


def test_status_result_parses_and_state_object() -> None:
    status = StatusResult.from_json(STATUS_GREEN)
    assert status.state == "green"
    assert isinstance(status.current, EvidenceState)
    assert status.current.workspace_fingerprint.startswith("sha256:")


def test_explain_result_parses() -> None:
    explain = ExplainResult.from_json(EXPLAIN)
    assert explain.selection_mode == "all"
    assert explain.selected[0].reason


def test_strike_info_parses() -> None:
    strikes = [StrikeInfo.from_json(item) for item in ERRORS]
    assert strikes[0].attempt_count == 2
    assert strikes[0].message == "FAIL: clippy failed"
    assert strikes[1].task_id == 7


def test_task_list_and_export_parse() -> None:
    board = TaskList.from_json(TASK_LIST)
    snapshot = TaskExport.from_json(TASK_EXPORT)
    assert board.tasks[0].title == "demo task"
    assert snapshot.exported_at == 1795277534
    assert snapshot.tasks == board.tasks


def test_cli_version_parses() -> None:
    info = CliVersion.from_json(VERSION)
    assert info.version == "0.4.8"
    assert info.commit == "4f31a1a"

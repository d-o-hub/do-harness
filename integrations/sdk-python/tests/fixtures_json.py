"""Deterministic JSON documents in the exact shapes the CLI emits.

These fixtures mirror the real `do-harness` 0.4.x JSON envelopes; the SDK
tests never need a real harness binary.
"""

VERIFY_OK = {
    "ok": True,
    "root": "/repo",
    "failed": [],
    "unrunnable": [],
    "sensors": [
        {
            "details": {"reason": None, "output_bytes": 3, "output_truncated": False},
            "name": "fake-check",
            "ok": True,
            "exit_code": 0,
            "duration_ms": 17,
            "severity": "error",
            "allow_failure": False,
            "warned": False,
            "execution": "ran",
        }
    ],
}

VERIFY_FAIL = {
    "ok": False,
    "root": "/repo",
    "failed": ["fails"],
    "unrunnable": [],
    "sensors": [
        {
            "details": {"reason": None, "output_bytes": 5, "output_truncated": False},
            "name": "fails",
            "ok": False,
            "exit_code": 1,
            "duration_ms": 19,
            "severity": "error",
            "allow_failure": False,
            "warned": False,
            "execution": "ran",
        },
        {
            "details": {"reason": None, "output_bytes": 4, "output_truncated": False},
            "name": "warns",
            "ok": False,
            "exit_code": 1,
            "duration_ms": 22,
            "severity": "warn",
            "allow_failure": True,
            "warned": False,
            "execution": "ran",
        },
    ],
}

STATUS_GREEN = {
    "state": "green",
    "set": "verification",
    "reason": "",
    "current": {
        "workspace_fingerprint": "sha256:aa",
        "policy_fingerprint": "sha256:bb",
        "failed": [],
        "input_mismatches": [],
    },
    "recovery": None,
}

STATUS_RED = {
    "state": "red",
    "set": "verification",
    "reason": "sensor_failed",
    "current": {
        "workspace_fingerprint": "sha256:aa",
        "policy_fingerprint": "sha256:bb",
        "failed": ["fails"],
        "input_mismatches": [],
    },
    "recovery": "do-harness verify --set verification --record",
}

EXPLAIN = {
    "set": None,
    "selection_mode": "all",
    "changed_files": [],
    "selected": [{"name": "fake-check", "reason": "change filtering disabled; always applicable"}],
    "skipped": [],
}

ERRORS = [
    {
        "id": 1,
        "signature": "sensor:clippy",
        "scope": "branch:main",
        "task_id": None,
        "attempt_count": 2,
        "message": "FAIL: clippy failed",
        "created_at": 1795277534,
    },
    {
        "id": 2,
        "signature": "sensor:test",
        "scope": "task:7",
        "task_id": 7,
        "attempt_count": 1,
        "message": None,
        "created_at": 1795277535,
    },
]

TASK_LIST = {
    "summary": {"done": 0, "failed": 0, "in_progress": 0, "pending": 1},
    "tasks": [
        {
            "created_at": 1795277534,
            "id": 1,
            "method": None,
            "parent_id": None,
            "precondition": None,
            "status": "pending",
            "subtask_index": 0,
            "title": "demo task",
            "updated_at": 1795277534,
        }
    ],
}

TASK_EXPORT = {
    "exported_at": 1795277534,
    "tasks": TASK_LIST["tasks"],
    "summary": TASK_LIST["summary"],
}

VERSION = {
    "name": "do-harness",
    "version": "0.4.8",
    "commit": "4f31a1a",
    "commit_date": "2026-11-21",
    "dirty": False,
}

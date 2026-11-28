"""Pytest fixtures: a stub `do-harness` binary driven by FAKE_SCENARIO.

The stub is a real executable script (shebang `#!/usr/bin/env python3`) that
behaves like the CLI's JSON contract for the surfaces the SDK wraps. It
records the exact argv it received into the file named by FAKE_SPY_FILE when
set, so tests can assert argv-array invocation (no shell interpolation).
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from do_harness_sdk import __version__

from .fixtures_json import (
    ERRORS,
    EXPLAIN,
    STATUS_GREEN,
    STATUS_RED,
    TASK_EXPORT,
    TASK_LIST,
    VERIFY_FAIL,
    VERIFY_OK,
    VERSION,
)

STUB_TEMPLATE = r"""#!/usr/bin/env python3
import json
import os
import sys

scenario = os.environ.get("FAKE_SCENARIO", "ok")
spy = os.environ.get("FAKE_SPY_FILE")
if spy:
    with open(spy, "w", encoding="utf-8") as fh:
        json.dump(sys.argv[1:], fh)

if scenario == "garbage":
    print("this is not json")
    sys.exit(0)
if scenario == "empty":
    sys.exit(0)
if scenario == "usage-error":
    print("error: unknown signal set 'nope'", file=sys.stderr)
    sys.exit(2)
if scenario == "verify-fail":
    print('@VERIFY_FAIL@')
    sys.exit(1)
if scenario == "status-red":
    print('@STATUS_RED@')
    sys.exit(1)
if scenario == "exit2-with-report":
    print('@VERIFY_FAIL@')
    sys.exit(2)
if scenario == "version-current":
    bad = json.loads('@VERSION@')
    bad["version"] = @SDK_VERSION@
    print(json.dumps(bad))
    sys.exit(0)
if scenario == "version-mismatch":
    bad = json.loads('@VERSION@')
    bad["version"] = "999.0.0"
    print(json.dumps(bad))
    sys.exit(0)

cmd = sys.argv[1:]
if cmd[:1] == ["verify"]:
    print('@VERIFY_OK@')
elif cmd[:1] == ["status"]:
    print('@STATUS_GREEN@')
elif cmd[:1] == ["explain"]:
    print('@EXPLAIN@')
elif cmd[:2] == ["errors", "list"]:
    print('@ERRORS@')
elif cmd[:2] == ["task", "list"]:
    print('@TASK_LIST@')
elif cmd[:2] == ["task", "export"]:
    print('@TASK_EXPORT@')
elif cmd[:1] == ["version"]:
    print('@VERSION@')
else:
    print("error: unexpected command %r" % (cmd,), file=sys.stderr)
    sys.exit(2)
"""

_STUB_PAYLOADS = {
    "@VERIFY_OK@": VERIFY_OK,
    "@VERIFY_FAIL@": VERIFY_FAIL,
    "@STATUS_GREEN@": STATUS_GREEN,
    "@STATUS_RED@": STATUS_RED,
    "@EXPLAIN@": EXPLAIN,
    "@ERRORS@": ERRORS,
    "@TASK_LIST@": TASK_LIST,
    "@TASK_EXPORT@": TASK_EXPORT,
    "@VERSION@": VERSION,
    "@SDK_VERSION@": __version__,
}


def _stub_script() -> str:
    script = STUB_TEMPLATE
    for token, payload in _STUB_PAYLOADS.items():
        script = script.replace(token, json.dumps(payload))
    return script


@pytest.fixture
def fake_harness(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """Write the stub binary and point DO_HARNESS_BIN at it."""
    stub = tmp_path / "fake-do-harness"
    stub.write_text(_stub_script(), encoding="utf-8")
    stub.chmod(0o755)
    monkeypatch.setenv("DO_HARNESS_BIN", str(stub))
    return stub


@pytest.fixture
def spy_file(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    path = tmp_path / "argv.json"
    monkeypatch.setenv("FAKE_SPY_FILE", str(path))
    return path

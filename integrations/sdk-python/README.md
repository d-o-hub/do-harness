# do-harness-sdk (Python)

Typed, standard-library-only Python bindings for the
[do-harness](../../README.md) agent harness CLI. The SDK spawns the
`do-harness` binary (never a shell), asks for JSON output, and parses it
into typed dataclasses.

- **Fail closed.** A missing binary raises `BinaryNotFoundError`; output that
  is not the expected JSON document raises `ParseError`; a non-zero exit
  raises `CliError` (or the `VerifyFailed` / `StatusNotGreen` subclasses,
  which carry the parsed report in `.result`). The SDK never returns a
  partial or silently-green result.
- **No runtime dependencies.** Just the standard library. Optional
  `langchain` extra only for the framework adapter.
- **Version coupling.** The SDK package version mirrors the CLI release it
  targets (`0.4.8` ↔ CLI `0.4.x`). The SDK parses the CLI's documented JSON
  envelope contract (`SCHEMA_VERSION = 1`); `HarnessClient.assert_schema_compatible()`
  fails closed when the installed CLI is from a different release line.

## Installation

<!-- example: id=sdk_python_install classification=illustrative reason="Python SDK install commands" -->
```sh
pip install do-harness-sdk
# LangChain adapter (optional):
pip install 'do-harness-sdk[langchain]'
```

The harness binary is resolved from the `binary=` argument, then
`$DO_HARNESS_BIN`, then `do-harness` on `PATH`.

## Usage

<!-- example: id=sdk_python_usage classification=illustrative reason="Python SDK usage snippet" -->
```python
from do_harness_sdk import HarnessClient, VerifyFailed

client = HarnessClient()  # or HarnessClient(root="/path/to/repo")

# The gate: returns a VerifyResult; a red run raises VerifyFailed carrying
# the parsed result so you can inspect exactly which sensors failed.
try:
    result = client.verify(signal_set="verification", strict=True)
    print(result.ok, result.failed)
except VerifyFailed as exc:
    report = exc.result
    for sensor in report.failed_sensors:
        print(sensor.name, sensor.exit_code)

# Evidence freshness (status).
status = client.status(signal_set="verification")  # green -> StatusResult
# red/stale/missing raise StatusNotGreen; use .result.state/.result.reason

# Sensor selection without executing anything.
explained = client.explain(signal_set="verification")

# Strike state: fail-fast error signatures (`errors list`).
strikes = client.errors_list(scope="branch:main")
print(strikes[0].signature, strikes[0].attempt_count)

# Task board state (`task list` / `task export`).
board = client.task_list(status="pending")
snapshot = client.task_export()

# CLI version compatibility.
cli = client.assert_schema_compatible()
print(cli.version)
```

### LangChain adapter

`make_langchain_verify_tool` builds a LangChain `StructuredTool` that runs
`verify` and returns the structured sensor observation:

<!-- example: id=sdk_python_langchain classification=illustrative reason="LangChain tool adapter usage" -->
```python
from do_harness_sdk import HarnessClient
from do_harness_sdk.langchain_tool import make_langchain_verify_tool

tool = make_langchain_verify_tool(HarnessClient(), signal_set="verification")
observation = tool.invoke({"signal_set": "verification"})
# observation -> {"ok": bool, "root": str, "failed": [...], "sensors": [...]}
```

The tool keeps the SDK's fail-closed semantics: a red gate raises
`VerifyFailed` with the report attached, so the orchestrator observes the
failure instead of a false pass.

## Publishing

The package version mirrors the CLI release it targets, and the offline gate
`bash scripts/check-sdk-pack.sh` (from the repository root) builds the sdist
and wheel before any publish. The publish itself needs PyPI credentials and
runs outside the repository:

<!-- example: id=sdk_python_publish classification=illustrative reason="Publishing steps for the Python SDK" -->
```sh
python3 -m build
python3 -m twine upload dist/*
```

## Development

<!-- example: id=sdk_python_dev classification=illustrative reason="Python SDK test invocation" -->
```sh
python3 -m pytest tests/
```

Tests use a stub binary and fixture JSON documents; no real harness is needed.

"""do-harness-sdk: typed Python bindings for the do-harness CLI.

Thin, standard-library-only wrappers that spawn the ``do-harness`` binary
and parse its JSON reports into typed dataclasses. The SDK fails closed: a
missing binary, unparseable output, or a non-zero exit raises a
:class:`~do_harness_sdk.client.HarnessError` subclass.
"""

from ._version import __version__
from .client import (
    HarnessClient,
    HarnessError,
    BinaryNotFoundError,
    ParseError,
    CliError,
    VerifyFailed,
    StatusNotGreen,
)
from .models import (
    SCHEMA_VERSION,
    CliVersion,
    EvidenceState,
    ExplainResult,
    SelectedSensor,
    SensorResult,
    StatusResult,
    StrikeInfo,
    TaskExport,
    TaskList,
    TaskRecord,
    TaskSummary,
    VerifyResult,
)

__all__ = [
    "__version__",
    "SCHEMA_VERSION",
    "HarnessClient",
    "HarnessError",
    "BinaryNotFoundError",
    "ParseError",
    "CliError",
    "VerifyFailed",
    "StatusNotGreen",
    "CliVersion",
    "EvidenceState",
    "ExplainResult",
    "SelectedSensor",
    "SensorResult",
    "StatusResult",
    "StrikeInfo",
    "TaskExport",
    "TaskList",
    "TaskRecord",
    "TaskSummary",
    "VerifyResult",
]

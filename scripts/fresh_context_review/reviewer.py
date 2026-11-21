"""Fresh reviewer launch, outcome classification, and sanitized recording.

The reviewer is a separate process that receives exactly one packet JSON
object on stdin and returns a findings JSON object on stdout. The process
inherits a scrubbed environment (no session/agent variables) and a fresh,
empty conversation: the adapter passes nothing but the packet. Native launch
records (pid, command, spawn time) are recorded as process-isolation evidence.

Outcomes are distinct and exhaustive:

- ``success_with_findings``  reviewer completed, >=1 validated findings
- ``success_zero_findings``   reviewer completed, zero findings
- ``failure``                 launch/parse/reviewer error, or marker leak
- ``timeout``                 reviewer exceeded the predeclared deadline
- ``not_run``                 capability unavailable (no reviewer configured,
                              or reviewer declares itself unavailable)
- ``unverified``              review completed but isolation evidence is
                              incomplete (e.g. external reviewer without a
                              parent-only marker probe)

An unavailable or unverified capability is never a success.
"""

import hashlib
import json
import os
import subprocess
import time

# Environment keys a fresh reviewer may inherit. Everything else — session,
# agent, credential, and tool state — is excluded so the child cannot read
# parent conversation or connection context from the environment.
_CLEAN_ENV_ALLOW = {
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "TEMP",
    "TMP",
    "SYSTEMROOT",
    "SystemRoot",
}

OUTCOME_SUCCESS_FINDINGS = "success_with_findings"
OUTCOME_ZERO_FINDINGS = "success_zero_findings"
OUTCOME_FAILURE = "failure"
OUTCOME_TIMEOUT = "timeout"
OUTCOME_NOT_RUN = "not_run"
OUTCOME_UNVERIFIED = "unverified"

OUTCOMES = (
    OUTCOME_SUCCESS_FINDINGS,
    OUTCOME_ZERO_FINDINGS,
    OUTCOME_FAILURE,
    OUTCOME_TIMEOUT,
    OUTCOME_NOT_RUN,
    OUTCOME_UNVERIFIED,
)

UNAVAILABLE_REASON = "capability unavailable"


def clean_env(environ=None):
    env = environ if environ is not None else os.environ
    return {k: v for k, v in env.items() if k in _CLEAN_ENV_ALLOW and v != ""}


def validate_findings(raw, max_findings):
    """Validate and cap a reviewer's findings list.

    Returns (findings, truncated). Invalid records are a failure, not a
    silent drop.
    """
    if not isinstance(raw, list):
        raise ValueError("reviewer findings must be a JSON array")
    required = ("contract_or_path", "trigger", "consequence", "evidence",
                "smallest_correction")
    findings = []
    for i, item in enumerate(raw):
        if not isinstance(item, dict):
            raise ValueError("finding %d is not an object" % i)
        missing = [k for k in required if not isinstance(item.get(k), str) or not item[k].strip()]
        if missing:
            raise ValueError("finding %d missing fields: %s" % (i, ", ".join(missing)))
    truncated = len(findings) > max_findings
    return findings[:max_findings], truncated


def probe_marker(output, marker):
    """Parent-only marker probe over captured reviewer output.

    Returns True when the marker appears (isolation violated), False when a
    marker was configured and did not appear, None when no marker was
    configured (probe not run).
    """
    if not marker:
        return None
    return marker in output


def run_reviewer(cmd, packet_json, *, repo, timeout_s, max_output_bytes,
                 max_findings, marker):
    """Launch the fresh reviewer, capture sanitized evidence, classify outcome.

    Returns a record dict with only fixed reason codes and structured
    findings; raw reviewer stdout/stderr never enter the record.
    """
    spawn_time = time.time()
    started = time.monotonic()
    try:
        proc = subprocess.run(
            cmd,
            input=packet_json,
            capture_output=True,
            text=True,
            timeout=timeout_s,
            env=clean_env(),
            cwd=repo,
        )
        timed_out = False
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        proc = None
        partial = (exc.stdout or "")[:max_output_bytes]
        partial_err = (exc.stderr or "")[:max_output_bytes]
    elapsed = time.monotonic() - started

    record = {
        "reviewer_cmd_hash": sha256_hex(json.dumps(cmd, sort_keys=True)),
        "launch_evidence": {
            "fresh_process": True,
            "spawn_time_unix": int(spawn_time),
            "environment_scrubbed": True,
            "environment_keys_inherited": sorted(clean_env().keys()),
        },
        "elapsed_seconds": round(elapsed, 3),
        "limits": {
            "timeout_s": timeout_s,
            "max_output_bytes": max_output_bytes,
            "max_findings": max_findings,
        },
    }


    if timed_out:
        record["outcome"] = OUTCOME_TIMEOUT
        record["reason"] = "reviewer exceeded the predeclared timeout"
        record["marker_leaked"] = probe_marker(partial + partial_err, marker)
        return record

    output = (proc.stdout or "")[:max_output_bytes]
    err = (proc.stderr or "")[:max_output_bytes]
    record["reviewer_exit_code"] = proc.returncode
    record["marker_leaked"] = probe_marker(output + err, marker)

    if proc.returncode == 9 and UNAVAILABLE_REASON in err.lower():
        record["outcome"] = OUTCOME_NOT_RUN
        record["reason"] = UNAVAILABLE_REASON + ": reviewer declared no capability"
        return record
    if proc.returncode != 0:
        record["outcome"] = OUTCOME_FAILURE
        record["reason"] = "reviewer exited nonzero"
        record["error_kind"] = "reviewer_exit"
        return record

    try:
        payload = json.loads(output)
    except ValueError:
        record["outcome"] = OUTCOME_FAILURE
        record["reason"] = "reviewer stdout is not valid JSON"
        record["error_kind"] = "unparsable_output"
        return record

    try:
        findings, truncated = validate_findings(
            payload.get("findings", []), max_findings
        )
    except ValueError as exc:
        record["outcome"] = OUTCOME_FAILURE
        record["reason"] = str(exc)
        record["error_kind"] = "invalid_findings"
        return record

    record["findings"] = findings
    record["findings_count"] = len(findings)
    record["findings_truncated"] = truncated
    record["outcome"] = (
        OUTCOME_SUCCESS_FINDINGS if findings else OUTCOME_ZERO_FINDINGS
    )
    return record


def sha256_hex(raw):
    return "sha256:" + hashlib.sha256(raw.encode("utf-8")).hexdigest()

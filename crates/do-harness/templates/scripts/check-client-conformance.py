#!/usr/bin/env python3
"""Opt-in, synthetic client-boundary conformance protocol v1 (stdlib only).

An integration adapter receives one JSON request on stdin and returns one
JSON observation from its instrumented outbound boundary on stdout. Neither
stream is logged. The adapter is trusted test code, not a sanitizer or a
sandbox. No adapter runs unless explicitly supplied; this driver uses no network.
"""

import argparse
import datetime
import json
import re
import secrets
import subprocess
import tempfile

VERSION = 1
SCENARIOS = ("prompt", "tool-arguments", "file-excerpt", "outbound-request")
MAX_OUTPUT = 65536
TIMEOUT_SECONDS = 10
LIMITATIONS = [
    "Only the named client version, adapter, boundary and synthetic scenarios were tested.",
    "Adapter instrumentation is trusted; uninstrumented routes remain unverified.",
    "No production prompts, secrets, vault mappings or arbitrary tool payloads were captured.",
]


def identifier(value):
    """Keep metadata bounded and prevent arbitrary adapter text in artifacts."""
    if not re.fullmatch(r"[A-Za-z0-9_.:/-]{1,100}", value):
        raise argparse.ArgumentTypeError("expected a bounded identifier")
    return value


def observation(command, scenario):
    """Return only a fixed verdict/reason; never return the raw child streams."""
    marker = "private_" + secrets.token_hex(24)
    request = {
        "protocol_version": VERSION,
        "scenario_id": scenario,
        "expected_policy": "transform-or-refuse",
        "private_markers": [marker],
        "payload": {scenario: "synthetic context " + marker},
    }
    # Files bound parent memory even if a broken adapter emits excessive output.
    # The explicit sensor timeout/output policy provides the outer resource bound.
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        try:
            result = subprocess.run(  # nosec B603 - explicit trusted adapter argv
                command, input=json.dumps(request).encode(), stdout=out, stderr=err,
                timeout=TIMEOUT_SECONDS, check=False,
            )
            if result.returncode != 0:
                return "fail", "adapter-failed"
            if out.tell() > MAX_OUTPUT or err.tell() > MAX_OUTPUT:
                return "fail", "oversized-observation"
            out.seek(0)
            raw = out.read(MAX_OUTPUT + 1)
            err.seek(0)
            # A leak on either observation channel fails, even if JSON is invalid.
            if marker.encode() in raw or marker.encode() in err.read(MAX_OUTPUT + 1):
                return "fail", "raw-marker-leakage"
            data = json.loads(raw)
        except subprocess.TimeoutExpired:
            return "fail", "adapter-timeout"
        except (OSError, ValueError, TypeError):
            return "fail", "malformed-observation"
    fields = {"protocol_version", "supported", "instrumented", "refused", "outbound"}
    if not isinstance(data, dict) or set(data) != fields:
        return "fail", "malformed-observation"
    if (type(data["protocol_version"]) is not int or data["protocol_version"] != VERSION
            or any(type(data[k]) is not bool for k in ("supported", "instrumented", "refused"))
            or not isinstance(data["outbound"], str)):
        return "fail", "malformed-observation"
    if not data["supported"]:
        return "unknown", "unsupported-client"
    if not data["instrumented"]:
        return "unknown", "missing-instrumentation"
    if data["refused"]:
        return ("pass", "refused") if not data["outbound"] else ("fail", "refusal-sent-data")
    if not data["outbound"]:
        return "unknown", "no-boundary-observation"
    return "pass", "transformed"


def run(args):
    scenarios = []
    for scenario in SCENARIOS:
        outcome, reason = observation(args.adapter, scenario) if args.adapter else (
            "unknown", "untested-client")
        scenarios.append({"id": scenario, "expected_policy": "transform-or-refuse",
                          "observed_outcome": outcome, "reason": reason})
    outcomes = {s["observed_outcome"] for s in scenarios}
    verdict = "fail" if "fail" in outcomes else "unknown" if "unknown" in outcomes else "pass"
    return {
        "schema_version": VERSION, "claim": "client-boundary",
        "client": {"id": args.client, "version": args.client_version},
        "integration_method": args.integration, "test_suite_version": "1",
        "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "claims": {"guidance_installed": "unknown", "sanitizer_fixture": "unknown",
                   "client_boundary": verdict},
        "scenarios": scenarios, "verdict": verdict, "limitations": LIMITATIONS,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--client", type=identifier, default="unspecified")
    parser.add_argument("--client-version", type=identifier, default="unknown")
    parser.add_argument("--integration", type=identifier, default="uninstrumented")
    parser.add_argument("--required", action="store_true",
                        help="Fail closed on unknown/missing client integration")
    parser.add_argument("--adapter", nargs=argparse.REMAINDER,
                        help="Explicit trusted adapter argv (last option); never shell text")
    args = parser.parse_args()
    report = run(args)
    print(json.dumps(report, sort_keys=True))
    if report["verdict"] == "unknown":
        print("SKIP: client-conformance: boundary not verified")
    return int(report["verdict"] == "fail" or (args.required and report["verdict"] != "pass"))


if __name__ == "__main__":
    raise SystemExit(main())

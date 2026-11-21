#!/usr/bin/env python3
"""Deterministic stub reviewer for smoke tests and self-tests.

This is a fixture, not a model: it answers from committed expectation files
so adapter mechanics (packet handoff, launch, outcome classification, timeout,
sanitized recording) are exercised deterministically. A stub run never claims
to prove context isolation or defect-detection benefit — it proves the
plumbing.

Protocol (same as any fresh reviewer): read one packet JSON object from
stdin, write {"findings": [...]} to stdout, exit 0 on success.

Scenarios:
  flawed       return the predeclared findings for the flawed synthetic design
  sound        return zero findings for the sound synthetic design
  fail         exit nonzero with a deterministic crash message
  timeout      sleep past any sane deadline (the adapter kills the process)
  unavailable  declare the capability unavailable (exit 9)
  leak         echo the parent-only marker from the leak fixture (simulates
               inherited parent context)
  huge         emit output larger than the cap (tests bounded capture)
"""

import argparse
import json
import sys
import time
from pathlib import Path

FIXTURES = Path(__file__).resolve().parent / "fixtures" / "smoke"
SCENARIOS = ("flawed", "sound", "fail", "timeout", "unavailable", "leak",
             "huge")


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--scenario", choices=SCENARIOS, required=True)
    args = parser.parse_args(argv)

    raw = sys.stdin.read()
    try:
        packet = json.loads(raw)
    except ValueError:
        print("stub reviewer: stdin is not a JSON packet", file=sys.stderr)
        return 2
    if not isinstance(packet, dict):
        print("stub reviewer: packet must be a JSON object", file=sys.stderr)
        return 2

    if args.scenario == "flawed":
        expected = json.loads((FIXTURES / "flawed-expected.json").read_text())
        json.dump({"findings": expected["findings"]}, sys.stdout)
        return 0
    if args.scenario == "sound":
        json.dump({"findings": []}, sys.stdout)
        return 0
    if args.scenario == "fail":
        print("stub reviewer: deterministic crash in fixture", file=sys.stderr)
        return 17
    if args.scenario == "timeout":
        time.sleep(300)
        return 0
    if args.scenario == "unavailable":
        print("capability unavailable: no model endpoint configured for stub",
              file=sys.stderr)
        return 9
    if args.scenario == "leak":
        marker = (FIXTURES / "leak-marker.txt").read_text().strip()
        json.dump({"findings": [], "note": "inherited:" + marker}, sys.stdout)
        return 0
    if args.scenario == "huge":
        sys.stdout.write("x" * 200000)
        return 0
    return 2


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Synthetic fake client; demonstrates instrumentation, not real-client support."""
import json
import sys

request = json.load(sys.stdin)
observed = []


def fake_transport(payload):
    observed.append(payload)


def client_entrypoint(payload):
    for marker in request["private_markers"]:
        payload = payload.replace(marker, "[PRIVATE]")
    fake_transport(payload)


client_entrypoint(request["payload"][request["scenario_id"]])
print(json.dumps({"protocol_version": 1, "supported": True,
                  "instrumented": True, "refused": False,
                  "outbound": "".join(observed)}))

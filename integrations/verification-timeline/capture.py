#!/usr/bin/env python3
"""Capture one do-harness attempt with an explicit agent session binding."""

from argparse import ArgumentParser, ArgumentTypeError
from hashlib import sha256
import json
import os
from pathlib import Path
import re
import subprocess
from uuid import uuid4


def opaque_id(value: str) -> str:
    if not re.fullmatch(r"[a-zA-Z0-9._:-]{1,128}", value):
        raise ArgumentTypeError("expected 1–128 letters, digits, dots, underscores, hyphens, or colons")
    return value


def main() -> int:
    parser = ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--client", type=opaque_id, required=True)
    parser.add_argument("--session", type=opaque_id, required=True)
    parser.add_argument("--set", required=True, dest="signal_set")
    parser.add_argument("--task", type=int)
    args = parser.parse_args()

    root = args.root.resolve(strict=True)
    binary = os.environ.get("DO_HARNESS_BIN") or "do-harness"
    attempt_id = uuid4().hex
    directory = root / ".do-harness" / "timeline" / attempt_id
    if not directory.resolve().is_relative_to(root):
        parser.error("capture directory must be in the workspace")
    directory.mkdir(parents=True)
    evidence = directory / "evidence.json"
    common = [binary, "--root", str(root)]
    policy = ["--set", args.signal_set, "--evidence", str(evidence)]
    task = ["--record", "--task", str(args.task)] if args.task is not None else []
    verify = subprocess.run([*common, "verify", *policy, *task], check=False)
    if verify.returncode not in (0, 1) or not evidence.is_file():
        parser.exit(2, "verification did not write an evidence artifact\n")

    data = evidence.read_bytes()
    digest = "sha256:" + sha256(data).hexdigest()
    document = json.loads(data)
    if args.task is not None and document.get("task_id") != args.task:
        parser.exit(2, "verification evidence has the wrong task\n")
    status = subprocess.run(
        [*common, "status", *policy, "--format", "json"], stdout=subprocess.PIPE, check=False
    )
    if status.returncode not in (0, 1) or evidence.read_bytes() != data:
        parser.exit(2, "could not capture a stable observation of the evidence\n")
    snapshot = json.loads(status.stdout)
    if (snapshot.get("evidence") or {}).get("artifact", {}).get("sha256") != digest:
        parser.exit(2, "status did not identify the captured evidence\n")

    (directory / "status.json").write_bytes(status.stdout)
    binding = {
        "schema_version": 1,
        "client": args.client,
        "session_id": args.session,
        "attempt_id": attempt_id,
        "evidence_sha256": digest,
        "workspace": document["root"],
        "task_id": document.get("task_id"),
    }
    evidence.with_suffix(".json.session.json").write_text(json.dumps(binding) + "\n", encoding="utf-8")
    print(evidence)
    return verify.returncode


if __name__ == "__main__":
    raise SystemExit(main())

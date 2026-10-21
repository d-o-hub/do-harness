#!/usr/bin/env python3
"""Opt-in fixture check for a local context sanitizer.

The harness does not mediate context: it does not capture prompts, tool
arguments, or vault contents. This script proves one narrow thing — that a
configured sanitizer transforms a synthetic fixture — and nothing more. The
evidence it produces is capability evidence about the fixture, not proof
that a real coding client routes every outbound byte through the sanitizer.

The sensor is off unless `DO_HARNESS_CONTEXT_SANITIZATION=enabled`. An unset
mode means `disabled`, and an installed sanitizer binary never enables it
implicitly.

Configuration:

* `DO_HARNESS_CONTEXT_SANITIZATION` — `disabled` (default) or `enabled`.
* `DO_CONTEXT_SHIELD_BIN` — path to (or name of) the sanitizer executable.
  Required only when the mode is `enabled`.

Diagnostics are fixed strings: no path, environment value, payload, or child
output is ever interpolated into them.
"""

from __future__ import annotations

import os
import re
import secrets
import subprocess  # nosec B404 - the only child is the configured sanitizer
import sys
import tempfile
from pathlib import Path

MODE_ENV = "DO_HARNESS_CONTEXT_SANITIZATION"
BIN_ENV = "DO_CONTEXT_SHIELD_BIN"

DISABLED = "disabled"
ENABLED = "enabled"

# The child is given a bounded budget: a sanitizer that hangs must not hang
# the verification run.
CHILD_TIMEOUT_SECONDS = 30

# The shape the sanitized fixture must have. The email becomes a minted
# placeholder with a per-mapping counter and entropy suffix, and the AWS key
# is redacted outright.
SANITIZED_LINE = re.compile(
    r"^Contact email: __DO_PRIVATE_EMAIL_[0-9]+_[A-Fa-f0-9]{16}__ "
    r"with AWS key __DO_PRIVATE_REDACTED__$"
)

# The raw fixture values, which must not survive in either output stream.
RAW_EMAIL_MARKER = "synthetic_"

# The synthetic AWS access key the fixture carries. It is assembled at runtime
# from parts, because a credential-shaped literal in the source is itself a
# finding for every secret scanner — and this value is not a credential, it is
# the thing the check proves the sanitizer redacts. The shape is the one the
# built-in detector recognizes: a four-character prefix followed by sixteen
# uppercase alphanumerics.
RAW_KEY = "AKIA" + "IOSFODNN7EXAMPLE"


def fail(reason: str) -> int:
    """Print a fixed failure diagnostic and return the failure exit code."""
    print(f"FAIL: context-sanitization: {reason}")
    return 1


def resolve_executable() -> str | None:
    """Resolves the configured executable, or `None` when it is not usable.

    The value may be a path or a command on `PATH`. The check is that the
    file exists and is executable: a configured-but-unusable binary must fail
    closed, not be silently skipped.
    """
    configured = os.environ.get(BIN_ENV, "").strip()
    if not configured:
        return None
    candidate = Path(configured)
    if candidate.is_absolute() or os.sep in configured:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)
        return None
    from shutil import which

    resolved = which(configured)
    return resolved


def build_payload(nonce: str) -> str:
    """Builds the synthetic payload for one run."""
    return (
        f"do-harness-canary-begin:{nonce}\n"
        f"Contact email: synthetic_{nonce}@example.com with AWS key {RAW_KEY}\n"
        f"do-harness-canary-end:{nonce}\n"
    )


def run_child(executable: str, scratch: Path, payload: str) -> tuple[int, bytes, bytes]:
    """Runs the sanitizer on the payload in an isolated scratch directory.

    The child gets an empty explicit configuration, a cleared environment for
    the sanitizer's own variables, and a private working directory, so no
    ambient plugin, audit path, or vault file can be opened. The memory vault
    is discarded with the process.
    """
    config = scratch / "config.toml"
    config.write_text("", encoding="utf-8")
    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith("DO_CONTEXT_SHIELD_")
    }
    argv = [
        executable,
        "--config",
        str(config),
        "sanitize",
        "--session",
        "do-harness-synthetic",
        "--vault",
        "memory",
        "--detector",
        "regex",
        "--policy",
        "default",
        "--transformer",
        "pseudonymize",
        "--recipient",
        "external",
        "--data-category",
        "personal",
    ]
    # The executable comes from the operator's own configuration
    # (`DO_CONTEXT_SHIELD_BIN`), and it is the program under test: the check
    # exists to prove what that program does with the fixture. The argv is an
    # array and the shell is never involved, so nothing in it is interpreted
    # as a command.
    completed = subprocess.run(  # nosec B603 B607 - argv array, no shell
        argv,
        input=payload.encode("utf-8"),
        capture_output=True,
        cwd=str(scratch),
        env=environment,
        timeout=CHILD_TIMEOUT_SECONDS,
        check=False,
    )
    return completed.returncode, completed.stdout, completed.stderr


def main() -> int:
    mode = os.environ.get(MODE_ENV, DISABLED).strip()
    if mode not in (DISABLED, ENABLED):
        return fail("invalid mode")
    if mode == DISABLED:
        print("SKIP: context-sanitization: not configured")
        return 0

    executable = resolve_executable()
    if executable is None:
        return fail("missing executable")

    nonce = secrets.token_hex(8)
    payload = build_payload(nonce)
    try:
        with tempfile.TemporaryDirectory() as scratch_name:
            scratch = Path(scratch_name)
            try:
                code, stdout, stderr = run_child(executable, scratch, payload)
            except subprocess.TimeoutExpired:
                return fail("sanitizer timeout")
            except OSError:
                return fail("sanitizer invocation failed")
    except OSError:
        return fail("sanitizer invocation failed")

    if code != 0:
        return fail("sanitizer invocation failed")

    try:
        text = stdout.decode("utf-8")
    except UnicodeDecodeError:
        return fail("malformed sanitized output")

    for stream in (text, stderr.decode("utf-8", errors="replace")):
        if RAW_KEY in stream or RAW_EMAIL_MARKER in stream:
            return fail("raw sentinel leakage")

    lines = text.splitlines()
    if len(lines) != 3:
        return fail("malformed sanitized output")
    if lines[0] != f"do-harness-canary-begin:{nonce}":
        return fail("malformed sanitized output")
    if lines[2] != f"do-harness-canary-end:{nonce}":
        return fail("malformed sanitized output")
    if not SANITIZED_LINE.match(lines[1]):
        return fail("malformed sanitized output")

    print("PASS: context-sanitization: synthetic fixture sanitized")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""The release preflight must install every tool the verification workflow does.

`release.yml` runs the full `--set release` sensor pack on the tagged commit.
Its install step therefore has to cover at least everything `verify.yml`
installs, because the release set is a superset of the verification set: a
requirement added only to `verify.yml` passes every pull request and then fails
the tag preflight (`sdk-python-test` failed v0.5.0 with `No module named
pytest` for exactly that reason).

The comparison is on the *requirements* a step installs — pip requirement
files, editable specs, and bare distributions, plus the globally installed npm
linters — not on the flags around them, so rewording a command does not fail
the gate but dropping a requirement does.

The self-test controls prove the comparison can fail: a synthesized pair where
the release step is missing one requirement is rejected, and the same pair with
the requirement restored passes.
"""

import pathlib
import unittest

import yaml

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
VERIFY = REPO_ROOT / ".github/workflows/verify.yml"
RELEASE = REPO_ROOT / ".github/workflows/release.yml"

# pip flags that take no value and install nothing.
_PIP_FLAGS = {
    "--disable-pip-version-check",
    "--no-cache-dir",
    "--quiet",
    "-q",
    "--upgrade",
    "-U",
    "--user",
    "--break-system-packages",
}


def _run_blocks(workflow: dict) -> list[str]:
    blocks = []
    for job in (workflow.get("jobs") or {}).values():
        for step in job.get("steps") or []:
            run = step.get("run")
            if isinstance(run, str):
                blocks.append(run)
    return blocks


def _pip_requirements(blocks: list[str]) -> set[str]:
    """Every pip requirement named by a `python -m pip install` invocation."""
    found: set[str] = set()
    for block in blocks:
        for line in block.splitlines():
            words = line.split()
            if "pip" not in words or "install" not in words:
                continue
            index = words.index("install")
            tokens = words[index + 1 :]
            position = 0
            while position < len(tokens):
                token = tokens[position].strip("'\"")
                if token in _PIP_FLAGS:
                    position += 1
                    continue
                if token in {"-r", "--requirement", "-e", "--editable", "-c", "--constraint"}:
                    position += 1
                    if position >= len(tokens):
                        break
                    value = tokens[position].strip("'\"")
                    prefix = {
                        "-r": "req",
                        "--requirement": "req",
                        "-e": "editable",
                        "--editable": "editable",
                        "-c": "constraint",
                        "--constraint": "constraint",
                    }[token]
                    found.add(f"{prefix}:{value}")
                    position += 1
                    continue
                if token.startswith("-"):
                    # An unknown value-taking flag: skip it and its value.
                    position += 2
                    continue
                found.add(f"dist:{token}")
                position += 1
    return found


def _npm_globals(blocks: list[str]) -> set[str]:
    found: set[str] = set()
    for block in blocks:
        for line in block.splitlines():
            words = line.split()
            if "npm" in words and "install" in words and ("-g" in words or "--global" in words):
                for token in words[words.index("install") + 1 :]:
                    if not token.startswith("-"):
                        found.add(token.strip("'\""))
    return found


def compare(verify_workflow: dict, release_workflow: dict) -> list[str]:
    """Requirements `release_workflow` installs but `verify_workflow` needs."""
    verify_blocks = _run_blocks(verify_workflow)
    release_blocks = _run_blocks(release_workflow)
    missing = sorted(_pip_requirements(verify_blocks) - _pip_requirements(release_blocks))
    missing += sorted(_npm_globals(verify_blocks) - _npm_globals(release_blocks))
    return missing


def _workflow(install_line: str) -> dict:
    return {
        "jobs": {
            "gate": {
                "steps": [
                    {"name": "install", "run": install_line},
                    {"name": "sensors", "run": "do-harness verify --set release"},
                ]
            }
        }
    }


class ReleaseToolingParity(unittest.TestCase):
    def test_release_preflight_installs_every_verification_requirement(self):
        verify = yaml.safe_load(VERIFY.read_text(encoding="utf-8"))
        release = yaml.safe_load(RELEASE.read_text(encoding="utf-8"))
        missing = compare(verify, release)
        self.assertEqual(
            missing,
            [],
            "release.yml does not install requirements that verify.yml installs, "
            "so the tag preflight fails a sensor the pull request gate passed: "
            + ", ".join(missing),
        )

    def test_control_detects_a_missing_requirement(self):
        verify = _workflow("python3 -m pip install yamllint -r scripts/requirements-docs.txt")
        release = _workflow("python3 -m pip install yamllint")
        self.assertEqual(compare(verify, release), ["req:scripts/requirements-docs.txt"])

    def test_control_detects_a_missing_editable_extra(self):
        verify = _workflow("python3 -m pip install -e 'integrations/sdk-python[test]'")
        release = _workflow("python3 -m pip install yamllint")
        self.assertEqual(compare(verify, release), ["editable:integrations/sdk-python[test]"])

    def test_control_detects_a_missing_npm_global(self):
        verify = _workflow("npm install -g markdownlint-cli2")
        release = _workflow("npm install -g yamllint")
        self.assertEqual(compare(verify, release), ["markdownlint-cli2"])

    def test_control_passes_when_the_requirement_is_restored(self):
        verify = _workflow("python3 -m pip install yamllint -r scripts/requirements-docs.txt")
        release = _workflow(
            "python3 -m pip install --disable-pip-version-check -r scripts/requirements-docs.txt yamllint"
        )
        self.assertEqual(compare(verify, release), [])


if __name__ == "__main__":
    unittest.main()

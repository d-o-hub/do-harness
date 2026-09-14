#!/usr/bin/env python3
"""
Quick validation script for skills: Tier-1 structure + safety gate.

Checks YAML frontmatter shape, license presence, description quality,
body size (progressive disclosure), secret patterns, and shell syntax
(bash -n always; shellcheck -S error when installed, SKIP otherwise).
"""

import re
import shutil
import subprocess
import sys
from pathlib import Path

import yaml

MAX_SKILL_NAME_LENGTH = 64
MAX_DESCRIPTION_LENGTH = 1024
MIN_DESCRIPTION_LENGTH = 40
MAX_BODY_WORDS = 5000
MAX_BODY_LINES = 500
ALLOWED_LICENSES = {"MIT", "Apache-2.0"}

SECRET_PATTERNS = (
    (r"AKIA[0-9A-Z]{16}", "possible AWS access key"),
    (r"ghp_[A-Za-z0-9]{20,}", "possible GitHub token"),
    (r"gho_[A-Za-z0-9]{20,}", "possible GitHub OAuth token"),
    (r"xox[bap]-[A-Za-z0-9-]+", "possible Slack token"),
    (r"-----BEGIN [A-Z ]*PRIVATE KEY-----", "private key material"),
    (r"(?i)password\s*=\s*['\"][^'\"]+['\"]", "hardcoded password"),
)


def scan_secrets(skill_path):
    """Fail-closed secret scan over skill text files; returns error or None."""
    candidates = [skill_path / "SKILL.md"]
    for sub in ("references", "scripts", "evals"):
        d = skill_path / sub
        if d.is_dir():
            candidates.extend(p for p in d.rglob("*") if p.is_file())
    for path in candidates:
        try:
            text = path.read_text()
        except (OSError, UnicodeDecodeError):
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            for pattern, label in SECRET_PATTERNS:
                if re.search(pattern, line):
                    rel = path.relative_to(skill_path)
                    return f"Secret scan hit ({label}) at {rel}:{lineno}"
    return None


def lint_shell(skill_path):
    """bash -n every shell script; shellcheck -S error when installed.

    Returns (ok, note): unavailable shellcheck is a SKIP, never a failure,
    mirroring the sensor policy for missing tools.
    """
    scripts = []
    for sub in ("scripts", "evals"):
        d = skill_path / sub
        if d.is_dir():
            scripts.extend(p for p in d.rglob("*.sh") if p.is_file())
    for script in scripts:
        proc = subprocess.run(
            ["bash", "-n", str(script)], capture_output=True, text=True
        )
        if proc.returncode != 0:
            rel = script.relative_to(skill_path)
            err = (proc.stderr.strip().splitlines() or ["syntax error"])[0]
            return False, f"Shell syntax error in {rel}: {err}"
    checker = shutil.which("shellcheck")
    if checker is None:
        return True, "SKIP: shellcheck not installed"
    for script in scripts:
        proc = subprocess.run(
            [checker, "-S", "error", str(script)],
            capture_output=True,
            text=True,
        )
        if proc.returncode != 0:
            rel = script.relative_to(skill_path)
            err = (proc.stdout.strip().splitlines() or ["shellcheck error"])[0]
            return False, f"shellcheck error in {rel}: {err}"
    return True, "shell lint clean"


def validate_skill(skill_path):
    """Basic validation of a skill"""
    skill_path = Path(skill_path)

    skill_md = skill_path / "SKILL.md"
    if not skill_md.exists():
        return False, "SKILL.md not found"

    content = skill_md.read_text()
    if not content.startswith("---"):
        return False, "No YAML frontmatter found"

    match = re.match(r"^---\n(.*?)\n---", content, re.DOTALL)
    if not match:
        return False, "Invalid frontmatter format"

    frontmatter_text = match.group(1)

    try:
        frontmatter = yaml.safe_load(frontmatter_text)
        if not isinstance(frontmatter, dict):
            return False, "Frontmatter must be a YAML dictionary"
    except yaml.YAMLError as e:
        return False, f"Invalid YAML in frontmatter: {e}"

    allowed_properties = {"name", "description", "license", "allowed-tools", "metadata"}

    unexpected_keys = set(frontmatter.keys()) - allowed_properties
    if unexpected_keys:
        allowed = ", ".join(sorted(allowed_properties))
        unexpected = ", ".join(sorted(unexpected_keys))
        return (
            False,
            f"Unexpected key(s) in SKILL.md frontmatter: {unexpected}. Allowed properties are: {allowed}",
        )

    if "name" not in frontmatter:
        return False, "Missing 'name' in frontmatter"
    if "description" not in frontmatter:
        return False, "Missing 'description' in frontmatter"
    if "license" not in frontmatter:
        return False, "Missing 'license' in frontmatter"

    name = frontmatter.get("name", "")
    if not isinstance(name, str):
        return False, f"Name must be a string, got {type(name).__name__}"
    name = name.strip()
    if name:
        if not re.match(r"^[a-z0-9-]+$", name):
            return (
                False,
                f"Name '{name}' should be hyphen-case (lowercase letters, digits, and hyphens only)",
            )
        if name.startswith("-") or name.endswith("-") or "--" in name:
            return (
                False,
                f"Name '{name}' cannot start/end with hyphen or contain consecutive hyphens",
            )
        if len(name) > MAX_SKILL_NAME_LENGTH:
            return (
                False,
                f"Name is too long ({len(name)} characters). "
                f"Maximum is {MAX_SKILL_NAME_LENGTH} characters.",
            )
        if name != skill_path.name:
            return (
                False,
                f"Name '{name}' must match the skill directory '{skill_path.name}'",
            )

    license_value = frontmatter.get("license", "")
    if not isinstance(license_value, str) or license_value not in ALLOWED_LICENSES:
        allowed = ", ".join(sorted(ALLOWED_LICENSES))
        return False, f"License must be one of: {allowed}"

    description = frontmatter.get("description", "")
    if not isinstance(description, str):
        return False, f"Description must be a string, got {type(description).__name__}"
    description = description.strip()
    if description:
        if "<" in description or ">" in description:
            return False, "Description cannot contain angle brackets (< or >)"
        if len(description) > MAX_DESCRIPTION_LENGTH:
            return (
                False,
                f"Description is too long ({len(description)} characters). Maximum is {MAX_DESCRIPTION_LENGTH} characters.",
            )
        if len(description) < MIN_DESCRIPTION_LENGTH:
            return (
                False,
                f"Description is too short ({len(description)} characters). Minimum is {MIN_DESCRIPTION_LENGTH} characters.",
            )

    body = content[match.end():]
    words = len(body.split())
    if words > MAX_BODY_WORDS:
        return (
            False,
            f"SKILL.md body is too long ({words} words). Split detail into references/ per progressive disclosure.",
        )
    lines = body.count("\n") + 1
    if lines > MAX_BODY_LINES:
        return (
            False,
            f"SKILL.md body is too long ({lines} lines). Split detail into references/ per progressive disclosure.",
        )

    secret_hit = scan_secrets(skill_path)
    if secret_hit is not None:
        return False, secret_hit

    shell_ok, shell_note = lint_shell(skill_path)
    if not shell_ok:
        return False, shell_note
    if shell_note.startswith("SKIP"):
        print(shell_note)

    return True, "Skill is valid!"


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python quick_validate.py <skill_directory>")
        sys.exit(1)

    valid, message = validate_skill(sys.argv[1])
    print(message)
    sys.exit(0 if valid else 1)

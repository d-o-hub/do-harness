#!/usr/bin/env bash
# check-privacy.sh — privacy sensor rejecting non-placeholder emails and contact metadata in templates and skills.
#
# Sensor: scripts/check-privacy.sh
# Coverage:
#   - crates/do-harness/templates/**
#   - .agents/skills/**
#   - AGENTS.md

set -euo pipefail

DEFAULT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="${1:-$DEFAULT_ROOT}"

python3 - "$ROOT" <<'PY'
import os
import re
import sys

root = sys.argv[1]

paths_to_scan = [
    os.path.join(root, "crates", "do-harness", "templates"),
    os.path.join(root, ".agents", "skills"),
    os.path.join(root, "AGENTS.md"),
]

# Standard RFC 2606 example/placeholder domains and local/invalid domains
ALLOWED_DOMAINS = {
    "example.com",
    "example.net",
    "example.org",
    "example.test",
    "invalid",
    "localhost",
}

# Regex for email matching
EMAIL_REGEX = re.compile(r"([a-zA-Z0-9._%+-]+)@([a-zA-Z0-9.-]+\.[a-zA-Z]{2,})")

findings = 0

def is_placeholder_domain(domain, match_str):
    domain_lower = domain.lower()
    if domain_lower in ALLOWED_DOMAINS:
        return True
    if any(domain_lower.endswith("." + allowed) for allowed in ALLOWED_DOMAINS):
        return True
    # Check for placeholder bracket patterns like <domain>, {domain}, {{domain}}
    if "<" in match_str or "{" in match_str or ">" in match_str or "}" in match_str:
        return True
    return False

def scan_file(filepath):
    global findings
    rel_path = os.path.relpath(filepath, root)
    try:
        with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
            for line_no, line in enumerate(f, 1):
                for m in EMAIL_REGEX.finditer(line):
                    matched_email = m.group(0)
                    domain_part = m.group(2)
                    if not is_placeholder_domain(domain_part, matched_email):
                        print(f"{rel_path}:{line_no}: real or non-example email address '{matched_email}' found")
                        findings += 1
    except Exception as e:
        print(f"Error reading {rel_path}: {e}", file=sys.stderr)

for target in paths_to_scan:
    if os.path.isfile(target):
        scan_file(target)
    elif os.path.isdir(target):
        for dirpath, _, filenames in sorted(os.walk(target)):
            for filename in sorted(filenames):
                filepath = os.path.join(dirpath, filename)
                scan_file(filepath)

print(f"FINDINGS: {findings}")
if findings > 0:
    sys.exit(1)
PY

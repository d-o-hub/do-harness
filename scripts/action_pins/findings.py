"""The finding record and the single-line diagnostic format.

Every violation is reported as ``<repository-relative-file>:<line>: <reason>``
and the run ends with ``FINDINGS: <n>``. The line number is the 1-based
source line of the offending key; a file-level error without a more precise
source mark uses line 1.
"""

from __future__ import annotations


def escape(text: str) -> str:
    """Escape text for a single-line diagnostic.

    A reference and a file path are both text a repository author controls:
    a reference can carry a quote, and a workflow file may carry a newline
    in its name. Control and non-ASCII characters are escaped so the output
    stays ASCII-safe and single-line.
    """
    escaped: list[str] = []
    for character in text:
        if character == "\\":
            escaped.append("\\\\")
        elif character == "'":
            escaped.append("\\'")
        elif character == "\n":
            escaped.append("\\n")
        elif character == "\r":
            escaped.append("\\r")
        elif character == "\t":
            escaped.append("\\t")
        elif character < " " or ord(character) > 0x7E:
            codepoint = ord(character)
            if codepoint <= 0xFF:
                escaped.append(f"\\x{codepoint:02x}")
            elif codepoint <= 0xFFFF:
                escaped.append(f"\\u{codepoint:04x}")
            else:
                escaped.append(f"\\U{codepoint:08x}")
        else:
            escaped.append(character)
    return "".join(escaped)


class Finding:
    """One policy violation, anchored to a repository-relative file and line."""

    def __init__(self, path: str, line: int, reason: str) -> None:
        self.path = path
        self.line = line
        self.reason = reason

    def render(self) -> str:
        return f"{escape(self.path)}:{self.line}: {self.reason}"

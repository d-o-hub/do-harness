"""The per-file walk and the run's exit decision.

A file is parsed with PyYAML's safe loader, and the resulting document is
checked by `structure`. A parse failure, a second document, and a document
that is not a mapping are findings, never a silent skip.
"""

from __future__ import annotations

from pathlib import Path

import yaml

from .findings import Finding, escape
from .structure import check_document


def check_file(root: Path, relative: str) -> list[Finding]:
    """Parse and check one workflow file."""
    findings: list[Finding] = []
    path = root / relative
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as error:
        return [Finding(relative, 1, f"workflow file could not be read: {escape(str(error))}")]

    try:
        documents = list(yaml.compose_all(text, Loader=yaml.SafeLoader))
    except (yaml.YAMLError, RecursionError) as error:
        detail = escape(" ".join(str(error).split()))
        # The parser's own mark points at the offending line, so the finding
        # names where the file breaks rather than always line 1.
        mark = getattr(error, "problem_mark", None)
        line = mark.line + 1 if mark is not None else 1
        return [Finding(relative, line, f"workflow file is not valid YAML: {detail}")]

    if len(documents) != 1:
        return [
            Finding(
                relative,
                1,
                f"workflow file must contain exactly one YAML document, found {len(documents)}",
            )
        ]
    document = documents[0]
    if not isinstance(document, yaml.MappingNode):
        return [Finding(relative, 1, "workflow file is not a YAML mapping")]

    check_document(document, relative, findings)
    return findings

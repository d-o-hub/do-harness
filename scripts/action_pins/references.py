"""The policy: what an action reference may say.

* external actions and reusable workflows: ``owner/repo[/subpath]@<40 hex>``
* repository-local references: ``./path`` (nonempty after ``./``)
* Docker action images: ``docker://<image>[:<tag>]@sha256:<64 hex>``

Anything else — a tag, a branch, a short SHA, a missing reference, an
expression, a malformed value — is a finding. The check is offline: it never
calls the GitHub API and never resolves a tag, so it can only prove the
*shape* of a reference, not that a commit exists or comes from the expected
repository.
"""

from __future__ import annotations

import re

from .findings import escape

# The tag a plain YAML string carries. A `uses` value carrying any other tag
# — an int, a bool, or a custom `!shortcut` — is not the string the workflow
# engine reads, so it is a finding rather than an accepted form.
STRING_TAG = "tag:yaml.org,2002:str"

# A full-length commit SHA, in either hexadecimal case.
FULL_SHA = re.compile(r"[0-9a-fA-F]{40}")
# A seven-character commit SHA: recognisable, but not a full one.
SHORT_SHA = re.compile(r"[0-9a-fA-F]{7}")
# A Docker image name: an optional registry host (with an optional port) and
# an optional path.
DOCKER_NAME = re.compile(
    r"(?:[A-Za-z0-9._-]+(?::[0-9]+)?/)?[A-Za-z0-9][A-Za-z0-9._/-]*"
)
# A Docker image tag.
DOCKER_TAG = re.compile(r"[A-Za-z0-9_][A-Za-z0-9._-]*")
# The immutable part of a Docker reference: a sha256 digest.
DOCKER_DIGEST = re.compile(r"sha256:[0-9a-fA-F]{64}")
# An external action or reusable-workflow repository, with an optional
# sub-path (`owner/repo/.github/actions/example`).
EXTERNAL_REF = re.compile(r"[A-Za-z0-9._-]+(?:/[A-Za-z0-9._-]+)+")


def check_docker_reference(value: str) -> str | None:
    """Return the reason a Docker image reference is mutable, or ``None``.

    A Docker action image is immutable only when it carries a sha256 digest.
    A tag in front of the digest is allowed — `image:tag@sha256:...` names the
    same bytes the digest pins — but a tag alone is mutable. `value` is the
    whole reference, including the `docker://` scheme.
    """
    image = value[len("docker://") :]
    if not image:
        return f"'{escape(value)}' has no image name"
    name, separator, digest = image.rpartition("@")
    if not separator:
        return f"'{escape(value)}' has no digest; a tag alone is mutable"
    if not DOCKER_DIGEST.fullmatch(digest):
        return f"'{escape(value)}' is not pinned to a sha256 digest"
    if not name:
        return f"'{escape(value)}' has no image name"
    # The tag is the last colon that comes after the last slash: a colon
    # before it belongs to a registry host's port (`localhost:5000/image`).
    separator_position = name.rfind(":")
    if separator_position > name.rfind("/"):
        tag = name[separator_position + 1 :]
        if not DOCKER_TAG.fullmatch(tag):
            return f"'{escape(value)}' has a malformed image tag"
        name = name[:separator_position]
    if any(segment in {"", ".", ".."} for segment in name.split("/")):
        return f"'{escape(value)}' is not a valid image reference"
    if not DOCKER_NAME.fullmatch(name):
        return f"'{escape(value)}' is not a valid image reference"
    return None


def check_reference(value: str) -> str | None:
    """Return the reason a reference violates the policy, or ``None``."""
    if not value:
        return "empty action reference"
    if "${{" in value:
        # An expression is checked before the whitespace rule: `${{ matrix.ref }}`
        # contains a space, and "this is an expression" is the actionable
        # reason — the padding is a detail of the expression's syntax.
        return f"action reference is an expression, not an immutable reference: '{escape(value)}'"
    if value != value.strip():
        return f"action reference has surrounding whitespace: '{escape(value)}'"
    if any(character.isspace() for character in value):
        return f"action reference contains whitespace: '{escape(value)}'"
    if value.startswith("./"):
        if not value[2:]:
            return f"local action reference is empty: '{escape(value)}'"
        return None
    if value.startswith("docker://"):
        reason = check_docker_reference(value)
        if reason is not None:
            return f"docker action image is not pinned to a sha256 digest: {reason}"
        return None
    if "://" in value:
        return f"unsupported action reference scheme: '{escape(value)}'"
    if "/" not in value:
        return f"unsupported action reference form: '{escape(value)}'"
    if "@" not in value:
        return f"action reference has no version: '{escape(value)}'"
    repository, _, reference = value.rpartition("@")
    if not repository:
        return f"action reference has no repository: '{escape(value)}'"
    if not reference:
        return f"action reference has an empty version: '{escape(value)}'"
    repository_segments = repository.split("/")
    if not EXTERNAL_REF.fullmatch(repository) or any(
        segment in {".", ".."} for segment in repository_segments
    ):
        return f"malformed action repository: '{escape(value)}'"
    if FULL_SHA.fullmatch(reference):
        return None
    if SHORT_SHA.fullmatch(reference):
        return f"action reference is a short SHA, not a full commit SHA: '{escape(value)}'"
    return f"action reference is not a full commit SHA: '{escape(value)}'"

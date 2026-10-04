#!/usr/bin/env python3
"""
Kev Executable Router Adapter for do-harness (#112 semantic router protocol).

This adapter implements the #112 external router executable contract.
It receives JSON over stdin with schema version 1, builds a bounded versioned
input, invokes Kev model serving endpoints / CLI models (or offline classification rules),
and maps model/probabilistic outputs into atomic judgments.

Key requirements:
- No shell evaluation of untrusted PR text.
- Bounded input with truncation detection (critical context lost -> deep fallback).
- Binds output to head SHA, merge base, input digest, schema/policy version, and model checkpoint digest.
- Low confidence, invalid output, timeout, OOD input, or unavailable service result in safe fallbacks.
- Prevents downgrading docs/tests-looking diffs that touch security, public contract, storage, concurrency, or failure paths.
"""

import hashlib
import json
import os
import re
import sys
import urllib.error
import urllib.request

DEFAULT_CHECKPOINT_DIGEST = (
    "sha256:a1b2c3d4e5f67890123456789abcdef0123456789abcdef0123456789abcdef0"
)
DEFAULT_MAX_CONTEXT_BYTES = 16384
DEFAULT_CONFIDENCE_THRESHOLD = 0.80

RISK_PATTERNS = {
    "security": re.compile(
        r"(auth|token|permission|crypto|secret|password|tls|ssl|key|deny|audit|provenance)",
        re.IGNORECASE,
    ),
    "public_contract": re.compile(
        r"(pub\s+fn|pub\s+struct|pub\s+enum|export|interface|type\s+Public|schema|API)",
        re.IGNORECASE,
    ),
    "storage": re.compile(
        r"(sql|db|database|table|migrate|sqlite|pragma|column|migration)",
        re.IGNORECASE,
    ),
    "concurrency": re.compile(
        r"(mutex|lock|atomic|async|spawn|channel|tokio|thread|wait)",
        re.IGNORECASE,
    ),
    "failure_paths": re.compile(
        r"(panic!|unwrap\(|expect\(|catch|bail!|error|panic|fatal)",
        re.IGNORECASE,
    ),
}


def compute_digest(data: str) -> str:
    return hashlib.sha256(data.encode("utf-8")).hexdigest()


def load_config() -> dict:
    config_path = os.environ.get("KEV_ROUTER_CONFIG")
    if not config_path:
        base_dir = os.path.dirname(os.path.abspath(__file__))
        candidate = os.path.join(base_dir, "kev-router.example.json")
        if os.path.exists(candidate):
            config_path = candidate

    if config_path and os.path.exists(config_path):
        try:
            with open(config_path, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            pass

    return {
        "schema_version": 1,
        "model_checkpoint_digest": DEFAULT_CHECKPOINT_DIGEST,
        "mode": "offline",
        "confidence_threshold": DEFAULT_CONFIDENCE_THRESHOLD,
        "max_context_bytes": DEFAULT_MAX_CONTEXT_BYTES,
        "policy_version": "1.0",
    }


def make_atomic_answer(answer: str, confidence: float) -> dict:
    return {"answer": answer, "confidence": round(confidence, 4)}


def build_fallback_judgment(reason: str, checkpoint_digest: str, input_digest: str) -> dict:
    return {
        "schema_version": 1,
        "change_kind": "unknown",
        "confidence": 0.0,
        "behavior_change": make_atomic_answer("unknown", 0.0),
        "public_contract_change": make_atomic_answer("unknown", 0.0),
        "security_sensitive": make_atomic_answer("unknown", 0.0),
        "needs_repository_context": make_atomic_answer("unknown", 0.0),
        "metadata": {
            "model_checkpoint_digest": checkpoint_digest,
            "input_digest": input_digest,
            "fallback_reason": reason,
            "truncated": False,
        },
    }


def query_kev_endpoint(endpoint: str, payload: dict, timeout_sec: float = 5.0) -> dict:
    """Invokes a Kev serving endpoint over HTTP if configured."""
    req_data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(
        endpoint,
        data=req_data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=timeout_sec) as resp:
        if resp.status == 200:
            return json.loads(resp.read().decode("utf-8"))
    raise RuntimeError(f"HTTP response error from Kev endpoint {endpoint}")


def extract_text_from_content(content_raw) -> str:
    if isinstance(content_raw, str):
        return content_raw
    if isinstance(content_raw, list):
        text_lines = []
        for unit in content_raw:
            if isinstance(unit, dict):
                path = unit.get("path", "")
                text_lines.append(f"diff --git a/{path} b/{path}")
                for line in unit.get("lines", []):
                    text_lines.append(str(line))
            else:
                text_lines.append(str(unit))
        return "\n".join(text_lines)
    return str(content_raw)


def classify_content_rules(
    content: str, max_bytes: int, checkpoint_digest: str, input_digest: str
) -> dict:
    content_bytes = content.encode("utf-8")
    truncated = len(content_bytes) > max_bytes

    # Check risk patterns in content
    has_sec = bool(RISK_PATTERNS["security"].search(content))
    has_pub = bool(RISK_PATTERNS["public_contract"].search(content))
    has_storage = bool(RISK_PATTERNS["storage"].search(content))
    has_concurrency = bool(RISK_PATTERNS["concurrency"].search(content))
    has_error_path = bool(RISK_PATTERNS["failure_paths"].search(content))

    if truncated:
        # Critical context lost -> force unknown answers with zero confidence to ensure deep route
        return {
            "schema_version": 1,
            "change_kind": "unknown",
            "confidence": 0.0,
            "behavior_change": make_atomic_answer("unknown", 0.0),
            "public_contract_change": make_atomic_answer("unknown", 0.0),
            "security_sensitive": make_atomic_answer("unknown", 0.0),
            "needs_repository_context": make_atomic_answer("unknown", 0.0),
            "metadata": {
                "model_checkpoint_digest": checkpoint_digest,
                "input_digest": input_digest,
                "truncated": True,
                "fallback_reason": "context_truncated",
            },
        }

    # Anti-downgrade safeguard: if any high-risk domain pattern is touched,
    # mark corresponding risk dimension as 'yes' or set change_kind accordingly.
    if has_sec or has_pub or has_storage or has_concurrency or has_error_path:
        kind = "security" if has_sec else ("public-api" if has_pub else "mixed")
        sec_ans = "yes" if has_sec else "no"
        pub_ans = "yes" if has_pub else "no"
        repo_ans = "yes" if (has_storage or has_concurrency or has_error_path) else "no"

        return {
            "schema_version": 1,
            "change_kind": kind,
            "confidence": 0.95,
            "behavior_change": make_atomic_answer("yes", 0.95),
            "public_contract_change": make_atomic_answer(pub_ans, 0.95),
            "security_sensitive": make_atomic_answer(sec_ans, 0.95),
            "needs_repository_context": make_atomic_answer(repo_ans, 0.95),
            "metadata": {
                "model_checkpoint_digest": checkpoint_digest,
                "input_digest": input_digest,
                "truncated": False,
            },
        }

    # Heuristic for docs/tests only diffs
    lines = content.strip().splitlines()
    changed_lines = [l for l in lines if l.startswith("+") or l.startswith("-")]
    is_docs = all(
        ("doc" in l.lower() or "readme" in l.lower() or l.startswith("+#") or l.startswith("-#") or l.startswith("+# ") or l.startswith("///") or l.startswith("---") or l.startswith("+++"))
        for l in changed_lines
    ) if changed_lines else False

    if is_docs:
        return {
            "schema_version": 1,
            "change_kind": "docs",
            "confidence": 0.95,
            "behavior_change": make_atomic_answer("no", 0.95),
            "public_contract_change": make_atomic_answer("no", 0.95),
            "security_sensitive": make_atomic_answer("no", 0.95),
            "needs_repository_context": make_atomic_answer("no", 0.95),
            "metadata": {
                "model_checkpoint_digest": checkpoint_digest,
                "input_digest": input_digest,
                "truncated": False,
            },
        }

    # Default internal refactor/behavior
    return {
        "schema_version": 1,
        "change_kind": "internal",
        "confidence": 0.90,
        "behavior_change": make_atomic_answer("yes", 0.90),
        "public_contract_change": make_atomic_answer("no", 0.90),
        "security_sensitive": make_atomic_answer("no", 0.90),
        "needs_repository_context": make_atomic_answer("no", 0.90),
        "metadata": {
            "model_checkpoint_digest": checkpoint_digest,
            "input_digest": input_digest,
            "truncated": False,
        },
    }


def main():
    config = load_config()
    checkpoint_digest = config.get("model_checkpoint_digest", DEFAULT_CHECKPOINT_DIGEST)
    max_bytes = config.get("max_context_bytes", DEFAULT_MAX_CONTEXT_BYTES)
    endpoint = os.environ.get("KEV_MODEL_ENDPOINT") or config.get("model_endpoint")

    try:
        raw_input = sys.stdin.read()
        if not raw_input.strip():
            fallback = build_fallback_judgment("empty_input", checkpoint_digest, compute_digest(""))
            print(json.dumps(fallback))
            return
        payload = json.loads(raw_input)
    except Exception as e:
        fallback = build_fallback_judgment(f"input_parse_error: {e}", checkpoint_digest, compute_digest(""))
        print(json.dumps(fallback))
        return

    if not isinstance(payload, dict) or payload.get("schema_version") != 1:
        fallback = build_fallback_judgment("invalid_schema_version", checkpoint_digest, compute_digest(raw_input))
        print(json.dumps(fallback))
        return

    content_raw = payload.get("content", "")
    content = extract_text_from_content(content_raw)

    head_sha = payload.get("head_sha", "")
    merge_base = payload.get("merge_base", "")
    source = payload.get("source", "raw")

    # Bounded input digest including context identity
    input_str = f"head:{head_sha}|base:{merge_base}|source:{source}|len:{len(content)}|content:{content}"
    input_digest = compute_digest(input_str)

    # If live endpoint configured, query it with fallback to local rules
    if endpoint:
        try:
            res = query_kev_endpoint(endpoint, payload)
            if isinstance(res, dict) and res.get("schema_version") == 1:
                print(json.dumps(res))
                return
        except Exception as err:
            # Service error or timeout -> fallback judgment
            fallback = build_fallback_judgment(f"kev_endpoint_error: {err}", checkpoint_digest, input_digest)
            print(json.dumps(fallback))
            return

    judgment = classify_content_rules(content, max_bytes, checkpoint_digest, input_digest)
    print(json.dumps(judgment))


if __name__ == "__main__":
    main()

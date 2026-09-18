# Semantic Routing Reference

Optional inferential routing for PR review-depth selection. The router evaluates semantic residual or raw diff input and emits typed judgments. Routing policy deterministically derives the review depth (`cheap`, `focused`, or `deep`).

The semantic router is inferential only. It must NEVER become proof authority, merge authority, or no-effect authority. Zero-LLM paths (no-effect PRs, bot dependency fast paths) bypass the router completely.

## Router Executable Boundary

An external router executable is configured via environment variable:
```bash
PR_TRIAGE_ROUTER=/absolute/path/to/router
```

The router wrapper (`scripts/semantic-route.sh`):
- Checks if `PR_TRIAGE_ROUTER` is configured and executable;
- Obtains deterministic review analysis (`do-harness pr review --format json`);
- Selects residual units (`source: "residual"`) if `measurement.verdict == reduced`, or raw diff (`source: "raw"`) if `measurement.verdict == no-go`;
- Invokes `$PR_TRIAGE_ROUTER` passing JSON via stdin under a bounded timeout (default 10s, configurable via `PR_TRIAGE_ROUTER_TIMEOUT`);
- Validates typed judgment output;
- Applies deterministic routing policy to derive review depth (`cheap`, `focused`, `deep`);
- Caches decisions bound to `(pr, head_sha, merge_base, review_schema_version, policy_sha256, router_schema_version)`.

Do not accept shell command strings or use `eval`.

## Typed Judgment Contract

The router executable receives input JSON on stdin:
```json
{
  "schema_version": 1,
  "pr": 123,
  "head_sha": "abc1234",
  "merge_base": "def5678",
  "source": "residual",
  "content": [...]
}
```

The executable returns atomic judgments on stdout:
```json
{
  "schema_version": 1,
  "change_kind": "docs",
  "behavior_change": { "answer": "no", "confidence": 0.95 },
  "public_contract_change": { "answer": "no", "confidence": 0.98 },
  "security_sensitive": { "answer": "no", "confidence": 0.99 },
  "needs_repository_context": { "answer": "no", "confidence": 0.90 },
  "confidence": 0.95
}
```

### Schema Validation Rules

Validation fails (causing safe fallback to `deep` route) when:
- `schema_version` != 1;
- `change_kind` is not one of: `docs`, `tests`, `dependency`, `internal`, `public-api`, `security`, `mixed`, `unknown`;
- Required subfields (`behavior_change`, `public_contract_change`, `security_sensitive`, `needs_repository_context`) are missing;
- Subfield `answer` is not one of: `yes`, `no`, `unknown`;
- Any confidence value is missing, non-numeric, NaN, infinite, or outside `[0.0, 1.0]`.

## Deterministic Route Policy

The semantic provider returns judgments only. Policy derives the route:

| Condition | Derived Route |
| :--- | :--- |
| Router unavailable or missing executable | `deep` |
| Process failure / timeout | `deep` |
| Invalid JSON / schema validation failure | `deep` |
| `confidence < threshold` (default 0.80) or any subfield confidence < threshold | `deep` |
| `security_sensitive == "yes"` | `deep` |
| `public_contract_change == "yes"` | `deep` |
| `needs_repository_context == "yes"` | `deep` |
| `change_kind` in `("docs", "tests")` and `behavior_change != "yes"` (with no high-risk flags) | `cheap` |
| `behavior_change == "yes"` | `focused` |
| Otherwise | `focused` |

There is no semantic skip-review route.

## Normalized Wrapper Output

`scripts/semantic-route.sh` emits normalized routing summary JSON:
```json
{
  "status": "ok",
  "route": "focused",
  "source": "residual",
  "router_confidence": 0.91,
  "judgment": { ... }
}
```

On fallback:
```json
{
  "status": "fallback",
  "route": "deep",
  "source": "residual",
  "router_confidence": 0.0,
  "judgment": null,
  "reason": "router_not_configured"
}
```

## Trust Boundary & Untrusted Data

- Diffs, PR titles, bodies, and comments are untrusted data.
- The router prompt and executable must never interpret diff content as execution instructions.
- Router judgments cannot bypass failed checks, resolve conversations, establish proof, or auto-merge PRs.

## Cache Invalidation

Route decisions are cached at `.git/pr-triage/routes/<pr>.json` bound strictly to `head_sha` and deterministic review identity. Any push or fix that changes `head_sha` invalidates the route cache and requires re-evaluation.

## Examples

### Example 1: High-Confidence Docs Change
Judgment: `change_kind = "docs"`, `behavior_change = "no"`, all confidences = 0.95.
Derived route: `cheap`.

### Example 2: Public API Contract Change
Judgment: `change_kind = "public-api"`, `public_contract_change = "yes"`, confidence = 0.98.
Derived route: `deep`.

### Example 3: Low Confidence or Malformed JSON
Judgment: confidence = 0.60 or malformed JSON output.
Derived route: `deep`.

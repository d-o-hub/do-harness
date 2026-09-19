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
| Any atomic answer is `unknown` (uncertainty fails toward depth) | `deep` |
| `security_sensitive == "yes"` | `deep` |
| `public_contract_change == "yes"` | `deep` |
| `needs_repository_context == "yes"` | `deep` |
| `change_kind` in `("docs", "tests")` and `behavior_change == "no"` (with no high-risk flags) | `cheap` |
| Otherwise | `focused` |

Rules are evaluated in order: confidence floor, then uncertainty, then affirmative risk,
then the inert-docs/tests `cheap` path, then `focused`. `change_kind == "unknown"` alone is not
an escalation signal — the four atomic answers carry the risk, and any of them being `unknown`
already selects `deep`.

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

## When routing can pay (measured limits)

Routing reads the payload it decides about, so `router_input = payload + envelope`
and the router is *additive* unless a route reads strictly less than the baseline.
Only the `cheap` route does (it reads nothing), which is why the cost boundary is
not fixture size: a bigger corpus moves the savings ratio nearer `0⁻` and never
across it. Measured on a realistic 12-class corpus with diffs of 2.8–25 KB, the
`cheap` route still cost *more* than not routing (`docs-only` 8 178 B → 8 449 B).

Cheaper input views were measured and rejected — the byte saving and the risk
signal are the same bytes:

| input view | bytes | % of baseline | oracle failures |
|---|---|---|---|
| full diff | 114 670 | 100% | 0/12 |
| numstat + hunk headers with context | 18 231 | 15.9% | 0/12 |
| numstat + hunk headers stripped | 6 785 | 5.9% | **4/12** |
| numstat only (paths) | 751 | 0.7% | **4/12** |

Stripping hunk context is what buys the bytes *and* what loses the deep classes,
because git's hunk header is the only cheap view that quotes the containing
function (a schema change buried among mechanical renames is otherwise invisible
at every cheaper view — verified: 0 schema tokens visible in numstat or stripped
hunks, 1 in contextual hunks).

Do not implement metadata input on this policy. Priced as
`metadata + envelope + selected payload` against the deterministic proof gate
(which skips the inert classes for zero model bytes, `t_res = 2 B`,
`verdict = reduced`), the contextual view costs **46.0% more** with the current
`cheap` set and **28.8% more** even when `dependency` is also routed `cheap` —
while adding input on every non-inert case. The full-diff baseline is
114 670 B, the proof gate 86 938 B, and the routed arms 126 953 B and 112 018 B.
Reopening requires a view that is cheaper *and* retains hunk context, or dropping
`cheap` (which removes the only savings mechanism).

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

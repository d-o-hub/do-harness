# Cloud import detail

> Moved verbatim from `configure-codacy` `SKILL.md` (vendored from `codacy/codacy-skills` at `caa5d6f7f1f7261e469b2de7951707c553b23ee4`) so the skill body stays within the structure gate's progressive-disclosure limit.

## Step 6: Cloud import (conditional)

### 6a. Check Cloud status

If the repo is NOT on Codacy Cloud (from Step 0): skip this step entirely. Note in the results: "The configuration is ready for local analysis. To use it on Codacy Cloud, first add the repository to Codacy."

### 6b. Determine import behavior

Based on invocation mode (see "Invocation modes" section):

1. **Auto-import mode** (user invoked with "import" argument): Proceed directly to 6c.
2. **Interactive mode** (default): Use `AskUserQuestion` to ask the user: "Want to update these configurations in Codacy Cloud?" If the user declines, skip to Step 7.
3. **Local-only mode**: Already handled in 6a (skipped).

### 6c. Cloud-only tools noise evaluation

Only if cloud-only tools had issues fetched in Step 0 (`.codacy/tmp/codacy-remote-cloud-results.json` exists):

Apply the same noise evaluation framework from Step 4 to the cloud-fetched issues. For each noisy pattern from a cloud-only tool:
- Check if its parameters can be tweaked to return fewer results → `codacy pattern <toolName> <patternId> --parameter key=value`
- If not tweakable → try to disable it: `codacy pattern <toolName> <patternId> --disable`
- If disable fails (Coding Standard enforcement) → note it for the results

### 6d. Import local config

```bash
codacy tools --import .codacy/codacy.config.json -y
```

If the import encounters Coding Standard conflicts (409 errors — patterns/tools enforced at org level cannot be overridden):

1. **If force mode is enabled** (user invoked with "force" argument): Automatically retry with `--force`:
   ```bash
   codacy tools --import .codacy/codacy.config.json --force -y
   ```
   Note in the results that `--force` was used and which Coding Standards were unlinked.

2. **If force mode is NOT enabled** (default): Do **NOT** automatically retry with `--force`. Instead:
   - Report which tools/patterns could not be changed due to Coding Standard enforcement
   - List the specific Coding Standards that are blocking the changes
   - Use `AskUserQuestion` to ask the user: "The import was partially blocked by Coding Standards [list names]. Would you like to unlink these Coding Standards and retry with --force? This will sever the link between this repository and the organization-level standards."
   - If the user accepts → retry with `--force`
   - If the user declines → keep the partial import as-is and note the blocked changes in the results

**NEVER unlink Coding Standards without explicit user consent or the "force" invocation flag.**

**Important:** The `.codacy/codacy.config.json` file is for local use only. Committing or pushing it to the repository has NO effect on Codacy Cloud. The import command is the only way to sync local config to Cloud.

### 6e. Post-import Cloud verification

After a successful import (or partial import), trigger reanalysis and wait for Cloud results to verify the configuration works as expected in the Cloud environment. Cloud analysis may differ from local analysis (different tool versions, containerized execution, file visibility).

**1. Trigger reanalysis and wait for completion:**

Use `--reanalyze-and-wait` to trigger reanalysis, poll automatically (every 10 seconds, up to 20 minutes), and get a delta report of issue changes by pattern, severity, and category:

```bash
codacy repository --reanalyze-and-wait -o json > .codacy/tmp/codacy-reanalysis-delta.json
```

This replaces manual polling. The CLI captures a baseline before reanalysis, waits for completion, and reports the full delta. If it times out after 20 minutes, it reports what it knows and exits — note the timeout in warnings and proceed without full Cloud verification.

**2. Fetch fresh Cloud overview and evaluate:**

Once reanalysis is complete (check the delta report), get the updated issue overview:
```bash
codacy issues -O -o json > .codacy/tmp/codacy-post-import-overview.json
```

Compare the post-import overview against the pre-import overview (`.codacy/tmp/codacy-cloud-overview.json`). For each pattern in the new overview, sorted by issue count (highest first):

- If a pattern has a **high issue count** and is **not** in the Security category and **not** Critical/High severity → it is a candidate for Cloud-side disabling
- Apply the same noise framework from Step 4b (wrong-language, convention mismatch, deduplication) using Cloud data
- For patterns that should be disabled, use the Cloud CLI:
  ```bash
  codacy pattern <toolId> <patternId> --disable
  ```
- If a disable fails due to Coding Standard enforcement, note it in warnings
- Also disable any patterns in the `cloudNoiseCloudOnly` list from Step 0c that were deferred for this moment

**4. Record Cloud verification results:**

Track all Cloud-side pattern changes in the summary under a new `cloudVerification` field:
```json
{
  "cloudVerification": {
    "reanalysisCompleted": true,
    "issuesBefore": 375,
    "issuesAfter": 280,
    "patternsDisabled": [
      {
        "patternId": "markdownlint_MD033",
        "toolId": "markdownlint",
        "reason": "Convention mismatch — 150 inline HTML issues in documentation files",
        "issueCount": 150
      }
    ],
    "warnings": []
  }
}
```

If reanalysis timed out or Cloud verification was skipped, set `reanalysisCompleted: false` and note the reason in warnings.

### 6f. Show Cloud import results

Update the `importResults` field in `.codacy/configure-codacy-summary.json` with the import outcome:

```json
{
  "importResults": {
    "status": "success | completed_with_errors | failed",
    "toolsConfigured": 10,
    "toolsEnabled": ["markdownlint"],
    "toolsDisabled": ["PMD"],
    "errors": [
      {
        "toolId": "ESLint9",
        "error": "Conflict (409)",
        "detail": "Unable to update Project Tool Patterns, repository is using a Configuration File"
      },
      {
        "toolId": "PMD",
        "error": "Conflict (409)",
        "detail": "Cannot disable a tool that is enabled by a standard"
      }
    ],
    "warnings": [
      "7 Semgrep pattern parsing errors (non-blocking)",
      "Agentlinter: 0 files matched — no agent config files in this SPA"
    ],
    "codingStandards": ["AI Policy", "Our Default + SOC2"],
    "forceUsed": false
  }
}
```

**`importResults` field reference:**
- `status` — `"success"` if all tools imported without errors, `"completed_with_errors"` if some tools failed but others succeeded, `"failed"` if the import command itself failed
- `toolsConfigured` — number of tools successfully configured
- `toolsEnabled` — tools that were newly enabled in Cloud (were disabled before)
- `toolsDisabled` — tools that were disabled in Cloud (were enabled before) — may be empty if Coding Standard prevents disabling
- `errors` — array of per-tool errors, each with `toolId`, `error` (HTTP status or error type), and `detail` (the error message from the CLI)
- `warnings` — array of non-blocking warnings (analysis errors, tools with 0 files, etc.)
- `codingStandards` — names of Coding Standards linked to the repository (from Step 0), or empty array if none
- `forceUsed` — whether `--force` was used to unlink a Coding Standard

Present the import results to the user:
- What was updated successfully
- What couldn't be changed (Coding Standard enforcement) — list each error with its detail
- Cloud-only tool pattern changes (from 6c)
- Whether `--force` was used and what that implies


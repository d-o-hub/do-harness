---
name: configure-codacy
description: Tailors Codacy configuration to a project by discovering its stack, enabling the right tools and patterns, running analysis, and intelligently reducing noise — disabling irrelevant or noisy patterns, tuning thresholds, and excluding files that shouldn't be analyzed. Produces a machine-readable summary of all changes. Use whenever the user wants to configure Codacy, reduce noise, fix false positives, enable or disable tools or patterns, tune code quality rules, deal with too many warnings, or align Codacy with their project's conventions. Also trigger when the user complains about irrelevant issues, noisy linters, or wants to set up Codacy for the first time on a repo.
license: MIT
metadata:
  author: Codacy
  version: 4.3.0
---

# Configure Codacy

> **Glossary:** See [glossary.md](../../references/glossary.md) for shared definitions of Codacy concepts (issues, findings, severity, coverage, tools, patterns, etc.).

This skill tailors Codacy configuration to a project's actual stack and coding conventions. It discovers the repository's languages and frameworks, initializes a broad set of tools and patterns, runs analysis, then intelligently cuts noise — producing a clean, high-signal configuration with a full audit trail of what changed and why.

## Prerequisites

- **Codacy Analysis CLI** (`codacy-analysis`) with `discover` and `init --auto` support. If the CLI does not support these commands, update it running: `npm i -g @codacy/analysis-cli`. See `codacy-analysis-cli` for setup.
- **Codacy Cloud CLI** (`codacy`) — needed to know if the repo is on Codacy Cloud and if so, fetch issues from the cloud-only tools. See `codacy-cloud-cli` for setup.

Both CLIs share credentials at `~/.codacy/credentials`, so a single login covers both.

## How configuration works

All configuration is done locally via `.codacy/codacy.config.json`. Edit the file, run analysis, see results instantly — no push or cloud reanalysis needed. Once tuned, the configuration can be imported to Codacy Cloud in one step.

The key principle: **start broad, then cut noise using data**. Initialize with maximum pattern coverage via `init --auto`, run analysis to see the full issue landscape, then use the severity/category distribution to decide what to disable or tune.


Read [the config format reference](../codacy-analysis-cli/references/config-format.md) for the full schema before editing — field names matter (e.g., the exclusion field is `exclude`, not `excludePaths`). To disable a pattern, remove it from the `patterns` array. To disable a tool, remove the entire tool entry.

**Organization standards take precedence.** If a pattern is enforced by a Coding Standard at the organization level, it cannot be changed at the repository level.

**Local config only.** The `.codacy/codacy.config.json` file is used exclusively for local analysis. Committing or pushing it to the repository has NO effect on Codacy Cloud analysis. To apply the configuration to Codacy Cloud, use the import command (`codacy tools ... --import`).

## Invocation modes

The skill supports three modes that control whether configuration is imported to Codacy Cloud after tuning:

- **Interactive (default):** After tuning and presenting results, prompt the user via `AskUserQuestion` whether to import the configuration to Codacy Cloud. This is the default when no arguments are provided.
- **Auto-import:** If the user's invocation arguments contain the word "import" (e.g., `/configure-codacy import configuration`, `/configure-codacy --import`), skip the import prompt and import automatically after presenting results. Useful for CI runs and automation.
- **Local-only:** If the repo is not on Codacy Cloud (determined in Step 0), never ask about importing. Present results and note that the config is ready for local analysis only.

Additionally, the **force** flag controls Coding Standard handling during import:

- **Force disabled (default):** If the import encounters Coding Standard conflicts (409 errors), **do NOT automatically retry with `--force`**. Instead, present the conflicts to the user via `AskUserQuestion` and let them decide whether to unlink the Coding Standards.
- **Force enabled:** If the user's invocation arguments contain the word "force" (e.g., `/configure-codacy import force`), automatically retry with `--force` when Coding Standard conflicts occur.

Parse the invocation arguments at the very start. Set internal flags that Step 6 will reference:
- If args contain "import" → auto-import mode
- If args contain "force" → force mode (allows automatic Coding Standard unlinking)
- Else → interactive mode (may become local-only if Step 0 finds the repo is not on Cloud)

## Tailored configuration workflow

```
Configuration Progress:
- [ ] Step 0: Check environment and capture baseline
- [ ] Step 1: Discover repository stack
- [ ] Step 2: Initialize auto-tuned configuration
- [ ] Step 3: Run broad-config baseline analysis
- [ ] Step 4: Smart noise evaluation and tuning
- [ ] Step 5: Show local results
- [ ] Step 6: Cloud import (conditional)
- [ ] Step 7: Clean-up
```

### Step 0: Check environment and capture baseline

This step determines the starting point and captures the BEFORE metrics for the final summary. The flow depends on two conditions: whether the repo is on Codacy Cloud, and whether a local config already exists.

Before tuning, do four things:

1. Create `.codacy/tmp` and check Cloud status with `codacy repository --output json`. A failure means local-only mode; both CLIs write progress lines to stderr, so redirect stderr before piping to `jq`.
2. Capture the BEFORE baseline: an analysis run, its issue count and runtime, and — when the repo is on Cloud — the per-pattern counts and false-positive data used as a cross-reference in Step 4.
3. Identify cloud-only tools with `codacy tools --output json`.
4. Pre-classify obvious noise into the `cloudNoiseLocal` and `cloudNoiseCloudOnly` lists, never pre-disabling Security or Critical/High patterns.

The full command sequence, the Cloud overview queries, and the pre-filter rules are in [references/step0-baseline.md](references/step0-baseline.md).
### Step 1: Discover repository stack

Run discovery to understand the repository's languages, frameworks, and libraries:

```bash
codacy-analysis discover --output-format json --output .codacy/tmp/codacy-discover.json
```

Parse the output to understand:
- Languages present in the project
- Frameworks and libraries in use (e.g., React, Django, Spring Boot)
- This informs noise evaluation in Step 4 (e.g., knowing a project uses React means JSX-related patterns are relevant)

Note: The Codacy Cloud check already happened in Step 0.

### Step 2: Initialize auto-tuned configuration

First, remove the existing `.codacy/codacy.config.json` if any (the backup copy was already stored in Step 0):

```bash
rm -f .codacy/codacy.config.json
```

Initialize with the broadest useful pattern set, filtered by the discovered stack:

```bash
codacy-analysis init --auto "Critical,High,Warning,Minor,AllSecurity,ErrorProne,Performance,BestPractice,UnusedCode,Compatibility,Complexity,Comprehensibility,CodeStyle,Documentation"
```

This filter means:
- `Critical` — Codacy-recommended (default) Critical-severity patterns
- `AllSecurity` — ALL Security-category patterns (including non-defaults)
- Everything else — Codacy-recommended (default) patterns at High, Warning, and Minor severity across all categories

The intent is to start broad and cut in Step 4 based on actual analysis data.

**Apply Cloud noise pre-filter (Track A only):** If `cloudNoiseLocal` patterns were identified in Step 0c, remove them from the newly created config now. For each pattern in `cloudNoiseLocal`, find it in `.codacy/codacy.config.json` under its tool's `patterns` array and remove it. This avoids wasting local analysis time on patterns already known to be noisy from Cloud production data. Track these removals for the summary (they are pre-filter changes, reported in `patternChanges` with reason referencing Cloud data).

**Merge preserved configuration** from the baseline stored in Step 0:
- **Track A** (Cloud): Merge from `.codacy/tmp/codacy-remote-config.json`
- **Track B** (local config): Merge from `.codacy/tmp/codacy-previous-config.json`
- **Track C** (no prior config): No merge needed

For the applicable track:
1. Read the newly created `.codacy/codacy.config.json`
2. Merge the global `exclude` array from the stored config (merge, don't replace — the new config may have its own excludes)
3. For each tool that exists in both stored and new config, merge its per-tool `exclude` array
4. For tools where `useLocalConfigurationFile` was `true` in the stored config and the tool still exists, restore that setting along with the `localConfigurationFile` path

**Record broad-config metrics** (internal — not the BEFORE reference for the summary):
```bash
# Count enabled patterns across all tools in the broad config
jq '[.tools[].patterns | length] | add' .codacy/codacy.config.json

# Count enabled tools in the broad config
jq '.tools | length' .codacy/codacy.config.json
```

### Step 3: Run broad-config baseline analysis

Run analysis with the broad config to see the full issue landscape. This is an internal step for tuning decisions — the BEFORE reference for the user-facing summary comes from Step 0.

```bash
codacy-analysis analyze --install-dependencies --output-format json --output .codacy/tmp/codacy-baseline.json
```

Always use `--output <file>` to avoid broken JSON from stdout buffering.

**Record broad-config metrics** (used for tuning decisions in Step 4, not for the summary):
```bash
# Total issues in broad config
jq '.issues | length' .codacy/tmp/codacy-baseline.json

# Total runtime in milliseconds
jq '.metadata.durationMs' .codacy/tmp/codacy-baseline.json
```

**Parse the issue distribution** — this is the basis for all tuning decisions:

```bash
# Issues grouped by pattern (noise detection)
jq '[.issues | group_by(.patternId) | .[] | {patternId: .[0].patternId, toolId: .[0].toolId, severity: .[0].severity, category: .[0].category, count: length}] | sort_by(-.count)' .codacy/tmp/codacy-baseline.json

# Issues by severity
jq '.issues | group_by(.severity) | map({severity: .[0].severity, count: length})' .codacy/tmp/codacy-baseline.json

# Issues by category
jq '.issues | group_by(.category) | map({category: .[0].category, count: length})' .codacy/tmp/codacy-baseline.json

# Top 20 files by issue count
jq '[.issues | group_by(.filePath) | .[] | {filePath: .[0].filePath, count: length}] | sort_by(-.count) | .[0:20]' .codacy/tmp/codacy-baseline.json

# Per-tool results
jq '.toolResults | map({toolId, status, issueCount, durationMs, filesAnalyzed})' .codacy/tmp/codacy-baseline.json
```

### Step 4: Smart noise evaluation and tuning

This is the core of the skill. Work through the baseline results using a structured, context-aware decision framework.

When `.codacy/tmp/codacy-cloud-overview.json` exists (Track A), use its per-pattern issue counts and false-positive data as a cross-reference: the Cloud overview reflects the full repository, the local baseline only what the local CLI can analyze.

**4a. Establish the noise floor.** The Critical+High share of total issues sets how aggressively to cut low-priority categories:

| Critical+High % of total | Action on CodeStyle & Documentation patterns |
|---|---|
| **>50%** | Disable ALL CodeStyle and Documentation patterns at Minor and Warning severity. |
| **30–50%** | Disable Minor-severity CodeStyle and Documentation patterns. Keep Warning-level ones. |
| **10–30%** | Keep all patterns, but focus file exclusions on noisy paths. |
| **<10%** | Keep everything — the codebase is clean enough to benefit from style enforcement. |

The same logic applies proportionally to other low-priority categories (Comprehensibility, Compatibility) when critical issues dominate.

**4b. Pattern-level decisions.** For each pattern, highest issue count first, apply this priority chain: keep every security concern covered (deduplicate across tools, never remove the last pattern covering a concern); never disable valid Critical/High findings; disable patterns for languages or frameworks the project does not use; then apply the noise floor. **4c** evaluates file-level exclusions, **4d** whole-tool changes, **4e** recovers patterns lost by a tool-level disable, and **4f** validates the tuned config with a second analysis run.

The full rule chain, the file and tool evaluation criteria, and the validation pass are in [references/noise-tuning.md](references/noise-tuning.md).
### Step 5: Show local results

#### 5a. Generate summary JSON

Write `.codacy/configure-codacy-summary.json` with the before/after metrics, a detailed change log, and supporting context. Runtime values are in milliseconds (raw `durationMs` from the CLI).

**The `before` values come from Step 0** (the baseline captured before any changes — Cloud config for Track A, local config for Track B, or unconfigured for Track C). **The `after` values come from Step 4f** (the final tuned configuration). If no initial config existed (Track C), `before` values reflect an unconfigured state — use `null` for metrics that cannot be measured (e.g., runtime) and `0` for counts (patterns, tools, issues).

```json
{
  "summary": {
    "enabledPatterns": { "before": 1000, "after": 300 },
    "enabledTools": { "before": 34, "after": 15 },
    "issues": { "before": 7000, "after": 550 },
    "analysisRuntime": { "before": 240000, "after": 60000 }
  },
  "toolChanges": [
    {
      "toolId": "Biome",
      "action": "disabled",
      "reason": "Project uses ESLint9 with local config; Biome is redundant and produced 16K false positives in TypeScript",
      "patternsAffected": 232
    },
    {
      "toolId": "markdownlint",
      "action": "enabled",
      "reason": "New tool added for Markdown linting; README.md excluded per-tool to avoid inline HTML noise",
      "patternsAffected": 43
    }
  ],
  "patternChanges": [
    {
      "patternId": "Semgrep_python.lang.security.audit.xss.template-injection",
      "toolId": "Semgrep",
      "action": "disabled",
      "reason": "Wrong stack — pattern for Python; project is JavaScript-only",
      "delta": -45,
      "parameters": []
    },
    {
      "patternId": "Lizard_ccn-medium",
      "toolId": "Lizard",
      "action": "updated",
      "reason": "Raised threshold from 10 to 20 to match codebase complexity profile",
      "delta": -120,
      "parameters": [
        { "id": "threshold", "before": "10", "after": "20" }
      ]
    },
    {
      "patternId": "Semgrep_codacy.javascript.security.hard-coded-password",
      "toolId": "Semgrep",
      "action": "enabled",
      "reason": "New High Security pattern — detects hardcoded passwords; found 101 issues across 41 files for review",
      "delta": 101,
      "parameters": []
    }
  ],
  "fileExclusions": {
    "global": ["src/routeTree.gen.ts"],
    "perTool": {
      "markdownlint": ["README.md"]
    }
  },
  "securityCoverage": {
    "deduplication": [
      "Semgrep slack-webhook-url disabled — same concern covered by Semgrep detected-slack-webhook"
    ],
    "newCoverage": [
      "unsafe-dynamic-method (Critical, 5 issues)",
      "open-redirect-from-function (Critical, 4 issues)",
      "hard-coded-password (High, 101 issues)"
    ],
    "noisyButKept": [
      "Semgrep hard-coded-password (101 issues) — kept per security guardrail, file exclusions preferred over disabling"
    ]
  },
  "keyImprovements": [
    "Lizard complexity thresholds tuned to match mature React SPA — 576 fewer noise issues while keeping genuinely complex functions flagged",
    "6 new security patterns detecting open redirects, unsafe dynamic methods, hardcoded passwords, and React href injection (119 new findings)",
    "markdownlint added for Markdown quality",
    "Checkov expanded from 6 to 1358 IaC security patterns"
  ],
  "localConfigTools": [
    {
      "toolId": "ESLint9",
      "configFile": "eslint.config.js",
      "issueCount": 42,
      "note": "Issues from this tool are controlled by the project's ESLint config, not Codacy patterns. Edit eslint.config.js to reduce noise."
    }
  ],
  "codacyYaml": null,
  "importResults": null,
  "cloudVerification": null
}
```

**Field reference:**

**`summary`** — before/after metrics (same as before).

**`toolChanges`** — one entry per tool added or removed. Each entry:
- `toolId` — the tool identifier
- `action` — `"enabled"` (tool was added) or `"disabled"` (tool was removed)
- `reason` — why the tool was added or removed
- `patternsAffected` — number of patterns in the tool that was added/removed

**`patternChanges`** — one entry per individual pattern change. Each entry:
- `patternId` — the pattern identifier
- `toolId` — which tool this pattern belongs to
- `action` — `"enabled"`, `"disabled"`, or `"updated"`
- `reason` — why this change was made (auditability)
- `delta` — the change in issue count for this pattern, comparing INITIAL state (Step 0) vs FINAL tuned state (Step 4f). If the pattern was not enabled in the initial config, the delta is the full issue count from the tuned analysis (positive if newly enabled, 0 if disabled during tuning).
- `parameters` — parameter changes (empty array if not applicable)

Do NOT include individual pattern entries inside `patternChanges` for patterns that were added/removed as part of a whole tool change. Those are covered by the `toolChanges` entry. `patternChanges` is for surgical, individual pattern changes within a tool that remains enabled.

Do NOT include `"restored"` patterns here. Restoration (Step 4e) is an internal mechanism to ensure patterns from the baseline config that had Critical/High/Security results aren't lost by `init --auto`. It preserves what was already there — it's not a user-facing change.

**`fileExclusions`** — only lists **new** exclusions added during this tuning run. Exclusions already present in the baseline config are preserved automatically but not listed here (they are not changes).
- `global` — new global exclusion globs added during tuning
- `perTool.<toolId>` — new per-tool exclusion globs added during tuning

Omit `fileExclusions` entirely if no new exclusions were added.

**`securityCoverage`** — documents how security concerns are handled:
- `deduplication` — security patterns that were disabled because another pattern covers the same concern (list the kept pattern and the disabled one)
- `newCoverage` — new security patterns enabled that were NOT in the baseline config, with issue count
- `noisyButKept` — security patterns that are noisy but were kept active per the security guardrail

**`keyImprovements`** — array of 3–6 human-readable sentences summarizing the most impactful improvements. Focus on what changed and the quantitative impact. These should be suitable for presenting to the user as a summary.

**`localConfigTools`** — array of tools that have `useLocalConfigurationFile: true`. Each entry includes:
- `toolId` — the tool identifier
- `configFile` — path to the project's config file used by this tool
- `issueCount` — number of issues this tool produced in the tuned analysis
- `note` — human-readable explanation that noise from this tool is governed by the project's own config, not Codacy patterns

Empty array if no tools use local configuration files.

**`codacyYaml`** — string containing the generated `.codacy.yaml` file content if file exclusions were added (see Step 5c). `null` if no `.codacy.yaml` was generated.

**`importResults`** — `null` until Step 6 runs. If Cloud import is performed, this is populated with the results (see Step 6f for the schema).

**`cloudVerification`** — `null` unless Step 6e runs. Contains post-import Cloud verification results: whether reanalysis completed, issue counts before/after, patterns disabled on Cloud, and warnings. See Step 6e for the schema.

#### 5b. Present local results

Display a clear before/after summary to the user:

1. **Metrics table** — enabled patterns, enabled tools, total issues, and runtime (before vs after, with reduction %). The "before" is the baseline from Step 0. If no prior config existed (Track C), note "No prior configuration" in the before column.
2. **Tool changes** — tools added or removed, with reasons
3. **Top pattern changes by impact** — the `patternChanges` entries with the largest `|delta|`, sorted descending
4. **New security coverage** — new security patterns that were not in the baseline config
5. **Security deduplication** — any security patterns that were deduplicated (which was kept, which was disabled)
6. **Key improvements** — the `keyImprovements` array from the summary JSON, presented as a bulleted list
7. **Warnings** — failed tools, Semgrep parsing errors, tools with 0 files matched, any other issues encountered
8. **Local config file limitations** — for tools with `useLocalConfigurationFile: true`, note that their issues are controlled by the project's own config file (e.g., `.eslintrc`, `.rubocop.yml`), not by Codacy's managed patterns. Noise from these tools can only be reduced by editing the project's own config. List each such tool and the number of issues it produced, so the user knows where to look if they want further noise reduction.

#### 5c. Generate `.codacy.yaml` for Cloud file exclusions

File exclusions in `.codacy/codacy.config.json` only apply to local analysis. Codacy Cloud does not support importing file exclusions via the API or CLI. To apply file exclusions on Codacy Cloud, they must be defined in a `.codacy.yaml` file committed to the repository root (see [Codacy configuration file](https://docs.codacy.com/repositories-configure/codacy-configuration-file/)).

**If any new file exclusions were added during tuning** (global or per-tool), generate a `.codacy.yaml` file:

1. Read the existing `.codacy.yaml` from the repo root (if it exists) to preserve any existing configuration
2. Merge the new exclusions with any existing `exclude_paths` and per-engine `exclude_paths`
3. Write the updated `.codacy.yaml` to the repo root

The `.codacy.yaml` format uses Java glob syntax:
```yaml
---
exclude_paths:
  - "src/routeTree.gen.ts"
  - "vendor/**"
engines:
  markdownlint:
    exclude_paths:
      - "CHANGELOG.md"
  stylelint:
    exclude_paths:
      - "assets/vendor/**"
```

**Engine name mapping:** The engine names in `.codacy.yaml` may differ from the tool IDs in `.codacy/codacy.config.json`. Use the tool ID in lowercase as the engine name (e.g., `ESLint9` → `eslint9`, `Semgrep` → `semgrep`, `markdownlint` → `markdownlint`). If unsure, the tool ID in lowercase is the safe default.

After writing the file:

1. Store the full content of the generated `.codacy.yaml` as a string in the `codacyYaml` field of the summary JSON
2. Present to the user:
   - Note that file exclusions cannot be imported to Codacy Cloud via API
   - The `.codacy.yaml` file has been created/updated in the repo root
   - The user should commit and push this file to apply the exclusions on Codacy Cloud
   - Codacy Cloud always reads `.codacy.yaml` from the default branch

If no new file exclusions were added, skip this step and leave `codacyYaml` as `null`.

### Step 6: Cloud import (conditional)

If the repo is not on Codacy Cloud (Step 0), skip this step and note that the configuration is ready for local analysis.

Import behavior follows the invocation mode: auto-import proceeds directly, interactive asks the user first, and local-only has already stopped. Before importing, evaluate cloud-only tool noise with the same framework as Step 4 (`.codacy/tmp/codacy-remote-cloud-results.json`). Then import with `codacy tools --import .codacy/codacy.config.json -y`. On Coding Standard conflicts (409), retry with `--force` only in force mode; otherwise report the blocking standards and ask the user. Afterwards verify the imported state on Cloud and present the import results.

The per-phase commands, the conflict handling, and the `importResults` field reference are in [references/cloud-import.md](references/cloud-import.md).
### Step 7: Clean-up

Remove the temporary directory and all intermediate files:

```bash
rm -rf .codacy/tmp
```

## Per-tool tuning tips

Tool-specific judgment calls for Semgrep, Lizard, ESLint9, markdownlint, and Stylelint are in [references/per-tool-tuning.md](references/per-tool-tuning.md).
## Security guardrail

> **Every security concern must be covered by at least one active pattern.** This is a hard rule with no exceptions.

The guardrail operates at the **concern level**, not the individual pattern level. If two tools both detect "hardcoded secrets," disabling the less precise one is acceptable because the concern remains covered. However, if a security concern (e.g., SQL injection, XSS, path traversal, hardcoded secrets) would lose ALL active pattern coverage, the disable must be reverted.

When a security pattern is noisy and cannot be deduplicated:
- **Exclude specific files** where it triggers false positives (e.g., test fixtures, mock data)
- **Leave the false positives** for the user to triage in Codacy Cloud (they can ignore individual instances with a reason)
- **Never remove the last pattern covering a security concern** — it must stay active to catch real vulnerabilities in future code

For Critical/High severity patterns in non-Security categories, apply the same caution: prefer file exclusion over pattern disabling. Only disable these if they are clearly for the wrong stack (e.g., a Java Critical pattern in a pure Python project).

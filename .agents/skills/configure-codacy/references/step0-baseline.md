# Step 0 baseline detail

> Moved verbatim from `configure-codacy` `SKILL.md` (vendored from `codacy/codacy-skills` at `caa5d6f7f1f7261e469b2de7951707c553b23ee4`) so the skill body stays within the structure gate's progressive-disclosure limit.

## Step 0: Check environment and capture baseline

This step determines the starting point and captures the BEFORE metrics for the final summary. The flow depends on two conditions: whether the repo is on Codacy Cloud, and whether a local config already exists.

### 0a. Create temp directory and check Codacy Cloud status

Create the temporary directory for intermediate analysis files:
```bash
mkdir -p .codacy/tmp
```

The Cloud CLI auto-detects provider, organization, and repository from the git remote origin URL — no need to derive these manually. All `codacy` commands below can be run without explicit `<provider> <org> <repo>` when inside the repo. For clarity, the examples use the explicit form; replace with auto-detected short form in practice.

**CLI output caveat:** Both `codacy` and `codacy-analysis` CLIs write progress lines (e.g., `- Fetching tools...`, `- Fetching issues...`) to stderr before the actual JSON output. When piping CLI output directly to `jq`, redirect stderr to avoid parse errors:

```bash
codacy repository --output json 2>/dev/null | jq '...'
```

Check Cloud status:
```bash
codacy repository --output json
```

If this succeeds, the repo is on Codacy Cloud. Also list enabled tools to identify cloud-only tools later:
```bash
codacy tools --output json 2>/dev/null | jq '[.[] | select(.settings.isEnabled == true) | {name, isClientSide}]'
```

If it fails (repo not on Codacy, no auth, or no Cloud CLI), note that cloud features will be skipped — the local workflow is fully self-contained. Set the invocation mode to local-only (see "Invocation modes" section).

### 0b. Capture baseline (BEFORE reference)

Branch based on Cloud status and local config existence. The baseline captured here is the true BEFORE reference for the summary in Step 5.

**Track A — Repo is on Codacy Cloud (authoritative baseline):**

The Cloud configuration is the authoritative source. Use `init --remote` to fetch it.

**Important:** `init --remote` will fail if `.codacy/codacy.config.json` already exists. Always delete it first (the backup copy step happens before the delete):

```bash
# Delete any existing config first — init --remote refuses to overwrite
rm -f .codacy/codacy.config.json

# Fetch the current Cloud configuration
codacy-analysis init --remote <provider> <org> <repo>

# Store a copy for later merging and comparison
cp .codacy/codacy.config.json .codacy/tmp/codacy-remote-config.json

# Record BEFORE metrics
jq '[.tools[].patterns | length] | add' .codacy/codacy.config.json
jq '.tools | length' .codacy/codacy.config.json

# Run analysis with the Cloud config to capture the BEFORE issue landscape
codacy-analysis analyze --install-dependencies --output-format json --output .codacy/tmp/codacy-remote-results.json

# Record BEFORE issue count and runtime
jq '.issues | length' .codacy/tmp/codacy-remote-results.json
jq '.metadata.durationMs' .codacy/tmp/codacy-remote-results.json
```

Fetch the Cloud issue overview — this gives per-pattern issue counts, false positive counts, and suggested actions across the entire repository without downloading every individual issue:
```bash
codacy issues -O -o json > .codacy/tmp/codacy-cloud-overview.json
```
This overview data is valuable for tuning decisions in Step 4: it shows which patterns produce the most issues on Cloud (the production truth), includes false positive rates per pattern, and generates suggested actions to reduce noise (identifying patterns accounting for 10%+ of all issues or 3x the average). The suggested actions include ready-to-run disable commands, adapted for coding standard or config-file constraints. Save it for later comparison.

Also check for cloud-only tools — tools enabled in Cloud but not available in the local Analysis CLI. Get the local CLI's supported tools via `codacy-analysis info`, then compare against the Cloud-enabled tools list above. Any Cloud-enabled tool not in the `info` output is cloud-only (e.g., SonarSharp, Codacy ScalaMeta Pro). If cloud-only tools exist and have issues, fetch them:
```bash
codacy issues --output json > .codacy/tmp/codacy-remote-cloud-results.json
```

**Track B — NOT on Cloud, but local `.codacy/codacy.config.json` exists:**

```bash
# Store a copy for later merging and comparison
cp .codacy/codacy.config.json .codacy/tmp/codacy-previous-config.json

# Record BEFORE metrics
jq '[.tools[].patterns | length] | add' .codacy/codacy.config.json
jq '.tools | length' .codacy/codacy.config.json

# Run analysis with the existing local config
codacy-analysis analyze --install-dependencies --output-format json --output .codacy/tmp/codacy-previous-results.json

# Record BEFORE issue count and runtime
jq '.issues | length' .codacy/tmp/codacy-previous-results.json
jq '.metadata.durationMs' .codacy/tmp/codacy-previous-results.json
```

**Track C — No Cloud, no local config:**

Record BEFORE as unconfigured: 0 patterns, 0 tools, 0 issues, `null` runtime. No analysis to run.

Save the BEFORE metrics from whichever track ran. The broad config metrics from Steps 2-3 are internal only — they are NOT reported to the user.

### 0c. Cloud noise pre-evaluation (Track A only)

If the Cloud overview was fetched (`.codacy/tmp/codacy-cloud-overview.json` exists), analyze it now to identify patterns that are already known to be noisy based on Cloud production data. This saves time by pre-disabling known noise before running local analysis.

Start by checking the overview's **suggested actions** — the CLI already identifies patterns accounting for 10%+ of all issues or 3x the average per-pattern count and provides ready-to-run disable commands. Use these suggestions as the primary signal, then supplement with the criteria below.

Parse the overview's `patterns` array (each entry has `{id, title, total}`) and false positive counts:

```bash
# Issue counts per pattern
jq '[.overview.patterns[] | {id, title, total}] | sort_by(-.total)' .codacy/tmp/codacy-cloud-overview.json

# Patterns with potential false positives (count > 0)
jq '[.overview.patterns[] | select(.potentialFalsePositives > 0) | {id, title, total, potentialFalsePositives, falsePositiveRatio: ((.potentialFalsePositives / .total) * 100 | round)}] | sort_by(-.falsePositiveRatio)' .codacy/tmp/codacy-cloud-overview.json
```

**Identify noisy patterns using these criteria:**

1. **Wrong-language patterns** — cross-reference pattern IDs against the discovered stack (Step 1 runs next, but the Cloud tools list from Step 0a gives a rough language picture). Pattern IDs often contain the target language (e.g., `python.`, `java.`, `ruby.`). Mark patterns for languages clearly not in the project.

2. **Convention/style noise** — patterns with very high issue counts (top 5% by count) in categories like CodeStyle, Documentation, or Comprehensibility are likely convention mismatches. Mark these as candidates.

3. **High false-positive ratio patterns** — patterns where a significant proportion of issues are flagged as potential false positives (e.g., `potentialFalsePositives / total > 30%`) are strong candidates for pre-disabling. These patterns are producing results that Codacy's own heuristics consider unreliable. Store the false positive data for use in Step 4b rule 7, where patterns with high false-positive ratios get additional scrutiny during local verification.

4. **Never pre-disable Security patterns** — security patterns are never marked for pre-disabling regardless of count or false-positive ratio. They are handled in Step 4 with full context.

5. **Never pre-disable Critical/High severity patterns** — these require local verification before any action.

**Classify each noisy pattern into two lists:**

- **`cloudNoiseLocal`** — noisy patterns from tools supported by the local Analysis CLI. These will be removed from `.codacy/codacy.config.json` immediately after `init --auto` in Step 2.
- **`cloudNoiseCloudOnly`** — noisy patterns from cloud-only tools (identified in Step 0b). These will be disabled via Cloud CLI during Step 6 (import), only if the import is performed.

Store both lists for use in Steps 2 and 6. This is a pre-filter — Step 4 will perform deeper noise evaluation on the local analysis results.


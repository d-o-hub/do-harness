# Noise evaluation detail

> Moved verbatim from `configure-codacy` `SKILL.md` (vendored from `codacy/codacy-skills` at `caa5d6f7f1f7261e469b2de7951707c553b23ee4`) so the skill body stays within the structure gate's progressive-disclosure limit.

## Step 4: Smart noise evaluation and tuning

This is the core of the skill. Work through the baseline results using a structured, context-aware decision framework.

**Cloud overview as a cross-reference (Track A only):** If `.codacy/tmp/codacy-cloud-overview.json` exists (fetched in Step 0b), use its per-pattern issue counts and false positive data as a cross-reference against the local baseline. The cloud overview reflects the full repository analysis on Codacy (production truth), while the local baseline only covers files the local CLI can analyze. When a pattern shows high counts in the cloud overview but low counts locally, it may indicate cloud-only tool coverage — factor this into deduplication and disable decisions (don't disable a pattern that is still needed on Cloud). The false positive counts from the overview are especially valuable for rule 7 below — patterns with high false-positive ratios on Cloud are likely producing the same unreliable results locally.

### 4a. Establish the noise floor

Calculate the percentage of Critical+High issues (severity `"Error"` or `"High"`) relative to total issues. This determines how aggressively to cut lower-priority categories:

| Critical+High % of total | Action on CodeStyle & Documentation patterns |
|---|---|
| **>50%** | Disable ALL CodeStyle and Documentation patterns at Minor and Warning severity. The codebase has serious problems — style issues are just noise obscuring them. |
| **30–50%** | Disable Minor-severity CodeStyle and Documentation patterns. Keep Warning-level ones. |
| **10–30%** | Keep all patterns, but focus file exclusions on noisy paths. |
| **<10%** | Keep everything — the codebase is clean enough to benefit from style enforcement. |

The same logic applies proportionally to other low-priority categories (Comprehensibility, Compatibility) when critical issues dominate. Use judgment.

### 4b. Pattern-level decisions

For each pattern in the baseline results, sorted by issue count (highest first), apply this priority chain:

1. **Security patterns must cover every security concern.** Any pattern with `category == "Security"` stays enabled UNLESS another active pattern from a different tool already covers the same semantic concern (see rule 2). If a security pattern is noisy and no deduplication applies, exclude specific files instead of disabling the pattern.

2. **Cross-tool deduplication of overlapping patterns.** When multiple tools flag the same semantic concern (e.g., "hardcoded secrets", "SQL injection", "XSS", "unused imports"), identify the overlap by comparing issue messages, affected files, and affected lines. For the overlapping pair:
   - Keep the pattern from the more specialized or precise tool (e.g., Semgrep security rules over Biome generic checks; a dedicated SAST scanner over a general linter)
   - If precision is comparable, keep the pattern from the tool that is more actively used in the project (more patterns enabled, used in CI, has a local config file)
   - Disable the redundant pattern from the other tool
   - This is NOT removing a concern — it is deduplicating. The concern remains covered by the kept pattern.
   - This applies to ALL categories, not just Security — e.g., two tools both flagging "unused imports"
   - Document in the change log: which pattern was kept, which was disabled, and why the kept pattern is the better source

3. **NEVER disable valid Critical/High issues.** Patterns with `severity == "Error"` or `severity == "High"` that are finding real problems must stay. If they appear to be false positives, exclude the offending files rather than disabling the pattern.

4. **Wrong stack → disable.** Cross-reference with the discover output from Step 1. Patterns for languages or frameworks not present in the project are pure noise. Examples:
   - Python security patterns in a JavaScript-only project
   - Apex patterns from PMD7 in a Java-only project
   - Semgrep rules for languages not in the repo
   Remove these patterns from the config.

5. **Noise floor → disable.** Apply the decisions from 4a. If the noise floor says CodeStyle/Documentation at Minor severity should go, remove those patterns.

6. **Convention mismatch → disable.** If a pattern flags something that >80% of the codebase does consistently, the pattern contradicts the project's established conventions. Examples:
   - Tabs-vs-spaces rules when the entire project uses the "wrong" style consistently
   - Naming convention rules that don't match the project's established naming

7. **False-positive prone → disable.** Use the Cloud overview's false positive data (from Step 0c) as the primary signal: patterns where `potentialFalsePositives / total > 30%` are strong disable candidates — Codacy's own heuristics are flagging a significant share of their results as unreliable. For these patterns, review a sample of actual instances in the code to confirm. If the local instances confirm the pattern is producing low-value results for this codebase, disable it. For patterns without Cloud false-positive data, check if the pattern is known to produce high false-positive rates in general. Only disable after verifying the hits are genuinely not useful for this codebase.

8. **Parameter tuning over disabling.** Before disabling a valuable pattern, check if it has configurable parameters:
   - Lizard complexity thresholds — raise to match the codebase's actual complexity profile
   - Line length limits — set to the project's observed maximum
   - Other threshold-based rules — adjust to reduce false hits while keeping the rule active
   Tuning preserves coverage while reducing noise.

9. **File exclusion over disabling.** When a pattern is valid but fires on files where it doesn't apply, exclude the files rather than disabling the pattern. The pattern stays active for real source code.

### 4c. File-level evaluation

Review the top files by issue count from the baseline results (Step 3). Exclusions must be **strictly data-driven** — only add exclusions for files/paths that actually appear in the baseline results and are producing meaningful noise.

**Important:** The analysis CLI already respects `.gitignore`. Files matched by `.gitignore` are never analyzed, so they do NOT need exclusion in the Codacy config. Do NOT preemptively add exclusions for paths like `dist/**`, `node_modules/**`, `build/**`, or `coverage/**` unless they are actually appearing in the results with issues.

**Process for each noisy file/path in the top-N by issue count:**

1. Check if the file represents generated code (e.g., `*.generated.ts`, `routeTree.gen.ts`, auto-generated API clients, `.pb.go` files)
2. Check if the file is vendored or third-party code committed to the repo (e.g., `.yarn/releases/`, `vendor/` that is not gitignored)
3. Check if the file is build output that was committed (not gitignored)
4. Check if the file is a test fixture, snapshot, or mock data that produces false positives from a specific tool
5. If any of the above apply, add to the appropriate exclusion:
   - Generated/vendored/build output that affect all tools: add to the global `exclude` array
   - Files noisy for a specific tool only: add to that tool's per-tool `exclude` array (e.g., markdownlint hitting `CHANGELOG.md`, Stylelint hitting committed vendor CSS)
6. If the file is legitimate source code, do NOT exclude it — address the noise through pattern-level tuning (rules 1-9 above) instead

Every exclusion must be justified by actual results. Do NOT maintain a prescriptive checklist of "always exclude these paths."

### 4d. Tool-level evaluation

For each tool in the config:

- **Irrelevant to the stack?** If the discover output shows no files for a tool's target language, remove the entire tool entry. Examples: RuboCop in a project with no Ruby, Checkstyle in a project with no Java.
- **Failed to run?** Check `toolResults[].status`. If `"failed"`, check `errors[]` for the reason. Keep the tool in the config (it may work after fixing the root cause) but note it in the summary.
- **Zero issues?** A tool with zero issues is not noise — it's either confirming code quality or not finding its target files. Leave it enabled unless it's clearly irrelevant to the stack.

### 4e. Lost patterns recovery

Compare the baseline analysis results (from Step 0) against the current `.codacy/codacy.config.json` after tuning. For each pattern that:
- Found issues in the baseline analysis (`.codacy/tmp/codacy-remote-results.json` for Track A, `.codacy/tmp/codacy-previous-results.json` for Track B)
- Has `category == "Security"` OR `severity == "Error"` OR `severity == "High"`
- Is NOT present in the current config (was excluded by `init --auto` or removed during tuning)

→ Add it back to the config under the appropriate tool. This is an internal preservation step — restored patterns are NOT reported as changes in the summary (they were already in the baseline config, so from the user's perspective nothing changed).

Skip this step for Track C (no prior config — nothing to recover).

### Apply all changes

Edit `.codacy/codacy.config.json` with all decisions from 4a–4e. Track every change for the summary in Step 5:
- For each disabled pattern: record the patternId, action `"disabled"`, and reason
- For each tuned pattern: record the patternId, action `"updated"`, old/new parameters, and reason
- For each removed tool: record it in `toolChanges`
- For each added tool: record it in `toolChanges`
- For each new file exclusion added: record it in `fileExclusions`
- Restored patterns (Step 4e) are NOT included in the summary — they preserve the status quo

**Security guardrail — mandatory check before saving.** Before writing the updated config, verify that every **security concern** (e.g., hardcoded secrets, SQL injection, XSS, path traversal, open redirects) is still covered by at least one active pattern with `category == "Security"`. A security pattern may be disabled only if another active pattern from a different tool covers the same concern (cross-tool deduplication per rule 2 in Step 4b). If any concern has lost all coverage, re-enable the most precise pattern for that concern.

### 4f. Validation pass

Run analysis with the tuned config to validate the improvement:

```bash
codacy-analysis analyze --install-dependencies --output-format json --output .codacy/tmp/codacy-tuned.json
```

**Record AFTER metrics:**
```bash
# Enabled patterns after tuning
jq '[.tools[].patterns | length] | add' .codacy/codacy.config.json

# Enabled tools after tuning
jq '.tools | length' .codacy/codacy.config.json

# Total issues after tuning
jq '.issues | length' .codacy/tmp/codacy-tuned.json

# Runtime after tuning
jq '.metadata.durationMs' .codacy/tmp/codacy-tuned.json
```

**Validate:**
- Issues should have decreased meaningfully vs the broad-config baseline (Step 3).
- If issues increased or didn't decrease meaningfully (<20% reduction), review the tuning decisions and re-iterate once: go back to "Do AI analysis of the results" (Step 4b) with the updated results, apply further changes, and re-run this validation.
- The goal is that every remaining issue is worth looking at.


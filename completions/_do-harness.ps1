
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'do-harness' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'do-harness'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'do-harness' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('version', 'version', [CompletionResultType]::ParameterValue, 'Print CLI version information')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'Run computational sensors')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Run computational sensors')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List sensor names')
            [CompletionResult]::new('ls', 'ls', [CompletionResultType]::ParameterValue, 'List sensor names')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'Explain which sensors the current change selects, without running them')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Report verification evidence freshness without running sensors')
            [CompletionResult]::new('pr', 'pr', [CompletionResultType]::ParameterValue, 'Deterministic PR analysis (read-only; works in any git repository)')
            [CompletionResult]::new('skills', 'skills', [CompletionResultType]::ParameterValue, 'Inspect and select skills by progressive disclosure')
            [CompletionResult]::new('init-db', 'init-db', [CompletionResultType]::ParameterValue, 'Apply pending database migrations')
            [CompletionResult]::new('seed', 'seed', [CompletionResultType]::ParameterValue, 'Seed invariants from plans/invariants.json')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Scaffold a harness workspace in a target directory')
            [CompletionResult]::new('task', 'task', [CompletionResultType]::ParameterValue, 'Inspect and export task state')
            [CompletionResult]::new('trace', 'trace', [CompletionResultType]::ParameterValue, 'Record and list interaction traces')
            [CompletionResult]::new('distill', 'distill', [CompletionResultType]::ParameterValue, 'Extract a heuristic from a resolved trace into a skill. Review output against the anti-AI-slop checklist (.agents/skills/skill-creator/references/anti_ai_slop.md)')
            [CompletionResult]::new('errors', 'errors', [CompletionResultType]::ParameterValue, 'Inspect and clear fail-fast error signatures')
            [CompletionResult]::new('loc', 'loc', [CompletionResultType]::ParameterValue, 'Report line-of-code state for the 500-LOC invariant')
            [CompletionResult]::new('split', 'split', [CompletionResultType]::ParameterValue, 'Extract a large top-level item into a sibling module')
            [CompletionResult]::new('eval', 'eval', [CompletionResultType]::ParameterValue, 'Validate skill structure and benchmark skill evals')
            [CompletionResult]::new('hook', 'hook', [CompletionResultType]::ParameterValue, 'Manage git hooks that run `do-harness verify`')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Run diagnostic checks on binary resolution and git hook health')
            [CompletionResult]::new('metrics', 'metrics', [CompletionResultType]::ParameterValue, 'Report harness trends: sensor stats, strikes, eval pass-rate history')
            [CompletionResult]::new('overlap', 'overlap', [CompletionResultType]::ParameterValue, 'Rank skill pairs by guidance overlap (Tier-2 distinctiveness advisory)')
            [CompletionResult]::new('maintenance', 'maintenance', [CompletionResultType]::ParameterValue, 'Prune old beats and compact the state database')
            [CompletionResult]::new('compliance', 'compliance', [CompletionResultType]::ParameterValue, 'Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act')
            [CompletionResult]::new('audit-chain', 'audit-chain', [CompletionResultType]::ParameterValue, 'Recompute workflow event hash chain and report first divergence')
            [CompletionResult]::new('dora', 'dora', [CompletionResultType]::ParameterValue, 'Derive DORA deployment metrics from git history (deterministic, read-only)')
            [CompletionResult]::new('completions', 'completions', [CompletionResultType]::ParameterValue, 'Generate shell completions')
            [CompletionResult]::new('man', 'man', [CompletionResultType]::ParameterValue, 'Generate man page documentation')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;version' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;verify' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--set', '--set', [CompletionResultType]::ParameterName, 'Run only the sensors in this development signal set')
            [CompletionResult]::new('--only', '--only', [CompletionResultType]::ParameterName, 'Run only the named sensor (repeatable or comma-separated)')
            [CompletionResult]::new('--exclude', '--exclude', [CompletionResultType]::ParameterName, 'Exclude named sensors from the run')
            [CompletionResult]::new('--jobs', '--jobs', [CompletionResultType]::ParameterName, 'Maximum sensors in flight (overrides `jobs` in do-harness.toml)')
            [CompletionResult]::new('--task', '--task', [CompletionResultType]::ParameterName, 'Scope records and fail-fast strikes to this task id (or ''global'')')
            [CompletionResult]::new('--evidence', '--evidence', [CompletionResultType]::ParameterName, 'Write a machine-readable evidence artifact to path')
            [CompletionResult]::new('--approver', '--approver', [CompletionResultType]::ParameterName, 'Approver identity recorded with `--bless` (defaults to `DO_HARNESS_APPROVER` or the git user email)')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--fail-fast', '--fail-fast', [CompletionResultType]::ParameterName, 'Halt at the first failing sensor')
            [CompletionResult]::new('--changed', '--changed', [CompletionResultType]::ParameterName, 'Run only sensors applicable to the working-tree change')
            [CompletionResult]::new('--record', '--record', [CompletionResultType]::ParameterName, 'Persist beats and error signatures into the state database')
            [CompletionResult]::new('--global', '--global', [CompletionResultType]::ParameterName, 'Record unscoped beats in the global namespace')
            [CompletionResult]::new('--strict', '--strict', [CompletionResultType]::ParameterName, 'Enforce strong evidence: exit non-zero on skips or missing timing/exit codes')
            [CompletionResult]::new('--bless', '--bless', [CompletionResultType]::ParameterName, 'Lower or initialize blessed findings baselines from this run (requires --record; a bless never raises a baseline)')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;check' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--set', '--set', [CompletionResultType]::ParameterName, 'Run only the sensors in this development signal set')
            [CompletionResult]::new('--only', '--only', [CompletionResultType]::ParameterName, 'Run only the named sensor (repeatable or comma-separated)')
            [CompletionResult]::new('--exclude', '--exclude', [CompletionResultType]::ParameterName, 'Exclude named sensors from the run')
            [CompletionResult]::new('--jobs', '--jobs', [CompletionResultType]::ParameterName, 'Maximum sensors in flight (overrides `jobs` in do-harness.toml)')
            [CompletionResult]::new('--task', '--task', [CompletionResultType]::ParameterName, 'Scope records and fail-fast strikes to this task id (or ''global'')')
            [CompletionResult]::new('--evidence', '--evidence', [CompletionResultType]::ParameterName, 'Write a machine-readable evidence artifact to path')
            [CompletionResult]::new('--approver', '--approver', [CompletionResultType]::ParameterName, 'Approver identity recorded with `--bless` (defaults to `DO_HARNESS_APPROVER` or the git user email)')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--fail-fast', '--fail-fast', [CompletionResultType]::ParameterName, 'Halt at the first failing sensor')
            [CompletionResult]::new('--changed', '--changed', [CompletionResultType]::ParameterName, 'Run only sensors applicable to the working-tree change')
            [CompletionResult]::new('--record', '--record', [CompletionResultType]::ParameterName, 'Persist beats and error signatures into the state database')
            [CompletionResult]::new('--global', '--global', [CompletionResultType]::ParameterName, 'Record unscoped beats in the global namespace')
            [CompletionResult]::new('--strict', '--strict', [CompletionResultType]::ParameterName, 'Enforce strong evidence: exit non-zero on skips or missing timing/exit codes')
            [CompletionResult]::new('--bless', '--bless', [CompletionResultType]::ParameterName, 'Lower or initialize blessed findings baselines from this run (requires --record; a bless never raises a baseline)')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;list' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--sets', '--sets', [CompletionResultType]::ParameterName, 'List development signal-set names instead of sensors')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;ls' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--sets', '--sets', [CompletionResultType]::ParameterName, 'List development signal-set names instead of sensors')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;explain' {
            [CompletionResult]::new('--set', '--set', [CompletionResultType]::ParameterName, 'Explain this development signal set (default: all sensors)')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--changed', '--changed', [CompletionResultType]::ParameterName, 'Select by working-tree change instead of listing everything')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;status' {
            [CompletionResult]::new('--set', '--set', [CompletionResultType]::ParameterName, 'Check evidence for this development signal set')
            [CompletionResult]::new('--evidence', '--evidence', [CompletionResultType]::ParameterName, 'Read evidence from this file instead of the default artifact')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;pr' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('readiness', 'readiness', [CompletionResultType]::ParameterValue, 'Check merge readiness for a pull request')
            [CompletionResult]::new('no-effect', 'no-effect', [CompletionResultType]::ParameterValue, 'Report whether a PR or revision range introduces any effective change')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'Emit the semantic residual: changed units evidence could not prove')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;pr;readiness' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;pr;no-effect' {
            [CompletionResult]::new('--base', '--base', [CompletionResultType]::ParameterName, 'Base revision (local mode; requires --head)')
            [CompletionResult]::new('--head', '--head', [CompletionResultType]::ParameterName, 'Head revision (local mode; requires --base)')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;pr;review' {
            [CompletionResult]::new('--base', '--base', [CompletionResultType]::ParameterName, 'Base revision (local mode; requires --head)')
            [CompletionResult]::new('--head', '--head', [CompletionResultType]::ParameterName, 'Head revision (local mode; requires --base)')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--recompute', '--recompute', [CompletionResultType]::ParameterName, 'Ignore the cached report and recompute from scratch')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;pr;help' {
            [CompletionResult]::new('readiness', 'readiness', [CompletionResultType]::ParameterValue, 'Check merge readiness for a pull request')
            [CompletionResult]::new('no-effect', 'no-effect', [CompletionResultType]::ParameterValue, 'Report whether a PR or revision range introduces any effective change')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'Emit the semantic residual: changed units evidence could not prove')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;pr;help;readiness' {
            break
        }
        'do-harness;pr;help;no-effect' {
            break
        }
        'do-harness;pr;help;review' {
            break
        }
        'do-harness;pr;help;help' {
            break
        }
        'do-harness;skills' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('suggest', 'suggest', [CompletionResultType]::ParameterValue, 'Rank skills by relevance to a query using metadata only')
            [CompletionResult]::new('drift', 'drift', [CompletionResultType]::ParameterValue, 'Check manifest-managed shared skills against their pinned digests')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;skills;suggest' {
            [CompletionResult]::new('--query', '--query', [CompletionResultType]::ParameterName, 'Task description to match against skill metadata')
            [CompletionResult]::new('--limit', '--limit', [CompletionResultType]::ParameterName, 'Maximum number of candidates to return')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;skills;drift' {
            [CompletionResult]::new('--manifest', '--manifest', [CompletionResultType]::ParameterName, 'Manifest to read; defaults to `.agents/skills-manifest.toml`')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;skills;help' {
            [CompletionResult]::new('suggest', 'suggest', [CompletionResultType]::ParameterValue, 'Rank skills by relevance to a query using metadata only')
            [CompletionResult]::new('drift', 'drift', [CompletionResultType]::ParameterValue, 'Check manifest-managed shared skills against their pinned digests')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;skills;help;suggest' {
            break
        }
        'do-harness;skills;help;drift' {
            break
        }
        'do-harness;skills;help;help' {
            break
        }
        'do-harness;init-db' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--check', '--check', [CompletionResultType]::ParameterName, 'Report pending migrations and exit non-zero when any are pending')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Report pending migrations without applying them (exit 0)')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Skip the interactive confirmation prompt')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Skip the interactive confirmation prompt')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;seed' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--prune', '--prune', [CompletionResultType]::ParameterName, 'Delete invariants no longer present in plans/invariants.json')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;init' {
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Language pack to scaffold (default: detect from the repository)')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Overwrite existing files')
            [CompletionResult]::new('--no-seed', '--no-seed', [CompletionResultType]::ParameterName, 'Do not seed invariants')
            [CompletionResult]::new('--minimal', '--minimal', [CompletionResultType]::ParameterName, 'Minimal setup without extra skills')
            [CompletionResult]::new('--no-gitignore', '--no-gitignore', [CompletionResultType]::ParameterName, 'Skip creating/modifying .gitignore')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;task' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Write the task list to plans/tasks.json or specified output')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Validate plans/tasks.json against the state database')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Print tasks from the state database')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show details for a specific task')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add a task in `pending` state')
            [CompletionResult]::new('advance', 'advance', [CompletionResultType]::ParameterValue, 'Advance the task''s subtask pointer')
            [CompletionResult]::new('done', 'done', [CompletionResultType]::ParameterValue, 'Mark a task done once its sensor-gated subtasks have passed')
            [CompletionResult]::new('fail', 'fail', [CompletionResultType]::ParameterValue, 'Mark a task failed')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove or cancel a task')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;task;export' {
            [CompletionResult]::new('-o', '-o', [CompletionResultType]::ParameterName, 'Output file path or - for stdout')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Output file path or - for stdout')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format (json/text)')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--stdout', '--stdout', [CompletionResultType]::ParameterName, 'Write directly to stdout instead of file')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;task;import' {
            [CompletionResult]::new('--file', '--file', [CompletionResultType]::ParameterName, 'Snapshot file (defaults to plans/tasks.json under the root)')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--check', '--check', [CompletionResultType]::ParameterName, 'Exit non-zero when the snapshot and database drift')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;task;list' {
            [CompletionResult]::new('--status', '--status', [CompletionResultType]::ParameterName, 'Filter tasks by status (pending, `in_progress`, completed, failed)')
            [CompletionResult]::new('--method', '--method', [CompletionResultType]::ParameterName, 'Filter tasks by method name')
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Filter tasks by parent ID')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;task;show' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;task;add' {
            [CompletionResult]::new('--method', '--method', [CompletionResultType]::ParameterName, 'Name of HTN method defined in plans/methods.json')
            [CompletionResult]::new('--parent', '--parent', [CompletionResultType]::ParameterName, 'Parent task id')
            [CompletionResult]::new('--precondition', '--precondition', [CompletionResultType]::ParameterName, 'Recorded precondition guard')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;task;advance' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Perform dry run without state changes')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;task;done' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Perform dry run without state changes')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;task;fail' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;task;remove' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;task;help' {
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Write the task list to plans/tasks.json or specified output')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Validate plans/tasks.json against the state database')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Print tasks from the state database')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show details for a specific task')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add a task in `pending` state')
            [CompletionResult]::new('advance', 'advance', [CompletionResultType]::ParameterValue, 'Advance the task''s subtask pointer')
            [CompletionResult]::new('done', 'done', [CompletionResultType]::ParameterValue, 'Mark a task done once its sensor-gated subtasks have passed')
            [CompletionResult]::new('fail', 'fail', [CompletionResultType]::ParameterValue, 'Mark a task failed')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove or cancel a task')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;task;help;export' {
            break
        }
        'do-harness;task;help;import' {
            break
        }
        'do-harness;task;help;list' {
            break
        }
        'do-harness;task;help;show' {
            break
        }
        'do-harness;task;help;add' {
            break
        }
        'do-harness;task;help;advance' {
            break
        }
        'do-harness;task;help;done' {
            break
        }
        'do-harness;task;help;fail' {
            break
        }
        'do-harness;task;help;remove' {
            break
        }
        'do-harness;task;help;help' {
            break
        }
        'do-harness;trace' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Record a trace of an executed command and its resolution')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Print traces for a session')
            [CompletionResult]::new('sessions', 'sessions', [CompletionResultType]::ParameterValue, 'List distinct trace session identifiers')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;trace;add' {
            [CompletionResult]::new('--session', '--session', [CompletionResultType]::ParameterName, 'Session identifier grouping related traces')
            [CompletionResult]::new('--task', '--task', [CompletionResultType]::ParameterName, 'Owning task id')
            [CompletionResult]::new('--command', '--command', [CompletionResultType]::ParameterName, 'The command that was executed')
            [CompletionResult]::new('--error-diff', '--error-diff', [CompletionResultType]::ParameterName, 'Error diff or failure output captured')
            [CompletionResult]::new('--resolution-steps', '--resolution-steps', [CompletionResultType]::ParameterName, 'Steps taken to resolve the failure')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;trace;list' {
            [CompletionResult]::new('--session', '--session', [CompletionResultType]::ParameterName, 'Session identifier')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;trace;sessions' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;trace;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Record a trace of an executed command and its resolution')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Print traces for a session')
            [CompletionResult]::new('sessions', 'sessions', [CompletionResultType]::ParameterValue, 'List distinct trace session identifiers')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;trace;help;add' {
            break
        }
        'do-harness;trace;help;list' {
            break
        }
        'do-harness;trace;help;sessions' {
            break
        }
        'do-harness;trace;help;help' {
            break
        }
        'do-harness;distill' {
            [CompletionResult]::new('--skill', '--skill', [CompletionResultType]::ParameterName, 'Skill the heuristic belongs to')
            [CompletionResult]::new('--pattern', '--pattern', [CompletionResultType]::ParameterName, 'Generalized pattern to record')
            [CompletionResult]::new('--description', '--description', [CompletionResultType]::ParameterName, 'When the pattern applies')
            [CompletionResult]::new('--from-trace', '--from-trace', [CompletionResultType]::ParameterName, 'Source trace id; required as evidence of a resolved fix')
            [CompletionResult]::new('--min-strikes', '--min-strikes', [CompletionResultType]::ParameterName, 'Strike count at or above which a signature sculpts a scaffold (default: the fail-fast threshold)')
            [CompletionResult]::new('--task', '--task', [CompletionResultType]::ParameterName, 'Scope strike lookup to this task id')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--from-strikes', '--from-strikes', [CompletionResultType]::ParameterName, 'Generate a starter skill scaffold from recorded sensor strikes (AGENTS.md §6 steering loop) instead of distilling from a trace')
            [CompletionResult]::new('--to-fixture', '--to-fixture', [CompletionResultType]::ParameterName, 'Raise the skill''s pass-rate bar after this recovery (review ticks anti-AI-slop checklist first)')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Perform dry run without modifying skill files')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;errors' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List fail-fast error signatures (prefixed with `sensor:`)')
            [CompletionResult]::new('clear', 'clear', [CompletionResultType]::ParameterValue, 'Clear fail-fast error signatures (e.g. `sensor:<name>`)')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;errors;list' {
            [CompletionResult]::new('--task', '--task', [CompletionResultType]::ParameterName, 'Scope to one task id')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;errors;clear' {
            [CompletionResult]::new('--sensor', '--sensor', [CompletionResultType]::ParameterName, 'Only clear this signature key (e.g. `sensor:<name>`)')
            [CompletionResult]::new('--task', '--task', [CompletionResultType]::ParameterName, 'Only clear signatures for this task id')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Force clearing without prompt')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Perform dry run without clearing')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;errors;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List fail-fast error signatures (prefixed with `sensor:`)')
            [CompletionResult]::new('clear', 'clear', [CompletionResultType]::ParameterValue, 'Clear fail-fast error signatures (e.g. `sensor:<name>`)')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;errors;help;list' {
            break
        }
        'do-harness;errors;help;clear' {
            break
        }
        'do-harness;errors;help;help' {
            break
        }
        'do-harness;loc' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--warn', '--warn', [CompletionResultType]::ParameterName, 'Show only files at or above the 450-line decomposition threshold')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;split' {
            [CompletionResult]::new('--target', '--target', [CompletionResultType]::ParameterName, 'Name the sibling module instead of deriving it from the item')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Print the plan without writing files')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;eval' {
            [CompletionResult]::new('--skill', '--skill', [CompletionResultType]::ParameterName, 'Restrict evaluation to this skill directory name')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--approver', '--approver', [CompletionResultType]::ParameterName, 'Approver identity recorded with `--bless` (defaults to `DO_HARNESS_APPROVER` or the git user email)')
            [CompletionResult]::new('--agent-cmd', '--agent-cmd', [CompletionResultType]::ParameterName, 'Run this shell command once per eval case instead of the deterministic walkthrough (real Skill Lift). cwd is the sandbox root; the prompt is in `$DO_HARNESS_PROMPT` and on stdin; stdout is saved to `agent_stdout.txt` for assertions')
            [CompletionResult]::new('--agent-timeout', '--agent-timeout', [CompletionResultType]::ParameterName, 'Kill an agent run after this many seconds')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--bless', '--bless', [CompletionResultType]::ParameterName, 'Re-baseline graders and update pass-rate floor on green run')
            [CompletionResult]::new('--list-skills', '--list-skills', [CompletionResultType]::ParameterName, 'List available skills')
            [CompletionResult]::new('--fail-fast', '--fail-fast', [CompletionResultType]::ParameterName, 'Halt on first failing evaluation')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Perform dry-run evaluation')
            [CompletionResult]::new('--no-lift', '--no-lift', [CompletionResultType]::ParameterName, 'Skip the without-skill baseline run (no Skill Lift measured)')
            [CompletionResult]::new('--strict-fixtures', '--strict-fixtures', [CompletionResultType]::ParameterName, 'Fail skills whose fixture has dataset-quality gaps (thin cases, no negative out-of-scope case)')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;hook' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('install', 'install', [CompletionResultType]::ParameterValue, 'Write pre-commit and pre-push hooks into `.git/hooks/`')
            [CompletionResult]::new('uninstall', 'uninstall', [CompletionResultType]::ParameterValue, 'Remove managed hooks, leaving foreign hook files untouched')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show whether the managed hooks and release binary are present')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'Show diff between installed hooks and current templates')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;hook;install' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Overwrite foreign (unmanaged) hook files')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;hook;uninstall' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;hook;status' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;hook;diff' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;hook;help' {
            [CompletionResult]::new('install', 'install', [CompletionResultType]::ParameterValue, 'Write pre-commit and pre-push hooks into `.git/hooks/`')
            [CompletionResult]::new('uninstall', 'uninstall', [CompletionResultType]::ParameterValue, 'Remove managed hooks, leaving foreign hook files untouched')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show whether the managed hooks and release binary are present')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'Show diff between installed hooks and current templates')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;hook;help;install' {
            break
        }
        'do-harness;hook;help;uninstall' {
            break
        }
        'do-harness;hook;help;status' {
            break
        }
        'do-harness;hook;help;diff' {
            break
        }
        'do-harness;hook;help;help' {
            break
        }
        'do-harness;doctor' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--strict', '--strict', [CompletionResultType]::ParameterName, 'Strictly enforce warning checks as failures')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;metrics' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--sensor', '--sensor', [CompletionResultType]::ParameterName, 'Filter by sensor name')
            [CompletionResult]::new('--skill', '--skill', [CompletionResultType]::ParameterName, 'Filter by skill name')
            [CompletionResult]::new('--since', '--since', [CompletionResultType]::ParameterName, 'Filter metrics since a Unix timestamp in seconds')
            [CompletionResult]::new('--scope', '--scope', [CompletionResultType]::ParameterName, 'Filter by workstream scope (e.g. `branch:main`, `task:1`, `global`, or `all`). Defaults to the current git branch')
            [CompletionResult]::new('--task', '--task', [CompletionResultType]::ParameterName, 'Filter by task id (equivalent to `--scope task:<ID>`)')
            [CompletionResult]::new('--branch', '--branch', [CompletionResultType]::ParameterName, 'Filter by branch name (equivalent to `--scope branch:<NAME>`)')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--all', '--all', [CompletionResultType]::ParameterName, 'Aggregate across all workstreams and scopes')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;overlap' {
            [CompletionResult]::new('--threshold', '--threshold', [CompletionResultType]::ParameterName, 'Cosine similarity at or above which a pair prints as WARN')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;maintenance' {
            [CompletionResult]::new('--prune-beats', '--prune-beats', [CompletionResultType]::ParameterName, 'Delete beats older than this many days (keeps the most recent per task)')
            [CompletionResult]::new('--keep-per-task', '--keep-per-task', [CompletionResultType]::ParameterName, 'Minimum most-recent beats kept per task when pruning')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;compliance' {
            [CompletionResult]::new('--framework', '--framework', [CompletionResultType]::ParameterName, 'Filter framework (owasp-agentic-top10, nist-ai-rmf, eu-ai-act, soc2)')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;audit-chain' {
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;dora' {
            [CompletionResult]::new('--days', '--days', [CompletionResultType]::ParameterName, 'Rolling window in days (overrides the pinned policy for this run)')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--source', '--source', [CompletionResultType]::ParameterName, 'Where the numbers come from')
            [CompletionResult]::new('--now', '--now', [CompletionResultType]::ParameterName, 'Measurement clock as Unix seconds (default: system clock)')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--record', '--record', [CompletionResultType]::ParameterName, 'Persist the snapshot into the state database')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;completions' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;man' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Verbosity level (-v, -vv)')
            [CompletionResult]::new('-q', '-q', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--quiet', '--quiet', [CompletionResultType]::ParameterName, 'Suppress non-error messages')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Dry run without side effects')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
        'do-harness;help' {
            [CompletionResult]::new('version', 'version', [CompletionResultType]::ParameterValue, 'Print CLI version information')
            [CompletionResult]::new('verify', 'verify', [CompletionResultType]::ParameterValue, 'Run computational sensors')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List sensor names')
            [CompletionResult]::new('explain', 'explain', [CompletionResultType]::ParameterValue, 'Explain which sensors the current change selects, without running them')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Report verification evidence freshness without running sensors')
            [CompletionResult]::new('pr', 'pr', [CompletionResultType]::ParameterValue, 'Deterministic PR analysis (read-only; works in any git repository)')
            [CompletionResult]::new('skills', 'skills', [CompletionResultType]::ParameterValue, 'Inspect and select skills by progressive disclosure')
            [CompletionResult]::new('init-db', 'init-db', [CompletionResultType]::ParameterValue, 'Apply pending database migrations')
            [CompletionResult]::new('seed', 'seed', [CompletionResultType]::ParameterValue, 'Seed invariants from plans/invariants.json')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Scaffold a harness workspace in a target directory')
            [CompletionResult]::new('task', 'task', [CompletionResultType]::ParameterValue, 'Inspect and export task state')
            [CompletionResult]::new('trace', 'trace', [CompletionResultType]::ParameterValue, 'Record and list interaction traces')
            [CompletionResult]::new('distill', 'distill', [CompletionResultType]::ParameterValue, 'Extract a heuristic from a resolved trace into a skill. Review output against the anti-AI-slop checklist (.agents/skills/skill-creator/references/anti_ai_slop.md)')
            [CompletionResult]::new('errors', 'errors', [CompletionResultType]::ParameterValue, 'Inspect and clear fail-fast error signatures')
            [CompletionResult]::new('loc', 'loc', [CompletionResultType]::ParameterValue, 'Report line-of-code state for the 500-LOC invariant')
            [CompletionResult]::new('split', 'split', [CompletionResultType]::ParameterValue, 'Extract a large top-level item into a sibling module')
            [CompletionResult]::new('eval', 'eval', [CompletionResultType]::ParameterValue, 'Validate skill structure and benchmark skill evals')
            [CompletionResult]::new('hook', 'hook', [CompletionResultType]::ParameterValue, 'Manage git hooks that run `do-harness verify`')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Run diagnostic checks on binary resolution and git hook health')
            [CompletionResult]::new('metrics', 'metrics', [CompletionResultType]::ParameterValue, 'Report harness trends: sensor stats, strikes, eval pass-rate history')
            [CompletionResult]::new('overlap', 'overlap', [CompletionResultType]::ParameterValue, 'Rank skill pairs by guidance overlap (Tier-2 distinctiveness advisory)')
            [CompletionResult]::new('maintenance', 'maintenance', [CompletionResultType]::ParameterValue, 'Prune old beats and compact the state database')
            [CompletionResult]::new('compliance', 'compliance', [CompletionResultType]::ParameterValue, 'Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act')
            [CompletionResult]::new('audit-chain', 'audit-chain', [CompletionResultType]::ParameterValue, 'Recompute workflow event hash chain and report first divergence')
            [CompletionResult]::new('dora', 'dora', [CompletionResultType]::ParameterValue, 'Derive DORA deployment metrics from git history (deterministic, read-only)')
            [CompletionResult]::new('completions', 'completions', [CompletionResultType]::ParameterValue, 'Generate shell completions')
            [CompletionResult]::new('man', 'man', [CompletionResultType]::ParameterValue, 'Generate man page documentation')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'do-harness;help;version' {
            break
        }
        'do-harness;help;verify' {
            break
        }
        'do-harness;help;list' {
            break
        }
        'do-harness;help;explain' {
            break
        }
        'do-harness;help;status' {
            break
        }
        'do-harness;help;pr' {
            [CompletionResult]::new('readiness', 'readiness', [CompletionResultType]::ParameterValue, 'Check merge readiness for a pull request')
            [CompletionResult]::new('no-effect', 'no-effect', [CompletionResultType]::ParameterValue, 'Report whether a PR or revision range introduces any effective change')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'Emit the semantic residual: changed units evidence could not prove')
            break
        }
        'do-harness;help;pr;readiness' {
            break
        }
        'do-harness;help;pr;no-effect' {
            break
        }
        'do-harness;help;pr;review' {
            break
        }
        'do-harness;help;skills' {
            [CompletionResult]::new('suggest', 'suggest', [CompletionResultType]::ParameterValue, 'Rank skills by relevance to a query using metadata only')
            [CompletionResult]::new('drift', 'drift', [CompletionResultType]::ParameterValue, 'Check manifest-managed shared skills against their pinned digests')
            break
        }
        'do-harness;help;skills;suggest' {
            break
        }
        'do-harness;help;skills;drift' {
            break
        }
        'do-harness;help;init-db' {
            break
        }
        'do-harness;help;seed' {
            break
        }
        'do-harness;help;init' {
            break
        }
        'do-harness;help;task' {
            [CompletionResult]::new('export', 'export', [CompletionResultType]::ParameterValue, 'Write the task list to plans/tasks.json or specified output')
            [CompletionResult]::new('import', 'import', [CompletionResultType]::ParameterValue, 'Validate plans/tasks.json against the state database')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Print tasks from the state database')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show details for a specific task')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add a task in `pending` state')
            [CompletionResult]::new('advance', 'advance', [CompletionResultType]::ParameterValue, 'Advance the task''s subtask pointer')
            [CompletionResult]::new('done', 'done', [CompletionResultType]::ParameterValue, 'Mark a task done once its sensor-gated subtasks have passed')
            [CompletionResult]::new('fail', 'fail', [CompletionResultType]::ParameterValue, 'Mark a task failed')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove or cancel a task')
            break
        }
        'do-harness;help;task;export' {
            break
        }
        'do-harness;help;task;import' {
            break
        }
        'do-harness;help;task;list' {
            break
        }
        'do-harness;help;task;show' {
            break
        }
        'do-harness;help;task;add' {
            break
        }
        'do-harness;help;task;advance' {
            break
        }
        'do-harness;help;task;done' {
            break
        }
        'do-harness;help;task;fail' {
            break
        }
        'do-harness;help;task;remove' {
            break
        }
        'do-harness;help;trace' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Record a trace of an executed command and its resolution')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'Print traces for a session')
            [CompletionResult]::new('sessions', 'sessions', [CompletionResultType]::ParameterValue, 'List distinct trace session identifiers')
            break
        }
        'do-harness;help;trace;add' {
            break
        }
        'do-harness;help;trace;list' {
            break
        }
        'do-harness;help;trace;sessions' {
            break
        }
        'do-harness;help;distill' {
            break
        }
        'do-harness;help;errors' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List fail-fast error signatures (prefixed with `sensor:`)')
            [CompletionResult]::new('clear', 'clear', [CompletionResultType]::ParameterValue, 'Clear fail-fast error signatures (e.g. `sensor:<name>`)')
            break
        }
        'do-harness;help;errors;list' {
            break
        }
        'do-harness;help;errors;clear' {
            break
        }
        'do-harness;help;loc' {
            break
        }
        'do-harness;help;split' {
            break
        }
        'do-harness;help;eval' {
            break
        }
        'do-harness;help;hook' {
            [CompletionResult]::new('install', 'install', [CompletionResultType]::ParameterValue, 'Write pre-commit and pre-push hooks into `.git/hooks/`')
            [CompletionResult]::new('uninstall', 'uninstall', [CompletionResultType]::ParameterValue, 'Remove managed hooks, leaving foreign hook files untouched')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show whether the managed hooks and release binary are present')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'Show diff between installed hooks and current templates')
            break
        }
        'do-harness;help;hook;install' {
            break
        }
        'do-harness;help;hook;uninstall' {
            break
        }
        'do-harness;help;hook;status' {
            break
        }
        'do-harness;help;hook;diff' {
            break
        }
        'do-harness;help;doctor' {
            break
        }
        'do-harness;help;metrics' {
            break
        }
        'do-harness;help;overlap' {
            break
        }
        'do-harness;help;maintenance' {
            break
        }
        'do-harness;help;compliance' {
            break
        }
        'do-harness;help;audit-chain' {
            break
        }
        'do-harness;help;dora' {
            break
        }
        'do-harness;help;completions' {
            break
        }
        'do-harness;help;man' {
            break
        }
        'do-harness;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}

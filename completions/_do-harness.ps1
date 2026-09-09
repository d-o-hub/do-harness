
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
            [CompletionResult]::new('init-db', 'init-db', [CompletionResultType]::ParameterValue, 'Apply pending database migrations')
            [CompletionResult]::new('seed', 'seed', [CompletionResultType]::ParameterValue, 'Seed invariants from plans/invariants.json')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Scaffold a harness workspace in a target directory')
            [CompletionResult]::new('task', 'task', [CompletionResultType]::ParameterValue, 'Inspect and export task state')
            [CompletionResult]::new('trace', 'trace', [CompletionResultType]::ParameterValue, 'Record and list interaction traces')
            [CompletionResult]::new('distill', 'distill', [CompletionResultType]::ParameterValue, 'Extract a heuristic from a resolved trace')
            [CompletionResult]::new('errors', 'errors', [CompletionResultType]::ParameterValue, 'Inspect and clear fail-fast error signatures')
            [CompletionResult]::new('eval', 'eval', [CompletionResultType]::ParameterValue, 'Validate skill structure and benchmark skill evals')
            [CompletionResult]::new('hook', 'hook', [CompletionResultType]::ParameterValue, 'Manage git hooks that run `do-harness verify`')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Run diagnostic checks on binary resolution and git hook health')
            [CompletionResult]::new('metrics', 'metrics', [CompletionResultType]::ParameterValue, 'Report harness trends: sensor stats, strikes, eval pass-rate history')
            [CompletionResult]::new('compliance', 'compliance', [CompletionResultType]::ParameterValue, 'Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act')
            [CompletionResult]::new('audit-chain', 'audit-chain', [CompletionResultType]::ParameterValue, 'Recompute workflow event hash chain and report first divergence')
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
            [CompletionResult]::new('--only', '--only', [CompletionResultType]::ParameterName, 'Run only the named sensor (repeatable or comma-separated)')
            [CompletionResult]::new('--exclude', '--exclude', [CompletionResultType]::ParameterName, 'Exclude named sensors from the run')
            [CompletionResult]::new('--task', '--task', [CompletionResultType]::ParameterName, 'Scope records and fail-fast strikes to this task id')
            [CompletionResult]::new('--evidence', '--evidence', [CompletionResultType]::ParameterName, 'Write a machine-readable evidence artifact to path')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--fail-fast', '--fail-fast', [CompletionResultType]::ParameterName, 'Halt at the first failing sensor')
            [CompletionResult]::new('--record', '--record', [CompletionResultType]::ParameterName, 'Persist beats and error signatures into the state database')
            [CompletionResult]::new('--strict', '--strict', [CompletionResultType]::ParameterName, 'Enforce strong evidence: exit non-zero on skips or missing timing/exit codes')
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
            [CompletionResult]::new('--only', '--only', [CompletionResultType]::ParameterName, 'Run only the named sensor (repeatable or comma-separated)')
            [CompletionResult]::new('--exclude', '--exclude', [CompletionResultType]::ParameterName, 'Exclude named sensors from the run')
            [CompletionResult]::new('--task', '--task', [CompletionResultType]::ParameterName, 'Scope records and fail-fast strikes to this task id')
            [CompletionResult]::new('--evidence', '--evidence', [CompletionResultType]::ParameterName, 'Write a machine-readable evidence artifact to path')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--fail-fast', '--fail-fast', [CompletionResultType]::ParameterName, 'Halt at the first failing sensor')
            [CompletionResult]::new('--record', '--record', [CompletionResultType]::ParameterName, 'Persist beats and error signatures into the state database')
            [CompletionResult]::new('--strict', '--strict', [CompletionResultType]::ParameterName, 'Enforce strong evidence: exit non-zero on skips or missing timing/exit codes')
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
        'do-harness;init-db' {
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
        'do-harness;seed' {
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
        'do-harness;init' {
            [CompletionResult]::new('--language', '--language', [CompletionResultType]::ParameterName, 'Language pack to scaffold')
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
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--to-fixture', '--to-fixture', [CompletionResultType]::ParameterName, 'Raise the skill''s pass-rate bar after this recovery')
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
        'do-harness;eval' {
            [CompletionResult]::new('--skill', '--skill', [CompletionResultType]::ParameterName, 'Restrict evaluation to this skill directory name')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Workspace root override (default: walk up from cwd)')
            [CompletionResult]::new('--config', '--config', [CompletionResultType]::ParameterName, 'Explicit path to do-harness.toml')
            [CompletionResult]::new('--color', '--color', [CompletionResultType]::ParameterName, 'Color output (auto, always, never)')
            [CompletionResult]::new('--output', '--output', [CompletionResultType]::ParameterName, 'Default output file path')
            [CompletionResult]::new('--bless', '--bless', [CompletionResultType]::ParameterName, 'Re-baseline graders and update pass-rate floor on green run')
            [CompletionResult]::new('--list-skills', '--list-skills', [CompletionResultType]::ParameterName, 'List available skills')
            [CompletionResult]::new('--fail-fast', '--fail-fast', [CompletionResultType]::ParameterName, 'Halt on first failing evaluation')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Perform dry-run evaluation')
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
            [CompletionResult]::new('--since', '--since', [CompletionResultType]::ParameterName, 'Filter metrics since timestamp (Unix timestamp or ISO string)')
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
            [CompletionResult]::new('init-db', 'init-db', [CompletionResultType]::ParameterValue, 'Apply pending database migrations')
            [CompletionResult]::new('seed', 'seed', [CompletionResultType]::ParameterValue, 'Seed invariants from plans/invariants.json')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Scaffold a harness workspace in a target directory')
            [CompletionResult]::new('task', 'task', [CompletionResultType]::ParameterValue, 'Inspect and export task state')
            [CompletionResult]::new('trace', 'trace', [CompletionResultType]::ParameterValue, 'Record and list interaction traces')
            [CompletionResult]::new('distill', 'distill', [CompletionResultType]::ParameterValue, 'Extract a heuristic from a resolved trace')
            [CompletionResult]::new('errors', 'errors', [CompletionResultType]::ParameterValue, 'Inspect and clear fail-fast error signatures')
            [CompletionResult]::new('eval', 'eval', [CompletionResultType]::ParameterValue, 'Validate skill structure and benchmark skill evals')
            [CompletionResult]::new('hook', 'hook', [CompletionResultType]::ParameterValue, 'Manage git hooks that run `do-harness verify`')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Run diagnostic checks on binary resolution and git hook health')
            [CompletionResult]::new('metrics', 'metrics', [CompletionResultType]::ParameterValue, 'Report harness trends: sensor stats, strikes, eval pass-rate history')
            [CompletionResult]::new('compliance', 'compliance', [CompletionResultType]::ParameterValue, 'Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act')
            [CompletionResult]::new('audit-chain', 'audit-chain', [CompletionResultType]::ParameterValue, 'Recompute workflow event hash chain and report first divergence')
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
        'do-harness;help;compliance' {
            break
        }
        'do-harness;help;audit-chain' {
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

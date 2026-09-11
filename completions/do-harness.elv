
use builtin;
use str;

set edit:completion:arg-completer[do-harness] = {|@words|
    fn spaces {|n|
        builtin:repeat $n ' ' | str:join ''
    }
    fn cand {|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }
    var command = 'do-harness'
    for word $words[1..-1] {
        if (str:has-prefix $word '-') {
            break
        }
        set command = $command';'$word
    }
    var completions = [
        &'do-harness'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
            cand version 'Print CLI version information'
            cand verify 'Run computational sensors'
            cand check 'Run computational sensors'
            cand list 'List sensor names'
            cand ls 'List sensor names'
            cand explain 'Explain which sensors the current change selects, without running them'
            cand status 'Report verification evidence freshness without running sensors'
            cand init-db 'Apply pending database migrations'
            cand seed 'Seed invariants from plans/invariants.json'
            cand init 'Scaffold a harness workspace in a target directory'
            cand task 'Inspect and export task state'
            cand trace 'Record and list interaction traces'
            cand distill 'Extract a heuristic from a resolved trace'
            cand errors 'Inspect and clear fail-fast error signatures'
            cand eval 'Validate skill structure and benchmark skill evals'
            cand hook 'Manage git hooks that run `do-harness verify`'
            cand doctor 'Run diagnostic checks on binary resolution and git hook health'
            cand metrics 'Report harness trends: sensor stats, strikes, eval pass-rate history'
            cand maintenance 'Prune old beats and compact the state database'
            cand compliance 'Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act'
            cand audit-chain 'Recompute workflow event hash chain and report first divergence'
            cand completions 'Generate shell completions'
            cand man 'Generate man page documentation'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'do-harness;version'= {
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;verify'= {
            cand --format 'Output format'
            cand --set 'Run only the sensors in this development signal set'
            cand --only 'Run only the named sensor (repeatable or comma-separated)'
            cand --exclude 'Exclude named sensors from the run'
            cand --task 'Scope records and fail-fast strikes to this task id'
            cand --evidence 'Write a machine-readable evidence artifact to path'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --fail-fast 'Halt at the first failing sensor'
            cand --changed 'Run only sensors applicable to the working-tree change'
            cand --record 'Persist beats and error signatures into the state database'
            cand --strict 'Enforce strong evidence: exit non-zero on skips or missing timing/exit codes'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;check'= {
            cand --format 'Output format'
            cand --set 'Run only the sensors in this development signal set'
            cand --only 'Run only the named sensor (repeatable or comma-separated)'
            cand --exclude 'Exclude named sensors from the run'
            cand --task 'Scope records and fail-fast strikes to this task id'
            cand --evidence 'Write a machine-readable evidence artifact to path'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --fail-fast 'Halt at the first failing sensor'
            cand --changed 'Run only sensors applicable to the working-tree change'
            cand --record 'Persist beats and error signatures into the state database'
            cand --strict 'Enforce strong evidence: exit non-zero on skips or missing timing/exit codes'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;list'= {
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --sets 'List development signal-set names instead of sensors'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;ls'= {
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --sets 'List development signal-set names instead of sensors'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;explain'= {
            cand --set 'Explain this development signal set (default: all sensors)'
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --changed 'Select by working-tree change instead of listing everything'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;status'= {
            cand --set 'Check evidence for this development signal set'
            cand --evidence 'Read evidence from this file instead of the default artifact'
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;init-db'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --check 'Report pending migrations and exit non-zero when any are pending'
            cand --dry-run 'Report pending migrations without applying them (exit 0)'
            cand -y 'Skip the interactive confirmation prompt'
            cand --yes 'Skip the interactive confirmation prompt'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;seed'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --prune 'Delete invariants no longer present in plans/invariants.json'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;init'= {
            cand --language 'Language pack to scaffold (default: detect from the repository)'
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --force 'Overwrite existing files'
            cand --no-seed 'Do not seed invariants'
            cand --minimal 'Minimal setup without extra skills'
            cand --no-gitignore 'Skip creating/modifying .gitignore'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;task'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
            cand export 'Write the task list to plans/tasks.json or specified output'
            cand import 'Validate plans/tasks.json against the state database'
            cand list 'Print tasks from the state database'
            cand show 'Show details for a specific task'
            cand add 'Add a task in `pending` state'
            cand advance 'Advance the task''s subtask pointer'
            cand done 'Mark a task done once its sensor-gated subtasks have passed'
            cand fail 'Mark a task failed'
            cand remove 'Remove or cancel a task'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'do-harness;task;export'= {
            cand -o 'Output file path or - for stdout'
            cand --output 'Output file path or - for stdout'
            cand --format 'Output format (json/text)'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --stdout 'Write directly to stdout instead of file'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;task;import'= {
            cand --file 'Snapshot file (defaults to plans/tasks.json under the root)'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --check 'Exit non-zero when the snapshot and database drift'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;task;list'= {
            cand --status 'Filter tasks by status (pending, `in_progress`, completed, failed)'
            cand --method 'Filter tasks by method name'
            cand --parent 'Filter tasks by parent ID'
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;task;show'= {
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;task;add'= {
            cand --method 'Name of HTN method defined in plans/methods.json'
            cand --parent 'Parent task id'
            cand --precondition 'Recorded precondition guard'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;task;advance'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --dry-run 'Perform dry run without state changes'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;task;done'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --dry-run 'Perform dry run without state changes'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;task;fail'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;task;remove'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;task;help'= {
            cand export 'Write the task list to plans/tasks.json or specified output'
            cand import 'Validate plans/tasks.json against the state database'
            cand list 'Print tasks from the state database'
            cand show 'Show details for a specific task'
            cand add 'Add a task in `pending` state'
            cand advance 'Advance the task''s subtask pointer'
            cand done 'Mark a task done once its sensor-gated subtasks have passed'
            cand fail 'Mark a task failed'
            cand remove 'Remove or cancel a task'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'do-harness;task;help;export'= {
        }
        &'do-harness;task;help;import'= {
        }
        &'do-harness;task;help;list'= {
        }
        &'do-harness;task;help;show'= {
        }
        &'do-harness;task;help;add'= {
        }
        &'do-harness;task;help;advance'= {
        }
        &'do-harness;task;help;done'= {
        }
        &'do-harness;task;help;fail'= {
        }
        &'do-harness;task;help;remove'= {
        }
        &'do-harness;task;help;help'= {
        }
        &'do-harness;trace'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
            cand add 'Record a trace of an executed command and its resolution'
            cand list 'Print traces for a session'
            cand sessions 'List distinct trace session identifiers'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'do-harness;trace;add'= {
            cand --session 'Session identifier grouping related traces'
            cand --task 'Owning task id'
            cand --command 'The command that was executed'
            cand --error-diff 'Error diff or failure output captured'
            cand --resolution-steps 'Steps taken to resolve the failure'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;trace;list'= {
            cand --session 'Session identifier'
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;trace;sessions'= {
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;trace;help'= {
            cand add 'Record a trace of an executed command and its resolution'
            cand list 'Print traces for a session'
            cand sessions 'List distinct trace session identifiers'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'do-harness;trace;help;add'= {
        }
        &'do-harness;trace;help;list'= {
        }
        &'do-harness;trace;help;sessions'= {
        }
        &'do-harness;trace;help;help'= {
        }
        &'do-harness;distill'= {
            cand --skill 'Skill the heuristic belongs to'
            cand --pattern 'Generalized pattern to record'
            cand --description 'When the pattern applies'
            cand --from-trace 'Source trace id; required as evidence of a resolved fix'
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --to-fixture 'Raise the skill''s pass-rate bar after this recovery'
            cand --dry-run 'Perform dry run without modifying skill files'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;errors'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
            cand list 'List fail-fast error signatures (prefixed with `sensor:`)'
            cand clear 'Clear fail-fast error signatures (e.g. `sensor:<name>`)'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'do-harness;errors;list'= {
            cand --task 'Scope to one task id'
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;errors;clear'= {
            cand --sensor 'Only clear this signature key (e.g. `sensor:<name>`)'
            cand --task 'Only clear signatures for this task id'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --force 'Force clearing without prompt'
            cand --dry-run 'Perform dry run without clearing'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;errors;help'= {
            cand list 'List fail-fast error signatures (prefixed with `sensor:`)'
            cand clear 'Clear fail-fast error signatures (e.g. `sensor:<name>`)'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'do-harness;errors;help;list'= {
        }
        &'do-harness;errors;help;clear'= {
        }
        &'do-harness;errors;help;help'= {
        }
        &'do-harness;eval'= {
            cand --skill 'Restrict evaluation to this skill directory name'
            cand --format 'Output format'
            cand --approver 'Approver identity recorded with `--bless` (defaults to `DO_HARNESS_APPROVER` or the git user email)'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --bless 'Re-baseline graders and update pass-rate floor on green run'
            cand --list-skills 'List available skills'
            cand --fail-fast 'Halt on first failing evaluation'
            cand --dry-run 'Perform dry-run evaluation'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;hook'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
            cand install 'Write pre-commit and pre-push hooks into `.git/hooks/`'
            cand uninstall 'Remove managed hooks, leaving foreign hook files untouched'
            cand status 'Show whether the managed hooks and release binary are present'
            cand diff 'Show diff between installed hooks and current templates'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'do-harness;hook;install'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --force 'Overwrite foreign (unmanaged) hook files'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;hook;uninstall'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;hook;status'= {
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;hook;diff'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;hook;help'= {
            cand install 'Write pre-commit and pre-push hooks into `.git/hooks/`'
            cand uninstall 'Remove managed hooks, leaving foreign hook files untouched'
            cand status 'Show whether the managed hooks and release binary are present'
            cand diff 'Show diff between installed hooks and current templates'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'do-harness;hook;help;install'= {
        }
        &'do-harness;hook;help;uninstall'= {
        }
        &'do-harness;hook;help;status'= {
        }
        &'do-harness;hook;help;diff'= {
        }
        &'do-harness;hook;help;help'= {
        }
        &'do-harness;doctor'= {
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand --strict 'Strictly enforce warning checks as failures'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;metrics'= {
            cand --format 'Output format'
            cand --sensor 'Filter by sensor name'
            cand --skill 'Filter by skill name'
            cand --since 'Filter metrics since a Unix timestamp in seconds'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;maintenance'= {
            cand --prune-beats 'Delete beats older than this many days (keeps the most recent per task)'
            cand --keep-per-task 'Minimum most-recent beats kept per task when pruning'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;compliance'= {
            cand --framework 'Filter framework (owasp-agentic-top10, nist-ai-rmf, eu-ai-act, soc2)'
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;audit-chain'= {
            cand --format 'Output format'
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help (see more with ''--help'')'
            cand --help 'Print help (see more with ''--help'')'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;completions'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;man'= {
            cand --root 'Workspace root override (default: walk up from cwd)'
            cand --config 'Explicit path to do-harness.toml'
            cand --color 'Color output (auto, always, never)'
            cand --output 'Default output file path'
            cand -v 'Verbosity level (-v, -vv)'
            cand --verbose 'Verbosity level (-v, -vv)'
            cand -q 'Suppress non-error messages'
            cand --quiet 'Suppress non-error messages'
            cand --dry-run 'Dry run without side effects'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
        &'do-harness;help'= {
            cand version 'Print CLI version information'
            cand verify 'Run computational sensors'
            cand list 'List sensor names'
            cand explain 'Explain which sensors the current change selects, without running them'
            cand status 'Report verification evidence freshness without running sensors'
            cand init-db 'Apply pending database migrations'
            cand seed 'Seed invariants from plans/invariants.json'
            cand init 'Scaffold a harness workspace in a target directory'
            cand task 'Inspect and export task state'
            cand trace 'Record and list interaction traces'
            cand distill 'Extract a heuristic from a resolved trace'
            cand errors 'Inspect and clear fail-fast error signatures'
            cand eval 'Validate skill structure and benchmark skill evals'
            cand hook 'Manage git hooks that run `do-harness verify`'
            cand doctor 'Run diagnostic checks on binary resolution and git hook health'
            cand metrics 'Report harness trends: sensor stats, strikes, eval pass-rate history'
            cand maintenance 'Prune old beats and compact the state database'
            cand compliance 'Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act'
            cand audit-chain 'Recompute workflow event hash chain and report first divergence'
            cand completions 'Generate shell completions'
            cand man 'Generate man page documentation'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'do-harness;help;version'= {
        }
        &'do-harness;help;verify'= {
        }
        &'do-harness;help;list'= {
        }
        &'do-harness;help;explain'= {
        }
        &'do-harness;help;status'= {
        }
        &'do-harness;help;init-db'= {
        }
        &'do-harness;help;seed'= {
        }
        &'do-harness;help;init'= {
        }
        &'do-harness;help;task'= {
            cand export 'Write the task list to plans/tasks.json or specified output'
            cand import 'Validate plans/tasks.json against the state database'
            cand list 'Print tasks from the state database'
            cand show 'Show details for a specific task'
            cand add 'Add a task in `pending` state'
            cand advance 'Advance the task''s subtask pointer'
            cand done 'Mark a task done once its sensor-gated subtasks have passed'
            cand fail 'Mark a task failed'
            cand remove 'Remove or cancel a task'
        }
        &'do-harness;help;task;export'= {
        }
        &'do-harness;help;task;import'= {
        }
        &'do-harness;help;task;list'= {
        }
        &'do-harness;help;task;show'= {
        }
        &'do-harness;help;task;add'= {
        }
        &'do-harness;help;task;advance'= {
        }
        &'do-harness;help;task;done'= {
        }
        &'do-harness;help;task;fail'= {
        }
        &'do-harness;help;task;remove'= {
        }
        &'do-harness;help;trace'= {
            cand add 'Record a trace of an executed command and its resolution'
            cand list 'Print traces for a session'
            cand sessions 'List distinct trace session identifiers'
        }
        &'do-harness;help;trace;add'= {
        }
        &'do-harness;help;trace;list'= {
        }
        &'do-harness;help;trace;sessions'= {
        }
        &'do-harness;help;distill'= {
        }
        &'do-harness;help;errors'= {
            cand list 'List fail-fast error signatures (prefixed with `sensor:`)'
            cand clear 'Clear fail-fast error signatures (e.g. `sensor:<name>`)'
        }
        &'do-harness;help;errors;list'= {
        }
        &'do-harness;help;errors;clear'= {
        }
        &'do-harness;help;eval'= {
        }
        &'do-harness;help;hook'= {
            cand install 'Write pre-commit and pre-push hooks into `.git/hooks/`'
            cand uninstall 'Remove managed hooks, leaving foreign hook files untouched'
            cand status 'Show whether the managed hooks and release binary are present'
            cand diff 'Show diff between installed hooks and current templates'
        }
        &'do-harness;help;hook;install'= {
        }
        &'do-harness;help;hook;uninstall'= {
        }
        &'do-harness;help;hook;status'= {
        }
        &'do-harness;help;hook;diff'= {
        }
        &'do-harness;help;doctor'= {
        }
        &'do-harness;help;metrics'= {
        }
        &'do-harness;help;maintenance'= {
        }
        &'do-harness;help;compliance'= {
        }
        &'do-harness;help;audit-chain'= {
        }
        &'do-harness;help;completions'= {
        }
        &'do-harness;help;man'= {
        }
        &'do-harness;help;help'= {
        }
    ]
    $completions[$command]
}

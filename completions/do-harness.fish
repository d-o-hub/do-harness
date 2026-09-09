# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_do_harness_global_optspecs
    string join \n root= config= v/verbose q/quiet color= output= dry-run h/help V/version
end

function __fish_do_harness_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_do_harness_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_do_harness_using_subcommand
    set -l cmd (__fish_do_harness_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c do-harness -n "__fish_do_harness_needs_command" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_needs_command" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_needs_command" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_needs_command" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_needs_command" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_needs_command" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_needs_command" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_needs_command" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_needs_command" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "version" -d 'Print CLI version information'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "verify" -d 'Run computational sensors'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "check" -d 'Run computational sensors'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "list" -d 'List sensor names'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "ls" -d 'List sensor names'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "init-db" -d 'Apply pending database migrations'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "seed" -d 'Seed invariants from plans/invariants.json'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "init" -d 'Scaffold a harness workspace in a target directory'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "task" -d 'Inspect and export task state'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "trace" -d 'Record and list interaction traces'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "distill" -d 'Extract a heuristic from a resolved trace'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "errors" -d 'Inspect and clear fail-fast error signatures'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "eval" -d 'Validate skill structure and benchmark skill evals'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "hook" -d 'Manage git hooks that run `do-harness verify`'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "doctor" -d 'Run diagnostic checks on binary resolution and git hook health'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "metrics" -d 'Report harness trends: sensor stats, strikes, eval pass-rate history'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "compliance" -d 'Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "audit-chain" -d 'Recompute workflow event hash chain and report first divergence'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "completions" -d 'Generate shell completions'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "man" -d 'Generate man page documentation'
complete -c do-harness -n "__fish_do_harness_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c do-harness -n "__fish_do_harness_using_subcommand version" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand version" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand version" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand version" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand version" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand version" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand version" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand version" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand version" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand version" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l only -d 'Run only the named sensor (repeatable or comma-separated)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l exclude -d 'Exclude named sensors from the run' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l task -d 'Scope records and fail-fast strikes to this task id' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l evidence -d 'Write a machine-readable evidence artifact to path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l fail-fast -d 'Halt at the first failing sensor'
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l record -d 'Persist beats and error signatures into the state database'
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l strict -d 'Enforce strong evidence: exit non-zero on skips or missing timing/exit codes'
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand verify" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l only -d 'Run only the named sensor (repeatable or comma-separated)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l exclude -d 'Exclude named sensors from the run' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l task -d 'Scope records and fail-fast strikes to this task id' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l evidence -d 'Write a machine-readable evidence artifact to path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l fail-fast -d 'Halt at the first failing sensor'
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l record -d 'Persist beats and error signatures into the state database'
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l strict -d 'Enforce strong evidence: exit non-zero on skips or missing timing/exit codes'
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand check" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand list" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand list" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand list" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand list" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand list" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand list" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand list" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand list" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand list" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand ls" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand ls" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand ls" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand ls" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand ls" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand ls" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand ls" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand ls" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand ls" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand ls" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand init-db" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand init-db" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand init-db" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand init-db" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand init-db" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand init-db" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand init-db" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand init-db" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand init-db" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand seed" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand seed" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand seed" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand seed" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand seed" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand seed" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand seed" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand seed" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand seed" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l language -d 'Language pack to scaffold' -r -f -a "rust\t'Rust sensor pack (fmt/check/clippy/test/loc) plus a check-loc script'
generic\t'No built-in sensors; commented sensor stubs to fill in'"
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l force -d 'Overwrite existing files'
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l no-seed -d 'Do not seed invariants'
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l minimal -d 'Minimal setup without extra skills'
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l no-gitignore -d 'Skip creating/modifying .gitignore'
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand init" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -f -a "export" -d 'Write the task list to plans/tasks.json or specified output'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -f -a "list" -d 'Print tasks from the state database'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -f -a "show" -d 'Show details for a specific task'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -f -a "add" -d 'Add a task in `pending` state'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -f -a "advance" -d 'Advance the task\'s subtask pointer'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -f -a "done" -d 'Mark a task done once its sensor-gated subtasks have passed'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -f -a "fail" -d 'Mark a task failed'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -f -a "remove" -d 'Remove or cancel a task'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and not __fish_seen_subcommand_from export list show add advance done fail remove help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -s o -l output -d 'Output file path or - for stdout' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -l format -d 'Output format (json/text)' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -l stdout -d 'Write directly to stdout instead of file'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from export" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -l status -d 'Filter tasks by status (pending, in_progress, completed, failed)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -l method -d 'Filter tasks by method name' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -l parent -d 'Filter tasks by parent ID' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from show" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from show" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from show" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from show" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from show" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from show" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from show" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from show" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from show" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -l method -d 'Name of HTN method defined in plans/methods.json' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -l parent -d 'Parent task id' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -l precondition -d 'Recorded precondition guard' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from add" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from advance" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from advance" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from advance" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from advance" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from advance" -l dry-run -d 'Perform dry run without state changes'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from advance" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from advance" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from advance" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from advance" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from done" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from done" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from done" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from done" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from done" -l dry-run -d 'Perform dry run without state changes'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from done" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from done" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from done" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from done" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from fail" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from fail" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from fail" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from fail" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from fail" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from fail" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from fail" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from fail" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from fail" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from remove" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from remove" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from remove" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from remove" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from remove" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from remove" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from remove" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from remove" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from help" -f -a "export" -d 'Write the task list to plans/tasks.json or specified output'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from help" -f -a "list" -d 'Print tasks from the state database'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from help" -f -a "show" -d 'Show details for a specific task'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from help" -f -a "add" -d 'Add a task in `pending` state'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from help" -f -a "advance" -d 'Advance the task\'s subtask pointer'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from help" -f -a "done" -d 'Mark a task done once its sensor-gated subtasks have passed'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from help" -f -a "fail" -d 'Mark a task failed'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove or cancel a task'
complete -c do-harness -n "__fish_do_harness_using_subcommand task; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -f -a "add" -d 'Record a trace of an executed command and its resolution'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -f -a "list" -d 'Print traces for a session'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -f -a "sessions" -d 'List distinct trace session identifiers'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and not __fish_seen_subcommand_from add list sessions help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -l session -d 'Session identifier grouping related traces' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -l task -d 'Owning task id' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -l command -d 'The command that was executed' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -l error-diff -d 'Error diff or failure output captured' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -l resolution-steps -d 'Steps taken to resolve the failure' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from add" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -l session -d 'Session identifier' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from sessions" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from sessions" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from sessions" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from sessions" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from sessions" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from sessions" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from sessions" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from sessions" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from sessions" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from sessions" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from help" -f -a "add" -d 'Record a trace of an executed command and its resolution'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from help" -f -a "list" -d 'Print traces for a session'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from help" -f -a "sessions" -d 'List distinct trace session identifiers'
complete -c do-harness -n "__fish_do_harness_using_subcommand trace; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l skill -d 'Skill the heuristic belongs to' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l pattern -d 'Generalized pattern to record' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l description -d 'When the pattern applies' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l from-trace -d 'Source trace id; required as evidence of a resolved fix' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l to-fixture -d 'Raise the skill\'s pass-rate bar after this recovery'
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -l dry-run -d 'Perform dry run without modifying skill files'
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand distill" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -f -a "list" -d 'List fail-fast error signatures (prefixed with `sensor:`)'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -f -a "clear" -d 'Clear fail-fast error signatures (e.g. `sensor:<name>`)'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and not __fish_seen_subcommand_from list clear help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -l task -d 'Scope to one task id' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from list" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -l sensor -d 'Only clear this signature key (e.g. `sensor:<name>`)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -l task -d 'Only clear signatures for this task id' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -l force -d 'Force clearing without prompt'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -l dry-run -d 'Perform dry run without clearing'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from clear" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from help" -f -a "list" -d 'List fail-fast error signatures (prefixed with `sensor:`)'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from help" -f -a "clear" -d 'Clear fail-fast error signatures (e.g. `sensor:<name>`)'
complete -c do-harness -n "__fish_do_harness_using_subcommand errors; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -l skill -d 'Restrict evaluation to this skill directory name' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -l bless -d 'Re-baseline graders and update pass-rate floor on green run'
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -l list-skills -d 'List available skills'
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -l fail-fast -d 'Halt on first failing evaluation'
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -l dry-run -d 'Perform dry-run evaluation'
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand eval" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -f -a "install" -d 'Write pre-commit and pre-push hooks into `.git/hooks/`'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -f -a "uninstall" -d 'Remove managed hooks, leaving foreign hook files untouched'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -f -a "status" -d 'Show whether the managed hooks and release binary are present'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -f -a "diff" -d 'Show diff between installed hooks and current templates'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and not __fish_seen_subcommand_from install uninstall status diff help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from install" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from install" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from install" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from install" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from install" -l force -d 'Overwrite foreign (unmanaged) hook files'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from install" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from install" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from install" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from install" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from install" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from uninstall" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from uninstall" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from uninstall" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from uninstall" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from uninstall" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from uninstall" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from uninstall" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from uninstall" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from uninstall" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from status" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from status" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from status" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from status" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from status" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from status" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from status" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from status" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from status" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from diff" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from diff" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from diff" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from diff" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from diff" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from diff" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from diff" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from diff" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from diff" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from help" -f -a "install" -d 'Write pre-commit and pre-push hooks into `.git/hooks/`'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from help" -f -a "uninstall" -d 'Remove managed hooks, leaving foreign hook files untouched'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from help" -f -a "status" -d 'Show whether the managed hooks and release binary are present'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from help" -f -a "diff" -d 'Show diff between installed hooks and current templates'
complete -c do-harness -n "__fish_do_harness_using_subcommand hook; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -l strict -d 'Strictly enforce warning checks as failures'
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand doctor" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -l sensor -d 'Filter by sensor name' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -l skill -d 'Filter by skill name' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -l since -d 'Filter metrics since timestamp (Unix timestamp or ISO string)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand metrics" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -l framework -d 'Filter framework (owasp-agentic-top10, nist-ai-rmf, eu-ai-act, soc2)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand compliance" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand audit-chain" -l format -d 'Output format' -r -f -a "text\t'Human-readable text output'
json\t'Machine-readable JSON output'"
complete -c do-harness -n "__fish_do_harness_using_subcommand audit-chain" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand audit-chain" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand audit-chain" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand audit-chain" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand audit-chain" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand audit-chain" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand audit-chain" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand audit-chain" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c do-harness -n "__fish_do_harness_using_subcommand audit-chain" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand completions" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand completions" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand completions" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand completions" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand completions" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand completions" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand completions" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand completions" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand completions" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand man" -l root -d 'Workspace root override (default: walk up from cwd)' -r -f -a "(__fish_complete_directories)"
complete -c do-harness -n "__fish_do_harness_using_subcommand man" -l config -d 'Explicit path to do-harness.toml' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand man" -l color -d 'Color output (auto, always, never)' -r
complete -c do-harness -n "__fish_do_harness_using_subcommand man" -l output -d 'Default output file path' -r -F
complete -c do-harness -n "__fish_do_harness_using_subcommand man" -s v -l verbose -d 'Verbosity level (-v, -vv)'
complete -c do-harness -n "__fish_do_harness_using_subcommand man" -s q -l quiet -d 'Suppress non-error messages'
complete -c do-harness -n "__fish_do_harness_using_subcommand man" -l dry-run -d 'Dry run without side effects'
complete -c do-harness -n "__fish_do_harness_using_subcommand man" -s h -l help -d 'Print help'
complete -c do-harness -n "__fish_do_harness_using_subcommand man" -s V -l version -d 'Print version'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "version" -d 'Print CLI version information'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "verify" -d 'Run computational sensors'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "list" -d 'List sensor names'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "init-db" -d 'Apply pending database migrations'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "seed" -d 'Seed invariants from plans/invariants.json'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "init" -d 'Scaffold a harness workspace in a target directory'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "task" -d 'Inspect and export task state'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "trace" -d 'Record and list interaction traces'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "distill" -d 'Extract a heuristic from a resolved trace'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "errors" -d 'Inspect and clear fail-fast error signatures'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "eval" -d 'Validate skill structure and benchmark skill evals'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "hook" -d 'Manage git hooks that run `do-harness verify`'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "doctor" -d 'Run diagnostic checks on binary resolution and git hook health'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "metrics" -d 'Report harness trends: sensor stats, strikes, eval pass-rate history'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "compliance" -d 'Print compliance mapping to OWASP Agentic Top 10, NIST AI RMF, and EU AI Act'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "audit-chain" -d 'Recompute workflow event hash chain and report first divergence'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "completions" -d 'Generate shell completions'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "man" -d 'Generate man page documentation'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and not __fish_seen_subcommand_from version verify list init-db seed init task trace distill errors eval hook doctor metrics compliance audit-chain completions man help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from task" -f -a "export" -d 'Write the task list to plans/tasks.json or specified output'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from task" -f -a "list" -d 'Print tasks from the state database'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from task" -f -a "show" -d 'Show details for a specific task'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from task" -f -a "add" -d 'Add a task in `pending` state'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from task" -f -a "advance" -d 'Advance the task\'s subtask pointer'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from task" -f -a "done" -d 'Mark a task done once its sensor-gated subtasks have passed'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from task" -f -a "fail" -d 'Mark a task failed'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from task" -f -a "remove" -d 'Remove or cancel a task'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from trace" -f -a "add" -d 'Record a trace of an executed command and its resolution'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from trace" -f -a "list" -d 'Print traces for a session'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from trace" -f -a "sessions" -d 'List distinct trace session identifiers'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from errors" -f -a "list" -d 'List fail-fast error signatures (prefixed with `sensor:`)'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from errors" -f -a "clear" -d 'Clear fail-fast error signatures (e.g. `sensor:<name>`)'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from hook" -f -a "install" -d 'Write pre-commit and pre-push hooks into `.git/hooks/`'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from hook" -f -a "uninstall" -d 'Remove managed hooks, leaving foreign hook files untouched'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from hook" -f -a "status" -d 'Show whether the managed hooks and release binary are present'
complete -c do-harness -n "__fish_do_harness_using_subcommand help; and __fish_seen_subcommand_from hook" -f -a "diff" -d 'Show diff between installed hooks and current templates'

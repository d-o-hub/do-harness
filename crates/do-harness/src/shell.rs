//! Locating a usable `bash` across platforms.
//!
//! Windows makes this non-obvious. A bare `bash` on `PATH` can resolve to the
//! WSL launcher in `System32`, which is not Git Bash: with no distribution
//! installed it exits non-zero and writes nothing useful, so every shell
//! invocation fails with an empty diagnostic. Git Bash ships at a small set of
//! well-known locations, so resolve those first and only then fall back to
//! `PATH`.
//!
//! The candidate list is a pure function so it can be tested on any host.

use std::path::PathBuf;
use std::process::Command;

/// Well-known Git Bash locations, most specific first.
///
/// `%ProgramFiles%` is the default install root; `%LOCALAPPDATA%` covers
/// per-user installs. Both `Git\bin` (wrapper) and `Git\usr\bin` (real binary)
/// contain a usable `bash`.
#[must_use]
pub fn windows_bash_candidates() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(base) = std::env::var_os(var) {
            roots.push(PathBuf::from(base).join("Git"));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Programs").join("Git"));
    }
    let mut out = Vec::new();
    for root in roots {
        out.push(root.join("bin").join("bash.exe"));
        out.push(root.join("usr").join("bin").join("bash.exe"));
    }
    out
}

/// Resolves the `bash` executable to invoke.
///
/// On Windows the first existing Git Bash candidate wins; when none exists the
/// bare name is returned so the caller gets an ordinary "not found" error
/// instead of a hard-coded path that cannot work. On other platforms this is
/// always `bash`.
#[must_use]
pub fn bash_program() -> PathBuf {
    if cfg!(windows) {
        if let Some(found) = windows_bash_candidates()
            .into_iter()
            .find(|candidate| candidate.is_file())
        {
            return found;
        }
    }
    PathBuf::from("bash")
}

/// A `Command` for bash, resolved via [`bash_program`].
#[must_use]
pub fn bash() -> Command {
    Command::new(bash_program())
}

/// Resolves a program name that may be a POSIX shell.
///
/// Returns the resolved path for `bash` (and `sh`, which Windows does not ship
/// either) and the name unchanged for everything else. Used where a program
/// comes from configuration rather than from this crate, so a stock
/// `["bash", "scripts/…"]` sensor keeps working on Windows.
#[must_use]
pub fn resolve_program(program: &str) -> PathBuf {
    let stem = std::path::Path::new(program)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(program);
    if stem == "bash" {
        return bash_program();
    }
    if stem == "sh" && cfg!(windows) {
        // Git Bash's own `sh` sits beside its `bash`.
        if let Some(bash) = windows_bash_candidates()
            .into_iter()
            .find(|candidate| candidate.is_file())
        {
            let sh = bash.with_file_name("sh.exe");
            if sh.is_file() {
                return sh;
            }
            return bash;
        }
    }
    PathBuf::from(program)
}

/// Terminates a child and everything it spawned.
///
/// `Child::kill` on Windows calls `TerminateProcess` on the direct child only.
/// A shell wrapper (`bash -c "…"`) is a *parent* of the real work, so killing
/// it orphans the grandchild, which then keeps the inherited stdout/stderr pipe
/// open — a reader draining that pipe blocks forever, which is how a
/// `--fail-fast` cancellation or an agent timeout could hang indefinitely on
/// Windows.
///
/// On Windows this uses `taskkill /T /F`, which walks the process tree. On
/// POSIX, killing the direct child is sufficient because the shell substitutes
/// `exec` for the final command in a simple pipeline, so the child *is* the
/// work. A failure to run `taskkill` falls back to `Child::kill` so termination
/// is always attempted.
pub fn kill_tree(child: &mut std::process::Child) {
    if cfg!(windows) {
        {
            let pid = child.id();
            let killed = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if killed {
                let _ = child.wait();
                return;
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Builds a `Command` for a program that may be a POSIX shell or a Windows
/// command shim.
///
/// Two Windows specifics are handled here:
///
/// * `bash`/`sh` resolve to Git Bash (see [`resolve_program`]).
/// * `npm`, `npx`, and `pnpm` are installed as `.cmd` batch shims. Rust's
///   `Command::new` cannot execute those — on Windows it relies on
///   `CreateProcess`, which runs `.exe` images but not batch files, and a
///   bare name is not searched for a `.cmd` suffix. Spawning one therefore
///   fails with "program not found", which would break every generated
///   Node-pack sensor (`["npm", "run", "typecheck"]`) on Windows. Batch shims
///   are launched through `cmd /C`, the documented way to run them.
#[must_use]
pub fn command(program: &str, args: &[String]) -> Command {
    if cfg!(windows) {
        if let Some(shim) = windows_batch_shim(program) {
            let mut command = Command::new("cmd");
            command.arg("/C").arg(shim);
            command.args(args);
            return command;
        }
    }
    let mut command = Command::new(resolve_program(program));
    command.args(args);
    command
}

/// Finds a `.cmd`/`.bat` shim for `program` on `PATH`, if any.
///
/// Only consulted when `program` is a bare name: an explicit path or a name
/// with an extension is used as given. (`Path::join` replaces the base when the
/// component is absolute, so an unguarded join would let a caller's absolute
/// path be probed against every `PATH` entry.)
#[must_use]
pub fn windows_batch_shim(program: &str) -> Option<PathBuf> {
    let path = std::path::Path::new(program);
    if path.extension().is_some() || path.is_absolute() || program.contains('/') {
        return None;
    }
    let search = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&search) {
        if !dir.is_absolute() {
            continue;
        }
        for ext in ["cmd", "bat"] {
            let candidate = dir.join(format!("{program}.{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Whether a failed direct execution of a script means "run it through bash".
/// POSIX hosts report `PermissionDenied` for a non-executable script. Windows
/// cannot execute a shebang file at all and reports a *different* error
/// (`ERROR_BAD_EXE_FORMAT`, surfaced as `InvalidInput`/`Other` rather than
/// `PermissionDenied`), so checking only `PermissionDenied` silently skipped
/// the bash fallback there.
#[must_use]
pub fn needs_bash_fallback(err: &std::io::Error) -> bool {
    if cfg!(windows) {
        // Windows cannot run shebang scripts; any failure means "use bash".
        true
    } else {
        err.kind() == std::io::ErrorKind::PermissionDenied
    }
}

/// Retries for a transient `ETXTBSY` exec failure (see [`retry_executable_busy`]).
const EXEC_BUSY_RETRIES: u32 = 20;

/// Backoff between `ETXTBSY` retries; the race clears in microseconds, so this
/// only needs to yield the CPU to the writer holding the script open. The
/// combined budget is ~200ms, far longer than the race and short enough not to
/// stall a genuine failure appreciably.
const EXEC_BUSY_BACKOFF_MILLIS: u64 = 10;

/// Whether the error is the transient `ETXTBSY` fork/exec race.
///
/// Executing a script that is momentarily open for writing elsewhere fails with
/// `ETXTBSY`; Rust surfaces that as [`std::io::ErrorKind::ExecutableFileBusy`].
/// It is a race, not a property of the target: a forked child holds every
/// write-descriptor it inherited until its own `execve` completes, so an exec
/// of a just-written script can fail while an unrelated thread is mid-write or
/// mid-spawn.
#[must_use]
pub fn is_executable_busy(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::ExecutableFileBusy
}

/// Runs `attempt`, retrying while it fails with the transient busy condition.
///
/// Every exec of a script this process just wrote needs this: `fs::write`
/// followed immediately by an exec races any other thread that forks in
/// between. Without the retry the failure surfaces as a spurious launch error
/// (or, worse, as a silent fallback to a degraded code path).
pub fn retry_executable_busy<T>(
    mut attempt: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut result = attempt();
    for _ in 0..EXEC_BUSY_RETRIES {
        if !matches!(&result, Err(err) if is_executable_busy(err)) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(EXEC_BUSY_BACKOFF_MILLIS));
        result = attempt();
    }
    result
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    /// The retry predicate must not swallow unrelated launch errors: only the
    /// transient busy condition is retried. Built from `ErrorKind` directly so
    /// the assertion holds on every platform, including Windows where a raw
    /// error code of 26 means something else entirely.
    #[test]
    fn only_executable_file_busy_is_retried() {
        assert!(is_executable_busy(&std::io::Error::new(
            std::io::ErrorKind::ExecutableFileBusy,
            "busy"
        )));
        assert!(!is_executable_busy(&std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing"
        )));
        assert!(!is_executable_busy(&std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied"
        )));
        assert!(!is_executable_busy(&std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "interrupted"
        )));
    }

    /// `ETXTBSY` is the unix spelling of the race; this is the mapping the
    /// predicate relies on, asserted only on unix because raw error 26 is a
    /// *different* code on Windows.
    #[cfg(unix)]
    #[test]
    fn unix_text_file_busy_maps_to_executable_file_busy() {
        /// `ETXTBSY` on Linux/macOS (`asm-generic/errno.h`: ETXTBSY = 26).
        const TXTBSY: i32 = 26;
        assert!(is_executable_busy(&std::io::Error::from_raw_os_error(
            TXTBSY
        )));
    }

    /// The retry runs the closure again only while it reports the transient
    /// condition, and returns the first non-busy result unchanged.
    #[test]
    fn retry_repeats_only_while_busy_then_succeeds() {
        let mut calls = 0;
        let out = retry_executable_busy(|| {
            calls += 1;
            if calls < 3 {
                Err(std::io::Error::new(
                    std::io::ErrorKind::ExecutableFileBusy,
                    "busy",
                ))
            } else {
                Ok(calls)
            }
        })
        .unwrap();
        assert_eq!(out, 3);
        assert_eq!(calls, 3);
    }

    /// A non-busy error is returned on the first attempt without retrying, so
    /// a real failure is never delayed or masked.
    #[test]
    fn retry_does_not_repeat_a_permanent_error() {
        let mut calls = 0;
        let err = retry_executable_busy::<()>(|| {
            calls += 1;
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"))
        })
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(calls, 1);
    }

    /// Every candidate is a `bash.exe` beneath a `Git` root, and the list is
    /// built from the documented Windows environment roots. The env vars do
    /// not exist on a POSIX test host, so this asserts the *shape* rather than
    /// host-specific paths.
    #[test]
    fn windows_candidates_are_bash_exes_under_git_roots() {
        let candidates = windows_bash_candidates();
        assert!(
            candidates
                .iter()
                .all(|c| c.extension().is_some_and(|e| e == "exe")),
            "{candidates:?}"
        );
        assert!(
            candidates
                .iter()
                .all(|c| c.to_string_lossy().contains("Git")),
            "{candidates:?}"
        );
        // Each Git root contributes both bin/ and usr/bin/ variants, so the
        // count is even and non-empty whenever any root variable is set.
        assert_eq!(candidates.len() % 2, 0, "{candidates:?}");
        if !candidates.is_empty() {
            assert!(candidates.len() >= 2, "{candidates:?}");
        }
    }

    /// On this (non-Windows) host the resolver must yield plain `bash`, and a
    /// `Command` built from it must be runnable.
    #[test]
    fn bash_program_is_usable_on_posix() {
        if cfg!(windows) {
            return;
        }
        assert_eq!(bash_program(), PathBuf::from("bash"));
        let out = bash().arg("-c").arg("exit 0").output().unwrap();
        assert!(out.status.success());
    }

    /// The fallback predicate distinguishes the POSIX permission case. On
    /// Windows every direct-exec failure qualifies, which is what makes the
    /// walkthrough runner fall back instead of failing outright.
    #[test]
    fn fallback_predicate_handles_both_platforms() {
        let denied = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "not executable");
        let missing = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        assert!(needs_bash_fallback(&denied));
        if cfg!(windows) {
            // Windows cannot exec shebang scripts at all, so NotFound (or the
            // bad-exec-format error) must also fall back.
            assert!(needs_bash_fallback(&missing));
        } else {
            assert!(!needs_bash_fallback(&missing));
        }
    }

    /// `shell::command` must produce a runnable command for an ordinary
    /// program on every platform, so the `.cmd`-shim path never regresses the
    /// common case.
    #[test]
    fn command_runs_a_plain_program() {
        let out = command("echo", &["shim-probe".to_owned()])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("shim-probe"));
    }

    /// Names that already carry an extension never resolve through the shim
    /// path, so an exact path or an `.exe` always wins over a PATH search.
    ///
    /// The positive case is environment-dependent (nvm installs `npm.cmd` even
    /// on Linux so one tree serves both platforms), so this asserts the
    /// invariant rather than host specifics: any hit must be an actual
    /// `.cmd`/`.bat` file.
    #[test]
    fn batch_shim_lookup_only_matches_real_batch_files() {
        assert!(windows_batch_shim("node.exe").is_none());
        // Absolute and slash-containing names bypass the PATH search entirely.
        assert!(windows_batch_shim("/usr/bin/node").is_none());
        assert!(windows_batch_shim("./npm").is_none());
        for name in ["npm", "npx", "pnpm", "definitely-absent-tool"] {
            if let Some(found) = windows_batch_shim(name) {
                assert!(found.is_file(), "{found:?}");
                assert!(
                    matches!(
                        found.extension().and_then(|e| e.to_str()),
                        Some("cmd" | "bat")
                    ),
                    "{found:?}"
                );
            }
        }
    }
}

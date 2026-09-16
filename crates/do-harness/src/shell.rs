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

/// Whether a failed direct execution of a script means "run it through bash".
///
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

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
}

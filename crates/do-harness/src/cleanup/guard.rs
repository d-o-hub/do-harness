//! The safety guards that decide whether a build tree may be reclaimed.
//!
//! Every guard can only *refuse*. None of them can promote a candidate: the
//! merge proof does that. The split matters because a guard is a statement
//! about the present (is anything using this tree?) while the proof is a
//! statement about the past (was this branch merged?).

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use fs2::FileExt;

use super::Reason;
use super::discover::{Entry, git};

/// Which liveness probes this host could run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    /// Cargo's build lock and the process table were both checked.
    LockAndProcess,
    /// Only Cargo's build lock could be checked (no process probe on this host).
    LockOnly,
}

impl Probe {
    /// Wire spelling for the report.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LockAndProcess => "lock-and-process",
            Self::LockOnly => "lock-only",
        }
    }
}

/// Outcome of the present-tense guards for one worktree.
#[derive(Debug, Clone)]
pub struct Guarded {
    /// Liveness probes that ran.
    pub probe: Probe,
    /// The refusal reason, when a guard fired.
    pub refusal: Option<Reason>,
}

/// Runs every present-tense guard against one worktree.
///
/// `target` is the build tree selected for this candidate; `None` means the
/// caller already refused the candidate for a target-resolution reason.
#[must_use]
pub fn inspect(entry: &Entry, main: &Path, target: Option<&Path>, optimistic: bool) -> Guarded {
    let probe = if cfg!(target_os = "linux") {
        Probe::LockAndProcess
    } else {
        Probe::LockOnly
    };
    let refusal = refusal(entry, main, target, optimistic);
    Guarded { probe, refusal }
}

/// The first guard that refuses, in precedence order.
fn refusal(entry: &Entry, main: &Path, target: Option<&Path>, optimistic: bool) -> Option<Reason> {
    if entry.prunable.is_some() {
        return Some(Reason::Prunable);
    }
    if entry.locked.is_some() {
        return Some(Reason::Locked);
    }
    if let Some(target) = target {
        if is_protected(main, &entry.path, target) {
            return Some(Reason::Protected);
        }
    }
    if dirty(&entry.path) {
        return Some(Reason::Dirty);
    }
    target.and_then(|target| in_use(target, optimistic))
}

/// Whether `git status --porcelain` reports anything, including untracked files.
///
/// A status failure is treated as dirty: the guard must not read "cannot tell"
/// as "clean".
fn dirty(worktree: &Path) -> bool {
    match git(worktree, &["status", "--porcelain"]) {
        Ok(output) => !output.stdout.is_empty(),
        Err(_) => true,
    }
}

/// Whether a path is one this command may never delete.
fn is_protected(main: &Path, worktree: &Path, target: &Path) -> bool {
    if target
        .components()
        .any(|component| component.as_os_str() == ".do-harness")
    {
        return true;
    }
    if std::fs::symlink_metadata(target).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return true;
    }
    let Ok(canonical_target) = target.canonicalize() else {
        return false;
    };
    let Ok(canonical_main) = main.canonicalize() else {
        return true;
    };
    if canonical_target.starts_with(&canonical_main) {
        return true;
    }
    match worktree.canonicalize() {
        Ok(canonical_worktree) => {
            canonical_target.starts_with(&canonical_worktree)
                && canonical_target != canonical_worktree.join("target")
        }
        Err(_) => true,
    }
}

/// Runs the liveness probes, returning a refusal when the tree may be in use.
fn in_use(target: &Path, optimistic: bool) -> Option<Reason> {
    if lock_held(target) {
        return Some(Reason::InUse);
    }
    if cfg!(target_os = "linux") {
        return process_uses(target).then_some(Reason::InUse);
    }
    if optimistic {
        None
    } else {
        Some(Reason::InUseUnknown)
    }
}

/// Whether a Cargo build lock under `target` is held.
///
/// The probe can only refuse: a lock file that cannot be opened is treated as
/// held, and a missing lock file proves nothing. Cargo documents the build
/// directory layout as internal, so this is deliberately advisory.
fn lock_held(target: &Path) -> bool {
    lock_files(target).iter().any(|path| {
        match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => {
                let held = file.try_lock_exclusive().is_err();
                let _ = FileExt::unlock(&file);
                held
            }
            Err(_) => true,
        }
    })
}

/// Cargo's per-profile lock files: `<target>/.cargo-lock` and one level down.
fn lock_files(target: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let direct = target.join(".cargo-lock");
    if direct.exists() {
        files.push(direct);
    }
    if let Ok(entries) = std::fs::read_dir(target) {
        for entry in entries.flatten() {
            let candidate = entry.path().join(".cargo-lock");
            if candidate.exists() {
                files.push(candidate);
            }
        }
    }
    files
}

/// Whether a live compiler or test process works inside `target`.
#[cfg(target_os = "linux")]
fn process_uses(target: &Path) -> bool {
    const NAMES: [&str; 6] = [
        "cargo",
        "rustc",
        "rustdoc",
        "nextest",
        "sccache",
        "rust-analyzer",
    ];
    let Ok(canonical) = target.canonicalize() else {
        return true;
    };
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return true;
    };
    for process in processes.flatten() {
        let Ok(pid) = process.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
        let name = comm.trim();
        if !NAMES.contains(&name) {
            continue;
        }
        let Ok(cwd) = std::fs::read_link(format!("/proc/{pid}/cwd")) else {
            continue;
        };
        if cwd.starts_with(&canonical) {
            return true;
        }
    }
    false
}

/// Non-Linux hosts have no process probe; the caller reports `lock-only`.
#[cfg(not(target_os = "linux"))]
fn process_uses(_target: &Path) -> bool {
    false
}

/// Total size of the regular files under `dir`, following no symlinks.
#[must_use]
pub fn dir_size(dir: &Path) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            total = total.saturating_add(dir_size(&entry.path()));
        } else if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    total
}

/// Whether `path` is an ordinary directory (never a symlink to one).
#[must_use]
pub fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

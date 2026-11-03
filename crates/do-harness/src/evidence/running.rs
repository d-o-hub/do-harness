//! In-flight verify markers.
//!
//! A run that owns an evidence artifact holds a shared lock on a sibling
//! `<artifact>.running.lock` for its whole duration and publishes its pid,
//! start time and set in `<artifact>.running`. `status` probes the lock
//! exclusively to tell "a run is executing now" from "nobody has ever run this
//! set"; the OS drops the lock when the process dies, so a killed run leaves no
//! phantom running state. Shared locks keep concurrent evidence writers legal,
//! which the write-side chain/atomic-replace lock already supports.
//!
//! Liveness and metadata are separate files on purpose: Windows refuses to
//! truncate or write a region another handle has byte-range locked (os error
//! 33), so the file that carries the lock is never written after locking.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Retries for a transient lock held by the other side's probe (microseconds
/// at most), so a probe never reports a run that is not there.
const LOCK_RETRY_ATTEMPTS: u32 = 10;
const LOCK_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(10);
/// `status` probes are more frequent, so retry more briefly.
const PROBE_ATTEMPTS: u32 = 3;
const PROBE_DELAY: std::time::Duration = std::time::Duration::from_millis(5);

/// Sibling marker of an evidence artifact.
pub(crate) fn marker_path(evidence_path: &Path) -> PathBuf {
    evidence_path.with_extension("running")
}

/// Lock file that records a run's liveness for the same artifact.
fn lock_path(evidence_path: &Path) -> PathBuf {
    let mut path = marker_path(evidence_path).into_os_string();
    path.push(".lock");
    PathBuf::from(path)
}

/// Published metadata of an in-flight run.
///
/// Every field is optional so a reader that catches the marker between
/// creation and its first write still sees a running run rather than an error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunningInfo {
    /// Process that owns the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// Unix seconds when the run started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    /// Signal set the run belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<String>,
}

/// Shared lock held for the duration of a run. The lock file persists (like
/// the evidence write lock) and an unlocked one is simply ignored.
pub(crate) struct RunningGuard {
    info_path: PathBuf,
    file: std::fs::File,
}

impl RunningGuard {
    /// Marks the run as in flight for `set`.
    ///
    /// Concurrent runs on one set share the lock; they are already serialized
    /// by the evidence write lock.
    ///
    /// # Errors
    ///
    /// An I/O or containment error, including a marker path that crosses a
    /// symlink or a lock that stays contended after the retry budget.
    pub(crate) fn begin(root: &Path, evidence_path: &Path, set: Option<&str>) -> Result<Self> {
        let lock = crate::managed_path::evidence_path(root, &lock_path(evidence_path))?;
        let info_path = crate::managed_path::evidence_path(root, &marker_path(evidence_path))?;
        if let Some(parent) = lock.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let info = RunningInfo {
            pid: Some(std::process::id()),
            started_at: Some(unix_now()),
            set: set.map(ToOwned::to_owned),
        };
        // Published before the lock so a probe that observes the lock can
        // always read the metadata, and never rewritten while locked.
        crate::managed_path::write_atomic(&info_path, &serde_json::to_vec(&info)?)?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock)?;
        acquire_shared(&file)?;
        Ok(Self { info_path, file })
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
        // A concurrent run shares the info file; a failed removal only leaves
        // metadata that no live lock vouches for.
        let _ = std::fs::remove_file(&self.info_path);
    }
}

/// Reads an in-flight marker without blocking.
///
/// `Some` means a run holds the shared lock. A marker with no lock belongs to
/// a run that died and is ignored (the OS already released its lock).
pub(crate) fn inspect(root: &Path, evidence_path: &Path) -> Option<RunningInfo> {
    let lock = crate::managed_path::evidence_path(root, &lock_path(evidence_path)).ok()?;
    // Read+write: `LockFileEx` requires a handle with data access.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock)
        .ok()?;
    let info_path = crate::managed_path::evidence_path(root, &marker_path(evidence_path)).ok()?;
    let mut attempts = 0;
    loop {
        match fs2::FileExt::try_lock_exclusive(&file) {
            Ok(()) => {
                let _ = fs2::FileExt::unlock(&file);
                return None;
            }
            Err(err) if is_contended(&err) => {
                if attempts < PROBE_ATTEMPTS {
                    attempts += 1;
                    std::thread::sleep(PROBE_DELAY);
                } else {
                    return Some(read_info(&info_path));
                }
            }
            Err(_) => return None,
        }
    }
}

/// Human summary of an in-flight run.
pub(crate) fn describe(set: Option<&str>, info: &RunningInfo) -> String {
    let set = set.or(info.set.as_deref()).unwrap_or("(default)");
    let mut summary = format!("verify is in flight for signal set '{set}'");
    append_details(&mut summary, info);
    summary
}

/// Appends `(pid <n>, since <unix>)` when the marker carries them.
fn append_details(summary: &mut String, info: &RunningInfo) {
    let mut details = Vec::new();
    if let Some(pid) = info.pid {
        details.push(format!("pid {pid}"));
    }
    if let Some(since) = info.started_at {
        details.push(format!("since {since}"));
    }
    if !details.is_empty() {
        let _ = write!(summary, " ({})", details.join(", "));
    }
}

/// Acquires a shared lock on `file`, retrying transient probe contention.
///
/// Fully qualified: Rust's inherent `File::try_lock_*` shadows the fs2 trait
/// and reports a different error type.
fn acquire_shared(file: &std::fs::File) -> Result<()> {
    let mut attempts = 0;
    loop {
        match fs2::FileExt::try_lock_shared(file) {
            Ok(()) => return Ok(()),
            Err(err) if is_contended(&err) => {
                if attempts < LOCK_RETRY_ATTEMPTS {
                    attempts += 1;
                    std::thread::sleep(LOCK_RETRY_DELAY);
                } else {
                    return Err(err.into());
                }
            }
            Err(err) => return Err(err.into()),
        }
    }
}

/// Whether `err` means "another handle holds the lock".
///
/// Windows reports contention as a raw `ERROR_LOCK_VIOLATION` with no standard
/// `ErrorKind`, so the platform's contended error from fs2 is compared too.
fn is_contended(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::WouldBlock
        || err.raw_os_error() == fs2::lock_contended_error().raw_os_error()
}

/// Tolerant read: an unparsable marker is still a running run.
fn read_info(path: &Path) -> RunningInfo {
    crate::managed_path::read_evidence(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Unix seconds, saturating instead of panicking on a pre-epoch clock.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

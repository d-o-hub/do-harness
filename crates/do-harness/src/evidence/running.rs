//! In-flight verify markers.
//!
//! Each run owns a uniquely named marker file in a sibling
//! `<artifact>.running/` directory and holds a shared lock on it for the whole
//! run. `status` probes those files exclusively: a held lock means "a run is
//! executing now", and a leftover without one belongs to a run that died (the
//! OS released its lock with the process), so it never reports phantom state.
//!
//! No two runs ever touch the same marker file. Windows refuses to truncate or
//! write a region that another handle has byte-range locked (os error 33), so a
//! single shared marker file would break the concurrent `verify` runs the
//! evidence write lock already legalizes.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Attempts before `status` treats a momentarily held probe as a live run.
const PROBE_ATTEMPTS: u32 = 3;
const PROBE_DELAY: std::time::Duration = std::time::Duration::from_millis(5);
/// Suffixes tried when two runs in one second collide on a marker name.
const NAME_ATTEMPTS: u32 = 8;

/// Directory holding one marker file per in-flight run of an artifact.
pub(crate) fn marker_dir(evidence_path: &Path) -> PathBuf {
    evidence_path.with_extension("running")
}

/// Published metadata of an in-flight run.
///
/// Every field is optional so a marker written by an older build still parses.
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

/// Shared lock held for the duration of a run; the marker file is removed
/// when the guard drops, and an orphan left by a crash is pruned by the next
/// run of the same artifact.
pub(crate) struct RunningGuard {
    path: PathBuf,
    file: std::fs::File,
}

impl RunningGuard {
    /// Publishes a marker for this run and locks it.
    ///
    /// # Errors
    ///
    /// An I/O or containment error: a marker directory that cannot be created
    /// or a marker path that crosses a symlink or workspace boundary.
    pub(crate) fn begin(root: &Path, evidence_path: &Path, set: Option<&str>) -> Result<Self> {
        let dir = managed_dir(root, &marker_dir(evidence_path))?;
        std::fs::create_dir_all(&dir)?;
        prune_stale(&dir);
        let info = RunningInfo {
            pid: Some(std::process::id()),
            started_at: Some(unix_now()),
            set: set.map(ToOwned::to_owned),
        };
        let bytes = serde_json::to_vec(&info)?;
        let mut attempt = 0;
        loop {
            let path = dir.join(marker_name(&info, attempt));
            match std::fs::OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&path)
            {
                Ok(mut file) => {
                    // Writes are safe here: this handle is the file's only
                    // owner and holds the lock it takes next.
                    file.write_all(&bytes)?;
                    file.flush()?;
                    acquire_shared(&file)?;
                    return Ok(Self { path, file });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    attempt += 1;
                    if attempt >= NAME_ATTEMPTS {
                        return Err(err.into());
                    }
                }
                Err(err) => return Err(err.into()),
            }
        }
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Reads the metadata of any live marker for this artifact.
///
/// Returns `None` when no run holds a lock, which is also what a marker left
/// by a killed run looks like.
pub(crate) fn inspect(root: &Path, evidence_path: &Path) -> Option<RunningInfo> {
    let dir = managed_dir(root, &marker_dir(evidence_path)).ok()?;
    let mut markers: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    markers.sort();
    markers.into_iter().find_map(|path| probe(&path))
}

/// Human summary of an in-flight run.
pub(crate) fn describe(set: Option<&str>, info: &RunningInfo) -> String {
    let set = set.or(info.set.as_deref()).unwrap_or("(default)");
    let mut summary = format!("verify is in flight for signal set '{set}'");
    append_details(&mut summary, info);
    summary
}

/// Returns the metadata of a marker whose lock is held, or `None`.
fn probe(path: &Path) -> Option<RunningInfo> {
    // Read+write: `LockFileEx` requires a handle with data access.
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .ok()?;
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
                    return Some(read_info(path));
                }
            }
            Err(_) => return None,
        }
    }
}

/// Best-effort removal of markers no live run holds.
fn prune_stale(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.filter_map(|entry| entry.ok().map(|e| e.path())) {
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        else {
            continue;
        };
        if fs2::FileExt::try_lock_exclusive(&file).is_ok() {
            let _ = fs2::FileExt::unlock(&file);
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Marker directory, contained when it lives inside the workspace.
///
/// `managed_path::within` rather than `evidence_path`: the marker is a
/// directory, and an explicit external evidence destination keeps its own
/// directory just like the artifact does.
fn managed_dir(root: &Path, dir: &Path) -> anyhow::Result<PathBuf> {
    if dir.starts_with(root) {
        crate::managed_path::within(root, dir)
    } else {
        Ok(dir.to_path_buf())
    }
}

/// Marker file name: owner identity plus a collision suffix.
fn marker_name(info: &RunningInfo, attempt: u32) -> String {
    let pid = info.pid.unwrap_or(0);
    let started = info.started_at.unwrap_or(0);
    if attempt == 0 {
        format!("{pid}-{started}.json")
    } else {
        format!("{pid}-{started}-{attempt}.json")
    }
}

/// Appends `(pid <n>, since <unix>)` when the marker carries them.
fn append_details(summary: &mut String, info: &RunningInfo) {
    let mut details = Vec::new();
    if let Some(pid) = info.pid {
        details.push(format!("pid {pid}"));
    }
    if let Some(started_at) = info.started_at {
        details.push(format!("since {started_at}"));
    }
    if !details.is_empty() {
        let _ = write!(summary, " ({})", details.join(", "));
    }
}

/// Acquires a shared lock on `file`, retrying a transient probe.
///
/// Fully qualified: Rust's inherent `File::try_lock_*` shadows the fs2 trait
/// (and post-dates the workspace MSRV), and reports a different error type.
fn acquire_shared(file: &std::fs::File) -> Result<()> {
    let mut attempts = 0;
    loop {
        match fs2::FileExt::try_lock_shared(file) {
            Ok(()) => return Ok(()),
            Err(err) if is_contended(&err) => {
                if attempts < PROBE_ATTEMPTS {
                    attempts += 1;
                    std::thread::sleep(PROBE_DELAY);
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

//! Offline verification timeline over saved evidence attempts (issue #647).
//!
//! The timeline is a *reading* surface: it never re-runs a sensor, never opens
//! the network, and never copies raw sensor output, argv, or transcripts into
//! the page. Each attempt keeps a link to its own saved artifact, so the
//! evidence stays the source of truth and the report stays reproducible from
//! the same frozen inputs.
//!
//! Attempt order is the order the caller supplied (`--evidence` repeated):
//! independently saved artifacts carry no reliable total order, so the page
//! labels it as supplied rather than inventing one from timestamps.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sha2::Digest as _;

mod render;
mod session;
#[cfg(test)]
mod tests;

/// Schema window the renderer accepts, read from the evidence contract so a new
/// schema version cannot drift from this surface.
const MIN_SCHEMA: u32 = crate::evidence::MIN_CURRENT_SCHEMA_VERSION;
const MAX_SCHEMA: u32 = crate::evidence::EVIDENCE_SCHEMA_VERSION;
/// Version of the timeline's own JSON document, independent of the evidence schema.
const TIMELINE_SCHEMA: u32 = 1;

/// Output format for `timeline`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TimelineFormat {
    /// One self-contained HTML page that opens offline.
    Html,
    /// The same timeline as a JSON document.
    Json,
}

/// One saved evidence artifact, in the order the caller supplied it.
pub(super) struct Attempt {
    /// Path as supplied (rendered as the artifact link).
    pub(super) path: PathBuf,
    /// SHA-256 of the artifact bytes (`sha256:…`), the identity a captured
    /// status snapshot binds to.
    pub(super) sha256: String,
    /// Parsed artifact.
    pub(super) document: crate::evidence::EvidenceDocument,
    /// Explicit capture-time session attribution, when available.
    session: Option<session::SessionBinding>,
}

/// A captured `status --format json` observation.
pub(super) struct Snapshot {
    /// Path as supplied.
    pub(super) path: PathBuf,
    /// When the observation was captured (the snapshot file's modification
    /// time), in Unix seconds.
    pub(super) observed_at: i64,
    /// Freshness state (`green` | `red` | `stale` | `missing` | `running`).
    pub(super) state: String,
    /// Machine-readable reason from the status contract.
    pub(super) reason: String,
    /// Requested signal set recorded by the snapshot.
    pub(super) set: Option<String>,
    /// Recorded workspace fingerprint.
    pub(super) workspace_fingerprint: Option<String>,
    /// Recorded policy fingerprint.
    pub(super) policy_fingerprint: Option<String>,
    /// Digest of the artifact the snapshot judged, when it recorded one.
    pub(super) artifact_sha256: Option<String>,
}

/// How one snapshot bound to the supplied attempts.
pub(super) struct Binding {
    /// The captured observation.
    pub(super) snapshot: Snapshot,
    /// Index of the attempt it describes, when the fingerprints identify one.
    pub(super) attempt: Option<usize>,
    /// Why it is unbound or ambiguous; `None` when it bound cleanly.
    pub(super) note: Option<String>,
}

/// The rendered timeline: attempts, their freshness observations, and gaps.
pub(super) struct Timeline {
    /// Attempts in supplied order.
    pub(super) attempts: Vec<Attempt>,
    /// Captured snapshots and how each bound.
    pub(super) bindings: Vec<Binding>,
    /// Explicit coverage gaps (unexplained chain breaks, invalidated runs).
    pub(super) gaps: Vec<String>,
}

/// Renders a timeline from the supplied artifacts.
///
/// # Errors
///
/// Returns an error when an artifact or snapshot cannot be read or parsed, when
/// an artifact's evidence schema is outside the supported window, when the
/// artifacts come from different workspaces, or when the output cannot be
/// written.
pub fn run(
    root: &Path,
    attempts: &[PathBuf],
    snapshots: &[PathBuf],
    format: TimelineFormat,
    output: Option<&Path>,
) -> Result<()> {
    if attempts.is_empty() {
        bail!("timeline needs at least one --evidence artifact");
    }
    let attempts: Vec<Attempt> = attempts
        .iter()
        .map(|path| load_attempt(&resolve(root, path)))
        .collect::<Result<Vec<_>>>()?;
    check_one_workspace(&attempts)?;
    session::check_attempt_ids(&attempts)?;
    let snapshots: Vec<Snapshot> = snapshots
        .iter()
        .map(|path| load_snapshot(&resolve(root, path)))
        .collect::<Result<Vec<_>>>()?;
    let bindings = bind(&snapshots, &attempts);
    let mut gaps = chain_gaps(&attempts);
    gaps.extend(binding_gaps(&bindings));
    let timeline = Timeline {
        attempts,
        bindings,
        gaps,
    };
    match format {
        TimelineFormat::Html => {
            let page = render::html(&timeline);
            write_output(root, output, page.as_bytes(), "timeline.html")
        }
        TimelineFormat::Json => {
            let document = render::json(&timeline)?;
            write_output(root, output, document.as_bytes(), "-")
        }
    }
}

/// Writes `bytes` to `output` (or the format's default, with `-` meaning stdout).
fn write_output(root: &Path, output: Option<&Path>, bytes: &[u8], default: &str) -> Result<()> {
    let target = output.map_or_else(|| PathBuf::from(default), |path| resolve(root, path));
    if target.as_os_str() == "-" {
        use std::io::Write;
        std::io::stdout().write_all(bytes)?;
        return Ok(());
    }
    if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    std::fs::write(&target, bytes).with_context(|| format!("cannot write {}", target.display()))?;
    println!("wrote {}", target.display());
    Ok(())
}

/// Resolves a caller-supplied path against the workspace root, so a fixture or
/// a script can name artifacts relative to the checkout.
fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// Reads and validates one evidence artifact.
fn load_attempt(path: &Path) -> Result<Attempt> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("cannot read evidence artifact {}", path.display()))?;
    let document: crate::evidence::EvidenceDocument = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} is not an evidence artifact", path.display()))?;
    if !(MIN_SCHEMA..=MAX_SCHEMA).contains(&document.schema_version) {
        bail!(
            "{} records evidence schema v{}; timeline renders v{MIN_SCHEMA}..=v{MAX_SCHEMA} — \
             re-run `verify --record` to produce a current artifact",
            path.display(),
            document.schema_version
        );
    }
    let sha256 = format!("sha256:{}", hex::encode(sha2::Sha256::digest(&bytes)));
    let session = session::read(path, &sha256, &document)?;
    Ok(Attempt {
        path: path.to_path_buf(),
        sha256,
        document,
        session,
    })
}

/// Refuses artifacts from different workspaces: their fingerprints are not
/// comparable, so a merged timeline would invent continuity.
fn check_one_workspace(attempts: &[Attempt]) -> Result<()> {
    let Some(first) = attempts.first() else {
        return Ok(());
    };
    for attempt in &attempts[1..] {
        if attempt.document.root != first.document.root {
            bail!(
                "{} records root {} but {} records root {}; supply attempts from one workspace",
                first.path.display(),
                first.document.root,
                attempt.path.display(),
                attempt.document.root
            );
        }
    }
    Ok(())
}

/// Reads one captured `status --format json` snapshot.
fn load_snapshot(path: &Path) -> Result<Snapshot> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("cannot read status snapshot {}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} is not a status snapshot", path.display()))?;
    let recorded_artifact = value.get("evidence").and_then(|side| side.get("artifact"));
    let recorded_observed_at = recorded_artifact
        .and_then(|artifact| artifact.get("observed_at"))
        .and_then(Value::as_i64);
    let artifact_sha256 = recorded_artifact
        .and_then(|artifact| artifact.get("sha256"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let observed_at = recorded_observed_at.unwrap_or_else(|| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |since| {
                i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
            })
    });
    let state = value
        .get("state")
        .and_then(Value::as_str)
        .with_context(|| {
            format!(
                "{} has no `state` field; capture it with `status --format json`",
                path.display()
            )
        })?;
    let evidence = value.get("evidence");
    Ok(Snapshot {
        path: path.to_path_buf(),
        observed_at,
        state: state.to_owned(),
        reason: value
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        set: evidence
            .and_then(|side| side.get("signal_set"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        workspace_fingerprint: evidence
            .and_then(|side| side.get("workspace_fingerprint"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        policy_fingerprint: evidence
            .and_then(|side| side.get("policy_fingerprint"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        artifact_sha256,
    })
}

/// Binds each snapshot to the artifact it judged.
///
/// The artifact digest is the identity: a snapshot records the exact bytes it
/// observed, so the binding is exact even when two attempts share input
/// fingerprints (a clean tree hashes the same however its files changed).
/// Snapshots captured before the digest was recorded fall back to the recorded
/// fingerprints and say so; a snapshot that matches nothing, or matches
/// several attempts, is reported unbound instead of guessed at.
fn bind(snapshots: &[Snapshot], attempts: &[Attempt]) -> Vec<Binding> {
    snapshots
        .iter()
        .map(|snapshot| {
            if let Some(digest) = &snapshot.artifact_sha256 {
                let matches: Vec<usize> = attempts
                    .iter()
                    .enumerate()
                    .filter(|(_, attempt)| &attempt.sha256 == digest)
                    .map(|(index, _)| index)
                    .collect();
                return match matches.as_slice() {
                    [index] => Binding {
                        snapshot: clone_snapshot(snapshot),
                        attempt: Some(*index),
                        note: None,
                    },
                    [] => Binding {
                        snapshot: clone_snapshot(snapshot),
                        attempt: None,
                        note: Some(format!(
                            "the snapshot judges artifact {digest}, which none of the supplied \
                             attempts supplies"
                        )),
                    },
                    _ => Binding {
                        snapshot: clone_snapshot(snapshot),
                        attempt: None,
                        note: Some(format!(
                            "{} supplied attempts carry the same digest",
                            matches.len()
                        )),
                    },
                };
            }
            let matches: Vec<usize> = attempts
                .iter()
                .enumerate()
                .filter(|(_, attempt)| fingerprint_match(snapshot, &attempt.document))
                .map(|(index, _)| index)
                .collect();
            match matches.as_slice() {
                [index] => Binding {
                    snapshot: clone_snapshot(snapshot),
                    attempt: Some(*index),
                    note: Some(
                        "bound by recorded fingerprints: the snapshot carries no artifact digest"
                            .to_owned(),
                    ),
                },
                [] => Binding {
                    snapshot: clone_snapshot(snapshot),
                    attempt: None,
                    note: Some(
                        "no supplied attempt records these fingerprints; the observation is unbound"
                            .to_owned(),
                    ),
                },
                _ => Binding {
                    snapshot: clone_snapshot(snapshot),
                    attempt: None,
                    note: Some(format!(
                        "{} supplied attempts record these fingerprints, and the snapshot carries \
                         no artifact digest, so the observation is ambiguous",
                        matches.len()
                    )),
                },
            }
        })
        .collect()
}

/// Clones the snapshot fields the renderer needs.
fn clone_snapshot(snapshot: &Snapshot) -> Snapshot {
    Snapshot {
        path: snapshot.path.clone(),
        observed_at: snapshot.observed_at,
        state: snapshot.state.clone(),
        reason: snapshot.reason.clone(),
        set: snapshot.set.clone(),
        workspace_fingerprint: snapshot.workspace_fingerprint.clone(),
        policy_fingerprint: snapshot.policy_fingerprint.clone(),
        artifact_sha256: snapshot.artifact_sha256.clone(),
    }
}

/// Whether a snapshot observed exactly this artifact's recorded inputs.
fn fingerprint_match(snapshot: &Snapshot, document: &crate::evidence::EvidenceDocument) -> bool {
    if snapshot.set.as_deref() != document.signal_set.as_deref() {
        return false;
    }
    match (
        &snapshot.workspace_fingerprint,
        &snapshot.policy_fingerprint,
    ) {
        (Some(workspace), Some(policy)) => {
            workspace == &document.workspace_fingerprint && policy == &document.policy_fingerprint
        }
        // `missing`/`running` snapshots carry no evidence side: nothing to bind.
        _ => false,
    }
}

/// Chain and invalidation gaps between consecutive supplied attempts.
fn chain_gaps(attempts: &[Attempt]) -> Vec<String> {
    let mut gaps = Vec::new();
    for (index, attempt) in attempts.iter().enumerate() {
        if let Some(reason) = &attempt.document.invalidated_reason {
            gaps.push(format!(
                "attempt {} ({}) was invalidated during the run: {reason}",
                index + 1,
                attempt.path.display()
            ));
        }
        if attempt.document.sensors.is_empty() {
            gaps.push(format!(
                "attempt {} ({}) records no sensor results",
                index + 1,
                attempt.path.display()
            ));
        }
        if index == 0 {
            continue;
        }
        let previous = &attempts[index - 1];
        let expected = Some(previous.document.chain_hash.as_str());
        if attempt.document.prev_hash.as_deref() != expected {
            gaps.push(format!(
                "attempt {} ({}) does not follow attempt {} ({}) in the evidence chain",
                index + 1,
                attempt.path.display(),
                index,
                previous.path.display()
            ));
        }
    }
    gaps
}

/// Gaps for snapshots that could not be bound to one attempt.
fn binding_gaps(bindings: &[Binding]) -> Vec<String> {
    bindings
        .iter()
        .filter_map(|binding| {
            binding.note.as_ref().map(|note| {
                format!(
                    "status snapshot {}: {note}",
                    binding.snapshot.path.display()
                )
            })
        })
        .collect()
}

/// Every observation bound to one attempt, in supplied order.
///
/// Freshness is time-dependent: the same artifact is green when it was just
/// recorded, stale after an edit, and red again when a rerun reproduces the
/// failure. Rendering the whole sequence keeps that history instead of
/// collapsing it to the last snapshot the caller happened to pass.
pub(super) fn observations_for(timeline: &Timeline, index: usize) -> Vec<&Binding> {
    timeline
        .bindings
        .iter()
        .filter(|binding| binding.attempt == Some(index))
        .collect()
}

/// Every sensor row of one attempt, in artifact order.
pub(super) fn sensor_rows(document: &crate::evidence::EvidenceDocument) -> Vec<Value> {
    document
        .sensors
        .iter()
        .map(|sensor| {
            json!({
                "name": sensor.name,
                "verdict": sensor.verdict,
                "exit_code": sensor.exit_code,
                "duration_ms": sensor.duration_ms,
                "recorded": sensor.recorded,
            })
        })
        .collect()
}

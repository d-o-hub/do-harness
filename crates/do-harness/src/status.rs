//! `do-harness status`: cheap evidence freshness inspection.
//!
//! Status never executes a sensor. It loads the evidence artifact for the
//! requested signal set, recomputes the current workspace and policy
//! fingerprints, and reports one of `green`, `red`, `stale`, or `missing`:
//!
//! - `green`: passing evidence covers the requested set and matches the
//!   current workspace and policy.
//! - `red`: matching evidence exists and contains required failures.
//! - `stale`: evidence exists but no longer applies (workspace or policy
//!   changed).
//! - `missing`: no qualifying evidence exists.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::evidence::EvidenceDocument;
use crate::report::Format;
use crate::{CliError, config};

mod locate;

use locate::{Coverage, evidence_paths, find_covering_evidence};

/// Freshness states reported by `status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceState {
    /// Current passing evidence covers the requested set.
    Green,
    /// Current matching evidence contains required failures.
    Red,
    /// Evidence exists but no longer applies to workspace/policy.
    Stale,
    /// No qualifying evidence exists.
    Missing,
}

/// Evidence side of the status comparison.
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceSide {
    /// Signal set the evidence was recorded for.
    pub signal_set: Option<String>,
    /// Aggregate verdict (`pass` | `fail`).
    pub verdict: String,
    /// Recorded workspace fingerprint.
    pub workspace_fingerprint: String,
    /// Recorded policy fingerprint.
    pub policy_fingerprint: String,
    /// Failed sensor names.
    pub failed: Vec<String>,
}

/// Current side of the status comparison.
#[derive(Debug, Clone, Serialize)]
pub struct CurrentSide {
    /// Current workspace fingerprint.
    pub workspace_fingerprint: String,
    /// Current policy fingerprint.
    pub policy_fingerprint: String,
    /// Failed sensors when the state is red.
    pub failed: Vec<String>,
    /// Sensors with changed or unestablished canonical inputs.
    pub input_mismatches: Vec<String>,
}

/// Machine-readable status report (the `--format json` contract).
#[derive(Debug, Clone, Serialize)]
pub struct StatusReport {
    /// Freshness state (`green` | `red` | `stale` | `missing`).
    pub state: EvidenceState,
    /// Requested signal set; `None` means the legacy default artifact.
    pub set: Option<String>,
    /// Machine-readable reason (`current`, `failures`, `workspace_changed`,
    /// `policy_changed`, `insufficient_coverage`, `no_evidence`,
    /// `legacy_schema`, `set_mismatch`, `unreadable`).
    pub reason: String,
    /// Evidence side; absent when no qualifying artifact exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceSide>,
    /// Current fingerprint side.
    pub current: CurrentSide,
    /// Required sensors the evidence does not cover; empty unless the reason
    /// is `insufficient_coverage`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing_sensors: Vec<String>,
    /// Command that re-records the evidence. Absent for states whose remedy
    /// is not a `verify --record` run (`current`, `failures`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery: Option<String>,
}

/// Default evidence path for `verify --set <set>` runs.
#[must_use]
pub fn default_path_for_set(root: &Path, set: &str) -> PathBuf {
    root.join(format!(".do-harness/evidence.{set}.json"))
}

/// Computes the status report without executing any sensor.
///
/// `evidence_override` replaces the default artifact lookup. Exit-code
/// mapping is left to the caller: green is success, red/stale/missing are
/// verification failures, usage problems are errors.
///
/// # Errors
///
/// Returns an error when the config cannot load or the set is unknown.
pub fn status_report(
    root: &Path,
    cfg: &config::Config,
    config_bytes: Option<&[u8]>,
    set: Option<&str>,
    evidence_override: Option<&Path>,
) -> anyhow::Result<StatusReport> {
    let facts = CurrentFacts::compute(root, cfg, config_bytes, set)?;
    let paths = evidence_paths(root, set, evidence_override);
    let (document, covering) = find_covering_evidence(&paths, set, &facts.required);
    let Some(document) = document else {
        return Ok(missing_report(set, &paths, covering, facts.side));
    };
    Ok(decide(set, &facts, &document))
}

/// Current fingerprint facts a status decision compares evidence against.
struct CurrentFacts {
    /// Current fingerprint side of the report.
    side: CurrentSide,
    /// Required sensor names for the requested set and change.
    required: Vec<String>,
    /// Current workspace fingerprint.
    workspace: String,
    /// Current policy fingerprint.
    policy: String,
    /// Current set-free config fingerprint.
    config: String,
    manifest: crate::provenance::InputManifest,
}

impl CurrentFacts {
    /// Recomputes fingerprints and the required sensor list (no sensors run).
    fn compute(
        root: &Path,
        cfg: &config::Config,
        config_bytes: Option<&[u8]>,
        set: Option<&str>,
    ) -> anyhow::Result<Self> {
        let candidates = crate::signals::candidate_specs(cfg, set)?;
        let changed = crate::changes::discover(root);
        let selection = crate::applicability::select(&candidates, &changed);
        let required = selection.selected_names();
        let fingerprints =
            crate::fingerprint::for_run(root, cfg, config_bytes, set, &candidates, &changed);
        let workspace = fingerprints.workspace;
        let policy = fingerprints.policy;
        let config = fingerprints.config;
        let manifest = crate::provenance::InputManifest::capture(root, &candidates, config_bytes);
        Ok(Self {
            side: CurrentSide {
                workspace_fingerprint: workspace.clone(),
                policy_fingerprint: policy.clone(),
                failed: Vec::new(),
                input_mismatches: manifest
                    .sensors
                    .iter()
                    .filter(|(_, digest)| digest.is_none())
                    .map(|(name, _)| name.clone())
                    .collect(),
            },
            required,
            workspace,
            policy,
            config,
            manifest,
        })
    }
}

/// Builds the missing-evidence report from the search outcome.
fn missing_report(
    set: Option<&str>,
    paths: &[PathBuf],
    covering: Coverage,
    current: CurrentSide,
) -> StatusReport {
    let reason = if covering == Coverage::Legacy {
        "legacy_schema"
    } else if covering == Coverage::SetMismatch {
        "set_mismatch"
    } else if paths.iter().any(|p| p.exists()) {
        "unreadable"
    } else {
        "no_evidence"
    };
    report(
        EvidenceState::Missing,
        set,
        reason,
        None,
        current,
        Vec::new(),
    )
}

/// Builds a status report, deriving the recovery hint from the reason.
fn report(
    state: EvidenceState,
    set: Option<&str>,
    reason: &str,
    evidence: Option<EvidenceSide>,
    current: CurrentSide,
    missing_sensors: Vec<String>,
) -> StatusReport {
    StatusReport {
        state,
        set: set.map(ToOwned::to_owned),
        reason: reason.to_owned(),
        evidence,
        current,
        missing_sensors,
        recovery: recovery_hint(reason, set),
    }
}

/// The `verify` invocation that re-records evidence for the requested set.
fn recovery_command(set: Option<&str>) -> String {
    set.map_or_else(
        || "do-harness verify --record".to_owned(),
        |set| format!("do-harness verify --set {set} --record"),
    )
}

/// Recovery hint for states whose remedy is re-recording the evidence.
fn recovery_hint(reason: &str, set: Option<&str>) -> Option<String> {
    matches!(
        reason,
        "no_evidence" | "insufficient_coverage" | "policy_changed"
    )
    .then(|| recovery_command(set))
}

/// Name changed or unknown input identities without exposing their values.
fn current_input_side(facts: &CurrentFacts, document: &EvidenceDocument) -> CurrentSide {
    let mut current_side = facts.side.clone();
    current_side.input_mismatches = facts
        .manifest
        .sensors
        .iter()
        .filter(|(name, current)| {
            current.is_none()
                || document
                    .input_manifest
                    .as_ref()
                    .and_then(|m| m.sensors.get(*name))
                    != Some(*current)
        })
        .map(|(name, _)| name.clone())
        .collect();
    current_side
}

/// Decides green/red/stale/missing for one evidence document.
///
/// Same-set evidence decides green/red/stale against the full policy;
/// stronger cross-set evidence decides green when it shares the config
/// fingerprint and covers every required sensor — even when sensors outside
/// the requested contract failed.
fn decide(set: Option<&str>, facts: &CurrentFacts, document: &EvidenceDocument) -> StatusReport {
    let current_side = current_input_side(facts, document);
    let evidence = evidence_side(document);
    let (passed, executed) = recorded_sensors(document);
    let same_set = document.signal_set.as_deref() == set;
    let covers = covers(facts, same_set, &executed, &passed);
    let workspace_match = document.workspace_fingerprint == facts.workspace;
    let policy_match = document.policy_fingerprint == facts.policy;
    let config_match = document.config_fingerprint == facts.config;

    let provenance_ok = document.input_manifest.as_ref().is_some_and(|manifest| {
        manifest.established()
            && facts.manifest.established()
            && manifest.git_revision == facts.manifest.git_revision
            && manifest.branch == facts.manifest.branch
            && facts
                .required
                .iter()
                .all(|name| manifest.sensors.get(name) == facts.manifest.sensors.get(name))
    }) && document.verify_chain(document.prev_hash.as_deref());
    let policy_ok = if same_set { policy_match } else { config_match };
    if covers && workspace_match && policy_ok && provenance_ok {
        if let Some(reason) = &document.invalidated_reason {
            let mut current = current_side.clone();
            current.failed.clone_from(&evidence.failed);
            return report(
                EvidenceState::Red,
                set,
                reason,
                Some(evidence),
                current,
                Vec::new(),
            );
        }
        if same_set && document.summary.verdict == "fail" {
            let mut current = current_side.clone();
            current.failed.clone_from(&evidence.failed);
            return report(
                EvidenceState::Red,
                set,
                "failures",
                Some(evidence),
                current,
                Vec::new(),
            );
        }
        return report(
            EvidenceState::Green,
            set,
            "current",
            Some(evidence),
            current_side,
            Vec::new(),
        );
    }
    if !covers {
        return report(
            EvidenceState::Missing,
            set,
            "insufficient_coverage",
            Some(evidence),
            current_side,
            missing_sensors(facts, same_set, &executed, &passed),
        );
    }
    report(
        EvidenceState::Stale,
        set,
        stale_reason(workspace_match, policy_ok),
        Some(evidence),
        current_side,
        Vec::new(),
    )
}

/// The evidence side of the status comparison.
fn evidence_side(document: &EvidenceDocument) -> EvidenceSide {
    EvidenceSide {
        signal_set: document.signal_set.clone(),
        verdict: document.summary.verdict.clone(),
        workspace_fingerprint: document.workspace_fingerprint.clone(),
        policy_fingerprint: document.policy_fingerprint.clone(),
        failed: document
            .sensors
            .iter()
            .filter(|s| s.verdict == "fail")
            .map(|s| s.name.clone())
            .collect(),
    }
}

/// Recorded sensor names: `(passed, executed)`.
fn recorded_sensors(document: &EvidenceDocument) -> (Vec<&str>, Vec<&str>) {
    let passed = document
        .sensors
        .iter()
        .filter(|s| s.verdict == "pass")
        .map(|s| s.name.as_str())
        .collect();
    let executed = document.sensors.iter().map(|s| s.name.as_str()).collect();
    (passed, executed)
}

/// Whether the recorded sensors cover every required name.
///
/// Same-set coverage counts executed sensors (a failing run still describes
/// its contract); cross-set coverage counts only passed ones.
fn covers(facts: &CurrentFacts, same_set: bool, executed: &[&str], passed: &[&str]) -> bool {
    let recorded = if same_set { executed } else { passed };
    facts
        .required
        .iter()
        .all(|name| recorded.iter().any(|p| p == name))
}

/// Required sensors the evidence does not record.
fn missing_sensors(
    facts: &CurrentFacts,
    same_set: bool,
    executed: &[&str],
    passed: &[&str],
) -> Vec<String> {
    let recorded = if same_set { executed } else { passed };
    facts
        .required
        .iter()
        .filter(|name| !recorded.iter().any(|p| p == *name))
        .cloned()
        .collect()
}

/// Staleness reason when the fingerprints no longer match.
fn stale_reason(workspace_match: bool, policy_ok: bool) -> &'static str {
    if workspace_match && policy_ok {
        "inputs_changed_or_unknown"
    } else if workspace_match {
        "policy_changed"
    } else {
        "workspace_changed"
    }
}

/// Runs the `status` subcommand: fingerprint comparison, no sensor execution.
pub(crate) async fn run(
    root: &Path,
    config_path: Option<&Path>,
    set: Option<String>,
    evidence: Option<PathBuf>,
    format: Format,
) -> std::result::Result<(), CliError> {
    let (cfg, config_bytes) = config::load_raw(root, config_path)
        .await
        .map_err(CliError::Usage)?;
    let report = status_report(
        root,
        &cfg,
        config_bytes.as_deref(),
        set.as_deref(),
        evidence.as_deref(),
    )
    .map_err(CliError::Usage)?;
    match format {
        Format::Text => print_text(&report),
        Format::Json => println!(
            "{}",
            crate::report::json_pretty(&report).map_err(CliError::Usage)?
        ),
    }
    match report.state {
        EvidenceState::Green => Ok(()),
        EvidenceState::Red | EvidenceState::Stale | EvidenceState::Missing => {
            Err(CliError::Verify(anyhow::anyhow!(failure_summary(&report))))
        }
    }
}

/// Failure summary for the error stream, naming unusable sensors and the
/// command that clears the state.
fn failure_summary(report: &StatusReport) -> String {
    let state = match report.state {
        EvidenceState::Red => "red",
        EvidenceState::Stale => "stale",
        _ => "missing",
    };
    let mut summary = format!("evidence is {state} ({})", report.reason);
    if !report.missing_sensors.is_empty() {
        let _ = write!(
            summary,
            "; missing sensors: {}",
            report.missing_sensors.join(", ")
        );
    }
    if let Some(recovery) = &report.recovery {
        let _ = write!(summary, "\n  → recover: {recovery}");
    }
    summary
}

/// Prints the human-readable one-line status plus actionable detail.
fn print_text(report: &StatusReport) {
    let set = report.set.as_deref().unwrap_or("(default)");
    match report.state {
        EvidenceState::Green => {
            println!("green: signal set '{set}' evidence is current.");
        }
        EvidenceState::Red => {
            let failed = report
                .evidence
                .as_ref()
                .map(|e| e.failed.join(", "))
                .unwrap_or_default();
            println!("red: signal set '{set}' evidence has failures: {failed}.");
        }
        EvidenceState::Stale => {
            println!(
                "stale: signal set '{set}' evidence no longer applies ({}).",
                report.reason
            );
        }
        EvidenceState::Missing => {
            println!(
                "missing: no current evidence for signal set '{set}' ({}).",
                report.reason
            );
        }
    }
}

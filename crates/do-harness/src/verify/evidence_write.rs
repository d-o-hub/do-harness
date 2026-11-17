//! Evidence-artifact resolution and writing for `verify`.
//!
//! Extracted from `verify.rs` so the command flow and the artifact contract
//! stay under the 500-line ceiling; the split is move-only.

use std::path::Path;

use crate::evidence::EvidenceSkipped;
use crate::sensors::{self, VerifyOpts};
use crate::{CliError, config, evidence, report};

use super::{all_unrunnable, unrunnable_error};

/// Converts a findings count to the database integer type.
pub(super) fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Resolves the evidence artifact the run owns, or `None` when it writes none.
pub(super) fn evidence_destination(root: &Path, opts: &VerifyOpts) -> Option<std::path::PathBuf> {
    opts.evidence
        .clone()
        .map(|p| if p.is_relative() { root.join(p) } else { p })
        .or_else(|| {
            opts.output
                .clone()
                .map(|p| if p.is_relative() { root.join(p) } else { p })
        })
        .or_else(|| {
            opts.set
                .as_ref()
                .map(|set| crate::status::default_path_for_set(root, set))
        })
        .or_else(|| opts.strict.then(|| root.join(".do-harness/evidence.json")))
}

/// Writes the evidence artifact for runs that own one.
///
/// Signal-set runs own their evidence file so a feedback run can never
/// clobber verification evidence (or vice versa). Runs without `--set` keep
/// the legacy behavior: an artifact only for `--evidence` or `--strict`.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_evidence(
    root: &Path,
    cfg: &config::Config,
    config_bytes: Option<&[u8]>,
    report: &report::VerifyReport,
    opts: &VerifyOpts,
    started_at: i64,
    selection: &sensors::ResolvedSelection<'_>,
    invalidated_reason: Option<&str>,
) -> std::result::Result<(), CliError> {
    let Some(path) = evidence_destination(root, opts) else {
        return Ok(());
    };
    let selected: Vec<String> = selection.specs.iter().map(|s| s.name.clone()).collect();
    let skipped: Vec<EvidenceSkipped> = selection
        .skipped
        .iter()
        .map(|s| EvidenceSkipped {
            name: s.name.clone(),
            reason: s.reason.clone(),
        })
        .collect();
    let candidates =
        crate::signals::candidate_specs(cfg, opts.set.as_deref()).map_err(CliError::Usage)?;
    // Fingerprints describe the post-execution workspace: sensors may create
    // files, and status never executes sensors, so the left-behind state is
    // the stable comparison point.
    // Fingerprints must describe the same selection input `status` recomputes:
    // a range run on a clean checkout has an empty working tree, so hashing
    // the working-tree change would make its own evidence look stale.
    let post = if opts.since.is_some() {
        crate::changes::discover_selection(root, opts.changed, opts.since.as_deref()).changed
    } else {
        crate::changes::discover(root)
    };
    let fingerprints = crate::fingerprint::for_run(
        root,
        cfg,
        config_bytes,
        opts.set.as_deref(),
        &candidates,
        &post,
    );
    let finished_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let meta = evidence::RunMeta {
        cfg,
        config_bytes,
        root,
        set: opts.set.as_deref(),
        selected: &selected,
        fingerprints,
        changed: opts.changed,
        changes: (selection.changes.mode != crate::changes::SelectionMode::All)
            .then_some(&selection.changes),
        selection: (selection.changes.mode != crate::changes::SelectionMode::All).then(|| {
            evidence::EvidenceSelection::from_changes(&selection.changes, &selection.selected)
        }),
        skipped,
        task: opts.task,
        record: opts.record,
        started_at,
        finished_at,
        invalidated_reason: invalidated_reason.map(ToOwned::to_owned),
    };
    let path = crate::managed_path::evidence_path(root, &path).map_err(CliError::Usage)?;
    let mut doc = evidence::EvidenceDocument::from_run(report, &meta);
    evidence::write_document(root, &path, &mut doc).map_err(CliError::Verify)?;

    if opts.strict && !(doc.is_strict_clean() && doc.verify_chain(doc.prev_hash.as_deref())) {
        eprintln!(
            "strict evidence check failed; artifact at {}",
            path.display()
        );
        if all_unrunnable(report) {
            return Err(unrunnable_error(report));
        }
        return Err(CliError::Verify(anyhow::anyhow!("weak evidence")));
    }
    Ok(())
}

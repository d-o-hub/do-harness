//! Native fresh-context design review: packet assembly, reviewer launch,
//! sanitized records, dispositions, and the host file-handoff bridge.
//!
//! Every capability the harness ships is a native CLI subcommand usable in
//! any git repository (the Repository-Agnostic Capability Contract in
//! AGENTS.md): this command moved the fresh-context design-review pilot out
//! of repository-local Python and into the CLI. It is explicit opt-in — a
//! review runs only when `--reviewer-command-json` names an external
//! reviewer process, nothing here is ever wired into sensors or CI, and no
//! model is launched by default.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::cli::{DesignReviewAction, DesignReviewRunArgs, DispositionStatus};

pub mod bridge;
pub mod packet;
pub mod reviewer;

use packet::{DEFAULT_MAX_FINDINGS, DEFAULT_MAX_OUTPUT_BYTES, DEFAULT_TIMEOUT_S, PacketSpec};
use reviewer::Outcome;

/// Experiment identity shared by every record this command writes.
pub const EXPERIMENT: &str = "fresh-context-review-pilot";

/// Routes a `do-harness design-review` action.
///
/// # Errors
///
/// Usage-class failures (exit 2 under the dispatch mapping): invalid
/// arguments, an unresolvable revision, or a pinned path missing at that
/// revision. A recorded review outcome — success, failure, timeout,
/// `not_run`, or `unverified` — is a success of the command: the record
/// carries the verdict.
pub fn run(root: &Path, action: DesignReviewAction) -> Result<()> {
    match action {
        DesignReviewAction::Run(args) => run_review(root, &args),
        DesignReviewAction::Dispose {
            record,
            finding_id,
            status,
            note,
        } => dispose(&record, &finding_id, status, note.as_deref()),
        DesignReviewAction::Bridge {
            handoff_dir,
            max_wait_s,
        } => bridge::run(&handoff_dir, max_wait_s),
    }
}

fn parse_reviewer_command(raw: Option<&str>) -> Result<Option<Vec<String>>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value: Value = serde_json::from_str(raw)
        .map_err(|_| anyhow::anyhow!("--reviewer-command-json must be a JSON argv list"))?;
    let list = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("--reviewer-command-json must be a JSON argv list"))?;
    let mut command = Vec::new();
    for item in list {
        let text = item
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("--reviewer-command-json must be a JSON argv list"))?;
        if text.is_empty() {
            bail!("--reviewer-command-json must be a non-empty argv list");
        }
        command.push(text.to_owned());
    }
    Ok(Some(command))
}

/// Writes a record deterministically: canonical JSON plus a newline.
fn write_record(path: &Path, record: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    let body = packet::canonical_json(record)? + "\n";
    std::fs::write(path, body.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Assembles the packet, launches one fresh reviewer, records the outcome.
fn run_review(root: &Path, args: &DesignReviewRunArgs) -> Result<()> {
    let meta: Value = serde_json::from_str(&args.meta)
        .map_err(|_| anyhow::anyhow!("--meta must be a JSON object"))?;
    if !meta.is_object() {
        bail!("--meta must be a JSON object");
    }
    let reviewer_command = parse_reviewer_command(args.reviewer_command_json.as_deref())?;
    let review_id = args.review_id.clone().unwrap_or_else(|| {
        args.record_out.file_stem().map_or_else(
            || "review".to_owned(),
            |stem| stem.to_string_lossy().into_owned(),
        )
    });

    let spec = PacketSpec {
        review_id: review_id.clone(),
        objective: args.objective.clone(),
        intended_behavior: args.intended_behavior.clone().unwrap_or_default(),
        constraints: args.constraint.clone(),
        proposed_contract: args.proposed_contract.clone(),
        open_questions: args.open_question.clone(),
        source_paths: args.source_path.clone(),
        test_paths: args.test_path.clone(),
        doc_paths: args.doc_path.clone(),
        design_doc: args.design_doc.to_string_lossy().into_owned(),
        revision: args.revision.clone(),
        timeout_s: args.limit_timeout.unwrap_or(DEFAULT_TIMEOUT_S),
        max_findings: args.limit_findings.unwrap_or(DEFAULT_MAX_FINDINGS),
        max_output_bytes: args.limit_output.unwrap_or(DEFAULT_MAX_OUTPUT_BYTES),
    };
    let frozen_packet = packet::build_packet(root, &spec)?;
    let packet_hash = packet::packet_hash(&frozen_packet)?;
    let mut frozen = frozen_packet;
    frozen["packet_hash"] = json!(packet_hash);
    if let Some(path) = &args.packet_out {
        let body = packet::canonical_json(&frozen)? + "\n";
        std::fs::write(path, body.as_bytes())
            .with_context(|| format!("failed to write {}", path.display()))?;
    }

    let mut record = match &reviewer_command {
        None => json!({
            "outcome": Outcome::NotRun.as_str(),
            "reason": "no reviewer capability configured; fresh-context review is explicit opt-in",
        }),
        Some(command) => {
            let packet_json = packet::canonical_json(&frozen)?;
            let mut record = reviewer::run_reviewer(
                command,
                &packet_json,
                root,
                spec.timeout_s,
                spec.max_output_bytes,
                spec.max_findings,
                args.parent_marker.as_deref(),
            )?;
            let succeeded = record["outcome"] == json!(Outcome::SuccessWithFindings.as_str())
                || record["outcome"] == json!(Outcome::SuccessZeroFindings.as_str());
            if succeeded && args.parent_marker.is_none() {
                record["outcome"] = json!(Outcome::Unverified.as_str());
                record["reason"] = json!(
                    "isolation unverified: external reviewer without a parent-only marker probe"
                );
            }
            record
        }
    };

    let outcome = record["outcome"].as_str().unwrap_or_default().to_owned();
    record["schema_version"] = json!(packet::SCHEMA_VERSION);
    record["experiment"] = json!(EXPERIMENT);
    record["review_id"] = json!(review_id);
    record["packet_hash"] = json!(packet_hash);
    record["packet"] = frozen;
    record["reviewer_mode"] = json!(if reviewer_command.is_some() {
        "external"
    } else {
        "none"
    });
    record["meta"] = meta;
    record["isolation"] = json!({
        "parent_conversation": "none supplied to reviewer process",
        "host_injected_instructions": "cannot be excluded for external reviewer services; recorded as unverified unless native records prove otherwise",
        "marker_probe_configured": args.parent_marker.is_some(),
        "marker_leaked": record.get("marker_leaked").cloned().unwrap_or(Value::Null),
    });
    record["dispositions"] = json!([]);
    write_record(&args.record_out, &record)?;
    println!(
        "design-review: outcome={outcome} review_id={review_id} record={}",
        args.record_out.display()
    );
    Ok(())
}

/// Dispositions a recorded finding as confirmed, rejected, or unresolved.
///
/// # Errors
///
/// Fails when the record cannot be read or parsed, or the finding id is not
/// present in the record.
fn dispose(
    record_path: &Path,
    finding_id: &str,
    status: DispositionStatus,
    note: Option<&str>,
) -> Result<()> {
    let raw = std::fs::read_to_string(record_path)
        .with_context(|| format!("failed to read {}", record_path.display()))?;
    let mut record: Value = serde_json::from_str(&raw)
        .with_context(|| format!("{} is not a valid record", record_path.display()))?;
    let present = record
        .get("findings")
        .and_then(Value::as_array)
        .is_some_and(|findings| {
            findings
                .iter()
                .any(|finding| finding.get("id").and_then(Value::as_str) == Some(finding_id))
        });
    if !present {
        bail!("finding '{finding_id}' not present in record");
    }
    let mut dispositions = record
        .get("dispositions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    dispositions.retain(|entry| entry.get("id").and_then(Value::as_str) != Some(finding_id));
    let mut entry = json!({"id": finding_id, "status": status.as_str()});
    if let Some(note) = note {
        entry["note"] = json!(note);
    }
    dispositions.push(entry);
    record["dispositions"] = json!(dispositions);
    write_record(record_path, &record)?;
    println!(
        "design-review: dispositioned {finding_id} as {} in {}",
        status.as_str(),
        record_path.display()
    );
    Ok(())
}

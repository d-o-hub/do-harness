//! Inventory collection and ledger evaluation for PR conversations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};

use super::digest::compute_inventory_digest;
use super::disposition::parse_disposition;
#[allow(unused_imports)]
use super::model::{
    ConversationItem, ConversationLedger, DispositionDecision, DispositionEntry, DispositionRecord,
    DispositionStatus, ItemType, NeedsTriageItem,
};
use crate::pr::gh;
use crate::pr::waiver_comment::{first_line, is_authorized_role};

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn is_courtesy_or_pass(author: &str, body: &str, is_pr_author: bool) -> bool {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return true;
    }
    // PR author comments on their own PR that don't raise questions
    if is_pr_author && !trimmed.contains('?') {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    // Common courtesy words
    if lower == "lgtm"
        || lower == "thanks"
        || lower == "thank you"
        || lower == "done"
        || lower == "fixed"
        || lower == "merged"
        || lower.starts_with(":+1:")
        || lower.starts_with(":shipit:")
    {
        return true;
    }
    // Bot pass statements (e.g. Codacy 0 issues, CodeQL passed)
    if (author.ends_with("[bot]") || author == "codacy-production")
        && (lower.contains("0 issues")
            || lower.contains("up to standards")
            || lower.contains("no issues found")
            || lower.contains("all checks passed"))
    {
        return true;
    }
    false
}

/// Builds the complete conversation ledger and validates all dispositions.
///
/// # Errors
///
/// Returns an error only on unrecoverable traversal or permission query failures.
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
pub fn build_inventory(
    root: &Path,
    _pr: u64,
    view: &gh::PrReadinessView,
    threads: &[gh::ReviewThread],
    comments: &[gh::Comment],
    reviews: &[gh::Review],
) -> Result<ConversationLedger> {
    // 1. Gather all raw items first to calculate hashes
    let mut items = Vec::new();
    let mut item_hash_map: BTreeMap<String, String> = BTreeMap::new();

    // Map reviews
    for rev in reviews {
        let node_id = rev.node_id.as_deref().unwrap_or("");
        let id_str = if node_id.is_empty() {
            rev.id.to_string()
        } else {
            node_id.to_owned()
        };
        let item_ref = format!("review:{id_str}");
        let body_hash = sha256_hex(rev.body.as_bytes());
        item_hash_map.insert(item_ref.clone(), body_hash.clone());

        let author = rev
            .user
            .as_ref()
            .map_or("unknown", |u| u.login.as_str())
            .to_owned();

        items.push(ConversationItem {
            item_ref,
            item_type: ItemType::Review,
            id: id_str,
            author,
            url: rev.html_url.clone(),
            created_at: rev.submitted_at.clone().unwrap_or_default(),
            updated_at: None,
            body: rev.body.clone(),
            body_hash,
            commit_oid: rev.commit_id.clone(),
            state: rev.state.clone(),
            actionable: rev.state == "CHANGES_REQUESTED",
        });
    }

    // Map review threads
    for thread in threads {
        let item_ref = format!("review-thread:{}", thread.id);
        let first_comment_body = thread
            .comments
            .nodes
            .first()
            .map_or("", |c| c.body.as_str());
        let body_hash = sha256_hex(first_comment_body.as_bytes());
        item_hash_map.insert(item_ref.clone(), body_hash.clone());

        let author = thread
            .comments
            .nodes
            .first()
            .and_then(|c| c.author.as_ref())
            .map_or("unknown", |a| a.login.as_str())
            .to_owned();

        items.push(ConversationItem {
            item_ref,
            item_type: ItemType::ReviewThread,
            id: thread.id.clone(),
            author,
            url: None,
            created_at: thread
                .comments
                .nodes
                .first()
                .and_then(|c| c.created_at.clone())
                .unwrap_or_default(),
            updated_at: None,
            body: first_comment_body.to_owned(),
            body_hash,
            commit_oid: None,
            state: if thread.is_resolved {
                "resolved".to_owned()
            } else {
                "unresolved".to_owned()
            },
            actionable: !thread.is_resolved,
        });

        // Also record individual thread reply comments
        for (idx, c) in thread.comments.nodes.iter().enumerate() {
            if idx == 0 {
                continue;
            }
            let c_id =
                c.id.clone()
                    .unwrap_or_else(|| format!("{}_{idx}", thread.id));
            let c_ref = format!("review-comment:{c_id}");
            let c_hash = sha256_hex(c.body.as_bytes());
            item_hash_map.insert(c_ref.clone(), c_hash.clone());
            items.push(ConversationItem {
                item_ref: c_ref,
                item_type: ItemType::ReviewComment,
                id: c_id,
                author: c
                    .author
                    .as_ref()
                    .map_or("unknown", |a| a.login.as_str())
                    .to_owned(),
                url: c.url.clone(),
                created_at: c.created_at.clone().unwrap_or_default(),
                updated_at: None,
                body: c.body.clone(),
                body_hash: c_hash,
                commit_oid: None,
                state: "comment".to_owned(),
                actionable: false,
            });
        }
    }

    // Pre-populate hash map with issue comments so dispositions can validate them
    for comm in comments {
        let cid = comm.id.unwrap_or(0);
        let item_ref = format!("issue-comment:{cid}");
        let body_hash = sha256_hex(comm.body.as_bytes());
        item_hash_map.insert(item_ref, body_hash);
    }

    let mut control_comment_ids = BTreeSet::new();
    let mut candidate_dispositions = Vec::new();
    for comm in comments {
        if let Some(record) = parse_disposition(&comm.body) {
            candidate_dispositions.push((comm, record));
        }
    }

    // 2. Validate dispositions against PR state and item hashes
    let mut accepted_dispositions = Vec::new();
    let mut rejected_dispositions = Vec::new();
    let mut disposed_item_refs = BTreeSet::new();

    for (comm, record) in candidate_dispositions {
        let author = comm.user.as_ref().map_or("unknown", |u| u.login.as_str());

        let perm = gh::user_permission(root, author).unwrap_or_default();
        let authorized = is_authorized_role(&perm);

        let status = if !authorized {
            DispositionStatus::Unauthorized
        } else if record.head_sha != view.head_ref_oid {
            DispositionStatus::StaleHead
        } else if let Some(expected_hash) = item_hash_map.get(&record.item_ref) {
            if &record.item_sha256 != expected_hash {
                DispositionStatus::MismatchedItemHash
            } else if record.decision == DispositionDecision::Fixed
                && record.evidence_sha.as_deref() != Some(&view.head_ref_oid)
            {
                DispositionStatus::MissingEvidence
            } else {
                DispositionStatus::Accepted
            }
        } else {
            DispositionStatus::MismatchedItemHash
        };

        let entry = DispositionEntry {
            record: record.clone(),
            author: author.to_owned(),
            comment_id: comm.id,
            authorized,
            status: status.clone(),
        };

        if status == DispositionStatus::Accepted {
            disposed_item_refs.insert(record.item_ref.clone());
            accepted_dispositions.push(record);
            if let Some(cid) = comm.id {
                control_comment_ids.insert(cid);
            }
        } else {
            rejected_dispositions.push(entry);
        }
    }

    // Now insert issue comments (excluding accepted control comments)
    let mut needs_triage = Vec::new();

    for comm in comments {
        let cid = comm.id.unwrap_or(0);
        let item_ref = format!("issue-comment:{cid}");
        let body_hash = sha256_hex(comm.body.as_bytes());
        let author = comm
            .user
            .as_ref()
            .map_or("unknown", |u| u.login.as_str())
            .to_owned();

        if control_comment_ids.contains(&cid) {
            continue;
        }

        // Codecov reports are handled specifically by Codecov evaluator
        if crate::pr::readiness::is_codecov_author(&author) {
            continue;
        }

        // Waiver comments are control records handled by the waiver evaluator
        if crate::pr::waiver_comment::parse_candidate_waiver(comm).is_some() {
            continue;
        }

        let is_disposed = disposed_item_refs.contains(&item_ref);
        let courtesy = is_courtesy_or_pass(&author, &comm.body, false);

        let actionable = !is_disposed && !courtesy;
        if actionable {
            needs_triage.push(NeedsTriageItem {
                item_ref: item_ref.clone(),
                author: author.clone(),
                excerpt: first_line(&comm.body, 80),
                reason: "unassessed top-level review or bot comment".to_owned(),
            });
        }

        items.push(ConversationItem {
            item_ref,
            item_type: ItemType::IssueComment,
            id: cid.to_string(),
            author,
            url: comm.html_url.clone(),
            created_at: comm.created_at.clone(),
            updated_at: None,
            body: comm.body.clone(),
            body_hash,
            commit_oid: None,
            state: if is_disposed { "disposed" } else { "open" }.to_owned(),
            actionable,
        });
    }

    let inventory_digest =
        compute_inventory_digest(&items, &accepted_dispositions, &rejected_dispositions);

    Ok(ConversationLedger {
        review_decision: view.review_decision.clone(),
        items,
        accepted_dispositions,
        rejected_dispositions,
        needs_triage,
        inventory_digest,
        complete: true,
    })
}

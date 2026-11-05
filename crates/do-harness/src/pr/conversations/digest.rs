//! Canonical inventory digest computation for conversations and dispositions.

use sha2::{Digest, Sha256};

use super::model::{ConversationItem, DispositionEntry, DispositionRecord};

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Computes a canonical SHA-256 digest over sorted items, accepted dispositions,
/// and rejected dispositions.
#[must_use]
pub fn compute_inventory_digest(
    items: &[ConversationItem],
    accepted: &[DispositionRecord],
    rejected: &[DispositionEntry],
) -> String {
    let mut hasher = Sha256::new();

    // 1. Sort items canonically by item_type and item_ref
    let mut sorted_items = items.to_vec();
    sorted_items.sort_by(|a, b| {
        a.item_type
            .cmp(&b.item_type)
            .then_with(|| a.item_ref.cmp(&b.item_ref))
    });

    hasher.update(b"ITEMS_BEGIN\n");
    for item in &sorted_items {
        hasher.update(item.item_type.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(item.item_ref.as_bytes());
        hasher.update(b":");
        hasher.update(item.author.as_bytes());
        hasher.update(b":");
        hasher.update(item.body_hash.as_bytes());
        hasher.update(b":");
        hasher.update(item.created_at.as_bytes());
        hasher.update(b":");
        hasher.update(item.state.as_bytes());
        hasher.update(b"\n");
    }
    hasher.update(b"ITEMS_END\n");

    // 2. Sort accepted dispositions by item_ref
    let mut sorted_accepted = accepted.to_vec();
    sorted_accepted.sort_by(|a, b| a.item_ref.cmp(&b.item_ref));

    hasher.update(b"ACCEPTED_BEGIN\n");
    for d in &sorted_accepted {
        hasher.update(d.item_ref.as_bytes());
        hasher.update(b":");
        hasher.update(d.decision.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(d.head_sha.as_bytes());
        hasher.update(b":");
        hasher.update(d.item_sha256.as_bytes());
        hasher.update(b":");
        hasher.update(d.policy_sha256.as_bytes());
        hasher.update(b":");
        if let Some(ev) = &d.evidence_sha {
            hasher.update(ev.as_bytes());
        }
        hasher.update(b"\n");
    }
    hasher.update(b"ACCEPTED_END\n");

    // 3. Sort rejected dispositions by item_ref then author
    let mut sorted_rejected = rejected.to_vec();
    sorted_rejected.sort_by(|a, b| {
        a.record
            .item_ref
            .cmp(&b.record.item_ref)
            .then_with(|| a.author.cmp(&b.author))
    });

    hasher.update(b"REJECTED_BEGIN\n");
    for r in &sorted_rejected {
        hasher.update(r.record.item_ref.as_bytes());
        hasher.update(b":");
        hasher.update(r.author.as_bytes());
        hasher.update(b":");
        hasher.update(format!("{:?}", r.status).as_bytes());
        hasher.update(b"\n");
    }
    hasher.update(b"REJECTED_END\n");

    sha256_hex(&hasher.finalize())
}

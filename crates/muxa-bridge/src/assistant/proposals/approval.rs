//! What an approval of a proposal item ran and made, kept in the item's own
//! file beside it (ADR 0047 §4). Keyed by the item's id and its proposal's
//! hash, as a Routine's approval is (ADR 0026, Implementation notes).
use std::collections::BTreeMap;

use muxa_protocol::{InboxItem, ItemKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::items::store::TaskItems;

/// The item file's field for the approvals, by item id.
const FIELD: &str = "approvals";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    /// The proposal's hash when the approval began.
    pub hash: String,
    /// The Capture's or Work item's `rev` the last attempt ran at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<u64>,
    /// Epoch seconds: an attempt is running since then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    /// Epoch seconds: the host sent it without review (`overall_requests = "auto"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_at: Option<u64>,
    /// Epoch seconds: the host tried to send it and a limit refused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_refused: Option<u64>,
    /// What the approval made: the sorted Capture, its Work item, the request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
}

impl Approval {
    pub fn new(hash: &str) -> Self {
        Self {
            hash: hash.to_owned(),
            ..Self::default()
        }
    }

    /// The action ran and made what it records.
    pub fn done(&self) -> bool {
        self.capture.is_some() || self.request.is_some()
    }

    /// Counts against `max_auto_requests_per_day` from `day_start`: sent, or being sent.
    pub fn auto_since(&self, day_start: u64) -> bool {
        self.auto_at.is_some_and(|at| at >= day_start) && (self.done() || self.since.is_some())
    }
}

/// The proposal's hash: its kind and its fields as stored on the item.
pub fn hash(item: &InboxItem) -> Option<String> {
    let (kind, body) = match item.kind {
        ItemKind::Sort => ("sort", serde_json::to_vec(item.sort.as_ref()?)),
        ItemKind::Delegate => ("delegate", serde_json::to_vec(item.delegate.as_ref()?)),
        _ => return None,
    };
    let body = body.ok()?;
    let mut h = Sha256::new();
    h.update(kind.as_bytes());
    h.update([0]);
    h.update(&body);
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("sha256:{hex}"))
}

/// Every approval in the file. An unreadable field is read as none.
pub fn all(items: &TaskItems) -> BTreeMap<String, Approval> {
    items
        .extra
        .get(FIELD)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Item `id`'s approval, when it is for the proposal of `hash`.
pub fn get(items: &TaskItems, id: &str, hash: &str) -> Option<Approval> {
    all(items).remove(id).filter(|a| a.hash == hash)
}

/// Writes item `id`'s approval, and drops those of items the file no longer has.
pub fn put(items: &mut TaskItems, id: &str, approval: Approval) {
    let mut map = all(items);
    map.insert(id.to_owned(), approval);
    map.retain(|k, _| items.items.iter().any(|s| s.item.id == *k));
    match serde_json::to_value(map) {
        Ok(v) => {
            items.extra.insert(FIELD.into(), v);
        }
        Err(e) => tracing::warn!(task = %items.task, error = %e, "cannot keep an approval"),
    }
}

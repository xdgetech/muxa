//! Policies, corrections and hypotheses (ADR 0048), at `work` level 10: what
//! the user told the overall assistant, and what the host saw. Their texts are
//! never logged.
use serde::{Deserialize, Serialize};

/// Level 10: `add-policy`, `update-policy`, `update-plan`, `forget`, `confirm`
/// and `reject` with a reason on proposals, `hypothesis` Inbox items, and
/// `policies`, `hypotheses` and `corrections` in `inbox-list`.
pub const WORK_POLICIES: u32 = 10;

/// The user's own words, which outrank every hypothesis and note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    /// `y-` and 8 hex digits.
    pub id: String,
    pub rev: u64,
    /// 1 to 500 bytes, one paragraph. Never logged.
    pub text: String,
    /// `all`, or one workspace's name.
    pub scope: String,
    /// Epoch seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<u64>,
    /// Epoch seconds. Past it the policy is expired and in no brief.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
    /// `user`, or the `h-` hypothesis it was confirmed from.
    pub source: String,
    pub created_at: u64,
    pub updated_at: u64,
}

/// A user's action on a plan, as the host saw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Correction {
    /// `x-` and 8 hex digits.
    pub id: String,
    pub kind: CorrectionKind,
    /// Epoch seconds.
    pub at: u64,
    /// The plan that was current.
    pub plan: String,
    /// The Work item or Inbox item. Absent with `reordered`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<String>,
    /// Whether the plan's `top` listed the row.
    pub in_top: bool,
    /// The row's workspace; absent for nobody's items.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// With `reordered`: the rows' order before.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
    /// With `reordered`: the rows' order after.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    /// The user's optional reason, at most 500 bytes. Never logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CorrectionKind {
    /// Today's rows dragged into another order.
    Reordered,
    /// A row removed from Today ("Not today").
    Dismissed,
    /// A `sort`, `delegate` or `hypothesis` item rejected.
    Rejected,
    Pinned,
    Unpinned,
    /// Later: "not now", never "unimportant".
    Deferred,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// The overall assistant's guess at a preference, which the user confirms or rejects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hypothesis {
    /// `h-` and 8 hex digits.
    pub id: String,
    /// 1 to 500 bytes. Never logged.
    pub text: String,
    /// `all`, or one workspace's name.
    pub scope: String,
    /// 1 to 5 correction ids.
    pub evidence: Vec<String>,
    pub confidence: Confidence,
    pub state: HypothesisState,
    /// The plan that proposed it.
    pub plan: String,
    /// Epoch seconds: when a `proposed` one expires, or a `rejected` one leaves the brief.
    pub expires_at: u64,
    /// Epoch seconds: when it was proposed.
    pub at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HypothesisState {
    Proposed,
    /// It became a policy.
    Confirmed,
    Rejected,
    /// Nobody answered it in 14 days.
    Expired,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// A `hypothesis` Inbox item's payload: what `confirm` makes a policy of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HypothesisProposal {
    /// The `h-` id.
    pub hypothesis: String,
    /// Never logged.
    pub text: String,
    pub scope: String,
    pub evidence: Vec<String>,
    pub confidence: Confidence,
}

/// What `update-policy` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyAction {
    /// Change `text`, `scope`, `from` or `until`.
    Edit,
    Delete,
}

/// What `update-plan` does to the newest `resulted` plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanAction {
    /// Set `userOrder` to `order`.
    Reorder,
    /// Add `row` to `dismissed`, with `text` as the reason.
    Dismiss,
}

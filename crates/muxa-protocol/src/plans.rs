//! Delegate notes, the overall assistant and its plans (ADR 0047), at `work`
//! level 9. A plan never changes a Work item, an Inbox item or their order.
use serde::{Deserialize, Serialize};

use crate::{AssistScope, Delivery, Notes, SortAction};

/// Level 9: `apply-notes`, `plan-now`, `pin`, `unpin` and `due` on
/// `update-work`, `sort` and `delegate` Inbox items, and `plans` and
/// `assistant` in `inbox-list`.
pub const WORK_PLANS: u32 = 9;

/// One overall run's plan: at most 3 rows to do next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// `p-` and 8 hex digits.
    pub id: String,
    /// Grows by one on every write; `apply-notes` sends it as `version`.
    pub rev: u64,
    /// Epoch seconds: the schedule's time, or when `plan-now` asked.
    pub at: u64,
    /// Epoch seconds: when the host read the inputs for the brief.
    pub inputs_at: u64,
    pub delivery: Delivery,
    /// Epoch seconds: when `delivery` was last derived.
    pub delivery_at: u64,
    /// The time the user has, from `plan-now`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes: Option<u32>,
    /// The run record, `t-…` of kind `assistant-run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// The workspaces whose data the brief carried.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workspaces: Vec<String>,
    /// At most 3, pinned Work items first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top: Vec<PlanRow>,
    /// Work items moved down with Later whose `due` is within 24 hours.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub due_soon: Vec<String>,
    /// Derived when listed: requests whose run has not answered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unanswered: Vec<String>,
    /// Derived when listed: requests whose newest result is older than their Work item.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<String>,
    /// Derived against the previous plan.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<PlanChange>,
    /// Level 10: Today's rows in the user's order, from `update-plan reorder`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_order: Vec<String>,
    /// Level 10: rows the user removed from Today, from `update-plan dismiss`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dismissed: Vec<String>,
    /// The overall assistant's proposed notes. Never logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Notes>,
    /// Why a plan is `failed` or `cancelled`, at most 500 bytes: host text
    /// naming ids and limits, never the agent's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One row of `top`: a Work item or an Inbox item. Texts are never logged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub why: String,
    pub next: String,
    pub minutes: Minutes,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub basis: Vec<Basis>,
    /// What the agent would need to know to rank it better.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
}

/// The agent's estimate of the time a row takes: `low <= estimated <= high`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Minutes {
    pub low: u32,
    pub high: u32,
    pub estimated: u32,
}

/// One reason a row is in the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Basis {
    pub kind: BasisKind,
    pub text: String,
    /// A URL, a path or an id the reason comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Epoch seconds, e.g. an estimated deadline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BasisKind {
    Due,
    Dependency,
    Goal,
    Time,
    /// The user's pin.
    Manual,
    /// Level 10: a policy `y-…` in `source` (ADR 0048).
    Policy,
    /// Level 10: a hypothesis `h-…` in `source`.
    Hypothesis,
    /// Level 10: a correction `x-…` in `source`.
    Correction,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// A row that entered, left or moved since the previous plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanChange {
    /// The Work item or Inbox item.
    pub id: String,
    pub change: ChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Entered,
    Left,
    Moved,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// A `sort` Inbox item's action: what `sort-capture` would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortProposal {
    pub capture: String,
    pub action: SortAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// With `link`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<String>,
}

/// A `delegate` Inbox item's action: what `request-assistant` would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateProposal {
    pub work: String,
    /// The receiving workspace.
    pub workspace: String,
    pub scope: AssistScope,
    /// The ask, at most 1 KiB. Never logged.
    pub purpose: String,
    /// Why the overall assistant asks. Never logged.
    pub reason: String,
}

/// Who assists on this host, in `inbox-list`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantInfo {
    /// The overall assistant's workspace. Clients leave it out of workspace pickers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overall: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delegates: Vec<DelegateInfo>,
}

/// A workspace with `[workspace.assistant]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegateInfo {
    pub workspace: String,
    pub share: Share,
    pub overall_requests: OverallRequests,
    /// Epoch seconds: when its `NOTES.md` last changed. Absent: no notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_at: Option<u64>,
}

/// What a workspace shares with the overall assistant (ADR 0047 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Share {
    None,
    Outline,
    Summaries,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// Whether the overall assistant may ask this workspace (ADR 0047 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverallRequests {
    /// Refuses even approved requests.
    None,
    /// The user approves each one.
    Review,
    /// Sent without approval, within every budget.
    Auto,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

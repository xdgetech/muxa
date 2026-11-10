//! Staff and hand-offs inside a workspace (ADR 0051), at `work` level 12. A
//! staff member is a host `[[worker]]`; a request version runs as a chain of
//! steps, one run each, passed on by hand-offs.
use serde::{Deserialize, Serialize};

use crate::Delivery;

/// Level 12: `worker` on `request-assistant` and `capture {assist}`, `step`
/// on `apply-notes`, `handoff` Inbox items, a request's `steps`, a result's
/// `worker` and `step`, and a delegate's `staff` in `inbox-list`.
pub const WORK_STAFF: u32 = 12;

/// One run of a request version's chain. Step 1 is the first member asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestStep {
    /// From 1.
    pub step: u32,
    /// The staff member who runs it: a host `[[worker]]` name.
    pub worker: String,
    pub by: StepBy,
    /// As a request's, for this step's run.
    pub delivery: Delivery,
    /// Epoch seconds: when `delivery` was last derived.
    pub delivery_at: u64,
    /// The step's run record, `t-…` of kind `assistant-run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// The hand-off this step's result asks for: the next step, once approved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<Handoff>,
}

/// Who picked a step's worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepBy {
    /// The user named the worker.
    User,
    /// The overall assistant named it in a `delegate` proposal.
    Overall,
    /// Nobody named one: the workspace's lead took it.
    Lead,
    /// The previous step handed it on.
    Handoff,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// "Pass this to that member next." Its purpose is never logged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handoff {
    /// A staff member of the same workspace, not yet in the chain.
    pub worker: String,
    /// What the next member does within the request's scope, 1 byte to 1 KiB.
    pub purpose: String,
}

/// A `handoff` Inbox item's action: what approve starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffProposal {
    /// `q-…`.
    pub request: String,
    pub version: u64,
    /// The step whose result asks for the hand-off; approve starts `step + 1`.
    pub step: u32,
    /// The Work item, to open its detail.
    pub work: String,
    /// The worker of `step`.
    pub from: String,
    /// The next worker.
    pub worker: String,
    /// Never logged.
    pub purpose: String,
}

/// A workspace's staff member, in `inbox-list`'s `assistant.delegates[]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffMember {
    /// A host `[[worker]]` name.
    pub worker: String,
    /// The worker's job description, at most 1 KiB.
    pub description: String,
    /// The member who takes requests that name no worker.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lead: bool,
    /// Epoch seconds: when the member's own notes last changed. Absent: none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_at: Option<u64>,
}

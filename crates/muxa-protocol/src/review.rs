//! A request's Result to review (ADR 0058), at `work` level 16: every stored
//! Result makes one Inbox item of kind `result`, answered like a Task's
//! review: hand off, send the member what to do next, done, or later.
use serde::{Deserialize, Serialize};

use crate::Handoff;

/// Level 16: `result` Inbox items, and `reply` on `result` and `write` items.
pub const WORK_RESULT_REVIEW: u32 = 16;

/// A `result` item's Result, so the user reviews it without opening the
/// Work item. Its summary is the item's `summary`. Never logged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultReview {
    /// `q-…`.
    pub request: String,
    pub version: u64,
    /// The step of `version` that produced the Result; 1 without a staff.
    pub step: u32,
    /// The Work item, to open its detail.
    pub work: String,
    /// The staff member who wrote it. Absent: the workspace has no staff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    /// Each finding's first line, at most 200 characters.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_questions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next: Vec<String>,
    /// Paths the run wrote, absolute.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// The hand-off the step proposes; `approve` starts it while it is offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<Handoff>,
}

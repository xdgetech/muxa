//! The assistant proposes Tasks (ADR 0060), at `work` level 18: a staff
//! member's Result or the secretary's `propose-task` carries a Task to
//! start, and approving it in the Inbox starts that Task in plan mode.
use serde::{Deserialize, Serialize};

/// Level 18: `result.task`, `new-task` items, `start-task` and `propose-task`.
pub const WORK_TASK_PROPOSALS: u32 = 18;

/// A Task the assistant proposes. Approval starts it once, through
/// `create-task`, on the workspace's default Task worker. Never logged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProposal {
    pub workspace: String,
    /// A repo of the workspace, as `create-task`'s `repo`. Absent: the Task
    /// gets its own folder under the workspace's `tasks/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// One line, at most 200 characters.
    pub title: String,
    /// What the Task's agent does; at most 4096 bytes.
    pub goal: String,
    /// The Work item the Task is for (`w-…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<String>,
    /// On a `result` item: the step whose Result proposed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    /// On a `result` item: the staff member who proposed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
}

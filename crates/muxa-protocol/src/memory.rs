//! Workspace memory (ADR 0053), at `work` level 14. A Task's agent writes
//! under "For the workspace" in its `TASK.md`; at the Task's end the host
//! asks the user to append that text to `<root>/.muxa/WORKSPACE.md`. The
//! text and the file are never logged.
use serde::{Deserialize, Serialize};

/// Level 14: `memory` Inbox items with `approve`, `reject` and `later`, and
/// a workspace's `memory` and `memoryAt` in `task-list`.
pub const WORK_MEMORY: u32 = 14;

/// A `memory` Inbox item's payload: the exact text approve appends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryProposal {
    /// The Task whose close-out proposed it.
    pub task: String,
    /// The Task's title; the appended heading carries it.
    pub title: String,
    /// Shown whole, as plain text, before Approve. At most 4 KiB.
    pub text: String,
    /// `sha256:` and the hex SHA-256 of `task`, NUL, `title`, NUL, `text`.
    pub hash: String,
}

/// What a workspace does with a Task's "For the workspace" at its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryMode {
    /// The user approves each addition in the Inbox.
    Review,
    /// The host appends at once, within the cap; otherwise it asks as `review`.
    Auto,
    /// Nothing is read and nothing is made.
    Off,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

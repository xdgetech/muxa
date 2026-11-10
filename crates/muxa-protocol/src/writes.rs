//! External writes only when asked (ADR 0052), at `work` level 13. A Result
//! proposes writes; the user approves each exact text in the Inbox; one
//! writer worker performs it as a Task. Actions, targets, contents and
//! reasons are never logged: a target can hold a token.
use serde::{Deserialize, Serialize};

/// Level 13: a Result's `writes`, `write` Inbox items with `approve`,
/// `reject` and `later`, and a delegate's `writer` and `writes` in `inbox-list`.
pub const WORK_WRITES: u32 = 13;

/// A write a Result proposes, as the host stored it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultWrite {
    /// 1 to 200 characters on one line, e.g. "Post to #release".
    pub action: String,
    /// 1 to 500 bytes on one line: `slack:#release`, `linear:ENG-12`, a URL.
    pub target: String,
    /// The exact text to write, 1 byte to 8 KiB.
    pub content: String,
    /// `sha256:` and the hex SHA-256 of `action`, NUL, `target`, NUL, `content`.
    pub hash: String,
    /// Why the member proposes it, 1 to 500 bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The `write` Inbox item that asks the user, once made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// The writer's Task, once approved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
}

/// A `write` Inbox item's payload: the exact write approve hands the writer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProposal {
    /// `q-…`.
    pub request: String,
    /// The request version whose Result proposed it; may be older than the current one.
    pub version: u64,
    /// The step of `version` whose Result proposed it. Absent: step 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    /// The staff member whose Result proposed it. Absent: the workspace's agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    pub action: String,
    pub target: String,
    /// Shown whole, as plain text, before Approve.
    pub content: String,
    pub hash: String,
}

/// The worker that performs a workspace's approved writes, in `inbox-list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriterInfo {
    /// A host `[[worker]]` name.
    pub worker: String,
    /// The worker's job description, at most 1 KiB.
    pub description: String,
}

/// Whether a workspace takes write proposals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Writes {
    /// A Result with `writes` fails the check; changes go into `next`.
    None,
    /// The user approves each write; the writer performs it.
    Review,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

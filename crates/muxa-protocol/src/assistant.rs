//! Captures, Work items and assistant requests (ADR 0046), at `work` level 8.
//! None of them is a Task or an Inbox item.
use serde::{Deserialize, Serialize};

/// Level 8: `capture`, `sort-capture`, `update-work`, `request-assistant`,
/// `cancel-request`, `get-work`, and `work`, `captures` and `requests` in
/// `inbox-list`.
pub const WORK_ASSISTANT: u32 = 8;

/// The user's original input. Its `text` is never rewritten. Never logged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capture {
    /// `c-` and 8 hex digits.
    pub id: String,
    pub rev: u64,
    /// The client's id for this capture; the host stores one Capture per value.
    pub client_id: String,
    /// At most 16 KiB.
    pub text: String,
    pub source: CaptureSource,
    pub state: CaptureState,
    /// Epoch seconds.
    pub created_at: u64,
    /// Epoch seconds.
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// The Work items made from it or linked to it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub work: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureSource {
    App,
    Tmux,
    Cli,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureState {
    /// Saved, not sorted yet: listed under Unsorted.
    Received,
    /// Sorted into a Work item, linked to one, or kept as a note.
    Sorted,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// The user's unfinished work. Only the user completes or withdraws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkItem {
    /// `w-` and 8 hex digits.
    pub id: String,
    /// Grows by one on every write; `update-work` sends the one it saw.
    pub rev: u64,
    /// One line, at most 200 characters.
    pub title: String,
    pub state: WorkState,
    /// Epoch seconds.
    pub created_at: u64,
    /// Epoch seconds.
    pub updated_at: u64,
    /// The workspace the work belongs to, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Set by `later`: listed below the others, as Inbox items (ADR 0040).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub later_at: Option<u64>,
    /// With `withdrawn` after `merge`: the Work item it was merged into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_into: Option<String>,
    /// Level 9: set by `pin`. Pinned items are listed first, in pin order (ADR 0047).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<u64>,
    /// Level 9: the user's deadline. Only the user sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<Due>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub captures: Vec<String>,
    /// Assistant requests, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requests: Vec<String>,
    /// Tasks started for it, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<String>,
}

/// `waiting-decision` and `in-progress` are derived from the linked Tasks and
/// requests when the host lists the item; they are not stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkState {
    Open,
    InProgress,
    WaitingDecision,
    Done,
    Withdrawn,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// One delegation of a Work item to a workspace's agent. Each `version`
/// runs at most once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantRequest {
    /// `q-` and 8 hex digits.
    pub id: String,
    pub work: String,
    /// Grows by one each time the user asks again.
    pub version: u64,
    pub workspace: String,
    pub scope: AssistScope,
    pub delivery: Delivery,
    /// Epoch seconds: when `delivery` was last derived.
    pub delivery_at: u64,
    /// Epoch seconds.
    pub created_at: u64,
    /// Epoch seconds.
    pub updated_at: u64,
    /// What the user asked for, at most 4096 bytes. Never logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// The current version's run record, `t-…` of kind `assistant-run`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// Newest first, at most 10.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<AssistResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssistScope {
    Research,
    Compare,
    Draft,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// Derived from the run record and the adapter, never from the agent's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Delivery {
    /// The run exists; the prompt is not typed yet.
    Pending,
    /// The prompt was typed and confirmed.
    Received,
    /// The adapter reports the agent busy.
    Processing,
    /// A send was interrupted. Never retyped automatically.
    Unconfirmed,
    Resulted,
    Failed,
    /// No result in the workspace's `run_timeout`. The agent is not stopped.
    TimedOut,
    CancelRequested,
    Cancelled,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// What a run returned, after the host's check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistResult {
    /// The request version that produced it.
    pub version: u64,
    /// Epoch seconds: when the host stored it.
    pub at: u64,
    /// At most 4096 bytes. Never logged.
    pub summary: String,
    /// An older version's result.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_questions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next: Vec<String>,
    /// Paths the run wrote, absolute.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub possible_duplicates: Vec<PossibleDuplicate>,
    /// Level 9: the delegate's proposed notes (ADR 0047 §1). Never logged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<Notes>,
}

/// A deadline the user set on a Work item (ADR 0047 §6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Due {
    /// Epoch seconds.
    pub at: u64,
    /// IANA name, e.g. `Asia/Tokyo`: the zone the user set it in.
    pub timezone: String,
}

/// A run's proposed replacement of the workspace's `.muxa/assistant/NOTES.md`.
/// `apply-notes` writes it only while the live file still hashes to `base`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notes {
    /// The full new text, at most 16 KiB. Never logged.
    pub text: String,
    /// Lowercase hex SHA-256 of the file the run read; of no bytes when there was none.
    pub base: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub text: String,
    /// At least one.
    pub sources: Vec<ResultSource>,
}

/// A URL or a path, with when the run read it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Epoch seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PossibleDuplicate {
    pub work: String,
    pub reason: String,
}

/// `capture {assist}`: ask the assistant right away.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistSpec {
    #[serde(deserialize_with = "crate::known::known")]
    pub scope: AssistScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortAction {
    /// Make a new Work item from the Capture.
    Work,
    /// Link the Capture to an existing Work item.
    Link,
    /// Keep the Capture as a note; no Work item.
    Note,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkAction {
    /// Change `title`, `workspace` or, at level 9, `due`.
    Edit,
    Later,
    Complete,
    Withdraw,
    Reopen,
    /// Withdraw this item into `into`; its Captures move there.
    Merge,
    /// Level 9: list it first, after the items pinned before it.
    Pin,
    /// Level 9: clear `pinnedAt`.
    Unpin,
}

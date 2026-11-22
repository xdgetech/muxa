//! Data types the muxa daemon and its clients share besides protocol v2's own
//! (`v2`): agent conversations, the Reader's entries, and Routine state.

mod routines;
pub mod settings;
pub mod v2;
pub use routines::{
    Disposition, InboxEntry, InboxEntryKind, Proposal, RoutineFailure, RoutineInfo, RoutineProblem,
    RoutineRun, RoutineSuccess, RoutineTrigger, SourceLink,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocInfo {
    /// What to send in `doc`: relative to the repo for `repo`, `plans/<name>` for `plan`.
    pub path: String,
    pub kind: DocKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Epoch seconds of the last change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    Repo,
    /// Written by the pane's agent outside the repo.
    Plan,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// An image file in a pane's repo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    /// Relative to the repo; what to send in `image`.
    pub path: String,
    /// Epoch seconds of the last change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<u64>,
    /// Bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// A file that differs from HEAD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeInfo {
    /// Relative to the repo; what to send in `diff`.
    pub path: String,
    pub status: ChangeStatus,
    /// The path before a rename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    /// Lines added and removed; absent for a binary file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    /// A merge conflict.
    Conflicted,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// One item of an agent conversation, the same for every agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatEntry {
    /// Stable within a chat; a later entry with the same id replaces this one.
    pub id: String,
    pub kind: ChatKind,
    /// The prompt, the reply (markdown), or the notice. Empty for a tool call.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ChatTool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatKind {
    User,
    Assistant,
    Tool,
    Notice,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// A choice the agent waits on, e.g. a permission. The adapter knows the agent's
/// menu, so each option carries the keys that pick it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPrompt {
    /// The tool entry the agent waits on.
    pub entry: String,
    pub kind: PromptKind,
    /// For a question: the question as the agent asks it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    pub options: Vec<PromptOption>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PromptKind {
    /// May the agent run this tool call?
    Permission,
    /// The agent asks the user to pick an answer.
    Question,
    /// The agent asks to leave plan mode.
    Plan,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOption {
    pub label: String,
    /// tmux key names to send to the pane, as in `keys`.
    pub keys: Vec<String>,
    /// Declines or cancels, e.g. a deny.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cancel: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTool {
    pub name: String,
    /// One line about the call, e.g. the command or the file.
    pub summary: String,
    /// The start of the result, once it arrived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub state: AgentState,
    /// Epoch seconds of the last state change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Running,
    Waiting,
    Idle,
    Done,
    Error,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

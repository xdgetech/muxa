//! The muxa client/bridge wire format: one JSON object per line. See
//! `protocol/PROTOCOL.md`; the shared fixtures in `protocol/fixtures` are the contract.

use serde::{Deserialize, Serialize};

/// Changes when a message changes incompatibly.
pub const VERSION: u32 = 1;

/// Sent by the client. Every request has an `id`, echoed in its `result`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Request {
    Ping {
        id: u64,
    },
    List {
        id: u64,
    },
    /// Show this pane. Replaces the watched pane. `screen` and `output`
    /// messages for it carry this request's id as `watch`.
    Watch {
        id: u64,
        pane: String,
    },
    Unwatch {
        id: u64,
    },
    /// Named keys, e.g. `C-c`, `Up`, `S-Enter`.
    Keys {
        id: u64,
        pane: String,
        keys: Vec<String>,
    },
    /// Typed literally.
    Text {
        id: u64,
        pane: String,
        text: String,
    },
    /// Raw key bytes, for a client that encodes keys itself.
    Bytes {
        id: u64,
        pane: String,
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
    },
    /// Pasted, bracketed when the program asked for it.
    Paste {
        id: u64,
        pane: String,
        text: String,
    },
    /// Follow the conversation of the agent in this pane, through an agent
    /// adapter. Needs the `chat` capability. Replaces the current chat;
    /// `chat-entries` for it carry this request's id as `chat`.
    Chat {
        id: u64,
        pane: String,
    },
    Unchat {
        id: u64,
    },
    /// The Markdown documents for this pane: its repo's, and its agent's (a plan).
    /// Needs the `docs` capability. A `doc-list` comes before the `result`.
    Docs {
        id: u64,
        pane: String,
    },
    /// Open one of the documents `docs` listed, and follow its changes. Replaces
    /// the open document; `doc-content` for it carries this request's id as `doc`.
    Doc {
        id: u64,
        pane: String,
        path: String,
    },
    Undoc {
        id: u64,
    },
    /// The files that differ from HEAD in this pane's git repo, untracked ones
    /// included. Needs the `changes` capability. A `change-list` comes before the `result`.
    Changes {
        id: u64,
        pane: String,
    },
    /// Show the diff of one listed file, and follow it. Replaces the open diff;
    /// `diff-content` for it carries this request's id as `diff`.
    Diff {
        id: u64,
        pane: String,
        path: String,
    },
    Undiff {
        id: u64,
    },
    /// Start a task: a worktree of `repo` on `branch`, and a session with an agent
    /// in it. Needs the `tasks` capability. A `task-created` comes before the `result`.
    NewTask {
        id: u64,
        /// An absolute path inside the repo, e.g. a session's `repoDir`.
        repo: String,
        branch: String,
        /// Where a new branch starts; the repo's default branch when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<String>,
    },
    /// Close a task's session: one with a worktree. Needs the `close-task`
    /// capability. The worktree is removed only as `mode` allows, and never
    /// with uncommitted changes; the branch is kept.
    CloseTask {
        id: u64,
        /// `$n`
        session: String,
        mode: CloseMode,
    },
    /// The client is leaving; the bridge exits.
    Bye {
        id: u64,
    },
}

impl Request {
    pub fn id(&self) -> u64 {
        match self {
            Request::Ping { id }
            | Request::List { id }
            | Request::Watch { id, .. }
            | Request::Unwatch { id }
            | Request::Keys { id, .. }
            | Request::Text { id, .. }
            | Request::Bytes { id, .. }
            | Request::Paste { id, .. }
            | Request::Chat { id, .. }
            | Request::Unchat { id }
            | Request::Docs { id, .. }
            | Request::Doc { id, .. }
            | Request::Undoc { id }
            | Request::NewTask { id, .. }
            | Request::Changes { id, .. }
            | Request::Diff { id, .. }
            | Request::Undiff { id }
            | Request::CloseTask { id, .. }
            | Request::Bye { id } => *id,
        }
    }
}

/// Sent by the bridge. Clients ignore message types they do not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Message {
    /// The first line the bridge sends, before any request.
    Hello {
        protocol: u32,
        bridge: String,
        tmux: String,
        capabilities: Vec<String>,
    },
    /// The answer to one request. `id` is absent only when the request was
    /// unreadable. No `error` means success.
    Result {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<ErrorBody>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sessions: Option<Vec<Session>>,
    },
    /// Reset the emulator to `cols` x `rows`, then feed `data`.
    Screen {
        watch: u64,
        pane: String,
        cols: u16,
        rows: u16,
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
    },
    /// Feed `data` to the emulator of the current screen.
    Output {
        watch: u64,
        pane: String,
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
    },
    /// Sessions, windows or panes changed: send `list` again.
    SessionsChanged,
    /// The watched pane no longer exists.
    PaneGone {
        watch: u64,
        pane: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// New or changed entries of a chat. An entry replaces any earlier one with
    /// the same id; `reset` first clears the list. `status` is the agent's state
    /// now, absent when unknown. `seq` grows by one per message of a chat.
    ChatEntries {
        chat: u64,
        seq: u64,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        reset: bool,
        entries: Vec<ChatEntry>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<Agent>,
        /// On a reset: the adapter that reads this agent, e.g. `claude-code`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
        /// On a reset: earlier history exists but was not sent.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
        /// What the agent waits for now, with its choices; absent when none is known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<ChatPrompt>,
    },
    /// The chat stopped, e.g. the agent's transcript became unreadable.
    ChatEnded { chat: u64, reason: String },
    /// The documents for a `docs` request, sent just before its `result`.
    DocList {
        docs: u64,
        /// The repo's top directory name, when the pane is in a git repo.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root: Option<String>,
        entries: Vec<DocInfo>,
        /// More documents exist than were listed.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
    },
    /// The whole text of the open document, first and after every change.
    DocContent {
        doc: u64,
        path: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mtime: Option<u64>,
    },
    /// The open document is no longer readable, e.g. it was removed.
    DocEnded { doc: u64, reason: String },
    /// The changed files for a `changes` request, sent just before its `result`.
    ChangeList {
        changes: u64,
        /// The repo's top directory name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root: Option<String>,
        entries: Vec<ChangeInfo>,
        /// More files changed than were listed.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
    },
    /// The unified diff of the open file, first and after every change. A
    /// binary file has no text.
    DiffContent {
        diff: u64,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        binary: bool,
    },
    /// The open diff stopped, e.g. the file has no changes any more.
    DiffEnded { diff: u64, reason: String },
    /// The session a `new-task` made or found, sent just before its `result`.
    TaskCreated { task: u64, session: String },
    /// The bridge is exiting.
    Bye { reason: String },
}

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseMode {
    /// Remove the worktree when the branch is merged; otherwise refuse.
    Close,
    /// Keep the worktree.
    Keep,
    /// Remove the worktree even when the branch is not merged.
    Force,
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
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    BadRequest,
    InvalidPane,
    NoPane,
    UnsupportedKey,
    TooLarge,
    /// No agent adapter knows the program in this pane.
    Unsupported,
    /// The task launcher failed; the message says why.
    TaskFailed,
    Tmux,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// The repo's main checkout, absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub panes: Vec<Pane>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pane {
    pub id: String,
    pub window: String,
    pub index: u32,
    pub role: Role,
    pub cols: u16,
    pub rows: u16,
    pub command: String,
    pub path: String,
    pub dead: bool,
    /// The foreground process is a shell; an agent state is then likely stale.
    pub shell_foreground: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<Agent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Agent,
    Shell,
    Hidden,
    Unknown,
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
}

mod base64_bytes {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(data: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(data))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}

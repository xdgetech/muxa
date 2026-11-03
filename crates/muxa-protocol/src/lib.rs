//! The muxa client/bridge wire format: one JSON object per line. See
//! `protocol/PROTOCOL.md`; the shared fixtures in `protocol/fixtures` are the contract.

mod restart_recovery;
mod workers;
pub use restart_recovery::{RestartOperation, RestartRecoveryAction};
pub use workers::{WorkerAssignment, WorkerInfo, WorkerSelection};

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
    /// The grid the client's pane area shows at its default font (capability
    /// `viewport`, ADR 0020). The bridge sizes the watched pane's window for
    /// it, for this watch and later ones.
    Viewport {
        id: u64,
        cols: u32,
        rows: u32,
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
    /// `diff-content` for it carries this request's id as `diff`. `scope` says
    /// which list the path is from; absent means the uncommitted one.
    Diff {
        id: u64,
        pane: String,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<DiffScope>,
    },
    Undiff {
        id: u64,
    },
    /// The image files in this pane's git repo, ignored ones included. Needs the
    /// `images` capability. An `image-list` comes before the `result`.
    Images {
        id: u64,
        pane: String,
    },
    /// Open one of the images `images` listed, and follow its changes. Replaces
    /// the open image; `image-content` for it carries this request's id as `image`.
    Image {
        id: u64,
        pane: String,
        path: String,
    },
    Unimage {
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
    /// The task records the bridge keeps, and the workspaces a new one may use.
    /// Needs the `task-records` capability. A `task-list` comes before the `result`.
    ListTasks {
        id: u64,
    },
    /// Record a new task and start an agent for it. A `task-saved` comes before
    /// the `result`.
    CreateTask {
        id: u64,
        /// A workspace name from `task-list`.
        workspace: String,
        title: String,
        /// What the agent should do; the bridge hands it to the agent.
        goal: String,
        /// Free text that groups tasks.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
        /// An absolute path inside a repo; without one the task gets a directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repo: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Where a new branch starts; the repo's default branch when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<String>,
        /// Start a Task Loop with the agent (capability `task-loop`, ADR 0019).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r#loop: Option<LoopSpec>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker: Option<String>,
    },
    /// Set a task record's status. Fails with `conflict` when `rev` is not the
    /// record's current one. A `task-saved` comes before the `result`.
    UpdateTask {
        id: u64,
        /// The record's id, e.g. `t-7f3a2c`.
        task: String,
        /// The `rev` the client last saw.
        rev: u64,
        status: TaskStatus,
    },
    /// Restart the agent of a task in the same pane, with a handoff written
    /// first (ADR 0018). Needs the `task-handoff` capability. Refused while the
    /// agent is running, unless `force`. A `task-saved` comes before the `result`.
    RestartAgent {
        id: u64,
        /// The record's id, e.g. `t-7f3a2c`.
        task: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        force: bool,
        #[serde(default, skip_serializing_if = "WorkerSelection::is_keep")]
        worker: WorkerSelection,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rev: Option<u64>,
    },
    /// Resolve an interrupted restart without an automatic process or prompt retry.
    /// Requires `task-restart-recovery`; task-saved precedes result.
    RecoverRestart {
        id: u64,
        task: String,
        rev: u64,
        operation: String,
        action: RestartRecoveryAction,
    },
    /// Start or restart the Task Loop of a task (capability `task-loop`,
    /// ADR 0019). A `task-saved` comes before the `result`.
    StartLoop {
        id: u64,
        /// The record's id, e.g. `t-7f3a2c`.
        task: String,
        r#loop: LoopSpec,
        /// Count the attempts from zero again.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        reset: bool,
    },
    /// Stop the Task Loop of a task (capability `task-loop`). A `task-saved`
    /// comes before the `result`.
    StopLoop {
        id: u64,
        /// The record's id, e.g. `t-7f3a2c`.
        task: String,
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
            | Request::Viewport { id, .. }
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
            | Request::Images { id, .. }
            | Request::Image { id, .. }
            | Request::Unimage { id }
            | Request::CloseTask { id, .. }
            | Request::ListTasks { id }
            | Request::CreateTask { id, .. }
            | Request::UpdateTask { id, .. }
            | Request::RestartAgent { id, .. }
            | Request::RecoverRestart { id, .. }
            | Request::StartLoop { id, .. }
            | Request::StopLoop { id, .. }
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
        /// What the branch's commits changed since it left its base (capability
        /// `branch-changes`); absent on the base itself or without one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchChanges>,
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
    /// The images for an `images` request, newest first, sent just before its `result`.
    ImageList {
        images: u64,
        /// The repo's top directory name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root: Option<String>,
        entries: Vec<ImageInfo>,
        /// More images exist than were listed.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
    },
    /// The whole file of the open image, first and after every change.
    ImageContent {
        image: u64,
        path: String,
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mtime: Option<u64>,
    },
    /// The open image is no longer readable, e.g. it was removed.
    ImageEnded { image: u64, reason: String },
    /// The session a `new-task` made or found, sent just before its `result`.
    TaskCreated { task: u64, session: String },
    /// The task records for a `list-tasks` request, sent just before its `result`.
    TaskList {
        tasks: u64,
        /// The host's workspaces, from its config.
        workspaces: Vec<WorkspaceInfo>,
        entries: Vec<TaskRecord>,
        /// More records exist than were listed.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        workers: Vec<WorkerInfo>,
    },
    /// The record a task request wrote, sent just before its `result`.
    TaskSaved { task: u64, record: TaskRecord },
    /// A task record changed on the host: send `list-tasks` again (capability
    /// `tasks-changed`).
    TasksChanged,
    /// The bridge is exiting.
    Bye { reason: String },
}

/// A Task: a durable record of work, kept by the bridge on the host. It
/// outlives the tmux session that runs it (ADR 0015).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    /// e.g. `t-7f3a2c`.
    pub id: String,
    /// Grows by one on every write; `update-task` sends the one it saw.
    pub rev: u64,
    pub workspace: String,
    pub title: String,
    pub goal: String,
    pub status: TaskStatus,
    /// Epoch seconds.
    pub created_at: u64,
    /// Epoch seconds.
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The repo's main checkout, absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Where the agent works: the worktree or the task directory, absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// The last known session name, for display. Sessions link by `taskId`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// From the bridge, e.g. why the launch failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The agents that worked on the task, oldest first (ADR 0018). Recorded
    /// only while a bridge started or restarted them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<AgentRun>,
    /// The task's Task Loop, absent when it has none (ADR 0019).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#loop: Option<Box<TaskLoop>>,
    /// A start or restart prompt the host still waits to type (ADR 0019).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_prompt: Option<PendingPrompt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_operation: Option<Box<RestartOperation>>,
}

/// What a client asks for when it starts a Task Loop. The host fills in the
/// defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopSpec {
    /// A shell command, run in the task's `dir`; one of the workspace's `checks`.
    pub check: String,
    /// Failed checks before the loop stops: 1 to 10, default 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    /// Seconds one check may run: 10 to 3600, default 600.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// A task's Task Loop: the host runs `check` after each finished turn of the
/// agent, and types a fail prompt until it passes (ADR 0019).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskLoop {
    pub check: String,
    pub max_attempts: u32,
    /// Seconds.
    pub timeout: u64,
    pub state: LoopState,
    /// Failed checks so far.
    pub attempts: u32,
    /// Epoch seconds: when the loop started or last typed a prompt.
    pub armed_at: u64,
    /// Epoch seconds of the last state change.
    pub since: u64,
    /// Set when the state is `paused` or `stopped`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<LoopReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<LoopCheck>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoopState {
    /// Waits for the agent's next finished turn.
    Armed,
    Checking,
    /// The agent waits for the user; the loop goes on after the answer.
    Paused,
    Passed,
    Stopped,
}

/// Why a loop is `paused` or `stopped`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoopReason {
    /// The agent waits for the user (paused).
    Waiting,
    /// `maxAttempts` checks failed.
    Limit,
    /// A check ran longer than `timeout`.
    Timeout,
    /// The agent's state is `error`.
    Error,
    /// No linked session, no live agent pane, or a shell in the foreground.
    NoAgent,
    /// A prompt could not be typed within 30 seconds.
    NotSent,
    /// The agent did not take the typed prompt.
    Stalled,
    /// The user set the status to anything but `open`.
    Status,
    /// The user stopped it.
    User,
}

/// The last check a loop ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopCheck {
    pub attempt: u32,
    /// Absent when the check timed out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    /// How long it ran.
    pub seconds: u64,
    /// Epoch seconds when it ended.
    pub at: u64,
}

/// A prompt the host waits to type into the task's agent, kept in the record
/// so that the wait survives a restart of the host process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingPrompt {
    pub kind: PendingPromptKind,
    /// Epoch seconds when the wait began.
    pub since: u64,
    /// The `id` of the run the prompt is for.
    pub run: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PendingPromptKind {
    /// The start prompt of `create-task`.
    Start,
    /// The prompt of `restart-agent`.
    Restart,
}

/// One agent that worked on a task, as the bridge started or restarted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRun {
    /// From 1, one higher than the previous run's.
    pub id: u32,
    /// The adapter that saw the agent, e.g. `claude-code`; absent until one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// Epoch seconds.
    pub started_at: u64,
    /// Epoch seconds when the bridge restarted it; absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
    /// The agent's own session id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<String>,
    /// The session's transcript on the host, absolute. A path only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
    pub reason: RunReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment: Option<WorkerAssignment>,
    /// An interrupted restart may have replaced a process without confirmation.
    /// Absent on legacy/confirmed runs; unknown values have no known meaning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// Why the bridge started a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunReason {
    /// `create-task` started the agent.
    Start,
    /// `restart-agent` replaced the agent.
    Restart,
}

/// Set by the user; separate from the agent's state. A passing Task Loop moves
/// `open` to `review` (ADR 0019).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Open,
    /// The work waits for the user to review it.
    Review,
    Done,
    Dropped,
}

/// A workspace from the host's config; `create-task` names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub name: String,
    /// The check commands a Task Loop may run here; absent means no loops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<WorkspaceChecks>,
    /// The main checkouts the config lists for it, absolute. A client picks
    /// this workspace for a new task in one of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,
}

/// `"any"` or a list of exact commands, from the host's `[workspace.loop]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceChecks {
    /// Any command, at most 4096 bytes, with no control character.
    Any,
    /// Only these commands, exactly.
    List(Vec<String>),
}

impl Serialize for WorkspaceChecks {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            WorkspaceChecks::Any => s.serialize_str("any"),
            WorkspaceChecks::List(list) => list.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for WorkspaceChecks {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Word(String),
            List(Vec<String>),
        }
        match Raw::deserialize(d)? {
            Raw::Word(w) if w == "any" => Ok(WorkspaceChecks::Any),
            Raw::Word(w) => Err(serde::de::Error::custom(format!(
                "checks must be \"any\" or a list, not {w:?}"
            ))),
            Raw::List(list) => Ok(WorkspaceChecks::List(list)),
        }
    }
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

/// The files a branch's commits changed since it left its base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchChanges {
    /// The ref compared with, e.g. `origin/main`.
    pub base: String,
    /// Commits since the fork point.
    pub commits: u32,
    pub entries: Vec<ChangeInfo>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// Which list a `diff` path is from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffScope {
    /// Against HEAD: staged, unstaged and untracked.
    Uncommitted,
    /// The branch's commits since it left its base.
    Branch,
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
    /// The task record changed since the client read it; list it again.
    Conflict,
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
    /// tmux-agents' task name (`@agent_task`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// The task record this session runs (`@muxa_task`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
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

//! The muxa client/bridge wire format: one JSON object per line. See
//! `protocol/PROTOCOL.md`; the shared fixtures in `protocol/fixtures` are the contract.

mod assistant;
mod items;
mod known;
mod plans;
mod policies;
mod restart_recovery;
mod routines;
mod workers;
pub use assistant::{
    AssistResult, AssistScope, AssistSpec, AssistantRequest, Capture, CaptureSource, CaptureState,
    Delivery, Due, Finding, Notes, PossibleDuplicate, ResultSource, SortAction, WORK_ASSISTANT,
    WorkAction, WorkItem, WorkState,
};
pub use items::{Attachments, ChangeCount, InboxItem, ItemKind, ItemState, WORK_ITEMS};
pub use known::Known;
pub use plans::{
    AssistantInfo, Basis, BasisKind, ChangeKind, DelegateInfo, DelegateProposal, Minutes,
    OverallRequests, Plan, PlanChange, PlanRow, Share, SortProposal, WORK_PLANS,
};
pub use policies::{
    Confidence, Correction, CorrectionKind, Hypothesis, HypothesisProposal, HypothesisState,
    PlanAction, Policy, PolicyAction, WORK_POLICIES,
};
pub use restart_recovery::{RestartOperation, RestartRecoveryAction};
pub use routines::{
    Disposition, InboxAction, InboxEntry, InboxEntryKind, Proposal, RoutineFailure, RoutineInfo,
    RoutineProblem, RoutineRun, RoutineSuccess, RoutineTrigger, SourceLink, WORK_ASK,
    WORK_ASK_AGENT, WORK_CAPABILITY, WORK_CLOSE_ASK, WORK_END_REMOVES, WORK_READ, WORK_ROUTINES,
    work_capability, work_level,
};
pub use workers::{DEFAULT_WORKER, WorkerInfo};

use serde::{Deserialize, Serialize};

/// Changes when a message changes incompatibly.
pub const VERSION: u32 = 1;

/// Scrollback lines a `history` request sends when it names none, and at most.
pub const DEFAULT_HISTORY_LINES: u32 = 2000;
pub const MAX_HISTORY_LINES: u32 = 10_000;

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
    /// The pane's scrollback and screen, once (capability `history`). A
    /// `history` comes before the `result`.
    History {
        id: u64,
        pane: String,
        /// Scrollback lines above the screen, 1 to `MAX_HISTORY_LINES`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lines: Option<u32>,
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
    /// with uncommitted changes; the branch is kept. With `work` level 5, also
    /// an ask session; `mode` then changes nothing (ADR 0039).
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
        /// An absolute path inside a repo, or, with `work` level 2, a repo
        /// relative to the workspace root. Without one the task gets a directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        repo: Option<String>,
        /// With `work` level 2 it may be absent: the bridge derives one from
        /// the title (ADR 0031).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Where a new branch starts; the repo's default branch when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<String>,
        /// Start a Task Loop with the agent (capability `task-loop`, ADR 0019).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r#loop: Option<LoopSpec>,
        /// A worker name or `default`; absent is the workspace default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker: Option<String>,
        /// A reading Task: `repo` and no `branch`; the agent works in the
        /// Task's own folder and reads the repo (`work` level 4, ADR 0034).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        read: bool,
        /// The Work item the Task is for (`work` level 8, ADR 0046). The host
        /// links them, and a second request for the same item and `rev`
        /// returns the Task it already made.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work: Option<String>,
        /// With `work`: the item's `rev` the client saw.
        #[serde(default, rename = "workRev", skip_serializing_if = "Option::is_none")]
        work_rev: Option<u64>,
    },
    /// Turn a reading Task into a repo Task: a worktree, then a restart of
    /// the agent there (`work` level 4, ADR 0034 §3). A `task-saved` comes
    /// before the `result`.
    BranchTask {
        id: u64,
        task: String,
        /// Absent: the bridge names one after the title (ADR 0031).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
    /// Close a Task's session and keep its record, `TASK.md` and directory
    /// (`work` level 4, ADR 0034 §4). A `task-saved` comes before the `result`.
    ParkTask {
        id: u64,
        task: String,
    },
    /// Set a task record's status. Fails with `conflict` when `rev` is not
    /// the record's current one. A `task-saved` comes before the `result`.
    UpdateTask {
        id: u64,
        /// The record's id, e.g. `t-7f3a2c`.
        task: String,
        /// The `rev` the client last saw.
        rev: u64,
        #[serde(deserialize_with = "known::known")]
        status: TaskStatus,
        /// With `done` or `dropped`: what happens to the Task's worktree
        /// (`work` level 7, ADR 0043). Without it, the worktree stays.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worktree: Option<WorktreeEnd>,
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
        /// A worker name or `default`; absent keeps the current worker.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worker: Option<String>,
        /// When sent, a stale revision fails with `conflict`.
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
    /// The host's Routines (capability `work` level 1, ADR 0026). A
    /// `routine-list` comes before the `result`.
    ListRoutines {
        id: u64,
    },
    /// Start a run of an enabled Routine now. Needs the muxa daemon. A
    /// `routine-saved` comes before the `result`.
    RunRoutine {
        id: u64,
        workspace: String,
        /// The Routine's name in that workspace.
        routine: String,
    },
    /// The stored Inbox entries (capability `work` level 1). An
    /// `inbox-list` comes before the `result`.
    ListInbox {
        id: u64,
    },
    /// Approve, dismiss or defer a stored Inbox entry. An `inbox-saved`
    /// comes before the `result`.
    UpdateInbox {
        id: u64,
        /// e.g. `e-1a2b3c4d`, or an item's `i-` id.
        entry: String,
        #[serde(deserialize_with = "known::known")]
        action: InboxAction,
        /// With `later`: epoch seconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        until: Option<u64>,
        /// With `redo` and `change-course`: what to send the agent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        /// With `choose`: the index into the prompt's options.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        option: Option<u32>,
        /// With an answer that ends the Task, as in `update-task`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worktree: Option<WorktreeEnd>,
    },
    /// Start an agent session in a repo's main checkout, to ask about its
    /// code: no Task, no worktree (capability `work` level 2, ADR 0031). A
    /// `session-started` comes before the `result`.
    StartSession {
        id: u64,
        workspace: String,
        /// As `create-task`'s `repo`.
        repo: String,
    },
    /// Save the user's input (`work` level 8, ADR 0046). The same `clientId`
    /// returns the stored Capture and writes nothing. A `capture-saved` comes
    /// before the `result`.
    Capture {
        id: u64,
        #[serde(rename = "clientId")]
        client_id: String,
        text: String,
        #[serde(
            default,
            deserialize_with = "known::known_opt",
            skip_serializing_if = "Option::is_none"
        )]
        source: Option<CaptureSource>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
        /// Also make a Work item and ask the assistant; needs `workspace`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assist: Option<AssistSpec>,
    },
    /// Sort an unsorted Capture. A `capture-saved` comes before the `result`.
    SortCapture {
        id: u64,
        capture: String,
        rev: u64,
        #[serde(deserialize_with = "known::known")]
        action: SortAction,
        /// With `link`: the Work item.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work: Option<String>,
        /// With `work`: the new item's title; absent takes the text's first line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
    },
    /// Change a Work item. A stale `rev` is `conflict`. A `work-saved` comes
    /// before the `result`.
    UpdateWork {
        id: u64,
        work: String,
        rev: u64,
        action: WorkAction,
        /// With `edit`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// With `edit`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
        /// With `merge`: the Work item to merge into.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        into: Option<String>,
        /// With `edit`, at level 9: the user's deadline.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        due: Option<Due>,
    },
    /// Ask a workspace's agent to work on a Work item. Needs the muxa daemon.
    /// The same `work` and `rev` return the request already made. A
    /// `work-saved` comes before the `result`.
    RequestAssistant {
        id: u64,
        work: String,
        rev: u64,
        #[serde(deserialize_with = "known::known")]
        scope: AssistScope,
        /// Absent: the Work item's workspace.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        purpose: Option<String>,
    },
    /// Stop a request's `version`. A `work-saved` comes before the `result`.
    CancelRequest {
        id: u64,
        request: String,
        version: u64,
    },
    /// One Work item with all its Captures' texts and its requests. A
    /// `work-detail` comes before the `result`.
    GetWork {
        id: u64,
        work: String,
    },
    /// Write a result's or a plan's proposed notes to the live `NOTES.md`
    /// (`work` level 9, ADR 0047). A live file that no longer hashes to
    /// `base` is `conflict`. A `work-saved` or `plan-saved` comes before the
    /// `result`.
    ApplyNotes {
        id: u64,
        /// A request `q-…` or a plan `p-…`.
        source: String,
        /// The request's version, or the plan's `rev`.
        version: u64,
        /// The proposal's `base`.
        base: String,
    },
    /// Start an overall run now (`work` level 9). A `plan-saved` comes
    /// before the `result`.
    PlanNow {
        id: u64,
        /// The time the user has.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minutes: Option<u32>,
    },
    /// Keep a policy in the user's words (`work` level 10, ADR 0048). A
    /// `policy-saved` comes before the `result`.
    AddPolicy {
        id: u64,
        /// 1 to 500 bytes. Never logged.
        text: String,
        /// `all`, or one workspace's name.
        scope: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        until: Option<u64>,
    },
    /// Edit or delete a policy. A stale `rev` is `conflict`. `edit` sends a
    /// `policy-saved` before the `result`; `delete` only the `result`.
    UpdatePolicy {
        id: u64,
        /// The `y-` id.
        policy: String,
        rev: u64,
        action: PolicyAction,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        until: Option<u64>,
    },
    /// Reorder Today's rows, or remove one, on the newest `resulted` plan.
    /// A `plan-saved` comes before the `result`.
    UpdatePlan {
        id: u64,
        /// The `p-` id.
        plan: String,
        rev: u64,
        action: PlanAction,
        /// With `reorder`: every row's Work item or Inbox item, in the new order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        order: Vec<String>,
        /// With `dismiss`: the row's Work item or Inbox item.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        row: Option<String>,
        /// With `dismiss`: the user's reason, kept on the correction. Never logged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Remove a correction `x-…` or a hypothesis `h-…`, and the proposed
    /// hypotheses a forgotten correction is evidence of.
    Forget {
        id: u64,
        /// The record's id. Not `id`, which is the request's.
        record: String,
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
            | Request::History { id, .. }
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
            | Request::BranchTask { id, .. }
            | Request::ParkTask { id, .. }
            | Request::RecoverRestart { id, .. }
            | Request::StartLoop { id, .. }
            | Request::StopLoop { id, .. }
            | Request::ListRoutines { id }
            | Request::RunRoutine { id, .. }
            | Request::ListInbox { id }
            | Request::UpdateInbox { id, .. }
            | Request::StartSession { id, .. }
            | Request::Capture { id, .. }
            | Request::SortCapture { id, .. }
            | Request::UpdateWork { id, .. }
            | Request::RequestAssistant { id, .. }
            | Request::CancelRequest { id, .. }
            | Request::GetWork { id, .. }
            | Request::ApplyNotes { id, .. }
            | Request::PlanNow { id, .. }
            | Request::AddPolicy { id, .. }
            | Request::UpdatePolicy { id, .. }
            | Request::UpdatePlan { id, .. }
            | Request::Forget { id, .. }
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
    /// A pane's scrollback, then its screen: lines `cols` wide, each ending in
    /// a newline, with their colors as escape sequences.
    History {
        history: u64,
        pane: String,
        cols: u16,
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
        /// The pane has older lines than were sent.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
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
    /// The Routines for a `list-routines` request, sent just before its `result`.
    RoutineList {
        routines: u64,
        entries: Vec<RoutineInfo>,
        /// Workspaces whose routines file is broken (ADR 0036).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        problems: Vec<RoutineProblem>,
    },
    /// The Routine a `run-routine` started, sent just before its `result`.
    RoutineSaved { routine: u64, entry: RoutineInfo },
    /// The stored Inbox entries for a `list-inbox` request, newest first,
    /// sent just before its `result`.
    InboxList {
        inbox: u64,
        entries: Vec<InboxEntry>,
        /// Level 3: every item, Routine entries included, open ones first (ADR 0033).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        items: Vec<InboxItem>,
        /// Level 8: Work items, open ones first, then the ones moved down
        /// with Later, then at most 100 done or withdrawn, newest first.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        work: Vec<WorkItem>,
        /// Level 8: unsorted Captures, newest first.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        captures: Vec<Capture>,
        /// Level 8: the requests of the listed Work items.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        requests: Vec<AssistantRequest>,
        /// Level 9: the kept plans, newest first.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        plans: Vec<Plan>,
        /// Level 9: the overall assistant and the delegates.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assistant: Option<Box<AssistantInfo>>,
        /// Level 10: active policies, then expired ones.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        policies: Vec<Policy>,
        /// Level 10: `proposed` and `rejected` hypotheses, newest first.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        hypotheses: Vec<Hypothesis>,
        /// Level 10: at most 50 corrections, newest first.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        corrections: Vec<Correction>,
    },
    /// What an `update-inbox` wrote, sent just before its `result`: the
    /// Routine entry, the item, or both for a Routine's item.
    InboxSaved {
        inbox: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entry: Option<Box<InboxEntry>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item: Option<Box<InboxItem>>,
    },
    /// The session a `start-session` made, sent just before its `result`.
    SessionStarted {
        /// The request's id.
        start: u64,
        /// `$n`
        session: String,
        name: String,
    },
    /// What a `capture` or `sort-capture` wrote, sent just before its `result`.
    CaptureSaved {
        capture: u64,
        entry: Capture,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work: Option<Box<WorkItem>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<Box<AssistantRequest>>,
    },
    /// What an `update-work`, `request-assistant` or `cancel-request` wrote,
    /// sent just before its `result`.
    WorkSaved {
        work: u64,
        entry: WorkItem,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<Box<AssistantRequest>>,
    },
    /// The Work item a `get-work` asked for, sent just before its `result`.
    WorkDetail {
        work: u64,
        /// With its derived state, as `inbox-list` lists it.
        entry: WorkItem,
        /// The Captures `entry.captures` names, in that order, sorted or not.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        captures: Vec<Capture>,
        /// The item's requests, in `entry.requests` order.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        requests: Vec<AssistantRequest>,
    },
    /// What a `plan-now` or `apply-notes` wrote, sent just before its `result`.
    PlanSaved { plan: u64, entry: Box<Plan> },
    /// What an `add-policy`, an `update-policy edit` or a `confirm` wrote,
    /// sent just before its `result` (`work` level 10).
    PolicySaved { policy: u64, entry: Policy },
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
    /// A reading Task: it reads `repo` and works in its own folder (ADR 0034).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read: bool,
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
    /// The Work item the Task was started for (ADR 0046).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<String>,
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
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
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
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
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
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
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
    /// The worker requested for this run, or `default`. Absent on a legacy
    /// run: unrecorded, not the workspace default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
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
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
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
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

/// A workspace from the host's config; `create-task` names one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub name: String,
    /// The workspace's root folder, absolute (ADR 0031). A client shows a
    /// repo under it by its relative path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// The check commands a Task Loop may run here; absent means no loops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<WorkspaceChecks>,
    /// Its main checkouts, absolute and sorted: the repos under its root and
    /// those the config lists. A client picks this workspace for a new task
    /// in one of them.
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
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
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

/// What a Task's end does to its worktree (ADR 0043).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorktreeEnd {
    /// Remove the worktree and its branch when nothing can be lost;
    /// otherwise fail with `would-lose-work` and change nothing.
    Remove,
    /// Remove them anyway: the user said yes.
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
    /// Removing the Task's worktree could lose work; the message says why.
    /// Nothing changed. Ask the user, then send `worktree: force`.
    WouldLoseWork,
    Tmux,
    Internal,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
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
    /// The workspace of an ask session (`@muxa_ask`, ADR 0031, 0039).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
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
    /// Also a role a newer host added.
    #[serde(other)]
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
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
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

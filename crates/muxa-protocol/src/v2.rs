//! Protocol v2 (`protocol/PROTOCOL-v2.md`, ADR 0067): binary frames with JSON
//! control messages on channel 0. Pure encoding and decoding, no i/o.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use crate::settings::{Settings, SettingsOp};

pub const VERSION: u32 = 2;
/// The largest payload of one frame.
pub const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
/// Output credit each terminal channel starts with.
pub const INITIAL_CREDIT: u32 = 256 * 1024;
pub const HEADER: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Data,
    Credit,
    Close,
    Resize,
    Ack,
}

impl Kind {
    pub fn code(self) -> u8 {
        match self {
            Kind::Data => 0,
            Kind::Credit => 1,
            Kind::Close => 2,
            Kind::Resize => 3,
            Kind::Ack => 4,
        }
    }

    pub fn from_code(code: u8) -> Option<Kind> {
        Some(match code {
            0 => Kind::Data,
            1 => Kind::Credit,
            2 => Kind::Close,
            3 => Kind::Resize,
            4 => Kind::Ack,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub channel: u32,
    pub kind: Kind,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    TooLarge(usize),
    UnknownKind(u8),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::TooLarge(n) => write!(f, "frame payload of {n} bytes is over 16 MiB"),
            FrameError::UnknownKind(k) => write!(f, "unknown frame kind {k}"),
        }
    }
}

impl std::error::Error for FrameError {}

impl Frame {
    pub fn new(channel: u32, kind: Kind, payload: Vec<u8>) -> Frame {
        Frame {
            channel,
            kind,
            payload,
        }
    }

    /// A control message on channel 0.
    pub fn control<T: Serialize>(msg: &T) -> Result<Frame, serde_json::Error> {
        Ok(Frame::new(0, Kind::Data, serde_json::to_vec(msg)?))
    }

    /// Terminal output: `[offset u64 LE][bytes]`.
    pub fn output(channel: u32, offset: u64, bytes: &[u8]) -> Frame {
        Frame::new(channel, Kind::Data, with_u64(offset, bytes))
    }

    /// Terminal input: `[seq u64 LE][bytes]`.
    pub fn input(channel: u32, seq: u64, bytes: &[u8]) -> Frame {
        Frame::new(channel, Kind::Data, with_u64(seq, bytes))
    }

    pub fn ack(channel: u32, seq: u64) -> Frame {
        Frame::new(channel, Kind::Ack, seq.to_le_bytes().to_vec())
    }

    pub fn credit(channel: u32, bytes: u32) -> Frame {
        Frame::new(channel, Kind::Credit, bytes.to_le_bytes().to_vec())
    }

    pub fn resize(channel: u32, cols: u16, rows: u16) -> Frame {
        let mut p = cols.to_le_bytes().to_vec();
        p.extend_from_slice(&rows.to_le_bytes());
        Frame::new(channel, Kind::Resize, p)
    }

    pub fn encode(&self) -> Result<Vec<u8>, FrameError> {
        let len = self.payload.len();
        let len32 = u32::try_from(len)
            .ok()
            .filter(|_| len <= MAX_PAYLOAD)
            .ok_or(FrameError::TooLarge(len))?;
        let mut out = Vec::with_capacity(HEADER + len);
        out.extend_from_slice(&self.channel.to_le_bytes());
        out.push(self.kind.code());
        out.extend_from_slice(&len32.to_le_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    /// Takes one whole frame from the front of `buf`, if it holds one.
    pub fn decode(buf: &mut Vec<u8>) -> Result<Option<Frame>, FrameError> {
        if buf.len() < HEADER {
            return Ok(None);
        }
        let len = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]) as usize;
        if len > MAX_PAYLOAD {
            return Err(FrameError::TooLarge(len));
        }
        let kind = Kind::from_code(buf[4]).ok_or(FrameError::UnknownKind(buf[4]))?;
        if buf.len() < HEADER + len {
            return Ok(None);
        }
        let channel = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let payload = buf[HEADER..HEADER + len].to_vec();
        buf.drain(..HEADER + len);
        Ok(Some(Frame {
            channel,
            kind,
            payload,
        }))
    }

    /// The u64 prefix of an output, input or ack payload, and the rest.
    pub fn split_u64(&self) -> Option<(u64, &[u8])> {
        let (head, rest) = self.payload.split_first_chunk::<8>()?;
        Some((u64::from_le_bytes(*head), rest))
    }

    pub fn as_u32(&self) -> Option<u32> {
        self.payload
            .as_slice()
            .try_into()
            .ok()
            .map(u32::from_le_bytes)
    }

    pub fn as_size(&self) -> Option<(u16, u16)> {
        match self.payload.as_slice() {
            [c0, c1, r0, r1] => Some((
                u16::from_le_bytes([*c0, *c1]),
                u16::from_le_bytes([*r0, *r1]),
            )),
            _ => None,
        }
    }
}

fn with_u64(n: u64, bytes: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(8 + bytes.len());
    p.extend_from_slice(&n.to_le_bytes());
    p.extend_from_slice(bytes);
    p
}

/// What a program reported with OSC 7501.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
}

/// A terminal's facts. Never its screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terminal {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub live: bool,
    #[serde(default)]
    pub status: Option<Status>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
}

/// A place a client asks for (ADR 0068). A repo is absolute, or relative
/// to the Workspace's root as in ADR 0031.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PlaceSpec {
    /// A new folder in the Workspace's tasks folder.
    Folder,
    /// A worktree of `repo` on a new or existing `branch`.
    Branch {
        repo: String,
        branch: String,
        #[serde(default)]
        base: Option<String>,
    },
    /// The repo's main checkout, never written.
    Read { repo: String },
    /// A folder or worktree that already exists in the Workspace: nothing is
    /// made. It is the working place.
    Existing { path: String },
    /// The repo's own checkout as the working place, as it is (ADR 0077).
    /// It is the only place; one muxa agent works there at a time.
    Checkout { repo: String },
    /// Every repo of the Workspace, each as a `read` place, beside the
    /// Task's folder (ADR 0077).
    AllRead,
}

/// A Task's place as it is: the `muxa.places` label of its agent terminal
/// holds a JSON list of them, the working place first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    pub kind: PlaceKind,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaceKind {
    Folder,
    Branch,
    Read,
    /// A repo's own checkout; `branch` is the one it had when the Task started.
    Checkout,
}

/// How a program or stream ended; the CLOSE payload from the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exit {
    pub reason: String,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub signal: Option<i64>,
}

/// Client → server on channel 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientMsg {
    Hello {
        version: u32,
        client: String,
        #[serde(default)]
        resume: Option<Resume>,
    },
    Terminals {
        id: u64,
    },
    Attach {
        id: u64,
        terminal: String,
        cols: u16,
        rows: u16,
        #[serde(default)]
        history: u32,
    },
    Detach {
        id: u64,
        channel: u32,
    },
    /// Starts a Task's agent in its working place, or finds the one running
    /// there (ADR 0068).
    StartTask {
        id: u64,
        workspace: String,
        title: String,
        #[serde(default)]
        goal: Option<String>,
        /// Empty: one folder.
        #[serde(default)]
        places: Vec<PlaceSpec>,
        /// Deprecated and ignored: the agent gets all the Workspace's
        /// connections and MCP servers (ADR 0075). Still accepted.
        #[serde(default)]
        connections: Vec<String>,
        #[serde(default)]
        mcp: Vec<String>,
        /// Claude Code starts in plan mode (ADR 0060).
        #[serde(default)]
        plan: bool,
        /// A Task Loop on the agent from its start.
        #[serde(default, rename = "loop")]
        task_loop: Option<LoopSpec>,
        /// Talk to this Bot of the Workspace (ADR 0083): a folder Task whose
        /// agent starts from the Bot's Soul and Memory. A name with no Bot
        /// yet: the talk may propose adding it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bot: Option<String>,
    },
    /// Asks for `changed`, `removed`, `resync` and `inbox` notifications.
    Watch {
        id: u64,
    },
    Inbox {
        id: u64,
    },
    /// Reads a Capture line into fields, for the client to show as chips.
    /// `today` is the device's local date, `YYYY-MM-DD`.
    ParseCapture {
        id: u64,
        text: String,
        today: String,
    },
    InboxAdd {
        id: u64,
        kind: ItemKind,
        title: String,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        workspace: Option<String>,
        #[serde(default)]
        due: Option<String>,
    },
    /// Puts an open item at `index` in the manual order.
    InboxMove {
        id: u64,
        item: String,
        index: usize,
    },
    /// Hides an item until `until` (Unix seconds); it then returns on top.
    InboxLater {
        id: u64,
        item: String,
        until: i64,
    },
    InboxDone {
        id: u64,
        item: String,
    },
    InboxReopen {
        id: u64,
        item: String,
    },
    InboxDelete {
        id: u64,
        item: String,
    },
    Routines {
        id: u64,
    },
    /// Runs a Routine now, outside its schedule.
    RunRoutine {
        id: u64,
        workspace: String,
        name: String,
    },
    /// Starts a shell in the working place of the Task whose agent this is
    /// (ADR 0079).
    TaskShell {
        id: u64,
        terminal: String,
    },
    /// Closes the Task of this agent terminal as `inbox-approve` does, and
    /// marks its open items Done (ADR 0079).
    CloseTask {
        id: u64,
        terminal: String,
        /// Absent: `remove`.
        #[serde(default)]
        worktree: Option<WorktreeMode>,
    },
    /// A file from the user's device into the Task's working place, under
    /// `.muxa/uploads/` (ADR 0081). `data` is base64.
    Upload {
        id: u64,
        terminal: String,
        name: String,
        data: String,
    },
    /// Ends a Task's shell and removes its terminal. Only those.
    CloseTerminal {
        id: u64,
        terminal: String,
    },
    /// The Task of this agent terminal is ready for review: one Review item
    /// while it is open.
    TaskReview {
        id: u64,
        terminal: String,
        #[serde(default)]
        note: Option<String>,
    },
    /// The documents, changed files or images of the terminal's folder.
    ReaderList {
        id: u64,
        terminal: String,
        kind: ReaderKind,
        /// Documents: HTML files too (ADR 0065).
        #[serde(default)]
        html: bool,
    },
    /// Opens a listed file and follows it until `reader-close`.
    ReaderOpen {
        id: u64,
        terminal: String,
        kind: ReaderKind,
        path: String,
        #[serde(default)]
        html: bool,
        /// Changes: which diff. Absent: uncommitted.
        #[serde(default)]
        scope: Option<DiffScope>,
    },
    ReaderClose {
        id: u64,
        reader: u32,
    },
    /// The Task Loops of the live Task agents.
    Loops {
        id: u64,
    },
    /// Starts or restarts the Task Loop of an agent terminal.
    LoopStart {
        id: u64,
        terminal: String,
        #[serde(rename = "loop")]
        spec: LoopSpec,
        /// The failed checks count from 0 again.
        #[serde(default)]
        reset: bool,
    },
    LoopStop {
        id: u64,
        terminal: String,
    },
    /// Follows the conversation of a Task's agent until `chat-close`.
    ChatOpen {
        id: u64,
        terminal: String,
    },
    ChatClose {
        id: u64,
        chat: u32,
    },
    /// The Workspaces of `workspaces.toml`, for starting a Task. Nothing secret.
    Workspaces {
        id: u64,
    },
    /// Each subscription and the plan limits its agents last reported (ADR 0091).
    Subscriptions {
        id: u64,
    },
    /// Approves a Review item: closes its Task (ADR 0068 §7, ADR 0043).
    InboxApprove {
        id: u64,
        item: String,
        /// Absent: `remove`.
        #[serde(default)]
        worktree: Option<WorktreeMode>,
    },
    /// Types the user's comment into the Task's agent, and resolves the
    /// Review item.
    InboxRequestChanges {
        id: u64,
        item: String,
        comment: String,
    },
    /// Runs again what made a Failed item, and resolves it.
    InboxRetry {
        id: u64,
        item: String,
    },
    /// Applies an item's settings proposal with its revision, and marks the
    /// item Done (ADR 0083).
    InboxApply {
        id: u64,
        item: String,
    },
    /// Accepts or dismisses an item's suggestion (ADR 0084).
    InboxSuggestion {
        id: u64,
        item: String,
        accept: bool,
    },
    /// The host's editable settings (ADR 0074).
    Settings {
        id: u64,
    },
    /// Applies `ops` when `revision` is still the current one.
    SettingsUpdate {
        id: u64,
        revision: String,
        ops: Vec<SettingsOp>,
    },
    /// Adds one Inbox item per open v1 Task whose place is still there.
    MigrateV1 {
        id: u64,
        #[serde(default)]
        dry_run: bool,
    },
}

/// What closing a Task does with a branch worktree that holds work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorktreeMode {
    /// Only a clean and merged worktree goes; else `would-lose-work`.
    #[default]
    Remove,
    /// It goes anyway, its branch with `git branch -D`.
    Force,
}

/// A subscription, a named agent login, and the plan limits its agents last
/// reported (ADR 0091). No credential and no account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionUsage {
    pub name: String,
    /// `claude-code` or `codex`.
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// Shortest first; empty before its first report.
    #[serde(default)]
    pub windows: Vec<UsageWindow>,
    /// Unix seconds of the last report; None before the first.
    #[serde(default)]
    pub as_of: Option<u64>,
}

/// One plan window: 300 minutes, 10080 minutes (a week).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageWindow {
    pub minutes: u32,
    /// 0 to 100, rounded.
    pub used_percent: u32,
    /// Unix seconds. A time in the past: the window reset, with no report since.
    pub resets_at: u64,
}

/// A Workspace as a client may see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub root: String,
    /// The repos as `start-task` takes them: relative to the root, or a
    /// listed repo's folder name.
    pub repos: Vec<String>,
    /// The agent's program name, e.g. `claude`; never its arguments or
    /// environment.
    #[serde(default)]
    pub agent: Option<String>,
    /// `null` when the Workspace runs no Task Loops.
    #[serde(rename = "loop")]
    pub task_loop: Option<WorkspaceLoop>,
    pub connections: Vec<WorkspaceConnection>,
    /// The `[[workspace.mcp]]` servers every agent gets (ADR 0069, 0075).
    #[serde(default)]
    pub mcp: Vec<WorkspaceMcp>,
    /// Each repo's checked-out branch, by its name in `repos`. A repo on a
    /// detached HEAD has none.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub branches: std::collections::BTreeMap<String, String>,
}

/// An MCP server's name only; its command, URL and variables stay on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMcp {
    pub name: String,
    /// Always true since ADR 0075: every agent of the Workspace gets it.
    pub default: bool,
    #[serde(default)]
    pub used_by: UsedBy,
    /// Unix seconds when an agent last started with it; `null`: never.
    #[serde(default)]
    pub last_used: Option<u64>,
}

/// Who gets a connection or MCP server now (ADR 0069 §7, ADR 0075): every
/// live Task agent and every Routine of the Workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedBy {
    /// The names of the Routines in `routines.toml` that get it.
    #[serde(default)]
    pub routines: Vec<String>,
    /// The ids of the live Task agent terminals that got it.
    #[serde(default)]
    pub tasks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceLoop {
    pub checks: LoopChecks,
}

/// The checks a Task Loop may run: `"any"` or a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopChecks {
    Any,
    List(Vec<String>),
}

impl Serialize for LoopChecks {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            LoopChecks::Any => s.serialize_str("any"),
            LoopChecks::List(list) => list.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for LoopChecks {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Word(String),
            List(Vec<String>),
        }
        match Raw::deserialize(d)? {
            Raw::Word(w) if w == "any" => Ok(LoopChecks::Any),
            Raw::Word(w) => Err(serde::de::Error::custom(format!(
                "checks is \"any\" or a list, not {w:?}"
            ))),
            Raw::List(list) => Ok(LoopChecks::List(list)),
        }
    }
}

/// A connection's name only: its secret never crosses the wire (ADR 0069).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceConnection {
    pub name: String,
    /// Always true since ADR 0075: every agent of the Workspace gets it.
    pub default: bool,
    #[serde(default)]
    pub used_by: UsedBy,
    /// Unix seconds when an agent last started with it; `null`: never.
    #[serde(default)]
    pub last_used: Option<u64>,
}

/// A Routine's id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineRef {
    pub workspace: String,
    pub name: String,
}

/// What changed in a followed conversation (plan F4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatUpdate {
    /// `entries` replace everything the client has; else each one replaces
    /// the entry with its id, or is added.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reset: bool,
    pub entries: Vec<crate::ChatEntry>,
    /// Older entries exist than the reset carries.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// What the agent waits on now; absent: nothing known. Every update
    /// carries the current one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<ChatPrompt>,
}

/// A choice the agent waits on, with the bytes that pick each option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatPrompt {
    /// The tool entry the agent waits on.
    pub entry: String,
    pub kind: crate::PromptKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    pub options: Vec<ChatOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatOption {
    pub label: String,
    /// What to send as the terminal's input to pick this option.
    pub input: String,
    /// Declines or cancels, e.g. a deny.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cancel: bool,
}

/// A Task Loop a client asks for (ADR 0019): after each finished turn the
/// host runs `check` in the working place.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopSpec {
    /// Absent: the Workspace's only allowed check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    /// 1 to 10, default 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    /// Seconds one check may run: 10 to 3600, default 600.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// Where a Task Loop stands. Times are Unix seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopInfo {
    pub terminal: String,
    pub check: String,
    pub max_attempts: u32,
    pub timeout: u64,
    /// `armed`, `checking`, `paused`, `passed` or `stopped`.
    pub state: String,
    /// Failed checks so far.
    pub attempts: u32,
    pub armed_at: u64,
    /// When `state` last changed.
    pub since: u64,
    /// Why it is paused or stopped: `waiting`, `limit`, `error`, `no-agent`, `user`…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<LastCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastCheck {
    pub attempt: u32,
    /// Absent when the check timed out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    pub seconds: u64,
    pub at: u64,
}

/// What the Reader lists and opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReaderKind {
    Docs,
    Changes,
    Images,
}

/// Which diff of a changed file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiffScope {
    /// Against HEAD: staged, unstaged and untracked.
    Uncommitted,
    /// The branch's commits since it left its base.
    Branch,
}

/// A file the Reader may open. Which fields are set depends on the kind.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderEntry {
    /// Relative to the folder's repo, or to the folder outside a repo.
    pub path: String,
    /// Documents: the first heading, or an HTML `<title>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Unix seconds of the last change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<u64>,
    /// Images: bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<crate::ChangeStatus>,
    /// Changes: the path before a rename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    /// Changes: lines added and removed; absent for a binary file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderList {
    /// The repo's folder name; absent outside a repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    pub entries: Vec<ReaderEntry>,
    /// More files exist than were listed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
    /// Changes: what the branch's commits changed since it left its base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<ReaderBranch>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderBranch {
    /// The ref compared with, e.g. `origin/main`.
    pub base: String,
    pub commits: u32,
    pub entries: Vec<ReaderEntry>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// An open file as it is now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReaderContent {
    pub path: String,
    /// Documents and diffs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Images: the bytes in base64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    /// A diff of a binary file: no text.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime: Option<u64>,
}

/// What an Inbox item asks of the user (ADR 0066).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Review,
    Reply,
    Decide,
    Read,
    Todo,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxItem {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    /// Hidden until then (Unix seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub later_until: Option<i64>,
    /// Unix seconds.
    pub created: i64,
    /// `capture`, `task`, or `routine`.
    pub source: String,
    /// The Task agent terminal that made the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<String>,
    /// The Routine that made the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine: Option<RoutineRef>,
    /// Where an older Task worked: Resume Task adopts it (`existing`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<ItemPlace>,
    /// Pages the item is about, e.g. a PR or a Slack thread (ADR 0082).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<ItemLink>,
    /// What a Decide item proposes. Start Task takes it as the goal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<ItemProposal>,
    /// The Bot whose run made the item, in `workspace`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot: Option<String>,
    /// Settings a Bot proposes in a talk (ADR 0083): `inbox-apply` applies
    /// them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_proposal: Option<SettingsProposal>,
    /// What the Capture assistant suggests (ADR 0084).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<ItemSuggestion>,
}

/// Settings changes a Bot proposed, against the settings at `revision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsProposal {
    pub revision: String,
    pub ops: Vec<SettingsOp>,
}

/// The Capture assistant's suggestion for an item. Each part is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemSuggestion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ItemKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Titles of the items to make instead of this one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub split: Vec<String>,
    /// A goal for Start Task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_goal: Option<String>,
    /// A Bot of the suggested (or the item's) Workspace to ask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot: Option<String>,
}

/// A page an item is about. `http` or `https` only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemLink {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// A change a Routine proposes; muxa never performs it by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemProposal {
    pub action: String,
    pub target: String,
    /// The exact text to write.
    pub content: String,
}

/// How a Bot's items ended (ADR 0082): the apps show it beside the Bot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotCount {
    pub workspace: String,
    pub bot: String,
    pub done: u32,
    pub deleted: u32,
    /// The host's local date of `added`, `YYYY-MM-DD`; empty before its first item.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub day: String,
    /// Items that counted against `daily_items` on `day`. Its "N more" item and
    /// its talks' proposals do not count.
    #[serde(default)]
    pub added: u32,
}

/// An existing working place in a Workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemPlace {
    pub workspace: String,
    pub path: String,
}

/// A v1 Task record as `migrate-v1` found it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V1Task {
    pub id: String,
    pub workspace: String,
    pub title: String,
    pub path: String,
    /// The place is still there.
    pub exists: bool,
    pub status: V1Status,
}

/// What `migrate-v1` does with a v1 Task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum V1Status {
    /// A dry run: it would be added.
    New,
    Added,
    /// An earlier run added it.
    InInbox,
    /// Its place is gone.
    Missing,
    /// Its Workspace is gone.
    NoWorkspace,
    /// Its place is no longer in its Workspace.
    Outside,
}

/// The whole Inbox: open items in the user's order, then recent done ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inbox {
    pub revision: u64,
    pub items: Vec<InboxItem>,
    pub done: Vec<InboxItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bots: Vec<BotCount>,
}

/// A Routine as the host defines it, with its last run. `(workspace, name)`
/// is its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Routine {
    pub workspace: String,
    pub name: String,
    /// Nothing runs, also not `run-routine`, while this is false.
    pub enabled: bool,
    /// `HH:MM` in `timezone`; absent for a Routine that runs only on request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    /// The five cron fields; absent means every day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    /// Unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_due: Option<i64>,
    /// Unix seconds; a run is going on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running_since: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<RoutineLast>,
    /// The Bot that owns it (ADR 0082).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot: Option<String>,
    /// The event that runs it, e.g. `review` (ADR 0084).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
}

/// How the last run ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineLast {
    /// Unix seconds.
    pub at: i64,
    pub ok: bool,
    /// Why it failed: one sentence with ids only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Items the run published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<u32>,
}

/// A Workspace whose `.muxa/routines.toml` has Routines while
/// `[workspace.routines]` turns them off: none of them run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutinesOff {
    pub workspace: String,
    pub count: u32,
}

/// A Workspace whose Routines could not be read; it lists none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineProblem {
    pub workspace: String,
    pub reason: String,
}

/// A Capture line read into fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    pub kind: ItemKind,
    pub title: String,
    pub workspace: Option<String>,
    pub due: Option<String>,
}

/// `hello.resume`: the session to continue and where each channel stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resume {
    pub token: String,
    #[serde(default)]
    pub channels: Vec<ResumeChannel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeChannel {
    pub channel: u32,
    pub terminal: String,
    /// The next output offset the client expects.
    pub offset: u64,
    /// The last input sequence number the client got an ACK for.
    #[serde(default)]
    pub input_seq: u64,
    pub cols: u16,
    pub rows: u16,
    #[serde(default)]
    pub history: u32,
}

/// A channel that continues after a resume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resumed {
    pub channel: u32,
    /// The last input the server applied; `None` when it cannot tell.
    pub input_seq: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    BadRequest,
    Version,
    NotFound,
    Backend,
    Closed,
    /// The thing is busy: a Routine already runs.
    Conflict,
    /// Closing a Task would lose uncommitted or unmerged work; nothing changed.
    WouldLoseWork,
    /// The host locked its settings (`--settings-read-only`).
    ReadOnly,
}

/// Server → client on channel 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ServerMsg {
    Welcome {
        version: u32,
        server: String,
        backend: String,
        token: String,
        resumed: Vec<Resumed>,
    },
    Reply {
        id: u64,
        #[serde(flatten)]
        body: Reply,
    },
    Error {
        #[serde(default)]
        id: Option<u64>,
        code: ErrorCode,
        message: String,
    },
    Paint {
        channel: u32,
    },
    /// A terminal started, or its state, size or liveness changed.
    Changed {
        terminal: Terminal,
    },
    Removed {
        terminal: String,
    },
    /// Changes were lost: the client lists the terminals again.
    Resync {},
    /// After `watch`: the Inbox changed.
    Inbox {
        inbox: Inbox,
    },
    /// An open Reader file changed.
    Reader {
        reader: u32,
        content: ReaderContent,
    },
    /// An open Reader file is gone, or cannot be read any more.
    ReaderEnded {
        reader: u32,
        reason: String,
    },
    /// After `watch`: a Task Loop changed. It carries every loop.
    Loops {
        loops: Vec<LoopInfo>,
    },
    /// A followed conversation changed.
    Chat {
        chat: u32,
        update: ChatUpdate,
    },
    /// A followed conversation ended, e.g. the agent exited.
    ChatEnded {
        chat: u32,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Reply {
    Terminals {
        terminals: Vec<Terminal>,
    },
    Attached {
        channel: u32,
    },
    Started {
        terminal: Terminal,
        existing: bool,
    },
    Inbox {
        inbox: Inbox,
    },
    Capture {
        capture: Capture,
    },
    Routines {
        routines: Vec<Routine>,
        problems: Vec<RoutineProblem>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        off: Vec<RoutinesOff>,
    },
    Listed {
        list: ReaderList,
    },
    Opened {
        reader: u32,
        content: ReaderContent,
    },
    Loops {
        loops: Vec<LoopInfo>,
    },
    Loop {
        #[serde(rename = "loop")]
        info: LoopInfo,
    },
    Chat {
        chat: u32,
        update: ChatUpdate,
    },
    Workspaces {
        workspaces: Vec<Workspace>,
    },
    Subscriptions {
        subscriptions: Vec<SubscriptionUsage>,
    },
    Settings {
        settings: Settings,
    },
    Migrated {
        tasks: Vec<V1Task>,
        added: u32,
    },
    /// Where an upload went, relative to the Task's working place.
    Uploaded {
        path: String,
    },
    Empty {},
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frames_decode_only_when_whole() {
        let a = Frame::output(3, 42, b"hi").encode().unwrap();
        let b = Frame::ack(3, 7).encode().unwrap();
        let mut buf = a[..5].to_vec();
        assert_eq!(Frame::decode(&mut buf), Ok(None));
        buf.extend_from_slice(&a[5..]);
        buf.extend_from_slice(&b);
        let out = Frame::decode(&mut buf).unwrap().unwrap();
        assert_eq!(out.split_u64(), Some((42, &b"hi"[..])));
        let ack = Frame::decode(&mut buf).unwrap().unwrap();
        assert_eq!((ack.kind, ack.split_u64()), (Kind::Ack, Some((7, &[][..]))));
        assert!(buf.is_empty());
    }

    #[test]
    fn bad_frames_are_errors() {
        let mut big = vec![0, 0, 0, 0, 0];
        big.extend_from_slice(&((MAX_PAYLOAD as u32) + 1).to_le_bytes());
        assert!(matches!(
            Frame::decode(&mut big),
            Err(FrameError::TooLarge(_))
        ));
        let mut unknown = vec![0, 0, 0, 0, 9, 0, 0, 0, 0];
        assert_eq!(Frame::decode(&mut unknown), Err(FrameError::UnknownKind(9)));
    }

    #[test]
    fn control_messages_match_the_spec() {
        let hello: ClientMsg = serde_json::from_value(
            json!({"type": "hello", "version": 2, "client": "muxa-cli", "resume": null}),
        )
        .unwrap();
        assert!(matches!(hello, ClientMsg::Hello { version: 2, .. }));
        let resume: ClientMsg = serde_json::from_value(json!({"type": "hello", "version": 2,
            "client": "c", "resume": {"token": "ab", "channels": [{"channel": 3,
            "terminal": "t7", "offset": 10, "input_seq": 2, "cols": 80, "rows": 24}]}}))
        .unwrap();
        let ClientMsg::Hello {
            resume: Some(resume),
            ..
        } = resume
        else {
            panic!("no resume");
        };
        assert_eq!(
            (resume.channels[0].offset, resume.channels[0].history),
            (10, 0)
        );
        assert_eq!(
            serde_json::to_value(Resumed {
                channel: 3,
                input_seq: None
            })
            .unwrap(),
            json!({"channel": 3, "input_seq": null})
        );
        let attach: ClientMsg = serde_json::from_value(
            json!({"type": "attach", "id": 4, "terminal": "t7", "cols": 80, "rows": 24}),
        )
        .unwrap();
        assert_eq!(
            attach,
            ClientMsg::Attach {
                id: 4,
                terminal: "t7".into(),
                cols: 80,
                rows: 24,
                history: 0
            }
        );
        let reply = ServerMsg::Reply {
            id: 4,
            body: Reply::Attached { channel: 1 },
        };
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            json!({"type": "reply", "id": 4, "channel": 1})
        );
        let open: ClientMsg = serde_json::from_value(json!({"type": "reader-open", "id": 5,
            "terminal": "t7", "kind": "changes", "path": "a.rs", "scope": "branch"}))
        .unwrap();
        assert!(matches!(
            open,
            ClientMsg::ReaderOpen {
                kind: ReaderKind::Changes,
                scope: Some(DiffScope::Branch),
                html: false,
                ..
            }
        ));
        let opened = ServerMsg::Reply {
            id: 5,
            body: Reply::Opened {
                reader: 2,
                content: ReaderContent {
                    path: "a.rs".into(),
                    text: Some("diff".into()),
                    ..ReaderContent::default()
                },
            },
        };
        let wire = serde_json::to_value(&opened).unwrap();
        assert_eq!(
            wire,
            json!({"type": "reply", "id": 5, "reader": 2, "content": {"path": "a.rs", "text": "diff"}})
        );
        assert_eq!(serde_json::from_value::<ServerMsg>(wire).unwrap(), opened);
        let listed = ServerMsg::Reply {
            id: 6,
            body: Reply::Listed {
                list: ReaderList {
                    entries: vec![ReaderEntry {
                        path: "a.rs".into(),
                        status: Some(crate::ChangeStatus::Renamed),
                        old_path: Some("b.rs".into()),
                        ..ReaderEntry::default()
                    }],
                    ..ReaderList::default()
                },
            },
        };
        let wire = serde_json::to_value(&listed).unwrap();
        assert_eq!(
            wire["list"],
            json!({"entries": [{"path": "a.rs", "status": "renamed", "old_path": "b.rs"}]})
        );
        assert_eq!(serde_json::from_value::<ServerMsg>(wire).unwrap(), listed);
        let err = ServerMsg::Error {
            id: Some(4),
            code: ErrorCode::NotFound,
            message: "t7".into(),
        };
        assert_eq!(
            serde_json::to_value(&err).unwrap()["code"],
            json!("not-found")
        );
    }

    #[test]
    fn task_actions_and_workspaces_match_the_spec() {
        let approve: ClientMsg = serde_json::from_value(
            json!({"type": "inbox-approve", "id": 1, "item": "i3", "worktree": "force"}),
        )
        .unwrap();
        assert!(matches!(
            approve,
            ClientMsg::InboxApprove {
                worktree: Some(WorktreeMode::Force),
                ..
            }
        ));
        let plain: ClientMsg =
            serde_json::from_value(json!({"type": "inbox-approve", "id": 1, "item": "i3"}))
                .unwrap();
        assert!(matches!(
            plain,
            ClientMsg::InboxApprove { worktree: None, .. }
        ));
        let ws = Workspace {
            name: "app".into(),
            root: "/src".into(),
            repos: vec!["app".into()],
            agent: Some("claude".into()),
            task_loop: Some(WorkspaceLoop {
                checks: LoopChecks::Any,
            }),
            connections: vec![WorkspaceConnection {
                name: "github".into(),
                default: true,
                used_by: UsedBy {
                    routines: vec!["triage".into()],
                    tasks: vec!["t7".into()],
                },
                last_used: Some(1_700_000_000),
            }],
            mcp: vec![WorkspaceMcp {
                name: "docs".into(),
                default: false,
                used_by: UsedBy::default(),
                last_used: None,
            }],
            branches: Default::default(),
        };
        let wire = serde_json::to_value(&ws).unwrap();
        assert_eq!(
            wire["mcp"],
            json!([{"name": "docs", "default": false,
                "used_by": {"routines": [], "tasks": []}, "last_used": null}])
        );
        assert_eq!(
            wire["connections"][0],
            json!({"name": "github", "default": true,
                "used_by": {"routines": ["triage"], "tasks": ["t7"]}, "last_used": 1_700_000_000})
        );
        // A reply from a daemon before ADR 0069 §7 still reads.
        let old: WorkspaceMcp =
            serde_json::from_value(json!({"name": "docs", "default": true})).unwrap();
        assert_eq!((old.used_by, old.last_used), (UsedBy::default(), None));
        assert_eq!(wire["loop"], json!({"checks": "any"}));
        assert_eq!(serde_json::from_value::<Workspace>(wire).unwrap(), ws);
        let none = Workspace {
            task_loop: None,
            ..ws.clone()
        };
        assert_eq!(serde_json::to_value(&none).unwrap()["loop"], json!(null));
        let list: WorkspaceLoop = serde_json::from_value(json!({"checks": ["make test"]})).unwrap();
        assert_eq!(list.checks, LoopChecks::List(vec!["make test".into()]));
        assert!(serde_json::from_value::<WorkspaceLoop>(json!({"checks": "all"})).is_err());
        assert_eq!(
            serde_json::to_value(ErrorCode::WouldLoseWork).unwrap(),
            json!("would-lose-work")
        );
        // An item stored before items linked what made them still loads.
        let old: InboxItem = serde_json::from_value(json!({"id": "i1", "kind": "failed",
            "title": "x", "created": 1, "source": "routine"}))
        .unwrap();
        assert_eq!((old.terminal, old.routine, old.place), (None, None, None));
    }

    #[test]
    fn adopting_and_migrating_match_the_spec() {
        let place: PlaceSpec =
            serde_json::from_value(json!({"kind": "existing", "path": "/w/tasks/a-t-1"})).unwrap();
        assert_eq!(
            place,
            PlaceSpec::Existing {
                path: "/w/tasks/a-t-1".into()
            }
        );
        let migrate: ClientMsg =
            serde_json::from_value(json!({"type": "migrate-v1", "id": 4})).unwrap();
        assert_eq!(
            migrate,
            ClientMsg::MigrateV1 {
                id: 4,
                dry_run: false
            }
        );
        let item: InboxItem = serde_json::from_value(json!({"id": "i2", "kind": "todo",
            "title": "Resume: Fix", "created": 1, "source": "v1",
            "place": {"workspace": "w", "path": "/w/x"}}))
        .unwrap();
        assert_eq!(item.place.as_ref().map(|p| p.path.as_str()), Some("/w/x"));
        let reply = Reply::Migrated {
            tasks: vec![V1Task {
                id: "t-000001".into(),
                workspace: "w".into(),
                title: "Fix".into(),
                path: "/w/x".into(),
                exists: true,
                status: V1Status::InInbox,
            }],
            added: 0,
        };
        let wire = serde_json::to_value(&reply).unwrap();
        assert_eq!(
            wire,
            json!({"tasks": [{"id": "t-000001", "workspace": "w", "title": "Fix",
                "path": "/w/x", "exists": true, "status": "in-inbox"}], "added": 0})
        );
        assert_eq!(serde_json::from_value::<Reply>(wire).unwrap(), reply);
    }
}

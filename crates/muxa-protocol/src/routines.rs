//! Routines and the stored Inbox entries they publish (ADR 0026), as the
//! daemon keeps them in each Routine's state file.
use serde::{Deserialize, Serialize};

/// A Routine as the host's `workspaces.toml` defines it, with its schedule
/// state. `(workspace, name)` is its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineInfo {
    pub workspace: String,
    pub name: String,
    /// Absent: the Routine runs only on `run-routine`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<RoutineTrigger>,
    /// Nothing runs, also not `run-routine`, while this is false.
    pub enabled: bool,
    /// A worker name; absent is the workspace default agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    /// The label stamped on Tasks the Routine's proposals create.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Epoch seconds of the next scheduled run; absent without a trigger or while disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_due: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success: Option<RoutineSuccess>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<RoutineFailure>,
    /// The open run. At most one runs at a time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<RoutineRun>,
}

/// A local wall-clock time on the days of a schedule, in an IANA time zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineTrigger {
    /// `HH:MM`, 24-hour.
    pub at: String,
    /// e.g. `Europe/Berlin`.
    pub timezone: String,
    /// The five cron fields (ADR 0036). Absent from an older host: every day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
}

/// A workspace whose `.muxa/routines.toml` could not be used. It lists no Routines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineProblem {
    pub workspace: String,
    /// Names the file and the Routine. A TOML parse error may quote the file.
    pub reason: String,
}

/// A Routine run that has not ended. Its record has kind `routine-run`; task
/// lists skip it. Its session carries `taskId` = `task`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRun {
    /// e.g. `2026-11-03T09:00` for a scheduled run, `now-1762160400` for run-now.
    pub occurrence: String,
    /// Epoch seconds.
    pub started_at: u64,
    /// The run's record id; absent for a moment while the run starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineSuccess {
    /// Epoch seconds of the publish.
    pub at: u64,
    pub occurrence: String,
    pub task: String,
    /// Entries stored by the publish; a repeated key is not counted.
    pub published: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineFailure {
    pub at: u64,
    pub occurrence: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// One sentence, with ids only: never output, instructions or source text.
    pub reason: String,
}

/// A stored Inbox entry: Routine output waiting for the user (ADR 0026).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxEntry {
    /// e.g. `e-1a2b3c4d`.
    pub id: String,
    pub workspace: String,
    pub routine: String,
    /// Chosen by the agent; a key the Routine already stored is skipped.
    pub key: String,
    pub kind: InboxEntryKind,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<SourceLink>,
    /// Epoch seconds.
    pub created_at: u64,
    /// The `routine-run` record that published it.
    pub run: String,
    pub occurrence: String,
    pub disposition: Disposition,
    /// Epoch seconds when a device first opened it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<u64>,
    /// Set by `later`: the entry is listed below the others (ADR 0040).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub later_at: Option<u64>,
    /// Only on an entry stored before ADR 0040: when a hidden entry was open again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
    /// Set by `protect`: `delete-inbox` keeps the entry (ADR 0046).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_at: Option<u64>,
    /// Only for kind `proposal`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<Proposal>,
    /// After `approve`: the Task that performs the proposal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// `YYYY-MM-DD`, for a Reply or To do item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    /// The Bot that owns the Routine (ADR 0082).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InboxEntryKind {
    Briefing,
    Proposal,
    /// Something to answer (ADR 0082).
    Reply,
    Todo,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    Open,
    Later,
    Approved,
    Dismissed,
    /// A value a newer host added; shown as unknown.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLink {
    /// `https` or `http`.
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// The action an approval starts. The Task gets it unchanged, with `hash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub action: String,
    pub target: String,
    pub content: String,
    /// `sha256:<hex>` of action, target and content.
    pub hash: String,
}
